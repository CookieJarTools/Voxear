//! Block worlds: ground of blocks, a block a cube, each of a kind its
//! world's file lists (see `file`), from big blocks to build with to small
//! ones to sculpt with.
//!
//! A **Block World** on an Obj reaches from the Obj's position along +x, +y
//! and +z, a block `block_size` metres a side, and down and back as far as
//! its blocks go. Its blocks are kept in chunks of `CHUNK` a side
//! ([`chunk`]), only those with blocks in them, by where they are, so a
//! world is as large as its blocks and no larger. Each chunk's faces are
//! worked out apart ([`mesh`]): what the renderer draws, physics stands
//! things on and the GI's rays see.
//!
//! A world drawn smooth (a **Voxel Terrain**) keeps how full each block is
//! where the ground's surface passes, and its surface runs smoothly between
//! its blocks' middles ([`smooth`]) in place of their cubes' faces.
//!
//! A kind of block can be water (`file::BlockKind::water`): its faces show
//! only where it meets air, and are drawn apart, see-through, after the
//! ground; the ground's faces show where they meet it as they do air.
//! Nothing stands on it, and things float in it ([`BlockWorld::water_over`]).
//! It flows ([`flow`]): from water placed, down and then out sideways,
//! falling towards a planet's middle. A Voxel Terrain's water is smooth
//! too, its surface made as its ground's is (see `smooth`).

pub mod chunk;
pub mod edits;
pub mod file;
pub mod flow;
pub mod generate;
pub mod mesh;
pub mod sight;
pub mod smooth;
pub mod stream;
pub mod vox;

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use voxear_ecs::Component;

use chunk::{AIR, BlockId, CHUNK, Chunk, EMPTY, FULL, Fill};
use mesh::ChunkMesh;

/// How a kind of block's faces are drawn, by its number in its world (see
/// `file::BlockFile::faces`): the surfaces its top, its sides and its
/// bottom are drawn with, and whether it is water.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockFaces {
    pub top: u8,
    pub sides: u8,
    pub bottom: u8,
    pub water: bool,
}

impl BlockFaces {
    /// A kind of ground drawn with these surfaces.
    pub const fn new(top: u8, sides: u8, bottom: u8) -> Self {
        Self { top, sides, bottom, water: false }
    }

    /// Water drawn with surface `surface`.
    pub const fn water(surface: u8) -> Self {
        Self { top: surface, sides: surface, bottom: surface, water: true }
    }
}

/// Whether block `block` is water, by `faces`.
pub fn is_water(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.water)
}

/// Whether block `block` is ground: neither air nor water.
pub fn is_ground(faces: &[BlockFaces], block: BlockId) -> bool {
    block != AIR && !is_water(faces, block)
}

/// Every block of a world, by chunk: chunks with no block in them are not
/// kept, and a chunk is shared, so a change to one leaves the rest as they
/// were.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockMap {
    chunks: HashMap<[i32; 3], Arc<Chunk>>,
}

impl BlockMap {
    /// The chunk block `at` is in, and where in it.
    pub fn chunk_of(at: [i32; 3]) -> ([i32; 3], [u32; 3]) {
        let side = CHUNK as i32;
        (at.map(|v| v.div_euclid(side)), at.map(|v| v.rem_euclid(side) as u32))
    }

    /// The block at `at`, counted in blocks from the world's corner.
    pub fn get(&self, at: [i32; 3]) -> BlockId {
        let (key, [x, y, z]) = Self::chunk_of(at);
        self.chunks.get(&key).map_or(AIR, |chunk| chunk.get(x, y, z))
    }

    /// Puts `block` at `at`, its chunk copied if it is shared, made if it is
    /// new, and let go once it is all air.
    pub fn set(&mut self, at: [i32; 3], block: BlockId) {
        let (key, [x, y, z]) = Self::chunk_of(at);
        if block == AIR && !self.chunks.contains_key(&key) {
            return;
        }
        let chunk = self.chunks.entry(key).or_insert_with(|| Arc::new(Chunk::filled(AIR)));
        Arc::make_mut(chunk).set(x, y, z, block);
        if block == AIR && chunk.is_air() {
            self.chunks.remove(&key);
        }
    }

    /// How full the block at `at` is (see [`Fill`]).
    pub fn fill(&self, at: [i32; 3]) -> Fill {
        let (key, [x, y, z]) = Self::chunk_of(at);
        self.chunks.get(&key).map_or(EMPTY, |chunk| chunk.fill(x, y, z))
    }

