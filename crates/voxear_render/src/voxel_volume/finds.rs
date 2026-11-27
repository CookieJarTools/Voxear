//! What a generator's nodes find over its ground on the CPU, laid into an
//! endless Voxel Volume's columns on the GPU: its spires and bridges. A
//! Spires or Arches node finds them a cell of its world at a time from the
//! ground's height round the cell (`voxear_terrain::generate::GpuFinds`),
//! which the GPU's program, run a column at a time, cannot ask; its steps
//! there read each cell's record from a table instead (`findAt` in
//! `shaders/generator.slang`), so the spires and bridges the GPU makes are
//! the CPU's to the bit.
//!
//! The records are worked out on threads of their own as the window comes
//! near them, the nearest first, and kept for every volume and ring that
//! asks; a column is made once the records of every cell that reaches it
//! are in its table. The CPU's own ground (`solid.rs`) runs the nodes
//! themselves and finds the same.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use voxear_terrain::generate::{GpuFinder, GpuFinds};

/// The most records kept before those not asked for lately are let go.
const MOST_KEPT: usize = 1 << 16;

/// The most records waiting to be worked out: those asked for longest ago go.
const MOST_ASKED: usize = 1 << 15;

/// The words of a table's head for each node, after the count.
const HEAD_WORDS: usize = 4;

/// Records worked out and kept, and those asked for, by node and cell.
#[derive(Default)]
struct Kept {
    records: HashMap<(u64, [i32; 2]), (Arc<Vec<u32>>, u64)>,
    asked: VecDeque<(Arc<dyn GpuFinds>, [i32; 2])>,
    working: HashSet<(u64, [i32; 2])>,
    /// Counts each ask, which says how lately a record was wanted.
    asks: u64,
    workers: bool,
}

/// Every volume's records, and the threads working them out.
struct Work {
    kept: Mutex<Kept>,
    wake: Condvar,
}

static WORK: std::sync::LazyLock<Arc<Work>> =
    std::sync::LazyLock::new(|| Arc::new(Work { kept: Mutex::new(Kept::default()), wake: Condvar::new() }));

fn kept() -> std::sync::MutexGuard<'static, Kept> {
    WORK.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The record of `cell` for `finds`, where it is worked out.
fn ready(finds: &dyn GpuFinds, cell: [i32; 2]) -> Option<Arc<Vec<u32>>> {
    let mut kept = kept();
    let asks = kept.asks;
    kept.records.get_mut(&(finds.id(), cell)).map(|(record, seen)| {
        *seen = asks;
        record.clone()
    })
}

/// Asks for `cells`' records of `finds`, the first first, ahead of what
/// was asked before: those not kept or being worked out are worked out on
/// the threads.
fn ask(finds: &Arc<dyn GpuFinds>, cells: &[[i32; 2]]) {
    let mut kept = kept();
    kept.asks += 1;
    let id = finds.id();
    let fresh: Vec<[i32; 2]> = cells
        .iter()
        .filter(|cell| !kept.records.contains_key(&(id, **cell)) && !kept.working.contains(&(id, **cell)))
        .copied()
        .collect();
    // The newest asks first, then what was asked before and is still waiting, as another ring of the same volume
    // asks for its own.
    let newest: HashSet<[i32; 2]> = fresh.iter().copied().collect();
    let before = std::mem::take(&mut kept.asked);
    let mut asked: VecDeque<(Arc<dyn GpuFinds>, [i32; 2])> =
        fresh.into_iter().map(|cell| (finds.clone(), cell)).collect();
    asked.extend(before.into_iter().filter(|(other, cell)| other.id() != id || !newest.contains(cell)));
    asked.truncate(MOST_ASKED);
    kept.asked = asked;
    if !kept.workers && !kept.asked.is_empty() {
        kept.workers = true;
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 4).clamp(2, 4);
        for n in 0..threads {
            let _ = std::thread::Builder::new().name(format!("Voxel finds {n}")).spawn(work);
        }
    }
    drop(kept);
    WORK.wake.notify_all();
}

