//! Worlds of blocks without end: ground of blocks made round the camera as
//! it goes, from the world's generator (see `generate`), a chunk column at a
//! time on the terrain's background threads, nearest first.
//!
//! Near the camera, out to its view distance, the world's own chunks: its
//! blocks, built on, stood on and drawn, each column made again with the
//! changes kept for it as it comes back, and its changes kept as it goes.
//! Their blocks are made a column further out than they are drawn, and a
//! column's faces are made in the background too, once the eight columns
//! round it have their blocks: so each is made once, and the frame never
//! waits for it. Beyond, out to its far distance, rings of coarser chunks,
//! each ring's blocks twice as large as the one inside it, drawn alone: a
//! ring's chunk columns are left out where the four finer columns under
//! each are drawn, so near and far never both draw the same ground, and the
//! coarse ground shows until the fine is drawn.
//!
//! A world of cubes makes each coarse column's faces alone, the columns
//! beside it not made: its blocks meet no blocks there, and its ground's
//! faces show as cliffs, hidden by the ground beside them, but its water
//! shows none (see `BlockWorld::columns`), or a sea would be cut by walls of
//! water. So do the near columns past the last made.
//!
//! A world drawn smooth would show every such side as a groove, its surface
//! rounded off into it. Its coarse columns' blocks are made a column further
//! out than they are drawn, as the near ones' are, and kept; a column's
//! faces are made once the eight columns round it have their blocks, so a
//! ring's ground runs on across its columns. Where a ring meets the finer
//! ring inside it, the two are the same ground at two sizes of block, and
//! the coarser has skirts to close what lies between (see
//! `smooth::mesh_skirted`).

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
};

use crate::coord_hash::{CoordMap, CoordSet};

use super::{
    BlockMap, BlockWorld, RING_FADE_SECONDS, RingFades,
    chunk::{CHUNK, Chunk},
    edits::{BlockEdits, ChunkChange},
    file::BlockFile,
    generate::{
        BlockGenerate, column_chunks,
        far::{self, FarTile, far_chunks, far_tile},
    },
    light::{ChunkLight, LightAt, heights, light_column},
    mesh::ChunkMesh,
    mesh_lit,
};
use crate::generate::WorldGen;

/// How many chunk columns are made at once in the background, at most,
/// until the streamer is told otherwise (see `WorldBudget::columns`).
const MOST_MAKING: usize = 24;

/// The most levels a world draws: its own chunks, and rings of coarser
/// chunks, each twice as large as the one inside it, out to 128 times as
/// large, 16 km off with metre blocks at four columns a ring.
pub const MOST_LEVELS: u32 = 8;

/// How far past the rest a coarse column whose ground finer columns are
/// kept over is counted, in metres, when what to make first is chosen: it
/// shows only where those are not made yet, and is made last.
const COVERED: f32 = 1.0e6;

/// How far ahead, in seconds, where the world is seen from is taken to be
/// when what to make first is chosen; and how far that may be at most, in
/// near columns.
const LEAD_SECONDS: f32 = 1.0;
const LEAD_COLUMNS: f32 = 4.0;

/// One in how many of the columns made at once is kept for the rings
/// whatever is nearer, so the horizon keeps up with a camera flying on.
const RINGS_ALWAYS: usize = 12;

/// The first level made as the far view (see `generate::far`): from the
/// generator's heights alone, kept on disk where the world has a folder.
pub const FAR_FROM: u32 = 3;

/// Something a world's generator builds past its near chunks, as the far
/// view shows it: a wood, a town's roofs, a tower. It is handed each far
/// tile as the ground alone makes it, and puts its own blocks into the
/// tile's columns ([`FarTile::add`]). Tiles are kept without them, so what
/// is built far off can change, and be made again, with no ground made
/// again.
pub trait FarBuilt: Send + Sync {
    /// A number that changes whenever what it would put into a tile does.
    fn stamp(&self) -> u64;

    /// The first ring it is built into, by its level: the far view's first
    /// unless it says nearer. A coarser ring made in full, with its ground's
    /// overhangs and caves, takes what is built into the tile its ground
    /// would be as a far tile, block for block.
    fn from_level(&self) -> u32 {
        FAR_FROM
    }

    /// Puts its blocks into `tile`, a tile of the ground `making` makes.
    fn build(&self, making: &BlockMaking, tile: &mut FarTile);
}

/// What a world of blocks without end's chunks are made from: its file, the
/// way it generates, its generator, and the surfaces its blocks' faces are
/// drawn with.
#[derive(Clone)]
pub struct BlockMaking {
    pub file: BlockFile,
    pub generate: BlockGenerate,
    pub world: Arc<WorldGen>,
    pub faces: Arc<[super::BlockFaces]>,
    /// Where its far view's tiles are kept, the world's own folder, in which
    /// they go into a folder of what they were made from; none to keep none.
    pub far_folder: Option<std::path::PathBuf>,
    /// What is built into its far view's tiles over their ground, each in
    /// turn. None of the ground is made again when these change: only the
    /// far view's tiles are built on again.
    pub far_built: Vec<Arc<dyn FarBuilt>>,
}

impl BlockMaking {
    /// Whether `other` makes what this made: the same ground from the same
    /// generator, every block this drew drawn the same. Faces more at the
    /// end, as of blocks the world came to hold since, change nothing made.
    fn same(&self, other: &BlockMaking) -> bool {
        self.file == other.file
            && Arc::ptr_eq(&self.world, &other.world)
            && other.faces.starts_with(&self.faces)
            && self.far_folder == other.far_folder
    }

    /// A number for everything built into its far view's tiles.
    fn built_stamp(&self) -> u64 {
        let stamp = |built: &Arc<dyn FarBuilt>| [built.stamp(), u64::from(built.from_level())];
        let stamps: Vec<u8> = self.far_built.iter().flat_map(stamp).flat_map(u64::to_le_bytes).collect();
        crate::layers::stable_hash(&stamps)
    }

    /// The first ring anything is built into, by its level; none where
    /// nothing is built far off.
    fn built_from(&self) -> Option<u32> {
        self.far_built.iter().map(|built| built.from_level().max(1)).min()
    }

    /// Builds into `tile`, of ring `level`, everything built into that ring.
    fn build_into(&self, level: u32, tile: &mut FarTile) {
        for built in self.far_built.iter().filter(|built| built.from_level().max(1) <= level) {
            built.build(self, tile);
        }
    }

    /// The folder its far view's tiles are kept in, named by what they are
    /// made from, so a world made another way keeps its own.
    pub fn far_tiles(&self) -> Option<std::path::PathBuf> {
        let made_from = format!("{}\n{:016x}\n{}", self.file.to_text(), self.world.hash(), far::MADE_AS);
        let hash = crate::layers::stable_hash(made_from.as_bytes());
        self.far_folder.as_ref().map(|folder| folder.join(format!("{hash:016x}")))
    }
}

/// The far view's tile `at` of level `level`, as kept in `folder` where it
/// was kept, else made and kept there. Not kept where the folder cannot be
/// written to, as in a game installed where it may not write.
pub fn far_tile_kept(making: &BlockMaking, folder: Option<&std::path::Path>, level: u32, at: [i32; 2]) -> FarTile {
    let path = folder.map(|folder| folder.join(format!("{level}_{}_{}.far", at[0], at[1])));
    let read = path.as_ref().and_then(|path| std::fs::read(path).ok());
    let kept = read.and_then(|bytes| FarTile::from_bytes(&bytes).ok());
    if let Some(tile) = kept.filter(|tile| tile.level == level && tile.at == at) {
        return tile;
    }
    let tile = far_tile(&making.file, &making.generate, &making.world, level, at);
    if let Some(path) = path {
        // Written whole beside it, then put in its place, so a tile read as it is written is never half of one.
        static WRITING: AtomicU64 = AtomicU64::new(0);
        let writing = path.with_extension(format!("far.{}", WRITING.fetch_add(1, Ordering::Relaxed)));
        let written = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| {
            std::fs::write(&writing, tile.to_bytes())?;
            std::fs::rename(&writing, &path)
        });
        if written.is_err() {
            let _ = std::fs::remove_file(&writing);
        }
    }
    tile
}

/// Work done in the background, each by the number of what it was made
/// from.
enum Finished {
    /// A chunk column's chunks, at its level and place, and for a coarse
    /// one their faces; and what was built into its far view when it was
    /// made, by its stamp.
    Made {
        number: u64,
        level: u32,
        at: [i32; 2],
        chunks: Vec<([i32; 3], Chunk)>,
        meshes: Vec<([i32; 3], ChunkMesh)>,
        built: u64,
    },
    /// A near column's chunks' faces and light, a world of cubes', and the
    /// chunks round it they were made from.
    Meshed {
        number: u64,
        at: [i32; 2],
        meshes: Vec<([i32; 3], ChunkMesh)>,
        light: Vec<([i32; 3], ChunkLight)>,
        from: BlockMap,
    },
    /// A smooth coarse column's chunks' faces, made from its blocks and
    /// those of the columns round it.
    Joined { number: u64, level: u32, at: [i32; 2], meshes: Vec<([i32; 3], ChunkMesh)> },
}

/// A coarse column's chunks, kept for making its faces and those of the
/// columns round it.
type ColumnChunks = Arc<Vec<([i32; 3], Arc<Chunk>)>>;

/// The chunk columns of each level a window keeps round the camera: from
/// `low` to `high` (not reached) along x and z, in that level's columns.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Window {
    low: [i32; 2],
    high: [i32; 2],
}

impl Window {
    fn contains(&self, at: [i32; 2]) -> bool {
        (self.low[0]..self.high[0]).contains(&at[0]) && (self.low[1]..self.high[1]).contains(&at[1])
    }

    fn columns(&self) -> impl Iterator<Item = [i32; 2]> + '_ {
        (self.low[1]..self.high[1]).flat_map(move |z| (self.low[0]..self.high[0]).map(move |x| [x, z]))
    }

    /// The window a column wider each way.
    fn grown(&self) -> Window {
        Window { low: self.low.map(|v| v - 1), high: self.high.map(|v| v + 1) }
    }
}

/// A place a world of blocks without end is seen from, which its chunks are
/// kept round: where, in metres from the world's corner along x and z, and
/// how far, near and coarse. Only the first place's `far` counts: every
/// place has as many rings as it, so that all their rings lie on the same
/// columns and none overlaps another's by part of a column.
///
/// And, for the first place, how many times as far as `distance` its first
/// coarser ring begins, `pushed`, 1 or more: so far that the coarser rings'
/// blocks show no larger than they should from there (see
/// `WorldBudget::far_block_pixels`), each ring after it twice as far as the
/// one before. The world's own blocks are drawn on past where they are
/// made, out to where the first coarser ring begins, made as the coarse
/// rings are, for drawing alone: none stood on, built on or grown on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StreamFrom {
    pub at: [f32; 2],
    pub distance: f32,
    pub far: f32,
    pub pushed: f32,
}

impl StreamFrom {
    /// How many times as far as its own blocks reach its first coarser ring
    /// begins: as [`StreamFrom::pushed`] says, no further than it draws the
    /// world at all.
    fn pushed(&self) -> f32 {
        self.pushed.min((self.far / self.distance.max(1.0)).max(1.0))
    }
}

/// The level a column of the world's own blocks drawn past where they are
/// made is asked for and made at: blocks as large as the world's own, its
/// faces made alone, as a coarse column's are (see [`StreamFrom::pushed`]).
/// In the world's faces past its own (`BlockWorld::far`) its chunks are
/// kept at level -1.
const FINE: u32 = u32::MAX;

/// The most times as far as its own blocks reach a world's first coarser
/// ring is pushed out.
pub const MOST_PUSHED: f32 = 8.0;

/// How many columns out the first coarser ring begins, for a world whose own
/// blocks reach `reach` columns and whose rings are pushed out `pushed`
/// times as far (see [`StreamFrom::pushed`]).
fn pushed_reach(reach: i32, pushed: f32) -> i32 {
    let pushed = if pushed.is_finite() { pushed.clamp(1.0, MOST_PUSHED) } else { 1.0 };
    ((reach as f32 * pushed).ceil() as i32).max(reach)
}

/// The chunk columns of one level kept: the window round each place the
/// world is seen from that reaches this level.
#[derive(Clone, Debug, Default, PartialEq)]
struct Area(Vec<Window>);

impl Area {
    fn contains(&self, at: [i32; 2]) -> bool {
        self.0.iter().any(|window| window.contains(at))
    }

    /// Every column of it, once each.
    fn columns(&self) -> Vec<[i32; 2]> {
        let mut columns: Vec<[i32; 2]> = Vec::new();
        for (index, window) in self.0.iter().enumerate() {
            // Only those no window before it holds.
            columns.extend(window.columns().filter(|at| !self.0[..index].iter().any(|before| before.contains(*at))));
        }
        columns
    }

    /// Each window a column wider each way.
    fn grown(&self) -> Area {
        Area(self.0.iter().map(Window::grown).collect())
    }
}

/// Each level's columns kept round the places in `from`, a level's
/// together, and each place's own windows (see `windows`): as many levels
/// for every place as the first has. Each window's sides lie on its next
/// level's columns, so a coarse column is wholly in or out of each finer
/// window.
fn areas(file: &BlockFile, from: &[StreamFrom]) -> (Vec<Area>, Vec<Vec<Window>>, Option<Window>) {
    let first = from.first().map(|first| windows_out(file, (first.distance, first.far), first.at, first.pushed()));
    let levels = first.as_ref().map_or(1, |(windows, _)| windows.len());
    let fine = first.and_then(|(_, fine)| fine);
    let mut areas: Vec<Area> = vec![Area::default(); levels];
    let mut places = Vec::with_capacity(from.len());
    for (number, place) in from.iter().enumerate() {
        // Far enough for any number of levels, then as many as the first place has; its own pushed out as it says.
        let pushed = if number == 0 { place.pushed() } else { 1.0 };
        let mut own = windows_out(file, (place.distance, f32::MAX), place.at, pushed).0;
        own.truncate(levels);
        for (level, window) in own.iter().enumerate() {
            if !areas[level].0.contains(window) {
                areas[level].0.push(*window);
            }
        }
        places.push(own);
    }
    (areas, places, fine)
}