    /// Puts `block` at `at`, `fill` full, as [`BlockMap::set`] does.
    pub fn set_cell(&mut self, at: [i32; 3], block: BlockId, fill: Fill) {
        let (key, [x, y, z]) = Self::chunk_of(at);
        if block == AIR && fill == EMPTY && !self.chunks.contains_key(&key) {
            return;
        }
        let chunk = self.chunks.entry(key).or_insert_with(|| Arc::new(Chunk::filled(AIR)));
        Arc::make_mut(chunk).set_cell(x, y, z, block, fill);
        if block == AIR && chunk.is_air() {
            self.chunks.remove(&key);
        }
    }

    /// Whether chunk `key` and the six beside it are all ground through and
    /// through, none of it water as `faces` says, so it has no faces to draw:
    /// as deep in a planet.
    pub fn buried(&self, key: [i32; 3], faces: &[BlockFaces]) -> bool {
        let whole = |key: [i32; 3]| {
            self.chunks
                .get(&key)
                .is_some_and(|chunk| chunk.is_whole() && !chunk.may_hold(|block| is_water(faces, block)))
        };
        let beside = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
        whole(key) && beside.iter().all(|step| whole([0, 1, 2].map(|i| key[i] + step[i])))
    }

    /// Chunk `key`, if it has blocks.
    pub fn chunk(&self, key: [i32; 3]) -> Option<&Arc<Chunk>> {
        self.chunks.get(&key)
    }

    /// Puts a whole chunk at `key`, or takes it away when it is all air.
    pub fn put_chunk(&mut self, key: [i32; 3], chunk: Chunk) {
        if chunk.is_air() {
            self.chunks.remove(&key);
        } else {
            self.chunks.insert(key, Arc::new(chunk));
        }
    }

    /// Every chunk with blocks, by where it is.
    pub fn chunks(&self) -> impl Iterator<Item = (&[i32; 3], &Arc<Chunk>)> {
        self.chunks.iter()
    }

    /// Puts a chunk shared with another map at `key`, as it is.
    pub fn put_shared(&mut self, key: [i32; 3], chunk: Arc<Chunk>) {
        self.chunks.insert(key, chunk);
    }

    /// How many chunks have blocks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Whether no chunk has blocks.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// The same blocks, each by the number `change` gives it: as after a
    /// kind of block is taken out of its world's list.
    pub fn remapped(&self, change: impl Fn(BlockId) -> BlockId) -> Self {
        let mut map = Self::default();
        for (key, chunk) in &self.chunks {
            map.put_chunk(*key, chunk.remapped(&change));
        }
        map
    }

    /// Each chunk's faces, worked out on every core, each block drawn with
    /// the surfaces `faces` gives it (see `file::BlockFile::faces`).
    pub fn mesh_all(&self, faces: &[BlockFaces], smooth: bool) -> HashMap<[i32; 3], Arc<ChunkMesh>> {
        voxear_core::profile_scope!("Block Meshing");
        let keys: Vec<[i32; 3]> = self.chunks.keys().copied().collect();
        let meshes = crate::workers::each(&keys, |&key| mesh_any(self, key, faces, smooth));
        keys.into_iter().zip(meshes).filter(|(_, mesh)| !mesh.is_empty()).map(|(k, m)| (k, Arc::new(m))).collect()
    }
}

/// A world's chunks' faces, by chunk.
pub type ChunkMeshes = HashMap<[i32; 3], Arc<ChunkMesh>>;

/// A world of blocks: its blocks, each chunk's faces, the surfaces each
/// block's top, sides and bottom are drawn with and which are water (see
/// `file::BlockFile::faces`), how large a block is in metres, whether it is
/// drawn smooth (see `smooth`), and whether it casts the sun's shadows.
#[derive(Clone, Debug)]
pub struct BlockWorld {
    pub blocks: Arc<BlockMap>,
    pub meshes: Arc<HashMap<[i32; 3], Arc<ChunkMesh>>>,
    pub faces: Arc<[BlockFaces]>,
    /// For a world without end, its coarser chunks' faces beyond its own, by
    /// level and place in that level's chunks (see `stream`): drawn alone.
    pub far: Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>,
    pub block_size: f32,
    pub smooth: bool,
    pub cast_shadows: bool,
    /// For a planet, how far its sea is from its middle, the world's corner,
    /// in metres: physics pulls things near it towards its middle, and its
    /// water falls towards it.
    pub planet: Option<f32>,
    /// The blocks its water's next step looks at (see [`flow`]): those near
    /// where its blocks changed since the last.
    pub flowing: Arc<BTreeSet<[i32; 3]>>,
}

