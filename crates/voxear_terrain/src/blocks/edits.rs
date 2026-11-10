//! Changes to a world's blocks from what they were made as, in as few bytes
//! as say them: for each chunk changed, the blocks in it that differ, one by
//! one while they are few, or the whole chunk once that is smaller. A
//! `.blocks` file beside a `.blockworld` keeps the editor's changes from its
//! ground; a save of the scene keeps the changes scripts made, and a game
//! sends them to a player who joins.
//!
//! In a world drawn smooth, a block changes with how full it is (see
//! `chunk::Fill`), and a whole chunk keeps how full each of its blocks is.
//!
//! As bytes: `VXBLOCKS`, its version, then the rest compressed: how many
//! chunks, and each chunk's place, whether it is its blocks one by one,
//! whole, or its blocks one by one with how full each is, and then either
//! how many and each block's place in the chunk and number (and fill), or
//! its palette, bits and words (see `chunk::Chunk::parts`), and whether its
//! fills follow, a byte a block. Version 1 had no fills.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK, CHUNK_LEN, Chunk, EMPTY, Fill},
};

const MAGIC: &[u8; 8] = b"VXBLOCKS";
const VERSION: u32 = 2;

/// How one chunk changed.
#[derive(Clone, Debug, PartialEq)]
pub enum ChunkChange {
    /// Its blocks that differ, each by its place in the chunk (along x, then
    /// z, then up) and what it is now.
    Blocks(Vec<(u16, BlockId)>),
    /// Its blocks that differ, as with `Blocks`, each with how full it is
    /// now: for a world drawn smooth.
    Cells(Vec<(u16, BlockId, Fill)>),
    /// All of it, as it is now.
    Whole(Chunk),
}

/// Every chunk changed, by where it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockEdits {
    pub chunks: BTreeMap<[i32; 3], ChunkChange>,
}

/// Block `index` of a chunk, as `(x, y, z)`.
fn place(index: usize) -> [u32; 3] {
    let index = index as u32;
    [index % CHUNK, index / (CHUNK * CHUNK), (index / CHUNK) % CHUNK]
}

