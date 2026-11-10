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

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK},
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
/// those of facing `f`; and its faces as triangles, in blocks from the
/// chunk's corner, three indices a triangle, facing out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub quads: Vec<[u32; 2]>,
    pub buckets: [u32; FACINGS + 1],
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    /// Whether it has nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
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
pub fn mesh_chunk(map: &BlockMap, key: [i32; 3], faces: &[[u8; 3]]) -> ChunkMesh {
    let Some(chunk) = map.chunk(key) else { return ChunkMesh::default() };
    if chunk.is_air() || map.buried(key) {
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
    let solid = |p: [i32; 3]| block(p) != AIR;
    let surface = |id: BlockId, facing: usize| {
        let sides = faces.get(id as usize).copied().unwrap_or_default();
        match facing {
            2 => sides[0],
            3 => sides[2],
            _ => sides[1],
        }
    };
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
                    if id == AIR || solid(beyond) {
                        continue;
                    }
                    let look = surface(id, facing);
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
    mesh
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
        let faces = [[0, 0, 0], [1, 2, 3], [4, 4, 4]];
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
