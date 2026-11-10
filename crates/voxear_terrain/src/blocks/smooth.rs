//! A world of blocks drawn smooth: its ground's surface where it passes
//! between its blocks' middles, from how full each block is (see
//! `chunk::Fill`), in place of the blocks' cubes.
//!
//! Eight blocks' middles are the corners of a smaller cube between them.
//! Where the surface passes through such a cube, the cube has one point:
//! where the surface crosses its edges, on average. Wherever a block of
//! ground meets air, the surface there is a four-sided face between the
//! points of the four cubes round the line from one block's middle to the
//! other's (surface nets). A point is worked out alike in every chunk that
//! has it, from the same blocks, so chunks meet without gaps; a face
//! belongs to the chunk its block of ground is in.
//!
//! Where the surface is flat or gently curved, a chunk's triangles are then
//! merged into fewer, moving it no more than [`FLAT_ENOUGH`]: each kind of
//! ground apart, the points it shares with the chunks beside it and with
//! other kinds kept where they are, so they still meet edge for edge. Its
//! corners are kept so a second and third time, merged further ([`COARSER`]),
//! for drawing it far away: kept the same where chunks meet, a chunk at
//! any level meets the next at any other.
//!
//! Each corner of a triangle is drawn as two pairs of words, as
//! `smoothPoint` in `shaders/blocks.slang` reads them: where it is in its
//! chunk and the chunk's number in its world's pool (which the renderer
//! fills in); then the way the ground faces there, and the surfaces of the
//! block of ground its face comes from, for its top, sides and bottom. The
//! same triangles, their corners shared, are kept for collision and the
//! GI's rays.
//!
//! Water is not ground: the surface runs between the ground and the water
//! as it does the air, each block of water taken as far out of the ground as
//! the ground beside it says. The water has a smooth surface of its own,
//! made the same way, after the ground's corners ([`water_surface`]): over
//! how far into water or ground each block's middle is, so its edge meets
//! the ground's surface, and only where water meets air, so the ground
//! under it is not drawn twice. A full block's surface is its top; a sea's
//! top block keeps where its surface lies (`flow::surface_fill`), so a
//! generated sea lies flat at its sea level and round a planet; flowing
//! water stands as high as it flows.

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId, CHUNK, FULL, Fill, depth_of, fill_of},
    flow::smooth_surface,
    is_ground, is_water,
    mesh::{ChunkMesh, FACINGS},
};

/// How far into a chunk a block reaches the faces of the chunks beside it:
/// those before it from its first two blocks, those after it from its last
/// three. A chunk's cubes reach two blocks before it and one past it, and
/// the way the ground faces at their corners one block further.
pub const REACH_AFTER: u32 = 1;
pub const REACH_BELOW: u32 = CHUNK - 3;

/// The blocks round a chunk it reads: from three before it to two past it
/// along each axis.
const BEFORE: i32 = 3;
const SPAN: usize = CHUNK as usize + 5;

/// The cubes round a chunk that can have points: from two blocks before it
/// to the last of its own.
const CUBES: usize = CHUNK as usize + 2;

/// How far before a chunk its cubes start.
const CUBES_BEFORE: i32 = 2;

/// Where in its chunk a point is kept, along each axis: in 1024ths of a
/// block from two blocks before its corner. A chunk is a whole number of
/// them, so a point two chunks share is kept alike in both.
pub const POINT_FROM: f32 = -2.0;
pub const POINT_STEPS: f32 = 1024.0;

/// A cube's twelve edges, by its corners (bit 0 x, bit 1 y, bit 2 z).
const EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// The way a surface faces as one word: folded onto an octahedron, two
/// 12-bit numbers, as dug ground's are.
pub fn pack_normal(normal: [f32; 3]) -> u32 {
    let [x, y, z] = normal;
    let length = (x.abs() + y.abs() + z.abs()).max(1e-6);
    let (x, y, z) = (x / length, y / length, z / length);
    let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
    let (x, z) = if y < 0.0 { ((1.0 - z.abs()) * sign(x), (1.0 - x.abs()) * sign(z)) } else { (x, z) };
    let quantise = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 4095.0).round() as u32;
    quantise(x) | quantise(z) << 12
}

/// A point's place in its chunk as a number from 0 to 65535 along one axis.
fn quantise(v: f32) -> u32 {
    ((v - POINT_FROM) * POINT_STEPS).round().clamp(0.0, 65535.0) as u32
}

/// The blocks round chunk `key` and how far into the ground each one's
/// middle is, from two before it to one past it along each axis.
struct Padded {
    blocks: Vec<BlockId>,
    depths: Vec<f32>,
}