impl Component for BlockWorld {}

impl PartialEq for BlockWorld {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.blocks, &other.blocks) || self.blocks == other.blocks)
            && (Arc::ptr_eq(&self.meshes, &other.meshes) || self.meshes == other.meshes)
            && self.faces == other.faces
            && (Arc::ptr_eq(&self.far, &other.far) || self.far == other.far)
            && self.block_size == other.block_size
            && self.smooth == other.smooth
            && self.cast_shadows == other.cast_shadows
            && self.planet == other.planet
            && (Arc::ptr_eq(&self.flowing, &other.flowing) || self.flowing == other.flowing)
    }
}

impl BlockWorld {
    /// A world of `blocks`, every chunk's faces worked out with `faces`.
    pub fn new(blocks: BlockMap, faces: &[BlockFaces], block_size: f32) -> Self {
        Self::made(blocks, faces, block_size, false)
    }

    /// A world of `blocks` drawn smooth or not as `smooth` says, every
    /// chunk's faces worked out with `faces`, and any water running in it
    /// set flowing on.
    pub fn made(blocks: BlockMap, faces: &[BlockFaces], block_size: f32, smooth: bool) -> Self {
        let meshes = blocks.mesh_all(faces, smooth);
        let flowing = running(&blocks, faces);
        Self {
            blocks: Arc::new(blocks),
            meshes: Arc::new(meshes),
            faces: faces.into(),
            far: Arc::default(),
            block_size,
            smooth,
            cast_shadows: true,
            planet: None,
            flowing: Arc::new(flowing),
        }
    }

    /// Moves its water on a step (see [`flow`]), from the blocks near where
    /// it changed since the last. How many blocks changed.
    pub fn flow(&mut self) -> usize {
        if self.flowing.is_empty() {
            return 0;
        }
        let near = std::mem::take(Arc::make_mut(&mut self.flowing));
        let changes = flow::step(&self.blocks, &self.faces, self.fall(), self.smooth, &near);
        self.set_cells(changes)
    }

    /// The block at `at`, a point in metres from the world's corner, and
    /// which block that is.
    pub fn block_at(&self, at: [f32; 3]) -> ([i32; 3], BlockId) {
        let block = at.map(|v| (v / self.block_size).floor() as i32);
        (block, self.blocks.get(block))
    }

    /// Which way its water falls: down, or towards its middle on a planet.
    pub fn fall(&self) -> flow::Fall {
        if self.planet.is_some() { flow::Fall::Middle } else { flow::Fall::Down }
    }

    /// How deep under its water `at` is, a point in metres from its corner,
    /// and the way up from the water there (a facing, see `flow::Fall::up`):
    /// where `at` is in its water, or over it within `reach` metres of air,
    /// how far the top of the water in its column along that way is above
    /// it, in metres, less than nothing over the water. `None` where no
    /// water is there, or ground is between.
    pub fn water_depth(&self, at: [f32; 3], reach: f32) -> Option<(f32, usize)> {
        let (mut block, kind) = self.block_at(at);
        let up = self.fall().up(block);
        let (axis, toward) = (up / 2, if up & 1 == 0 { 1 } else { -1 });
        let water = |block: [i32; 3]| is_water(&self.faces, self.blocks.get(block));
        if !water(block) {
            if kind != AIR {
                return None;
            }
            // Down through the air to the water's top, if it comes before the ground and within reach.
            let last = ((at[axis] - toward as f32 * reach.max(0.0)) / self.block_size).floor() as i32;
            loop {
                block[axis] -= toward;
                if (block[axis] - last) * toward < 0 || self.blocks.get(block) != AIR && !water(block) {
                    return None;
                }
                if water(block) {
                    break;
                }
            }
        }
        // Up to the top of the water above it, as high as it stands in its block.
        loop {
            let mut next = block;
            next[axis] += toward;
            if !water(next) {
                break;
            }
            block = next;
        }
        // Drawn smooth, its surface stands where the smooth water's does.
        let fill = self.blocks.fill(block);
        let height = match self.smooth {
            true => 0.5 + flow::smooth_surface(fill),
            false => f32::from(flow::Water::of(fill).height().0) / 16.0,
        };
        let foot = if toward > 0 { block[axis] } else { block[axis] + 1 } as f32;
        let top = (foot + toward as f32 * height) * self.block_size;
        Some(((top - at[axis]) * toward as f32, up))
    }

