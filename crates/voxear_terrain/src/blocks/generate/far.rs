//! The far view of a world of blocks without end: past its second ring of
//! coarser chunks, its ground is kept as columns of runs, each a block from
//! one height to another, made straight from the generator's heights,
//! climate and sea a level's chunk column (a tile) at a time, with no
//! block's depth asked of the generator's Volume. So the far view shows the
//! ground's surface, its layers and its sea, and none of the overhangs or
//! caves a Volume makes, too small to see so far off.
//!
//! A tile is made into chunks (see [`far_chunks`]) and meshed as a coarse
//! ring's are, kept small on disk (see [`FarTile::to_bytes`]), and takes
//! more blocks into its columns (see [`FarTile::add`]), as a structure or a
//! tree may show far off.

use anyhow::{Result, ensure};

use super::{
    BlockGenerate,
    cells::{Sea, cell, source},
    columns::{columns_at, sea_of},
};
use crate::{
    blocks::{
        chunk::{AIR, BlockId, CHUNK, Chunk, fill_of},
        file::BlockFile,
        flow::{Water, surface_fill},
    },
    generate::WorldGen,
};

/// Blocks from `from` up to `to`, not reached, in the tile's level's blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub from: i32,
    pub to: i32,
    pub block: BlockId,
}

/// One chunk column of a level of the far view: each of its columns' surface
/// in the level's blocks and its runs, lowest first, a row along x after
/// another; and its sea's block and top.
#[derive(Clone, Debug, PartialEq)]
pub struct FarTile {
    pub level: u32,
    pub at: [i32; 2],
    pub sea: (BlockId, f32),
    surfaces: Vec<f32>,
    /// Where each column's runs start in `runs`, and where the last ends.
    starts: Vec<u32>,
    runs: Vec<Run>,
}

/// What a tile's bytes start with, and the version of what follows.
const MAGIC: &[u8; 4] = b"VXFR";
const VERSION: u8 = 1;

const SIDE: usize = CHUNK as usize;

impl FarTile {
    /// The runs of column `[x, z]`, lowest first.
    pub fn column(&self, [x, z]: [u32; 2]) -> &[Run] {
        let at = z as usize * SIDE + x as usize;
        &self.runs[self.starts[at] as usize..self.starts[at + 1] as usize]
    }

    /// The surface of column `[x, z]`, in the level's blocks.
    pub fn surface(&self, [x, z]: [u32; 2]) -> f32 {
        self.surfaces[z as usize * SIDE + x as usize]
    }

    /// Puts `block` into column `[x, z]` from `from` up to `to`, over
    /// whatever was there: how something placed on the ground, a structure
    /// or a tree as a few blocks, shows in the far view. Air takes blocks out.
    pub fn add(&mut self, [x, z]: [u32; 2], from: i32, to: i32, block: BlockId) {
        if from >= to {
            return;
        }
        let at = z as usize * SIDE + x as usize;
        let (first, last) = (self.starts[at] as usize, self.starts[at + 1] as usize);
        let mut column: Vec<Run> = Vec::with_capacity(last - first + 2);
        for run in &self.runs[first..last] {
            if run.from < from {
                column.push(Run { to: run.to.min(from), ..*run });
            }
            if run.to > to {
                column.push(Run { from: run.from.max(to), ..*run });
            }
        }
        if block != AIR {
            column.push(Run { from, to, block });
        }
        column.sort_by_key(|run| run.from);
        // Runs of one block that meet are one.
        let mut merged: Vec<Run> = Vec::with_capacity(column.len());
        for run in column {
            match merged.last_mut() {
                Some(last) if last.to == run.from && last.block == run.block => last.to = run.to,
                _ => merged.push(run),
            }
        }
        let grown = merged.len() as i64 - (last - first) as i64;
        self.runs.splice(first..last, merged);
        for start in &mut self.starts[at + 1..] {
            *start = (*start as i64 + grown) as u32;
        }
    }

