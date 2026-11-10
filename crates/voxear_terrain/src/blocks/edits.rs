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
//! When a world's blocks change size, what was built stays where it was, in
//! metres: each block of the new size whose middle lies in a changed block
//! takes that block ([`resized`]).
//!
//! As bytes: `VXBLOCKS`, its version, then the rest compressed: how many
//! chunks, and each chunk's place, whether it is its blocks one by one,
//! whole, or its blocks one by one with how full each is, and then either
//! how many and each block's place in the chunk and number (and fill), or
//! its palette, bits and words (see `chunk::Chunk::parts`), and whether its
//! fills follow, a byte a block. Version 1 had no fills.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK, CHUNK_LEN, Chunk, EMPTY, FULL, Fill, depth_of, fill_of},
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

/// One block changed: where it is in blocks from its world's corner, what
/// it is now and how full.
pub type Cell = ([i32; 3], BlockId, Fill);

/// The blocks of `now` that differ from `base`, each as it is now: chunks
/// shared by both are passed over at once.
pub fn changed(base: &BlockMap, now: &BlockMap) -> Vec<Cell> {
    let keys: BTreeSet<[i32; 3]> = base.chunks().chain(now.chunks()).map(|(key, _)| *key).collect();
    let air = Chunk::filled(AIR);
    let mut cells = Vec::new();
    for key in keys {
        let (was, is) = (base.chunk(key), now.chunk(key));
        if let (Some(was), Some(is)) = (was, is)
            && std::sync::Arc::ptr_eq(was, is)
        {
            continue;
        }
        let (was, is) = (was.map_or(&air, |c| &**c), is.map_or(&air, |c| &**c));
        let corner = key.map(|v| v * CHUNK as i32);
        for index in 0..CHUNK_LEN {
            let [x, y, z] = place(index);
            let (block, fill) = (is.get(x, y, z), is.fill(x, y, z));
            if block != was.get(x, y, z) || fill != was.fill(x, y, z) {
                cells.push(([corner[0] + x as i32, corner[1] + y as i32, corner[2] + z as i32], block, fill));
            }
        }
    }
    cells
}

/// How a world's blocks change size: from blocks `from` metres a side to
/// blocks `to` metres a side, drawn smooth or not, `water` saying which
/// kinds are water (whose fill is how it flows, not how full it is).
pub struct Resize<'a> {
    pub from: f32,
    pub to: f32,
    pub smooth: bool,
    pub water: &'a dyn Fn(BlockId) -> bool,
}

/// What `cells`, changed blocks of a world, become once its blocks change
/// size as `resize` says, each staying where it was in metres: every block
/// of the new size whose middle lies in one of them takes its kind. Drawn
/// smooth and given the world they were changed in (`old`), each new block
/// near them is as full as that world's blocks say round its middle, read
/// between their middles, so the ground's surface stays where it was;
/// without it, as full as the block it lies in, in its own blocks.
pub fn resized(cells: &[Cell], old: Option<&BlockMap>, resize: &Resize) -> Vec<Cell> {
    let ratio = resize.from / resize.to;
    // The blocks of the new size along an axis whose middles lie in old block `c`.
    let span = |c: i32| ((c as f32 * ratio - 0.5).ceil() as i32)..(((c + 1) as f32 * ratio - 0.5).ceil() as i32);
    let within =
        |c: [i32; 3]| span(c[1]).flat_map(move |y| span(c[2]).flat_map(move |z| span(c[0]).map(move |x| [x, y, z])));
    let mut made = Vec::new();
    let Some(old) = old.filter(|_| resize.smooth) else {
        for &(at, block, fill) in cells {
            // How full ground is counts in blocks, so in blocks of the new size; water's fill is how it flows.
            let as_it_was = !resize.smooth || (resize.water)(block);
            let fill = if as_it_was { fill } else { fill_of(depth_of(fill) * ratio, block != AIR) };
            made.extend(within(at).map(|new| (new, block, fill)));
        }
        return made;
    };
    // Drawn smooth: every block of the new size in a changed block or one beside it, read from the old world.
    let mut near: BTreeSet<[i32; 3]> = BTreeSet::new();
    for &([x, y, z], ..) in cells {
        for dy in -1..=1 {
            for dz in -1..=1 {
                near.extend((-1..=1).map(|dx| [x + dx, y + dy, z + dz]));
            }
        }
    }
    // How deep in the ground an old block's middle is: water is out of it.
    let depth = |at: [i32; 3]| if (resize.water)(old.get(at)) { -1.0 } else { depth_of(old.fill(at)) };
    for cell in near {
        for new in within(cell) {
            let block = old.get(cell);
            if (resize.water)(block) {
                made.push((new, block, old.fill(cell)));
                continue;
            }
            // Where its middle is among the old blocks' middles, and how deep the ground is there.
            let at = new.map(|v| (v as f32 + 0.5) / ratio - 0.5);
            let low = at.map(|v| v.floor() as i32);
            let part = [0, 1, 2].map(|i| at[i] - low[i] as f32);
            let mut deep = 0.0;
            let mut ground: Option<(f32, BlockId)> = None;
            for corner in 0..8 {
                let step = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
                let from = [0, 1, 2].map(|i| low[i] + step[i]);
                let weight: f32 = (0..3).map(|i| if step[i] == 1 { part[i] } else { 1.0 - part[i] }).product();
                let here = depth(from);
                deep += weight * here;
                let kind = old.get(from);
                if kind != AIR && !(resize.water)(kind) && ground.is_none_or(|(d, _)| here > d) {
                    ground = Some((here, kind));
                }
            }
            let deep = (deep * ratio).clamp(-1.0, 1.0);
            let kind = if block != AIR { Some(block) } else { ground.map(|(_, kind)| kind) };
            made.push(match kind.filter(|_| deep > 0.0) {
                Some(kind) => (new, kind, fill_of(deep, true)),
                None => (new, AIR, fill_of(deep.min(-0.01), false)),
            });
        }
    }
    made
}