    /// How high the water over `at` stands, in metres from the world's
    /// corner, where `at` (a point in metres from its corner) is in its
    /// water or over it within `reach` metres of air, and up is up: the top
    /// of the water in its column (see [`BlockWorld::water_depth`]). `None`
    /// where no water is there, ground is between, or, on a planet's sides,
    /// its water's up is another way.
    pub fn water_over(&self, at: [f32; 3], reach: f32) -> Option<f32> {
        let (depth, up) = self.water_depth(at, reach)?;
        (up == 2).then_some(at[1] + depth)
    }

    /// Puts each block of `changes` in place, each by where it is in blocks
    /// from the world's corner, and makes the faces again of every chunk a
    /// change reaches: its own, and those beside it whose faces it meets or
    /// shades. How many blocks changed.
    pub fn set_blocks(&mut self, changes: impl IntoIterator<Item = ([i32; 3], BlockId)>) -> usize {
        self.set_cells(changes.into_iter().map(|(at, block)| (at, block, if block == AIR { EMPTY } else { FULL })))
    }

    /// Puts each block of `changes` in place, as [`BlockWorld::set_blocks`]
    /// does, each as full as it says (see [`Fill`]); water near them flows
    /// from the next step (see [`BlockWorld::flow`]).
    pub fn set_cells(&mut self, changes: impl IntoIterator<Item = ([i32; 3], BlockId, Fill)>) -> usize {
        let mut changed = 0;
        let mut reached: Vec<[i32; 3]> = Vec::new();
        let wet = self.faces.iter().any(|faces| faces.water);
        let mut flowing: Vec<[i32; 3]> = Vec::new();
        let blocks = Arc::make_mut(&mut self.blocks);
        // How far into a chunk from either side a block is in the next chunk's padded blocks.
        let (first, last) = if self.smooth { (smooth::REACH_AFTER, smooth::REACH_BELOW) } else { (0, CHUNK - 1) };
        for (at, block, fill) in changes {
            if blocks.get(at) == block && blocks.fill(at) == fill {
                continue;
            }
            blocks.set_cell(at, block, fill);
            changed += 1;
            if wet {
                flowing.extend(flow::reached(at));
            }
            // A block on a chunk's side is in the next chunk's padded blocks too.
            let (key, local) = BlockMap::chunk_of(at);
            let near = |axis: usize| -> Vec<i32> {
                match local[axis] {
                    l if l <= first => vec![key[axis] - 1, key[axis]],
                    l if l >= last => vec![key[axis], key[axis] + 1],
                    _ => vec![key[axis]],
                }
            };
            for x in near(0) {
                for y in near(1) {
                    for z in near(2) {
                        reached.push([x, y, z]);
                    }
                }
            }
        }
        reached.sort_unstable();
        reached.dedup();
        self.remesh(&reached);
        if !flowing.is_empty() {
            Arc::make_mut(&mut self.flowing).extend(flowing);
        }
        changed
    }

    /// Makes chunks `keys`' faces again, on every core, from the blocks as
    /// they are now.
    pub fn remesh(&mut self, keys: &[[i32; 3]]) {
        voxear_core::profile_scope!("Block Meshing");
        let blocks = &self.blocks;
        let faces = &self.faces;
        let smooth = self.smooth;
        let made = crate::workers::each(keys, |&key| mesh_any(blocks, key, faces, smooth));
        let meshes = Arc::make_mut(&mut self.meshes);
        for (key, mesh) in keys.iter().zip(made) {
            if mesh.is_empty() {
                meshes.remove(key);
            } else {
                meshes.insert(*key, Arc::new(mesh));
            }
        }
    }

    /// The same blocks drawn with other `faces`, smooth or not as `smooth`
    /// says, every chunk's faces made again: as after its kinds of block
    /// changed how they are drawn.
    pub fn with_faces(&self, faces: &[BlockFaces], smooth: bool) -> Self {
        let meshes = self.blocks.mesh_all(faces, smooth);
        Self { meshes: Arc::new(meshes), faces: faces.into(), smooth, ..self.clone() }
    }
}

