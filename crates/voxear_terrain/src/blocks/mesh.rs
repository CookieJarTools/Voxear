//! A chunk's blocks as faces to draw: one where a block meets air, shaded
//! darker at each corner as the blocks round that corner close it in, and
//! faces side by side with the same surface and shading merged into one
//! quad (greedy meshing), so a flat field of blocks is a few quads, not
//! thousands.
//!
//! Each quad is two words, as `blockVertex` in `shaders/blocks.slang` reads
//! it: where it starts in its chunk, how many blocks it spans each way and
//! which way it faces; then its surface, its corners' shading, and the
//! chunk's number in its world's pool, which the renderer fills in. Quads
//! are kept by the way they face, so a chunk's faces turned away from the
//! camera are never drawn.
//!
//! Beside them, the same faces merged by surface alone, as triangles, for
//! collision and for the GI's rays, which need no shading.
//!
//! Water is seen through, so the ground's faces show where it meets water as
//! where it meets air, and water closes no corners in. Water's own faces show
//! only where it meets air, merged by surface, and are kept after the
//! ground's quads, every way they face together, to be drawn apart; they are
//! no triangles, so nothing stands on water and the GI's rays pass through.
//! In a world without end, none faces a chunk column not made (see
//! `stream`): the world goes on there, and a sea would show walls of water
//! where its rings of chunks meet.
//! Flowing water stands lower than its block (see `flow`): its top lower,
//! its sides only as high as it stands, and where it meets lower water, the
//! strip of its side between their tops. Such a quad keeps in place of its
//! corners' shading how far its corners on its block's lower and upper side
//! along the way up are moved, in sixteenths of a block, and in its first
//! word that way (`water_shape`).

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId, CHUNK, Fill},
    flow::Water,
    is_ground, is_water,
};

/// The ways a face can face, in the order quads are kept: +x, -x, +y, -y,
/// +z, -z.
pub const FACINGS: usize = 6;

/// For each facing, the axis it faces along, whether along it or against
/// it, and the two axes across its face, so that the first crossed with the
/// second faces out of it.
const AXES: [(usize, bool, usize, usize); FACINGS] =
    [(0, true, 1, 2), (0, false, 2, 1), (1, true, 2, 0), (1, false, 0, 2), (2, true, 0, 1), (2, false, 1, 0)];

/// A chunk's faces: its quads, kept by facing, `buckets[f]..buckets[f + 1]`
/// those of facing `f`, and after `buckets[FACINGS]` its water's; its
/// ground's faces as triangles, in blocks from the chunk's corner, three
/// indices a triangle, facing out; which of the chunk's sides see each
/// other through its air and water (see `sight`); and, drawn smooth, the
/// ways its triangles face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub quads: Vec<[u32; 2]>,
    pub buckets: [u32; FACINGS + 1],
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub sights: u16,
    /// For a chunk drawn smooth, the ways its triangles face, all within a
    /// cone: its axis, and the sine of how far from it the furthest turns.
    /// `None` where they face every way, or for a chunk of cubes, whose
    /// faces are kept by the way they face.
    pub cone: Option<[f32; 4]>,
}

impl ChunkMesh {
    /// Whether it has nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Its water's quads: where they start among its quads, and how many.
    pub fn water(&self) -> (u32, u32) {
        let first = self.buckets[FACINGS];
        (first, self.quads.len() as u32 - first)
    }
}

/// A quad as two words.
pub fn pack(at: [u32; 3], size: [u32; 2], facing: usize, surface: u8, shade: [u8; 4]) -> [u32; 2] {
    let first = at[0] | at[1] << 5 | at[2] << 10 | (size[0] - 1) << 15 | (size[1] - 1) << 20 | (facing as u32) << 25;
    let corners = shade.iter().enumerate().fold(0u32, |all, (i, &ao)| all | u32::from(ao & 3) << (2 * i));
    [first, u32::from(surface) | corners << 8]
}

/// A quad's place in its chunk, its size, facing, surface and corners'
/// shading, from its two words.
pub fn unpack(quad: [u32; 2]) -> ([u32; 3], [u32; 2], usize, u8, [u8; 4]) {
    let [first, second] = quad;
    let at = [first & 31, (first >> 5) & 31, (first >> 10) & 31];
    let size = [((first >> 15) & 31) + 1, ((first >> 20) & 31) + 1];
    let shade = std::array::from_fn(|i| ((second >> (8 + 2 * i)) & 3) as u8);
    (at, size, ((first >> 25) & 7) as usize, second as u8, shade)
}

