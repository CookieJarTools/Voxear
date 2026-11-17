//! A world's navigation kept up with its blocks, off the frame.
//!
//! Each frame, [`BlockNavBuilder::update`] only looks at whether the
//! world's blocks are the same blocks as before (the same `Arc`). When they
//! are not, the new blocks go to one of navigation's threads, which finds
//! the chunks that changed by comparing each chunk's `Arc` with the one it
//! last made places from (a chunk is shared until it changes, so this
//! costs a look per chunk, not per block), makes those chunks' places and
//! regions again, and the edges round them, and hands back a whole new
//! [`BlockNav`] sharing everything else with the last. While one build
//! runs, only the newest blocks wait for the next.
//!
//! A small change, as an edit is, is waited for the frame after it was
//! made, so a block placed is in the way of every search from then on. A
//! large one, as ground streaming in, is taken whenever it is done.

use std::sync::{
    Arc, Weak,
    atomic::{AtomicUsize, Ordering},
};

use voxear_terrain::{
    blocks::{BlockFaces, BlockMap, chunk::Chunk},
    coord_hash::{CoordMap, CoordSet},
};

use super::{
    BlockNav, NONE, NavChunk, Slot, around_index, around_step,
    places::ColumnBuilder,
    regions::{Edges, Regions, outer_edges},
    walking::{WalkTable, Walker},
};
use crate::threads::Job;

/// A build of no more chunks than this is waited for the frame after it
/// began.
const SMALL: usize = 24;
/// A world of no more chunks than this has its first places made at once.
const SMALL_WORLD: usize = 256;

/// What a world's navigation was last made from.
#[derive(Clone, Default)]
struct Sources {
    chunks: CoordMap<[i32; 3], Arc<Chunk>>,
    table: Arc<WalkTable>,
}

/// A build done.
struct Built {
    nav: Arc<BlockNav>,
    sources: Sources,
    spent: std::time::Duration,
}

/// Keeps a world of blocks' navigation up with its blocks.
pub struct BlockNavBuilder {
    nav: Arc<BlockNav>,
    sources: Option<Sources>,
    /// The build running, how many chunks it changes once that is known,
    /// and whether it looked small when it began.
    job: Option<(Arc<Job<Built>>, Arc<AtomicUsize>, bool)>,
    /// The blocks last handed over. Held weakly: the world's own copy is
    /// changed in place, never copied for this, and the old one's place is
    /// not given to new blocks while it is held, so a change is never
    /// missed for new blocks taking the old ones' place.
    seen: Option<(Weak<BlockMap>, Weak<[BlockFaces]>)>,
    /// The newest blocks, waiting for the build running to end.
    waiting: Option<(Arc<BlockMap>, Arc<[BlockFaces]>)>,
    /// Chunks made and the time it took, over the builds taken.
    pub made: usize,
    pub spent: std::time::Duration,
}

impl BlockNavBuilder {
    /// A builder keeping regions for `walkers`.
    pub fn new(walkers: &[Walker]) -> Self {
        Self {
            nav: Arc::new(BlockNav::new(walkers)),
            sources: Some(Sources::default()),
            job: None,
            seen: None,
            waiting: None,
            made: 0,
            spent: std::time::Duration::ZERO,
        }
    }

    /// The navigation as last made.
    pub fn nav(&self) -> &Arc<BlockNav> {
        &self.nav
    }

    /// Whether every change handed over has been made into the navigation.
    pub fn is_settled(&self) -> bool {
        self.job.is_none() && self.waiting.is_none()
    }

