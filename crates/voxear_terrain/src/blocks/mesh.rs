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
//! A block seen through, as leaves are (`BlockFaces::see_through`), hides
//! nothing: a face shows where its block meets one, and its own faces show
//! where it meets air, water or another block seen through, its own kind
//! too, so a crown of leaves is leaves all the way in. It is ground in
//! every other way: it closes corners in, and is stood on.
//!
//! A block that is bushy (`block::BlockShape::Bushy`) keeps its cube and has
//! three sheets of leaves besides, crossed through its middle and reaching
//! out past it, wherever it meets open air; a vine is a sheet against each
//! block of ground beside it, or two crossed sheets where it hangs free.
//! These are the chunk's loose quads ([`ChunkMesh::loose`]), kept after its
//! faces and before its water, seen from both sides and drawn whichever way
//! the camera looks. A loose quad's facing is [`TUFT`] or [`VINE`], and
//! where a face has its size it says which sheet it is ([`loose_sheet`]).
//! They are no triangles: nothing stands on them, and rays pass.
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
    hides, is_bushy, is_ground, is_vine, is_water, sways,
};

/// The ways a face can face, in the order quads are kept: +x, -x, +y, -y,
/// +z, -z.
pub const FACINGS: usize = 6;

/// For each facing, the axis it faces along, whether along it or against
/// it, and the two axes across its face, so that the first crossed with the
/// second faces out of it.
const AXES: [(usize, bool, usize, usize); FACINGS] =
    [(0, true, 1, 2), (0, false, 2, 1), (1, true, 2, 0), (1, false, 0, 2), (2, true, 0, 1), (2, false, 1, 0)];

/// The facing a loose quad has in place of a way it faces: a sheet of a
/// bushy block's leaves, or a sheet of a vine.
pub const TUFT: usize = 6;
pub const VINE: usize = 7;

/// The sheets of leaves a bushy block has, each by the way it runs across
/// and the way it runs up, from the block's middle, as `blockTuft` in
/// `shaders/block_quads.slang` has them: two upright ones from corner to
/// corner, and four that lean. A block has both upright ones and one that
/// leans.
pub const TUFT_SHEETS: [([i32; 3], [i32; 3]); 6] = [
    ([1, 0, 1], [0, 1, 0]),
    ([1, 0, -1], [0, 1, 0]),
    ([1, 0, 0], [0, 1, 1]),
    ([1, 0, 0], [0, 1, -1]),
    ([0, 0, 1], [1, 1, 0]),
    ([0, 0, 1], [-1, 1, 0]),
];

/// A vine's sheets: against the block beside it along +x, -x, +z and -z,
/// and the two it hangs free as, from corner to corner.
pub const VINE_SHEETS: u32 = 6;

/// A vine's sheet at the foot of its vine: no vine hangs on under it.
pub const VINE_TIP: u32 = 8;

/// A chunk's faces: its quads, kept by facing, `buckets[f]..buckets[f + 1]`
/// those of facing `f`, then its loose quads from `buckets[FACINGS]`, and
/// after `buckets[FACINGS + 1]` its water's; its
/// ground's faces as triangles, in blocks from the chunk's corner, three
/// indices a triangle, facing out; which of the chunk's sides see each
/// other through its air and water (see `sight`); and, drawn smooth, the
/// ways its triangles face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub quads: Vec<[u32; 2]>,
    pub buckets: [u32; FACINGS + 2],
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
        let first = self.buckets[FACINGS + 1];
        (first, self.quads.len() as u32 - first)
    }

    /// Its loose quads, the sheets of bushy leaves and vines: where they
    /// start among its quads, and how many.
    pub fn loose(&self) -> (u32, u32) {
        (self.buckets[FACINGS], self.buckets[FACINGS + 1] - self.buckets[FACINGS])
    }
}

/// A loose quad: a sheet of the block at `at` in its chunk, `sheet` of its
/// kind's (with [`VINE_TIP`] for a vine's foot), drawn with `surface`, as
/// open or closed in as `open` says from 0 to 3, its corners that sway a
/// bit each in `free`.
pub fn pack_loose(at: [u32; 3], kind: usize, sheet: u32, surface: u8, open: u8, free: u32) -> [u32; 2] {
    let first = at[0] | at[1] << 5 | at[2] << 10 | (sheet & 1023) << 15 | (kind as u32) << 25 | (free & 15) << 28;
    [first, u32::from(surface) | u32::from(open & 3) << 8]
}

