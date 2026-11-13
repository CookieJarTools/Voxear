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
//! A block with faces that join (`BlockFaces::connects`), as panes of glass
//! do, shows no face towards another of its kind, seen through or not, and
//! each of its faces that join is a quad of its own, never merged: in place
//! of its size it
//! keeps which of the eight faces round it, in its own plane, it joins
//! ([`joins`]), a bit each, by which the renderer picks each of its
//! corners from its picture's five tiles. A face joins one beside it of the
//! same block that shows too. Which way is which is the way its picture
//! lies (`terrainSide` in `shaders/terrain_ground.slang`), not the way its
//! quad is wound.
//!
//! A block creeps over those beside it by its `overlap` (`BlockFaces`):
//! where a face lies beside a face of another block with a higher one, in
//! its own plane, it is a quad of its own drawn with its look's twin
//! (`file::MIXED_LOOK`), which keeps in place of its size and its swaying
//! corners the look that creeps over it and from where ([`mixing`]): from
//! which of its four edges, or from which of its corners. The renderer
//! draws that look's picture a few ragged pixels in from there. Each block
//! that creeps over a face, and each way it does, is a quad more over the
//! same face, after it among the layer's quads and so drawn over it, the
//! higher in the order the later: such a quad is drawn only where its
//! fringe is. A face that joins, sways or is seen through takes none.
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
    PART_DRAWN, PART_HANGS, PART_SOLID, PART_TALL, connects, covers,
    file::{JOINS_BOTTOM, JOINS_SIDES, JOINS_TOP, MIXED_LOOK},
    hides, is_bushy, is_ground, is_part, is_see_through, is_vine, is_water, part_of,
    shape::{Around, BlockFacing, BlockState, Hinge, PartBox, StairCorner, part_boxes, posed_box, tall_boxes},
    sways,
};

/// The faces round a face it may join, in the order of their bits: where
/// each lies along its picture, to the right and down: left, right, up,
/// down, then up left, up right, down left and down right.
pub const JOINS: [(i32, i32); 8] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)];

/// For a face facing along each axis, the way its picture runs to the
/// right and the way it runs down, as the renderer lays a surface's
/// pictures on it: an axis and whether along it.
const PICTURE: [[(usize, i32); 2]; 3] = [[(2, 1), (1, -1)], [(0, 1), (2, -1)], [(0, 1), (1, -1)]];

/// Which of the faces round it a quad of a block that joins is joined to,
/// a bit each in [`JOINS`]' order. Such a quad is one face; only a surface
/// that joins keeps these where another quad keeps its size.
pub fn joins(quad: [u32; 2]) -> u8 {
    (quad[0] >> 15) as u8
}

/// What creeps over a quad drawn with a look's twin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mixing {
    /// The look whose picture creeps over it.
    pub look: u8,
    /// From where, a bit each: its edges in [`JOINS`]' order (left, right,
    /// up, down), or with `corners` its corners in the order [`JOINS`] has
    /// them after its edges (up left, up right, down left, down right).
    pub from: u8,
    pub corners: bool,
    /// Whether it is a quad more over a face drawn already, drawn only
    /// where its fringe is.
    pub over: bool,
}

/// What creeps over a quad drawn with a look's twin, from its first word.
pub fn mixing(quad: [u32; 2]) -> Mixing {
    let first = quad[0];
    Mixing { look: (first >> 15) as u8, from: (first >> 28) as u8, corners: first >> 24 & 1 != 0, over: first >> 23 & 1 != 0 }
}

