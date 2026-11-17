//! Maps and sets keyed by a few whole numbers, as chunks and columns are by
//! their places: hashed by [`CoordHasher`], many times quicker for such
//! keys than the standard library's hasher, which guards against keys
//! chosen to collide, as places in a world are not.

use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasherDefault, Hasher},
};

/// A map keyed by places.
pub type CoordMap<K, V> = HashMap<K, V, BuildHasherDefault<CoordHasher>>;

/// A set of places.
pub type CoordSet<K> = HashSet<K, BuildHasherDefault<CoordHasher>>;

/// Hashes a key a word at a time: each added to what came before and
/// multiplied by a large odd number, the whole turned at the end so its
/// high bits, which a table looks at first, are as well mixed as its low.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoordHasher(u64);

/// The multiplier: odd, with its bits well spread.
const MIX: u64 = 0xf135_7aea_2e62_a9c5;

impl CoordHasher {
    fn add(&mut self, word: u64) {
        self.0 = self.0.wrapping_add(word).wrapping_mul(MIX);
    }
}

impl Hasher for CoordHasher {
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn write_i32(&mut self, value: i32) {
        self.add(u64::from(value as u32));
    }

    fn write_i64(&mut self, value: i64) {
        self.add(value as u64);
    }
}

/// Work kept to be found again, by what it was worked out from: the newer
/// of it, and that from before the newer last filled. Once the newer hold
/// half of `most`, they become the older and the older are let go, so what
/// is still asked for stays kept as the rest goes: never all of it at once.
#[derive(Debug)]
pub struct Lately<K, V> {
    newer: CoordMap<K, V>,
    older: CoordMap<K, V>,
    most: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Lately<K, V> {
    /// Keeping `most` at most, two at least.
    pub fn new(most: usize) -> Self {
        Self { newer: CoordMap::default(), older: CoordMap::default(), most: most.max(2) }
    }

    /// What is kept for `key`, kept on with the newer.
    pub fn get(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.newer.get(key) {
            return Some(value.clone());
        }
        let value = self.older.remove(key)?;
        self.insert(key.clone(), value.clone());
        Some(value)
    }

    /// Keeps `value` for `key`, with the newer.
    pub fn insert(&mut self, key: K, value: V) {
        if self.newer.len() >= self.most / 2 && !self.newer.contains_key(&key) {
            self.older = std::mem::take(&mut self.newer);
        }
        self.older.remove(&key);
        self.newer.insert(key, value);
    }

    /// How many are kept.
    pub fn len(&self) -> usize {
        self.newer.len() + self.older.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_side_by_side_spread_over_a_tables_slots() {
        // Every column of a window 64 a side, as a table of 4,096 slots sees them: by the low bits and the high.
        let hash = |key: [i32; 2]| {
            let mut hasher = CoordHasher::default();
            std::hash::Hash::hash(&key, &mut hasher);
            hasher.finish()
        };
        let keys: Vec<[i32; 2]> = (-32..32).flat_map(|z| (-32..32).map(move |x| [x, z])).collect();
        let low: CoordSet<u64> = keys.iter().map(|&key| hash(key) & 4095).collect();
        let high: CoordSet<u64> = keys.iter().map(|&key| hash(key) >> 57).collect();
        // Spread at random, 4,096 keys fill about 2,600 of 4,096 slots.
        assert!(low.len() > 2000, "{} of 4096 slots", low.len());
        assert_eq!(high.len(), 128, "every one of the top seven bits' values");
        let set: CoordSet<[i32; 2]> = keys.iter().copied().collect();
        assert_eq!(set.len(), keys.len());
        assert!(set.contains(&[5, -7]) && !set.contains(&[32, 0]));
    }

    #[test]
    fn lately_keeps_what_is_asked_for_again_and_lets_the_rest_go_half_at_a_time() {
        let mut kept: Lately<u32, u32> = Lately::new(8);
        for key in 0..4 {
            kept.insert(key, key * 10);
        }
        // Full: the next goes with the newer, the first four become the older.
        kept.insert(4, 40);
        assert_eq!((kept.len(), kept.get(&0)), (5, Some(0)), "the older still found, and kept on");
        for key in 5..8 {
            kept.insert(key, key * 10);
        }
        // 0 was asked for again and moved with the newer; 1 to 3 were not, and go as the newer fill again.
        kept.insert(8, 80);
        assert_eq!(kept.get(&0), Some(0));
        assert_eq!((kept.get(&1), kept.get(&3)), (None, None));
        assert!(kept.len() <= 8);
        // Kept again under a key already there: one of it.
        kept.insert(8, 81);
        assert_eq!(kept.get(&8), Some(81));
    }
}