/// Which sheet a loose quad is, where a face has its size: for a tuft, its
/// place in [`TUFT_SHEETS`]; for a vine, the side it clings to or the way
/// it hangs, with [`VINE_TIP`] at the foot of its vine.
pub fn loose_sheet(quad: [u32; 2]) -> u32 {
    (quad[0] >> 15) & 1023
}

/// A number for a block of the world, from whole numbers alone: which of
/// its sheets of leaves leans, the same wherever its chunk is made.
fn block_number([x, y, z]: [i32; 3]) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// A quad as two words.
pub fn pack(at: [u32; 3], size: [u32; 2], facing: usize, surface: u8, shade: [u8; 4]) -> [u32; 2] {
    let first = at[0] | at[1] << 5 | at[2] << 10 | (size[0] - 1) << 15 | (size[1] - 1) << 20 | (facing as u32) << 25;
    let corners = shade.iter().enumerate().fold(0u32, |all, (i, &ao)| all | u32::from(ao & 3) << (2 * i));
    [first, u32::from(surface) | corners << 8]
}

/// Which of a quad's corners sway in the wind, a bit each from its first
/// corner's: those of a block that sways (`BlockFaces::sways`) which no
/// still block touches. Such a quad is one face of one block, never
/// merged, so every face that meets at a corner has that corner, and all
/// of them move it alike: nothing parts as leaves sway, and leaves stay
/// fixed to the wood and the ground they touch.
pub fn swaying_corners(quad: [u32; 2]) -> u8 {
    (quad[0] >> 28) as u8
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
    let hidden_by = |p: [i32; 3]| hides(faces, block(p));
    // Ground that stands still holds every corner it touches.
    let any_sway = faces.iter().any(|faces| faces.sways);
    let still = |p: [i32; 3]| is_ground(faces, block(p)) && !sways(faces, block(p));
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
                    if !is_ground(faces, id) || hidden_by(beyond) {
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
                    let mut key = pack([0; 3], [1, 1], 0, look, shade)[1];
                    if any_sway && sways(faces, id) {
                        // Its corners that only air, water and other swaying blocks meet at are free to move.
                        let mut free = 0u32;
                        for (c, (cu, cv)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                            let mut corner = p;
                            (corner[axis], corner[u], corner[v]) = (p[axis] + i32::from(out), p[u] + cu, p[v] + cv);
                            let [x, y, z] = corner;
                            let held = (0..8).any(|n| still([x - (n & 1), y - (n >> 1 & 1), z - (n >> 2)]));
                            free |= u32::from(!held) << c;
                        }
                        // A face with a corner that moves is kept apart, by its own place in the layer.
                        if free != 0 {
                            key |= free << 16 | ((j * side + i) as u32 + 1) << 20;
                        }
                    }
                    shaded[j * side + i] = key + 1;
                    plain[j * side + i] = u32::from(look) + 1;
                }
            }
            // Merged by surface and shading, to draw.
            for (i, j, w, h, key) in merge(&mut shaded, side) {
                let mut at = [0u32; 3];
                (at[axis], at[u], at[v]) = (layer as u32, i as u32, j as u32);
                let (look, shade) = (key as u8, std::array::from_fn(|c| ((key >> (8 + 2 * c)) & 3) as u8));
                let [first, second] = pack(at, [w as u32, h as u32], facing, look, shade);
                mesh.quads.push([first | ((key >> 16) & 15) << 28, second]);
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
    let loose = |faces: &BlockFaces| !faces.shape.is_cube() && !faces.water;
    if faces.iter().any(loose) && chunk.may_hold(|id| faces.get(usize::from(id)).is_some_and(loose)) {
        let corner = key.map(|v| v * CHUNK as i32);
        // A corner of a sheet sways unless a still block touches the corner of its block's cube nearest it.
        let held = |[x, y, z]: [i32; 3]| (0..8).any(|n| still([x - (n & 1), y - (n >> 1 & 1), z - (n >> 2)]));
        let beside = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
        for y in 0..side as i32 {
            for z in 0..side as i32 {
                for x in 0..side as i32 {
                    let p = [x, y, z];
                    let id = block(p);
                    let at = p.map(|v| v as u32);
                    let step = |by: [i32; 3]| [x + by[0], y + by[1], z + by[2]];
                    if is_bushy(faces, id) {
                        // Only where it meets open air: one deep in a crown shows none of its tufts.
                        let around = beside.iter().chain(&[[0, 1, 0], [0, -1, 0]]);
                        let open = around.filter(|by| !is_ground(faces, block(step(**by)))).count();
                        if open == 0 {
                            continue;
                        }
                        let open = [0, 0, 1, 2, 2, 3, 3][open];
                        let number = block_number([0, 1, 2].map(|k| corner[k] + p[k]));
                        for sheet in [0, 1, 2 + (number % 4) as usize] {
                            let (across, up) = TUFT_SHEETS[sheet];
                            let mut free = 0;
                            if sways(faces, id) {
                                for (c, (sa, sb)) in [(-1, -1), (1, -1), (1, 1), (-1, 1)].into_iter().enumerate() {
                                    let nearest = [0, 1, 2].map(|k| p[k] + i32::from(sa * across[k] + sb * up[k] > 0));
                                    free |= u32::from(!held(nearest)) << c;
                                }
                            }
                            let look = surface(faces, id, 0);
                            mesh.quads.push(pack_loose(at, TUFT, sheet as u32, look, open, free));
                        }
                    } else if is_vine(faces, id) {
                        let look = surface(faces, id, 0);
                        let tip = if is_vine(faces, block(step([0, -1, 0]))) { 0 } else { VINE_TIP };
                        let clings: Vec<u32> =
                            (0..4).filter(|&s| is_ground(faces, block(step(beside[s as usize])))).collect();
                        for &sheet in &clings {
                            mesh.quads.push(pack_loose(at, VINE, sheet | tip, look, 3, 0));
                        }
                        if clings.is_empty() {
                            // Hanging free, its foot and its head sway unless ground holds them.
                            let (under, over) = (is_ground(faces, block(step([0, -1, 0]))), solid(step([0, 1, 0])));
                            let free = match sways(faces, id) {
                                true => u32::from(!under) * 0b0011 | u32::from(!over) * 0b1100,
                                false => 0,
                            };
                            for sheet in 4..VINE_SHEETS {
                                mesh.quads.push(pack_loose(at, VINE, sheet | tip, look, 3, free));
                            }
                        }
                    }
                }
            }
        }
    }
    mesh.buckets[FACINGS + 1] = mesh.quads.len() as u32;
    if chunk.may_hold(|block| is_water(faces, block)) {
        let made = held(key, made);
        let open = |beyond: BlockId| beyond == AIR || is_vine(faces, beyond);
        mesh.quads.extend(water_quads((&block, &fills(map, key)), faces, |at, beyond| open(beyond) && made(at)));
    }
    mesh.sights = super::sight::sights(chunk, |block| !hides(faces, block));
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
    (((quad[1] >> 12) & 15) as u8, ((quad[1] >> 8) & 15) as u8, (up < 6).then_some(up as usize))
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
    let water = |p: [i32; 3]| is_water(faces, block(p)).then(|| Water::of(fill(p)).height());
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
    fn leaves_that_sway_keep_a_face_a_block_and_move_only_corners_no_still_block_touches() {
        // A floor, a trunk two high on it, and leaves over and round its top: three by three, two high.
        let mut map = BlockMap::default();
        for z in 0..8 {
            for x in 0..8 {
                map.set([x, 0, z], 1);
            }
        }
        map.set([4, 1, 4], 2);
        map.set([4, 2, 4], 2);
        for y in 3..5 {
            for z in 3..6 {
                for x in 3..6 {
                    map.set([x, y, z], 3);
                }
            }
        }
        let leaves = BlockFaces { sways: true, ..BlockFaces::new(3, 3, 3) };
        let ground = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::new(2, 2, 2)];
        let still = [ground[0], ground[1], ground[2], BlockFaces::new(3, 3, 3)];
        let swaying = [still[0], still[1], still[2], leaves];
        let (plain, mesh) = (mesh_chunk(&map, [0, 0, 0], &still), mesh_chunk(&map, [0, 0, 0], &swaying));
        assert!(plain.quads.iter().all(|&quad| swaying_corners(quad) == 0), "nothing sways unless its block does");
        let of_leaves = |mesh: &ChunkMesh| mesh.quads.iter().copied().filter(|&q| unpack(q).3 == 3).collect::<Vec<_>>();
        // Still, its crown's faces are merged into a few quads. Swaying, each stays a block's: nine on top, eight
        // under, where the trunk hides one, and six on each of its four sides.
        assert!(of_leaves(&plain).len() < 16, "{} quads", of_leaves(&plain).len());
        let quads = of_leaves(&mesh);
        assert!(quads.iter().all(|&quad| unpack(quad).1 == [1, 1]), "a face a block");
        assert_eq!(quads.len(), 9 + 8 + 4 * 6);
        // Where each corner of a quad is, in the chunk's blocks.
        let corners = |quad: [u32; 2]| -> [[u32; 3]; 4] {
            let (at, _, facing, ..) = unpack(quad);
            let (axis, out, u, v) = AXES[facing];
            [(0, 0), (1, 0), (1, 1), (0, 1)].map(|(cu, cv)| {
                let mut corner = at;
                (corner[axis], corner[u], corner[v]) = (at[axis] + u32::from(out), at[u] + cu, at[v] + cv);
                corner
            })
        };
        let mut moved = std::collections::HashMap::new();
        for &quad in &quads {
            for (c, corner) in corners(quad).into_iter().enumerate() {
                let sways = swaying_corners(quad) >> c & 1 == 1;
                // Every face that meets at a corner says the same of it.
                assert_eq!(*moved.entry(corner).or_insert(sways), sways, "corner {corner:?}");
            }
        }
        // The four corners of the trunk's top are held, and no other: the leaves' other corners are in the open.
        let held: Vec<_> = moved.iter().filter(|(_, sways)| !**sways).map(|(corner, _)| *corner).collect();
        assert_eq!(held.len(), 4, "{held:?}");
        assert!(held.iter().all(|c| c[1] == 3 && (4..=5).contains(&c[0]) && (4..=5).contains(&c[2])), "{held:?}");
        // The ground and the wood are as they were.
        let rest = |mesh: &ChunkMesh| mesh.quads.iter().copied().filter(|&q| unpack(q).3 != 3).collect::<Vec<_>>();
        assert_eq!(rest(&plain), rest(&mesh));
    }

    #[test]
    fn bushy_leaves_have_three_sheets_a_block_where_they_meet_air_and_their_corners_sway_with_the_blocks() {
        use crate::blocks::block::BlockShape;
        // A floor, a trunk two high on it, and leaves over and round its top: three by three, three high.
        let mut map = BlockMap::default();
        for z in 0..8 {
            for x in 0..8 {
                map.set([x, 0, z], 1);
            }
        }
        map.set([4, 1, 4], 2);
        map.set([4, 2, 4], 2);
        for y in 3..6 {
            for z in 3..6 {
                for x in 3..6 {
                    map.set([x, y, z], 3);
                }
            }
        }
        let ground = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::new(2, 2, 2)];
        let leaves = BlockFaces { sways: true, ..BlockFaces::new(3, 3, 3) };
        let bushy = BlockFaces { shape: BlockShape::Bushy, ..leaves };
        let plain = mesh_chunk(&map, [0, 0, 0], &[ground[0], ground[1], ground[2], leaves]);
        let mesh = mesh_chunk(&map, [0, 0, 0], &[ground[0], ground[1], ground[2], bushy]);
        assert_eq!(plain.loose(), (plain.buckets[FACINGS], 0), "cubes have no sheets");
        // Its faces and its triangles are a cube's: the sheets come after, and before the water.
        let faces = |mesh: &ChunkMesh| mesh.quads[..mesh.buckets[FACINGS] as usize].to_vec();
        assert_eq!(faces(&mesh), faces(&plain));
        assert_eq!((&mesh.positions, &mesh.indices), (&plain.positions, &plain.indices), "nothing stands on a sheet");
        let (first, count) = mesh.loose();
        assert_eq!((first + count, mesh.water().1), (mesh.quads.len() as u32, 0));
        // Every block of the crown meets air but the one in its middle and the one on the trunk: three sheets each.
        assert_eq!(count, 25 * 3);
        let sheets = &mesh.quads[first as usize..];
        assert!(sheets.iter().all(|&quad| unpack(quad).2 == TUFT && unpack(quad).3 == 3));
        let closed_in = |at: [u32; 3]| at == [4, 4, 4] || at == [4, 3, 4];
        assert!(sheets.iter().all(|&quad| !closed_in(unpack(quad).0)), "none for a block closed in");
        for block in sheets.chunks(3) {
            let which: Vec<u32> = block.iter().map(|&quad| loose_sheet(quad)).collect();
            assert!(which[0] == 0 && which[1] == 1 && (2..6).contains(&which[2]), "{which:?}");
            assert!(block.iter().all(|&quad| unpack(quad).0 == unpack(block[0]).0));
        }
        let leaning: std::collections::HashSet<u32> = sheets.chunks(3).map(|block| loose_sheet(block[2])).collect();
        assert!(leaning.len() >= 3, "blocks lean their third sheet different ways: {leaning:?}");
        // A sheet's corner sways as the corner of its block's cube nearest it does: held only on the trunk's top.
        let corner_of = |quad: [u32; 2], c: usize| {
            let (at, ..) = unpack(quad);
            let (across, up) = TUFT_SHEETS[loose_sheet(quad) as usize];
            let (sa, sb) = [(-1, -1), (1, -1), (1, 1), (-1, 1)][c];
            [0, 1, 2].map(|k| at[k] + u32::from(sa * across[k] + sb * up[k] > 0))
        };
        let on_trunk = |corner: [u32; 3]| corner[1] == 3 && (4..=5).contains(&corner[0]) && (4..=5).contains(&corner[2]);
        let (mut held, mut free) = (0, 0);
        for &quad in sheets {
            for c in 0..4 {
                let sways = swaying_corners(quad) >> c & 1 == 1;
                assert_eq!(sways, !on_trunk(corner_of(quad, c)), "corner {:?}", corner_of(quad, c));
                held += u32::from(!sways);
                free += u32::from(sways);
            }
        }
        assert!(held > 0 && free > held * 10, "{held} held, {free} free");
        // Leaves that do not sway have sheets that stand still; the same sheets every time.
        let still = BlockFaces { sways: false, ..bushy };
        let stood = mesh_chunk(&map, [0, 0, 0], &[ground[0], ground[1], ground[2], still]);
        assert!(stood.quads.iter().all(|&quad| swaying_corners(quad) == 0));
        assert_eq!(stood.loose().1, count);
        assert_eq!(mesh_chunk(&map, [0, 0, 0], &[ground[0], ground[1], ground[2], bushy]), mesh);
    }

    #[test]
    fn a_vine_is_a_sheet_on_each_block_beside_it_or_hangs_crossed_and_is_no_ground() {
        use crate::blocks::block::BlockShape;
        // A floor, a wall along x at z = 4, three high, and a roof slab reaching out from its top over z = 5 and 6.
        let mut map = BlockMap::default();
        for z in 0..8 {
            for x in 0..8 {
                map.set([x, 0, z], 1);
            }
        }
        for y in 1..5 {
            for x in 0..8 {
                map.set([x, y, 4], 1);
            }
        }
        for z in 5..7 {
            for x in 0..8 {
                map.set([x, 4, z], 1);
            }
        }
        let vine = BlockFaces { shape: BlockShape::Vine, sways: true, ..BlockFaces::new(2, 2, 2) };
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), vine];
        let bare = mesh_chunk(&map, [0, 0, 0], &faces);
        // A vine down the wall under the roof, and one hanging from the roof's edge, two blocks long.
        for y in 2..4 {
            map.set([2, y, 5], 2);
            map.set([5, y, 6], 2);
        }
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        // No ground: the wall's and the roof's faces show behind it, no corner is darker for it, nothing stands on it.
        let ground = |mesh: &ChunkMesh| mesh.quads[..mesh.buckets[FACINGS] as usize].to_vec();
        assert_eq!(ground(&mesh), ground(&bare));
        assert_eq!((&mesh.positions, &mesh.indices), (&bare.positions, &bare.indices));
        assert_eq!(mesh.sights, bare.sights, "and it is seen through");
        let (first, count) = mesh.loose();
        let sheets: Vec<_> = mesh.quads[first as usize..(first + count) as usize].to_vec();
        assert!(sheets.iter().all(|&quad| unpack(quad).2 == VINE && unpack(quad).3 == 2));
        // The one by the wall clings to it, the wall being towards -z: one sheet a block, the lower its foot.
        let by_wall: Vec<_> = sheets.iter().filter(|&&quad| unpack(quad).0[0] == 2).collect();
        assert_eq!(by_wall.len(), 2);
        assert!(by_wall.iter().all(|&&quad| loose_sheet(quad) & 7 == 3 && swaying_corners(quad) == 0));
        let foot = |quad: [u32; 2]| loose_sheet(quad) & VINE_TIP != 0;
        assert!(by_wall.iter().all(|&&quad| foot(quad) == (unpack(quad).0[1] == 2)));
        // The one out under the roof hangs free: two crossed sheets a block, held at the roof and swaying below.
        let hanging: Vec<_> = sheets.iter().filter(|&&quad| unpack(quad).0[0] == 5).collect();
        assert_eq!(hanging.len(), 4);
        assert!(hanging.iter().all(|&&quad| (4..6).contains(&(loose_sheet(quad) & 7))));
        for &&quad in &hanging {
            let top = unpack(quad).0[1] == 3;
            assert_eq!(swaying_corners(quad), if top { 0b0011 } else { 0b1111 }, "held where it meets the roof");
            assert_eq!(foot(quad), !top);
        }
        // A vine that does not sway hangs still.
        let stiff = [faces[0], faces[1], BlockFaces { sways: false, ..vine }];
        assert!(mesh_chunk(&map, [0, 0, 0], &stiff).quads.iter().all(|&quad| swaying_corners(quad) == 0));
        // Water beside a vine shows its face towards it, as towards air.
        let mut pond = BlockMap::default();
        pond.set([1, 0, 1], 3);
        pond.set([2, 0, 1], 2);
        let wet = [faces[0], faces[1], vine, BlockFaces::water(5)];
        let pond = mesh_chunk(&pond, [0, 0, 0], &wet);
        assert_eq!(pond.water().1, 6, "a block of water alone: every face, the one towards the vine too");
    }

    #[test]
    fn a_block_seen_through_hides_no_face_and_keeps_its_own_against_its_kind() {
        // A floor of ground, a trunk of wood two high on it, and leaves round its top: three by three, two high.
        let mut map = BlockMap::default();
        for z in 0..8 {
            for x in 0..8 {
                map.set([x, 0, z], 1);
            }
        }
        map.set([4, 1, 4], 2);
        map.set([4, 2, 4], 2);
        for y in 2..4 {
            for z in 3..6 {
                for x in 3..6 {
                    if map.get([x, y, z]) == AIR {
                        map.set([x, y, z], 3);
                    }
                }
            }
        }
        let leaves = BlockFaces { see_through: true, ..BlockFaces::new(3, 3, 3) };
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::new(2, 2, 2), leaves];
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        let quads: Vec<_> = mesh.quads.iter().map(|&q| unpack(q)).collect();
        let area =
            |look: u8| -> u32 { quads.iter().filter(|q| q.3 == look).map(|q| q.1.iter().product::<u32>()).sum() };
        // Every face of every leaf block but the five against the wood: 17 blocks, six faces each.
        assert_eq!(area(3), 17 * 6 - 5);
        // The wood shows through the leaves: the upper block's four sides and its top, the lower's four sides.
        assert_eq!(area(2), 9, "no face of the wood is hidden by leaves");
        // The same crown of blocks that hide what is behind them: only its outside, and less of the wood.
        let solid = [faces[0], faces[1], faces[2], BlockFaces::new(3, 3, 3)];
        let closed: Vec<_> = mesh_chunk(&map, [0, 0, 0], &solid).quads.iter().map(|&q| unpack(q)).collect();
        let closed_area =
            |look: u8| -> u32 { closed.iter().filter(|q| q.3 == look).map(|q| q.1.iter().product::<u32>()).sum() };
        assert_eq!((closed_area(3), closed_area(2)), (9 + 9 - 1 + 12 * 2, 4));
        // Leaves are ground to stand on: their faces are triangles too; and they close corners in, as ground does.
        assert!(mesh.indices.len() > mesh_chunk(&map, [0, 0, 0], &solid).indices.len());
        let under = quads.iter().find(|q| q.3 == 3 && q.2 == 3).expect("a leaf's underside");
        assert!(under.4.iter().any(|shade| *shade < 3), "the trunk and the leaves beside darken a corner");
        // A chunk of leaves through and through still has faces, and is seen through from side to side.
        let mut full = BlockMap::default();
        full.put_chunk([0, 0, 0], crate::blocks::chunk::Chunk::filled(3));
        for beside in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
            full.put_chunk(beside, crate::blocks::chunk::Chunk::filled(3));
        }
        assert!(!full.buried([0, 0, 0], &faces) && full.buried([0, 0, 0], &solid));
        let crown = mesh_chunk(&full, [0, 0, 0], &faces);
        assert!(!crown.is_empty());
        assert_eq!(crown.sights, crate::blocks::sight::ALL_SIGHTS);
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
        assert_eq!(first, mesh.buckets[FACINGS + 1]);
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