/// A [`Mixing`] as the bits a quad's first word keeps it in.
fn mixed(mixing: Mixing) -> u32 {
    let (corners, over) = (u32::from(mixing.corners), u32::from(mixing.over));
    u32::from(mixing.look) << 15 | over << 23 | corners << 24 | u32::from(mixing.from & 15) << 28
}

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
/// those of facing `f`, then its loose quads from `buckets[FACINGS]`, its
/// blocks of parts' from `buckets[FACINGS + 1]` (two quads' words each, see
/// [`pack_part`]), and after `buckets[FACINGS + 2]` its water's; its
/// ground's faces as triangles, in blocks from the chunk's corner, three
/// indices a triangle, facing out; which of the chunk's sides see each
/// other through its air and water (see `sight`); and, drawn smooth, the
/// ways its triangles face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub quads: Vec<[u32; 2]>,
    pub buckets: [u32; FACINGS + 3],
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Boxes it collides as besides its triangles, in blocks from its
    /// corner, lowest corner first: its fences and walls a block and a half
    /// high. Physics alone takes them; nothing draws them.
    pub colliders: Vec<[[f32; 3]; 2]>,
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
        let first = self.buckets[FACINGS + 2];
        (first, self.quads.len() as u32 - first)
    }

    /// Its blocks of parts' quads, two quads' words each: where they start
    /// among its quads, and how many words.
    pub fn parts(&self) -> (u32, u32) {
        (self.buckets[FACINGS + 1], self.buckets[FACINGS + 2] - self.buckets[FACINGS + 1])
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
    // A block of parts closes no corner in: a slab beside a face leaves it as light.
    let solid = |p: [i32; 3]| is_ground(faces, block(p)) && !is_part(faces, block(p));
    let hidden_by = |p: [i32; 3]| hides(faces, block(p));
    let any_part = faces.iter().any(|faces| faces.part & PART_DRAWN != 0 && !faces.water);
    // A face is never seen against a block that hides, nor against a side of a block of parts covered whole.
    let hidden_from = |p: [i32; 3], facing: usize| {
        hidden_by(p) || any_part && covers(faces, block(p)) & (1 << (facing ^ 1)) != 0
    };
    // Ground that stands still holds every corner it touches.
    let any_sway = faces.iter().any(|faces| faces.sways);
    let still = |p: [i32; 3]| is_ground(faces, block(p)) && !sways(faces, block(p));
    let any_join = faces.iter().any(|faces| faces.connects != 0);
    let any_mix = faces.iter().any(|faces| faces.mixes != 0);
    let kind = |id: BlockId| faces.get(usize::from(id)).copied().unwrap_or_default();
    let side = CHUNK as usize;
    let mut mesh = ChunkMesh::default();
    let mut shaded = vec![0u32; side * side];
    let mut plain = vec![0u32; side * side];
    // What each face of the layer keeps where its size would be: which faces round it it joins, for the blocks
    // that join, and what creeps over it, for a face drawn with its look's twin.
    let mut joined = vec![0u32; side * side];
    // The quads more over faces of the layer that more than one block creeps over, or one from an edge and a
    // corner apart from it: drawn after the layer's own.
    let mut layered: Vec<[u32; 2]> = Vec::new();
    for (facing, &(axis, out, u, v)) in AXES.iter().enumerate() {
        mesh.buckets[facing] = mesh.quads.len() as u32;
        let step = if out { 1 } else { -1 };
        for layer in 0..side {
            shaded.fill(0);
            plain.fill(0);
            if any_join || any_mix {
                joined.fill(0);
            }
            for j in 0..side {
                for i in 0..side {
                    let mut p = [0i32; 3];
                    (p[axis], p[u], p[v]) = (layer as i32, i as i32, j as i32);
                    let id = block(p);
                    let mut beyond = p;
                    beyond[axis] += step;
                    if !is_ground(faces, id) || hidden_from(beyond, facing) || any_part && is_part(faces, id) {
                        continue;
                    }
                    // A block whose faces join is one body with those of its kind: no face between two of them.
                    let joining = if any_join { connects(faces, id) } else { 0 };
                    if joining != 0 && block(beyond) == id {
                        continue;
                    }
                    let joining = joining & joins_bit(facing) != 0;
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
                    if joining {
                        // A face of its own, by its place in the layer, with the faces round it that are its
                        // block's and show as it does.
                        key |= ((j * side + i) as u32 + 1) << 20;
                        let [right, down] = PICTURE[axis];
                        let mut round = 0u8;
                        for (bit, (du, dv)) in JOINS.into_iter().enumerate() {
                            let mut q = p;
                            q[right.0] += du * right.1;
                            q[down.0] += dv * down.1;
                            let mut before = q;
                            before[axis] += step;
                            let shows = block(q) == id && !hidden_by(before) && block(before) != id;
                            round |= u8::from(shows) << bit;
                        }
                        joined[j * side + i] = u32::from(round) << 15;
                    }
                    if any_mix && !joining && kind(id).mixes & joins_bit(facing) != 0 {
                        // The blocks beside it in its plane that lie higher in the order and show a face as it
                        // does: each creeps over it.
                        let [right, down] = PICTURE[axis];
                        let own = kind(id).overlap;
                        let mut beside = [AIR; 8];
                        for (at, (du, dv)) in JOINS.into_iter().enumerate() {
                            let mut q = p;
                            q[right.0] += du * right.1;
                            q[down.0] += dv * down.1;
                            let mut before = q;
                            before[axis] += step;
                            let other = block(q);
                            let its = kind(other);
                            let creeps = other != id
                                && is_ground(faces, other)
                                && !is_see_through(faces, other)
                                && its.overlap > own
                                && its.connects & joins_bit(facing) == 0
                                && !hidden_by(before);
                            if creeps {
                                beside[at] = other;
                            }
                        }
                        // The lowest in the order first, so the higher is drawn over it where two meet.
                        let mut creeping: Vec<BlockId> = beside.into_iter().filter(|&other| other != AIR).collect();
                        creeping.sort_by_key(|&other| (kind(other).overlap, std::cmp::Reverse(other)));
                        creeping.dedup();
                        let mut ways: Vec<Mixing> = Vec::new();
                        for over in creeping {
                            let look = surface(faces, over, facing);
                            let edges = (0..4).fold(0u8, |all, at| all | u8::from(beside[at] == over) << at);
                            // Its corners that none of its own edges reaches: left or right of it, and above or
                            // below.
                            let apart = |corner: usize| edges & (1 << (corner & 1) | 4 << (corner >> 1)) == 0;
                            let corners = (0..4).filter(|&corner| beside[4 + corner] == over && apart(corner));
                            let corners = corners.fold(0u8, |all, corner| all | 1 << corner);
                            if edges != 0 {
                                ways.push(Mixing { look, from: edges, corners: false, over: !ways.is_empty() });
                            }
                            if corners != 0 {
                                ways.push(Mixing { look, from: corners, corners: true, over: !ways.is_empty() });
                            }
                        }
                        if let Some(&first) = ways.first() {
                            // Its look's twin, a face of its own by its place in the layer.
                            key |= u32::from(MIXED_LOOK) | ((j * side + i) as u32 + 1) << 20;
                            joined[j * side + i] = mixed(first);
                            let mut at = [0u32; 3];
                            (at[axis], at[u], at[v]) = (layer as u32, i as u32, j as u32);
                            let [place, second] = pack(at, [1, 1], facing, look | MIXED_LOOK, shade);
                            layered.extend(ways[1..].iter().map(|&way| [place | mixed(way), second]));
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
                // One face of a block that joins keeps the faces it joins where its size would be, and one another
                // block creeps over what does: a block wide and high, its size is nothing there. Any other face
                // keeps nothing.
                mesh.quads.push([first | joined[j * side + i] | ((key >> 16) & 15) << 28, second]);
            }
            mesh.quads.append(&mut layered);
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
    let loose = |faces: &BlockFaces| faces.shape.is_loose() && !faces.water;
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
    if any_part && chunk.may_hold(|id| is_part(faces, id)) {
        let corner = key.map(|v| v * CHUNK as i32);
        mesh_parts(&mut mesh, &block, &hidden_from, faces, &|at: [i32; 3]| map.get(at), corner);
    }
    mesh.buckets[FACINGS + 2] = mesh.quads.len() as u32;
    if chunk.may_hold(|block| is_water(faces, block)) {
        let made = held(key, made);
        // Water shows a face against a block of parts but where its side is covered whole.
        let open = |beyond: BlockId| {
            beyond == AIR || is_vine(faces, beyond) || is_part(faces, beyond) && covers(faces, beyond) == 0
        };
        mesh.quads.extend(water_quads((&block, &fills(map, key)), faces, |at, beyond| open(beyond) && made(at)));
    }
    mesh.sights = super::sight::sights(chunk, |block| !hides(faces, block));
    mesh
}

/// A part quad's kind among loose quads' (`VINE`), with this bit for the
/// first of its two quads' words and the other for its second, which a
/// vine's sheet never has.
pub const PART_HEAD: u32 = 1 << 24;
pub const PART_TAIL: u32 = 1 << 23;

/// One face of a box of a block of parts, as its two quads' words say it:
/// its block in its chunk, the way it faces, where its plane is along that
/// way and the rectangle it fills across it, in sixteenths (`AXES` gives
/// which ways are across), its surface, and what it moves by: whether it
/// moves, its block's motion (from 1, 0 for none), whether its block is
/// open (or on), what it turns about, and how many hanging blocks are over
/// its block before what holds them, from 1 (0 where it does not hang).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartQuad {
    pub at: [u32; 3],
    pub facing: usize,
    pub plane: u32,
    pub across: [u32; 2],
    pub up: [u32; 2],
    pub surface: u8,
    pub moves: Option<Hinge>,
    pub motion: u32,
    pub open: bool,
    pub hangs: u32,
}

/// A part quad as two quads' words, in the order the renderer reads them
/// (`partPoint` in `shaders/block_quads.slang`): the second's of each pair
/// keeps its low half alone, as the renderer puts a chunk's number over it.
pub fn pack_part(quad: &PartQuad) -> [[u32; 2]; 2] {
    let [x, y, z] = quad.at;
    let moving = u32::from(quad.moves.is_some());
    let kind = (VINE as u32) << 25;
    let head = x | y << 5 | z << 10 | (quad.facing as u32) << 15 | quad.plane << 18 | moving << 23 | PART_HEAD | kind;
    let head = head | (quad.across[0] & 15) << 28;
    let head_second = u32::from(quad.surface) | (quad.up[0] & 15) << 8 | (quad.hangs & 15) << 12;
    let hinge = quad.moves.unwrap_or_default();
    let axis = u32::from(hinge.axis.index());
    let tail = quad.across[1] | quad.up[1] << 5 | (quad.motion & 63) << 10 | u32::from(quad.open) << 16 | axis << 17;
    let tail = tail | PART_TAIL | kind;
    let [px, py, pz] = hinge.pivot.map(u32::from);
    [[head, head_second], [tail, px | py << 5 | pz << 10]]
}

/// A part quad from its two quads' words, as [`pack_part`] writes them.
pub fn unpack_part([[head, head_second], [tail, tail_second]]: [[u32; 2]; 2]) -> PartQuad {
    let moving = head >> 23 & 1 != 0;
    let hinge = Hinge {
        pivot: [tail_second & 31, tail_second >> 5 & 31, tail_second >> 10 & 31].map(|v| v as u8),
        axis: BlockFacing::from_index((tail >> 17 & 7) as u8),
    };
    PartQuad {
        at: [head & 31, head >> 5 & 31, head >> 10 & 31],
        facing: (head >> 15 & 7) as usize,
        plane: head >> 18 & 31,
        across: [head >> 28, tail & 31],
        up: [head_second >> 8 & 15, tail >> 5 & 31],
        surface: head_second as u8,
        moves: moving.then_some(hinge),
        motion: tail >> 10 & 63,
        open: tail >> 16 & 1 != 0,
        hangs: head_second >> 12 & 15,
    }
}

/// How many hanging blocks a part quad counts over its block at most.
const MOST_HANGING: u32 = 14;

/// What a block of parts of `kind` at `p` takes from those round it: which
/// of them a fence, a wall or a pane joins, and which corner a stair turns.
pub fn around_of(faces: &[BlockFaces], block: &dyn Fn([i32; 3]) -> BlockId, p: [i32; 3], kind: &BlockFaces) -> Around {
    use super::block::BlockShape;
    let step = |way: BlockFacing| {
        let s = way.step();
        [p[0] + s[0], p[1] + s[1], p[2] + s[2]]
    };
    let faces_of = |id: BlockId| faces.get(usize::from(id)).copied().unwrap_or_default();
    match kind.shape {
        BlockShape::Fence | BlockShape::Wall | BlockShape::Pane => {
            let mut joins = 0;
            for (bit, way) in BlockFacing::AROUND.into_iter().enumerate() {
                let id = block(step(way));
                let other = faces_of(id);
                // A whole block, or another of its kind; a gate across the way.
                let whole = is_ground(faces, id) && !is_part(faces, id);
                let gate = other.shape == BlockShape::FenceGate && !other.water && {
                    let facing = BlockState::from_byte(other.state).facing.step();
                    let across = way.step();
                    facing[0] * across[0] + facing[2] * across[2] == 0
                };
                let same = other.shape == kind.shape && !other.water;
                let joined = whole || same || gate && kind.shape != BlockShape::Pane;
                joins |= u8::from(joined && id != AIR) << bit;
            }
            Around { joins, ..Around::default() }
        }
        BlockShape::Stairs => {
            let state = BlockState::from_byte(kind.state);
            let stair = |id: BlockId| {
                let other = faces_of(id);
                let theirs = BlockState::from_byte(other.state);
                let across = theirs.facing != state.facing && theirs.facing != state.facing.opposite();
                (other.shape == BlockShape::Stairs && !other.water && theirs.half == state.half && across)
                    .then_some(theirs.facing)
            };
            let corner = match (stair(block(step(state.facing))), stair(block(step(state.facing.opposite())))) {
                (Some(other), _) => StairCorner::Outer(other),
                (None, Some(other)) => StairCorner::Inner(other),
                _ => StairCorner::Straight,
            };
            Around { corner, ..Around::default() }
        }
        _ => Around::default(),
    }
}

/// Every block of parts' faces in a chunk: each box's faces as part quads
/// (but those against a block that hides them, unless the box moves), its
/// boxes that collide as triangles, posed as they rest, and its fences' and
/// walls' tall boxes as colliders. `block` gives the blocks of the chunk
/// and a block round it, `hidden_from` whether a face facing so is hidden
/// by the block at a place, and `world` any block of the world, from the
/// chunk's `corner`, for how far a hanging block hangs.
fn mesh_parts(
    mesh: &mut ChunkMesh,
    block: &dyn Fn([i32; 3]) -> BlockId,
    hidden_from: &dyn Fn([i32; 3], usize) -> bool,
    faces: &[BlockFaces],
    world: &dyn Fn([i32; 3]) -> BlockId,
    corner: [i32; 3],
) {
    let side = CHUNK as i32;
    for y in 0..side {
        for z in 0..side {
            for x in 0..side {
                let p = [x, y, z];
                let id = block(p);
                if !is_part(faces, id) {
                    continue;
                }
                let kind = faces[usize::from(id)];
                let state = BlockState::from_byte(kind.state);
                let around = around_of(faces, block, p, &kind);
                let boxes = part_boxes(kind.shape, state, around, kind.custom.as_ref());
                // How many hanging blocks are over it, to what holds them up; none where nothing does.
                let hangs = match kind.part & PART_HANGS != 0 {
                    false => 0,
                    true => {
                        let mut over = 0;
                        let at = |up: i32| world([corner[0] + x, corner[1] + y + up, corner[2] + z]);
                        while over < MOST_HANGING && part_of(faces, at(over as i32 + 1)) & PART_HANGS != 0 {
                            over += 1;
                        }
                        let holder = at(over as i32 + 1);
                        if holder == AIR || is_water(faces, holder) { 0 } else { over + 1 }
                    }
                };
                let at = p.map(|v| v as u32);
                for part in &boxes {
                    for (facing, &(axis, out, u, v)) in AXES.iter().enumerate() {
                        let plane = if out { part.to[axis] } else { part.from[axis] };
                        let on_side = if out { plane == 16 } else { plane == 0 };
                        let mut beyond = p;
                        beyond[axis] += if out { 1 } else { -1 };
                        if part.moves.is_none() && on_side && hidden_from(beyond, facing) {
                            continue;
                        }
                        // A rail's end against the fence, wall or gate it joins lies inside what it joins.
                        let joined = BlockFacing::of_side(facing);
                        let bit = BlockFacing::AROUND.iter().position(|&way| way == joined);
                        if on_side && bit.is_some_and(|bit| around.joins & (1 << bit) != 0) && is_part(faces, block(beyond)) {
                            continue;
                        }
                        let (across, up) = ([part.from[u], part.to[u]], [part.from[v], part.to[v]]);
                        if across[0] >= across[1] || up[0] >= up[1] {
                            continue;
                        }
                        let quad = PartQuad {
                            at,
                            facing,
                            plane: u32::from(plane.min(16)),
                            across: across.map(|v| u32::from(v.min(16))),
                            up: up.map(|v| u32::from(v.min(16))),
                            surface: surface(faces, id, facing),
                            moves: part.moves,
                            motion: u32::from(kind.motion),
                            open: state.is_moved(),
                            hangs,
                        };
                        mesh.quads.extend(pack_part(&quad));
                    }
                }
                let origin = p.map(|v| v as f32);
                if kind.part & PART_SOLID != 0 {
                    let (turn, shift) = (f32::from(kind.rest[0]) / 10.0, f32::from(kind.rest[1]) / 4.0);
                    for part in &boxes {
                        let [low, high] = posed_box(part, turn, shift);
                        box_triangles(mesh, origin, [low, high].map(|c| c.map(|v| v / 16.0)));
                    }
                }
                if kind.part & PART_TALL != 0 {
                    for part in tall_boxes(kind.shape, state, around) {
                        let (from, to) = (part.from.map(|v| f32::from(v) / 16.0), part.to.map(|v| f32::from(v) / 16.0));
                        mesh.colliders.push([0, 1].map(|end| {
                            let c = if end == 0 { from } else { to };
                            [origin[0] + c[0], origin[1] + c[1], origin[2] + c[2]]
                        }));
                    }
                }
            }
        }
    }
}

/// A box's six faces as triangles, facing out, from `origin` in blocks and
/// its corners `[low, high]` in blocks from there.
fn box_triangles(mesh: &mut ChunkMesh, origin: [f32; 3], [low, high]: [[f32; 3]; 2]) {
    if (0..3).any(|i| high[i] - low[i] <= 1e-4) {
        return;
    }
    for &(axis, out, u, v) in &AXES {
        let mut corner = [0.0f32; 3];
        corner[axis] = origin[axis] + if out { high[axis] } else { low[axis] };
        corner[u] = origin[u] + low[u];
        corner[v] = origin[v] + low[v];
        let (w, h) = (high[u] - low[u], high[v] - low[v]);
        let first = mesh.positions.len() as u32;
        for (du, dv) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
            let mut point = corner;
            point[u] += du;
            point[v] += dv;
            mesh.positions.push(point);
        }
        mesh.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
}

/// The part boxes of the block of parts at `p` in `faces`' world, as
/// [`part_boxes`] makes them, with what it takes from the blocks round it:
/// for a brush's outline and a block's picture.
pub fn boxes_at(faces: &[BlockFaces], block: &dyn Fn([i32; 3]) -> BlockId, p: [i32; 3]) -> Vec<PartBox> {
    let id = block(p);
    if !is_part(faces, id) {
        return Vec::new();
    }
    let kind = faces[usize::from(id)];
    let around = around_of(faces, block, p, &kind);
    part_boxes(kind.shape, BlockState::from_byte(kind.state), around, kind.custom.as_ref())
}

/// The bit of a block's faces facing `facing` among those that join (see
/// `BlockFaces::connects`).
fn joins_bit(facing: usize) -> u8 {
    match facing {
        2 => JOINS_TOP,
        3 => JOINS_BOTTOM,
        _ => JOINS_SIDES,
    }
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
    use crate::blocks::flow::Water;
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
    fn a_block_that_joins_shows_no_face_between_two_of_it_and_each_face_says_which_round_it_it_joins() {
        // Glass: seen through, and its faces join. Stone is plain ground.
        let glass = BlockFaces { see_through: true, connects: 7, ..BlockFaces::new(3, 3, 3) };
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), glass];
        let quads = |blocks: &[([i32; 3], BlockId)]| {
            let mut map = BlockMap::default();
            blocks.iter().for_each(|&(at, block)| map.set(at, block));
            mesh_chunk(&map, [0, 0, 0], &faces).quads
        };
        let of = |quads: &[[u32; 2]], at: [u32; 3], facing: usize| -> Vec<u8> {
            let found = quads.iter().filter(|&&quad| {
                let (place, _, way, look, _) = unpack([quad[0] & !(0x3FF << 15), quad[1]]);
                place == at && way == facing && look == 3
            });
            found.map(|&quad| joins(quad)).collect()
        };
        let bits = |round: &[(i32, i32)]| -> u8 {
            round.iter().fold(0, |all, way| all | 1 << JOINS.iter().position(|kept| kept == way).unwrap())
        };
        // One alone: six faces, none joined.
        let alone = quads(&[([4, 4, 4], 2)]);
        assert_eq!(alone.len(), 6);
        assert!(alone.iter().all(|&quad| joins(quad) == 0));
        // A wall of three by three in the plane of x and y, facing along z: each pane a face of its own each
        // way it shows, none between two panes, though glass is seen through.
        let wall: Vec<([i32; 3], BlockId)> =
            (4..7).flat_map(|y| (4..7).map(move |x| ([x, y, 8], 2))).collect();
        let made = quads(&wall);
        assert_eq!(made.len(), 9 + 9 + 4 * 3, "its two sides, and its four edges");
        // The middle pane joins all eight round it; a picture on a face facing along z runs right along x and
        // down against y.
        assert_eq!(of(&made, [5, 5, 8], 4), [0xFF]);
        assert_eq!(of(&made, [5, 5, 8], 5), [0xFF]);
        // The pane at the wall's top left, seen from +z: those to its right, below it, and down right.
        assert_eq!(of(&made, [4, 6, 8], 4), [bits(&[(1, 0), (0, 1), (1, 1)])]);
        // The middle of its bottom row: left, right, up, and the two above.
        assert_eq!(of(&made, [5, 4, 8], 4), [bits(&[(-1, 0), (1, 0), (0, -1), (-1, -1), (1, -1)])]);
        // The wall's top edge, facing up: a row along x, a picture there running right along x.
        assert_eq!(of(&made, [5, 6, 8], 2), [bits(&[(-1, 0), (1, 0)])]);
        // Its side edge, facing +x: a column, a picture there running down against y.
        assert_eq!(of(&made, [6, 5, 8], 0), [bits(&[(0, -1), (0, 1)])]);

        // A pane whose face is covered joins nothing there: stone before the pane to the right, and the
        // middle's face no longer joins it; glass before it, the same.
        for cover in [1, 2] {
            let mut covered = wall.clone();
            covered.push(([6, 5, 9], cover));
            let made = quads(&covered);
            let middle = of(&made, [5, 5, 8], 4);
            assert_eq!(middle, [0xFF & !bits(&[(1, 0)])], "covered by {cover}");
            assert!(of(&made, [6, 5, 8], 4).is_empty(), "the covered pane shows no face that way");
        }
        // Another kind of block beside it is not joined: stone in the wall's middle.
        let mut holed = wall.clone();
        holed[4] = ([5, 5, 8], 1);
        let made = quads(&holed);
        assert_eq!(of(&made, [4, 5, 8], 4), [bits(&[(0, -1), (0, 1), (1, -1), (1, 1)])]);
        // A block whose sides alone join: its top is merged as ever, and still none shows between two of it.
        let shelf = BlockFaces { connects: JOINS_SIDES, ..BlockFaces::new(4, 3, 4) };
        let shelves = [BlockFaces::default(), BlockFaces::new(1, 1, 1), shelf];
        let mut map = BlockMap::default();
        (0..3).for_each(|x| map.set([x, 0, 0], 2));
        let made = mesh_chunk(&map, [0, 0, 0], &shelves).quads;
        let tops: Vec<[u32; 2]> = made.iter().map(|&quad| unpack(quad)).filter(|quad| quad.2 == 2).map(|quad| quad.1).collect();
        assert_eq!(tops, [[1, 3]], "one quad three blocks long");
        assert_eq!(made.len(), 2 + 3 + 3 + 2, "top and bottom, three faces each long side, an end each");
        // Stone's own faces are merged and keep their size as ever.
        let field: Vec<([i32; 3], BlockId)> = (0..4).flat_map(|z| (0..4).map(move |x| ([x, 0, z], 1))).collect();
        let (_, size, facing, ..) = unpack(quads(&field)[2]);
        assert_eq!((size, facing), ([4, 4], 2));
    }

    #[test]
    fn a_block_higher_in_the_order_creeps_over_the_faces_beside_it_in_their_plane() {
        // Stone (1) keeps to itself, dirt (2) lies over it and grass (3) over both; each can take another's edge.
        let ground = |look: u8, overlap: u8| BlockFaces { mixes: 7, overlap, ..BlockFaces::new(look, look, look) };
        let faces = [BlockFaces::default(), ground(1, 0), ground(2, 1), ground(3, 2)];
        let quads = |blocks: &[([i32; 3], BlockId)]| {
            let mut map = BlockMap::default();
            blocks.iter().for_each(|&(at, block)| map.set(at, block));
            mesh_chunk(&map, [0, 0, 0], &faces).quads
        };
        // A face by its place and the way it faces: its look, and what creeps over it.
        let top = |quads: &[[u32; 2]], at: [u32; 3]| -> Vec<(u8, Mixing)> {
            let found = quads.iter().filter(|&&quad| {
                let first = quad[0] & !(0x3FF << 15) & !(15 << 28);
                let (place, _, way, ..) = unpack([first, quad[1]]);
                place == at && way == 2 && quad[1] as u8 & MIXED_LOOK != 0
            });
            found.map(|&quad| (quad[1] as u8, mixing(quad))).collect()
        };
        let edge = |look: u8, way: (i32, i32)| {
            let from = 1u8 << JOINS.iter().position(|kept| *kept == way).unwrap();
            Mixing { look, from, corners: false, over: false }
        };
        // A floor of stone five by five with one block of grass in its middle. A top's picture runs right along
        // x and down against z, so the stone at lower x has the grass to its right.
        let mut floor: Vec<([i32; 3], BlockId)> = (0..5).flat_map(|z| (0..5).map(move |x| ([x, 0, z], 1))).collect();
        floor[12] = ([2, 0, 2], 3);
        let made = quads(&floor);
        assert_eq!(top(&made, [1, 0, 2]), [(1 | MIXED_LOOK, edge(3, (1, 0)))], "from its right");
        assert_eq!(top(&made, [3, 0, 2]), [(1 | MIXED_LOOK, edge(3, (-1, 0)))], "from its left");
        assert_eq!(top(&made, [2, 0, 3]), [(1 | MIXED_LOOK, edge(3, (0, 1)))], "the grass is down its picture");
        assert_eq!(top(&made, [2, 0, 1]), [(1 | MIXED_LOOK, edge(3, (0, -1)))]);
        // The four stones that touch the grass by a corner alone: from that corner.
        let corner = |look: u8, way: (i32, i32)| {
            let from = 1u8 << JOINS[4..].iter().position(|kept| *kept == way).unwrap();
            Mixing { look, from, corners: true, over: false }
        };
        assert_eq!(top(&made, [1, 0, 3]), [(1 | MIXED_LOOK, corner(3, (1, 1)))]);
        assert_eq!(top(&made, [3, 0, 1]), [(1 | MIXED_LOOK, corner(3, (-1, -1)))]);
        // Eight faces of their own round the grass; the grass's top is its own look, and the rest of the floor is
        // still merged: far fewer quads than faces.
        let up = |quad: &&[u32; 2]| (quad[0] >> 25) & 7 == 2;
        let twins = made.iter().filter(up).filter(|quad| quad[1] as u8 & MIXED_LOOK != 0).count();
        assert_eq!(twins, 8);
        assert!(top(&made, [2, 0, 2]).is_empty() && top(&made, [0, 0, 0]).is_empty());
        let tops = made.iter().filter(|&&quad| (quad[0] >> 25) & 7 == 2).count();
        assert!(tops < 9 + 8, "{tops} quads for 25 tops");

        // Each block beside a face that lies higher creeps over it from its own side: the lower first, and the
        // higher a quad more over the same face, drawn after it and only where its fringe is.
        let over = |mixing: Mixing| Mixing { over: true, ..mixing };
        let made = quads(&[([0, 0, 0], 2), ([1, 0, 0], 1), ([2, 0, 0], 3)]);
        let both = [(1 | MIXED_LOOK, edge(2, (-1, 0))), (1 | MIXED_LOOK, over(edge(3, (1, 0))))];
        assert_eq!(top(&made, [1, 0, 0]), both, "dirt from its left, then grass from its right");
        let first = made.iter().position(|&quad| mixing(quad) == both[0].1 && (quad[0] >> 25) & 7 == 2);
        let second = made.iter().position(|&quad| mixing(quad) == both[1].1 && (quad[0] >> 25) & 7 == 2);
        assert!(first < second && first.is_some(), "the quad more comes after the face's own");
        assert_eq!(made[first.unwrap()][1], made[second.unwrap()][1], "the same look and shading");
        // One block from an edge and from a corner apart from it: two quads; a corner its edge reaches, one.
        let made = quads(&[([1, 0, 1], 1), ([0, 0, 1], 3), ([2, 0, 0], 3), ([0, 0, 0], 3)]);
        let ways = [(1 | MIXED_LOOK, edge(3, (-1, 0))), (1 | MIXED_LOOK, over(corner(3, (1, 1))))];
        assert_eq!(top(&made, [1, 0, 1]), ways, "from its left, and from the corner down right of it");
        let made = quads(&[([0, 0, 0], 2), ([1, 0, 0], 3)]);
        assert_eq!(top(&made, [0, 0, 0]), [(2 | MIXED_LOOK, edge(3, (1, 0)))], "grass over dirt");
        assert!(top(&made, [1, 0, 0]).is_empty(), "nothing creeps over the higher");
        // A face that does not show creeps over nothing: grass under a block, or a step up or down from it.
        let made = quads(&[([0, 0, 0], 1), ([1, 0, 0], 3), ([1, 1, 0], 1)]);
        assert!(top(&made, [0, 0, 0]).is_empty(), "the grass's top is covered");
        let made = quads(&[([0, 0, 0], 1), ([1, 1, 0], 3)]);
        assert!(top(&made, [0, 0, 0]).is_empty(), "a step up is another plane");
        // Sides take it as tops do: grass beside stone in a wall, a side's picture running right along x.
        let made = quads(&[([0, 0, 0], 1), ([1, 0, 0], 3)]);
        let side: Vec<[u32; 2]> = made.iter().copied().filter(|quad| (quad[0] >> 25) & 7 == 4).collect();
        assert!(side.iter().any(|&quad| quad[1] as u8 == 1 | MIXED_LOOK && mixing(quad) == edge(3, (1, 0))));
        // A block that takes none keeps its faces merged beside any.
        let hard = [BlockFaces::default(), BlockFaces::new(1, 1, 1), ground(2, 1), ground(3, 2)];
        let mut map = BlockMap::default();
        [([0, 0, 0], 1), ([1, 0, 0], 3)].into_iter().for_each(|(at, block)| map.set(at, block));
        assert!(mesh_chunk(&map, [0, 0, 0], &hard).quads.iter().all(|quad| quad[1] as u8 & MIXED_LOOK == 0));
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

#[cfg(test)]
mod part_tests {
    use super::*;
    use crate::blocks::{
        block::{BlockDef, BlockShape},
        file::{BlockFile, BlockKind},
        shape::{BlockHalf, DoorHinge},
    };

    /// A world's faces of stone (1) and, after it, a block of each shape
    /// and state given, numbered from 2.
    fn world_of(parts: &[(BlockShape, BlockState)]) -> Vec<BlockFaces> {
        let mut file = BlockFile { blocks: vec![BlockKind::colored("Stone", [0.5; 3])], ..BlockFile::default() };
        for (at, &(shape, state)) in parts.iter().enumerate() {
            let mut kind = BlockKind::of_state(&format!("Blocks/Part {at}.block"), state.normalized(shape).to_byte());
            BlockDef { shape, ..BlockDef::colored([0.6, 0.4, 0.2]) }.fill(&mut kind);
            file.blocks.push(kind);
        }
        file.faces().1
    }

    fn parts_of(mesh: &ChunkMesh) -> Vec<PartQuad> {
        let (first, words) = mesh.parts();
        let quads = &mesh.quads[first as usize..(first + words) as usize];
        quads.chunks_exact(2).map(|pair| unpack_part([pair[0], pair[1]])).collect()
    }

    #[test]
    fn a_part_quad_keeps_all_it_says_in_two_quads_words() {
        let hinge = Hinge { pivot: [29, 16, 3], axis: BlockFacing::Up };
        let quad = PartQuad {
            at: [31, 0, 17],
            facing: 4,
            plane: 16,
            across: [3, 16],
            up: [0, 13],
            surface: 201,
            moves: Some(hinge),
            motion: 63,
            open: true,
            hangs: 15,
        };
        let words = pack_part(&quad);
        assert!(words.iter().all(|word| word[1] >> 16 == 0), "a chunk's number goes over each second word's top half");
        assert_eq!(unpack_part(words), quad);
        assert_eq!(unpack_part(pack_part(&PartQuad { moves: None, ..quad })).moves, None);
        // Read as loose quads, the first is a part's head and the second its tail, and neither is a vine's sheet.
        assert!(words[0][0] & PART_HEAD != 0 && words[1][0] & PART_HEAD == 0 && words[1][0] & PART_TAIL != 0);
        assert!(words.iter().all(|word| (word[0] >> 25) & 7 == VINE as u32));
    }

    #[test]
    fn a_slab_hides_the_face_it_lies_on_and_is_drawn_and_stood_on_as_half_a_block() {
        let faces = world_of(&[(BlockShape::Slab, BlockState::default_for(BlockShape::Slab))]);
        let mut map = BlockMap::default();
        map.set([4, 0, 4], 1);
        map.set([4, 1, 4], 2);
        map.set([5, 1, 4], 1);
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        // The stone's top under the slab is hidden; the stone beside it keeps its face towards the slab.
        let tops: Vec<_> = (mesh.buckets[2]..mesh.buckets[3]).map(|i| unpack(mesh.quads[i as usize]).0).collect();
        assert!(!tops.contains(&[4, 0, 4]) && tops.contains(&[5, 1, 4]), "{tops:?}");
        let west: Vec<_> = (mesh.buckets[1]..mesh.buckets[2]).map(|i| unpack(mesh.quads[i as usize]).0).collect();
        assert!(west.contains(&[5, 1, 4]), "the stone's face towards the slab shows: {west:?}");
        // The slab: five faces, none under it, none against the stone beside it; its top half a block up.
        let parts = parts_of(&mesh);
        assert_eq!(parts.len(), 4, "{parts:?}");
        let top = parts.iter().find(|quad| quad.facing == 2).expect("a top");
        assert_eq!((top.plane, top.across, top.up), (8, [0, 16], [0, 16]));
        assert!(parts.iter().all(|quad| quad.at == [4, 1, 4] && quad.moves.is_none() && quad.hangs == 0));
        // Its collision is half a block high.
        let highest = mesh.positions.iter().filter(|p| p[0] >= 4.0 && p[0] < 4.99 && p[2] >= 4.0 && p[2] < 4.99);
        let highest = highest.map(|p| p[1]).fold(0.0f32, f32::max);
        assert_eq!(highest, 1.5);
        assert!(is_ground(&faces, 2) && !hides(&faces, 2), "stood on, but hiding nothing whole");
    }

    #[test]
    fn a_fence_joins_fences_and_whole_blocks_and_collides_a_block_and_a_half_high() {
        let faces = world_of(&[(BlockShape::Fence, BlockState::default())]);
        let mut map = BlockMap::default();
        map.set([2, 1, 2], 2);
        map.set([3, 1, 2], 2);
        map.set([2, 1, 1], 1);
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        let first: Vec<_> = parts_of(&mesh).into_iter().filter(|quad| quad.at == [2, 1, 2]).collect();
        // A post and two rails each way it joins: east to the fence, north to the stone.
        let rails_east = first.iter().filter(|quad| quad.facing == 0 && quad.plane == 16).count();
        let rails_north = first.iter().filter(|quad| quad.facing == 5 && quad.plane == 0).count();
        assert_eq!((rails_east, rails_north), (0, 0), "a rail's end against what it joins is hidden");
        let reaching_east = first.iter().filter(|quad| quad.facing == 2 && quad.up == [10, 16]).count();
        assert_eq!(reaching_east, 2, "two rails reach east: {first:?}");
        assert!(mesh.colliders.iter().any(|[low, high]| low[0] == 2.375 && high[1] == 2.5), "{:?}", mesh.colliders);
        assert!(super::super::path::can_stand(&map, &faces, [2, 2, 1], 2), "stood on the stone");
        assert!(!super::super::path::can_stand(&map, &faces, [3, 2, 2], 2), "nothing stands on a fence");
    }

    #[test]
    fn an_open_door_collides_against_its_hinge_side_and_is_walked_through() {
        let shut = BlockState { facing: BlockFacing::North, half: BlockHalf::Lower, hinge: DoorHinge::Left, ..BlockState::default() };
        let faces = world_of(&[(BlockShape::Door, shut), (BlockShape::Door, BlockState { open: true, ..shut })]);
        assert!(is_ground(&faces, 2) && !is_ground(&faces, 3), "a shut door blocks the way, an open one does not");
        assert_eq!(faces[3].family, 3, "each its own block here");
        for (id, (low, high)) in [(2u16, ([0.0, 0.0], [1.0, 0.1875])), (3, ([0.0, 0.0], [0.1875, 1.0]))] {
            let mut map = BlockMap::default();
            map.set([0, 0, 0], id);
            let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
            let xs = mesh.positions.iter().map(|p| p[0]);
            let zs = mesh.positions.iter().map(|p| p[2]);
            let span = |values: Vec<f32>| [values.iter().copied().fold(f32::MAX, f32::min), values.iter().copied().fold(f32::MIN, f32::max)];
            let (x, z) = (span(xs.collect()), span(zs.collect()));
            assert!((x[0] - low[0]).abs() < 1e-4 && (x[1] - high[0]).abs() < 1e-4, "{id}: x {x:?}");
            assert!((z[0] - low[1]).abs() < 1e-4 && (z[1] - high[1]).abs() < 1e-4, "{id}: z {z:?}");
            let parts = parts_of(&mesh);
            assert!(parts.iter().all(|quad| quad.moves.is_some() && quad.motion > 0 && quad.open == (id == 3)));
        }
    }

    #[test]
    fn a_chain_counts_how_far_it_hangs_and_one_standing_free_does_not_sway() {
        let faces = world_of(&[(BlockShape::Chain, BlockState::default()), (BlockShape::Lever, BlockState::default_for(BlockShape::Lever))]);
        let mut map = BlockMap::default();
        map.set([1, 10, 1], 1);
        map.set([1, 9, 1], 2);
        map.set([1, 8, 1], 2);
        map.set([6, 1, 6], 2);
        let mesh = mesh_chunk(&map, [0, 0, 0], &faces);
        let hangs = |at: [u32; 3]| parts_of(&mesh).into_iter().find(|quad| quad.at == at).map(|quad| quad.hangs);
        assert_eq!([hangs([1, 9, 1]), hangs([1, 8, 1]), hangs([6, 1, 6])], [Some(1), Some(2), Some(0)]);
        assert!(!is_ground(&faces, 2) && !is_ground(&faces, 3), "chains and levers are walked through");
        assert!(super::super::keeps_water_out(&faces, 3), "but water keeps out of them");
        assert!(mesh.positions.iter().all(|p| !(p[0] > 1.0 && p[0] < 2.0 && p[1] > 8.0 && p[1] < 10.0)), "no collision");
    }

    #[test]
    fn stairs_turn_a_corner_where_another_meets_them_from_the_side() {
        let north = BlockState::default();
        let east = BlockState { facing: BlockFacing::East, ..north };
        let faces = world_of(&[(BlockShape::Stairs, north), (BlockShape::Stairs, east)]);
        let mut map = BlockMap::default();
        map.set([4, 0, 4], 2);
        map.set([4, 0, 3], 3);
        let around = around_of(&faces, &|p| map.get(p), [4, 0, 4], &faces[2]);
        assert_eq!(around.corner, StairCorner::Outer(BlockFacing::East));
        map.set([4, 0, 3], AIR);
        map.set([4, 0, 5], 3);
        assert_eq!(around_of(&faces, &|p| map.get(p), [4, 0, 4], &faces[2]).corner, StairCorner::Inner(BlockFacing::East));
        // A part-only chunk is still made, and a world of cubes alone makes no part quads.
        assert!(!mesh_chunk(&map, [0, 0, 0], &faces).is_empty());
        let mut cubes = BlockMap::default();
        cubes.set([0, 0, 0], 1);
        assert_eq!(mesh_chunk(&cubes, [0, 0, 0], &faces).parts().1, 0);
    }
}