impl BlockEdits {
    /// How `now` differs from `base`: chunks shared by both are passed over
    /// at once, the rest compared block by block.
    pub fn between(base: &BlockMap, now: &BlockMap) -> Self {
        let mut keys: Vec<[i32; 3]> = base.chunks().map(|(key, _)| *key).collect();
        keys.extend(now.chunks().map(|(key, _)| *key));
        keys.sort_unstable();
        keys.dedup();
        let mut chunks = BTreeMap::new();
        let air = Chunk::filled(AIR);
        for key in keys {
            let (was, is) = (base.chunk(key), now.chunk(key));
            if let (Some(was), Some(is)) = (was, is)
                && std::sync::Arc::ptr_eq(was, is)
            {
                continue;
            }
            let (was, is) = (was.map_or(&air, |c| &**c), is.map_or(&air, |c| &**c));
            let filled = was.has_fills() || is.has_fills();
            let mut changed = Vec::new();
            for index in 0..CHUNK_LEN {
                let [x, y, z] = place(index);
                let (block, fill) = (is.get(x, y, z), is.fill(x, y, z));
                if block != was.get(x, y, z) || (filled && fill != was.fill(x, y, z)) {
                    changed.push((index as u16, block, fill));
                }
            }
            if changed.is_empty() {
                continue;
            }
            // Four bytes a block one by one (five with its fill), or its palette and words whole: whichever is
            // smaller.
            let (palette, _, words) = is.parts();
            let whole = palette.len() * 2 + words.len() * 8 + if is.has_fills() { CHUNK_LEN } else { 0 };
            let each = if filled { 5 } else { 4 };
            let change = if changed.len() * each > whole {
                ChunkChange::Whole(is.clone())
            } else if filled {
                ChunkChange::Cells(changed)
            } else {
                ChunkChange::Blocks(changed.into_iter().map(|(index, block, _)| (index, block)).collect())
            };
            chunks.insert(key, change);
        }
        Self { chunks }
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// These changes with every block they make renumbered by `change`, as
    /// for a world whose kinds of block were taken out or moved.
    pub fn remapped(&self, change: impl Fn(BlockId) -> BlockId) -> Self {
        let renumber = |kept: &ChunkChange| match kept {
            ChunkChange::Blocks(changed) => {
                ChunkChange::Blocks(changed.iter().map(|&(index, block)| (index, change(block))).collect())
            }
            ChunkChange::Cells(changed) => ChunkChange::Cells(
                changed
                    .iter()
                    .map(|&(index, block, fill)| {
                        // Ground that became air is wholly air.
                        let block = change(block);
                        (index, block, if block == AIR && fill >= 128 { EMPTY } else { fill })
                    })
                    .collect(),
            ),
            ChunkChange::Whole(chunk) => ChunkChange::Whole(chunk.remapped(&change)),
        };
        Self { chunks: self.chunks.iter().map(|(key, kept)| (*key, renumber(kept))).collect() }
    }

    /// Makes these changes to `blocks`; the chunks changed, by where they are.
    pub fn apply(&self, blocks: &mut BlockMap) -> Vec<[i32; 3]> {
        for (key, change) in &self.chunks {
            match change {
                ChunkChange::Whole(chunk) => blocks.put_chunk(*key, chunk.clone()),
                ChunkChange::Blocks(changed) => {
                    let base = key.map(|v| v * CHUNK as i32);
                    for &(index, block) in changed {
                        let [x, y, z] = place(usize::from(index));
                        blocks.set([base[0] + x as i32, base[1] + y as i32, base[2] + z as i32], block);
                    }
                }
                ChunkChange::Cells(changed) => {
                    let base = key.map(|v| v * CHUNK as i32);
                    for &(index, block, fill) in changed {
                        let [x, y, z] = place(usize::from(index));
                        blocks.set_cell([base[0] + x as i32, base[1] + y as i32, base[2] + z as i32], block, fill);
                    }
                }
            }
        }
        self.chunks.keys().copied().collect()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend((self.chunks.len() as u32).to_le_bytes());
        for (key, change) in &self.chunks {
            key.iter().for_each(|v| body.extend(v.to_le_bytes()));
            match change {
                ChunkChange::Blocks(changed) => {
                    body.push(0);
                    body.extend((changed.len() as u32).to_le_bytes());
                    for &(index, block) in changed {
                        body.extend(index.to_le_bytes());
                        body.extend(block.to_le_bytes());
                    }
                }
                ChunkChange::Whole(chunk) => {
                    let (palette, bits, words) = chunk.parts();
                    body.push(1);
                    body.extend((palette.len() as u16).to_le_bytes());
                    palette.iter().for_each(|block| body.extend(block.to_le_bytes()));
                    body.push(bits as u8);
                    words.iter().for_each(|word| body.extend(word.to_le_bytes()));
                    match chunk.fills() {
                        Some(fills) => {
                            body.push(1);
                            body.extend(fills);
                        }
                        None => body.push(0),
                    }
                }
                ChunkChange::Cells(changed) => {
                    body.push(2);
                    body.extend((changed.len() as u32).to_le_bytes());
                    for &(index, block, fill) in changed {
                        body.extend(index.to_le_bytes());
                        body.extend(block.to_le_bytes());
                        body.push(fill);
                    }
                }
            }
        }
        let mut bytes = MAGIC.to_vec();
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(lz4_flex::compress_prepend_size(&body));
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 12 && &bytes[..8] == MAGIC, "not a file of block changes");
        let version = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
        ensure!((1..=VERSION).contains(&version), "block changes of version {version}, past {VERSION}");
        let body = lz4_flex::decompress_size_prepended(&bytes[12..]).context("its changes do not decompress")?;
        let mut at = 0usize;
        let mut take = |count: usize| -> Result<&[u8]> {
            let taken = body.get(at..at + count).context("the changes end early")?;
            at += count;
            Ok(taken)
        };
        let word = |b: &[u8]| u32::from_le_bytes(b.try_into().expect("four bytes"));
        let count = word(take(4)?);
        let mut chunks = BTreeMap::new();
        for _ in 0..count {
            let mut key = [0i32; 3];
            for v in &mut key {
                *v = i32::from_le_bytes(take(4)?.try_into().expect("four bytes"));
            }
            let change = match take(1)?[0] {
                0 => {
                    let changed = word(take(4)?) as usize;
                    ensure!(changed <= CHUNK_LEN, "more changes than a chunk has blocks");
                    let mut list = Vec::with_capacity(changed);
                    for _ in 0..changed {
                        let pair = take(4)?;
                        let index = u16::from_le_bytes([pair[0], pair[1]]);
                        ensure!(usize::from(index) < CHUNK_LEN, "a block past its chunk");
                        list.push((index, u16::from_le_bytes([pair[2], pair[3]])));
                    }
                    ChunkChange::Blocks(list)
                }
                1 => {
                    let entries = u16::from_le_bytes(take(2)?.try_into().expect("two bytes")) as usize;
                    let palette = (0..entries)
                        .map(|_| Ok(u16::from_le_bytes(take(2)?.try_into().expect("two bytes"))))
                        .collect::<Result<Vec<_>>>()?;
                    let bits = u32::from(take(1)?[0]);
                    let words = if bits == 0 { 0 } else { CHUNK_LEN.div_ceil((64 / bits.max(1)) as usize) };
                    let words = (0..words)
                        .map(|_| Ok(u64::from_le_bytes(take(8)?.try_into().expect("eight bytes"))))
                        .collect::<Result<Vec<_>>>()?;
                    let chunk = Chunk::from_parts(palette, bits, words).context("a chunk that is not one")?;
                    match version >= 2 && take(1)?[0] == 1 {
                        true => {
                            let fills = take(CHUNK_LEN)?.to_vec();
                            ChunkChange::Whole(chunk.with_fills(fills).context("fills that are not its blocks'")?)
                        }
                        false => ChunkChange::Whole(chunk),
                    }
                }
                2 => {
                    let changed = word(take(4)?) as usize;
                    ensure!(changed <= CHUNK_LEN, "more changes than a chunk has blocks");
                    let mut list = Vec::with_capacity(changed);
                    for _ in 0..changed {
                        let cell = take(5)?;
                        let index = u16::from_le_bytes([cell[0], cell[1]]);
                        ensure!(usize::from(index) < CHUNK_LEN, "a block past its chunk");
                        list.push((index, u16::from_le_bytes([cell[2], cell[3]]), cell[4]));
                    }
                    ChunkChange::Cells(list)
                }
                other => bail!("a chunk changed in an unknown way, {other}"),
            };
            chunks.insert(key, change);
        }
        Ok(Self { chunks })
    }
}