impl Padded {
    fn of(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces]) -> Self {
        let mut blocks = vec![AIR; SPAN * SPAN * SPAN];
        let mut depths = vec![-1.0; SPAN * SPAN * SPAN];
        let base = key.map(|v| v * CHUNK as i32 - BEFORE);
        let mut kept: Option<([i32; 3], Option<&std::sync::Arc<super::chunk::Chunk>>)> = None;
        let mut index = 0;
        for y in 0..SPAN as i32 {
            for z in 0..SPAN as i32 {
                for x in 0..SPAN as i32 {
                    let (chunk_key, [lx, ly, lz]) = BlockMap::chunk_of([base[0] + x, base[1] + y, base[2] + z]);
                    if kept.is_none_or(|(k, _)| k != chunk_key) {
                        kept = Some((chunk_key, map.chunk(chunk_key)));
                    }
                    if let Some((_, Some(chunk))) = kept {
                        blocks[index] = chunk.get(lx, ly, lz);
                        depths[index] = depth_of(chunk.fill(lx, ly, lz));
                    }
                    index += 1;
                }
            }
        }
        // Water keeps no fill of its own: a block further out than the ground beside it, or a block out of it.
        let span = SPAN as i32;
        let at = |x: i32, y: i32, z: i32| ((y * span + z) * span + x) as usize;
        let mut estimated = Vec::new();
        for y in 0..span {
            for z in 0..span {
                for x in 0..span {
                    if !is_water(faces, blocks[at(x, y, z)]) {
                        continue;
                    }
                    let beside = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
                    let out = beside
                        .iter()
                        .map(|[dx, dy, dz]| (x + dx, y + dy, z + dz))
                        .filter(|&(x, y, z)| [x, y, z].iter().all(|v| (0..span).contains(v)))
                        .filter(|&(x, y, z)| is_ground(faces, blocks[at(x, y, z)]))
                        .map(|(x, y, z)| depths[at(x, y, z)] - 1.0)
                        .fold(-1.0f32, f32::max);
                    estimated.push((at(x, y, z), out.min(-1e-3)));
                }
            }
        }
        for (index, depth) in estimated {
            depths[index] = depth;
        }
        Self { blocks, depths }
    }

    /// Where block `at` is kept, `at` counted from the chunk's corner.
    fn index(at: [i32; 3]) -> usize {
        let [x, y, z] = at.map(|v| (v + BEFORE) as usize);
        (y * SPAN + z) * SPAN + x
    }

    fn block(&self, at: [i32; 3]) -> BlockId {
        self.blocks[Self::index(at)]
    }

    fn depth(&self, at: [i32; 3]) -> f32 {
        self.depths[Self::index(at)]
    }

    /// The point of the cube whose lowest corner is block `low`'s middle, in
    /// blocks from the chunk's corner, and the way the ground faces there.
    fn point(&self, low: [i32; 3]) -> ([f32; 3], [f32; 3]) {
        let corner = |c: usize| [c & 1, (c >> 1) & 1, (c >> 2) & 1].map(|bit| bit as i32);
        let values: [f32; 8] = std::array::from_fn(|c| {
            let [x, y, z] = corner(c);
            self.depth([low[0] + x, low[1] + y, low[2] + z])
        });
        let mut sum = [0.0f32; 3];
        let mut count = 0.0;
        for (a, b) in EDGES {
            if (values[a] > 0.0) == (values[b] > 0.0) {
                continue;
            }
            let t = (values[a] / (values[a] - values[b])).clamp(0.0, 1.0);
            let (from, to) = (corner(a).map(|v| v as f32), corner(b).map(|v| v as f32));
            for axis in 0..3 {
                sum[axis] += from[axis] + (to[axis] - from[axis]) * t;
            }
            count += 1.0;
        }
        let within = if count > 0.0 { sum.map(|v| v / count) } else { [0.5; 3] };
        // Down the slope of how full the blocks are: at each corner from the blocks either side of it, then
        // blended across the cube to the point, so neighbouring points' ways agree and shading runs smoothly.
        let mut slope = [0.0f32; 3];
        for c in 0..8 {
            let offset = corner(c);
            let weight: f32 =
                (0..3).map(|axis| if offset[axis] == 1 { within[axis] } else { 1.0 - within[axis] }).product();
            let at = [0, 1, 2].map(|axis| low[axis] + offset[axis]);
            for axis in 0..3 {
                let (mut before, mut after) = (at, at);
                before[axis] -= 1;
                after[axis] += 1;
                slope[axis] += weight * (self.depth(after) - self.depth(before)) * 0.5;
            }
        }
        let mut normal = slope.map(|v| -v);
        let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 1e-5 {
            // Evenly full all round: from the cube's ground towards its air.
            normal = [0.0; 3];
            for (c, &value) in values.iter().enumerate() {
                let away = if value > 0.0 { -1.0 } else { 1.0 };
                for (axis, &bit) in corner(c).iter().enumerate() {
                    normal[axis] += (bit as f32 - 0.5) * away;
                }
            }
        }
        let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
        let normal = if length > 1e-6 { normal.map(|v| v / length) } else { [0.0, 1.0, 0.0] };
        let place = [0, 1, 2].map(|axis| low[axis] as f32 + 0.5 + within[axis]);
        (place, normal)
    }
}

/// Chunk `key`'s surface in `map`, drawn smooth, each face with the
/// surfaces `faces` gives its block of ground (its top, sides and bottom),
/// by its number.
pub fn mesh_chunk(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces]) -> ChunkMesh {
    mesh_within(map, key, faces, FLAT_ENOUGH)
}