impl BlockEdits {
    /// Every block these changes set, each as it sets it: a whole chunk's,
    /// all of them.
    pub fn cells(&self) -> Vec<Cell> {
        let mut cells = Vec::new();
        for (key, change) in &self.chunks {
            let corner = key.map(|v| v * CHUNK as i32);
            let at = |index: usize| {
                let [x, y, z] = place(index);
                [corner[0] + x as i32, corner[1] + y as i32, corner[2] + z as i32]
            };
            match change {
                ChunkChange::Blocks(changed) => cells.extend(
                    changed
                        .iter()
                        .map(|&(i, block)| (at(usize::from(i)), block, if block == AIR { EMPTY } else { FULL })),
                ),
                ChunkChange::Cells(changed) => {
                    cells.extend(changed.iter().map(|&(i, block, fill)| (at(usize::from(i)), block, fill)))
                }
                ChunkChange::Whole(chunk) => cells.extend((0..CHUNK_LEN).map(|i| {
                    let [x, y, z] = place(i);
                    (at(i), chunk.get(x, y, z), chunk.fill(x, y, z))
                })),
            }
        }
        cells
    }

    /// Changes that set each of `cells`, block by block.
    pub fn of_cells(cells: impl IntoIterator<Item = Cell>) -> Self {
        let mut chunks: BTreeMap<[i32; 3], BTreeMap<u16, (BlockId, Fill)>> = BTreeMap::new();
        for (at, block, fill) in cells {
            let (key, [x, y, z]) = BlockMap::chunk_of(at);
            chunks.entry(key).or_default().insert(((y * CHUNK + z) * CHUNK + x) as u16, (block, fill));
        }
        let listed = |cells: BTreeMap<u16, (BlockId, Fill)>| {
            ChunkChange::Cells(cells.into_iter().map(|(index, (block, fill))| (index, block, fill)).collect())
        };
        Self { chunks: chunks.into_iter().map(|(key, cells)| (key, listed(cells))).collect() }
    }

