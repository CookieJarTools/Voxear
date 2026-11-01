//! Memory as the memory profiler sees it: how much each part of the engine
//! holds, gathered into a [`Snapshot`] of named amounts, and what changed
//! between two snapshots ([`diff`]), as before and after a level change to
//! find what was not let go.
//!
//! Rust's own heap is counted by [`CountingAllocator`], which a program
//! installs as its global allocator: it adds two atomic sums to each
//! allocation and nothing else. Without it, [`heap`] reads zero.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// The system's allocator, counting what is live.
pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            counted(layout.size() as u64, true);
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            counted(layout.size() as u64, true);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) };
        counted(layout.size() as u64, false);
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(block, layout, new_size) };
        if !moved.is_null() {
            LIVE_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            LIVE_BYTES.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        }
        moved
    }
}

fn counted(bytes: u64, allocated: bool) {
    if allocated {
        LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed);
        LIVE_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    } else {
        LIVE_BYTES.fetch_sub(bytes, Ordering::Relaxed);
        LIVE_ALLOCATIONS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Rust's heap as [`CountingAllocator`] counts it: bytes and allocations
/// live now. Zero without it installed.
pub fn heap() -> (u64, u64) {
    (LIVE_BYTES.load(Ordering::Relaxed), LIVE_ALLOCATIONS.load(Ordering::Relaxed))
}

/// The process's resident memory, as the system counts it, where it says.
pub fn resident() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

/// One amount in a snapshot: what part of the engine holds it, what it is,
/// and how many bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub category: String,
    pub name: String,
    pub bytes: u64,
}

/// Memory at one moment: a label to know it by, and what each part held.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub label: String,
    pub entries: Vec<Entry>,
}

impl Snapshot {
    pub fn new(label: &str) -> Self {
        Self { label: label.to_string(), entries: Vec::new() }
    }

    /// Adds an amount, to one of the same name and category if there is one.
    pub fn add(&mut self, category: &str, name: &str, bytes: u64) {
        match self.entries.iter_mut().find(|e| e.category == category && e.name == name) {
            Some(entry) => entry.bytes += bytes,
            None => self.entries.push(Entry { category: category.into(), name: name.into(), bytes }),
        }
    }

    /// Each category's total, in the order the categories first appear.
    pub fn totals(&self) -> Vec<(String, u64)> {
        let mut totals: Vec<(String, u64)> = Vec::new();
        for entry in &self.entries {
            match totals.iter_mut().find(|(category, _)| *category == entry.category) {
                Some((_, bytes)) => *bytes += entry.bytes,
                None => totals.push((entry.category.clone(), entry.bytes)),
            }
        }
        totals
    }
}

/// An amount in two snapshots: what it is, and its bytes in each, zero where
/// it was not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub category: String,
    pub name: String,
    pub before: u64,
    pub after: u64,
}

impl Change {
    pub fn delta(&self) -> i64 {
        self.after as i64 - self.before as i64
    }
}

/// What changed from `before` to `after`: every amount that differs, the
/// largest change first.
pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    for entry in &after.entries {
        let was = before.entries.iter().find(|e| e.category == entry.category && e.name == entry.name);
        let before = was.map_or(0, |e| e.bytes);
        changes.push(Change { category: entry.category.clone(), name: entry.name.clone(), before, after: entry.bytes });
    }
    for entry in &before.entries {
        if !after.entries.iter().any(|e| e.category == entry.category && e.name == entry.name) {
            let gone =
                Change { category: entry.category.clone(), name: entry.name.clone(), before: entry.bytes, after: 0 };
            changes.push(gone);
        }
    }
    changes.retain(|change| change.delta() != 0);
    changes.sort_by_key(|change| std::cmp::Reverse(change.delta().unsigned_abs()));
    changes
}

/// Bytes as people read them: 512 B, 1.5 KB, 2.3 MB, 1.1 GB.
pub fn describe(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        1_048_576..1_073_741_824 => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
        _ => format!("{:.2} GB", bytes as f64 / 1_073_741_824.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_add_up_and_their_differences_come_largest_first() {
        let mut before = Snapshot::new("Day");
        before.add("GPU", "hdr", 8_000_000);
        before.add("GPU", "Rock", 1_000_000);
        before.add("ECS chunks", "Transform", 16_384);
        let mut after = Snapshot::new("Night");
        after.add("GPU", "hdr", 8_000_000);
        after.add("GPU", "Moon", 4_000_000);
        after.add("ECS chunks", "Transform", 16_384);
        after.add("ECS chunks", "Transform", 16_384);
        assert_eq!(after.totals(), [("GPU".to_string(), 12_000_000), ("ECS chunks".to_string(), 32_768)]);

        let changes = diff(&before, &after);
        let shown: Vec<(&str, i64)> = changes.iter().map(|c| (c.name.as_str(), c.delta())).collect();
        assert_eq!(
            shown,
            [("Moon", 4_000_000), ("Rock", -1_000_000), ("Transform", 16_384)],
            "what stayed is left out"
        );
        assert_eq!(describe(1536), "1.5 KB");
        assert_eq!(describe(3 * 1_048_576), "3.0 MB");
    }
}
