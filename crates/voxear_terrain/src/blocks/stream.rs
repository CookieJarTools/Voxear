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
    mesh_any,
};
use crate::generate::WorldGen;

/// How many chunk columns are made at once in the background, at most.
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
}

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
            near: HashSet::new(),
            drawn: HashSet::new(),
            meshing: HashSet::new(),
            seen: None,
            windows: None,
            again: false,
            asked: HashSet::new(),
            waiting: 0,
            making_now: Arc::new(AtomicUsize::new(0)),
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
            (self.near, self.drawn, self.meshing) = (HashSet::new(), HashSet::new(), HashSet::new());
            (self.coarse, self.asked) = (HashMap::new(), HashSet::new());
            self.number = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
            self.making = Some(Arc::new(making));
            changed = true;
        }
        let making = self.making.clone().expect("kept above");
        let windows = windows(&making.file, distance, far, camera);
        // Near columns are drawn within the first window, their blocks made a column further.
        let (shown, made) = (windows[0], windows[0].grown());
        changed |= self.take_finished(world, (base, pending), (shown, made));
        changed |= self.let_go(world, (base, pending), (shown, made));
        self.coarse.retain(|(level, at), _| windows.get(*level as usize).is_some_and(|window| window.contains(*at)));
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
                Finished::Made { number, .. } | Finished::Meshed { number, .. } if number != self.number => {}
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
                Finished::Made { level, at, meshes, .. } => {
                    self.asked.remove(&(level, at));
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
        // The changes kept for columns come near made, before their faces are.
        if !came.is_empty() {
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
        // What to make: its level (0 for a near column's blocks, and none for its faces), place and how far away.
        let mut wanted: Vec<(Option<u32>, [i32; 2], i64)> = Vec::new();
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
            let window = if level == 0 { made } else { *window };
            for at in window.columns() {
                let done = if level == 0 { self.near.contains(&at) } else { self.coarse.contains_key(&(level, at)) };
                if done || self.asked.contains(&(level, at)) {
                    continue;
                }
                wanted.push((Some(level), at, (away(at) * 16.0) as i64 + i64::from(level) * 4));
            }
        }
        wanted.sort_by_key(|(_, _, away)| *away);
        let room = MOST_MAKING.saturating_sub(self.making_now.load(Ordering::Acquire));
        self.waiting = wanted.len().saturating_sub(room);
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
                    // A coarse column's faces made here, the columns beside it taken as air.
                    let meshes = if level == 0 {
                        Vec::new()
                    } else {
                        let mut alone = BlockMap::default();
                        for (key, chunk) in &chunks {
                            alone.put_chunk(*key, chunk.clone());
                        }
                        chunks
                            .iter()
                            .map(|(key, _)| (*key, mesh_any(&alone, *key, &making.faces, making.file.smooth)))
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
    use crate::{blocks::file::Ground, generate::Preset};

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
    fn near_columns_come_with_their_changes_and_go_keeping_them_and_coarse_ones_fill_beyond() {
        let making = making();
        let mut world = BlockWorld::new(BlockMap::default(), &making.faces, 2.0);
        let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
        // A change kept for a chunk not made yet: a block of stone high over column (0, 0).
        let mut changed = BlockMap::default();
        changed.set([5, 200, 5], 3);
        Arc::make_mut(&mut pending).chunks.extend(BlockEdits::between(&BlockMap::default(), &changed).chunks);
        let mut streamer = BlockStreamer::default();
        let settle = |streamer: &mut BlockStreamer,
                      world: &mut BlockWorld,
                      base: &mut Arc<BlockMap>,
                      pending: &mut Arc<BlockEdits>,
                      at: [f32; 2]| {
            for _ in 0..2000 {
                streamer.update(world, base, pending, making.clone(), (100.0, 600.0), at);
                if streamer.settled() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("never settled");
        };
        settle(&mut streamer, &mut world, &mut base, &mut pending, [10.0, 10.0]);
        assert_eq!(world.blocks.get([5, 200, 5]), 3, "the kept change made as its column came");
        assert!(pending.is_empty());
        assert!(!world.meshes.is_empty() && !world.far.is_empty(), "near chunks drawn, and coarse ones beyond");
        // Near columns' ground as their generator makes it.
        let surface = (-8..300).rev().find(|&y| world.blocks.get([20, y, 20]) != 0).unwrap();
        let height = making.world.height_at(41.0, 41.0);
        assert!(((surface + 1) as f32 * 2.0 - height).abs() <= 2.0);
        // Gone 3 km on: the old columns let go, their change kept, coarse ones of the new place round it.
        settle(&mut streamer, &mut world, &mut base, &mut pending, [3000.0, 10.0]);
        assert_eq!(world.blocks.get([5, 200, 5]), 0);
        assert_eq!(pending.chunks.len(), 1, "the change kept while its column is away");
        assert!(world.blocks.get([1500, 2, 5]) != 0 || world.blocks.chunks().count() > 0);
        // And back: made again with it.
        settle(&mut streamer, &mut world, &mut base, &mut pending, [10.0, 10.0]);
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
                for _ in 0..4000 {
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
                    let made = mesh_any(&world.blocks, *key, &making.faces, smooth);
                    assert_eq!(**mesh, made, "{key:?}, smooth: {smooth}");
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