    /// Its runs as a few bytes, compressed: to keep on disk, so a place seen
    /// once comes back at once.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16 + self.surfaces.len() * 6 + self.runs.len() * 10);
        bytes.extend(MAGIC);
        bytes.push(VERSION);
        bytes.extend(self.level.to_le_bytes());
        bytes.extend(self.at[0].to_le_bytes());
        bytes.extend(self.at[1].to_le_bytes());
        bytes.extend(self.sea.0.to_le_bytes());
        bytes.extend(self.sea.1.to_le_bytes());
        for at in 0..self.surfaces.len() {
            bytes.extend(self.surfaces[at].to_le_bytes());
            let runs = &self.runs[self.starts[at] as usize..self.starts[at + 1] as usize];
            bytes.extend((runs.len() as u16).to_le_bytes());
            for run in runs {
                bytes.extend(run.from.to_le_bytes());
                bytes.extend(run.to.to_le_bytes());
                bytes.extend(run.block.to_le_bytes());
            }
        }
        lz4_flex::compress_prepend_size(&bytes)
    }

    /// A tile from what [`FarTile::to_bytes`] gave.
    pub fn from_bytes(compressed: &[u8]) -> Result<Self> {
        let bytes = lz4_flex::decompress_size_prepended(compressed)?;
        let mut read = Reader { bytes: &bytes, at: 0 };
        ensure!(read.take(4)? == MAGIC && read.take(1)?[0] == VERSION, "not a far tile this version reads");
        let level = read.u32()?;
        let at = [read.i32()?, read.i32()?];
        let sea = (read.u16()?, read.f32()?);
        let mut tile = FarTile { level, at, sea, surfaces: Vec::new(), starts: vec![0], runs: Vec::new() };
        for _ in 0..SIDE * SIDE {
            tile.surfaces.push(read.f32()?);
            for _ in 0..read.u16()? {
                tile.runs.push(Run { from: read.i32()?, to: read.i32()?, block: read.u16()? });
            }
            tile.starts.push(tile.runs.len() as u32);
        }
        ensure!(read.at == bytes.len(), "a far tile with more after it");
        Ok(tile)
    }
}

/// Reads a tile's bytes in order.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        ensure!(self.at + count <= self.bytes.len(), "a far tile cut short");
        self.at += count;
        Ok(&self.bytes[self.at - count..self.at])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
}

/// The far view's tile `at` of level `level` of the ground `generate` makes
/// for `file` with generator `world`: each column's blocks as a plain
/// height's are, its layers and its sea, as `column_chunks` makes them where
/// the ground is not pushed about or hollowed.
pub fn far_tile(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen, level: u32, at: [i32; 2]) -> FarTile {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let side = CHUNK as i32;
    let source = source(generate, world, file.block_size.max(0.01));
    let world = source.world();
    let middle = |v: i32, axis: usize| generate.position[axis] + (v as f32 + 0.5) * size;
    let places = (0..side).flat_map(|z| (0..side).map(move |x| (at[0] * side + x, at[1] * side + z)));
    let (xs, zs): (Vec<f32>, Vec<f32>) = places.map(|(x, z)| (middle(x, 0), middle(z, 1))).unzip();
    let mut heights = vec![0.0; xs.len()];
    world.heights(&xs, &zs, &mut heights);
    let columns = columns_at(file, world, level, (&xs, &zs, &heights));
    let (sea, sea_top) = sea_of(file, generate, world, level);
    let low = generate.bottom.div_euclid(scale as i32);
    let mut tile = FarTile { level, at, sea: (sea, sea_top), surfaces: Vec::new(), starts: vec![0], runs: Vec::new() };
    for column in &columns {
        let high = (column.surface.max(sea_top) + 1.0).ceil() as i32;
        let mut runs: Vec<Run> = Vec::new();
        for y in low..=high {
            let plainly = column.surface - (y as f32 + 0.5);
            let made =
                cell(column, plainly, plainly, plainly, plainly, Sea::at(sea, sea_top, y, file.smooth), file.smooth);
            let Some((block, _)) = made.filter(|(block, _)| *block != AIR) else { continue };
            match runs.last_mut() {
                Some(last) if last.to == y && last.block == block => last.to = y + 1,
                _ => runs.push(Run { from: y, to: y + 1, block }),
            }
        }
        tile.surfaces.push(column.surface);
        tile.runs.extend(runs);
        tile.starts.push(tile.runs.len() as u32);
    }
    tile
}

