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
//! A kind of block can be see-through (`file::BlockKind::cutout`): its faces
//! are drawn from a picture with holes in it, as leaves are, so what is
//! behind it shows. Blocks beside it keep their faces towards it, it keeps
//! its own towards another of its kind, and it is ground in every other
//! way: stood on, built on, dug.
//!
//! A kind of block can be water (`file::BlockKind::water`): its faces show
//! only where it meets air, and are drawn apart, see-through, after the
//! ground; the ground's faces show where they meet it as they do air.
//! Nothing stands on it, and things float in it ([`BlockWorld::water_over`]).
//! It flows ([`flow`]): from water placed, down and then out sideways. A
//! Voxel Terrain's water is smooth too, its surface made as its ground's
//! is (see `smooth`).

pub mod block;
pub mod chunk;
pub mod edits;
pub mod fell;
pub mod file;
pub mod flow;
pub mod generate;
pub mod grow;
pub mod kept;
pub mod light;
pub mod area_layer;
pub mod map;
pub mod map_known;
pub mod mesh;
pub mod path;
pub mod plant;
pub mod road_layer;
pub mod sapling;
pub mod shape;
pub mod sight;
pub mod smooth;
pub mod dungeon;
pub mod structure;
pub mod stream;
pub mod usable;
pub mod vox;
pub mod world_layers;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use voxear_ecs::Component;

use chunk::{AIR, BlockId, CHUNK, Chunk, EMPTY, FULL, Fill};
use mesh::ChunkMesh;

/// How a kind of block's faces are drawn, by its number in its world (see
/// `file::BlockFile::faces`): the surfaces its top, its sides and its
/// bottom are drawn with, whether it is water, whether it is seen through
/// where its faces' pictures are clear, whether it sways in the wind as
/// leaves do, its shape (a cube, a cube with tufts of leaves, or a vine),
/// and its own colour, sRGB a byte each, which water takes from in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockFaces {
    pub top: u8,
    pub sides: u8,
    pub bottom: u8,
    pub water: bool,
    pub see_through: bool,
    pub sways: bool,
    /// Which of its faces join those of its own kind beside it into one
    /// surface (see `mesh`), a bit each as `file::JOINS_TOP`, `JOINS_SIDES`
    /// and `JOINS_BOTTOM`: such a face is a quad of its own that says which
    /// of the faces round it it joins. With any, no face shows between two
    /// of it.
    pub connects: u8,
    /// Which of its faces can take another block's picture over their
    /// edge, a bit each as `connects` has them: such a face beside a block
    /// with a higher `overlap` is a quad of its own, drawn with its look's
    /// twin (see `mesh`).
    pub mixes: u8,
    /// How far up the order of blocks that meet it lies: its picture creeps
    /// over the edge of a block beside it with a lower one.
    pub overlap: u8,
    /// The block it is drawn as while a block covers its top (see
    /// [`covers_below`]), by its number: grass shows as earth under a
    /// block. `AIR` for itself always.
    pub covered: BlockId,
    pub shape: block::BlockShape,
    pub color: [u8; 3],
    /// Its placed state, as a byte (see `shape::BlockState`): each state of
    /// a block a world holds is its own number.
    pub state: u8,
    /// The number of the first entry of its world naming the same block,
    /// in whatever state: the same for every state of one block.
    pub family: BlockId,
    /// Its motion and swing among its world's (`file::BlockFile::motions`),
    /// counted from 1; 0 for none.
    pub motion: u8,
    /// Which of its sides its parts cover whole, a bit each as a chunk
    /// keeps its faces (see `shape::covers`).
    pub covers: u8,
    /// What it is as a block of parts, a bit each: [`PART_DRAWN`],
    /// [`PART_PASSES`], [`PART_TALL`], [`PART_SOLID`], [`PART_HANGS`] and
    /// [`PART_USED`].
    pub part: u8,
    /// A block of its own boxes' boxes.
    pub custom: Option<shape::CustomShape>,
    /// How its moving parts rest in its state, by its motion: turned, in
    /// tenths of a degree, and moved, in sixty-fourths of a block.
    pub rest: [i16; 2],
    /// How long its motion lasts, in thousandths of a second, and how long
    /// a button stays on once pressed, in hundredths.
    pub travel: u16,
    pub press: u16,
    /// How much light it gives, from 0 for none to 15 (see [`light`]).
    pub light: u8,
    /// What ground it makes for navigation: 0 as it is, or an area's number
    /// one more (see `block::BlockNavArea::number`).
    pub nav_area: u8,
    /// Whether navigation climbs it as a ladder (see
    /// `block::BlockDef::climbable`); vines, ropes and chains are climbed
    /// by their shape.
    pub climbable: bool,
}

/// A block drawn as parts, a few boxes, in place of a cube.
pub const PART_DRAWN: u8 = 1;
/// A block walked through: a lever, a chain, an open door.
pub const PART_PASSES: u8 = 2;
/// A block that collides a block and a half high: a fence, a wall.
pub const PART_TALL: u8 = 4;
/// A block of parts that collides at all.
pub const PART_SOLID: u8 = 8;
/// A block that hangs from what is over it, and sways.
pub const PART_HANGS: u8 = 16;
/// A block a player can use.
pub const PART_USED: u8 = 32;

impl BlockFaces {
    /// A kind of ground drawn with these surfaces.
    pub const fn new(top: u8, sides: u8, bottom: u8) -> Self {
        let (water, see_through, sways, shape) = (false, false, false, block::BlockShape::Cube);
        let (connects, mixes, overlap, covered) = (0, 0, 0, AIR);
        let (state, family, motion, covers, part, custom, rest) = (0, 0, 0, 0, 0, None, [0; 2]);
        let (travel, press) = (0, 0);
        Self {
            top,
            sides,
            bottom,
            water,
            see_through,
            sways,
            connects,
            mixes,
            overlap,
            covered,
            shape,
            color: [0; 3],
            state,
            family,
            motion,
            covers,
            part,
            custom,
            rest,
            travel,
            press,
            light: 0,
            nav_area: 0,
            climbable: false,
        }
    }

