//! A chunk of blocks: `CHUNK` blocks a side, each a block's number kept as
//! a short index into the chunk's own palette of the blocks in it, as few
//! bits as its palette needs. A chunk of one block, air or stone, keeps no
//! indices at all.

/// How many blocks a chunk is a side.
pub const CHUNK: u32 = 32;

/// How many blocks a chunk holds.
pub const CHUNK_LEN: usize = (CHUNK * CHUNK * CHUNK) as usize;

/// A block's number in its world's list of blocks, counted from 1; 0 is air.
pub type BlockId = u16;

/// Air: no block.
pub const AIR: BlockId = 0;

/// One chunk's blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    palette: Vec<BlockId>,
    bits: u32,
    words: Box<[u64]>,
    /// How many of its blocks are not air.
    solid: u32,
}

impl Chunk {
    /// A chunk all of `block`.
    pub fn filled(block: BlockId) -> Self {
        Self {
            palette: vec![block],
            bits: 0,
            words: Box::new([]),
            solid: if block == AIR { 0 } else { CHUNK_LEN as u32 },
        }
    }

    /// Where block `(x, y, z)`, each from 0 to `CHUNK - 1`, is kept: along x,
    /// then z, then up.
    fn index(x: u32, y: u32, z: u32) -> usize {
        ((y * CHUNK + z) * CHUNK + x) as usize
    }

    /// The block at `(x, y, z)` of the chunk, each from 0 to `CHUNK - 1`.
    pub fn get(&self, x: u32, y: u32, z: u32) -> BlockId {
        if self.bits == 0 {
            return self.palette[0];
        }
        let index = Self::index(x, y, z);
        let per_word = (64 / self.bits) as usize;
        let word = self.words[index / per_word];
        let shift = (index % per_word) as u32 * self.bits;
        self.palette[((word >> shift) & ((1 << self.bits) - 1)) as usize]
    }

    /// Puts `block` at `(x, y, z)`, its palette and indices grown to hold it.
    pub fn set(&mut self, x: u32, y: u32, z: u32, block: BlockId) {
        let was = self.get(x, y, z);
        if was == block {
            return;
        }
        self.solid = self.solid + u32::from(block != AIR) - u32::from(was != AIR);
        let entry = match self.palette.iter().position(|&kept| kept == block) {
            Some(entry) => entry,
            None => {
                self.palette.push(block);
                let wanted = bits_for(self.palette.len());
                if wanted != self.bits {
                    self.repack(wanted);
                }
                self.palette.len() - 1
            }
        };
        if self.bits == 0 {
            return;
        }
        let index = Self::index(x, y, z);
        let per_word = (64 / self.bits) as usize;
        let shift = (index % per_word) as u32 * self.bits;
        let mask = ((1u64 << self.bits) - 1) << shift;
        let word = &mut self.words[index / per_word];
        *word = (*word & !mask) | ((entry as u64) << shift);
    }

    /// The same blocks kept in `bits` bits each.
    fn repack(&mut self, bits: u32) {
        let old = std::mem::replace(self, Self { palette: Vec::new(), bits: 0, words: Box::new([]), solid: 0 });
        let per_word = (64 / bits) as usize;
        let mut words = vec![0u64; CHUNK_LEN.div_ceil(per_word)];
        for (index, entry) in old.entries().enumerate() {
            words[index / per_word] |= (entry as u64) << ((index % per_word) as u32 * bits);
        }
        *self = Self { palette: old.palette, bits, words: words.into_boxed_slice(), solid: old.solid };
    }

    /// Each block's place in the palette, in the order blocks are kept.
    fn entries(&self) -> impl Iterator<Item = usize> + '_ {
        let per_word = if self.bits == 0 { 1 } else { (64 / self.bits) as usize };
        (0..CHUNK_LEN).map(move |index| {
            if self.bits == 0 {
                return 0;
            }
            let word = self.words[index / per_word];
            ((word >> ((index % per_word) as u32 * self.bits)) & ((1 << self.bits) - 1)) as usize
        })
    }

    /// Whether every block of it is air.
    pub fn is_air(&self) -> bool {
        self.solid == 0
    }

    /// How many bits each block takes.
    pub fn bits(&self) -> u32 {
        self.bits
    }

    /// Its palette, how many bits each block's place in it takes, and the
    /// words they are packed in: for writing it.
    pub fn parts(&self) -> (&[BlockId], u32, &[u64]) {
        (&self.palette, self.bits, &self.words)
    }

    /// A chunk from its parts, as [`Chunk::parts`] gives them; `None` where
    /// they do not make one.
    pub fn from_parts(palette: Vec<BlockId>, bits: u32, words: Vec<u64>) -> Option<Self> {
        if palette.is_empty() || bits != bits_for(palette.len()).max(bits) || ![0, 1, 2, 4, 8, 16].contains(&bits) {
            return None;
        }
        let wanted = if bits == 0 { 0 } else { CHUNK_LEN.div_ceil((64 / bits) as usize) };
        if words.len() != wanted {
            return None;
        }
        let mut chunk = Self { palette, bits, words: words.into_boxed_slice(), solid: 0 };
        let entries: Vec<usize> = chunk.entries().collect();
        if entries.iter().any(|&entry| entry >= chunk.palette.len()) {
            return None;
        }
        chunk.solid = entries.iter().filter(|&&entry| chunk.palette[entry] != AIR).count() as u32;
        Some(chunk)
    }

    /// The same blocks, each by the number `change` gives it.
    pub fn remapped(&self, change: impl Fn(BlockId) -> BlockId) -> Self {
        let mut chunk = self.clone();
        chunk.palette.iter_mut().for_each(|block| *block = change(*block));
        let entries: Vec<usize> = chunk.entries().collect();
        chunk.solid = entries.iter().filter(|&&entry| chunk.palette[entry] != AIR).count() as u32;
        chunk
    }
}

/// The fewest bits, 0, 1, 2, 4, 8 or 16, that number `entries` palette
/// entries.
fn bits_for(entries: usize) -> u32 {
    match entries {
        0..=1 => 0,
        2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        17..=256 => 8,
        _ => 16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_keeps_its_blocks_in_as_few_bits_as_its_palette_needs() {
        let mut chunk = Chunk::filled(AIR);
        assert!(chunk.is_air());
        assert_eq!((chunk.bits(), chunk.get(5, 6, 7)), (0, AIR));
        chunk.set(1, 2, 3, 7);
        assert_eq!((chunk.bits(), chunk.get(1, 2, 3), chunk.get(3, 2, 1)), (1, 7, AIR));
        assert!(!chunk.is_air());
        // Five kinds of block: four bits each, every block kept where it was put.
        for (i, block) in [9, 11, 12].into_iter().enumerate() {
            chunk.set(31, 31, i as u32, block);
        }
        assert_eq!(chunk.bits(), 4);
        assert_eq!([chunk.get(1, 2, 3), chunk.get(31, 31, 0), chunk.get(31, 31, 2), chunk.get(0, 0, 0)], [7, 9, 12, 0]);
        for y in 0..CHUNK {
            chunk.set(10, y, 10, 300 + y as u16);
        }
        assert_eq!(chunk.bits(), 8);
        assert_eq!((chunk.get(10, 20, 10), chunk.get(1, 2, 3)), (320, 7));
    }
}