/// Chunk `key`'s surface, as [`mesh_chunk`] makes it, its triangles merged
/// where the surface moves no more than `error` blocks.
pub(crate) fn mesh_within(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces], error: f32) -> ChunkMesh {
    let mut out = ChunkMesh::default();
    if !map.chunk(key).is_some_and(|chunk| chunk.has_blocks()) || map.buried(key, faces) {
        return out;
    }
    let padded = Padded::of(map, key, faces);
    // Each cube's point's number among the mesh's points, made as faces come to it.
    let mut numbers = vec![u32::MAX; CUBES * CUBES * CUBES];
    let mut normals: Vec<[f32; 3]> = Vec::new();
    // Whether each point's cube is on the chunk's outer layer, where the chunks beside it may share it.
    let mut shared: Vec<bool> = Vec::new();
    let mut number = |low: [i32; 3], out: &mut ChunkMesh| -> u32 {
        let [x, y, z] = low.map(|v| (v + CUBES_BEFORE) as usize);
        let slot = &mut numbers[(y * CUBES + z) * CUBES + x];
        if *slot == u32::MAX {
            let (place, normal) = padded.point(low);
            out.positions.push(place);
            normals.push(normal);
            shared.push(low.iter().any(|&v| v < 0 || v >= CHUNK as i32 - 1));
            *slot = out.positions.len() as u32 - 1;
        }
        *slot
    };
    let mut triangles: Vec<([u32; 3], BlockId)> = Vec::new();
    let side = CHUNK as i32;
    for y in 0..side {
        for z in 0..side {
            for x in 0..side {
                let at = [x, y, z];
                let block = padded.block(at);
                if !is_ground(faces, block) {
                    continue;
                }
                for facing in 0..FACINGS {
                    let (axis, out_along) = (facing / 2, facing % 2 == 0);
                    let mut next = at;
                    next[axis] += if out_along { 1 } else { -1 };
                    if is_ground(faces, padded.block(next)) {
                        continue;
                    }
                    // The four cubes round the line between the two middles, turning about the axis.
                    let lower = if out_along { at } else { next };
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    let cube = |du: i32, dv: i32| {
                        let mut low = lower;
                        low[u] -= 1 - du;
                        low[v] -= 1 - dv;
                        low
                    };
                    let mut quad = [cube(0, 0), cube(1, 0), cube(1, 1), cube(0, 1)];
                    if !out_along {
                        quad.reverse();
                    }
                    let [a, b, c, d] = quad.map(|low| number(low, &mut out));
                    // Split along the shorter way across.
                    let p = |i: u32| out.positions[i as usize];
                    let apart = |i: u32, j: u32| (0..3).map(|k| (p(i)[k] - p(j)[k]).powi(2)).sum::<f32>();
                    let split =
                        if apart(a, c) <= apart(b, d) { [[a, b, c], [a, c, d]] } else { [[a, b, d], [b, c, d]] };
                    for triangle in split {
                        let [i, j, k] = triangle;
                        if i != j && j != k && i != k {
                            triangles.push((triangle, block));
                        }
                    }
                }
            }
        }
    }
    let looks: Vec<([u32; 3], [u8; 3])> = triangles
        .into_iter()
        .map(|(triangle, block)| {
            let BlockFaces { top, sides, bottom, .. } = faces.get(usize::from(block)).copied().unwrap_or_default();
            (triangle, [top, sides, bottom])
        })
        .collect();
    let every = (out.positions.clone(), normals.clone());
    let (points, normals, triangles) = simplified((&out.positions, &normals, &shared), looks.clone(), error);
    out.positions = points;
    let corners = |points: &[[f32; 3]], normals: &[[f32; 3]], triangle: [u32; 3], [top, sides, bottom]: [u8; 3]| {
        triangle.map(|i| {
            let [x, y, z] = points[i as usize];
            [
                [quantise(x) | quantise(y) << 16, quantise(z)],
                [pack_normal(normals[i as usize]) | u32::from(bottom) << 24, u32::from(top) | u32::from(sides) << 8],
            ]
        })
    };
    for (triangle, look) in triangles {
        out.indices.extend(triangle);
        out.quads.extend(corners(&out.positions, &normals, triangle, look).into_iter().flatten());
    }
    // Coarser levels after it, for far away, where they are made, merged as it is.
    let mut ends = [out.quads.len() as u32; LEVELS];
    if error > 0.0 {
        for (level, coarser) in COARSER.into_iter().enumerate() {
            let (points, normals, triangles) = simplified((&every.0, &every.1, &shared), looks.clone(), coarser);
            for (triangle, look) in triangles {
                out.quads.extend(corners(&points, &normals, triangle, look).into_iter().flatten());
            }
            ends[level + 1] = out.quads.len() as u32;
        }
    }
    let last = ends[LEVELS - 1];
    out.buckets = [0, ends[0], ends[1], ends[2], last, last, last];
    out.cone = cone(&out.positions, &out.indices);
    if map.chunk(key).is_some_and(|chunk| chunk.may_hold(|block| is_water(faces, block))) {
        out.quads.extend(water_surface(map, key, faces, &padded));
    }
    out
}

/// The cone the ways `indices`' triangles over `points` face lie within:
/// its axis, the way they face on average, and the sine of the angle from
/// it to the furthest; `None` for none, or where some turn a quarter or more
/// from it.
fn cone(points: &[[f32; 3]], indices: &[u32]) -> Option<[f32; 4]> {
    let ways: Vec<[f32; 3]> = indices
        .chunks_exact(3)
        .filter_map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| points[t[k] as usize]);
            let (e, f) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
            let n = [e[1] * f[2] - e[2] * f[1], e[2] * f[0] - e[0] * f[2], e[0] * f[1] - e[1] * f[0]];
            let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
            (length > 1e-9).then(|| n.map(|v| v / length))
        })
        .collect();
    let sum = ways.iter().fold([0.0f32; 3], |sum, n| [0, 1, 2].map(|k| sum[k] + n[k]));
    let length = sum.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length < 1e-6 {
        return None;
    }
    let axis = sum.map(|v| v / length);
    let least = ways.iter().map(|n| (0..3).map(|k| n[k] * axis[k]).sum::<f32>()).fold(1.0f32, f32::min);
    // A little wider than the triangles turn, for the rounding of their corners' places when drawn.
    let least = least - 0.01;
    (least > 0.0).then(|| [axis[0], axis[1], axis[2], (1.0 - least * least).max(0.0).sqrt()])
}

/// How far a smooth surface may move, in blocks, as its triangles are
/// merged where it is flat or gently curved.
pub const FLAT_ENOUGH: f32 = 0.1;

/// How far each coarser level of a smooth chunk's surface may move, in
/// blocks, for drawing it far away.
pub const COARSER: [f32; 2] = [0.5, 2.0];

/// How many levels a smooth chunk's surface is kept at: its own, then each
/// coarser, its corners one level after another (see `mesh::ChunkMesh`'s
/// buckets).
pub const LEVELS: usize = 1 + COARSER.len();

/// How much a turn of the way the ground faces counts against merging
/// triangles, beside how far their surface moves, so its shading stays.
const TURN_WEIGHT: f32 = 0.25;

