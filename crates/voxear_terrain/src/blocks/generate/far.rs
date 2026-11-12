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

/// The way tiles are made, counted up when it changes, so tiles kept on
/// disk made another way are made again (see `stream::BlockMaking`).
pub const MADE_AS: u32 = 2;

/// How the sea of `block`, its top `top` blocks up, stands at block `y` of
/// a far tile's column whose ground's surface is `surface`: as `Sea::at`
/// says, but where the ground lies under the sea, a block its top lies in is
/// at least an eighth full. So the sea still shows far off, where a block is
/// 64 m or more and the sea stands less than a sixteenth of one over its
/// floor, and low land beside it, rounded down to the same block, stays land.
fn far_sea(block: BlockId, top: f32, y: i32, (smooth, surface): (bool, f32)) -> Sea {
    let mut sea = Sea::at(block, top, y, smooth);
    if top > y as f32 && surface < top {
        sea.eighths = sea.eighths.map(|eighths| eighths.max(1.0));
    }
    sea
}

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
        // Under every layer the column is its bottom block all the way down: one run, not asked a block at a time.
        let layered = column.layers[0].1 + column.layers[1].1 + 3.0;
        let rock_to = ((column.surface - layered).floor() as i32).clamp(low, high.max(low));
        if rock_to > low && column.bottom != AIR {
            runs.push(Run { from: low, to: rock_to, block: column.bottom });
        }
        let asked_from = if column.bottom != AIR { rock_to } else { low };
        for y in asked_from..=high {
            let plainly = column.surface - (y as f32 + 0.5);
            let sea_at = far_sea(sea, sea_top, y, (file.smooth, column.surface));
            let made = cell(column, plainly, plainly, plainly, plainly, sea_at, file.smooth);
            let Some((block, _)) = made.filter(|(block, _)| *block != AIR) else { continue };
            match runs.last_mut() {
                Some(last) if last.to == y && last.block == block => last.to = y + 1,
                _ => runs.push(Run { from: y, to: y + 1, block }),
            }
        }
        // Of cubes, ground under the sea whose top block reaches the sea's level shows no water over it: that block
        // is the sea's, so a shallow sea far off is water, as drawn smooth it takes the ground's top block too.
        let under_sea = sea != AIR && !file.smooth && column.surface < sea_top;
        if let Some(last) = runs.last_mut().filter(|last| under_sea && last.block != sea) {
            if last.to - last.from > 1 {
                last.to -= 1;
                let top = last.to;
                runs.push(Run { from: top, to: top + 1, block: sea });
            } else {
                last.block = sea;
            }
        }
        tile.surfaces.push(column.surface);
        tile.runs.extend(runs);
        tile.starts.push(tile.runs.len() as u32);
    }
    // Ground the generator's Volume stands apart from the columns' own, arches and raised land: each column's
    // blocks whose middles lie in it, the column's top block over its bottom one.
    let standing = match world.volume().filter(|volume| source.own() && volume.spanned()) {
        Some(volume) => volume.standing(&xs, &zs, &heights),
        None => Vec::new(),
    };
    for (at, spans) in standing.iter().enumerate() {
        let place = [at as u32 % CHUNK, at as u32 / CHUNK];
        for [foot, top] in spans {
            // Its top block is that of ground as high as it stands.
            let raised = columns_at(file, world, level, (&xs[at..=at], &zs[at..=at], &[*top]));
            let cover = raised.first().map_or(columns[at].layers[0].0, |column| column.layers[0].0);
            let (from, to) = ((foot / size - 0.5).ceil() as i32, (top / size - 0.5).floor() as i32 + 1);
            let from = from.max((columns[at].surface - 0.5).floor() as i32 + 1);
            // What is thinner than a block of this level is a block still, where it stands clear of the ground.
            let to = if to <= from && top - foot > 0.0 && (top / size).floor() as i32 >= from { from + 1 } else { to };
            if to > from {
                tile.add(place, from, to - 1, columns[at].bottom);
                tile.add(place, to - 1, to, cover);
            }
        }
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
    // Chunks that are one block of rock in every column, from block `rock.0` up to `rock.1`, are not put a block
    // at a time.
    let places = || (0..CHUNK).flat_map(|z| (0..CHUNK).map(move |x| [x, z]));
    let lowest = |at: [u32; 2]| tile.column(at).first().copied();
    let rock = lowest([0, 0]).filter(|first| first.block != sea || sea == AIR).and_then(|first| {
        let mut within = (first.from, first.to);
        for at in places() {
            let run = lowest(at).filter(|run| run.block == first.block)?;
            let deep = (tile.surface(at) - 1.5).floor() as i32;
            within = (within.0.max(run.from), within.1.min(run.to).min(deep));
        }
        let from = within.0.div_euclid(side) + i32::from(within.0.rem_euclid(side) != 0);
        let to = within.1.div_euclid(side);
        (to > from).then_some((from * side, to * side, first.block))
    });
    for z in 0..CHUNK {
        for x in 0..CHUNK {
            let surface = tile.surface([x, z]);
            let runs = tile.column([x, z]);
            for run in runs {
                for y in run.from..run.to {
                    if rock.is_some_and(|(from, to, _)| (from..to).contains(&y)) {
                        continue;
                    }
                    let fill = if sea != AIR && run.block == sea {
                        let over = sea_top - (y as f32 + 0.5);
                        match far_sea(sea, sea_top, y, (smooth, surface)).eighths {
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
    if let Some((from, to, block)) = rock {
        for cy in from.div_euclid(side)..to.div_euclid(side) {
            chunks.insert(cy, Chunk::filled(block));
        }
    }
    chunks.into_iter().map(|(cy, chunk)| ([tile.at[0], cy, tile.at[1]], chunk.tidied())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::{
            file::{BlockKind, Ground},
            generate::column_chunks,
        },
        generate::{Landmass, Preset},
    };

    /// A world of plain ground: no overhangs or caves, and a sea.
    fn plain(smooth: bool) -> (BlockFile, BlockGenerate, WorldGen) {
        plain_with(smooth, true)
    }

    fn plain_with(smooth: bool, sea: bool) -> (BlockFile, BlockGenerate, WorldGen) {
        let mut file =
            BlockFile { smooth, ground: Ground { size: [0, 0], ..Ground::default() }, ..BlockFile::default() };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let generate = BlockGenerate {
            world: Some("hills".into()),
            overhangs: 0.0,
            caves: 0.0,
            bottom: -32,
            sea: if sea { file.blocks.len() as BlockId } else { AIR },
            position: [300.0, -700.0],
            ..BlockGenerate::default()
        };
        file.generate = Some(generate.clone());
        (file, generate, Preset::RollingHills.world())
    }

    #[test]
    fn raised_land_and_arches_stand_in_the_far_view() {
        use crate::generate::{
            Graph, NodeKind,
            nodes::{Arches, Noise, Output, Plateaus, RaisedLand},
        };
        let (file, generate, _) = plain_with(false, false);
        let generate = BlockGenerate { position: [0.0, 0.0], ..generate };
        let world_of = |height: NodeKind, volume: NodeKind| {
            let mut graph = Graph::default();
            let (height, volume) = (graph.add(height, [0.0; 2]), graph.add(volume, [0.0; 2]));
            let output = graph.add(NodeKind::Output(Output::default()), [0.0; 2]);
            assert!(graph.connect(height, 0, output, Output::HEIGHT) && graph.connect(volume, 0, output, Output::VOLUME));
            WorldGen::new(4, 1.0, -50.0, graph)
        };
        let hills = NodeKind::Noise(Noise { size: 400.0, low: 0.0, high: 80.0, ..Noise::default() });
        let land = Plateaus { size: 500.0, low: 0.0, high: 200.0, ramps: 0.0, canyon: 90.0, ..Plateaus::default() };
        let worlds = [
            world_of(hills, NodeKind::RaisedLand(RaisedLand { often: 0.7, ..RaisedLand::default() })),
            world_of(
                NodeKind::Plateaus(land),
                NodeKind::Arches(Arches { span_from: 90.0, span_to: 200.0, often: 1.0, ..Arches::default() }),
            ),
        ];
        for (which, world) in worlds.iter().enumerate() {
            // Blocks 8 m a side, tiles 256 m: over four kilometres, columns with blocks standing clear of the
            // ground under them, air between, and the same blocks in the tile's chunks.
            let (mut clear, mut found) = (0, 0);
            for at in (0..64).map(|at| [at % 8 - 4, at / 8 - 4]) {
                let tile = far_tile(&file, &generate, world, 3, at);
                let chunks = far_chunks(&tile, false);
                for column in (0..CHUNK * CHUNK).map(|at| [at % CHUNK, at / CHUNK]) {
                    let runs = tile.column(column);
                    let Some(gap) = runs.windows(2).find(|pair| pair[1].from > pair[0].to) else { continue };
                    clear += 1;
                    assert!(gap[1].from as f32 > tile.surface(column), "it stands over the ground");
                    let y = gap[1].to - 1;
                    let chunk = chunks.iter().find(|(key, _)| key[1] == y.div_euclid(CHUNK as i32));
                    let block = chunk.map(|(_, chunk)| chunk.get(column[0], y.rem_euclid(CHUNK as i32) as u32, column[1]));
                    assert_eq!(block, Some(gap[1].block), "a run is its chunk's blocks");
                    found += usize::from(gap[1].from - gap[0].to >= 3);
                }
            }
            assert!(clear > 40 && found > 10, "world {which}: {clear} columns stand clear, {found} by 24 m or more");
        }
    }

    #[test]
    fn a_far_tile_is_made_into_the_same_chunks_as_a_coarse_column_of_plain_ground() {
        for smooth in [false, true] {
            // Of cubes, the far view's sea keeps to the ground under it, where coarse chunks keep to their blocks.
            let (file, generate, world) = plain_with(smooth, smooth);
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
    fn the_sea_shows_in_the_farthest_tiles_where_it_stands_less_than_an_eighth_of_a_block_over_their_floor() {
        let (file, mut generate, _) = plain(false);
        generate.position = [0.0, 0.0];
        let sea = generate.sea;
        // Blocks 128 m a side, the sea at a block's floor and a few metres over it.
        let mut wet = 0;
        for (sea_level, at) in
            [0.0, 3.0].into_iter().flat_map(|level| [[0, 0], [1, 0], [0, 1], [-1, -1]].map(|at| (level, at)))
        {
            let world: WorldGen = Landmass { sea_level, ..Preset::Archipelago.landmass() }.into();
            let tile = far_tile(&file, &generate, &world, 7, at);
            assert!(tile.sea.1.fract() < 1.0 / 16.0, "less than a sixteenth of a block: {}", tile.sea.1);
            for column in (0..CHUNK).flat_map(|z| (0..CHUNK).map(move |x| [x, z])) {
                // Water where the ground lies under the sea, and land where it does not, however near its level.
                let top = tile.column(column).last().map(|run| run.block);
                if tile.surface(column) < tile.sea.1 {
                    assert_eq!(top, Some(sea), "{column:?} of {at:?}");
                    wet += 1;
                } else {
                    assert_ne!(top, Some(sea), "{column:?} of {at:?}");
                }
            }
        }
        assert!(wet > 100, "sea far off: {wet} columns");
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