/// How many blocks a side the padded blocks round a chunk are: the chunk
/// and a block of each neighbour.
const PADDED: usize = CHUNK as usize + 2;

/// Chunk `key`'s faces in `map`, each block's faces drawn with the
/// surfaces `faces` gives it (its top, sides and bottom), by its number.
pub fn mesh_chunk(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces]) -> ChunkMesh {
    mesh_made(map, key, faces, &|_| true)
}

/// Chunk `key`'s faces in `map`, as [`mesh_chunk`] makes them, where only
/// the chunk columns `made` picks, by place along x and z, are made: past
/// the others the world goes on, so its water shows no face towards them.
pub fn mesh_made(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces], made: &dyn Fn([i32; 2]) -> bool) -> ChunkMesh {
    let Some(chunk) = map.chunk(key) else { return ChunkMesh::default() };
    if chunk.is_air() || map.buried(key, faces) {
        return ChunkMesh::default();
    }
    // The chunk's blocks and those just past it, solid or not.
    let mut padded = vec![AIR; PADDED * PADDED * PADDED];
    let base = key.map(|v| v * CHUNK as i32 - 1);
    for y in 0..PADDED {
        for z in 0..PADDED {
            for x in 0..PADDED {
                let inside = [x, y, z].iter().all(|&v| (1..=CHUNK as usize).contains(&v));
                padded[(y * PADDED + z) * PADDED + x] = if inside {
                    chunk.get(x as u32 - 1, y as u32 - 1, z as u32 - 1)
                } else {
                    map.get([base[0] + x as i32, base[1] + y as i32, base[2] + z as i32])
                };
            }
        }
    }
    let block =
        |p: [i32; 3]| padded[((p[1] + 1) as usize * PADDED + (p[2] + 1) as usize) * PADDED + (p[0] + 1) as usize];
    let solid = |p: [i32; 3]| is_ground(faces, block(p));
    let side = CHUNK as usize;
    let mut mesh = ChunkMesh::default();
    let mut shaded = vec![0u32; side * side];
    let mut plain = vec![0u32; side * side];
    for (facing, &(axis, out, u, v)) in AXES.iter().enumerate() {
        mesh.buckets[facing] = mesh.quads.len() as u32;
        let step = if out { 1 } else { -1 };
        for layer in 0..side {
            shaded.fill(0);
            plain.fill(0);
            for j in 0..side {
                for i in 0..side {
                    let mut p = [0i32; 3];
                    (p[axis], p[u], p[v]) = (layer as i32, i as i32, j as i32);
                    let id = block(p);
                    let mut beyond = p;
                    beyond[axis] += step;
                    if !is_ground(faces, id) || solid(beyond) {
                        continue;
                    }
                    let look = surface(faces, id, facing);
                    // Each corner, as the blocks beside it in the air before the face close it in.
                    let shade = [(-1, -1), (1, -1), (1, 1), (-1, 1)].map(|(su, sv)| {
                        let at = |du: i32, dv: i32| {
                            let mut q = beyond;
                            (q[u], q[v]) = (q[u] + du, q[v] + dv);
                            u8::from(solid(q))
                        };
                        let (one, two, corner) = (at(su, 0), at(0, sv), at(su, sv));
                        if one == 1 && two == 1 { 0 } else { 3 - one - two - corner }
                    });
                    let key = pack([0; 3], [1, 1], 0, look, shade)[1];
                    shaded[j * side + i] = key + 1;
                    plain[j * side + i] = u32::from(look) + 1;
                }
            }
            // Merged by surface and shading, to draw.
            for (i, j, w, h, key) in merge(&mut shaded, side) {
                let mut at = [0u32; 3];
                (at[axis], at[u], at[v]) = (layer as u32, i as u32, j as u32);
                let (look, shade) = (key as u8, std::array::from_fn(|c| ((key >> (8 + 2 * c)) & 3) as u8));
                mesh.quads.push(pack(at, [w as u32, h as u32], facing, look, shade));
            }
            // Merged by surface alone, as triangles.
            for (i, j, w, h, _) in merge(&mut plain, side) {
                let mut corner = [0.0f32; 3];
                let face = layer as f32 + if out { 1.0 } else { 0.0 };
                (corner[axis], corner[u], corner[v]) = (face, i as f32, j as f32);
                let first = mesh.positions.len() as u32;
                for (du, dv) in [(0, 0), (w, 0), (w, h), (0, h)] {
                    let mut point = corner;
                    (point[u], point[v]) = (point[u] + du as f32, point[v] + dv as f32);
                    mesh.positions.push(point);
                }
                mesh.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
            }
        }
    }
    mesh.buckets[FACINGS] = mesh.quads.len() as u32;
    if chunk.may_hold(|block| is_water(faces, block)) {
        let made = held(key, made);
        mesh.quads.extend(water_quads((&block, &fills(map, key)), faces, |at, beyond| beyond == AIR && made(at)));
    }
    mesh.sights = super::sight::sights(chunk, |block| !is_ground(faces, block));
    mesh
}