    /// Water drawn with surface `surface`.
    pub const fn water(surface: u8) -> Self {
        let (top, sides, bottom) = (surface, surface, surface);
        Self { water: true, ..Self::new(top, sides, bottom) }
    }
}

/// Whether block `block` is water, by `faces`.
pub fn is_water(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.water)
}

/// Whether block `block` is ground: neither air nor water, nor a vine,
/// which hangs in the air and is walked through, nor a block of parts that
/// is walked through, as a lever or an open door is.
pub fn is_ground(faces: &[BlockFaces], block: BlockId) -> bool {
    block != AIR
        && !faces
            .get(usize::from(block))
            .is_some_and(|faces| faces.water || faces.shape.is_vine() || faces.part & PART_PASSES != 0)
}

/// Whether block `block` is drawn as parts, a few boxes, in place of a
/// cube (see `shape`).
pub fn is_part(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.part & PART_DRAWN != 0 && !faces.water)
}

/// What block `block` is as a block of parts (see [`BlockFaces::part`]);
/// 0 for a cube.
pub fn part_of(faces: &[BlockFaces], block: BlockId) -> u8 {
    faces.get(usize::from(block)).filter(|faces| !faces.water).map_or(0, |faces| faces.part)
}

/// Whether water flowing keeps out of block `block`: ground, and every
/// block of parts, those walked through too.
pub fn keeps_water_out(faces: &[BlockFaces], block: BlockId) -> bool {
    is_ground(faces, block) || is_part(faces, block)
}

/// `faces` as a world's far rings draw them, their blocks much larger than
/// its own: a block of parts that collides, a slab, a stair, a wall, as a
/// whole cube, and any other as air (see [`far_block`]).
pub fn far_faces(faces: &[BlockFaces]) -> Vec<BlockFaces> {
    faces
        .iter()
        .map(|kind| match kind.part & PART_DRAWN != 0 && !kind.water {
            true => BlockFaces { shape: block::BlockShape::Cube, part: 0, covers: 0, custom: None, ..*kind },
            false => *kind,
        })
        .collect()
}

/// Block `block` as a world's far rings hold it: air for a block of parts
/// that is too thin to see from so far, as a lever, a pane or a door is.
pub fn far_block(faces: &[BlockFaces], block: BlockId) -> BlockId {
    let part = part_of(faces, block);
    let thin = part & PART_DRAWN != 0 && (part & (PART_PASSES | PART_HANGS) != 0 || part & PART_SOLID == 0);
    let thin = thin
        || faces.get(usize::from(block)).is_some_and(|kind| {
            matches!(kind.shape, block::BlockShape::Pane | block::BlockShape::Fence | block::BlockShape::Door)
                && part & PART_DRAWN != 0
        });
    if thin { AIR } else { block }
}

/// Which of block `block`'s sides are covered whole, a bit each as a chunk
/// keeps its faces: every side of a cube that hides, and those its parts
/// cover of a block of parts.
pub fn covers(faces: &[BlockFaces], block: BlockId) -> u8 {
    match faces.get(usize::from(block)) {
        Some(kind) if kind.part & PART_DRAWN != 0 && !kind.water => kind.covers,
        _ if hides(faces, block) => 0b11_1111,
        _ => 0,
    }
}

/// Whether block `block` is a vine: drawn as sheets on the ground beside
/// it, or hanging, and no ground itself.
pub fn is_vine(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.shape.is_vine() && !faces.water)
}

/// Whether block `block` is a cube with tufts of leaves reaching out of it.
pub fn is_bushy(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.shape == block::BlockShape::Bushy && !faces.water)
}

/// Whether block `block` is ground seen through, as leaves are.
pub fn is_see_through(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.see_through && !faces.water)
}

/// Whether block `block` is ground that sways in the wind, as leaves do.
pub fn sways(faces: &[BlockFaces], block: BlockId) -> bool {
    faces.get(usize::from(block)).is_some_and(|faces| faces.sways && !faces.water)
}

/// Which of block `block`'s faces join those of its own kind beside it, a
/// bit each (see [`BlockFaces::connects`]); 0 for none.
pub fn connects(faces: &[BlockFaces], block: BlockId) -> u8 {
    faces.get(usize::from(block)).filter(|faces| !faces.water).map_or(0, |faces| faces.connects)
}

/// Whether block `block` hides what is behind it: ground that is not seen
/// through. A face against such a block is never seen, and is not made.
pub fn hides(faces: &[BlockFaces], block: BlockId) -> bool {
    is_ground(faces, block) && !is_see_through(faces, block) && !is_part(faces, block)
}

/// The bit of a block's sides (see [`covers`]) that is its underside.
const UNDERSIDE: u8 = 1 << 3;

/// Whether block `block` covers the top of the block under it, so a block
/// that shows as another while covered (see [`BlockFaces::covered`]) does:
/// a whole block that hides, a block of parts whose underside is covered
/// whole (a slab lying low, a stair the right way up, a log), and water.
/// Air does not, nor what light and rain come through: leaves, glass,
/// vines, plants, a slab lying high, a fence.
pub fn covers_below(faces: &[BlockFaces], block: BlockId) -> bool {
    is_water(faces, block) || !is_bushy(faces, block) && covers(faces, block) & UNDERSIDE != 0
}

/// What block `block` is drawn as with `above` over it: the block it shows
/// as while covered where `above` covers it (see [`covers_below`]), else
/// itself.
pub fn shown_as(faces: &[BlockFaces], block: BlockId, above: BlockId) -> BlockId {
    match faces.get(usize::from(block)).map_or(AIR, |kind| kind.covered) {
        AIR => block,
        covered if covers_below(faces, above) => covered,
        _ => block,
    }
}