/// Where a `.blockworld`'s changes from its ground are kept: beside it,
/// named as it is.
pub fn blocks_file(world: &str) -> String {
    match world.strip_suffix(".blockworld") {
        Some(stem) => format!("{stem}.blocks"),
        None => format!("{world}.blocks"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_from_the_ground_are_kept_small_and_come_back_as_they_were() {
        let file = super::super::file::BlockFile::default();
        let base = file.make_ground(None);
        let mut now = base.clone();
        // A few blocks dug out: kept one by one.
        for x in 3..6 {
            now.set([x, 7, 10], AIR);
        }
        // A chunk filled with a pattern: kept whole.
        for y in 0..32 {
            for z in 0..32 {
                for x in 0..32 {
                    now.set([96 + x, y, z], 1 + ((x + y + z) % 3) as u16);
                }
            }
        }
        let edits = BlockEdits::between(&base, &now);
        assert_eq!(edits.chunks.len(), 2);
        assert!(matches!(edits.chunks[&[0, 0, 0]], ChunkChange::Blocks(ref list) if list.len() == 3));
        assert!(matches!(edits.chunks[&[3, 0, 0]], ChunkChange::Whole(_)));
        let bytes = edits.to_bytes();
        // Three blocks dug at a dozen bytes, and a chunk of three kinds in less than its two bits a block.
        assert!(bytes.len() < CHUNK_LEN / 4, "{} bytes", bytes.len());
        let few = BlockEdits { chunks: edits.chunks.iter().take(1).map(|(k, c)| (*k, c.clone())).collect() };
        assert!(few.to_bytes().len() < 64, "{} bytes for three blocks", few.to_bytes().len());
        let back = BlockEdits::from_bytes(&bytes).unwrap();
        assert_eq!(back, edits);
        let mut rebuilt = base.clone();
        back.apply(&mut rebuilt);
        assert_eq!(BlockEdits::between(&now, &rebuilt), BlockEdits::default(), "the same blocks again");
        assert!(BlockEdits::from_bytes(b"VXBLOCKS\x01\0\0\0nonsense").is_err());
        assert_eq!(blocks_file("Blocks/Field.blockworld"), "Blocks/Field.blocks");
    }

    #[test]
    fn smooth_changes_keep_how_full_each_block_is_and_older_changes_still_read() {
        use super::super::chunk::fill_of;
        // Smooth ground 4.3 blocks up over a chunk.
        let mut base = BlockMap::default();
        for z in 0..32 {
            for x in 0..32 {
                for y in 0..6 {
                    let depth = 4.3 - (y as f32 + 0.5);
                    base.set_cell([x, y, z], if depth > 0.0 { 1 } else { AIR }, fill_of(depth, depth > 0.0));
                }
            }
        }
        // A dent: a block a little less full, and the air over another a little fuller.
        let mut now = base.clone();
        now.set_cell([3, 4, 3], 1, 140);
        now.set_cell([8, 4, 8], AIR, 120);
        let edits = BlockEdits::between(&base, &now);
        assert!(matches!(edits.chunks[&[0, 0, 0]], ChunkChange::Cells(ref list) if list.len() == 2));
        let back = BlockEdits::from_bytes(&edits.to_bytes()).unwrap();
        assert_eq!(back, edits);
        let mut rebuilt = base.clone();
        back.apply(&mut rebuilt);
        assert_eq!((rebuilt.fill([3, 4, 3]), rebuilt.fill([8, 4, 8]), rebuilt.get([8, 4, 8])), (140, 120, AIR));
        // A whole chunk keeps its fills.
        let whole =
            BlockEdits { chunks: [([0, 0, 0], ChunkChange::Whole((**now.chunk([0, 0, 0]).unwrap()).clone()))].into() };
        assert_eq!(BlockEdits::from_bytes(&whole.to_bytes()).unwrap(), whole);
        // Version 1: a block dug out, as it was written then.
        let mut body = 1u32.to_le_bytes().to_vec();
        [0i32, 0, 0].iter().for_each(|v| body.extend(v.to_le_bytes()));
        body.push(0);
        body.extend(1u32.to_le_bytes());
        body.extend(7u16.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        let mut old = b"VXBLOCKS".to_vec();
        old.extend(1u32.to_le_bytes());
        old.extend(lz4_flex::compress_prepend_size(&body));
        assert!(
            matches!(BlockEdits::from_bytes(&old).unwrap().chunks[&[0, 0, 0]], ChunkChange::Blocks(ref list) if list == &[(7, AIR)])
        );
    }
}