    /// Takes a build that is done (waiting for a small one), and hands the
    /// world's blocks over when they changed. True when the navigation
    /// changed.
    pub fn update(&mut self, blocks: &Arc<BlockMap>, faces: &Arc<[BlockFaces]>) -> bool {
        let mut changed = self.take(false);
        let same = self.seen.as_ref().is_some_and(|(seen_blocks, seen_faces)| {
            std::ptr::eq(seen_blocks.as_ptr(), Arc::as_ptr(blocks))
                && std::ptr::addr_eq(seen_faces.as_ptr(), Arc::as_ptr(faces))
        });
        if !same {
            self.seen = Some((Arc::downgrade(blocks), Arc::downgrade(faces)));
            self.waiting = Some((blocks.clone(), faces.clone()));
        }
        if self.job.is_none()
            && let Some((blocks, faces)) = self.waiting.take()
        {
            // A small world's first places are made at once, so it is walked from its first frame.
            let at_once = self.nav.slot_count() == 0 && blocks.len() <= SMALL_WORLD;
            let sources = self.sources.take().unwrap_or_default();
            // As many chunks as before, give or take a few: an edit, not ground streaming in or out.
            let looks_small = blocks.len().abs_diff(sources.chunks.len()) <= 4 && !sources.chunks.is_empty();
            let (nav, counted) = (self.nav.clone(), Arc::new(AtomicUsize::new(usize::MAX)));
            let count = counted.clone();
            let work = move || rebuild(&nav, sources, &blocks, &faces, &count);
            if at_once {
                self.job = Some((Job::done(work()), counted, true));
            } else {
                self.job = Some((Job::start(work), counted, looks_small));
            }
            changed |= self.take(false);
        }
        changed
    }

    /// Takes the build running if it is done, or, when `wait` or when it is
    /// small, once it is.
    fn take(&mut self, wait: bool) -> bool {
        let Some((job, counted, looks_small)) = &self.job else { return false };
        // How many chunks it changes is known once it has begun; before then, a guess.
        let small = match job.is_begun() {
            true => counted.load(Ordering::Acquire) <= SMALL,
            false => *looks_small,
        };
        let built = match wait || small {
            true => job.take(),
            false => job.try_take(),
        };
        let Some(built) = built else { return false };
        self.made += counted.load(Ordering::Acquire).min(usize::MAX / 2);
        self.spent += built.spent;
        self.nav = built.nav;
        self.sources = Some(built.sources);
        self.job = None;
        true
    }

    /// Waits until every change handed over is made into the navigation.
    pub fn settle(&mut self, blocks: &Arc<BlockMap>, faces: &Arc<[BlockFaces]>) {
        self.update(blocks, faces);
        while !self.is_settled() {
            self.take(true);
            self.update(blocks, faces);
        }
    }
}

/// `work` done on each of `items`, over several threads when there are many.
fn each<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    match items.len() < 8 {
        true => items.iter().map(work).collect(),
        false => voxear_terrain::workers::each(items, work),
    }
}