/// The chunks of `tile`, of a world drawn `smooth` or of cubes: each run's
/// blocks, and where the ground and the sea keep how full a block is, as
/// `column_chunks` keeps it for a plain height.
pub fn far_chunks(tile: &FarTile, smooth: bool) -> Vec<([i32; 3], Chunk)> {
    let side = CHUNK as i32;
    let (sea, sea_top) = tile.sea;
    let mut chunks: std::collections::BTreeMap<i32, Chunk> = std::collections::BTreeMap::new();
    let mut put = |[x, y, z]: [u32; 3], at: i32, block: BlockId, fill: Option<u8>| {
        let chunk = chunks.entry(at.div_euclid(side)).or_insert_with(|| Chunk::filled(AIR));
        match fill {
            Some(fill) => chunk.set_cell(x, y, z, block, fill),
            None => chunk.set(x, y, z, block),
        }
    };
    for z in 0..CHUNK {
        for x in 0..CHUNK {
            let surface = tile.surface([x, z]);
            let runs = tile.column([x, z]);
            for run in runs {
                for y in run.from..run.to {
                    let fill = if sea != AIR && run.block == sea {
                        let over = sea_top - (y as f32 + 0.5);
                        match Sea::at(sea, sea_top, y, smooth).eighths {
                            _ if smooth => Some(surface_fill(over)),
                            Some(eighths) if eighths < 8.0 => {
                                Some(Water::Flowing { level: eighths as u8, up: 2 }.fill())
                            }
                            _ => None,
                        }
                    } else {
                        smooth.then(|| fill_of(surface - (y as f32 + 0.5), true))
                    };
                    put([x, y.rem_euclid(side) as u32, z], y, run.block, fill);
                }
            }
            // Drawn smooth, the air a block from the surface keeps how far it is from it too.
            if smooth {
                let mut y = (surface - 0.5).ceil() as i32;
                while surface - (y as f32 + 0.5) > -1.0 {
                    if !runs.iter().any(|run| (run.from..run.to).contains(&y)) {
                        put(
                            [x, y.rem_euclid(side) as u32, z],
                            y,
                            AIR,
                            Some(fill_of(surface - (y as f32 + 0.5), false)),
                        );
                    }
                    y += 1;
                }
            }
        }
    }
    chunks.into_iter().map(|(cy, chunk)| ([tile.at[0], cy, tile.at[1]], chunk)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::{
            file::{BlockKind, Ground},
            generate::column_chunks,
        },
        generate::Preset,
    };

    /// A world of plain ground: no overhangs or caves, and a sea.
    fn plain(smooth: bool) -> (BlockFile, BlockGenerate, WorldGen) {
        let mut file =
            BlockFile { smooth, ground: Ground { size: [0, 0], ..Ground::default() }, ..BlockFile::default() };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let generate = BlockGenerate {
            world: Some("hills".into()),
            overhangs: 0.0,
            caves: 0.0,
            bottom: -32,
            sea: file.blocks.len() as BlockId,
            position: [300.0, -700.0],
            ..BlockGenerate::default()
        };
        file.generate = Some(generate.clone());
        (file, generate, Preset::RollingHills.world())
    }

    #[test]
    fn a_far_tile_is_made_into_the_same_chunks_as_a_coarse_column_of_plain_ground() {
        for smooth in [false, true] {
            let (file, generate, world) = plain(smooth);
            for (level, at) in [(3, [0, 0]), (3, [-2, 1]), (5, [1, -1])] {
                let tile = far_tile(&file, &generate, &world, level, at);
                let mut made = far_chunks(&tile, smooth);
                let mut coarse = column_chunks(&file, &generate, &world, level, at, None);
                made.sort_by_key(|(key, _)| key[1]);
                coarse.sort_by_key(|(key, _)| key[1]);
                assert_eq!(made.len(), coarse.len(), "level {level} at {at:?}, smooth {smooth}");
                for ((key, chunk), (kept, want)) in made.iter().zip(&coarse) {
                    assert_eq!(key, kept);
                    assert!(chunk == want, "chunk {key:?} of level {level}, smooth {smooth}");
                }
                // Each column a few runs: its layers, and the sea over it where the ground is low.
                let runs = (0..CHUNK).flat_map(|z| (0..CHUNK).map(move |x| [x, z])).map(|c| tile.column(c).len());
                assert!(runs.max().unwrap() <= 5);
            }
        }
    }

    #[test]
    fn a_far_tile_keeps_as_a_few_bytes_and_takes_blocks_added_to_its_columns() {
        let (file, generate, world) = plain(false);
        let mut tile = far_tile(&file, &generate, &world, 4, [2, -3]);
        let bytes = tile.to_bytes();
        assert!(bytes.len() < 16 * 1024, "{} bytes", bytes.len());
        assert_eq!(FarTile::from_bytes(&bytes).unwrap(), tile);
        assert!(FarTile::from_bytes(&bytes[..bytes.len() / 2]).is_err());
        // A tower of stone over a column, a block of it taken out, and nothing else changed.
        let block_at =
            |runs: &[Run], y: i32| runs.iter().find(|run| (run.from..run.to).contains(&y)).map(|run| run.block);
        let column = tile.column([5, 7]).to_vec();
        let top = column.last().unwrap().to;
        tile.add([5, 7], top - 1, top + 20, 3);
        tile.add([5, 7], top + 10, top + 11, AIR);
        let now = tile.column([5, 7]);
        assert_eq!(block_at(now, top - 2), block_at(&column, top - 2), "under it as it was");
        assert!((top - 1..top + 10).chain(top + 11..top + 20).all(|y| block_at(now, y) == Some(3)), "{now:?}");
        assert_eq!((block_at(now, top + 10), block_at(now, top + 20)), (None, None));
        assert!(
            now.windows(2)
                .all(|two| two[0].to <= two[1].from && (two[0].to < two[1].from || two[0].block != two[1].block))
        );
        assert_eq!(tile.column([6, 7]), far_tile(&file, &generate, &world, 4, [2, -3]).column([6, 7]));
        assert_eq!(FarTile::from_bytes(&tile.to_bytes()).unwrap(), tile);
    }
}