/// The blocks of water running in `map` (see [`flow`]): every block of it
/// that is not a source, and so may be fed no longer, or feed more.
fn running(map: &BlockMap, faces: &[BlockFaces]) -> BTreeSet<[i32; 3]> {
    let mut found = BTreeSet::new();
    for (key, chunk) in map.chunks() {
        if !chunk.has_fills() || !chunk.may_hold(|block| is_water(faces, block)) {
            continue;
        }
        for y in 0..CHUNK {
            for z in 0..CHUNK {
                for x in 0..CHUNK {
                    let water = is_water(faces, chunk.get(x, y, z));
                    if water && flow::Water::of(chunk.fill(x, y, z)) != flow::Water::Source {
                        let at = [x, y, z].map(|v| v as i32);
                        found.insert(std::array::from_fn(|i| key[i] * CHUNK as i32 + at[i]));
                    }
                }
            }
        }
    }
    found
}

/// Chunk `key`'s faces in `map`, drawn smooth or not as `smooth` says.
pub fn mesh_any(map: &BlockMap, key: [i32; 3], faces: &[BlockFaces], smooth: bool) -> ChunkMesh {
    match smooth {
        true => smooth::mesh_chunk(map, key, faces),
        false => mesh::mesh_chunk(map, key, faces),
    }
}

/// A world of blocks as its level built it, kept once a script first
/// changes it, so its changes since can be saved as only what differs (see
/// `edits::BlockEdits`).
#[derive(Clone, Debug)]
pub struct BuiltBlocks(pub Arc<BlockMap>);

impl Component for BuiltBlocks {}

/// Where a ray met a block: which block, the way the face it met faces, and
/// how far along the ray, in blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHit {
    pub block: [i32; 3],
    pub normal: [i32; 3],
    pub distance: f32,
}