/// `nav` made again for `blocks`, from `sources`, what it was made from:
/// the chunks that changed made again, and what round them depends on
/// them. `counted` is told how many chunks change, once that is known.
fn rebuild(nav: &BlockNav, sources: Sources, blocks: &BlockMap, faces: &[BlockFaces], counted: &AtomicUsize) -> Built {
    voxear_core::profile_scope!("Navigation Build");
    let began = std::time::Instant::now();
    let table = WalkTable::of(faces);
    let whole = !sources.table.grown_into(&table);
    let mut sources = sources;
    // The chunks of blocks that changed: shared ones are the same pointers.
    let mut changed: CoordSet<[i32; 3]> = CoordSet::default();
    for (key, chunk) in blocks.chunks() {
        if whole || sources.chunks.get(key).is_none_or(|kept| !Arc::ptr_eq(kept, chunk)) {
            changed.insert(*key);
        }
    }
    for key in sources.chunks.keys() {
        if blocks.chunk(*key).is_none() {
            changed.insert(*key);
        }
    }
    if whole {
        changed.extend(nav.slots().map(|(_, slot)| slot.key));
    }
    // A chunk's places stand on the top of the chunk under it and count room into the one over it.
    let mut dirty: CoordSet<[i32; 3]> = CoordSet::default();
    for key in &changed {
        for dy in -1..=1 {
            dirty.insert([key[0], key[1] + dy, key[2]]);
        }
    }
    counted.store(dirty.len(), Ordering::Release);
    for key in &changed {
        match blocks.chunk(*key) {
            Some(chunk) => sources.chunks.insert(*key, chunk.clone()),
            None => sources.chunks.remove(key),
        };
    }
    sources.table = Arc::new(table);

    let mut nav = nav.clone();
    nav.version += 1;
    if dirty.is_empty() {
        return Built { nav: Arc::new(nav), sources, spent: began.elapsed() };
    }
    // Places, a column of chunks at a time, sharing the blocks read for one with the next up.
    let mut columns: CoordMap<[i32; 2], Vec<i32>> = CoordMap::default();
    for key in &dirty {
        columns.entry([key[0], key[2]]).or_default().push(key[1]);
    }
    let columns: Vec<([i32; 2], Vec<i32>)> = columns
        .into_iter()
        .map(|(column, mut ys)| {
            ys.sort_unstable();
            (column, ys)
        })
        .collect();
    let table = sources.table.clone();
    let made: Vec<Vec<([i32; 3], NavChunk)>> = each(&columns, |([x, z], ys)| {
        let mut builder = ColumnBuilder::new(blocks, &table);
        ys.iter().map(|&y| ([*x, y, *z], builder.build([*x, y, *z]))).collect()
    });

    // Into the world's chunks: new ones numbered, empty ones let go.
    let version = nav.version;
    let mut placed: Vec<u32> = Vec::new();
    let mut moved: CoordSet<[i32; 3]> = CoordSet::default();
    for (key, chunk) in made.into_iter().flatten() {
        let kept = nav.keys.get(&key).copied();
        match (kept, chunk.is_empty()) {
            (None, true) => {}
            (Some(slot), true) => {
                nav.slots[slot as usize] = None;
                nav.keys.remove(&key);
                nav.free.push(slot);
                moved.insert(key);
            }
            (Some(slot), false) => {
                let kept = nav.slots[slot as usize].as_mut().expect("numbered");
                if *kept.chunk != chunk {
                    kept.chunk = Arc::new(chunk);
                    kept.changed = version;
                    placed.push(slot);
                }
            }
            (None, false) => {
                let slot = nav.free.pop().unwrap_or_else(|| {
                    nav.slots.push(None);
                    nav.slots.len() as u32 - 1
                });
                nav.slots[slot as usize] = Some(Slot {
                    key,
                    chunk: Arc::new(chunk),
                    around: [NONE; 27],
                    regions: Arc::from([]),
                    outer: Arc::from([]),
                    changed: version,
                });
                nav.keys.insert(key, slot);
                placed.push(slot);
                moved.insert(key);
            }
        }
    }
    // The chunks round any that came or went know them, or know they are gone.
    let mut near_moved: CoordSet<[i32; 3]> = CoordSet::default();
    for key in &moved {
        for index in 0..27 {
            let d = around_step(index);
            near_moved.insert([key[0] + d[0], key[1] + d[1], key[2] + d[2]]);
        }
    }
    for key in &near_moved {
        let Some(slot) = nav.keys.get(key).copied() else { continue };
        let mut around = [NONE; 27];
        for (index, near) in around.iter_mut().enumerate() {
            let d = around_step(index);
            *near = nav.keys.get(&[key[0] + d[0], key[1] + d[1], key[2] + d[2]]).copied().unwrap_or(NONE);
        }
        nav.slots[slot as usize].as_mut().expect("numbered").around = around;
    }
    // Regions of every chunk whose places changed.
    let walkers: Vec<Walker> = nav.walkers.to_vec();
    let regions: Vec<Arc<[Regions]>> = each(&placed, |&slot| {
        let slot = nav.slots[slot as usize].as_ref().expect("numbered");
        walkers.iter().map(|walker| Regions::of_chunk(slot.key, &slot.chunk, *walker)).collect()
    });
    for (slot, regions) in placed.iter().zip(regions) {
        nav.slots[*slot as usize].as_mut().expect("numbered").regions = regions;
    }
    // Edges out of every chunk a step reaches one that changed, came or went from: along x or z, up or down.
    let mut edged: CoordSet<u32> = CoordSet::default();
    let touched = placed.iter().map(|slot| nav.slots[*slot as usize].as_ref().expect("numbered").key);
    for key in touched.chain(moved.iter().copied()).collect::<Vec<_>>() {
        for [dx, dz] in [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]] {
            for dy in -1..=1 {
                if let Some(slot) = nav.keys.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) {
                    edged.insert(*slot);
                }
            }
        }
    }
    let edged: Vec<u32> = edged.into_iter().collect();
    let outer: Vec<Arc<[Edges]>> =
        each(&edged, |&slot| (0..walkers.len()).map(|class| outer_edges(&nav, slot, class)).collect());
    for (slot, outer) in edged.iter().zip(outer) {
        nav.slots[*slot as usize].as_mut().expect("numbered").outer = outer;
    }
    debug_assert!(nav.slots().all(|(slot, kept)| kept.around[around_index([0, 0, 0])] == slot));
    Built { nav: Arc::new(nav), sources, spent: began.elapsed() }
}

#[cfg(test)]
mod tests;
