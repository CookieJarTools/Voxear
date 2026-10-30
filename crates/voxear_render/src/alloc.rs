//! Hands out ranges of a fixed-size buffer and takes them back, so meshes
//! can come and go without the shared vertex and index buffers filling up.

/// First-fit over a sorted free list; freed ranges merge with their
/// neighbours, so space does not fragment into slivers over a long session.
pub(crate) struct RangeAllocator {
    capacity: u64,
    /// Free ranges as (start, length), sorted by start and never touching.
    free: Vec<(u64, u64)>,
}

impl RangeAllocator {
    pub fn new(capacity: u64) -> Self {
        Self { capacity, free: vec![(0, capacity)] }
    }

    pub fn alloc(&mut self, length: u64) -> Option<u64> {
        if length == 0 {
            return Some(0);
        }
        let index = self.free.iter().position(|&(_, free)| free >= length)?;
        let (start, free) = self.free[index];
        if free == length {
            self.free.remove(index);
        } else {
            self.free[index] = (start + length, free - length);
        }
        Some(start)
    }

    pub fn free(&mut self, start: u64, length: u64) {
        if length == 0 {
            return;
        }
        let index = self.free.partition_point(|&(s, _)| s < start);
        self.free.insert(index, (start, length));
        // Merge with the next range, then with the previous one.
        if index + 1 < self.free.len() && start + length == self.free[index + 1].0 {
            self.free[index].1 += self.free.remove(index + 1).1;
        }
        if index > 0 && self.free[index - 1].0 + self.free[index - 1].1 == start {
            self.free[index - 1].1 += self.free.remove(index).1;
        }
    }

    pub fn used(&self) -> u64 {
        self.capacity - self.free.iter().map(|&(_, length)| length).sum::<u64>()
    }
}

/// Numbered slots, such as mesh table entries, handed out lowest-free first.
pub(crate) struct SlotAllocator {
    next: u32,
    limit: u32,
    free: Vec<u32>,
}

impl SlotAllocator {
    pub fn new(limit: u32) -> Self {
        Self { next: 0, limit, free: Vec::new() }
    }

    pub fn alloc(&mut self) -> Option<u32> {
        if let Some(slot) = self.free.pop() {
            return Some(slot);
        }
        (self.next < self.limit).then(|| {
            self.next += 1;
            self.next - 1
        })
    }

    pub fn free(&mut self, slot: u32) {
        self.free.push(slot);
    }

    pub fn used(&self) -> u32 {
        self.next - self.free.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freed_ranges_merge_and_are_reused() {
        let mut ranges = RangeAllocator::new(100);
        let a = ranges.alloc(30).unwrap();
        let b = ranges.alloc(30).unwrap();
        let c = ranges.alloc(30).unwrap();
        assert_eq!((a, b, c), (0, 30, 60));
        assert_eq!(ranges.alloc(20), None, "only 10 left");

        ranges.free(a, 30);
        ranges.free(c, 30);
        ranges.free(b, 30);
        assert_eq!(ranges.used(), 0);
        assert_eq!(ranges.alloc(100), Some(0), "the three frees merged into one range");
    }

    #[test]
    fn slots_are_reused_before_new_ones() {
        let mut slots = SlotAllocator::new(3);
        let [a, b, c] = [slots.alloc(), slots.alloc(), slots.alloc()].map(Option::unwrap);
        assert_eq!((a, b, c, slots.alloc()), (0, 1, 2, None));
        slots.free(b);
        assert_eq!((slots.alloc(), slots.used()), (Some(1), 3));
    }
}