/// Whether any of `faces` shows as another block while covered.
pub fn any_covered(faces: &[BlockFaces]) -> bool {
    faces.iter().any(|kind| kind.covered != AIR)
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
    /// through, none of it water or seen through as `faces` says, so it has
    /// no faces to draw.
    pub fn buried(&self, key: [i32; 3], faces: &[BlockFaces]) -> bool {
        let whole = |key: [i32; 3]| {
            self.chunks.get(&key).is_some_and(|chunk| chunk.is_whole() && !chunk.may_hold(|block| !hides(faces, block)))
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
        self.mesh_lit(faces, smooth, None)
    }

    /// Each chunk's faces, as [`mesh_all`](Self::mesh_all) makes them, lit
    /// by `light` where it is given.
    pub fn mesh_lit(
        &self,
        faces: &[BlockFaces],
        smooth: bool,
        light: Option<&light::LightMap>,
    ) -> HashMap<[i32; 3], Arc<ChunkMesh>> {
        voxear_core::profile_scope!("Block Meshing");
        let keys: Vec<[i32; 3]> = self.chunks.keys().copied().collect();
        let light = light.map(|light| light as &dyn light::LightAt);
        let meshes = crate::workers::each(&keys, |&key| mesh_lit(self, key, faces, smooth, &|_| true, light));
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
    /// For a world without end, the chunk columns of its own made, by place
    /// along x and z (see `stream`): past them the world goes on, so its
    /// water shows no face towards them. `None` for a world of its blocks
    /// alone.
    pub columns: Option<Arc<HashSet<[i32; 2]>>>,
    /// For a world without end, the changes made to chunk columns not made
    /// yet: kept apart, as their blocks are not there to change, and put on
    /// their ground as it comes (see `stream`).
    pub unmade: Arc<edits::BlockEdits>,
    /// For a world without end seen from more places than one, as through
    /// Portals, what it shows from each (see [`BlockPlace`]): a view draws
    /// what is shown from the place nearest its camera. Empty, everything
    /// shows from anywhere.
    pub places: Arc<Vec<BlockPlace>>,
    /// For a world without end, how its rings fade into each other round
    /// where it is looked at from (see [`RingFades`]).
    pub fades: Arc<RingFades>,
    pub block_size: f32,
    pub smooth: bool,
    pub cast_shadows: bool,
    /// The blocks its water's next step looks at (see [`flow`]): those near
    /// where its blocks changed since the last.
    pub flowing: Arc<BTreeSet<[i32; 3]>>,
    /// Its links from each lever or button to the blocks it drives (see
    /// [`usable`]).
    pub links: Arc<usable::Links>,
    /// Its blocks moving by their motions now, and its buttons pressed.
    pub moving: Arc<Vec<usable::MovingBlock>>,
    pub pressed: Arc<Vec<usable::Pressed>>,
    /// Its own clock, in seconds, by which its blocks move: on as its game
    /// or editor goes on (see [`BlockWorld::tick`]).
    pub clock: f64,
    /// The highest block of the floor under its generated ground, in blocks
    /// from its corner (see `generate::BlockFloor`): no change reaches it or
    /// anything under it. None where it has no floor.
    pub floor: Option<i32>,
    /// Each block's sky light and block light, for a world of cubes (see
    /// [`light`]): worked out from its blocks, never saved, and kept up as
    /// they change. Empty for a world drawn smooth.
    pub light: Arc<light::LightMap>,
}

impl Component for BlockWorld {}

/// What a world of blocks without end shows from one of the places it is
/// seen from: its own blocks near that place, and each coarser ring beyond
/// them, with none of the detail kept round the other places, which lies
/// under this place's coarser rings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockPlace {
    /// Where the place is, in metres from the world's corner along x and z.
    pub at: [f32; 2],
    /// The chunk columns shown from it: each by its level, 0 for the
    /// world's own blocks, and its place in that level's columns.
    pub columns: crate::coord_hash::CoordSet<[i32; 3]>,
}

/// How the rings of a world without end fade into each other round the place
/// it is looked at from, so no ring ends at a line and no column of finer
/// ground pops in over the coarse (see `stream`).
///
/// Its own blocks are drawn whole out to `reach` metres from `at`, measured
/// along x or z, whichever is further, and fade out over the last `band`
/// metres of that, the first coarser ring fading in under them; each
/// coarser ring the same out to twice as far, over a band twice as wide,
/// but the coarsest, which fades into nothing. A coarse column whose finer
/// columns were all made after it showed fades out as they fade in, over
/// [`RING_FADE_SECONDS`] from when the last was made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RingFades {
    /// Where its rings are round, in metres from its corner along x and z.
    pub at: [f32; 2],
    /// 0 for a world with no rings to fade.
    pub reach: f32,
    pub band: f32,
    /// Its coarsest ring's level.
    pub coarsest: u32,
    /// For each coarse column shown by its level and place: when its finer
    /// columns were all made, on [`fade_clock`]; `f32::MAX` while they are
    /// not yet, the finer columns made there waiting unseen till then.
    pub covered: crate::coord_hash::CoordMap<[i32; 3], f32>,
    /// The coarse columns shown whose finer columns are all drawn now: their
    /// water is left out, the finer columns' drawn in its place. One covered
    /// once may not be now, as finer columns behind a camera flying on are
    /// let go, and then shows its own water again.
    pub drawn_over: crate::coord_hash::CoordSet<[i32; 3]>,
}

/// How long a coarse column takes to fade into the finer ones over it, in
/// seconds.
pub const RING_FADE_SECONDS: f32 = 0.6;

impl RingFades {
    /// Whether the water of the coarse column at `level` and `x`, `z` in
    /// that level's columns is left out, the finer columns' water over it
    /// drawn whole in its place, as water shows no ring's fade.
    pub fn water_left_out(&self, level: i32, x: i32, z: i32) -> bool {
        level > 0 && self.drawn_over.contains(&[level, x, z])
    }
}

/// Seconds since the program first asked: the clock rings fade by, the same
/// for the streamer and the renderer.
pub fn fade_clock() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
}

/// How fast water running out sideways from what feeds it flows, in metres
/// a second, where it has just left it.
pub const RUNNING_SPEED: f32 = 1.2;