/// The first block a ray from `origin` along `direction`, both in blocks
/// from the world's corner, meets within `max_distance` blocks: stepping
/// from block to block along it, through every block it passes.
pub fn raycast(blocks: &BlockMap, origin: [f32; 3], direction: [f32; 3], max_distance: f32) -> Option<BlockHit> {
    let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length <= 0.0 {
        return None;
    }
    let direction = direction.map(|v| v / length);
    let mut at = origin.map(|v| v.floor() as i32);
    let step = direction.map(|v| if v > 0.0 { 1 } else { -1 });
    // How far along the ray each crossing of a block's side is, along each axis, and between crossings.
    let mut next = [0.0f32; 3];
    let mut each = [f32::INFINITY; 3];
    for axis in 0..3 {
        if direction[axis] != 0.0 {
            each[axis] = (1.0 / direction[axis]).abs();
            let side = if step[axis] > 0 { at[axis] as f32 + 1.0 } else { at[axis] as f32 };
            next[axis] = (side - origin[axis]) / direction[axis];
        } else {
            next[axis] = f32::INFINITY;
        }
    }
    let mut normal = [0; 3];
    let mut distance = 0.0;
    loop {
        if blocks.get(at) != AIR {
            return Some(BlockHit { block: at, normal, distance });
        }
        let axis = if next[0] < next[1] {
            if next[0] < next[2] { 0 } else { 2 }
        } else if next[1] < next[2] {
            1
        } else {
            2
        };
        distance = next[axis];
        if distance > max_distance {
            return None;
        }
        at[axis] += step[axis];
        next[axis] += each[axis];
        normal = [0; 3];
        normal[axis] = -step[axis];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_kept_by_chunk_on_either_side_of_the_corner_and_let_go_once_air() {
        let mut map = BlockMap::default();
        map.set([-1, 0, 40], 3);
        assert_eq!(BlockMap::chunk_of([-1, 0, 40]), ([-1, 0, 1], [31, 0, 8]));
        assert_eq!((map.get([-1, 0, 40]), map.get([0, 0, 40])), (3, AIR));
        let shared = map.clone();
        map.set([-2, 0, 40], 4);
        assert_eq!(shared.get([-2, 0, 40]), AIR, "a copy keeps its chunk as it was");
        map.set([-1, 0, 40], AIR);
        map.set([-2, 0, 40], AIR);
        assert_eq!(map.chunks().count(), 0);
    }

    #[test]
    fn a_change_makes_its_chunks_faces_again_and_those_beside_it_where_it_meets_them() {
        let file = file::BlockFile::default();
        let (_, faces) = file.faces();
        let mut world = BlockWorld::new(file.make_ground(None), &faces, 1.0);
        let before = world.meshes.clone();
        // A block dug out of the middle of chunk (0, 0, 0): its faces alone made again.
        assert_eq!(world.set_blocks([([10, 7, 10], AIR)]), 1);
        assert!(!Arc::ptr_eq(&world.meshes[&[0, 0, 0]], &before[&[0, 0, 0]]));
        assert!(Arc::ptr_eq(&world.meshes[&[1, 0, 0]], &before[&[1, 0, 0]]));
        // One dug on its +x side: the next chunk's too, whose side it opens.
        let before = world.meshes.clone();
        world.set_blocks([
            ([31, 3, 10], AIR),
            ([31, 4, 10], AIR),
            ([31, 5, 10], AIR),
            ([31, 6, 10], AIR),
            ([31, 7, 10], AIR),
        ]);
        assert!(!Arc::ptr_eq(&world.meshes[&[1, 0, 0]], &before[&[1, 0, 0]]));
        let opened = world.meshes[&[1, 0, 0]].quads.iter().any(|&q| {
            let (at, _, facing, ..) = mesh::unpack(q);
            facing == 1 && at[0] == 0 && at[2] == 10
        });
        assert!(opened, "the next chunk's block beside the hole shows its -x face");
        // The same block again changes nothing.
        assert_eq!(world.set_blocks([([31, 7, 10], AIR)]), 0);
        // A block placed above the ground in a chunk of its own.
        world.set_blocks([([5, 40, 5], 3)]);
        assert!(world.meshes.contains_key(&[0, 1, 0]));
        world.set_blocks([([5, 40, 5], AIR)]);
        assert!(!world.meshes.contains_key(&[0, 1, 0]), "let go once empty");
        assert_eq!(world.block_at([5.5, 7.5, 10.2]), ([5, 7, 10], 1));
    }

    #[test]
    fn water_stands_as_high_as_the_top_of_its_column() {
        let mut file = file::BlockFile::default();
        file.blocks.push(file::BlockKind { water: true, ..file::BlockKind::colored("Water", [0.1, 0.4, 0.5]) });
        let (_, faces) = file.faces();
        let mut map = BlockMap::default();
        for y in 0..5 {
            map.set([0, y, 0], if y < 2 { 3 } else { 4 });
        }
        let world = BlockWorld::new(map, &faces, 2.0);
        assert_eq!(world.water_over([1.0, 5.0, 1.0], 1.0), Some(10.0), "in it, its top");
        assert_eq!(world.water_over([1.0, 10.5, 1.0], 1.0), Some(10.0), "just over it");
        assert_eq!(world.water_over([1.0, 13.0, 1.0], 1.0), None, "too far over it");
        assert_eq!(world.water_over([1.0, 1.0, 1.0], 1.0), None, "in the ground under it");
        assert_eq!(world.water_over([3.0, 5.0, 1.0], 1.0), None, "beside it");
        // On a planet's side, water's up is out from its middle: a point 1.5 m out over water whose top is 10 m out.
        let mut side = BlockMap::default();
        for x in 0..5 {
            side.set([x, 0, 0], if x < 3 { 3 } else { 4 });
        }
        let mut planet = BlockWorld::new(side, &faces, 2.0);
        planet.planet = Some(4.0);
        assert_eq!(planet.water_depth([11.5, 1.0, 1.0], 2.0), Some((-1.5, 0)));
        assert_eq!(planet.water_depth([7.0, 1.0, 1.0], 2.0), Some((3.0, 0)), "in it, 3 m under its top");
        assert_eq!(planet.water_over([7.0, 1.0, 1.0], 2.0), None, "its up is not the world's");
    }

    #[test]
    fn a_ray_meets_the_first_block_in_its_way_and_the_face_it_came_through() {
        let mut map = BlockMap::default();
        map.set([5, 2, -3], 1);
        let hit = raycast(&map, [0.5, 2.5, -2.5], [1.0, 0.0, 0.0], 100.0).expect("the block");
        assert_eq!((hit.block, hit.normal), ([5, 2, -3], [-1, 0, 0]));
        assert!((hit.distance - 4.5).abs() < 1e-5);
        let down = raycast(&map, [5.5, 40.0, -2.5], [0.0, -1.0, 0.0], 100.0).expect("from above");
        assert_eq!(down.normal, [0, 1, 0]);
        assert!(raycast(&map, [0.5, 2.5, -2.5], [1.0, 0.0, 0.0], 3.0).is_none(), "too far");
        assert!(raycast(&map, [0.5, 2.5, -2.5], [-1.0, 0.2, 0.0], 100.0).is_none());
    }
}
