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
}