/// A thread working out the records asked for, the first first.
fn work() {
    loop {
        let job = {
            let mut kept = kept();
            match kept.asked.pop_front() {
                Some((finds, cell)) => {
                    kept.working.insert((finds.id(), cell));
                    Some((finds, cell))
                }
                None => {
                    let _ = WORK.wake.wait_timeout(kept, std::time::Duration::from_millis(250));
                    None
                }
            }
        };
        let Some((finds, cell)) = job else { continue };
        let record = Arc::new(finds.record(cell));
        let mut kept = kept();
        let key = (finds.id(), cell);
        kept.working.remove(&key);
        let asks = kept.asks;
        kept.records.insert(key, (record, asks));
        if kept.records.len() > MOST_KEPT {
            // The half asked for longest ago go.
            let mut seen: Vec<u64> = kept.records.values().map(|(_, seen)| *seen).collect();
            seen.sort_unstable();
            let keep_from = seen[seen.len() / 2];
            kept.records.retain(|_, (_, seen)| *seen >= keep_from);
        }
    }
}

/// One node's part of a table: its cells' records in a square `side`
/// cells a side, wrapping, each `words` words with the two of its place.
struct Part {
    finds: Arc<dyn GpuFinds>,
    side: i32,
    words: usize,
    start: usize,
    /// The cell each slot holds, and the frame it was last wanted on.
    written: Vec<Option<[i32; 2]>>,
    seen: Vec<u64>,
}

impl Part {
    fn slot_of(&self, cell: [i32; 2]) -> usize {
        (cell[1].rem_euclid(self.side) * self.side + cell[0].rem_euclid(self.side)) as usize
    }

    /// The cells whose records reach the box from `low` to `high`, in
    /// metres in the generator's world: those it lies in and as many round
    /// as the node reads.
    fn cells(&self, low: [f32; 2], high: [f32; 2]) -> ([i32; 2], [i32; 2]) {
        let (cell, reach) = (self.finds.cell(), self.finds.reach());
        let at = |metres: f32| (metres / cell).floor() as i32;
        ([at(low[0]) - reach, at(low[1]) - reach], [at(high[0]) + reach, at(high[1]) + reach])
    }
}

/// The table of what an endless volume's generator's nodes find, for a
/// window of columns `metres` a side round the camera, the column the
/// volume's corner is at lying at `at` in the generator's world.
pub(crate) struct LaidFinds {
    pub table: voxear_rhi::Buffer,
    parts: Vec<Part>,
    at: [f32; 2],
    metres: f32,
    /// How many columns round a column its making reads the generator at.
    ring: i32,
    frame: u64,
}

/// The words of a table of `parts` with no cell in it, each part's records
/// starting where its head says.
fn empty_words(parts: &mut [Part]) -> Vec<u32> {
    let mut words = vec![parts.len() as u32];
    let mut start = 1 + parts.len() * HEAD_WORDS;
    for part in parts.iter_mut() {
        part.start = start;
        words.extend([part.side as u32, part.words as u32, start as u32, 0]);
        start += (part.side * part.side) as usize * part.words;
    }
    words.resize(start, 0);
    for part in parts.iter() {
        for slot in 0..(part.side * part.side) as usize {
            let at = part.start + slot * part.words;
            words[at] = i32::MIN as u32;
            words[at + 1] = i32::MIN as u32;
        }
    }
    words
}

impl LaidFinds {
    /// The table for `finds`, a window `window` columns a side, each
    /// `metres` metres, the volume's corner at `at` in the generator's
    /// world; making a column reads the generator as far as `ring`
    /// columns round it.
    pub fn new(
        gpu: &voxear_rhi::Gpu,
        finds: &[GpuFinder],
        window: u32,
        metres: f32,
        at: [f32; 2],
        ring: u32,
    ) -> anyhow::Result<Self> {
        let ring = ring as i32;
        let across = (window as i32 + 2 * ring + 2) as f32 * metres;
        let mut parts: Vec<Part> = finds
            .iter()
            .map(|GpuFinder(finds)| {
                // The cells a window's width spans, and those its edges reach from, with one to spare as it moves.
                let side = (across / finds.cell()).ceil() as i32 + 2 * finds.reach() + 2;
                let slots = (side * side) as usize;
                Part {
                    finds: finds.clone(),
                    side,
                    words: finds.words() + 2,
                    start: 0,
                    written: vec![None; slots],
                    seen: vec![0; slots],
                }
            })
            .collect();
        let words = empty_words(&mut parts);
        let usage =
            voxear_rhi::vk::BufferUsageFlags::STORAGE_BUFFER | voxear_rhi::vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let table = gpu.create_buffer(
            "voxel world finds",
            words.len() as u64 * 4,
            usage,
            voxear_rhi::MemoryLocation::CpuToGpu,
        )?;
        table.write(0, &words);
        Ok(Self { table, parts, at, metres, ring, frame: 0 })
    }