/// The nine chunk columns round `at`, its own among them.
fn round([x, z]: [i32; 2]) -> impl Iterator<Item = [i32; 2]> {
    (-1..=1).flat_map(move |dz| (-1..=1).map(move |dx| [x + dx, z + dz]))
}

/// How far each level's window reaches round `camera`, a point in metres
/// from the world's corner: the near columns out to `distance`, then each
/// coarser level twice as far, out to `far`, every window's sides on its
/// next level's columns, so a coarse column is wholly in or out of the
/// window inside it.
#[cfg(test)]
fn windows(file: &BlockFile, distance: f32, far: f32, camera: [f32; 2]) -> Vec<Window> {
    windows_out(file, (distance, far), camera, 1.0).0
}

/// As [`windows`], the coarser levels' windows `pushed` times as many of
/// their columns across (see [`StreamFrom::pushed`]); and the window of the
/// world's own blocks drawn past where they are made, as far as the first
/// coarser level's begins, where that is further than they are made.
fn windows_out(
    file: &BlockFile,
    (distance, far): (f32, f32),
    camera: [f32; 2],
    pushed: f32,
) -> (Vec<Window>, Option<Window>) {
    let column = CHUNK as f32 * file.block_size.max(0.01);
    let reach = (distance / column).ceil().max(1.0) as i32;
    let outer = pushed_reach(reach, pushed);
    let window = |level: u32, reach: i32| {
        let size = column * (1u32 << level) as f32;
        let at = camera.map(|v| (v / size).floor() as i32);
        // From an even column to an even one, so the next level's columns hold it whole.
        let low = at.map(|v| (v - reach).div_euclid(2) * 2);
        let high = at.map(|v| (v + reach + 2).div_euclid(2) * 2);
        Window { low, high }
    };
    let mut windows = vec![window(0, reach)];
    let fine = (outer > reach).then(|| window(0, outer));
    // A level more while the last falls short of `far`, the world's own blocks reaching as far as they are drawn.
    let mut level = 0u32;
    while column * (1u32 << level) as f32 * (outer as f32) < far && level + 1 < MOST_LEVELS {
        level += 1;
        windows.push(window(level, outer));
    }
    (windows, fine)
}

/// Keeps a world of blocks without end's chunks round the camera.
pub struct BlockStreamer {
    making: Option<Arc<BlockMaking>>,
    number: u64,
    latest: Arc<AtomicU64>,
    /// The coarse columns made, by level and place: their chunks' faces.
    coarse: CoordMap<(u32, [i32; 2]), Vec<([i32; 3], Arc<ChunkMesh>)>>,
    /// The columns of the world's own blocks made for drawing past where
    /// they are made, by place, and the window they are kept in (see
    /// [`StreamFrom::pushed`]).
    fine: CoordMap<[i32; 2], Vec<([i32; 3], Arc<ChunkMesh>)>>,
    fine_window: Option<Window>,
    /// The blocks of a smooth world's coarse columns, by level and place.
    coarse_blocks: CoordMap<(u32, [i32; 2]), ColumnChunks>,
    /// The smooth coarse columns whose faces are being made.
    joining: CoordSet<(u32, [i32; 2])>,
    /// The near columns whose blocks are made, by place.
    near: CoordSet<[i32; 2]>,
    /// The near columns drawn: their faces made.
    drawn: CoordSet<[i32; 2]>,
    /// The near columns whose faces are being made.
    meshing: CoordSet<[i32; 2]>,
    /// The world's faces as the last update left them, to find faces made
    /// since, by changes to its blocks, for columns not drawn.
    seen: Option<Arc<HashMap<[i32; 3], Arc<ChunkMesh>>>>,
    /// Each level's columns kept as the last update left them.
    windows: Option<Vec<Area>>,
    /// Whether faces made were let go, their blocks changed since, to be
    /// asked for again.
    again: bool,
    asked: CoordSet<(u32, [i32; 2])>,
    /// Coarse columns made before changes were kept for the ground under
    /// them, made again to show what was built, the old drawn till then.
    stale: CoordSet<(u32, [i32; 2])>,
    /// For each coarse column made round where the world is looked at
    /// from, by level and place: when its finer columns were all made, on
    /// `fade_clock`, or `f32::MAX` while it shows with them not made yet
    /// (see [`RingFades::covered`]).
    covered_since: CoordMap<(u32, [i32; 2]), f32>,
    /// Whether a coarse column is fading into the finer ones over it, and
    /// when the first of those fades is over, on `fade_clock`.
    fading: bool,
    fade_over: f32,
    /// For each coarse column, a number given it as it was made, new each
    /// time; and the columns shown, with those numbers, as the far chunks
    /// were last put together from them, and those far chunks (see
    /// [`Self::show_coarse`]).
    coarse_made: CoordMap<(u32, [i32; 2]), u64>,
    made_count: u64,
    far_from: (u64, Option<Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>>),
    waiting: usize,
    making_now: Arc<AtomicUsize>,
    /// How many of those being made are the far view's tiles, and how many
    /// are the rings' columns, the far view's among them.
    far_now: Arc<AtomicUsize>,
    rings_now: Arc<AtomicUsize>,
    /// Where the first place the world is seen from was last, and when; and
    /// how fast it moves, in metres a second along x and z.
    moving: Option<([f32; 2], std::time::Instant)>,
    velocity: [f32; 2],
    /// The folder the far view's tiles of what it makes are kept in.
    far_tiles: Option<std::path::PathBuf>,
    /// How many columns it makes at once, at most: their blocks or their
    /// faces. Changed as it streams, it takes hold from the next update.
    pub at_once: usize,
    send: mpsc::Sender<Finished>,
    finished: Mutex<mpsc::Receiver<Finished>>,
}

impl Default for BlockStreamer {
    fn default() -> Self {
        let (send, finished) = mpsc::channel();
        Self {
            making: None,
            number: 0,
            latest: Arc::new(AtomicU64::new(0)),
            coarse: CoordMap::default(),
            fine: CoordMap::default(),
            fine_window: None,
            coarse_blocks: CoordMap::default(),
            joining: CoordSet::default(),
            near: CoordSet::default(),
            drawn: CoordSet::default(),
            meshing: CoordSet::default(),
            seen: None,
            windows: None,
            again: false,
            asked: CoordSet::default(),
            stale: CoordSet::default(),
            covered_since: CoordMap::default(),
            fading: false,
            fade_over: 0.0,
            coarse_made: CoordMap::default(),
            made_count: 0,
            far_from: (0, None),
            waiting: 0,
            making_now: Arc::new(AtomicUsize::new(0)),
            far_now: Arc::new(AtomicUsize::new(0)),
            rings_now: Arc::new(AtomicUsize::new(0)),
            moving: None,
            velocity: [0.0; 2],
            far_tiles: None,
            at_once: MOST_MAKING,
            send,
            finished: Mutex::new(finished),
        }
    }
}

impl BlockStreamer {
    /// Whether every column of every window is made and drawn, and none is
    /// being made.
    pub fn settled(&self) -> bool {
        self.waiting == 0
            && self.asked.is_empty()
            && self.meshing.is_empty()
            && self.joining.is_empty()
            && self.making_now.load(Ordering::Acquire) == 0
    }

    /// Whether every near column within `radius` metres of `at`, in metres
    /// from the world's corner along x and z, is made and drawn: there is
    /// ground to see there, and to stand on once physics has taken it up.
    pub fn drawn_round(&self, at: [f32; 2], radius: f32) -> bool {
        let Some(making) = &self.making else { return false };
        let column = CHUNK as f32 * making.file.block_size.max(0.01);
        let low = at.map(|v| ((v - radius) / column).floor() as i32);
        let high = at.map(|v| ((v + radius) / column).floor() as i32);
        (low[1]..=high[1]).all(|z| (low[0]..=high[0]).all(|x| self.drawn.contains(&[x, z])))
    }

