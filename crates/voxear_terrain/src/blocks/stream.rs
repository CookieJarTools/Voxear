//! Worlds of blocks without end: ground of blocks made round the camera as
//! it goes, from the world's generator (see `generate`), a chunk column at a
//! time on the terrain's background threads, nearest first.
//!
//! Near the camera, out to its view distance, the world's own chunks: its
//! blocks, built on, stood on and drawn, each column made again with the
//! changes kept for it as it comes back, and its changes kept as it goes.
//! Beyond, out to its far distance, rings of coarser chunks, each ring's
//! blocks twice as large as the one inside it, drawn alone: a ring's chunk
//! columns are left out where the four finer columns under each are made,
//! so near and far never both draw the same ground, and the coarse ground
//! shows until the fine is made.

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
    mesh::{ChunkMesh, mesh_chunk},
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
    pub faces: Arc<[[u8; 3]]>,
}

impl BlockMaking {
    fn same(&self, other: &BlockMaking) -> bool {
        self.file == other.file && Arc::ptr_eq(&self.world, &other.world)
    }
}

/// A chunk column made in the background: what it was made from, by its
/// number, its level and place, its chunks, and for a coarse one their
/// faces.
struct Finished {
    number: u64,
    level: u32,
    at: [i32; 2],
    chunks: Vec<([i32; 3], Chunk)>,
    meshes: Vec<([i32; 3], ChunkMesh)>,
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
    /// The near columns made, by place.
    near: HashSet<[i32; 2]>,
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
            asked: HashSet::new(),
            waiting: 0,
            making_now: Arc::new(AtomicUsize::new(0)),
            send,
            finished: Mutex::new(finished),
        }
    }
}