/// The surface block `id` is drawn with on its faces facing `facing`.
fn surface(faces: &[BlockFaces], id: BlockId, facing: usize) -> u8 {
    let kind = faces.get(id as usize).copied().unwrap_or_default();
    match facing {
        2 => kind.top,
        3 => kind.bottom,
        _ => kind.sides,
    }
}

/// How full each block round chunk `key` is (see `chunk::Fill`), by where
/// it is from the chunk's corner.
pub(super) fn fills(map: &BlockMap, key: [i32; 3]) -> impl Fn([i32; 3]) -> Fill + '_ {
    let chunk = map.chunk(key);
    let corner = key.map(|v| v * CHUNK as i32);
    move |p: [i32; 3]| match chunk {
        Some(chunk) if p.iter().all(|v| (0..CHUNK as i32).contains(v)) => {
            chunk.fill(p[0] as u32, p[1] as u32, p[2] as u32)
        }
        _ => map.fill(std::array::from_fn(|i| corner[i] + p[i])),
    }
}

/// Whether the block at `at` from chunk `key`'s corner is in a chunk
/// column `made` picks, or in the chunk's own.
pub(super) fn held<'a>(key: [i32; 3], made: &'a dyn Fn([i32; 2]) -> bool) -> impl Fn([i32; 3]) -> bool + 'a {
    let side = CHUNK as i32;
    move |at: [i32; 3]| {
        let column = [at[0].div_euclid(side), at[2].div_euclid(side)];
        column == [0, 0] || made([key[0] + column[0], key[2] + column[1]])
    }
}

/// A water quad's shape: how far its corners on its block's lower side
/// along the way up are raised and those on its upper side lowered, in
/// sixteenths of a block, and the way up (a facing), where any is moved.
pub fn water_shape(quad: [u32; 2]) -> (u8, u8, Option<usize>) {
    let up = (quad[0] >> 28) & 7;
    match up < 6 {
        true => (((quad[1] >> 12) & 15) as u8, ((quad[1] >> 8) & 15) as u8, Some(up as usize)),
        false => (0, 0, None),
    }
}

/// How the water of a quad of a river's top flows, in metres a second along
/// x and z, where it flows along a current (see `flow::current_fill`): which
/// way kept, one more than its number, where a quad's shape keeps how far
/// its lower corners are raised, which a top has no use for.
pub fn water_current(quad: [u32; 2]) -> Option<[f32; 2]> {
    let (up, facing) = ((quad[0] >> 28) & 7, (quad[0] >> 25) & 7);
    let way = ((quad[1] >> 12) & 15) as usize;
    let top = facing == 2 && (up == 7 || up == super::flow::UP as u32);
    let way = super::flow::WAYS.get(way.checked_sub(1).filter(|_| top)?)?;
    Some(way.map(|v| v * super::flow::RIVER_SPEED))
}