    /// These changes for a world whose blocks change size as `resize` says,
    /// each staying where it was in metres (see [`resized`]).
    pub fn resized(&self, resize: &Resize) -> Self {
        Self::of_cells(resized(&self.cells(), None, resize))
    }

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
    fn what_was_built_stays_where_it_was_when_blocks_change_size() {
        use super::super::file::{BlockFile, Ground};
        // Flat ground 16 m a side and 4 m high in blocks a metre a side, a pillar of stone on it and a hole dug in it.
        let mut file =
            BlockFile { ground: Ground { size: [16, 16], height: 4, ..Ground::default() }, ..BlockFile::default() };
        let base = file.make_ground(None);
        let mut built = base.clone();
        for y in 4..7 {
            built.set([5, y, 9], 3);
        }
        built.set([10, 3, 2], AIR);
        let cells = changed(&base, &built);
        assert_eq!(cells.len(), 4);
        // Blocks half a metre a side: its ground as large in metres, twice as many blocks each way.
        assert_eq!(file.set_block_size(0.5), 0.5);
        assert_eq!((file.ground.size, file.ground.height, file.ground.under_depth), ([32, 32], 8, 6));
        let ground = file.make_ground(None);
        let none = |_: BlockId| false;
        let resize = Resize { from: 1.0, to: 0.5, smooth: false, water: &none };
        let mut now = ground.clone();
        for (at, block, fill) in resized(&cells, None, &resize) {
            now.set_cell(at, block, fill);
        }
        // The pillar: the same 1 m by 1 m by 3 m of stone, now 2 by 2 by 6 blocks; the hole 2 blocks each way.
        let stone = |map: &BlockMap, at: [i32; 3]| map.get(at) == 3;
        let pillar = (8..14).flat_map(|y| (18..20).flat_map(move |z| (10..12).map(move |x| [x, y, z])));
        assert!(pillar.into_iter().all(|at| stone(&now, at)));
        assert!(!stone(&now, [12, 8, 18]) && !stone(&now, [10, 14, 18]) && now.get([9, 8, 18]) == AIR);
        let hole = (6..8).flat_map(|y| (4..6).flat_map(move |z| (20..22).map(move |x| [x, y, z])));
        assert!(hole.into_iter().all(|at| now.get(at) == AIR));
        assert_eq!((now.get([19, 7, 4]), now.get([20, 5, 4])), (1, 2), "the ground round the hole as it is made");
        // As changes alone, as a world without end keeps those out of reach: the same blocks.
        let edits = BlockEdits::between(&base, &built).resized(&resize);
        let mut again = ground.clone();
        edits.apply(&mut again);
        assert_eq!(again, now);
        // And back to a metre: as it was built.
        let back = Resize { from: 0.5, to: 1.0, smooth: false, water: &none };
        file.set_block_size(1.0);
        let mut first = file.make_ground(None);
        for (at, block, fill) in resized(&changed(&ground, &now), None, &back) {
            first.set_cell(at, block, fill);
        }
        assert_eq!(first, built);
    }

    #[test]
    fn smooth_ground_built_up_keeps_its_surface_when_blocks_change_size() {
        use super::super::smooth::{Ball, ball};
        // Smooth ground 4.3 m up in blocks a metre a side, a mound added on it.
        let ground = |size: f32| {
            let mut map = BlockMap::default();
            let cells = (16.0 / size) as i32;
            for z in 0..cells {
                for x in 0..cells {
                    for y in 0..(6.0 / size) as i32 {
                        let depth = 4.3 / size - (y as f32 + 0.5);
                        map.set_cell([x, y, z], if depth > 0.0 { 1 } else { AIR }, fill_of(depth, depth > 0.0));
                    }
                }
            }
            map
        };
        let base = ground(1.0);
        let mut built = base.clone();
        for (at, block, fill) in ball(&built.clone(), [8.5, 4.5, 8.5], 2.0, Ball::Add(1), &[]) {
            built.set_cell(at, block, fill);
        }
        let none = |_: BlockId| false;
        let resize = Resize { from: 1.0, to: 0.5, smooth: true, water: &none };
        let mut now = ground(0.5);
        for (at, block, fill) in resized(&changed(&base, &built), Some(&built), &resize) {
            now.set_cell(at, block, fill);
        }
        // How high the ground stands over a spot, in metres, by its blocks and how full they are.
        let top = |map: &BlockMap, size: f32, x: f32, z: f32| {
            let column = [(x / size) as i32, (z / size) as i32];
            let y = (0..40).rev().find(|&y| map.get([column[0], y, column[1]]) != AIR).unwrap();
            (y as f32 + 0.5 + depth_of(map.fill([column[0], y, column[1]]))) * size
        };
        // The mound's top, 6.5 m up, and the ground away from it, 4.3 m up, stand where they did.
        for (x, z) in [(8.5, 8.5), (8.5, 9.5), (2.5, 3.5)] {
            let (was, is) = (top(&built, 1.0, x, z), top(&now, 0.5, x + 0.01, z + 0.01));
            assert!((was - is).abs() < 0.3, "at {x}, {z}: {was} m before, {is} m after");
        }
        assert!(top(&now, 0.5, 8.5, 8.5) > 6.0);
    }

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