    /// Brings `world`'s chunks round `camera`, in metres from its corner
    /// along x and z, made by `making`, out to `distance` metres near and
    /// `far` metres coarse: `base` keeps the near chunks as made and
    /// `pending` the changes to chunks not made now, taken from as columns
    /// come and given to as they go. True if anything changed.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut BlockWorld,
        base: &mut Arc<BlockMap>,
        pending: &mut Arc<BlockEdits>,
        making: BlockMaking,
        (distance, far): (f32, f32),
        camera: [f32; 2],
    ) -> bool {
        self.update_from(world, base, pending, making, &[StreamFrom { at: camera, distance, far, pushed: 1.0 }])
    }

    /// As [`update`](Self::update), with chunks kept round every place in
    /// `from` and not the camera alone: the first is where the world is
    /// looked at from, and the rest are where it is seen from besides, as
    /// the far sides of Portals, each as far as it says. What is nearest any
    /// of them is made first.
    pub fn update_from(
        &mut self,
        world: &mut BlockWorld,
        base: &mut Arc<BlockMap>,
        pending: &mut Arc<BlockEdits>,
        making: BlockMaking,
        from: &[StreamFrom],
    ) -> bool {
        let mut changed = false;
        // What is built into the far view changed, its ground the same: its tiles are built on again, the old drawn
        // till then, and nothing else is made again.
        let rebuilt =
            self.making.as_ref().is_some_and(|kept| kept.same(&making) && kept.built_stamp() != making.built_stamp());
        if rebuilt {
            // From the first ring either builds into.
            let kept = self.making.as_ref().and_then(|kept| kept.built_from());
            let first = kept.into_iter().chain(making.built_from()).min().unwrap_or(FAR_FROM);
            let far = |(level, _): &&(u32, [i32; 2])| *level >= first;
            let stale: Vec<(u32, [i32; 2])> =
                self.coarse.keys().chain(self.coarse_blocks.keys()).filter(far).copied().collect();
            self.stale.extend(stale);
            self.again = true;
        }
        if !self.making.as_ref().is_some_and(|kept| kept.same(&making)) {
            // Made anew: every column let go, the changes on those near kept.
            if !self.near.is_empty() {
                Arc::make_mut(pending).chunks.extend(BlockEdits::between(base, &world.blocks).chunks);
            }
            *base = Arc::new(BlockMap::default());
            // Its links and its clock stay: they are the world's, not its chunks'.
            let (links, clock) = (world.links.clone(), world.clock);
            *world = BlockWorld::made(BlockMap::default(), &making.faces, making.file.block_size, making.file.smooth);
            (world.links, world.clock) = (links, clock);
            world.columns = Some(Arc::default());
            // Its floor is the world's settings', made anew with them.
            world.floor = making.generate.floor_top();
            (self.near, self.drawn, self.meshing) = Default::default();
            (self.coarse, self.asked, self.stale) = Default::default();
            self.covered_since.clear();
            self.fine.clear();
            (self.coarse_blocks, self.joining) = Default::default();
            self.number = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
            self.far_tiles = making.far_tiles();
            self.making = Some(Arc::new(making));
            changed = true;
        } else if self.making.as_ref().is_some_and(|kept| kept.faces != making.faces) {
            // It has faces more, as once it holds a block it had not held: faces are made with them from here on,
            // and nothing made is made again. A column's faces being made now with those before are right for its
            // blocks as they were, and are made again if its blocks change.
            world.faces = making.faces.clone();
            self.making = Some(Arc::new(making));
            changed = true;
        } else if rebuilt {
            self.making = Some(Arc::new(making));
        }
        let making = self.making.clone().expect("kept above");
        let (windows, by_place, fine) = areas(&making.file, from);
        // Near columns are drawn within the first level's windows, their blocks made a column further.
        let (shown, made) = (windows[0].clone(), windows[0].grown());
        changed |= self.take_finished(world, (base, pending), (&shown, &made));
        changed |= self.let_go(world, (base, pending), (&shown, &made));
        self.coarse.retain(|(level, at), _| windows.get(*level as usize).is_some_and(|window| window.contains(*at)));
        self.fine.retain(|at, _| fine.is_some_and(|fine| fine.contains(*at)));
        let (coarse, fines) = (&self.coarse, &self.fine);
        self.coarse_made.retain(|key, _| coarse.contains_key(key) || key.0 == FINE && fines.contains_key(&key.1));
        self.coarse_blocks
            .retain(|(level, at), _| windows.get(*level as usize).is_some_and(|window| window.grown().contains(*at)));
        // Faces made since for columns not drawn, as where water flowed or a change reached, are not drawn: the
        // coarse ground is drawn there.
        if !self.seen.as_ref().is_some_and(|seen| Arc::ptr_eq(seen, &world.meshes)) {
            let stray: Vec<[i32; 3]> =
                world.meshes.keys().filter(|key| !self.drawn.contains(&[key[0], key[2]])).copied().collect();
            if !stray.is_empty() {
                let meshes = Arc::make_mut(&mut world.meshes);
                for key in stray {
                    meshes.remove(&key);
                }
                changed = true;
            }
        }
        // Asked again only where something came or went, or the camera moved, or there is still more to make and
        // room to make it: every thread busy, the far view would be looked over every frame for nothing.
        let moved = self.windows.as_deref() != Some(&windows[..]) || self.fine_window != fine;
        let room = self.making_now.load(Ordering::Acquire) < self.at_once.max(1);
        if changed || moved || self.waiting > 0 && room || std::mem::take(&mut self.again) {
            let lead = self.lead(from.first(), &making);
            self.ask(world, &making, (&windows, fine), (from, pending, lead));
        }
        // Again where columns came or went or the camera moved, and as the first fade under way is over.
        if changed || moved || self.fading && super::fade_clock() >= self.fade_over {
            changed |= self.show_coarse(world, (&by_place, fine), from);
        }
        self.windows = Some(windows);
        self.fine_window = fine;
        self.seen = Some(world.meshes.clone());
        changed
    }

    /// Takes in what the background threads made: near columns' blocks, with
    /// the changes kept for them, within `made`; their faces, within
    /// `shown`, where the blocks they were made from are still those of the
    /// world; and coarse columns' faces. True if anything changed.
    fn take_finished(
        &mut self,
        world: &mut BlockWorld,
        (base, pending): (&mut Arc<BlockMap>, &mut Arc<BlockEdits>),
        (shown, made): (&Area, &Area),
    ) -> bool {
        voxear_core::profile_scope!("Take Finished");
        let mut changed = false;
        let finished: Vec<Finished> = {
            let receive = self.finished.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            receive.try_iter().collect()
        };
        // The world's chunks by column, found once and only where faces came.
        let meshed = finished.iter().any(|done| matches!(done, Finished::Meshed { .. }));
        let columns = meshed.then(|| columns_of(&world.blocks)).unwrap_or_default();
        let mut came: Vec<[i32; 2]> = Vec::new();
        for done in finished {
            match done {
                Finished::Made { number, .. } | Finished::Meshed { number, .. } | Finished::Joined { number, .. }
                    if number != self.number => {}
                Finished::Made { level: FINE, at, meshes, .. } => {
                    // The world's own blocks drawn past where they are made, kept while their window holds them.
                    self.asked.remove(&(FINE, at));
                    let meshes = meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                    self.made_count += 1;
                    self.coarse_made.insert((FINE, at), self.made_count);
                    self.fine.insert(at, meshes);
                    changed = true;
                }
                Finished::Made { level: 0, at, chunks, .. } => {
                    self.asked.remove(&(0, at));
                    if !made.contains(at) || self.near.contains(&at) {
                        continue;
                    }
                    let blocks = Arc::make_mut(&mut world.blocks);
                    let kept = Arc::make_mut(base);
                    // Shared, so a chunk left as it was is known unchanged at a glance when it goes.
                    for (key, chunk) in chunks.into_iter().filter(|(_, chunk)| !chunk.is_air()) {
                        let chunk = Arc::new(chunk);
                        kept.put_shared(key, chunk.clone());
                        blocks.put_shared(key, chunk);
                    }
                    self.near.insert(at);
                    came.push(at);
                    changed = true;
                }
                Finished::Made { level, at, chunks, meshes, built, .. } => {
                    self.asked.remove(&(level, at));
                    let was_stale = self.stale.remove(&(level, at));
                    // Made while what is built into the far view was other than it is now: shown, and made again.
                    if self.making.as_ref().is_some_and(|making| making.built_stamp() != built) {
                        self.stale.insert((level, at));
                        self.again = true;
                    }
                    if self.making.as_ref().is_some_and(|making| making.file.smooth) {
                        // Its faces made again with those round it, its old ones drawn till then.
                        if was_stale {
                            self.joining.remove(&(level, at));
                            self.coarse.remove(&(level, at));
                        }
                        // Kept, for its faces and those of the columns round it, made once theirs are.
                        let chunks = chunks.into_iter().map(|(key, chunk)| (key, Arc::new(chunk))).collect();
                        self.coarse_blocks.insert((level, at), Arc::new(chunks));
                        self.again = true;
                        continue;
                    }
                    let meshes = meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                    self.made_count += 1;
                    self.coarse_made.insert((level, at), self.made_count);
                    self.coarse.insert((level, at), meshes);
                    changed = true;
                }
                Finished::Joined { level, at, meshes, .. } => {
                    self.joining.remove(&(level, at));
                    let meshes = meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                    self.made_count += 1;
                    self.coarse_made.insert((level, at), self.made_count);
                    self.coarse.insert((level, at), meshes);
                    changed = true;
                }
                Finished::Meshed { at, meshes, light, from, .. } => {
                    self.meshing.remove(&at);
                    // Blocks changed round it since are made into faces again, next.
                    let now = round_chunks(at, &columns, &world.blocks);
                    if !shown.contains(at) || self.drawn.contains(&at) || !same_chunks(&now, &from) {
                        self.again = true;
                        continue;
                    }
                    // Its light as its faces were made with it, in place of any a change flooded into it before.
                    if !world.smooth {
                        let kept = Arc::make_mut(&mut world.light);
                        kept.forget_columns(&[at]);
                        for (key, chunk) in light {
                            kept.put(key, chunk);
                        }
                    }
                    let kept = Arc::make_mut(&mut world.meshes);
                    for (key, mesh) in meshes {
                        if mesh.is_empty() {
                            kept.remove(&key);
                        } else {
                            kept.insert(key, Arc::new(mesh));
                        }
                    }
                    self.drawn.insert(at);
                    changed = true;
                }
            }
        }
        // The changes kept for columns come near made, before their faces are; past the columns made, water
        // shows no faces.
        if !came.is_empty() {
            world.columns = Some(Arc::new(self.near.iter().copied().collect()));
            let mut theirs = BlockEdits::default();
            Arc::make_mut(pending).chunks.retain(|key, change| {
                let mine = came.contains(&[key[0], key[2]]);
                if mine {
                    theirs.chunks.insert(*key, change.clone());
                }
                !mine
            });
            theirs.apply(Arc::make_mut(&mut world.blocks));
        }
        changed
    }

    /// Lets go of the near columns gone from `made`, their changes kept, and
    /// stops drawing those gone from `shown`. True if any went.
    fn let_go(
        &mut self,
        world: &mut BlockWorld,
        (base, pending): (&mut Arc<BlockMap>, &mut Arc<BlockEdits>),
        (shown, made): (&Area, &Area),
    ) -> bool {
        voxear_core::profile_scope!("Let Go");
        let hidden: Vec<[i32; 2]> = self.drawn.iter().copied().filter(|at| !shown.contains(*at)).collect();
        for at in &hidden {
            self.drawn.remove(at);
        }
        let gone: Vec<[i32; 2]> = self.near.iter().copied().filter(|at| !made.contains(*at)).collect();
        if hidden.is_empty() && gone.is_empty() {
            return false;
        }
        let leaving = |key: &[i32; 3]| gone.contains(&[key[0], key[2]]);
        let mut was = BlockMap::default();
        let mut now = BlockMap::default();
        for (key, chunk) in base.chunks().filter(|(key, _)| leaving(key)) {
            was.put_shared(*key, chunk.clone());
        }
        for (key, chunk) in world.blocks.chunks().filter(|(key, _)| leaving(key)) {
            now.put_shared(*key, chunk.clone());
        }
        let changes = BlockEdits::between(&was, &now);
        // The coarse columns over what was built are made again with it, so it shows as the camera goes.
        for key in changes.chunks.keys() {
            for level in 1..MOST_LEVELS {
                let at = [key[0] >> level, key[2] >> level];
                let made = self.coarse.contains_key(&(level, at)) || self.coarse_blocks.contains_key(&(level, at));
                if made {
                    self.stale.insert((level, at));
                }
            }
        }
        Arc::make_mut(pending).chunks.extend(changes.chunks);
        let blocks = Arc::make_mut(&mut world.blocks);
        let kept = Arc::make_mut(base);
        for (key, _) in now.chunks() {
            blocks.put_chunk(*key, Chunk::filled(super::chunk::AIR));
            kept.put_chunk(*key, Chunk::filled(super::chunk::AIR));
        }
        for at in &gone {
            self.near.remove(at);
            self.drawn.remove(at);
        }
        if !gone.is_empty() && world.light.size().0 > 0 {
            Arc::make_mut(&mut world.light).forget_columns(&gone);
        }
        world.columns = Some(Arc::new(self.near.iter().copied().collect()));
        let undrawn = |key: &[i32; 3]| gone.contains(&[key[0], key[2]]) || hidden.contains(&[key[0], key[2]]);
        Arc::make_mut(&mut world.meshes).retain(|key, _| !undrawn(key));
        true
    }

    /// Where the first place the world is seen from will be a little while
    /// on, by how fast it has moved: what it flies towards is made first.
    /// None while it keeps still.
    fn lead(&mut self, first: Option<&StreamFrom>, making: &BlockMaking) -> Option<[f32; 2]> {
        let first = first?;
        let now = std::time::Instant::now();
        if let Some((was, when)) = self.moving {
            let seconds = now.duration_since(when).as_secs_f32();
            if seconds > 0.0 {
                // Eased towards how fast it moved since, over about a quarter of a second.
                let share = (seconds * 4.0).min(1.0);
                for axis in 0..2 {
                    let speed = (first.at[axis] - was[axis]) / seconds;
                    self.velocity[axis] += (speed - self.velocity[axis]) * share;
                }
            }
        }
        self.moving = Some((first.at, now));
        let ahead = self.velocity.map(|v| v * LEAD_SECONDS);
        let length = (ahead[0] * ahead[0] + ahead[1] * ahead[1]).sqrt();
        let most = LEAD_COLUMNS * CHUNK as f32 * making.file.block_size.max(0.01);
        (length > 1.0).then(|| {
            let scale = (most / length).min(1.0);
            [first.at[0] + ahead[0] * scale, first.at[1] + ahead[1] * scale]
        })
    }

    /// Asks the background threads for what is still to make, nearest
    /// first, as many as there is room for: the faces of near columns whose
    /// blocks and their neighbours' are made, the blocks of near columns,
    /// and coarse columns. Nearest counts in metres, from where the world
    /// is seen from or where the first place is heading, `lead`; each
    /// coarser ring a little further off than it is, and a coarse column
    /// whose ground finer columns are kept over after everything else.
    fn ask(
        &mut self,
        world: &BlockWorld,
        making: &Arc<BlockMaking>,
        (windows, fine): (&[Area], Option<Window>),
        (from, pending, lead): (&[StreamFrom], &BlockEdits, Option<[f32; 2]>),
    ) {
        voxear_core::profile_scope!("Ask");
        let (shown, made) = (windows[0].clone(), windows[0].grown());
        let mut columns = None;
        let smooth = making.file.smooth;
        // What to make: its level (0 for a near column's blocks, and none for its faces), place and how far away.
        let mut wanted: Vec<(Option<u32>, [i32; 2], i64)> = Vec::new();
        // And the smooth coarse columns whose faces to make, their blocks and those round them made.
        let mut joins: Vec<(u32, [i32; 2], i64)> = Vec::new();
        let column = CHUNK as f32 * making.file.block_size.max(0.01);
        let places: Vec<[f32; 2]> = from.iter().map(|place| place.at).chain(lead).collect();
        for (level, window) in windows.iter().enumerate() {
            let level = level as u32;
            let size = column * (1u32 << level) as f32;
            // How far a column is from the nearest place, in metres, a ring further off for each level; and past all
            // the rest where the finer level's windows hold the four columns over it.
            let finer = level.checked_sub(1).and_then(|finer| windows.get(finer as usize));
            // The first coarser ring's ground the world's own blocks drawn past where they are made lie over, too.
            let fine_over = |child: [i32; 2]| level == 1 && fine.is_some_and(|fine| fine.contains(child));
            let away = |at: [i32; 2]| {
                let from = |place: &[f32; 2]| {
                    let off =
                        |axis: usize| ((place[axis] - (at[axis] as f32 + 0.5) * size).abs() - size * 0.5).max(0.0);
                    (off(0) * off(0) + off(1) * off(1)).sqrt()
                };
                let near = places.iter().map(from).fold(f32::MAX, f32::min);
                let under = finer.is_some_and(|finer| {
                    (0..4).all(|child| {
                        let child = [at[0] * 2 + (child & 1), at[1] * 2 + (child >> 1)];
                        finer.contains(child) || fine_over(child)
                    })
                });
                (near + level as f32 * column * 2.0 + if under { COVERED } else { 0.0 }) as i64
            };
            if level == 0 {
                for at in shown.columns() {
                    let ready = round(at).all(|column| self.near.contains(&column));
                    if ready && !self.drawn.contains(&at) && !self.meshing.contains(&at) {
                        // Before blocks as far away, as it is what shows.
                        wanted.push((None, at, away(at) - column as i64));
                    }
                }
            }
            if level > 0 && smooth {
                for at in window.columns() {
                    let key = (level, at);
                    let ready = round(at).all(|column| self.coarse_blocks.contains_key(&(level, column)));
                    if ready && !self.coarse.contains_key(&key) && !self.joining.contains(&key) {
                        joins.push((level, at, away(at) - column as i64));
                    }
                }
            }
            let window = if level == 0 {
                made.clone()
            } else if smooth {
                window.grown()
            } else {
                window.clone()
            };
            for at in window.columns() {
                let done = if level == 0 {
                    self.near.contains(&at)
                } else if smooth {
                    // A column gone from the blocks kept but drawn still needs none.
                    self.coarse_blocks.contains_key(&(level, at))
                } else {
                    self.coarse.contains_key(&(level, at))
                };
                let stale = self.stale.contains(&(level, at));
                if done && !stale || self.asked.contains(&(level, at)) {
                    continue;
                }
                wanted.push((Some(level), at, away(at)));
            }
        }
        // The world's own blocks drawn past where they are made, round the first place, out to its first coarser
        // ring: by how far off they are as near columns are, and so before coarse ones as far.
        if let Some(fine) = fine {
            let away = |at: [i32; 2]| {
                let from = |place: &[f32; 2]| {
                    let off =
                        |axis: usize| ((place[axis] - (at[axis] as f32 + 0.5) * column).abs() - column * 0.5).max(0.0);
                    (off(0) * off(0) + off(1) * off(1)).sqrt()
                };
                places.iter().map(from).fold(f32::MAX, f32::min) as i64
            };
            for at in fine.columns().filter(|at| !windows[0].contains(*at)) {
                if self.fine.contains_key(&at) || self.asked.contains(&(FINE, at)) {
                    continue;
                }
                wanted.push((Some(FINE), at, away(at)));
            }
        }
        wanted.sort_by_key(|(_, _, away)| *away);
        joins.sort_by_key(|(_, _, away)| *away);
        let mut room = self.at_once.max(1).saturating_sub(self.making_now.load(Ordering::Acquire));
        // Faces first: they are what shows, and are quick beside blocks.
        let joined = joins.len().min(room);
        self.waiting = wanted.len() + (joins.len() - joined);
        for (level, at, _) in joins.into_iter().take(joined) {
            self.making_now.fetch_add(1, Ordering::AcqRel);
            self.joining.insert((level, at));
            let (making, latest, making_now, send) =
                (making.clone(), self.latest.clone(), self.making_now.clone(), self.send.clone());
            let number = self.number;
            let columns: Vec<ColumnChunks> =
                round(at).filter_map(|column| self.coarse_blocks.get(&(level, column)).cloned()).collect();
            crate::workers::spawn(move || {
                if latest.load(Ordering::Acquire) == number {
                    let mut from = BlockMap::default();
                    for (key, chunk) in columns.iter().flat_map(|column| column.iter()) {
                        if !chunk.is_air() {
                            from.put_shared(*key, chunk.clone());
                        }
                    }
                    let keys: Vec<[i32; 3]> =
                        from.chunks().map(|(key, _)| *key).filter(|key| [key[0], key[2]] == at).collect();
                    let meshes = keys
                        .into_iter()
                        .map(|key| (key, super::smooth::mesh_skirted(&from, key, &making.faces, &|_| true)))
                        .collect();
                    let _ = send.send(Finished::Joined { number, level, at, meshes });
                }
                making_now.fetch_sub(1, Ordering::AcqRel);
            });
        }
        room -= joined;
        // Some of the rings are always being made, however much nearer there is to make, so the horizon keeps up
        // with a camera flying on: the nearest of them first, as many as make a twelfth of those at once, none
        // where so few are made at once that a twelfth is none.
        let ring = |level: &Option<u32>| level.is_some_and(|level| level > 0 && level != FINE);
        let short = (self.at_once / RINGS_ALWAYS).saturating_sub(self.rings_now.load(Ordering::Acquire));
        let rings: Vec<usize> = (0..wanted.len()).filter(|&at| ring(&wanted[at].0)).take(short).collect();
        let first: Vec<(Option<u32>, [i32; 2], i64)> = rings.iter().rev().map(|&at| wanted.remove(at)).collect();
        wanted.splice(0..0, first.into_iter().rev());
        // The far view's tiles take a while each: no more of them at once than leaves most threads to what is near.
        let far_room = (self.at_once / 4).max(1);
        for (level, at, _) in wanted {
            let far = level.is_some_and(|level| level >= FAR_FROM && level != FINE);
            if room == 0 {
                break;
            }
            if far && self.far_now.load(Ordering::Acquire) >= far_room {
                continue;
            }
            room -= 1;
            self.waiting -= 1;
            // Counted among the far view's tiles and the rings being made while it is.
            let counts: Vec<Arc<AtomicUsize>> = [(far, &self.far_now), (ring(&level), &self.rings_now)]
                .into_iter()
                .filter(|(counted, _)| *counted)
                .map(|(_, count)| count.clone())
                .collect();
            for count in &counts {
                count.fetch_add(1, Ordering::AcqRel);
            }
            self.making_now.fetch_add(1, Ordering::AcqRel);
            let (making, latest, making_now, send) =
                (making.clone(), self.latest.clone(), self.making_now.clone(), self.send.clone());
            let number = self.number;
            let Some(level) = level else {
                // A near column's faces, from its chunks and those round it as they are now.
                self.meshing.insert(at);
                let columns = columns.get_or_insert_with(|| columns_of(&world.blocks));
                let from = round_chunks(at, columns, &world.blocks);
                crate::workers::spawn(move || {
                    if latest.load(Ordering::Acquire) == number {
                        let keys = from.chunks().map(|(key, _)| *key).filter(|key| [key[0], key[2]] == at);
                        let keys: Vec<[i32; 3]> = keys.collect();
                        // A world of cubes' light, worked out from the columns round it, as its faces are made.
                        let (light, lit) = match (making.file.smooth, heights(&from)) {
                            (false, Some(heights)) => {
                                let (light, lit) = light_column(&from, &making.faces, at, heights);
                                (light, Some(lit))
                            }
                            _ => (Vec::new(), None),
                        };
                        let meshes = keys
                            .into_iter()
                            .map(|key| {
                                let lit = lit.as_ref().map(|lit| lit as &dyn LightAt);
                                (key, mesh_lit(&from, key, &making.faces, making.file.smooth, &|_| true, lit))
                            })
                            .collect();
                        let _ = send.send(Finished::Meshed { number, at, meshes, light, from });
                    }
                    making_now.fetch_sub(1, Ordering::AcqRel);
                });
                continue;
            };
            self.asked.insert((level, at));
            let far_tiles = self.far_tiles.clone();
            // What was built on the ground a coarse column covers, kept for chunks not near.
            let mut built = BlockEdits::default();
            // Drawn alone, the world's own blocks past where they are made are made as a coarse column's are.
            let (fine, level) = if level == FINE { (true, 0) } else { (false, level) };
            if level > 0 || fine {
                let under = |key: &[i32; 3]| [key[0] >> level, key[2] >> level] == at;
                built.chunks.extend(pending.chunks.iter().filter(|(key, _)| under(key)).map(|(k, c)| (*k, c.clone())));
            }
            let stamp = making.built_stamp();
            crate::workers::spawn(move || {
                if latest.load(Ordering::Acquire) == number {
                    let mut chunks = if level >= FAR_FROM {
                        let mut tile = far_tile_kept(&making, far_tiles.as_deref(), level, at);
                        making.build_into(level, &mut tile);
                        far_chunks(&tile, making.file.smooth)
                    } else {
                        let mut chunks = column_chunks(&making.file, &making.generate, &making.world, level, at, None);
                        // What is built into a ring this near is built into the tile its ground would be, and put
                        // into its chunks.
                        if level > 0 && making.built_from().is_some_and(|first| first <= level) {
                            let bare = far_tile(&making.file, &making.generate, &making.world, level, at);
                            let mut built = bare.clone();
                            making.build_into(level, &mut built);
                            put_built(&mut chunks, &bare, &built);
                        }
                        chunks
                    };
                    raise(&mut chunks, &built, level, making.file.smooth);
                    // A coarse column's blocks of parts as cubes, or as air where they are too thin to see.
                    let any_part = making.faces.iter().any(|faces| faces.part & super::PART_DRAWN != 0);
                    if (level > 0 || fine) && any_part {
                        for (_, chunk) in &mut chunks {
                            if chunk.may_hold(|block| super::is_part(&making.faces, block)) {
                                *chunk = chunk.remapped(|block| super::far_block(&making.faces, block));
                            }
                        }
                    }
                    // A coarse column's faces made here, alone: the columns beside it not made.
                    let meshes = if level == 0 && !fine || making.file.smooth {
                        Vec::new()
                    } else {
                        let mut alone = BlockMap::default();
                        for (key, chunk) in &chunks {
                            alone.put_chunk(*key, chunk.clone());
                        }
                        let far = if any_part { super::far_faces(&making.faces) } else { making.faces.to_vec() };
                        // Its light from its own blocks alone, the columns beside it taken to be open sky.
                        let lit = match (making.file.smooth, heights(&alone)) {
                            (false, Some(heights)) => Some(light_column(&alone, &far, at, heights).1),
                            _ => None,
                        };
                        chunks
                            .iter()
                            .map(|(key, _)| {
                                let light = lit.as_ref().map(|lit| lit as &dyn LightAt);
                                let mesh = mesh_lit(&alone, *key, &far, making.file.smooth, &|c| c == at, light);
                                (*key, mesh)
                            })
                            .collect()
                    };
                    let level = if fine { FINE } else { level };
                    let _ = send.send(Finished::Made { number, level, at, chunks, meshes, built: stamp });
                }
                for count in counts {
                    count.fetch_sub(1, Ordering::AcqRel);
                }
                making_now.fetch_sub(1, Ordering::AcqRel);
            });
        }
    }

    /// Draws, from each place the world is seen from, each coarse column
    /// where the four finer columns under it are not all drawn from there:
    /// each place by its own windows, as the detail kept round another
    /// place lies under this one's coarser rings. The coarsest columns show
    /// from every place, wherever they are. True if what is drawn changed.
    fn show_coarse(
        &mut self,
        world: &mut BlockWorld,
        (by_place, fine): (&[Vec<Window>], Option<Window>),
        from: &[StreamFrom],
    ) -> bool {
        voxear_core::profile_scope!("Coarse");
        // Under the first place, the world's own blocks drawn past where they are made count as its own.
        let drawn_fine = |level: u32, at: [i32; 2], first: bool| match level {
            0 => self.drawn.contains(&at) || first && self.fine.contains_key(&at),
            _ => self.coarse.contains_key(&(level, at)),
        };
        let now = super::fade_clock();
        let mut covered_since = std::mem::take(&mut self.covered_since);
        covered_since.retain(|key, _| self.coarse.contains_key(key));
        let mut fades = RingFades::default();
        let mut fade_over = f32::MAX;
        let mut shown_columns: CoordSet<(u32, [i32; 2])> = CoordSet::default();
        let mut places = Vec::with_capacity(by_place.len());
        for (number, (windows, place)) in by_place.iter().zip(from).enumerate() {
            let top = windows.len() - 1;
            // Round where the world is looked at from, its rings fade into each other (see `RingFades`): out to as far
            // as its own blocks' window always reaches, the last column of it a band.
            let rings =
                self.making.as_ref().filter(|making| number == 0 && top > 0 && !making.file.smooth).map(|making| {
                    let column = CHUNK as f32 * making.file.block_size.max(0.01);
                    let reach = (place.distance / column).ceil().max(1.0) as i32;
                    (pushed_reach(reach, place.pushed()) as f32 * column, column)
                });
            if let Some((reach, band)) = rings {
                fades = RingFades { at: place.at, reach, band, coarsest: top as u32, ..RingFades::default() };
            }
            let own = self.drawn.iter().filter(|at| windows[0].contains(**at));
            let mut columns: CoordSet<[i32; 3]> = own.map(|at| [0, at[0], at[1]]).collect();
            // Past them, round the first place, its own blocks made for drawing alone, where the near are not drawn.
            let first = number == 0 && fine.is_some();
            if let Some(fine) = fine.filter(|_| first) {
                let near = |at: &[i32; 2]| windows[0].contains(*at) && self.drawn.contains(at);
                for at in self.fine.keys().filter(|at| fine.contains(**at) && !near(at)) {
                    columns.insert([-1, at[0], at[1]]);
                    shown_columns.insert((FINE, *at));
                }
            }
            for (level, at) in self.coarse.keys() {
                let index = *level as usize;
                let mine = index >= top || windows[index].contains(*at);
                let finer = match (index, fine) {
                    (1, Some(fine)) if first => fine,
                    _ => windows[(index - 1).min(top)],
                };
                let covered = (0..4).all(|i| {
                    let child = [at[0] * 2 + (i & 1), at[1] * 2 + (i >> 1)];
                    finer.contains(child) && drawn_fine(level - 1, child, first)
                });
                let shown = match rings {
                    Some((reach, band)) if mine => {
                        // Covered once, it stays so: the finer columns stay seen as one leaves the window.
                        let since = covered_since.entry((*level, *at)).or_insert(match covered {
                            // Covered when first seen, as all of it is made at once: nothing to fade.
                            true => now - RING_FADE_SECONDS,
                            false => f32::MAX,
                        });
                        if covered && *since == f32::MAX {
                            *since = now;
                        }
                        let fades_now = *since != f32::MAX && now - *since < RING_FADE_SECONDS;
                        if fades_now {
                            fade_over = fade_over.min(*since + RING_FADE_SECONDS);
                        }
                        // Under the band where the finer ring fades out, or past it.
                        let scale = (1u32 << (level - 1)) as f32;
                        let size = CHUNK as f32 * self.making.as_ref().map_or(1.0, |m| m.file.block_size) * scale * 2.0;
                        let furthest = (0..2)
                            .map(|axis| {
                                let (low, high) = (at[axis] as f32 * size, (at[axis] + 1) as f32 * size);
                                (low - place.at[axis]).abs().max((high - place.at[axis]).abs())
                            })
                            .fold(0.0f32, f32::max);
                        let banded = furthest > (reach - band) * scale;
                        let shown = !covered || fades_now || banded;
                        if shown {
                            fades.covered.insert([*level as i32, at[0], at[1]], *since);
                        }
                        shown
                    }
                    _ => mine && !covered,
                };
                if !shown {
                    continue;
                }
                columns.insert([*level as i32, at[0], at[1]]);
                shown_columns.insert((*level, *at));
            }
            places.push(super::BlockPlace { at: place.at, columns });
        }
        // Seen from one place alone, everything made shows from anywhere, as it always did.
        if places.len() < 2 {
            places.clear();
        }
        // The far chunks put together again only where the columns shown, or one's faces, changed since: with
        // thousands of chunks in the far view, the columns shown are far quicker to look over.
        let made = |&(level, at): &(u32, [i32; 2])| {
            let number = self.coarse_made.get(&(level, at)).copied().unwrap_or(0);
            let mut h = u64::from(level) << 58 ^ u64::from(at[0] as u32) << 29 ^ u64::from(at[1] as u32) ^ number << 7;
            // Mixed, so a sum of them tells sets apart.
            h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            h ^ (h >> 31)
        };
        let far_from = shown_columns.iter().map(made).fold(shown_columns.len() as u64, u64::wrapping_add);
        let same_far =
            far_from == self.far_from.0 && self.far_from.1.as_ref().is_some_and(|far| Arc::ptr_eq(far, &world.far));
        if !same_far {
            let mut far: HashMap<[i32; 4], Arc<ChunkMesh>> = HashMap::new();
            for (level, at) in &shown_columns {
                let (meshes, level) = match *level {
                    FINE => (self.fine.get(at), -1),
                    level => (self.coarse.get(&(level, *at)), level as i32),
                };
                for (key, mesh) in
                    meshes.map_or(&[][..], |meshes| &meshes[..]).iter().filter(|(_, mesh)| !mesh.is_empty())
                {
                    far.insert([level, key[0], key[1], key[2]], mesh.clone());
                }
            }
            world.far = Arc::new(far);
            self.far_from = (far_from, Some(world.far.clone()));
        }
        let same_places = *world.places == places;
        if !same_places {
            world.places = Arc::new(places);
        }
        self.covered_since = covered_since;
        self.fading = fade_over != f32::MAX;
        self.fade_over = fade_over;
        let same_fades = *world.fades == fades;
        if !same_fades {
            world.fades = Arc::new(fades);
        }
        !same_far || !same_places || !same_fades
    }
}

