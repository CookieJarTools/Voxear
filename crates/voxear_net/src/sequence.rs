//! Sequence numbers that wrap round, and a ring of entries kept by them.

/// Whether 16-bit sequence `a` comes after `b`, allowing for wrapping.
pub fn newer(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

/// Entries for the last `N` sequence numbers: each slot keeps the sequence
/// it was last set for, so an old entry is never mistaken for a new one.
#[derive(Clone, Debug)]
pub struct SequenceBuffer<T> {
    entries: Vec<Option<(u16, T)>>,
}

impl<T> SequenceBuffer<T> {
    pub fn new(size: usize) -> Self {
        Self { entries: (0..size).map(|_| None).collect() }
    }

    fn slot(&self, sequence: u16) -> usize {
        sequence as usize % self.entries.len()
    }

    pub fn insert(&mut self, sequence: u16, value: T) {
        let slot = self.slot(sequence);
        self.entries[slot] = Some((sequence, value));
    }

    pub fn get(&self, sequence: u16) -> Option<&T> {
        match &self.entries[self.slot(sequence)] {
            Some((s, value)) if *s == sequence => Some(value),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, sequence: u16) -> Option<&mut T> {
        let slot = self.slot(sequence);
        match &mut self.entries[slot] {
            Some((s, value)) if *s == sequence => Some(value),
            _ => None,
        }
    }

    pub fn remove(&mut self, sequence: u16) -> Option<T> {
        let slot = self.slot(sequence);
        match self.entries[slot].take() {
            Some((s, value)) if s == sequence => Some(value),
            other => {
                self.entries[slot] = other;
                None
            }
        }
    }

    pub fn contains(&self, sequence: u16) -> bool {
        self.get(sequence).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_wraps_round() {
        assert!(newer(1, 0));
        assert!(newer(0, 65535));
        assert!(!newer(65535, 0));
        assert!(!newer(7, 7));
    }

    #[test]
    fn a_slot_reused_forgets_the_old_sequence() {
        let mut buffer = SequenceBuffer::new(4);
        buffer.insert(1, "one");
        buffer.insert(5, "five");
        assert_eq!(buffer.get(1), None);
        assert_eq!(buffer.get(5), Some(&"five"));
        assert_eq!(buffer.remove(1), None);
        assert_eq!(buffer.remove(5), Some("five"));
    }
}