    /// The box of the generator's world the columns from `least` to `most`
    /// read, with the columns round them their making reads.
    fn reach_of(&self, least: [i32; 2], most: [i32; 2]) -> ([f32; 2], [f32; 2]) {
        let place = |column: i32, axis: usize| self.at[axis] + column as f32 * self.metres;
        let low = [place(least[0] - self.ring, 0), place(least[1] - self.ring, 1)];
        let high = [place(most[0] + 1 + self.ring, 0), place(most[1] + 1 + self.ring, 1)];
        (low, high)
    }

    /// Follows the volume's window, from column `least`, `window` a side
    /// round the camera's column `middle`: asks for the records of the
    /// cells reaching it not worked out yet, the nearest first, and writes
    /// those worked out into the table, in slots no frame in flight may
    /// still read for the cell they held.
    pub fn follow(&mut self, least: [i32; 2], window: u32, middle: [i32; 2]) {
        self.frame += 1;
        let most = [least[0] + window as i32 - 1, least[1] + window as i32 - 1];
        let (low, high) = self.reach_of(least, most);
        let middle_at = self.reach_of(middle, middle);
        let frame = self.frame;
        for part in &mut self.parts {
            let (first, last) = part.cells(low, high);
            let centre = part.cells(middle_at.0, middle_at.1);
            let centre = [(centre.0[0] + centre.1[0]) / 2, (centre.0[1] + centre.1[1]) / 2];
            let mut missing = Vec::new();
            for z in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    let cell = [x, z];
                    let slot = part.slot_of(cell);
                    if part.written[slot] == Some(cell) {
                        part.seen[slot] = frame;
                        continue;
                    }
                    let free = part.written[slot].is_none()
                        || frame > part.seen[slot] + voxear_rhi::FRAMES_IN_FLIGHT as u64 + 1;
                    match ready(part.finds.as_ref(), cell) {
                        Some(record) if free => {
                            let mut words = Vec::with_capacity(part.words);
                            words.extend([cell[0] as u32, cell[1] as u32]);
                            words.extend(record.iter().copied());
                            words.resize(part.words, 0);
                            self.table.write(((part.start + slot * part.words) * 4) as u64, &words);
                            part.written[slot] = Some(cell);
                            part.seen[slot] = frame;
                        }
                        Some(_) => {}
                        None => missing.push(cell),
                    }
                }
            }
            if !missing.is_empty() {
                let far = |c: &[i32; 2]| (c[0] - centre[0]).pow(2) + (c[1] - centre[1]).pow(2);
                missing.sort_unstable_by_key(far);
                ask(&part.finds, &missing);
            }
        }
    }

    /// Whether column `column` can be made: the records of every cell
    /// reaching it, and the columns round it its making reads, are in the
    /// table.
    pub fn ready_for(&self, column: [i32; 2]) -> bool {
        let (low, high) = self.reach_of(column, column);
        self.parts.iter().all(|part| {
            let (first, last) = part.cells(low, high);
            (first[1]..=last[1])
                .all(|z| (first[0]..=last[0]).all(|x| part.written[part.slot_of([x, z])] == Some([x, z])))
        })
    }
}

/// A table of `finds` holding every cell that reaches the box from `low` to
/// `high` in the generator's world, worked out now: for asking the GPU
/// about places alone.
pub(crate) fn table_over(finds: &[GpuFinder], low: [f32; 2], high: [f32; 2]) -> Vec<u32> {
    let mut parts: Vec<Part> = finds
        .iter()
        .map(|GpuFinder(finds)| {
            let probe =
                Part { finds: finds.clone(), side: 1, words: 0, start: 0, written: Vec::new(), seen: Vec::new() };
            let (first, last) = probe.cells(low, high);
            let side = (last[0] - first[0]).max(last[1] - first[1]) + 1;
            Part { side, words: finds.words() + 2, ..probe }
        })
        .collect();
    let mut words = empty_words(&mut parts);
    for part in &parts {
        let (first, last) = part.cells(low, high);
        for z in first[1]..=last[1] {
            for x in first[0]..=last[0] {
                let at = part.start + part.slot_of([x, z]) * part.words;
                words[at] = x as u32;
                words[at + 1] = z as u32;
                let record = part.finds.record([x, z]);
                words[at + 2..at + 2 + record.len()].copy_from_slice(&record);
            }
        }
    }
    words
}