/// A chunk's triangles, each with its surfaces (top, sides, bottom), merged
/// into fewer where the surface moves no more than `error` blocks: each
/// kind of ground apart, and the edges of each, where it meets the chunk's
/// sides or another kind, kept where they are, so chunks and kinds still
/// meet point for point. `shared` says which points the chunks beside it
/// may have too, which stay. The points the triangles use, with the way the
/// ground faces at each, and the triangles numbered among them.
#[allow(clippy::type_complexity)]
fn simplified(
    (points, normals, shared): (&[[f32; 3]], &[[f32; 3]], &[bool]),
    triangles: Vec<([u32; 3], [u8; 3])>,
    error: f32,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<([u32; 3], [u8; 3])>) {
    use meshopt::{SimplifyOptions, simplify_with_attributes_and_locks_decoder};
    let mut kinds: Vec<([u8; 3], Vec<u32>)> = Vec::new();
    // Points two kinds meet at stay too, though they lie inside each kind's ground, as where two touch at a corner.
    let mut locks = shared.to_vec();
    let mut kind_of: Vec<Option<usize>> = vec![None; points.len()];
    for (triangle, look) in triangles {
        let kind = match kinds.iter().position(|(kind, _)| *kind == look) {
            Some(kind) => kind,
            None => {
                kinds.push((look, Vec::new()));
                kinds.len() - 1
            }
        };
        kinds[kind].1.extend(triangle);
        for i in triangle.map(|i| i as usize) {
            match kind_of[i] {
                Some(other) if other != kind => locks[i] = true,
                _ => kind_of[i] = Some(kind),
            }
        }
    }
    let attributes: Vec<f32> = normals.iter().flatten().copied().collect();
    let options = SimplifyOptions::LockBorder | SimplifyOptions::ErrorAbsolute | SimplifyOptions::Sparse;
    let mut numbers = vec![u32::MAX; points.len()];
    let (mut kept, mut ways, mut merged) = (Vec::new(), Vec::new(), Vec::new());
    for (look, indices) in kinds {
        let fewer = match error > 0.0 {
            true => simplify_with_attributes_and_locks_decoder(
                &indices,
                points,
                &attributes,
                &[TURN_WEIGHT; 3],
                size_of::<[f32; 3]>(),
                &locks,
                0,
                error,
                options,
                None,
            ),
            false => indices,
        };
        for triangle in fewer.chunks_exact(3) {
            let numbered = [0, 1, 2].map(|k| {
                let i = triangle[k] as usize;
                if numbers[i] == u32::MAX {
                    numbers[i] = kept.len() as u32;
                    kept.push(points[i]);
                    ways.push(normals[i]);
                }
                numbers[i]
            });
            merged.push((numbered, look));
        }
    }
    (kept, ways, merged)
}

/// The faces between `quad`'s four cubes' points, each cube by its lowest
/// corner, split along the shorter way across: two triangles, or fewer
/// where points meet.
fn split(quad: [u32; 4], place: impl Fn(u32) -> [f32; 3]) -> Vec<[u32; 3]> {
    let [a, b, c, d] = quad;
    let apart = |i: u32, j: u32| (0..3).map(|k| (place(i)[k] - place(j)[k]).powi(2)).sum::<f32>();
    let split = if apart(a, c) <= apart(b, d) { [[a, b, c], [a, c, d]] } else { [[a, b, d], [b, c, d]] };
    split.into_iter().filter(|&[i, j, k]| i != j && j != k && i != k).collect()
}

/// The four cubes round the line from block `at`'s middle to the next
/// block's along facing `facing`, each by its lowest corner, in the order
/// that makes their face look along the facing.
fn round_line(at: [i32; 3], facing: usize) -> [[i32; 3]; 4] {
    let (axis, out_along) = (facing / 2, facing % 2 == 0);
    let mut next = at;
    next[axis] += if out_along { 1 } else { -1 };
    let lower = if out_along { at } else { next };
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let cube = |du: i32, dv: i32| {
        let mut low = lower;
        low[u] -= 1 - du;
        low[v] -= 1 - dv;
        low
    };
    let mut quad = [cube(0, 0), cube(1, 0), cube(1, 1), cube(0, 1)];
    if !out_along {
        quad.reverse();
    }
    quad
}