impl BlockStreamer {
    /// Whether every column of every window is made, and none is being made.
    pub fn settled(&self) -> bool {
        self.waiting == 0 && self.asked.is_empty() && self.making_now.load(Ordering::Acquire) == 0
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
            *world = BlockWorld::new(BlockMap::default(), &making.faces, making.file.block_size);
            (self.near, self.coarse, self.asked) = (HashSet::new(), HashMap::new(), HashSet::new());
            self.number = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
            self.making = Some(Arc::new(making));
            changed = true;
        }
        let making = self.making.clone().expect("kept above");
        let windows = windows(&making.file, distance, far, camera);
        // What the background threads made.
        let mut came: Vec<[i32; 2]> = Vec::new();
        let finished: Vec<Finished> = {
            let receive = self.finished.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            receive.try_iter().collect()
        };
        for done in finished {
            if done.number != self.number {
                continue;
            }
            self.asked.remove(&(done.level, done.at));
            if done.level == 0 {
                if !windows[0].contains(done.at) || self.near.contains(&done.at) {
                    continue;
                }
                let blocks = Arc::make_mut(&mut world.blocks);
                let made = Arc::make_mut(base);
                for (key, chunk) in done.chunks {
                    made.put_chunk(key, chunk.clone());
                    blocks.put_chunk(key, chunk);
                }
                self.near.insert(done.at);
                came.push(done.at);
            } else {
                let meshes = done.meshes.into_iter().map(|(key, mesh)| (key, Arc::new(mesh))).collect();
                self.coarse.insert((done.level, done.at), meshes);
                changed = true;
            }
        }
        // Columns come near: the changes kept for them made, and their faces and their neighbours' made again.
        if !came.is_empty() {
            let blocks = Arc::make_mut(&mut world.blocks);
            let kept = Arc::make_mut(pending);
            let mut theirs = BlockEdits::default();
            kept.chunks.retain(|key, change| {
                let mine = came.contains(&[key[0], key[2]]);
                if mine {
                    theirs.chunks.insert(*key, change.clone());
                }
                !mine
            });
            theirs.apply(blocks);
            let mut keys: Vec<[i32; 3]> = Vec::new();
            for at in &came {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let column = [at[0] + dx, at[1] + dz];
                        keys.extend(world.blocks.chunks().map(|(k, _)| *k).filter(|k| [k[0], k[2]] == column));
                    }
                }
            }
            keys.sort_unstable();
            keys.dedup();
            world.remesh(&keys);
            changed = true;
        }
        // Columns gone from the window: let go, their changes kept.
        let gone: Vec<[i32; 2]> = self.near.iter().copied().filter(|at| !windows[0].contains(*at)).collect();
        if !gone.is_empty() {
            let leaving = |key: &[i32; 3]| gone.contains(&[key[0], key[2]]);
            let mut was = BlockMap::default();
            let mut now = BlockMap::default();
            for (key, chunk) in base.chunks().filter(|(k, _)| leaving(k)) {
                was.put_chunk(*key, (**chunk).clone());
            }
            for (key, chunk) in world.blocks.chunks().filter(|(k, _)| leaving(k)) {
                now.put_chunk(*key, (**chunk).clone());
            }
            Arc::make_mut(pending).chunks.extend(BlockEdits::between(&was, &now).chunks);
            let keys: Vec<[i32; 3]> = world.blocks.chunks().map(|(k, _)| *k).filter(|k| leaving(k)).collect();
            let blocks = Arc::make_mut(&mut world.blocks);
            let made = Arc::make_mut(base);
            let meshes = Arc::make_mut(&mut world.meshes);
            for key in keys {
                blocks.put_chunk(key, Chunk::filled(super::chunk::AIR));
                made.put_chunk(key, Chunk::filled(super::chunk::AIR));
                meshes.remove(&key);
            }
            for at in gone {
                self.near.remove(&at);
            }
            changed = true;
        }
        self.coarse.retain(|(level, at), _| windows.get(*level as usize).is_some_and(|window| window.contains(*at)));
        // The rest to make, nearest first: near columns before coarse ones.
        let mut wanted: Vec<(u32, [i32; 2], i64)> = Vec::new();
        for (level, window) in windows.iter().enumerate() {
            let level = level as u32;
            let size = CHUNK as f32 * making.file.block_size * (1u32 << level) as f32;
            let middle = camera.map(|v| v / size - 0.5);
            for at in window.columns() {
                let made = if level == 0 { self.near.contains(&at) } else { self.coarse.contains_key(&(level, at)) };
                if made || self.asked.contains(&(level, at)) {
                    continue;
                }
                let away = (at[0] as f32 - middle[0]).powi(2) + (at[1] as f32 - middle[1]).powi(2);
                wanted.push((level, at, (away * 16.0) as i64 + i64::from(level) * 4));
            }
        }
        wanted.sort_by_key(|(_, _, away)| *away);
        let room = MOST_MAKING.saturating_sub(self.making_now.load(Ordering::Acquire));
        self.waiting = wanted.len().saturating_sub(room);
        for (level, at, _) in wanted.into_iter().take(room) {
            self.asked.insert((level, at));
            self.making_now.fetch_add(1, Ordering::AcqRel);
            let (making, latest, making_now, send) =
                (making.clone(), self.latest.clone(), self.making_now.clone(), self.send.clone());
            let number = self.number;
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
                        chunks.iter().map(|(key, _)| (*key, mesh_chunk(&alone, *key, &making.faces))).collect()
                    };
                    let _ = send.send(Finished { number, level, at, chunks, meshes });
                }
                making_now.fetch_sub(1, Ordering::AcqRel);
            });
        }
        // Coarse columns drawn where the four finer columns under each are not all made.
        let made_fine = |level: u32, at: [i32; 2]| {
            if level == 0 { self.near.contains(&at) } else { self.coarse.contains_key(&(level, at)) }
        };
        let mut far: HashMap<[i32; 4], Arc<ChunkMesh>> = HashMap::new();
        for ((level, at), meshes) in &self.coarse {
            let finer = windows[*level as usize - 1];
            let covered = (0..4).all(|i| {
                let child = [at[0] * 2 + (i & 1), at[1] * 2 + (i >> 1)];
                finer.contains(child) && made_fine(level - 1, child)
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
            changed = true;
        }
        changed
    }
}

/// The changes `pending` keeps for chunks not near now, with those `base`
/// and `blocks` show for the chunks near: all of a world without end's
/// changes, for saving.
pub fn all_changes(base: &BlockMap, blocks: &BlockMap, pending: &BlockEdits) -> BlockEdits {
    let mut all = pending.clone();
    all.chunks.extend(BlockEdits::between(base, blocks).chunks);
    all.chunks.retain(|_, change| !matches!(change, ChunkChange::Blocks(list) if list.is_empty()));
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
            let all = (0..4).all(|i| streamer.near.contains(&[child[0] + (i & 1), child[1] + (i >> 1)]));
            assert!(!all, "coarse column {key:?} over made near ones");
        }
    }
}