/// Puts into `chunks`, a coarse column's, every block of far tile `built`
/// that tile `bare`, the same ground with nothing built into it, has not.
fn put_built(chunks: &mut Vec<([i32; 3], Chunk)>, bare: &FarTile, built: &FarTile) {
    let side = CHUNK as i32;
    let block_at =
        |runs: &[far::Run], y: i32| runs.iter().find(|run| run.from <= y && y < run.to).map_or(0, |run| run.block);
    for z in 0..CHUNK {
        for x in 0..CHUNK {
            let (was, now) = (bare.column([x, z]), built.column([x, z]));
            if was == now {
                continue;
            }
            let low = was.iter().chain(now).map(|run| run.from).min().unwrap_or(0);
            let high = was.iter().chain(now).map(|run| run.to).max().unwrap_or(0);
            for y in (low..high).filter(|&y| block_at(was, y) != block_at(now, y)) {
                let key = [built.at[0], y.div_euclid(side), built.at[1]];
                let found = chunks.iter().position(|(at, _)| *at == key);
                let at = found.unwrap_or_else(|| {
                    chunks.push((key, Chunk::filled(0)));
                    chunks.len() - 1
                });
                chunks[at].1.set(x, y.rem_euclid(side) as u32, z, block_at(now, y));
            }
        }
    }
}