/// A chunk's water's faces as quads, merged by surface and shape: one
/// wherever a block of water meets a block `meets` picks, by where it is
/// from the chunk's corner and which block, as high as it stands; and where
/// it meets lower water across the way up, the strip of its side above that
/// water's top. `blocks` gives each block, and how full it is, from one
/// before the chunk's corner to one past its far side.
pub(super) fn water_quads(
    (block, fill): (&dyn Fn([i32; 3]) -> BlockId, &dyn Fn([i32; 3]) -> Fill),
    faces: &[BlockFaces],
    meets: impl Fn([i32; 3], BlockId) -> bool,
) -> Vec<[u32; 2]> {
    let side = CHUNK as usize;
    let mut quads = Vec::new();
    let mut mask = vec![0u32; side * side];
    let water = |p: [i32; 3]| is_water(faces, block(p)).then(|| super::flow::water_height(fill(p)));
    for (facing, &(axis, out, u, v)) in AXES.iter().enumerate() {
        for layer in 0..side {
            for j in 0..side {
                for i in 0..side {
                    let mut p = [0i32; 3];
                    (p[axis], p[u], p[v]) = (layer as i32, i as i32, j as i32);
                    let Some((height, up)) = water(p) else { continue };
                    let mut beyond = p;
                    beyond[axis] += if out { 1 } else { -1 };
                    // How far its corners on its lower side along the way up are raised and on its upper side lowered.
                    let shape = match water(beyond) {
                        Some((lower, its)) => match up.or(its) {
                            Some(up) if up / 2 != axis && lower < height => Some((lower, 16 - height, Some(up))),
                            _ => None,
                        },
                        None if meets(beyond, block(beyond)) => match up {
                            Some(up) if facing == up ^ 1 => Some((0, 0, None)),
                            Some(up) => Some((0, 16 - height, Some(up))),
                            None => Some((0, 0, None)),
                        },
                        None => None,
                    };
                    let Some((rise, drop, up)) = shape else { continue };
                    let look = u32::from(surface(faces, block(p), facing));
                    let up = up.filter(|_| rise > 0 || drop > 0);
                    // A side moved at its top or foot is a block high: merged across the way up, it would leave gaps.
                    let across = match up {
                        Some(up) if up / 2 != axis => (p[up / 2] + 1) as u32,
                        _ => 0,
                    };
                    // A river's top keeps which way it flows where its lower corners would be raised, which a top's
                    // never are.
                    let rise = match (facing, super::flow::current_parts(fill(p))) {
                        (2, Some((way, _))) => way as u8 + 1,
                        _ => rise,
                    };
                    let shaped = u32::from(rise) << 12 | u32::from(drop) << 8 | (up.unwrap_or(7) as u32) << 16;
                    mask[j * side + i] = (look | shaped | across << 19) + 1;
                }
            }
            for (i, j, w, h, key) in merge(&mut mask, side) {
                let mut at = [0u32; 3];
                (at[axis], at[u], at[v]) = (layer as u32, i as u32, j as u32);
                let [first, _] = pack(at, [w as u32, h as u32], facing, key as u8, [0; 4]);
                quads.push([first | ((key >> 16) & 7) << 28, key & 0xFFFF]);
            }
        }
    }
    quads
}

