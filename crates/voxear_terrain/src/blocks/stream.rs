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
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
};

use super::{
    BlockMap, BlockWorld,
    chunk::{CHUNK, Chunk},
    edits::{BlockEdits, ChunkChange},
    file::BlockFile,
    generate::{BlockGenerate, column_chunks},
    mesh::ChunkMesh,
    mesh_any, mesh_made,
};
use crate::generate::WorldGen;

/// How many chunk columns are made at once in the background, at most,
/// until the streamer is told otherwise (see `WorldBudget::columns`).
const MOST_MAKING: usize = 24;

/// The most rings of coarser chunks a world draws.
pub const MOST_LEVELS: u32 = 6;

/// What a world of blocks without end's chunks are made from: its file, the
/// way it generates, its generator, and the surfaces its blocks' faces are
/// drawn with.
#[derive(Clone)]
pub struct BlockMaking {
    pub file: BlockFile,
    pub generate: BlockGenerate,
    pub world: Arc<WorldGen>,
    pub faces: Arc<[super::BlockFaces]>,
}

impl BlockMaking {
    fn same(&self, other: &BlockMaking) -> bool {
        self.file == other.file && Arc::ptr_eq(&self.world, &other.world)
    }
}

/// Work done in the background, each by the number of what it was made
/// from.
enum Finished {
    /// A chunk column's chunks, at its level and place, and for a coarse
    /// one their faces.
    Made { number: u64, level: u32, at: [i32; 2], chunks: Vec<([i32; 3], Chunk)>, meshes: Vec<([i32; 3], ChunkMesh)> },
    /// A near column's chunks' faces, and the chunks round it they were made
    /// from.
    Meshed { number: u64, at: [i32; 2], meshes: Vec<([i32; 3], ChunkMesh)>, from: BlockMap },
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

/// The nine chunk columns round `at`, its own among them.
fn round([x, z]: [i32; 2]) -> impl Iterator<Item = [i32; 2]> {
    (-1..=1).flat_map(move |dz| (-1..=1).map(move |dx| [x + dx, z + dz]))
}

/// How far each level's window reaches round `camera`, a point in metres
/// from the world's corner: the near columns out to `distance`, then each
/// coarser level twice as far, out to `far`, every window's sides on its
/// next level's columns, so a coarse column is wholly in or out of the
/// window inside it.
fn windows(file: &BlockFile, distance: f32, far: f32, camera: [f32; 2]) -> Vec<Window> {
    let column = CHUNK as f32 * file.block_size.max(0.01);
    let reach = (distance / column).ceil().max(1.0) as i32;
    let mut windows = Vec::new();
    let mut level = 0u32;
    loop {
        let size = column * (1u32 << level) as f32;
        let at = camera.map(|v| (v / size).floor() as i32);
        // From an even column to an even one, so the next level's columns hold it whole.
        let low = at.map(|v| (v - reach).div_euclid(2) * 2);
        let high = at.map(|v| (v + reach + 2).div_euclid(2) * 2);
        windows.push(Window { low, high });
        if size * reach as f32 >= far || level + 1 >= MOST_LEVELS {
            return windows;
        }
        level += 1;
    }
}

/// Keeps a world of blocks without end's chunks round the camera.
pub struct BlockStreamer {
    making: Option<Arc<BlockMaking>>,
    number: u64,
    latest: Arc<AtomicU64>,
    /// The coarse columns made, by level and place: their chunks' faces.
    coarse: HashMap<(u32, [i32; 2]), Vec<([i32; 3], Arc<ChunkMesh>)>>,
    /// The blocks of a smooth world's coarse columns, by level and place.
    coarse_blocks: HashMap<(u32, [i32; 2]), ColumnChunks>,
    /// The smooth coarse columns whose faces are being made.
    joining: HashSet<(u32, [i32; 2])>,
    /// The near columns whose blocks are made, by place.
    near: HashSet<[i32; 2]>,
    /// The near columns drawn: their faces made.
    drawn: HashSet<[i32; 2]>,
    /// The near columns whose faces are being made.
    meshing: HashSet<[i32; 2]>,
    /// The world's faces as the last update left them, to find faces made
    /// since, by changes to its blocks, for columns not drawn.
    seen: Option<Arc<HashMap<[i32; 3], Arc<ChunkMesh>>>>,
    /// Each level's window as the last update left them.
    windows: Option<Vec<Window>>,
    /// Whether faces made were let go, their blocks changed since, to be
    /// asked for again.
    again: bool,
    asked: HashSet<(u32, [i32; 2])>,
    waiting: usize,
    making_now: Arc<AtomicUsize>,
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
            coarse: HashMap::new(),
            coarse_blocks: HashMap::new(),
            joining: HashSet::new(),
            near: HashSet::new(),
            drawn: HashSet::new(),
            meshing: HashSet::new(),
            seen: None,
            windows: None,
            again: false,
            asked: HashSet::new(),
            waiting: 0,
            making_now: Arc::new(AtomicUsize::new(0)),
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
        let mut changed = false;
        if !self.making.as_ref().is_some_and(|kept| kept.same(&making)) {
            // Made anew: every column let go, the changes on those near kept.
            if !self.near.is_empty() {
                Arc::make_mut(pending).chunks.extend(BlockEdits::between(base, &world.blocks).chunks);
            }
            *base = Arc::new(BlockMap::default());
            *world = BlockWorld::made(BlockMap::default(), &making.faces, making.file.block_size, making.file.smooth);
            world.columns = Some(Arc::default());
            (self.near, self.drawn, self.meshing) = (HashSet::new(), HashSet::new(), HashSet::new());
            (self.coarse, self.asked) = (HashMap::new(), HashSet::new());
            (self.coarse_blocks, self.joining) = (HashMap::new(), HashSet::new());
            self.number = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
            self.making = Some(Arc::new(making));
            changed = true;
        } else if self.making.as_ref().is_some_and(|kept| kept.faces != making.faces) {
            // Its faces alone are others, as once a game numbers a block its file does not list: faces are made
            // with them from here on, and nothing made is made again. A column's faces being made now with those
            // before are right for its blocks as they were, and are made again if its blocks change.
            self.making = Some(Arc::new(making));
        }
        let making = self.making.clone().expect("kept above");
        let windows = windows(&making.file, distance, far, camera);
        // Near columns are drawn within the first window, their blocks made a column further.
        let (shown, made) = (windows[0], windows[0].grown());
        changed |= self.take_finished(world, (base, pending), (shown, made));
        changed |= self.let_go(world, (base, pending), (shown, made));
        self.coarse.retain(|(level, at), _| windows.get(*level as usize).is_some_and(|window| window.contains(*at)));
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
        // Asked again only where something came or went, or the camera moved, or there is still more to make.
        let moved = self.windows.as_deref() != Some(&windows[..]);
        if changed || moved || self.waiting > 0 || std::mem::take(&mut self.again) {
            self.ask(world, &making, &windows, camera);
        }
        if changed || moved {
            changed |= self.show_coarse(world, &windows);
        }
        self.windows = Some(windows);
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
        (shown, made): (Window, Window),
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
                Finished::Made { level, at, chunks, meshes, .. } => {
                    self.asked.remove(&(level, at));
                    if self.making.as_ref().is_some_and(|making| making.file.smooth) {
                        // Kept, for its faces and those of the columns round it, made once theirs are.
                        let chunks = chunks.into_iter().map(|(key, chunk)| (key, Arc::new(chunk))).collect();
                        self.coarse_blocks.insert((level, at), Arc::new(chunks));
                        self.again = true;
                        continue;
                    }
                    let meshes = meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                    self.coarse.insert((level, at), meshes);
                    changed = true;
                }
                Finished::Joined { level, at, meshes, .. } => {
                    self.joining.remove(&(level, at));
                    let meshes = meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                    self.coarse.insert((level, at), meshes);
                    changed = true;
                }
                Finished::Meshed { at, meshes, from, .. } => {
                    self.meshing.remove(&at);
                    // Blocks changed round it since are made into faces again, next.
                    let now = round_chunks(at, &columns, &world.blocks);
                    if !shown.contains(at) || self.drawn.contains(&at) || !same_chunks(&now, &from) {
                        self.again = true;
                        continue;
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
            world.columns = Some(Arc::new(self.near.clone()));
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
        (shown, made): (Window, Window),
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
        Arc::make_mut(pending).chunks.extend(BlockEdits::between(&was, &now).chunks);
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
        world.columns = Some(Arc::new(self.near.clone()));
        let undrawn = |key: &[i32; 3]| gone.contains(&[key[0], key[2]]) || hidden.contains(&[key[0], key[2]]);
        Arc::make_mut(&mut world.meshes).retain(|key, _| !undrawn(key));
        true
    }

    /// Asks the background threads for what is still to make, nearest
    /// first, as many as there is room for: the faces of near columns whose
    /// blocks and their neighbours' are made, the blocks of near columns,
    /// and coarse columns.
    fn ask(&mut self, world: &BlockWorld, making: &Arc<BlockMaking>, windows: &[Window], camera: [f32; 2]) {
        voxear_core::profile_scope!("Ask");
        let (shown, made) = (windows[0], windows[0].grown());
        let mut columns = None;
        let smooth = making.file.smooth;
        // What to make: its level (0 for a near column's blocks, and none for its faces), place and how far away.
        let mut wanted: Vec<(Option<u32>, [i32; 2], i64)> = Vec::new();
        // And the smooth coarse columns whose faces to make, their blocks and those round them made.
        let mut joins: Vec<(u32, [i32; 2], i64)> = Vec::new();
        for (level, window) in windows.iter().enumerate() {
            let level = level as u32;
            let size = CHUNK as f32 * making.file.block_size * (1u32 << level) as f32;
            let middle = camera.map(|v| v / size - 0.5);
            let away = |at: [i32; 2]| (at[0] as f32 - middle[0]).powi(2) + (at[1] as f32 - middle[1]).powi(2);
            if level == 0 {
                for at in shown.columns() {
                    let ready = round(at).all(|column| self.near.contains(&column));
                    if ready && !self.drawn.contains(&at) && !self.meshing.contains(&at) {
                        // Before blocks as far away, as it is what shows.
                        wanted.push((None, at, (away(at) * 16.0) as i64 - 2));
                    }
                }
            }
            if level > 0 && smooth {
                for at in window.columns() {
                    let key = (level, at);
                    let ready = round(at).all(|column| self.coarse_blocks.contains_key(&(level, column)));
                    if ready && !self.coarse.contains_key(&key) && !self.joining.contains(&key) {
                        joins.push((level, at, (away(at) * 16.0) as i64 + i64::from(level) * 4 - 2));
                    }
                }
            }
            let window = if level == 0 {
                made
            } else if smooth {
                window.grown()
            } else {
                *window
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
                if done || self.asked.contains(&(level, at)) {
                    continue;
                }
                wanted.push((Some(level), at, (away(at) * 16.0) as i64 + i64::from(level) * 4));
            }
        }
        wanted.sort_by_key(|(_, _, away)| *away);
        joins.sort_by_key(|(_, _, away)| *away);
        let mut room = self.at_once.max(1).saturating_sub(self.making_now.load(Ordering::Acquire));
        // Faces first: they are what shows, and are quick beside blocks.
        let joined = joins.len().min(room);
        self.waiting = wanted.len().saturating_sub(room - joined) + (joins.len() - joined);
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
        for (level, at, _) in wanted.into_iter().take(room) {
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
                        let meshes = keys
                            .into_iter()
                            .map(|key| (key, mesh_any(&from, key, &making.faces, making.file.smooth)))
                            .collect();
                        let _ = send.send(Finished::Meshed { number, at, meshes, from });
                    }
                    making_now.fetch_sub(1, Ordering::AcqRel);
                });
                continue;
            };
            self.asked.insert((level, at));
            crate::workers::spawn(move || {
                if latest.load(Ordering::Acquire) == number {
                    let chunks = column_chunks(&making.file, &making.generate, &making.world, level, at, None);
                    // A coarse column's faces made here, alone: the columns beside it not made.
                    let meshes = if level == 0 || making.file.smooth {
                        Vec::new()
                    } else {
                        let mut alone = BlockMap::default();
                        for (key, chunk) in &chunks {
                            alone.put_chunk(*key, chunk.clone());
                        }
                        chunks
                            .iter()
                            .map(|(key, _)| {
                                let mesh = mesh_made(&alone, *key, &making.faces, making.file.smooth, &|c| c == at);
                                (*key, mesh)
                            })
                            .collect()
                    };
                    let _ = send.send(Finished::Made { number, level, at, chunks, meshes });
                }
                making_now.fetch_sub(1, Ordering::AcqRel);
            });
        }
    }

    /// Draws each coarse column where the four finer columns under it are
    /// not all drawn. True if what is drawn changed.
    fn show_coarse(&self, world: &mut BlockWorld, windows: &[Window]) -> bool {
        voxear_core::profile_scope!("Coarse");
        let drawn_fine = |level: u32, at: [i32; 2]| {
            if level == 0 { self.drawn.contains(&at) } else { self.coarse.contains_key(&(level, at)) }
        };
        let mut far: HashMap<[i32; 4], Arc<ChunkMesh>> = HashMap::new();
        for ((level, at), meshes) in &self.coarse {
            let finer = windows[*level as usize - 1];
            let covered = (0..4).all(|i| {
                let child = [at[0] * 2 + (i & 1), at[1] * 2 + (i >> 1)];
                finer.contains(child) && drawn_fine(level - 1, child)
            });
            if covered {
                continue;
            }
            for (key, mesh) in meshes.iter().filter(|(_, mesh)| !mesh.is_empty()) {
                far.insert([*level as i32, key[0], key[1], key[2]], mesh.clone());
            }
        }
        let same_far = world.far.len() == far.len()
            && far.iter().all(|(key, mesh)| world.far.get(key).is_some_and(|kept| Arc::ptr_eq(kept, mesh)));
        if !same_far {
            world.far = Arc::new(far);
        }
        !same_far
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
    use super::*;
    use crate::{
        blocks::{
            chunk::{AIR, BlockId},
            file::{BlockKind, Ground},
            mesh::{FACINGS, unpack, water_shape},
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
        BlockMaking { file, generate, world: Arc::new(Preset::RollingHills.world()), faces: faces.into() }
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
        let mut most = 0;
        for _ in 0..60 {
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
    fn faces_alone_changed_are_taken_up_with_nothing_made_again() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        let mut streamer = BlockStreamer::default();
        settle(&mut streamer, &mut world, (&mut base, &mut pending), &making, [10.0, 10.0]);
        let (number, chunks) = (streamer.number, world.blocks.chunks().count());
        assert!(chunks > 0);
        // A block more than its file lists, numbered while the game runs: its faces at the end of the world's.
        let mut faces = making.faces.to_vec();
        faces.push(crate::blocks::BlockFaces::water(0));
        let grown = BlockMaking { faces: faces.into(), ..making.clone() };
        streamer.update(&mut world, &mut base, &mut pending, grown.clone(), (100.0, 600.0), [10.0, 10.0]);
        assert_eq!((streamer.number, world.blocks.chunks().count()), (number, chunks), "nothing started again");
        assert_eq!(streamer.making.as_ref().map(|kept| kept.faces.len()), Some(grown.faces.len()));
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
        // No coarse column draws where its four finer ones are made.
        for key in world.far.keys().filter(|k| k[0] == 1) {
            let child = [key[1] * 2, key[3] * 2];
            let all = (0..4).all(|i| streamer.drawn.contains(&[child[0] + (i & 1), child[1] + (i >> 1)]));
            assert!(!all, "coarse column {key:?} over drawn near ones");
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
            assert_eq!(streamer.drawn, window.columns().collect::<HashSet<_>>(), "smooth: {smooth}");
            assert_eq!(streamer.near, window.grown().columns().collect::<HashSet<_>>(), "smooth: {smooth}");
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