/// How many of a coarse block's blocks must have been built for it to be
/// made of what was built, as a part of all of them: an eighth, so a large
/// building shows far off and a pole does not grow into a tower.
const BUILT_SHOWS: u32 = 8;

/// Puts what `built` placed into `chunks`, of level `level`'s coarser
/// blocks: each coarse block that was air made of the last block built in
/// it, where at least an eighth of it was built, so what was built near
/// shows from far off. Taking ground out shows only once it is near.
fn raise(chunks: &mut Vec<([i32; 3], Chunk)>, built: &BlockEdits, level: u32, smooth: bool) {
    if built.chunks.is_empty() {
        return;
    }
    let side = CHUNK as i32;
    let mut cells: HashMap<[i32; 3], (u32, super::chunk::BlockId)> = HashMap::new();
    for (place, block) in built.placed() {
        let cell = cells.entry(place.map(|v| v >> level)).or_insert((0, block));
        *cell = (cell.0 + 1, block);
    }
    let whole = 1u32 << (3 * level.min(10));
    let mut found: HashMap<[i32; 3], usize> = chunks.iter().enumerate().map(|(at, (key, _))| (*key, at)).collect();
    for (cell, (count, block)) in cells {
        if count * BUILT_SHOWS < whole {
            continue;
        }
        let key = cell.map(|v| v.div_euclid(side));
        let at = *found.entry(key).or_insert_with(|| {
            chunks.push((key, Chunk::filled(super::chunk::AIR)));
            chunks.len() - 1
        });
        let [x, y, z] = cell.map(|v| v.rem_euclid(side) as u32);
        let chunk = &mut chunks[at].1;
        if chunk.get(x, y, z) != super::chunk::AIR {
            continue;
        }
        match smooth {
            true => chunk.set_cell(x, y, z, block, super::chunk::FULL),
            false => chunk.set(x, y, z, block),
        }
    }
}

/// Each column's chunks in `blocks`, by its place along x and z.
fn columns_of(blocks: &BlockMap) -> HashMap<[i32; 2], Vec<[i32; 3]>> {
    let mut columns: HashMap<[i32; 2], Vec<[i32; 3]>> = HashMap::new();
    for (key, _) in blocks.chunks() {
        columns.entry([key[0], key[2]]).or_default().push(*key);
    }
    columns
}

/// The chunks of the nine columns round column `at`, as `blocks` has them,
/// shared, found by `columns` (see [`columns_of`]).
fn round_chunks(at: [i32; 2], columns: &HashMap<[i32; 2], Vec<[i32; 3]>>, blocks: &BlockMap) -> BlockMap {
    let mut map = BlockMap::default();
    for key in round(at).filter_map(|column| columns.get(&column)).flatten() {
        if let Some(chunk) = blocks.chunk(*key) {
            map.put_shared(*key, chunk.clone());
        }
    }
    map
}

/// Whether two maps hold the same chunks, shared, none more and none less.
fn same_chunks(now: &BlockMap, was: &BlockMap) -> bool {
    now.len() == was.len()
        && now.chunks().all(|(key, chunk)| was.chunk(*key).is_some_and(|kept| Arc::ptr_eq(kept, chunk)))
}