/// How fast falling water falls, in metres a second.
pub const FALLING_SPEED: f32 = 5.0;

impl PartialEq for BlockWorld {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.blocks, &other.blocks) || self.blocks == other.blocks)
            && (Arc::ptr_eq(&self.meshes, &other.meshes) || self.meshes == other.meshes)
            && self.faces == other.faces
            && (Arc::ptr_eq(&self.far, &other.far) || self.far == other.far)
            && self.columns == other.columns
            && (Arc::ptr_eq(&self.places, &other.places) || self.places == other.places)
            && self.block_size == other.block_size
            && self.smooth == other.smooth
            && self.cast_shadows == other.cast_shadows
            && (Arc::ptr_eq(&self.flowing, &other.flowing) || self.flowing == other.flowing)
            // What moves and its clock are the moment's, not what the world is.
            && self.links == other.links
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
        let light = if smooth { light::LightMap::default() } else { light::LightMap::of(&blocks, faces) };
        let meshes = blocks.mesh_lit(faces, smooth, (!smooth).then_some(&light));
        let flowing = running(&blocks, faces);
        Self {
            blocks: Arc::new(blocks),
            meshes: Arc::new(meshes),
            faces: faces.into(),
            far: Arc::default(),
            columns: None,
            unmade: Arc::default(),
            places: Arc::default(),
            fades: Arc::default(),
            block_size,
            smooth,
            cast_shadows: true,
            flowing: Arc::new(flowing),
            links: Arc::default(),
            moving: Arc::default(),
            pressed: Arc::default(),
            clock: 0.0,
            floor: None,
            light: Arc::new(light),
        }
    }

    /// Puts each block of `changes` in its new state, by number: those
    /// marked to move by their motions starting now by its clock, one
    /// already moving going back from where it is.
    pub fn change_states(&mut self, changes: &[([i32; 3], BlockId, bool)]) {
        let clock = self.clock;
        for &(at, block, animate) in changes {
            let kind = self.faces.get(usize::from(block)).copied().unwrap_or_default();
            let length = f32::from(kind.travel) / 1000.0;
            let moving = Arc::make_mut(&mut self.moving);
            let was = moving.iter().position(|moving| moving.at == at).map(|at| moving.remove(at));
            if !animate || length <= 0.0 {
                continue;
            }
            let opening = shape::BlockState::from_byte(kind.state).is_moved();
            // Turned back part way, it goes back from where it is.
            let gone = was.map_or(length, |was| ((clock - was.started) as f32).clamp(0.0, was.length));
            let started = clock - f64::from((length - gone).max(0.0));
            moving.push(usable::MovingBlock { at, started, length, opening });
        }
        self.set_blocks(changes.iter().map(|&(at, block, _)| (at, block)));
    }

    /// Moves its clock on `delta` seconds: its blocks that are done moving
    /// rest, and the buttons whose time is up are given back, to be turned
    /// off.
    pub fn tick(&mut self, delta: f32) -> Vec<[i32; 3]> {
        self.clock += f64::from(delta.max(0.0));
        let clock = self.clock;
        if self.moving.iter().any(|moving| clock - moving.started > f64::from(moving.length)) {
            Arc::make_mut(&mut self.moving).retain(|moving| clock - moving.started <= f64::from(moving.length));
        }
        if !self.pressed.iter().any(|pressed| pressed.until <= clock) {
            return Vec::new();
        }
        let pressed = Arc::make_mut(&mut self.pressed);
        let up: Vec<[i32; 3]> = pressed.iter().filter(|pressed| pressed.until <= clock).map(|pressed| pressed.at).collect();
        pressed.retain(|pressed| pressed.until > clock);
        up
    }

    /// Keeps the button at `at` on until `seconds` from now by its clock.
    pub fn press(&mut self, at: [i32; 3], seconds: f32) {
        let until = self.clock + f64::from(seconds.max(0.0));
        let pressed = Arc::make_mut(&mut self.pressed);
        pressed.retain(|pressed| pressed.at != at);
        pressed.push(usable::Pressed { at, until });
    }

    /// Links the lever or button at `from` to the block at `to`, within
    /// `usable::LINK_REACH`; whether it was linked now.
    pub fn link(&mut self, from: [i32; 3], to: [i32; 3]) -> bool {
        let near = (0..3).all(|i| (to[i] - from[i]).abs() <= usable::LINK_REACH);
        let linked = self.links.get(&from).is_some_and(|all| all.contains(&to));
        let full = self.links.get(&from).is_some_and(|all| all.len() >= usable::MOST_LINKED);
        if !near || linked || full || from == to {
            return false;
        }
        Arc::make_mut(&mut self.links).entry(from).or_default().push(to);
        true
    }

    /// Takes away the link from `from` to `to`, or every link from `from`
    /// where `to` is none; whether any went.
    pub fn unlink(&mut self, from: [i32; 3], to: Option<[i32; 3]>) -> bool {
        if !self.links.contains_key(&from) {
            return false;
        }
        let links = Arc::make_mut(&mut self.links);
        let Some(all) = links.get_mut(&from) else { return false };
        let before = all.len();
        match to {
            Some(to) => all.retain(|&linked| linked != to),
            None => all.clear(),
        }
        let gone = all.len() != before;
        if all.is_empty() {
            links.remove(&from);
        }
        gone
    }

    /// Its links as a list, from each lever or button to each block it
    /// drives: as saved edits keep them.
    pub fn link_list(&self) -> Vec<([i32; 3], [i32; 3])> {
        self.links.iter().flat_map(|(from, all)| all.iter().map(move |to| (*from, *to))).collect()
    }

    /// Its links put back from a list, as saved edits keep them.
    pub fn set_links(&mut self, list: &[([i32; 3], [i32; 3])]) {
        let mut links = usable::Links::new();
        for &(from, to) in list {
            let all = links.entry(from).or_default();
            if !all.contains(&to) && all.len() < usable::MOST_LINKED {
                all.push(to);
            }
        }
        self.links = Arc::new(links);
    }

    /// Moves its water on a step (see [`flow`]), from the blocks near where
    /// it changed since the last. How many blocks changed.
    pub fn flow(&mut self) -> usize {
        if self.flowing.is_empty() {
            return 0;
        }
        let near = std::mem::take(Arc::make_mut(&mut self.flowing));
        let changes = flow::step(&self.blocks, &self.faces, self.smooth, &near);
        self.set_cells(changes)
    }

    /// The block at `at`, a point in metres from the world's corner, and
    /// which block that is.
    pub fn block_at(&self, at: [f32; 3]) -> ([i32; 3], BlockId) {
        let block = at.map(|v| (v / self.block_size).floor() as i32);
        (block, self.blocks.get(block))
    }

    /// The block at `at`, in blocks from the world's corner, as it is
    /// drawn: the block it shows as while covered where a block covers it
    /// (see [`BlockFaces::covered`]), as grass under a block shows as earth.
    pub fn shown(&self, at: [i32; 3]) -> BlockId {
        let block = self.blocks.get(at);
        match self.faces.get(usize::from(block)).is_some_and(|kind| kind.covered != AIR) {
            true => shown_as(&self.faces, block, self.blocks.get([at[0], at[1] + 1, at[2]])),
            false => block,
        }
    }

    /// How deep under its water `at` is, a point in metres from its corner:
    /// where `at` is in its water, or over it within `reach` metres of air,
    /// how far the top of the water in its column is above it, in metres,
    /// less than nothing over the water. `None` where no water is there, or
    /// ground is between.
    pub fn water_depth(&self, at: [f32; 3], reach: f32) -> Option<f32> {
        let (mut block, kind) = self.block_at(at);
        let water = |block: [i32; 3]| is_water(&self.faces, self.blocks.get(block));
        if !water(block) {
            if kind != AIR {
                return None;
            }
            // Down through the air to the water's top, if it comes before the ground and within reach.
            let last = ((at[1] - reach.max(0.0)) / self.block_size).floor() as i32;
            loop {
                block[1] -= 1;
                if block[1] < last || self.blocks.get(block) != AIR && !water(block) {
                    return None;
                }
                if water(block) {
                    break;
                }
            }
        }
        // Up to the top of the water above it, as high as it stands in its block.
        loop {
            let next = [block[0], block[1] + 1, block[2]];
            if !water(next) {
                break;
            }
            block = next;
        }
        // Drawn smooth, its surface stands where the smooth water's does.
        let fill = self.blocks.fill(block);
        let height = match self.smooth {
            true => 0.5 + flow::smooth_surface(fill),
            false => f32::from(flow::water_height(fill).0) / 16.0,
        };
        let top = (block[1] as f32 + height) * self.block_size;
        Some(top - at[1])
    }

    /// How the water at `at` flows of itself, in metres a second, a point in
    /// metres from the world's corner in its water or over it within `reach`
    /// metres of air, as [`BlockWorld::water_depth`] finds it: water running
    /// out sideways flows away from what feeds it, faster the further it has
    /// run; falling water falls; a generated river's flows along its
    /// current. `None` where there is no water there, and nothing for still
    /// water.
    pub fn water_flow(&self, at: [f32; 3], reach: f32) -> Option<[f32; 3]> {
        self.water_depth(at, reach)?;
        let (mut block, _) = self.block_at(at);
        let water = |block: [i32; 3]| is_water(&self.faces, self.blocks.get(block));
        // The block of water at the point, or the top of the water under it.
        while !water(block) {
            block[1] -= 1;
        }
        let fill = self.blocks.fill(block);
        Some(match flow::Water::of(fill) {
            // A generated river's water flows along its current.
            flow::Water::Source if !self.smooth => flow::current_of(fill).map_or([0.0; 3], |[x, z]| [x, 0.0, z]),
            flow::Water::Source => [0.0; 3],
            flow::Water::Falling => [0.0, -FALLING_SPEED, 0.0],
            flow::Water::Flowing { level, up } => {
                // Away from the side it is fed from, along the ground.
                let speed = RUNNING_SPEED * (1.0 + f32::from(flow::REACH.saturating_sub(level)) * 0.15);
                let mut way = [0.0; 3];
                let axis = usize::from(up) / 2;
                if axis != 1 {
                    way[axis] = if up % 2 == 0 { -speed } else { speed };
                }
                way
            }
        })
    }

    /// How high the water over `at` stands, in metres from the world's
    /// corner, where `at` (a point in metres from its corner) is in its
    /// water or over it within `reach` metres of air: the top of the water
    /// in its column (see [`BlockWorld::water_depth`]). `None` where no
    /// water is there, or ground is between.
    pub fn water_over(&self, at: [f32; 3], reach: f32) -> Option<f32> {
        Some(at[1] + self.water_depth(at, reach)?)
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
        let floor = self.floor.unwrap_or(i32::MIN);
        // A block that shows as another while covered is drawn by what is over it: a change reaches the block
        // under it too.
        let covering = !self.smooth && any_covered(&self.faces);
        let mut lit: Vec<[i32; 3]> = Vec::new();
        // In a world without end, a change to a column not made yet is kept till its ground comes.
        let made = self.columns.clone();
        let mut unmade: Vec<edits::Cell> = Vec::new();
        for (at, block, fill) in changes {
            if at[1] <= floor {
                continue;
            }
            let column = [at[0].div_euclid(CHUNK as i32), at[2].div_euclid(CHUNK as i32)];
            if made.as_ref().is_some_and(|made| !made.contains(&column)) {
                unmade.push((at, block, fill));
                changed += 1;
                continue;
            }
            if blocks.get(at) == block && blocks.fill(at) == fill {
                continue;
            }
            let was = blocks.get(at);
            blocks.set_cell(at, block, fill);
            changed += 1;
            // Only a block that lets light through otherwise, or gives other light, changes any block's light.
            let kind =
                |id: BlockId| (light::passes(&self.faces, id), self.faces.get(usize::from(id)).map_or(0, |f| f.light));
            if kind(was) != kind(block) {
                lit.push(at);
            }
            if wet {
                flowing.extend(flow::reached(at));
            }
            let under = [at[0], at[1] - 1, at[2]];
            for at in std::iter::once(at).chain(covering.then_some(under)) {
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
        }
        // Light flooded again round what changed: the chunks whose light changed are drawn again too.
        if !self.smooth && !lit.is_empty() {
            let light = Arc::make_mut(&mut self.light);
            reached.extend(light.relight(&self.blocks, &self.faces, &lit));
        }
        reached.sort_unstable();
        reached.dedup();
        self.remesh(&reached);
        if !flowing.is_empty() {
            Arc::make_mut(&mut self.flowing).extend(flowing);
        }
        if !unmade.is_empty() {
            Arc::make_mut(&mut self.unmade).put_cells(unmade);
        }
        changed
    }

    /// Makes chunks `keys`' faces again, on every core, from the blocks as
    /// they are now: drawn smooth, at their first level alone (see
    /// `smooth::mesh_near`), as quick as can be for a change.
    pub fn remesh(&mut self, keys: &[[i32; 3]]) {
        self.mesh_again(keys, true);
    }

    /// Makes chunks `keys`' faces again, on every core, from the blocks as
    /// they are now: drawn smooth, at their first level alone where `near`
    /// says, or else at every level (see `smooth::LEVELS`). In a world
    /// without end, its water shows no face towards columns not made.
    fn mesh_again(&mut self, keys: &[[i32; 3]], near: bool) {
        voxear_core::profile_scope!("Block Meshing");
        let (blocks, faces, smooth) = (&self.blocks, &self.faces, self.smooth);
        let made = |column: [i32; 2]| self.columns.as_ref().is_none_or(|made| made.contains(&column));
        let light = (!smooth).then_some(&*self.light as &dyn light::LightAt);
        let meshed = crate::workers::each(keys, |&key| match (smooth, near) {
            (true, true) => smooth::mesh_near(blocks, key, faces, &made),
            _ => mesh_lit(blocks, key, faces, smooth, &made, light),
        });
        let meshes = Arc::make_mut(&mut self.meshes);
        for (key, mesh) in keys.iter().zip(meshed) {
            if mesh.is_empty() {
                meshes.remove(key);
            } else {
                meshes.insert(*key, Arc::new(mesh));
            }
        }
    }

    /// The same blocks drawn with other `faces`, smooth or not as `smooth`
    /// says, every chunk's faces made again at every level: as after its
    /// kinds of block changed how they are drawn.
    pub fn with_faces(&self, faces: &[BlockFaces], smooth: bool) -> Self {
        let mut world = Self { meshes: Arc::default(), faces: faces.into(), smooth, ..self.clone() };
        // What lets light through or gives it may be other now.
        world.light =
            Arc::new(if smooth { light::LightMap::default() } else { light::LightMap::of(&world.blocks, faces) });
        let keys: Vec<[i32; 3]> = world.blocks.chunks().map(|(key, _)| *key).collect();
        world.mesh_again(&keys, false);
        world
    }

    /// As [`with_faces`](Self::with_faces), making nothing again where
    /// `faces` differ from its own only in colours, which no mesh keeps: as
    /// while a block's colour is dragged.
    pub fn restyled(&self, faces: &[BlockFaces], smooth: bool) -> Self {
        // What a mesh keeps of a block's faces: its surfaces, and what it shows, moves and joins.
        let drawn = |faces: &BlockFaces| {
            let joins = (faces.connects, faces.mixes, faces.overlap, faces.covered);
            let shape = (faces.water, faces.see_through, faces.sways, faces.shape, joins);
            let parts = (faces.state, faces.family, faces.motion, faces.covers, faces.part, faces.custom, faces.rest);
            let parts = (parts, faces.travel, faces.press, faces.light);
            (faces.top, faces.sides, faces.bottom, shape, parts)
        };
        if smooth == self.smooth && faces.iter().map(drawn).eq(self.faces.iter().map(drawn)) {
            return Self { faces: faces.into(), ..self.clone() };
        }
        self.with_faces(faces, smooth)
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
    mesh_made(map, key, faces, smooth, &|_| true)
}

/// Chunk `key`'s faces in `map`, as [`mesh_any`] makes them, for a world
/// without end of which only the chunk columns `made` picks, by place along
/// x and z, are made: past the others the world goes on, so its water shows
/// no face towards them, though no blocks are there yet.
pub fn mesh_made(
    map: &BlockMap,
    key: [i32; 3],
    faces: &[BlockFaces],
    smooth: bool,
    made: &dyn Fn([i32; 2]) -> bool,
) -> ChunkMesh {
    mesh_lit(map, key, faces, smooth, made, None)
}

/// Chunk `key`'s faces in `map`, as [`mesh_made`] makes them, a world of
/// cubes' lit by `light` where it is given (see `mesh::ChunkMesh::light`).
pub fn mesh_lit(
    map: &BlockMap,
    key: [i32; 3],
    faces: &[BlockFaces],
    smooth: bool,
    made: &dyn Fn([i32; 2]) -> bool,
    light: Option<&dyn light::LightAt>,
) -> ChunkMesh {
    match smooth {
        true => smooth::mesh_made(map, key, faces, made),
        false => mesh::mesh_lit(map, key, faces, made, light),
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
    raycast_where(blocks, origin, direction, max_distance, &|block| block != AIR)
}

/// As [`raycast`], meeting only the blocks `meets` picks: the ray passes
/// through every other, as a hand reaching into water passes through it to
/// the ground.
pub fn raycast_where(
    blocks: &BlockMap,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    meets: &dyn Fn(BlockId) -> bool,
) -> Option<BlockHit> {
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
        if meets(blocks.get(at)) {
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

/// As [`raycast_where`], meeting a block of parts by its boxes, as it is
/// drawn and stood on: the ray meets a slab lying low on its top, half a
/// block down, and passes through the empty half of its block to what lies
/// behind; a stair, a fence or a pane where its parts are. One that is
/// walked through (a lever, a button, a chain, an open door) is met by its
/// whole block still, so it is easy to point at. `faces` draws `blocks`.
pub fn raycast_shaped(
    blocks: &BlockMap,
    faces: &[BlockFaces],
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    meets: &dyn Fn(BlockId) -> bool,
) -> Option<BlockHit> {
    let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length <= 0.0 {
        return None;
    }
    let direction = direction.map(|v| v / length);
    // Where the ray goes on from, how far that is along it, and the way the side it crossed there faces.
    let (mut from, mut gone, mut crossed) = (origin, 0.0f32, [0; 3]);
    loop {
        let mut hit = raycast_where(blocks, from, direction, max_distance - gone, meets)?;
        if hit.normal == [0; 3] {
            hit.normal = crossed;
        }
        let id = blocks.get(hit.block);
        if !is_part(faces, id) || part_of(faces, id) & PART_PASSES != 0 {
            return Some(BlockHit { distance: gone + hit.distance, ..hit });
        }
        let corner = hit.block.map(|v| v as f32);
        let entered = mesh::boxes_at(faces, &|at| blocks.get(at), hit.block)
            .iter()
            .filter_map(|part| {
                let low: [f32; 3] = std::array::from_fn(|i| corner[i] + f32::from(part.from[i]) / 16.0);
                let high: [f32; 3] = std::array::from_fn(|i| corner[i] + f32::from(part.to[i]) / 16.0);
                enter_box(from, direction, low, high)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((along, normal)) = entered {
            let normal = if along <= 0.0 { hit.normal } else { normal };
            return Some(BlockHit { block: hit.block, normal, distance: gone + along });
        }
        // Past its parts: on from where the ray leaves its block.
        let (out, axis) = (0..3)
            .filter(|&i| direction[i] != 0.0)
            .map(|i| {
                let side = if direction[i] > 0.0 { corner[i] + 1.0 } else { corner[i] };
                ((side - from[i]) / direction[i], i)
            })
            .fold((f32::INFINITY, 0), |a, b| if b.0 < a.0 { b } else { a });
        let out = out.max(0.0) + 1e-4;
        crossed = [0; 3];
        crossed[axis] = if direction[axis] > 0.0 { -1 } else { 1 };
        from = std::array::from_fn(|i| from[i] + direction[i] * out);
        gone += out;
        if gone > max_distance {
            return None;
        }
    }
}

/// How far along a ray from `origin` along `direction` it enters the box
/// from `low` to `high`, and the way the side it enters by faces; 0 from
/// inside it. None where it misses the box or the box is behind.
fn enter_box(origin: [f32; 3], direction: [f32; 3], low: [f32; 3], high: [f32; 3]) -> Option<(f32, [i32; 3])> {
    let (mut near, mut far, mut side) = (f32::MIN, f32::MAX, [0; 3]);
    for axis in 0..3 {
        let (o, d) = (origin[axis], direction[axis]);
        if d.abs() < 1e-8 {
            if o < low[axis] || o > high[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((low[axis] - o) / d, (high[axis] - o) / d);
        if a.min(b) > near {
            near = a.min(b);
            side = [0; 3];
            side[axis] = if d > 0.0 { -1 } else { 1 };
        }
        far = far.min(a.max(b));
    }
    (near <= far && far >= 0.0).then_some((near.max(0.0), side))
}

/// How high the ground is in block `at` of `blocks` under the point
/// `(x, z)` within it, in blocks from the bottom of the world: the top of a
/// whole block of ground; the top of a block of parts's boxes under the
/// point, half a block down on a slab lying low. None where the block is
/// no ground (air, water, one walked through), or its parts leave the point
/// open, as beside a pane. `faces` draws `blocks`.
pub fn ground_top(blocks: &BlockMap, faces: &[BlockFaces], at: [i32; 3], (x, z): (f32, f32)) -> Option<f32> {
    let id = blocks.get(at);
    if !is_ground(faces, id) {
        return None;
    }
    let top = (at[1] + 1) as f32;
    if !is_part(faces, id) {
        return Some(top);
    }
    // The highest of its boxes over the point.
    let within = [(x - at[0] as f32) * 16.0, (z - at[2] as f32) * 16.0];
    let over = |part: &shape::PartBox| {
        (f32::from(part.from[0])..=f32::from(part.to[0])).contains(&within[0])
            && (f32::from(part.from[2])..=f32::from(part.to[2])).contains(&within[1])
    };
    let boxes = mesh::boxes_at(faces, &|place| blocks.get(place), at);
    let highest = boxes.iter().filter(|part| over(part)).map(|part| part.to[1]).max()?;
    Some(at[1] as f32 + f32::from(highest) / 16.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_meets_a_slab_on_its_top_and_passes_its_empty_half_and_the_ground_on_it_is_half_a_block_down() {
        use block::BlockShape;
        use shape::{BlockFacing, BlockState};
        // Stone at 1; a slab lying low at 2, one lying high at 3, one standing against the east at 4, a stair
        // rising to the north at 5, and a lever at 6.
        let state = |facing: BlockFacing| BlockState { facing, ..BlockState::default() }.to_byte();
        let part =
            |shape: BlockShape, state: u8| BlockFaces { shape, state, part: PART_DRAWN, ..BlockFaces::default() };
        let mut faces = vec![BlockFaces::default(); 7];
        faces[2] = part(BlockShape::Slab, state(BlockFacing::Down));
        faces[3] = part(BlockShape::Slab, state(BlockFacing::Up));
        faces[4] = part(BlockShape::Slab, state(BlockFacing::East));
        faces[5] = part(BlockShape::Stairs, state(BlockFacing::North));
        faces[6] = BlockFaces { part: PART_DRAWN | PART_PASSES, ..part(BlockShape::Lever, state(BlockFacing::Down)) };
        let mut map = BlockMap::default();
        for x in 0..8 {
            map.set([x, 0, 0], 1);
        }
        for (x, block) in [(1, 2), (2, 3), (3, 4), (4, 5), (5, 6)] {
            map.set([x, 1, 0], block);
        }
        let any = |block: BlockId| block != AIR;
        let down = |x: f32, z: f32| raycast_shaped(&map, &faces, [x, 3.0, z], [0.0, -1.0, 0.0], 10.0, &any).unwrap();
        // Straight down: the low slab's top half a block down, facing up; the high slab's at the top of its block.
        let low = down(1.5, 0.5);
        assert_eq!((low.block, low.normal), ([1, 1, 0], [0, 1, 0]));
        assert!((low.distance - 1.5).abs() < 1e-3, "{low:?}");
        assert!((down(2.5, 0.5).distance - 1.0).abs() < 1e-3);
        // Beside the slab standing against the east, down to the stone; on it, its top.
        assert_eq!(down(3.25, 0.5).block, [3, 0, 0]);
        assert_eq!(down(3.75, 0.5).block, [3, 1, 0]);
        // The stair: its back at the north, the way it rises, its block's top; its step at the south, half a block down.
        assert!((down(4.5, 0.25).distance - 1.0).abs() < 1e-3);
        assert!((down(4.5, 0.75).distance - 1.5).abs() < 1e-3);
        // The lever, walked through, is met by its whole block.
        assert!((down(5.5, 0.5).distance - 1.0).abs() < 1e-3);
        // Along the ground just over the low slab's top, the ray passes over it and meets the high slab's side.
        let along = raycast_shaped(&map, &faces, [0.5, 1.75, 0.5], [1.0, 0.0, 0.0], 10.0, &any).unwrap();
        assert_eq!((along.block, along.normal), ([2, 1, 0], [-1, 0, 0]));
        // The ground on each.
        assert_eq!(ground_top(&map, &faces, [1, 1, 0], (1.5, 0.5)), Some(1.5));
        assert_eq!(ground_top(&map, &faces, [2, 1, 0], (2.5, 0.5)), Some(2.0));
        assert_eq!(ground_top(&map, &faces, [3, 1, 0], (3.2, 0.5)), None, "open beside the standing slab");
        assert_eq!(ground_top(&map, &faces, [4, 1, 0], (4.5, 0.8)), Some(1.5));
        assert_eq!(ground_top(&map, &faces, [5, 1, 0], (5.5, 0.5)), None, "a lever is no ground");
        assert_eq!(ground_top(&map, &faces, [6, 0, 0], (6.5, 0.5)), Some(1.0));
    }

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
    fn water_at_a_worlds_edge_shows_its_sides_but_none_towards_columns_a_world_without_end_has_not_made() {
        // A chunk of water, alone.
        let faces = [BlockFaces::default(), BlockFaces::new(1, 1, 1), BlockFaces::water(5)];
        let mut map = BlockMap::default();
        map.put_chunk([0, 0, 0], Chunk::filled(2));
        let alone = |column: [i32; 2]| column == [0, 0];
        let beside = |column: [i32; 2]| column == [0, 0] || column == [1, 0];
        // Of cubes: how much of the chunk's side each way its water covers.
        let covers = |mesh: ChunkMesh| -> [u32; mesh::FACINGS] {
            let mut area = [0; mesh::FACINGS];
            for &quad in &mesh.quads[mesh.water().0 as usize..] {
                let (_, size, facing, ..) = mesh::unpack(quad);
                area[facing] += size[0] * size[1];
            }
            area
        };
        let whole = CHUNK * CHUNK;
        assert_eq!(covers(mesh_any(&map, [0, 0, 0], &faces, false)), [whole; 6], "at the world's edge");
        assert_eq!(covers(mesh_made(&map, [0, 0, 0], &faces, false, &alone)), [0, 0, whole, whole, 0, 0]);
        assert_eq!(covers(mesh_made(&map, [0, 0, 0], &faces, false, &beside)), [whole, 0, whole, whole, 0, 0]);
        // Drawn smooth: which of the chunk's sides, +x, -x, +z, -z, its water's surface has corners on, between its
        // top and bottom and its other sides.
        let sides = |mesh: ChunkMesh| -> [bool; 4] {
            let words = &mesh.quads[mesh.water().0 as usize..];
            let side = CHUNK as f32;
            let corners: Vec<[f32; 3]> = words
                .chunks(2)
                .map(|pair| [pair[0][0] & 0xFFFF, pair[0][0] >> 16, pair[0][1] & 0xFFFF])
                .map(|at| at.map(|q| q as f32 / smooth::POINT_STEPS + smooth::POINT_FROM))
                .filter(|p| (1.0..side - 1.0).contains(&p[1]))
                .collect();
            let across = |p: &[f32; 3], axis: usize| (1.0..side - 1.0).contains(&p[2 - axis]);
            let on = |axis: usize, at: f32| corners.iter().any(|p| (p[axis] - at).abs() < 0.01 && across(p, axis));
            [on(0, side), on(0, 0.0), on(2, side), on(2, 0.0)]
        };
        assert_eq!(sides(mesh_any(&map, [0, 0, 0], &faces, true)), [true; 4], "at the world's edge");
        assert_eq!(sides(mesh_made(&map, [0, 0, 0], &faces, true, &alone)), [false; 4], "its own column made alone");
        let towards = sides(mesh_made(&map, [0, 0, 0], &faces, true, &beside));
        assert_eq!(towards, [true, false, false, false], "towards air in a column made");
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
        // Water before the block: a ray meets it, or passes through it to the block behind.
        map.set([3, 2, -3], 2);
        let (from, along) = ([0.5, 2.5, -2.5], [1.0, 0.0, 0.0]);
        assert_eq!(raycast(&map, from, along, 100.0).unwrap().block, [3, 2, -3]);
        let through = raycast_where(&map, from, along, 100.0, &|block| block == 1).expect("the block behind");
        assert_eq!((through.block, through.normal), ([5, 2, -3], [-1, 0, 0]));
        assert!((through.distance - 4.5).abs() < 1e-5);
    }
}