/// Chunk `key`'s water's surface, drawn smooth: where its water meets air,
/// over a field of how far each block's middle is into water or ground
/// (water's by how high its surface stands, see `flow::smooth_surface`;
/// the ground's by its fill), the air a block further out than the water
/// beside it, whatever the ground near it. Each corner is two pairs of words, as the ground's are, its
/// surface the water's.
fn water_surface(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces], padded: &Padded) -> Vec<[u32; 2]> {
    let fills = super::mesh::fills(map, key);
    let span = SPAN as i32;
    let place = |index: usize| {
        let index = index as i32;
        [index % span, index / (span * span), (index / span) % span].map(|v| v - BEFORE)
    };
    let mut field: Vec<f32> = (0..padded.blocks.len())
        .map(|index| {
            let block = padded.blocks[index];
            if is_water(faces, block) {
                smooth_surface(fills(place(index)))
            } else if is_ground(faces, block) {
                padded.depths[index].max(0.05)
            } else {
                -1.0
            }
        })
        .collect();
    // Air beside water: a block further out than the water's surface. How near the ground air is says nothing
    // of the water, and would draw the water out over a beach.
    let beside = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
    let mut out_of_water = Vec::new();
    for index in 0..field.len() {
        if padded.blocks[index] != AIR {
            continue;
        }
        let at = place(index);
        let near = beside
            .iter()
            .map(|step| [0, 1, 2].map(|i| at[i] + step[i]))
            .filter(|next| next.iter().all(|&v| (-BEFORE..span - BEFORE).contains(&v)))
            .filter(|&next| is_water(faces, padded.block(next)))
            .map(|next| field[Padded::index(next)] - 1.0)
            .fold(f32::MIN, f32::max);
        if near > f32::MIN {
            out_of_water.push((index, near.min(-1e-3)));
        }
    }
    for (index, value) in out_of_water {
        field[index] = value;
    }
    let wet = Padded { blocks: padded.blocks.clone(), depths: field };
    let mut numbers = vec![u32::MAX; CUBES * CUBES * CUBES];
    let (mut points, mut normals, mut shared) = (Vec::new(), Vec::new(), Vec::new());
    let mut number = |low: [i32; 3]| -> u32 {
        let [x, y, z] = low.map(|v| (v + CUBES_BEFORE) as usize);
        let slot = &mut numbers[(y * CUBES + z) * CUBES + x];
        if *slot == u32::MAX {
            let (place, normal) = wet.point(low);
            points.push(place);
            normals.push(normal);
            shared.push(low.iter().any(|&v| v < 0 || v >= CHUNK as i32 - 1));
            *slot = points.len() as u32 - 1;
        }
        *slot
    };
    let mut quads: Vec<([u32; 4], BlockId)> = Vec::new();
    let side = CHUNK as i32;
    for y in 0..side {
        for z in 0..side {
            for x in 0..side {
                let at = [x, y, z];
                let block = padded.block(at);
                if !is_water(faces, block) {
                    continue;
                }
                for facing in 0..FACINGS {
                    let mut next = at;
                    next[facing / 2] += if facing % 2 == 0 { 1 } else { -1 };
                    if padded.block(next) == AIR {
                        quads.push((round_line(at, facing).map(&mut number), block));
                    }
                }
            }
        }
    }
    let triangles = quads.into_iter().flat_map(|(quad, block)| {
        let surface = faces.get(usize::from(block)).copied().unwrap_or_default().top;
        split(quad, |i| points[i as usize]).into_iter().map(move |t| (t, [surface; 3]))
    });
    // Merged where it lies flat or gently curved, as the ground is.
    let triangles = triangles.collect();
    let (points, normals, triangles) = simplified((&points, &normals, &shared), triangles, FLAT_ENOUGH);
    let mut words = Vec::new();
    for (triangle, [surface, ..]) in triangles {
        let surface = u32::from(surface);
        for i in triangle {
            let [x, y, z] = points[i as usize];
            words.push([quantise(x) | quantise(y) << 16, quantise(z)]);
            words.push([pack_normal(normals[i as usize]) | surface << 24, surface | surface << 8]);
        }
    }
    words
}

/// What a ball does to a world drawn smooth: adds ground of a kind where
/// there is none (the ground there keeps its kinds), takes the ground away,
/// or turns the ground in it to a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ball {
    Add(BlockId),
    Take,
    Paint(BlockId),
}