/// The rectangles of equal values in a `side` by `side` mask, row after
/// row, each as wide and then as tall as it goes: where each starts, its
/// width and height, and its value less one. The mask is left empty.
fn merge(mask: &mut [u32], side: usize) -> Vec<(usize, usize, usize, usize, u32)> {
    let mut found = Vec::new();
    for j in 0..side {
        let mut i = 0;
        while i < side {
            let key = mask[j * side + i];
            if key == 0 {
                i += 1;
                continue;
            }
            let width = (i..side).take_while(|&x| mask[j * side + x] == key).count();
            let height = (j..side).take_while(|&y| (i..i + width).all(|x| mask[y * side + x] == key)).count();
            for y in j..j + height {
                mask[y * side + i..y * side + i + width].fill(0);
            }
            found.push((i, j, width, height, key - 1));
            i += width;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::chunk::Chunk;

    /// Faces counted one a block: each side of a solid block that meets air.
    fn naive_faces(map: &BlockMap, key: [i32; 3]) -> usize {
        let base = key.map(|v| v * CHUNK as i32);
        let mut count = 0;
        for y in 0..CHUNK as i32 {
            for z in 0..CHUNK as i32 {
                for x in 0..CHUNK as i32 {
                    let p = [base[0] + x, base[1] + y, base[2] + z];
                    if map.get(p) == AIR {
                        continue;
                    }
                    for &(axis, out, _, _) in &AXES {
                        let mut q = p;
                        q[axis] += if out { 1 } else { -1 };
                        count += usize::from(map.get(q) == AIR);
                    }
                }
            }
        }
        count
    }

    #[test]
    fn water_shows_its_faces_where_it_meets_air_apart_and_the_ground_shows_through_it() {
        // A floor of ground, a pool of water two deep on it, 4 blocks a side, a pillar of ground standing in it.
        let mut map = BlockMap::default();
        for z in 0..8 {
            for x in 0..8 {
                map.set([x, 0, z], 1);
            }
        }
        for y in 1..3 {
            for z in 2..6 {
                for x in 2..6 {
                    map.set([x, y, z], 2);
                }
            }
        }
        for y in 1..4 {
            map.set([3, y, 3], 1);
        }
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::water(5)];
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        let (first, count) = mesh.water();
        assert_eq!(first, mesh.buckets[FACINGS]);
        let water: Vec<_> = mesh.quads[first as usize..].iter().map(|&q| unpack(q)).collect();
        assert_eq!(water.len(), count as usize);
        // Its top and four sides, where it meets air, and nothing against the floor or the pillar.
        let area = |facing: usize| -> u32 {
            water.iter().filter(|q| q.2 == facing).map(|q| q.1.iter().product::<u32>()).sum()
        };
        assert_eq!(area(2), 16 - 1, "its top, but where the pillar stands out of it");
        assert_eq!([0, 1, 4, 5].map(area), [8; 4], "each side 4 wide and 2 high");
        assert_eq!(area(3), 0, "none down onto the floor");
        assert!(water.iter().all(|q| q.3 == 5), "drawn with its surface");
        // The ground's faces show under the water and round the pillar in it, and the water closes no corner in.
        let ground: Vec<_> = mesh.quads[..first as usize].iter().map(|&q| unpack(q)).collect();
        let floor: u32 = ground.iter().filter(|q| q.2 == 2 && q.0[1] == 0).map(|q| q.1.iter().product::<u32>()).sum();
        assert_eq!(floor, 64 - 1, "the whole floor's top but under the pillar");
        let sides_in_water = ground.iter().filter(|q| q.0 == [3, 1, 3] || q.0 == [4, 1, 3]).count();
        assert!(sides_in_water > 0, "the pillar's sides under the water");
        assert!(ground.iter().filter(|q| q.2 == 2 && q.0[1] == 0).all(|q| q.4 == [3; 4] || q.0[0] >= 2));
        // Nothing stands on water: its faces are no triangles.
        let tops = mesh.indices.chunks(3).filter(|t| t.iter().all(|&i| mesh.positions[i as usize][1] == 3.0));
        assert_eq!(tops.count(), 0, "no triangles at the water's top");
        // A chunk of ground beside a chunk of water shows its faces against it, though both are whole.
        let mut sea = BlockMap::default();
        for key in [[-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1], [0, 0, 0]] {
            sea.put_chunk(key, Chunk::filled(1));
        }
        sea.put_chunk([1, 0, 0], Chunk::filled(2));
        let shore = mesh_chunk(&sea, [0, 0, 0], &faces);
        assert_eq!(shore.buckets[1] - shore.buckets[0], 1, "one quad facing the water");
        assert_eq!(shore.water().1, 0);
    }

    #[test]
    fn flowing_water_stands_as_high_as_it_flows_and_shows_its_side_over_lower_water() {
        // On a floor, a source and then water run out from it, seven and six eighths of a block high.
        let mut map = BlockMap::default();
        for x in 0..8 {
            map.set([x, 0, 1], 1);
        }
        map.set([2, 1, 1], 2);
        map.set_cell([3, 1, 1], 2, Water::Flowing { level: 7, up: 2 }.fill());
        map.set_cell([4, 1, 1], 2, Water::Flowing { level: 6, up: 2 }.fill());
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::water(5)];
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        let (first, _) = mesh.water();
        let water = &mesh.quads[first as usize..];
        let shape = |x: u32, facing: usize| {
            let found = water.iter().filter(|&&q| unpack(q).0[0] == x && unpack(q).2 == facing).collect::<Vec<_>>();
            assert_eq!(found.len(), 1, "one face at {x} facing {facing}");
            water_shape(*found[0])
        };
        // Their tops lowered by what they lack; the source's whole.
        assert_eq!([2, 3, 4].map(|x| shape(x, 2)), [(0, 0, None), (0, 2, Some(2)), (0, 4, Some(2))]);
        // Each side towards lower water a strip from its top down to that water's; the last's down to its foot.
        assert_eq!([2, 3, 4].map(|x| shape(x, 0)), [(14, 0, Some(2)), (12, 2, Some(2)), (0, 4, Some(2))]);
        // Its sides along z each a block high, their tops lowered as they stand.
        assert_eq!(shape(3, 4), (0, 2, Some(2)));
        assert_eq!(unpack(*water.iter().find(|&&q| unpack(q).0 == [3, 1, 1] && unpack(q).2 == 4).unwrap()).1, [1, 1]);
    }

    #[test]
    fn a_quad_keeps_what_it_was_packed_with() {
        let quad = pack([31, 0, 17], [32, 5], 4, 200, [3, 0, 2, 1]);
        assert_eq!(unpack(quad), ([31, 0, 17], [32, 5], 4, 200, [3, 0, 2, 1]));
        assert_eq!(quad[1] >> 16, 0, "room for the chunk's number in the pool");
    }

    #[test]
    fn a_flat_field_is_a_few_quads_covering_every_face_a_block_at_a_time_would() {
        let mut map = BlockMap::default();
        // A floor a block thick over the chunk, a pillar of another block on it, and a block hanging past its edge.
        for z in 0..32 {
            for x in 0..32 {
                map.set([x, 0, z], 1);
            }
        }
        for y in 1..5 {
            map.set([10, y, 12], 2);
        }
        map.set([32, 0, 3], 1);
        let faces = [BlockFaces::default(), BlockFaces::new(1, 2, 3), BlockFaces::new(4, 4, 4)];
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        // Every face a block at a time would draw, and no more, but in far fewer quads.
        let area: u32 = mesh.quads.iter().map(|&q| unpack(q).1.iter().product::<u32>()).sum();
        assert_eq!(area as usize, naive_faces(&map, [0, 0, 0]));
        assert!(mesh.quads.len() < 80, "{} quads", mesh.quads.len());
        // The floor's top: grass, all of it but round the pillar, darker at the pillar's foot.
        let tops: Vec<_> = (mesh.buckets[2]..mesh.buckets[3]).map(|i| unpack(mesh.quads[i as usize])).collect();
        assert!(tops.iter().filter(|q| q.0[1] == 0).all(|q| q.3 == 1));
        assert!(tops.iter().any(|q| q.0[1] == 0 && q.4.contains(&2)), "shaded where the pillar stands");
        assert!(tops.iter().any(|q| q.0[1] == 4 && q.3 == 4), "the pillar's top");
        // Its bottom and sides drawn with their own surfaces; no face where the next chunk's block meets it.
        let bottoms = (mesh.buckets[3]..mesh.buckets[4]).map(|i| unpack(mesh.quads[i as usize]).3);
        assert!(bottoms.into_iter().all(|surface| surface == 3));
        let east = (mesh.buckets[0]..mesh.buckets[1]).map(|i| unpack(mesh.quads[i as usize]));
        assert!(east.into_iter().filter(|q| q.0[0] == 31).all(|q| q.0[2] != 3), "the block beyond hides that face");
        // The triangles cover the same faces, facing out.
        assert_eq!(mesh.indices.len() % 6, 0);
        let area: f32 = mesh
            .indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| voxear_core::math::Vec3::from(mesh.positions[t[i] as usize]));
                (b - a).cross(c - a).length() * 0.5
            })
            .sum();
        assert!((area - naive_faces(&map, [0, 0, 0]) as f32).abs() < 1e-3);
        let up = mesh.indices.chunks(3).find(|t| t.iter().all(|&i| mesh.positions[i as usize][1] == 1.0)).unwrap();
        let [a, b, c] = [0, 1, 2].map(|i| voxear_core::math::Vec3::from(mesh.positions[up[i] as usize]));
        assert!((b - a).cross(c - a).y > 0.0, "the floor's top faces up");
        // A chunk of air has nothing; one of stone deep underground, nothing either.
        assert!(mesh_chunk(&map, [5, 5, 5], &faces).is_empty());
        let mut buried = BlockMap::default();
        for key in [[-1, -1, -1], [0, -1, -1], [-1, 0, -1], [-1, -1, 0], [-2, -1, -1], [-1, -2, -1], [-1, -1, -2]] {
            buried.put_chunk(key, Chunk::filled(1));
        }
        assert!(mesh_chunk(&buried, [-1, -1, -1], &faces).is_empty());
    }
}