/// The changes `pending` keeps for chunks not near now, with those `base`
/// and `blocks` show for the chunks near: all of a world without end's
/// changes, for saving.
pub fn all_changes(base: &BlockMap, blocks: &BlockMap, pending: &BlockEdits) -> BlockEdits {
    let mut all = pending.clone();
    all.chunks.extend(BlockEdits::between(base, blocks).chunks);
    all.chunks.retain(|_, change| match change {
        ChunkChange::Blocks(list) => !list.is_empty(),
        ChunkChange::Cells(list) => !list.is_empty(),
        ChunkChange::Whole(_) => true,
    });
    all
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        blocks::{
            chunk::{AIR, BlockId},
            file::{BlockKind, Ground},
            mesh::{FACINGS, unpack, water_shape},
            mesh_any,
            smooth::{POINT_FROM, POINT_STEPS},
        },
        generate::{Landmass, Preset, WorldGen},
    };

    fn making() -> BlockMaking {
        let file = BlockFile {
            block_size: 2.0,
            ground: Ground { size: [0, 0], ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("hills".into()),
                caves: 0.0,
                bottom: -8,
                ..BlockGenerate::default()
            }),
            ..BlockFile::default()
        };
        let (_, faces) = file.faces();
        let generate = file.generate.clone().unwrap();
        let world = Arc::new(Preset::RollingHills.world());
        BlockMaking { file, generate, world, faces: faces.into(), far_folder: None, far_built: Vec::new() }
    }

    /// Brings `world` round `at` and waits until every column is made.
    fn settle(
        streamer: &mut BlockStreamer,
        world: &mut BlockWorld,
        (base, pending): (&mut Arc<BlockMap>, &mut Arc<BlockEdits>),
        making: &BlockMaking,
        at: [f32; 2],
    ) {
        // A minute at most: on a busy machine its background work can take a while.
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < 60 {
            streamer.update(world, base, pending, making.clone(), (100.0, 600.0), at);
            if streamer.settled() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("never settled");
    }

    #[test]
    fn what_was_built_shows_in_coarse_chunks_where_it_fills_an_eighth_of_a_coarse_block() {
        // A tower four blocks square and forty high, and a pole a block square beside it.
        let mut built = BlockMap::default();
        for y in 10..50 {
            for (x, z) in (64..68).flat_map(|x| (64..68).map(move |z| (x, z))) {
                built.set([x, y, z], 3);
            }
            built.set([100, y, 100], 3);
        }
        let built = BlockEdits::between(&BlockMap::default(), &built);
        // Blocks four times as large: the tower a block square, the pole too thin to show.
        let mut chunks = vec![([0, 0, 0], Chunk::filled(AIR))];
        chunks[0].1.set(16, 5, 16, 1);
        raise(&mut chunks, &built, 2, false);
        let block = |cell: [i32; 3]| {
            let key = cell.map(|v| v.div_euclid(CHUNK as i32));
            let [x, y, z] = cell.map(|v| v.rem_euclid(CHUNK as i32) as u32);
            chunks.iter().find(|(k, _)| *k == key).map_or(AIR, |(_, chunk)| chunk.get(x, y, z))
        };
        assert!((2..=12).all(|y| y == 5 || block([16, y, 16]) == 3), "the tower, half a block at its foot");
        assert_eq!(block([16, 5, 16]), 1, "ground already there kept");
        assert_eq!((block([16, 13, 16]), block([17, 4, 16])), (AIR, AIR), "nothing past it");
        assert!((0..16).all(|y| block([25, y, 25]) == AIR), "a pole a sixteenth of a block across shows not");
    }

    #[test]
    fn a_tower_built_near_stays_in_the_coarse_rings_once_the_camera_goes_away() {
        let making = making();
        let fly = |tower: bool| {
            let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, false);
            let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
            let mut streamer = BlockStreamer::default();
            let settle = |streamer: &mut BlockStreamer, world: &mut BlockWorld, base: &mut _, pending: &mut _, at| {
                let start = std::time::Instant::now();
                loop {
                    streamer.update(world, base, pending, making.clone(), (100.0, 3000.0), at);
                    if streamer.settled() {
                        return;
                    }
                    assert!(start.elapsed().as_secs() < 60, "never settled");
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            };
            settle(&mut streamer, &mut world, &mut base, &mut pending, [10.0, 10.0]);
            if tower {
                // Sixteen blocks square and eighty high, standing over the ground.
                let blocks = (16..32).flat_map(|x| (16..32).flat_map(move |z| (60..140).map(move |y| ([x, y, z], 3))));
                world.set_blocks(blocks);
            }
            // Away along x, its near columns let go with the tower kept in what is pending.
            settle(&mut streamer, &mut world, &mut base, &mut pending, [900.0, 10.0]);
            world
        };
        let (bare, built) = (fly(false), fly(true));
        // The tower's faces in the coarse chunks over it, at whichever levels draw them.
        let over = |key: &[i32; 4]| [key[1], key[3]] == [0, 0] && key[0] >= 1;
        let quads = |world: &BlockWorld| -> usize {
            world.far.iter().filter(|(k, _)| over(k)).map(|(_, m)| m.quads.len()).sum()
        };
        assert!(quads(&bare) > 0, "the ground there drawn coarse");
        assert!(quads(&built) > quads(&bare), "and the tower with it: {} against {}", quads(&built), quads(&bare));
    }

    #[test]
    fn what_is_built_into_the_far_view_shows_in_its_tiles_and_is_built_again_when_it_changes_with_no_ground_made_again()
    {
        // A slab over every column of a far tile, as many blocks thick as it says.
        struct Slab(i32, u32);
        impl FarBuilt for Slab {
            fn stamp(&self) -> u64 {
                self.0 as u64
            }
            fn from_level(&self) -> u32 {
                self.1
            }
            fn build(&self, _making: &BlockMaking, tile: &mut FarTile) {
                for z in 0..CHUNK {
                    for x in 0..CHUNK {
                        let top = tile.column([x, z]).last().map(|run| run.to);
                        if let Some(top) = top.filter(|_| self.0 > 0) {
                            tile.add([x, z], top, top + self.0, 3);
                        }
                    }
                }
            }
        }
        // One ground throughout, with one thing or another built into its far view.
        let ground = making();
        let with = |built: Vec<Arc<dyn FarBuilt>>| BlockMaking { far_built: built, ..ground.clone() };
        let mut world = BlockWorld::made(BlockMap::default(), &ground.faces, 2.0, false);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        let mut settle = |streamer: &mut BlockStreamer, world: &mut BlockWorld, making: BlockMaking| {
            let start = std::time::Instant::now();
            loop {
                streamer.update(world, &mut base, &mut pending, making.clone(), (100.0, 3000.0), [10.0, 10.0]);
                if streamer.settled() {
                    return;
                }
                assert!(start.elapsed().as_secs() < 60, "never settled");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        // The highest face of the far view's rings and of the nearer ones, in metres.
        let highest = |world: &BlockWorld, far: bool| -> f32 {
            let rings = world.far.iter().filter(|(key, _)| (key[0] as u32 >= FAR_FROM) == far);
            let top = |key: &[i32; 4], mesh: &ChunkMesh| {
                let block = 2.0 * (1u32 << key[0]) as f32;
                let high = mesh.positions.iter().map(|at| at[1]).fold(f32::MIN, f32::max);
                (key[2] as f32 * CHUNK as f32 + high) * block
            };
            rings.map(|(key, mesh)| top(key, mesh)).fold(f32::MIN, f32::max)
        };
        settle(&mut streamer, &mut world, with(Vec::new()));
        let (bare_far, bare_near) = (highest(&world, true), highest(&world, false));
        let near: HashMap<[i32; 3], Arc<Chunk>> = world.blocks.chunks().map(|(k, c)| (*k, c.clone())).collect();
        let coarse: Vec<([i32; 4], Arc<ChunkMesh>)> =
            world.far.iter().filter(|(k, _)| (k[0] as u32) < FAR_FROM).map(|(k, m)| (*k, m.clone())).collect();
        assert!(!near.is_empty() && !coarse.is_empty());
        // A slab two blocks thick built in: the far view stands higher by it, in each ring's own blocks.
        settle(&mut streamer, &mut world, with(vec![Arc::new(Slab(2, FAR_FROM))]));
        assert!(highest(&world, true) >= bare_far + 2.0 * 16.0, "{} over {bare_far}", highest(&world, true));
        assert_eq!(highest(&world, false), bare_near, "rings made in full have none of it");
        // Nothing of the ground was made again: the near chunks and the nearer rings' faces are the ones they were.
        assert!(near.iter().all(|(key, chunk)| world.blocks.chunk(*key).is_some_and(|now| Arc::ptr_eq(now, chunk))));
        assert!(coarse.iter().all(|(key, mesh)| world.far.get(key).is_some_and(|now| Arc::ptr_eq(now, mesh))));
        // Thicker, then built into the nearer rings too, from the first: those stand higher by it as well, and
        // the world's own chunks are still as they were.
        settle(&mut streamer, &mut world, with(vec![Arc::new(Slab(5, FAR_FROM))]));
        assert!(highest(&world, true) >= bare_far + 5.0 * 16.0);
        assert_eq!(highest(&world, false), bare_near);
        settle(&mut streamer, &mut world, with(vec![Arc::new(Slab(5, 1))]));
        assert!(highest(&world, false) >= bare_near + 5.0 * 4.0, "{} over {bare_near}", highest(&world, false));
        assert!(near.iter().all(|(key, chunk)| world.blocks.chunk(*key).is_some_and(|now| Arc::ptr_eq(now, chunk))));
        // Gone: built on again, as the ground alone makes them.
        settle(&mut streamer, &mut world, with(Vec::new()));
        assert_eq!((highest(&world, true), highest(&world, false)), (bare_far, bare_near));
    }

    #[test]
    fn far_levels_come_from_the_far_view_kept_on_disk_so_they_come_back_as_kept() {
        let folder = std::env::temp_dir().join(format!("voxear-far-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let making = BlockMaking { far_folder: Some(folder.clone()), ..making() };
        let streamed = || {
            let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, false);
            let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
            let mut streamer = BlockStreamer::default();
            let start = std::time::Instant::now();
            loop {
                streamer.update(&mut world, &mut base, &mut pending, making.clone(), (100.0, 6000.0), [10.0, 10.0]);
                if streamer.settled() {
                    return world;
                }
                assert!(start.elapsed().as_secs() < 60, "never settled");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        let world = streamed();
        let levels: HashSet<i32> = world.far.keys().map(|key| key[0]).collect();
        assert!((1..=6).all(|level| levels.contains(&level)), "rings out to 6 km: {levels:?}");
        // Each far tile kept, by level and place, in a folder of what it was made from.
        let kept: Vec<std::path::PathBuf> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(kept.len(), 1);
        let tiles: Vec<String> =
            std::fs::read_dir(&kept[0]).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into()).collect();
        assert!(tiles.iter().all(|name| name.ends_with(".far")) && tiles.iter().any(|name| name.starts_with("3_")));
        assert!(!tiles.iter().any(|name| name.starts_with("2_")), "nearer rings are made in full: {tiles:?}");
        // A tower put into a kept tile shows as the world is made again: the tile was read back, not made anew.
        let (name, key) = world
            .far
            .keys()
            .filter(|key| key[0] == 6)
            .map(|key| (format!("6_{}_{}.far", key[1], key[3]), *key))
            .find(|(name, _)| tiles.contains(name))
            .expect("a level 6 tile drawn");
        let path = kept[0].join(&name);
        let mut tile = FarTile::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        let top = tile.column([16, 16]).last().map_or(0, |run| run.to);
        tile.add([16, 16], top, top + 40, 3);
        std::fs::write(&path, tile.to_bytes()).unwrap();
        let again = streamed();
        let quads = |world: &BlockWorld| -> usize {
            world
                .far
                .iter()
                .filter(|(k, _)| [k[0], k[1], k[3]] == [key[0], key[1], key[3]])
                .map(|(_, m)| m.quads.len())
                .sum()
        };
        assert!(quads(&again) > quads(&world), "the tower's faces too");
        let elsewhere = |world: &BlockWorld| -> Vec<usize> {
            let mut all: Vec<_> =
                world.far.iter().filter(|(k, _)| k[0] == 5).map(|(k, m)| (*k, m.quads.len())).collect();
            all.sort();
            all.into_iter().map(|(_, n)| n).collect()
        };
        assert_eq!(elsewhere(&again), elsewhere(&world), "the rest as it was");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_world_makes_no_more_columns_at_once_than_its_budget_which_takes_hold_as_it_streams() {
        let making = making();
        let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, false);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        // Low: two at once, and never more, however many columns want making.
        let mut streamer = BlockStreamer { at_once: 2, ..BlockStreamer::default() };
        let step = |streamer: &mut BlockStreamer, world: &mut BlockWorld, base: &mut _, pending: &mut _| {
            streamer.update(world, base, pending, making.clone(), (100.0, 600.0), [10.0, 10.0]);
            std::thread::sleep(std::time::Duration::from_millis(2));
            streamer.making_now.load(Ordering::Acquire)
        };
        // Until a few columns are made, however busy the threads are with other work.
        let mut most = 0;
        let start = std::time::Instant::now();
        while streamer.near.len() < 4 {
            assert!(start.elapsed().as_secs() < 60, "nothing made");
            let now = step(&mut streamer, &mut world, &mut base, &mut pending);
            assert!(now <= 2, "{now} columns at once on Low");
            most = most.max(now);
        }
        assert_eq!(most, 2, "two at once");
        assert!(!streamer.settled() && world.blocks.len() > 0, "some made, more to make");
        // Raised as it streams: more at once from the next update, and nothing made already made again.
        let made: Vec<([i32; 3], Arc<Chunk>)> =
            world.blocks.chunks().map(|(key, chunk)| (*key, chunk.clone())).collect();
        let coarse: Vec<((u32, [i32; 2]), Vec<Arc<ChunkMesh>>)> =
            streamer.coarse.iter().map(|(at, meshes)| (*at, meshes.iter().map(|(_, m)| m.clone()).collect())).collect();
        streamer.at_once = 16;
        let mut most = 0;
        let start = std::time::Instant::now();
        while !streamer.settled() {
            assert!(start.elapsed().as_secs() < 60, "never settled");
            let now = step(&mut streamer, &mut world, &mut base, &mut pending);
            assert!(now <= 16);
            most = most.max(now);
        }
        assert!(most > 2, "more at once once raised: {most}");
        for (key, chunk) in made {
            assert!(world.blocks.chunk(key).is_some_and(|now| Arc::ptr_eq(now, &chunk)), "{key:?} kept as made");
        }
        for (at, meshes) in coarse {
            let now = streamer.coarse.get(&at).expect("a coarse column kept");
            assert!(now.iter().zip(&meshes).all(|((_, now), was)| Arc::ptr_eq(now, was)), "{at:?} kept as made");
        }
    }

    #[test]
    fn near_columns_come_first_then_the_rings_outwards_and_ground_under_finer_columns_last() {
        let making = making();
        let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, false);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        // One at a time, so they come in the order they were asked for.
        let mut streamer = BlockStreamer { at_once: 1, ..BlockStreamer::default() };
        let mut came: Vec<(u32, [i32; 2])> = Vec::new();
        let start = std::time::Instant::now();
        loop {
            streamer.update(&mut world, &mut base, &mut pending, making.clone(), (100.0, 600.0), [10.0, 10.0]);
            let near = streamer.near.iter().map(|at| (0, *at));
            let made: Vec<(u32, [i32; 2])> = near.chain(streamer.coarse.keys().copied()).collect();
            let new: Vec<(u32, [i32; 2])> = made.into_iter().filter(|column| !came.contains(column)).collect();
            came.extend(new);
            if streamer.settled() {
                break;
            }
            assert!(start.elapsed().as_secs() < 120, "never settled");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // Every near column drawn before any ring; those made a column further, for the faces of the ones inside
        // them alone, as far off as they are.
        let windows = windows(&making.file, 100.0, 600.0, [10.0, 10.0]);
        let first_coarse = came.iter().position(|(level, _)| *level > 0).expect("rings made");
        let late = came[first_coarse..].iter().filter(|(level, at)| *level == 0 && windows[0].contains(*at));
        assert_eq!(late.count(), 0, "every near column drawn before any ring: {came:?}");
        // A ring's columns under the finer ones show only until those are made: they come after every ring's
        // columns round them.
        let under = |(level, at): &(u32, [i32; 2])| {
            let finer = level.checked_sub(1).map(|finer| &windows[finer as usize]);
            finer.is_some_and(|finer| {
                (0..4).all(|child| finer.contains([at[0] * 2 + (child & 1), at[1] * 2 + (child >> 1)]))
            })
        };
        let first_under = came.iter().position(under).expect("ground under the near columns made");
        assert!(came[first_under..].iter().all(under), "ground under finer columns last: {:?}", &came[first_under..]);
        // The rings out from the camera, the first before the coarser ones round it.
        let ring = |level: u32| came.iter().position(|column| column.0 == level && !under(column));
        assert!(ring(1) < ring(2) && ring(2) < ring(3), "{:?}", [ring(1), ring(2), ring(3)]);
    }

    /// A sea over the whole world settled round the camera, made of cubes or smooth, and its kind of block: its
    /// floor flat 60 m under it, deeper than the coarsest level's top blocks, 3 m up, so the finest level's 2 m blocks
    /// put its top at 2 m, and coarser ones' would put it elsewhere, whole.
    fn streamed_sea(smooth: bool) -> (BlockStreamer, BlockWorld, BlockId) {
        let mut making = making();
        making.file.smooth = smooth;
        making.generate.bottom = -64;
        making.file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let water = making.file.blocks.len() as BlockId;
        making.generate.sea = water;
        making.file.generate = Some(making.generate.clone());
        making.faces = making.file.faces().1.into();
        let flat = voxear_core::curve::Curve::flat;
        let (coast, relief) = (flat(-0.5), flat(0.0));
        let land = Landmass { coast, relief, bumps: 0.0, sea_level: 3.0, ..Preset::RollingHills.landmass() };
        making.world = Arc::new(WorldGen::from(land));
        let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, smooth);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        (streamer, world, water)
    }

    /// Where to put blocks of water in the air over a streamed world: over a column in the window's middle; on
    /// the +x side of the last column made along x, past the drawn window's edge; and on the +x side of the column
    /// inside it.
    fn in_the_air(streamer: &BlockStreamer) -> [[i32; 3]; 3] {
        let middle = [5 * CHUNK as i32 / 2, 30, 5 * CHUNK as i32 / 2];
        let (_, column) = streamer.near.iter().map(|at| (at[0], *at)).max().unwrap();
        let edge = [column[0] * CHUNK as i32 + CHUNK as i32 - 1, 30, column[1] * CHUNK as i32 + 3];
        [middle, edge, [edge[0] - CHUNK as i32, edge[1], edge[2]]]
    }

    #[test]
    fn a_streamed_sea_of_cubes_shows_no_walls_where_its_columns_meet_but_water_meeting_air_shows_its_sides() {
        let (streamer, mut world, water) = streamed_sea(false);
        // Each water quad of a chunk on a side of its column: by the way it faces, the column beside it.
        let on_sides = |mesh: &ChunkMesh| -> Vec<(usize, [i32; 2])> {
            let (first, _) = mesh.water();
            let last = CHUNK - 1;
            mesh.quads[first as usize..]
                .iter()
                .map(|&quad| unpack(quad))
                .filter_map(|(at, _, facing, ..)| match facing {
                    0 if at[0] == last => Some((facing, [1, 0])),
                    1 if at[0] == 0 => Some((facing, [-1, 0])),
                    4 if at[2] == last => Some((facing, [0, 1])),
                    5 if at[2] == 0 => Some((facing, [0, -1])),
                    _ => None,
                })
                .collect()
        };
        let tops = |mesh: &ChunkMesh| mesh.quads[mesh.water().0 as usize..].iter().any(|&q| unpack(q).2 == 2);
        // Near: none towards a column not made, past the window's edge, and every drawn column's neighbours made.
        assert!(world.meshes.values().any(|mesh| tops(mesh)), "the sea's top near");
        let mut edge = 0;
        for (key, mesh) in world.meshes.iter() {
            for (facing, [dx, dz]) in on_sides(mesh) {
                assert!(
                    streamer.near.contains(&[key[0] + dx, key[2] + dz]),
                    "a wall of water at {key:?} facing {facing}"
                );
            }
            let all = [[1, 0], [-1, 0], [0, 1], [0, -1]];
            edge += usize::from(all.iter().any(|[dx, dz]| !streamer.near.contains(&[key[0] + dx, key[2] + dz])));
        }
        assert_eq!(edge, 0, "a drawn column with a column beside it not made");
        // Coarse: none on any side of a column, every column made alone.
        assert!(world.far.values().any(|mesh| tops(mesh)), "the sea's top far");
        for (key, mesh) in world.far.iter() {
            assert_eq!(on_sides(mesh), [], "walls of water on coarse chunk {key:?}");
        }
        // Every level's sea stands as high, so none shows a step where they meet.
        let near = world.meshes.iter().map(|(key, mesh)| (0, key[1], &**mesh));
        let far = world.far.iter().map(|(key, mesh)| (key[0], key[2], &**mesh));
        let mut levels = HashSet::new();
        for (level, cy, mesh) in near.chain(far) {
            for &quad in &mesh.quads[mesh.water().0 as usize..] {
                let ((at, _, facing, ..), (_, drop, _)) = (unpack(quad), water_shape(quad));
                if facing == 2 {
                    let top = (cy * CHUNK as i32 + at[1] as i32 + 1) as f32 - f32::from(drop) / 16.0;
                    assert_eq!(top * 2.0 * (1 << level) as f32, 2.0, "level {level}'s sea");
                    levels.insert(level);
                }
            }
        }
        assert!(levels.len() >= 4, "{levels:?}");
        // Water meets air only at the sea's top: no side shows anywhere.
        let sides = |mesh: &ChunkMesh| {
            mesh.quads[mesh.water().0 as usize..].iter().filter(|&&q| ![2, 3].contains(&unpack(q).2)).count()
        };
        assert_eq!(world.meshes.values().chain(world.far.values()).map(|mesh| sides(mesh)).sum::<usize>(), 0);
        // A block of water put in the air shows every side, though the world goes on; on a column's side, its side
        // towards the column beside it, made, but none past the window's edge.
        let [middle, edge, inner] = in_the_air(&streamer);
        assert_eq!(world.blocks.get(middle), AIR);
        world.set_blocks([(middle, water), (edge, water), (inner, water)]);
        let faces_of = |at: [i32; 3]| -> Vec<usize> {
            let (key, local) = BlockMap::chunk_of(at);
            let mesh = &world.meshes[&key];
            let quads = mesh.quads[mesh.water().0 as usize..].iter().map(|&q| unpack(q));
            quads.filter(|q| q.0 == local).map(|q| q.2).collect()
        };
        assert_eq!(faces_of(middle), (0..FACINGS).collect::<Vec<_>>());
        assert!(faces_of(inner).contains(&0), "towards the column beside it");
        assert!(!faces_of(edge).contains(&0), "past the window's edge");
    }

    /// A smooth mesh's water's corners, in blocks from its chunk's corner.
    fn smooth_water(mesh: &ChunkMesh) -> Vec<[f32; 3]> {
        let words = &mesh.quads[mesh.water().0 as usize..];
        let corner = |pair: &[[u32; 2]]| [pair[0][0] & 0xFFFF, pair[0][0] >> 16, pair[0][1] & 0xFFFF];
        words.chunks(2).map(|pair| corner(pair).map(|q| q as f32 / POINT_STEPS + POINT_FROM)).collect()
    }

    #[test]
    fn a_streamed_smooth_sea_lies_flat_at_its_level_across_its_rings_but_water_meeting_air_shows_its_sides() {
        let (streamer, mut world, water) = streamed_sea(true);
        // Every corner of its water, near and far, at the sea's level: no walls where columns meet, and no steps
        // between the rings.
        let near = world.meshes.iter().map(|(key, mesh)| (0, key[1], &**mesh));
        let far = world.far.iter().map(|(key, mesh)| (key[0], key[2], &**mesh));
        let mut levels = HashSet::new();
        for (level, cy, mesh) in near.chain(far) {
            let size = 2.0 * (1 << level) as f32;
            for at in smooth_water(mesh) {
                let height = (cy as f32 * CHUNK as f32 + at[1]) * size;
                assert!((height - 3.0).abs() < size * 0.02, "level {level}'s sea at {height} m");
                levels.insert(level);
            }
        }
        assert!(levels.len() >= 4, "{levels:?}");
        // A block of water put in the air is closed all round, though the world goes on; on a column's side, closed
        // towards the column beside it, made, but running on past the window's edge.
        let [middle, edge, inner] = in_the_air(&streamer);
        assert_eq!(world.blocks.get(middle), AIR);
        world.set_blocks([(middle, water), (edge, water), (inner, water)]);
        // How far out of its middle the water round a block reaches, each way along each axis.
        let reach = |at: [i32; 3]| -> [[f32; 2]; 3] {
            let (key, local) = BlockMap::chunk_of(at);
            let middle = local.map(|v| v as f32 + 0.5);
            let round = smooth_water(&world.meshes[&key]);
            let near: Vec<_> = round.iter().filter(|p| (0..3).all(|i| (p[i] - middle[i]).abs() < 1.0)).collect();
            assert!(!near.is_empty(), "water round {at:?}");
            std::array::from_fn(|i| {
                let low = near.iter().map(|p| p[i]).fold(f32::MAX, f32::min);
                let high = near.iter().map(|p| p[i]).fold(f32::MIN, f32::max);
                [middle[i] - low, high - middle[i]]
            })
        };
        let closed = |ways: [[f32; 2]; 3]| ways.iter().flatten().all(|&out| (0.1..0.3).contains(&out));
        assert!(closed(reach(middle)), "{:?}", reach(middle));
        assert!(closed(reach(inner)), "towards the column beside it: {:?}", reach(inner));
        let [[_, out], ..] = reach(edge);
        assert!(out > 0.45, "running on past the window's edge: {out}");
    }

    #[test]
    fn a_smooth_worlds_coarse_columns_meet_edge_for_edge_within_their_ring() {
        let mut making = making();
        making.file.smooth = true;
        let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, true);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        // Every coarse chunk with another of its level beside it along x: each point on its side there is a point
        // of the other's, where a column made alone rounded its ground off into a groove.
        let (mut sides, mut points, mut met) = (0, 0, 0);
        let mut levels = HashSet::new();
        for (key, mesh) in world.far.iter() {
            let Some(beside) = world.far.get(&[key[0], key[1] + 1, key[2], key[3]]) else { continue };
            let edge: Vec<[f32; 3]> = mesh.positions.iter().copied().filter(|p| p[0] >= CHUNK as f32 - 0.5).collect();
            if edge.is_empty() {
                continue;
            }
            sides += 1;
            levels.insert(key[0]);
            for p in edge {
                let there = [p[0] - CHUNK as f32, p[1], p[2]];
                points += 1;
                met += usize::from(beside.positions.iter().any(|q| (0..3).all(|k| (q[k] - there[k]).abs() < 2e-3)));
            }
        }
        assert!(sides > 20 && levels.len() >= 2, "{sides} sides at levels {levels:?}");
        assert!(met * 100 >= points * 97, "{met} of {points} points on a side met by the chunk beside it");
        // A column's blocks are kept only a column past where it is drawn.
        let windows = streamer.windows.clone().unwrap();
        for (level, at) in streamer.coarse_blocks.keys() {
            assert!(windows[*level as usize].grown().contains(*at), "level {level}'s {at:?} kept");
        }
    }

    #[test]
    fn chunks_are_kept_round_every_place_a_world_is_seen_from_and_let_go_when_one_goes() {
        let making = making();
        let mut streamer = BlockStreamer::default();
        let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, false);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        // The camera at the middle, and a second place 9 km off, seen less far.
        let camera = StreamFrom { at: [0.0, 0.0], distance: 100.0, far: 600.0, pushed: 1.0 };
        let second = StreamFrom { at: [9000.0, -3000.0], distance: 64.0, far: 300.0, pushed: 1.0 };
        let start = std::time::Instant::now();
        let mut settle = |streamer: &mut BlockStreamer, world: &mut BlockWorld, from: &[StreamFrom]| {
            while start.elapsed().as_secs() < 120 {
                streamer.update_from(world, &mut base, &mut pending, making.clone(), from);
                if streamer.settled() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("never settled");
        };
        assert!(!streamer.drawn_round(second.at, 32.0), "nothing made yet");
        settle(&mut streamer, &mut world, &[camera, second]);
        // Columns are 64 m: both places have their ground, with faces, and nothing in between has.
        let column = |at: [f32; 2]| at.map(|v| (v / 64.0).floor() as i32);
        let has_faces = |world: &BlockWorld, at: [i32; 2]| world.meshes.keys().any(|key| [key[0], key[2]] == at);
        assert!(has_faces(&world, column(camera.at)) && has_faces(&world, column(second.at)));
        assert!(!has_faces(&world, column([4500.0, -1500.0])), "no ground made between the two");
        assert!(streamer.drawn_round(second.at, 32.0) && streamer.drawn_round(camera.at, 64.0));
        assert!(!streamer.drawn_round([4500.0, -1500.0], 32.0));
        // Round the second its coarse ground too, as far as it is seen there and no further.
        let coarse_by = |world: &BlockWorld, at: [f32; 2], within: f32| {
            world.far.keys().any(|key| {
                let size = 64.0 * (1u32 << key[0]) as f32;
                let middle = [(key[1] as f32 + 0.5) * size, (key[3] as f32 + 0.5) * size];
                (middle[0] - at[0]).abs() < within && (middle[1] - at[1]).abs() < within
            })
        };
        assert!(coarse_by(&world, second.at, 400.0), "coarse ground round the second place");
        assert!(!coarse_by(&world, [4500.0, -1500.0], 400.0));
        // From each place the world shows that place's own blocks and rings, and none of the other's detail.
        assert_eq!(world.places.len(), 2);
        let (first, other) = (&world.places[0], &world.places[1]);
        let [cx, cz] = column(camera.at);
        let [sx, sz] = column(second.at);
        assert!(first.columns.contains(&[0, cx, cz]) && !first.columns.contains(&[0, sx, sz]));
        assert!(other.columns.contains(&[0, sx, sz]) && !other.columns.contains(&[0, cx, cz]));
        // Every column shown from a place is one of a level's: no two overlap by part, but where the rings round
        // where the world is looked at from fade into each other, which says so.
        for (number, place) in world.places.iter().enumerate() {
            for &[level, x, z] in &place.columns {
                let under = (0..level).any(|finer| {
                    let scale = 1 << (level - finer);
                    (0..scale).any(|dx| (0..scale).any(|dz| place.columns.contains(&[finer, x * scale + dx, z * scale + dz])))
                });
                let fading = number == 0 && world.fades.covered.get(&[level, x, z]).is_some_and(|&s| s != f32::MAX);
                assert!(!under || fading, "column {x},{z} of level {level} is drawn over finer ones");
            }
        }
        // The coarsest ring shows from both, wherever it was made for: over the second place it shows from the
        // camera, which has none of the detail there, and not from the second place, which has.
        let top = world.far.keys().map(|key| key[0]).max().unwrap();
        let over = |at: [f32; 2]| [top, (at[0] / (64.0 * (1 << top) as f32)).floor() as i32, (at[1] / (64.0 * (1 << top) as f32)).floor() as i32];
        assert!(first.columns.contains(&over(second.at)) && !other.columns.contains(&over(second.at)));
        // From the camera only to fade, where it reaches under the finer ring's band.
        let fading = |column: &[i32; 3]| world.fades.covered.get(column).is_some_and(|&since| since != f32::MAX);
        assert!(other.columns.contains(&over(camera.at)));
        assert!(!first.columns.contains(&over(camera.at)) || fading(&over(camera.at)));
        let both = world.meshes.len();
        // The second place gone: its columns go, the camera's stay.
        settle(&mut streamer, &mut world, &[camera]);
        assert!(!has_faces(&world, column(second.at)) && has_faces(&world, column(camera.at)));
        assert!(world.meshes.len() < both && !coarse_by(&world, second.at, 400.0));
        assert!(!streamer.drawn_round(second.at, 32.0));
        assert!(world.places.is_empty(), "seen from one place, everything shows from anywhere");
    }

    #[test]
    fn windows_reach_twice_as_far_each_level_their_sides_on_the_next_levels_columns() {
        let file = making().file;
        let windows = windows(&file, 128.0, 1000.0, [10.0, -300.0]);
        assert!(windows.len() >= 3);
        for pair in windows.windows(2) {
            let (inner, outer) = (pair[0], pair[1]);
            assert!(inner.low.iter().chain(&inner.high).all(|v| v % 2 == 0), "on the next level's columns");
            assert!(
                outer.low[0] <= inner.low[0] / 2 && outer.high[0] >= inner.high[0] / 2,
                "the coarser holds the finer"
            );
        }
    }

    #[test]
    fn rings_pushed_out_reach_twice_as_far_with_the_worlds_own_blocks_drawn_out_to_the_first() {
        let file = making().file;
        let (near, fine) = windows_out(&file, (128.0, 4000.0), [10.0, -300.0], 1.0);
        assert!(fine.is_none(), "nothing drawn past the world's own blocks at none");
        let (pushed, fine) = windows_out(&file, (128.0, 4000.0), [10.0, -300.0], 2.0);
        let fine = fine.expect("its own blocks drawn on past where they are made");
        let across = |window: &Window| window.high[0] - window.low[0];
        assert_eq!(pushed[0], near[0], "its own blocks made as far as they were");
        assert!(across(&fine) >= 2 * across(&near[0]) - 4, "drawn on twice as far: {fine:?} {:?}", near[0]);
        // Every coarser ring twice as many of its own columns across, holding the one inside it, and one ring fewer.
        assert!(across(&pushed[1]) >= 2 * across(&near[1]) - 4);
        assert!(
            pushed[1].low[0] <= fine.low[0] / 2 && pushed[1].high[0] >= fine.high[0] / 2,
            "the first ring holds them"
        );
        assert_eq!(pushed.len() + 1, near.len());
        // Never past where the world is drawn at all.
        assert_eq!(StreamFrom { at: [0.0; 2], distance: 128.0, far: 192.0, pushed: 4.0 }.pushed(), 1.5);
        // Pushed out by a part of that, part as far.
        let (_, partly) = windows_out(&file, (128.0, 4000.0), [10.0, -300.0], 1.5);
        let partly = partly.expect("drawn on a little past where they are made");
        assert!(across(&partly) > across(&near[0]) && across(&partly) < across(&fine), "{partly:?}");
    }

    #[test]
    fn the_worlds_own_blocks_drawn_past_where_they_are_made_show_where_the_near_ones_do_not() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        // 64 m columns: its own blocks made out to two columns, drawn out to four.
        let from = [StreamFrom { at: [10.0, 10.0], distance: 100.0, far: 1500.0, pushed: 2.0 }];
        let start = std::time::Instant::now();
        while !streamer.settled() || streamer.fine.is_empty() {
            streamer.update_from(&mut world, &mut base, &mut pending, making.clone(), &from);
            assert!(start.elapsed().as_secs() < 90, "never settled");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::thread::sleep(std::time::Duration::from_secs_f32(RING_FADE_SECONDS + 0.1));
        streamer.update_from(&mut world, &mut base, &mut pending, making.clone(), &from);
        let fine: HashSet<[i32; 2]> = world.far.keys().filter(|key| key[0] == -1).map(|key| [key[1], key[3]]).collect();
        assert!(!fine.is_empty(), "its own blocks drawn past where they are made");
        assert!(fine.iter().all(|at| !streamer.drawn.contains(at)), "never where its own made blocks are drawn");
        // As large as its own: no blocks stood on there, nor kept.
        assert!(
            fine.iter()
                .all(|at| world.blocks.chunks().all(|(key, _)| [key[0], key[2]] != *at || streamer.near.contains(at)))
        );
        // The first coarser ring shows past them, or fading into them under its band, which begins at four columns.
        assert_eq!(world.fades.reach, 256.0);
        for key in world.far.keys().filter(|key| key[0] == 1) {
            let under = (0..4).all(|i| {
                let child = [key[1] * 2 + (i & 1), key[3] * 2 + (i >> 1)];
                fine.contains(&child) || streamer.drawn.contains(&child)
            });
            let since = world.fades.covered.get(&[1, key[1], key[3]]).copied();
            assert!(!under || since.is_some_and(|since| since != f32::MAX), "ring 1 at {key:?} over finer ground");
        }
    }

    #[test]
    fn more_detail_streams_more_columns_and_rings_that_still_meet_without_overlap_or_gaps() {
        use crate::{WorldBudget, WorldQuality};
        let making = making();
        // A world that says 128 m of its own blocks and 1 km of rings, at High and at Ultra.
        let streamed = |budget: WorldBudget| {
            let near = budget.near(128.0);
            let distances = (near, budget.far(1024.0, near));
            let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
            let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
            let mut streamer = BlockStreamer::default();
            let start = std::time::Instant::now();
            loop {
                streamer.update(&mut world, &mut base, &mut pending, making.clone(), distances, [10.0, 10.0]);
                if streamer.settled() {
                    break;
                }
                assert!(start.elapsed().as_secs() < 120, "never settled");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            (streamer, world, distances)
        };
        let (high, high_world, high_reach) = streamed(WorldQuality::High.budget().unwrap());
        let (ultra, ultra_world, ultra_reach) = streamed(WorldQuality::Ultra.budget().unwrap());
        assert_eq!((high_reach, ultra_reach), ((128.0, 1024.0), (192.0, 2048.0)));
        let (more, fewer) = (ultra.drawn.len(), high.drawn.len());
        assert!(more * 2 > fewer * 3, "{more} near columns, {fewer} at High");
        let levels = |world: &BlockWorld| world.far.keys().map(|key| key[0]).max().unwrap_or(0);
        assert!(levels(&ultra_world) > levels(&high_world), "a ring more round them");
        for (streamer, world, (_, far)) in [(&high, &high_world, high_reach), (&ultra, &ultra_world, ultra_reach)] {
            // Each level's columns drawn: the near ones, and every ring's.
            let mut drawn: HashSet<(i32, [i32; 2])> = streamer.drawn.iter().map(|at| (0, *at)).collect();
            drawn.extend(world.far.keys().map(|key| (key[0], [key[1], key[3]])));
            // No column draws where the four finer ones under it are drawn, but to fade into them (see `RingFades`).
            for &(level, [x, z]) in drawn.iter().filter(|(level, _)| *level > 0) {
                let finer = (0..4).all(|i| drawn.contains(&(level - 1, [x * 2 + (i & 1), z * 2 + (i >> 1)])));
                let fading = world.fades.covered.get(&[level, x, z]).is_some_and(|&since| since != f32::MAX);
                assert!(!finer || fading, "level {level} column {x}, {z} over the finer ones under it");
            }
            // And the ground is drawn all the way out, at one level or another.
            let column = CHUNK as f32 * 2.0;
            for step in 0..(far * 0.9 / 16.0) as i32 {
                let place = [10.0 + step as f32 * 16.0, 10.0 - step as f32 * 9.0];
                let covered = (0..MOST_LEVELS as i32).any(|level| {
                    let size = column * (1 << level) as f32;
                    drawn.contains(&(level, place.map(|v| (v / size).floor() as i32)))
                });
                assert!(covered, "nothing drawn at {place:?}, {far} m out");
            }
        }
    }

    #[test]
    fn faces_alone_changed_are_taken_up_with_nothing_made_again() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        let (number, chunks) = (streamer.number, world.blocks.chunks().count());
        assert!(chunks > 0);
        // A block the world had not held, numbered since: its faces at the end of the world's.
        let mut faces = making.faces.to_vec();
        faces.push(crate::blocks::BlockFaces::water(0));
        let grown = BlockMaking { faces: faces.into(), ..making.clone() };
        streamer.update(&mut world, &mut base, &mut pending, grown.clone(), (100.0, 600.0), [10.0, 10.0]);
        assert_eq!((streamer.number, world.blocks.chunks().count()), (number, chunks), "nothing started again");
        assert_eq!(streamer.making.as_ref().map(|kept| kept.faces.len()), Some(grown.faces.len()));
        assert_eq!(world.faces.len(), grown.faces.len(), "the world builds with them from here on");
        // A block it draws drawn another way is no face more: what was made is made again.
        let mut faces = grown.faces.to_vec();
        faces[1] = crate::blocks::BlockFaces::water(0);
        let redrawn = BlockMaking { faces: faces.into(), ..grown.clone() };
        streamer.update(&mut world, &mut base, &mut pending, redrawn, (100.0, 600.0), [10.0, 10.0]);
        assert_ne!(streamer.number, number, "started again");
        let number = streamer.number;
        // A file that is another starts it again, as before.
        let mut other = grown.clone();
        other.file.ground.under_depth += 1;
        streamer.update(&mut world, &mut base, &mut pending, other, (100.0, 600.0), [10.0, 10.0]);
        assert_ne!(streamer.number, number);
    }

    #[test]
    fn near_columns_come_with_their_changes_and_go_keeping_them_and_coarse_ones_fill_beyond() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        // A change kept for a chunk not made yet: a block of stone high over column (0, 0).
        let mut changed = BlockMap::default();
        changed.set([5, 200, 5], 3);
        Arc::make_mut(&mut pending).chunks.extend(BlockEdits::between(&BlockMap::default(), &changed).chunks);
        let mut streamer = BlockStreamer::default();
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        assert_eq!(world.blocks.get([5, 200, 5]), 3, "the kept change made as its column came");
        assert!(pending.is_empty());
        assert!(!world.meshes.is_empty() && !world.far.is_empty(), "near chunks drawn, and coarse ones beyond");
        // Near columns' ground as their generator makes it.
        let surface = (-8..300).rev().find(|&y| world.blocks.get([20, y, 20]) != 0).unwrap();
        let height = making.world.height_at(41.0, 41.0);
        assert!(((surface + 1) as f32 * 2.0 - height).abs() <= 2.0);
        // Gone 3 km on: the old columns let go, their change kept, coarse ones of the new place round it.
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [3000.0, 10.0]);
        assert_eq!(world.blocks.get([5, 200, 5]), 0);
        assert_eq!(pending.chunks.len(), 1, "the change kept while its column is away");
        assert!(world.blocks.get([1500, 2, 5]) != 0 || world.blocks.chunks().count() > 0);
        // And back: made again with it.
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        assert_eq!(world.blocks.get([5, 200, 5]), 3);
        // A coarse column draws where its four finer ones are made only to fade into them: as they come, or under
        // the band where they fade out; and says when they came, for the renderer to fade the two into each other.
        for key in world.far.keys().filter(|k| k[0] == 1) {
            let child = [key[1] * 2, key[3] * 2];
            let all = (0..4).all(|i| streamer.drawn.contains(&[child[0] + (i & 1), child[1] + (i >> 1)]));
            let since = world.fades.covered.get(&[1, key[1], key[3]]).copied();
            assert!(!all || since.is_some_and(|since| since != f32::MAX), "coarse column {key:?} over near ones");
        }
    }

    #[test]
    fn rings_fade_into_each_other_over_a_band_round_where_the_world_is_looked_at_from() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        let at = [10.0, 10.0];
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, at);
        // Once those covered last have faded.
        std::thread::sleep(std::time::Duration::from_secs_f32(RING_FADE_SECONDS + 0.1));
        streamer.update(&mut world, &mut base, &mut pending, making.clone(), (100.0, 600.0), at);
        // Columns of 64 m: its own blocks drawn whole out to two columns, the last of them a band.
        let fades = world.fades.clone();
        assert_eq!((fades.at, fades.reach, fades.band), (at, 128.0, 64.0));
        assert!(fades.coarsest > 1);
        // The first coarse ring shows under that band though its finer columns are all made there, and they are
        // marked as made, so they fade in over it as it fades out.
        let size = 128.0;
        let furthest = |x: i32, z: i32| {
            let edge = |low: i32, at: f32| (low as f32 * size - at).abs().max(((low + 1) as f32 * size - at).abs());
            edge(x, at[0]).max(edge(z, at[1]))
        };
        let under_band: Vec<[i32; 4]> = world
            .far
            .keys()
            .filter(|key| key[0] == 1)
            .filter(|key| (0..4).all(|i| streamer.drawn.contains(&[key[1] * 2 + (i & 1), key[3] * 2 + (i >> 1)])))
            .copied()
            .collect();
        assert!(!under_band.is_empty(), "coarse columns shown over made ones under the band");
        for key in &under_band {
            assert!(furthest(key[1], key[3]) > fades.reach - fades.band, "{key:?} reaches into the band");
            assert!(fades.covered.get(&[1, key[1], key[3]]).is_some_and(|&since| since != f32::MAX));
        }
        // Every coarse column shown round there is in its fades, those not covered marked as waiting.
        for key in world.far.keys() {
            assert!(fades.covered.contains_key(&[key[0], key[1], key[3]]), "{key:?} has its fade");
        }
    }

    #[test]
    fn near_columns_faces_are_made_once_in_the_background_as_the_whole_world_would_make_them() {
        for smooth in [false, true] {
            let mut making = making();
            making.file.smooth = smooth;
            let mut world = BlockWorld::made(BlockMap::default(), &making.faces, 2.0, smooth);
            let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
            let mut streamer = BlockStreamer::default();
            let settle = |streamer: &mut BlockStreamer,
                          world: &mut BlockWorld,
                          base: &mut Arc<BlockMap>,
                          pending: &mut Arc<BlockEdits>,
                          at: [f32; 2],
                          change: Option<[i32; 3]>| {
                let mut changed = change;
                let start = std::time::Instant::now();
                while start.elapsed().as_secs() < 60 {
                    streamer.update(world, base, pending, making.clone(), (100.0, 300.0), at);
                    // A change while a column's faces are being made, which it must not lose.
                    if let Some(at) = changed.take_if(|_| !streamer.meshing.is_empty()) {
                        world.set_blocks([(at, 3)]);
                    }
                    if streamer.settled() && changed.is_none() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                panic!("never settled");
            };
            settle(&mut streamer, &mut world, &mut base, &mut pending, [10.0, 10.0], None);
            // Drawn just within the window, their blocks made a column further.
            let window = windows(&making.file, 100.0, 300.0, [10.0, 10.0])[0];
            assert_eq!(streamer.drawn, window.columns().collect::<CoordSet<_>>(), "smooth: {smooth}");
            assert_eq!(streamer.near, window.grown().columns().collect::<CoordSet<_>>(), "smooth: {smooth}");
            // Every drawn chunk's faces as the whole world's blocks make them; none for columns not drawn.
            let whole = |world: &BlockWorld, streamer: &BlockStreamer| {
                for (key, mesh) in world.meshes.iter() {
                    assert!(streamer.drawn.contains(&[key[0], key[2]]), "{key:?} drawn outside the window");
                    // A chunk changed is made again at its first level alone.
                    let made = mesh_any(&world.blocks, *key, &making.faces, smooth);
                    let first = |mesh: &ChunkMesh| {
                        let near = if smooth { mesh.buckets[1] as usize } else { mesh.buckets[FACINGS] as usize };
                        let (first, count) = mesh.water();
                        let water = mesh.quads[first as usize..(first + count) as usize].to_vec();
                        (mesh.quads[..near].to_vec(), water, mesh.positions.clone(), mesh.indices.clone())
                    };
                    assert_eq!(first(mesh), first(&made), "{key:?}, smooth: {smooth}");
                }
            };
            whole(&world, &streamer);
            assert!(world.meshes.len() > 10);
            // Two columns on: those drawn before and still drawn keep their faces, made once; a change made while
            // faces are being made shows.
            let before = world.meshes.clone();
            let top = (-8..400).rev().find(|&y| world.blocks.get([70, y, 20]) != 0).unwrap();
            let changed = [70, top + 1, 20];
            settle(&mut streamer, &mut world, &mut base, &mut pending, [140.0, 10.0], Some(changed));
            whole(&world, &streamer);
            assert_eq!(world.blocks.get(changed), 3);
            let (edited, _) = BlockMap::chunk_of(changed);
            for (key, mesh) in before.iter().filter(|(key, _)| streamer.drawn.contains(&[key[0], key[2]])) {
                let same = world.meshes.get(key).is_some_and(|now| Arc::ptr_eq(now, mesh));
                assert!(same || *key == edited, "{key:?} made again, smooth: {smooth}");
            }
        }
    }
}