/// The changes a ball of `radius` blocks round `middle` (in blocks from the
/// world's corner) makes to `map`, drawn smooth: its surface lies between
/// blocks as far out as it reaches, each block as full as the ball and the
/// ground there make it. Water, by `faces`, is no ground: ground added
/// takes its place, and taking or painting ground leaves it as it is.
pub fn ball(
    map: &BlockMap,
    middle: [f32; 3],
    radius: f32,
    does: Ball,
    faces: &[BlockFaces],
) -> Vec<([i32; 3], BlockId, Fill)> {
    let radius = radius.max(0.0);
    let low = middle.map(|v| (v - radius - 1.5).floor() as i32);
    let high = middle.map(|v| (v + radius + 1.5).ceil() as i32);
    let mut changes = Vec::new();
    for z in low[2]..=high[2] {
        for y in low[1]..=high[1] {
            for x in low[0]..=high[0] {
                // How far inside the ball the block's middle is, in blocks.
                let away = [x, y, z].iter().zip(middle).map(|(&v, m)| (v as f32 + 0.5 - m).powi(2)).sum::<f32>();
                let inside = radius - away.sqrt();
                if inside <= -1.0 {
                    continue;
                }
                let at = [x, y, z];
                let (now, fill) = (map.get(at), map.fill(at));
                let water = is_water(faces, now);
                let depth = if water { -1.0 } else { depth_of(fill) };
                let (block, depth) = match does {
                    // Water is left as it is but where ground takes its place.
                    Ball::Add(kind) if inside > depth && !(water && inside <= 0.0) => {
                        let ground = is_ground(faces, now);
                        (if ground || inside <= 0.0 { now } else { kind }, inside)
                    }
                    Ball::Take if -inside < depth && !water => (if -inside > 0.0 { now } else { AIR }, -inside),
                    Ball::Paint(kind) if is_ground(faces, now) && inside > 0.0 && kind != AIR => (kind, depth),
                    _ => continue,
                };
                // Water added is a source, full.
                let fill = if is_water(faces, block) { FULL } else { fill_of(depth, block != AIR) };
                changes.push((at, block, fill));
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::chunk::fill_of;

    /// Ground whose surface is `height` blocks up, `size` blocks along x and
    /// z from the corner, each block as full as the surface makes it.
    fn ground(size: [i32; 2], height: impl Fn(i32, i32) -> f32) -> BlockMap {
        let mut map = BlockMap::default();
        for z in 0..size[1] {
            for x in 0..size[0] {
                let surface = height(x, z);
                for y in 0..(surface.ceil() as i32 + 1) {
                    let depth = surface - (y as f32 + 0.5);
                    let block = if depth > 0.0 { 1 } else { AIR };
                    map.set_cell([x, y, z], block, fill_of(depth, depth > 0.0));
                }
            }
        }
        map
    }

    #[test]
    fn a_smooth_surface_lies_where_its_blocks_fill_says_and_faces_away_from_the_ground() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 2, 3)];
        let map = ground([32, 32], |_, _| 10.3);
        // Every point, none merged away.
        let mesh = mesh_within(&map, [0, 0, 0], &faces, 0.0);
        assert!(!mesh.is_empty());
        // Away from the ground's edges, its top: flat at 10.3 blocks, facing up.
        let tops: Vec<usize> = (0..mesh.positions.len())
            .filter(|&i| {
                let [x, y, z] = mesh.positions[i];
                (3.0..28.0).contains(&x) && (3.0..28.0).contains(&z) && y > 5.0
            })
            .collect();
        assert!(!tops.is_empty());
        for &i in &tops {
            assert!((mesh.positions[i][1] - 10.3).abs() < 0.02, "{:?}", mesh.positions[i]);
        }
        // Every triangle faces away from the ground, and is drawn with its block's surfaces.
        let middle = [16.0, 5.0, 16.0];
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[triangle[i] as usize]);
            let along = |p: [f32; 3], q: [f32; 3]| [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let (e, f) = (along(a, b), along(a, c));
            let facing = [e[1] * f[2] - e[2] * f[1], e[2] * f[0] - e[0] * f[2], e[0] * f[1] - e[1] * f[0]];
            let out = along(middle, a);
            assert!((0..3).map(|k| facing[k] * out[k]).sum::<f32>() > 0.0, "{a:?} {b:?} {c:?}");
        }
        assert_eq!(mesh.quads.len(), mesh.indices.len() * 2, "two pairs of words a corner");
        assert_eq!(mesh.quads[1][1] & 0xFFFF, 1 | 2 << 8, "top and sides");
        assert_eq!(mesh.quads[1][0] >> 24, 3, "bottom");
    }

    #[test]
    fn a_slope_runs_smoothly_and_chunks_meet_at_the_same_points() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1)];
        // A gentle slope across two chunks: a block up every eight along x.
        let map = ground([64, 8], |x, _| 6.0 + x as f32 / 8.0);
        let every = mesh_within(&map, [0, 0, 0], &faces, 0.0);
        let (first, second) = (mesh_chunk(&map, [0, 0, 0], &faces), mesh_chunk(&map, [1, 0, 0], &faces));
        // On the slope, its top rises steadily, with no steps.
        let height_at = |mesh: &ChunkMesh, x: f32| {
            mesh.positions
                .iter()
                .filter(|p| (p[0] - x).abs() < 0.5 && (2.0..6.0).contains(&p[2]) && p[1] > 3.0)
                .map(|p| p[1])
                .fold(f32::MIN, f32::max)
        };
        // Its points lie between the columns, each halfway between two columns' heights.
        for x in [9.0f32, 13.0, 17.0, 22.0] {
            let expected = 6.0 + (x - 0.5) / 8.0;
            assert!((height_at(&every, x) - expected).abs() < 0.1, "at {x}: {}", height_at(&every, x));
        }
        // The points where the two chunks meet are the same points, each in its own chunk's terms.
        let border = |mesh: &ChunkMesh, x: f32| -> Vec<[i32; 3]> {
            let mut found: Vec<[i32; 3]> = mesh
                .positions
                .iter()
                .filter(|p| (p[0] - x).abs() < 0.5)
                .map(|p| p.map(|v| (v * 1000.0).round() as i32))
                .collect();
            found.sort_unstable();
            found
        };
        let shifted: Vec<[i32; 3]> =
            border(&second, 0.0).into_iter().map(|[x, y, z]| [x + 32_000, y, z]).collect::<Vec<_>>();
        let mut shifted = shifted;
        shifted.sort_unstable();
        let mine: Vec<[i32; 3]> = border(&first, 32.0);
        assert!(!mine.is_empty());
        assert!(mine.iter().all(|p| shifted.contains(p)), "the chunks' points at x 32 differ");
    }

    #[test]
    fn a_ball_adds_and_takes_ground_its_edge_between_blocks() {
        let mut map = ground([16, 16], |_, _| 4.0);
        // Added on the ground: wholly ground at its middle, its edge half a block past the next block up.
        let added = ball(&map, [8.5, 4.5, 8.5], 1.5, Ball::Add(2), &[]);
        for (at, block, fill) in added {
            map.set_cell(at, block, fill);
        }
        assert_eq!((map.get([8, 4, 8]), map.fill([8, 4, 8])), (2, 255));
        assert_eq!((map.get([8, 6, 8]), map.fill([8, 6, 8])), (AIR, fill_of(-0.5, false)));
        assert_eq!(map.get([8, 3, 8]), 1, "the ground under it keeps its kind");
        // Taken away again, deeper: a hollow whose floor lies between blocks.
        for (at, block, fill) in ball(&map, [8.5, 4.0, 8.5], 2.25, Ball::Take, &[]) {
            map.set_cell(at, block, fill);
        }
        assert_eq!(map.get([8, 4, 8]), AIR);
        assert_eq!((map.get([8, 1, 8]), map.fill([8, 1, 8])), (1, fill_of(0.25, true)));
        // Painted: only the ground in it changes kind, as full as it was.
        let before = map.fill([3, 3, 3]);
        for (at, block, fill) in ball(&map, [3.5, 3.5, 3.5], 1.0, Ball::Paint(3), &[]) {
            map.set_cell(at, block, fill);
        }
        assert_eq!((map.get([3, 3, 3]), map.fill([3, 3, 3])), (3, before));
        assert_eq!(map.get([3, 4, 3]), AIR);
    }

    #[test]
    fn water_over_smooth_ground_lies_smooth_at_its_level_and_the_ground_runs_on_under_it() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::water(7)];
        // Ground 3.3 blocks up, a lake over it up to 6 blocks, across the middle of the chunk.
        let mut map = ground([32, 32], |_, _| 3.3);
        for y in 3..6 {
            for z in 8..24 {
                for x in 8..24 {
                    map.set([x, y, z], 2);
                }
            }
        }
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        // The ground under the water lies where its fill says, as it does out of it.
        let inside = |p: &&[f32; 3]| (10.0..22.0).contains(&p[0]) && (10.0..22.0).contains(&p[2]);
        for p in mesh.positions.iter().filter(|p| p[1] > 2.0).filter(inside) {
            assert!((p[1] - 3.3).abs() < 0.02, "under the water: {p:?}");
        }
        // The water's surface after the ground's corners, two pairs of words a corner, three corners a face: flat at
        // its top, 6 blocks up, facing up, drawn with its surface; and none of it on the ground under it.
        let water_corners = |mesh: &ChunkMesh| -> Vec<([f32; 3], u32)> {
            let (first, _) = mesh.water();
            let words = &mesh.quads[first as usize..];
            assert_eq!(words.len() % 6, 0);
            words
                .chunks(2)
                .map(|pair| {
                    let at = [pair[0][0] & 0xFFFF, pair[0][0] >> 16, pair[0][1] & 0xFFFF];
                    (at.map(|q| q as f32 / POINT_STEPS + POINT_FROM), pair[1][1] & 0xFF)
                })
                .collect()
        };
        assert_eq!((mesh.buckets[1] as usize, mesh.water().0), (mesh.indices.len() * 2, mesh.buckets[FACINGS]));
        let corners = water_corners(&mesh);
        assert!(!corners.is_empty() && corners.iter().all(|&(_, surface)| surface == 7));
        let middle: Vec<_> = corners.iter().filter(|(p, _)| inside(&p)).collect();
        assert!(!middle.is_empty());
        assert!(middle.iter().all(|(p, _)| (p[1] - 6.0).abs() < 0.01), "its top at 6 blocks: {:?}", middle[0]);
        // A sea's top blocks keep where its surface lies: there, a fifth of a block over their middles.
        let mut sea = map.clone();
        for z in 8..24 {
            for x in 8..24 {
                sea.set_cell([x, 5, z], 2, super::super::flow::surface_fill(0.2));
            }
        }
        let corners = water_corners(&mesh_chunk(&sea, [0, 0, 0], &faces));
        let middle: Vec<_> = corners.iter().filter(|(p, _)| inside(&p)).collect();
        assert!(middle.iter().all(|(p, _)| (p[1] - 5.7).abs() < 0.01), "its sea level: {:?}", middle[0]);
        // Ground added into the water takes its place; taking ground leaves the water.
        let added = ball(&map, [16.5, 4.5, 16.5], 1.0, Ball::Add(1), &faces);
        assert!(added.iter().any(|&(at, block, _)| at == [16, 4, 16] && block == 1));
        let taken = ball(&map, [16.5, 4.5, 16.5], 1.0, Ball::Take, &faces);
        assert!(taken.iter().all(|&(at, ..)| map.get(at) != 2), "no water taken as ground");
    }

    #[test]
    fn a_lone_block_is_a_closed_shape_of_six_faces() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1)];
        let mut map = BlockMap::default();
        map.set([5, 5, 5], 1);
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        assert_eq!(mesh.positions.len(), 8, "a point for each cube round it");
        assert_eq!(mesh.indices.len(), 6 * 2 * 3);
        // Each edge of a closed shape is met by two triangles, once each way.
        let mut edges = std::collections::HashMap::new();
        for triangle in mesh.indices.chunks(3) {
            for (a, b) in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        assert!(edges.keys().all(|&(a, b)| edges.get(&(b, a)) == Some(&1)));
        assert!(mesh_chunk(&map, [1, 0, 0], &faces).is_empty(), "no faces where it has no ground");
    }

    #[test]
    fn flat_ground_is_merged_into_few_triangles_and_stays_where_it_was() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 2, 3)];
        let map = ground([32, 32], |_, _| 6.4);
        let (merged, every) = (mesh_chunk(&map, [0, 0, 0], &faces), mesh_within(&map, [0, 0, 0], &faces, 0.0));
        let tops = |mesh: &ChunkMesh| {
            mesh.indices.chunks(3).filter(|t| t.iter().all(|&i| mesh.positions[i as usize][1] > 6.0)).count()
        };
        assert!(tops(&merged) * 4 < tops(&every), "{} triangles of {}", tops(&merged), tops(&every));
        assert!(merged.positions.iter().filter(|p| p[1] > 6.0).all(|p| (p[1] - 6.4).abs() < 0.02), "on the same plane");
        assert_eq!(merged.buckets[1] as usize, merged.indices.len() * 2, "two pairs of words a corner");
        // A chunk of it holding its bottom faces every way; one of its top alone faces up.
        assert_eq!(merged.cone, None);
        let deep = ground([96, 96], |_, _| 40.4);
        let [x, y, z, spread] = mesh_chunk(&deep, [1, 1, 1], &faces).cone.expect("all up");
        assert!((y - 1.0).abs() < 1e-3 && x.abs() < 1e-3 && z.abs() < 1e-3 && spread < 0.15, "{:?}", [x, y, z, spread]);
    }

    #[test]
    fn a_hill_over_eight_chunks_merged_stays_closed_its_chunks_and_kinds_meeting_edge_for_edge() {
        // A ball of ground round the corner eight chunks share, its top half of another kind, on hills of noise.
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::new(2, 2, 2)];
        let mut map = BlockMap::default();
        let middle = [32.0, 32.0, 32.0];
        let changes = ball(&map, middle, 20.0, Ball::Add(1), &faces);
        for (at, block, fill) in changes {
            map.set_cell(at, block, fill);
        }
        for (at, _, fill) in ball(&map, [32.0, 44.0, 32.0], 14.0, Ball::Paint(2), &faces) {
            map.set_cell(at, 2, fill);
        }
        for (at, block, fill) in ball(&map, [40.0, 50.0, 30.0], 6.0, Ball::Take, &faces) {
            map.set_cell(at, block, fill);
        }
        let keys: Vec<[i32; 3]> = (0..8).map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1]).collect();
        // Every edge, by where its ends are in the world, and how many triangles go along it that way.
        let edges = |error: f32| {
            let mut edges = std::collections::HashMap::new();
            let mut triangles = 0;
            for key in &keys {
                let mesh = mesh_within(&map, *key, &faces, error);
                let at = |i: u32| {
                    let p = mesh.positions[i as usize];
                    [0, 1, 2].map(|k| ((p[k] + (key[k] * CHUNK as i32) as f32) * 1024.0).round() as i64)
                };
                for t in mesh.indices.chunks(3) {
                    triangles += 1;
                    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        *edges.entry((at(a), at(b))).or_insert(0) += 1;
                    }
                }
            }
            (edges, triangles)
        };
        let (merged, fewer) = edges(FLAT_ENOUGH);
        let (_, every) = edges(0.0);
        assert!(fewer * 3 < every * 2, "{fewer} triangles of {every}");
        // As many triangles back the other way: where two sheets touch, an edge has two each way.
        let open = merged.iter().filter(|&(&(a, b), count)| merged.get(&(b, a)) != Some(count)).count();
        assert_eq!(open, 0, "edges with no triangle back the other way, of {}", merged.len());
        // Each chunk drawn at any of its levels, whatever level the next is drawn at, still closed, and coarser
        // levels fewer triangles.
        let meshes: Vec<ChunkMesh> = keys.iter().map(|key| mesh_chunk(&map, *key, &faces)).collect();
        let drawn = |level_of: &dyn Fn(usize) -> usize| {
            let mut edges = std::collections::HashMap::new();
            let mut triangles = 0;
            for (n, (key, mesh)) in keys.iter().zip(&meshes).enumerate() {
                let level = level_of(n);
                let words = &mesh.quads[mesh.buckets[level] as usize..mesh.buckets[level + 1] as usize];
                let corner = |pair: &[[u32; 2]]| {
                    let at = [pair[0][0] & 0xFFFF, pair[0][0] >> 16, pair[0][1] & 0xFFFF];
                    [0, 1, 2].map(|k| at[k] as i64 + i64::from(key[k]) * i64::from(CHUNK) * 1024)
                };
                for t in words.chunks(6) {
                    let [a, b, c] = [corner(&t[0..2]), corner(&t[2..4]), corner(&t[4..6])];
                    triangles += 1;
                    for (p, q) in [(a, b), (b, c), (c, a)] {
                        *edges.entry((p, q)).or_insert(0) += 1;
                    }
                }
            }
            let open = edges.iter().filter(|&(&(a, b), count)| edges.get(&(b, a)) != Some(count)).count();
            (open, triangles)
        };
        let counts: Vec<usize> = (0..LEVELS)
            .map(|level| {
                let (open, triangles) = drawn(&|_| level);
                assert_eq!(open, 0, "level {level} open");
                triangles
            })
            .collect();
        assert!(counts.windows(2).all(|pair| pair[1] < pair[0]), "{counts:?}");
        for mix in [&|n: usize| n % 2 * 2, &|n: usize| n % 3, &|n: usize| (n / 2) % 2] as [&dyn Fn(usize) -> usize; 3] {
            assert_eq!(drawn(mix).0, 0, "levels mixed");
        }
    }

    #[test]
    fn a_lake_over_two_chunks_is_merged_flat_and_meets_itself_edge_for_edge_where_they_meet() {
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::water(7)];
        // Ground 3.3 blocks up over two chunks along x, a lake on it up to 6 blocks across where they meet.
        let mut map = ground([64, 32], |_, _| 3.3);
        for y in 3..6 {
            for z in 6..26 {
                for x in 8..56 {
                    if map.get([x, y, z]) == AIR {
                        map.set([x, y, z], 2);
                    }
                }
            }
        }
        // Each chunk's water's triangles, by where their corners are in the world, in 1024ths of a block.
        let water = |key: [i32; 3]| -> Vec<[[i64; 3]; 3]> {
            let mesh = mesh_chunk(&map, key, &faces);
            let words = &mesh.quads[mesh.water().0 as usize..];
            let corner = |pair: &[[u32; 2]]| {
                let at = [pair[0][0] & 0xFFFF, pair[0][0] >> 16, pair[0][1] & 0xFFFF];
                [0, 1, 2].map(|k| at[k] as i64 + ((key[k] * CHUNK as i32) as f32 + POINT_FROM) as i64 * 1024)
            };
            words.chunks(6).map(|t| [corner(&t[0..2]), corner(&t[2..4]), corner(&t[4..6])]).collect()
        };
        let triangles: Vec<_> = water([0, 0, 0]).into_iter().chain(water([1, 0, 0])).collect();
        // Its flat top in far fewer triangles than two a block.
        let top = triangles.iter().filter(|t| t.iter().all(|p| p[1] == 6 * 1024)).count();
        assert!(top > 0 && top < 48 * 20 / 4, "{top} triangles over its top");
        // Away from its shore, every edge met by a triangle going back the other way, across the chunks' seam too.
        let mut edges = std::collections::HashMap::new();
        for t in &triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        let inside = |p: [i64; 3]| {
            p[1] > 5 * 1024 && (12 * 1024..52 * 1024).contains(&p[0]) && (10 * 1024..22 * 1024).contains(&p[2])
        };
        let open: Vec<_> =
            edges.keys().filter(|(a, b)| inside(*a) && inside(*b) && !edges.contains_key(&(*b, *a))).collect();
        assert!(open.is_empty(), "{open:?}");
        assert!(edges.keys().any(|(a, b)| a[0] == 32 * 1024 && b[0] == 32 * 1024), "edges on the seam");
    }
}
