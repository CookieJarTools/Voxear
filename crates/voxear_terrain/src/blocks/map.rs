//! A world of blocks as a map draws it: a small model of the ground, a tile
//! of 32 by 32 columns at a time, each column cut down to what shows from
//! above and at the map's cut sides.
//!
//! A column of a map is its ground (how high it stands, its top block and
//! the two layers under it), the water over it, and up to two pieces
//! standing free of the ground: a trunk and its crown, a bridge, a roof.
//! Caves and whatever else lies inside the ground are left out.
//!
//! A map's zoom is a level, as the far view's is: at level 0 a column of
//! the map is a column of blocks, and each level up its cells are twice as
//! large. A world without end's tiles are the far view's (see
//! `generate::far`), made from the generator alone, with what stands on
//! that ground put into them: the structures its generator builds, the
//! plants it grows by itself, and the world's changes, what was built and
//! what was dug. A world whose every chunk is held is read from its blocks.
//!
//! Structures are built as the world's own chunks build them (see
//! `structure::build_over`), out to cells of eight blocks: past that a
//! house is too small a part of a cell to show. A plant the world grows
//! (see [`MapGround::growing`]) stands where it grows, a column of its
//! block with its crown round it, while it is at least a cell across;
//! where cells are larger its kind shows as the far view's woods do.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId, CHUNK, Chunk},
    edits::{BlockEdits, ChunkChange},
    generate::far::{FarTile, Run},
    grow::{FarGround, Woods, gone_far, grow_far},
    is_water,
    stream::{BlockMaking, FAR_FROM, FarBuilt, far_tile_kept},
};

/// A tile's columns along a side.
pub const MAP_TILE: usize = CHUNK as usize;

/// How thick a run standing free of the ground is, in the map's cells, for
/// it to be ground with a cave under it and not a piece over the ground.
const GROUND_FROM: i32 = 16;

/// How many of a map cell's blocks must have been built for the cell to be
/// built, as a part of all of them: an eighth, as the far view shows what
/// was built.
const BUILT_SHOWS: u64 = 8;

/// The coarsest level a map shows its generator's structures at: cells of
/// eight blocks. In larger ones a house is under an eighth of a cell.
const STRUCTURES_TO: u32 = 3;

/// How far under the ground's surface a structure's blocks still go into a
/// map, in blocks: its floor and its way in, and none of its cellars.
const UNDER_SHOWS: i32 = 4;

/// Something standing free of a column's ground, from `from` up to `to`,
/// not reached, in the map's cells: its top block, and the block most of
/// its sides are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapPiece {
    pub from: i32,
    pub to: i32,
    pub top: BlockId,
    pub side: BlockId,
}

impl MapPiece {
    /// No piece.
    pub const NONE: MapPiece = MapPiece { from: 0, to: 0, top: AIR, side: AIR };

    /// Whether there is one.
    pub fn is_some(&self) -> bool {
        self.to > self.from
    }
}

/// One column of a map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapColumn {
    /// Where its ground ends, in the map's cells: the first cell over it.
    pub top: i32,
    /// The ground's top block, the block under it and the block under that.
    pub blocks: [BlockId; 3],
    /// How many cells thick the top block and the one under it are, 255 at
    /// most.
    pub depths: [u8; 2],
    /// Where the water over its ground ends, `top` where there is none.
    pub water: i32,
    /// The water's block.
    pub water_block: BlockId,
    /// What stands free over the ground, lowest first.
    pub pieces: [MapPiece; 2],
    /// Whether its sides are drawn as its layers are, each as thick as it
    /// is: something built, as a house's walls under its roof. Ground is
    /// drawn with its top block's colour a little way down its steps, so a
    /// grassy slope is not striped with the earth under it.
    pub layered: bool,
}

impl MapColumn {
    /// A column with nothing in it: past a world's edge.
    pub const NONE: MapColumn = MapColumn {
        top: i32::MIN,
        blocks: [AIR; 3],
        depths: [0; 2],
        water: i32::MIN,
        water_block: AIR,
        pieces: [MapPiece::NONE; 2],
        layered: false,
    };

    /// Whether anything is in it.
    pub fn is_some(&self) -> bool {
        self.top != i32::MIN
    }

    /// Where its highest block ends: its ground's, its water's or a piece's.
    pub fn highest(&self) -> i32 {
        let pieces = self.pieces.iter().filter(|piece| piece.is_some()).map(|piece| piece.to);
        pieces.fold(self.top.max(self.water), i32::max)
    }

    /// The column `runs` make, lowest first, none over another, with
    /// `faces` telling water from ground.
    pub fn of_runs(runs: &[Run], faces: &[BlockFaces]) -> MapColumn {
        let ground: Vec<Run> = runs.iter().copied().filter(|run| !is_water(faces, run.block)).collect();
        let Some(first) = ground.first() else {
            return Self::of_water(runs);
        };
        // Runs that meet are one stretch of ground; a stretch over a gap is the ground again where it is thick,
        // with a cave under it, and a piece where it is thin.
        let mut top = first.to;
        let mut last_of_ground = 0;
        let mut pieces: Vec<(usize, usize)> = Vec::new();
        let mut at = 1;
        while at < ground.len() {
            let from = at;
            while at + 1 < ground.len() && ground[at + 1].from == ground[at].to {
                at += 1;
            }
            let (low, high) = (ground[from].from, ground[at].to);
            if low == top {
                (top, last_of_ground) = (high, at);
            } else if high - low >= GROUND_FROM {
                (top, last_of_ground) = (high, at);
                pieces.clear();
            } else {
                pieces.push((from, at));
            }
            at += 1;
        }
        let mut column = MapColumn { top, water: top, ..Self::NONE };
        column.layers(&ground[..=last_of_ground]);
        // The two highest pieces are kept: what shows from above.
        let kept = pieces.len().saturating_sub(2);
        for (slot, (from, to)) in pieces[kept..].iter().enumerate() {
            column.pieces[slot] = piece_of(&ground[*from..=*to]);
        }
        // Water stands on the ground: each run of it that meets the last.
        for run in runs.iter().filter(|run| is_water(faces, run.block)) {
            if run.from <= column.water && run.to > column.water {
                (column.water, column.water_block) = (run.to, run.block);
            }
        }
        column
    }

    /// A column of water alone, its ground too deep to have been made: the
    /// water is its ground.
    fn of_water(runs: &[Run]) -> MapColumn {
        let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
            return Self::NONE;
        };
        MapColumn { top: first.from, blocks: [last.block; 3], water: last.to, water_block: last.block, ..Self::NONE }
    }

    /// Takes its top block and the two layers under it from `ground`, its
    /// runs up to its top, lowest first.
    fn layers(&mut self, ground: &[Run]) {
        let mut bands: Vec<(BlockId, i32)> = Vec::with_capacity(3);
        for run in ground.iter().rev() {
            let full = bands.len() == 3;
            match bands.last_mut() {
                Some((block, thick)) if *block == run.block => *thick += run.to - run.from,
                _ if full => break,
                _ => bands.push((run.block, run.to - run.from)),
            }
        }
        let band = |at: usize| bands.get(at).or(bands.last()).copied().unwrap_or((AIR, 0));
        self.blocks = [band(0).0, band(1).0, band(2).0];
        self.depths = [band(0).1.clamp(0, 255) as u8, band(1).1.clamp(0, 255) as u8];
    }
}

/// The piece `runs` make, each meeting the next, lowest first.
fn piece_of(runs: &[Run]) -> MapPiece {
    let (first, last) = (runs[0], runs[runs.len() - 1]);
    let mut thick: Vec<(BlockId, i32)> = Vec::new();
    for run in runs {
        match thick.iter_mut().find(|(block, _)| *block == run.block) {
            Some((_, cells)) => *cells += run.to - run.from,
            None => thick.push((run.block, run.to - run.from)),
        }
    }
    let side = thick.iter().max_by_key(|(_, cells)| *cells).map_or(last.block, |(block, _)| *block);
    MapPiece { from: first.from, to: last.to, top: last.block, side }
}

/// A tile of a map: 32 by 32 columns of level `level`, a row along x after
/// another, the tile `at` along x and z in tiles of that level.
#[derive(Clone, Debug, PartialEq)]
pub struct MapTile {
    pub level: u32,
    pub at: [i32; 2],
    pub columns: Vec<MapColumn>,
}

impl MapTile {
    /// Column `[x, z]` of the tile.
    pub fn column(&self, [x, z]: [u32; 2]) -> &MapColumn {
        &self.columns[z as usize * MAP_TILE + x as usize]
    }

    /// The lowest ground's top and the highest block's top of its columns:
    /// none where it is empty.
    pub fn heights(&self) -> Option<[i32; 2]> {
        let some = self.columns.iter().filter(|column| column.is_some());
        some.fold(None, |range: Option<[i32; 2]>, column| {
            let [low, high] = range.unwrap_or([i32::MAX, i32::MIN]);
            Some([low.min(column.top), high.max(column.highest())])
        })
    }
}

/// Where a held world's chunks stand: each chunk column's chunks' heights,
/// lowest first. Made once for a map of blocks and kept beside it.
#[derive(Clone, Debug, Default)]
pub struct ChunkColumns {
    heights: HashMap<[i32; 2], Vec<i32>>,
}

impl ChunkColumns {
    pub fn of(blocks: &BlockMap) -> Self {
        let mut heights: HashMap<[i32; 2], Vec<i32>> = HashMap::new();
        for (key, _) in blocks.chunks() {
            heights.entry([key[0], key[2]]).or_default().push(key[1]);
        }
        heights.values_mut().for_each(|column| column.sort_unstable());
        Self { heights }
    }
}

/// What a map's tiles are made from.
#[derive(Clone)]
pub enum MapGround {
    /// A world whose every chunk is held: its blocks, and where its chunks
    /// stand.
    Held { blocks: Arc<BlockMap>, stands: Arc<ChunkColumns>, faces: Arc<[BlockFaces]> },
    /// A world without end: what it is made from, where its far view is
    /// kept (see `BlockMaking::far_tiles`), every change made to it, and
    /// the plants it grows by itself that a map shows (see
    /// [`MapGround::growing`]).
    Made { making: Arc<BlockMaking>, folder: Option<PathBuf>, changes: Arc<BlockEdits>, plants: Arc<[Woods]> },
}

impl MapGround {
    /// A held world's ground.
    pub fn held(blocks: Arc<BlockMap>, faces: Arc<[BlockFaces]>) -> Self {
        let stands = Arc::new(ChunkColumns::of(&blocks));
        MapGround::Held { blocks, stands, faces }
    }

    /// A world without end's ground.
    pub fn made(making: BlockMaking, changes: Arc<BlockEdits>) -> Self {
        let folder = making.far_tiles();
        MapGround::Made { making: Arc::new(making), folder, changes, plants: Arc::from(Vec::new()) }
    }

    /// A world without end's ground with the plants it grows by itself, a
    /// kind each: the rule it grows by, the block a map draws it as, and
    /// how tall and how far across a plant is at its middle size. `from`
    /// is not looked at: a map shows them at every level.
    pub fn growing(self, grown: Vec<Woods>) -> Self {
        match self {
            MapGround::Made { making, folder, changes, .. } => {
                MapGround::Made { making, folder, changes, plants: Arc::from(grown) }
            }
            held => held,
        }
    }

    /// The plants a world without end grows that its map shows.
    pub fn plants(&self) -> &[Woods] {
        match self {
            MapGround::Made { plants, .. } => plants,
            MapGround::Held { .. } => &[],
        }
    }

    /// How each kind of block is drawn, by its number.
    pub fn faces(&self) -> &Arc<[BlockFaces]> {
        match self {
            MapGround::Held { faces, .. } => faces,
            MapGround::Made { making, .. } => &making.faces,
        }
    }
}

/// The map's tile `at` of level `level` of `ground`. Safe on any thread;
/// a world without end's takes about as long as a far view's tile does.
pub fn map_tile(ground: &MapGround, level: u32, at: [i32; 2]) -> MapTile {
    let level = level.min(16);
    match ground {
        MapGround::Held { blocks, stands, faces } => held_tile(blocks, stands, faces, level, at),
        MapGround::Made { making, folder, changes, plants } => {
            // The far view keeps its own levels on disk; finer ones are quick to make again.
            let folder = folder.as_deref().filter(|_| level >= FAR_FROM);
            let mut far = far_tile_kept(making, folder, level, at);
            // Plants smaller than a cell show as the far view's woods do, where they stand thick enough.
            let cell = making.file.block_size.max(0.01) * (1u32 << level) as f32;
            for woods in plants.iter().filter(|woods| woods.width < cell) {
                woods.build(making, &mut far);
            }
            made_tile(&far, making, (changes, plants))
        }
    }
}

/// A far tile's columns as runs to change, a row along x after another.
fn runs_of(far: &FarTile) -> Vec<Vec<Run>> {
    let side = MAP_TILE as u32;
    (0..side * side).map(|at| far.column([at % side, at / side]).to_vec()).collect()
}

/// Puts `block` into `column` from `from` up to `to`, over whatever was
/// there; air takes blocks out.
fn put(column: &mut Vec<Run>, from: i32, to: i32, block: BlockId) {
    let mut next: Vec<Run> = Vec::with_capacity(column.len() + 2);
    for run in column.iter() {
        if run.from < from {
            next.push(Run { to: run.to.min(from), ..*run });
        }
        if run.to > to {
            next.push(Run { from: run.from.max(to), ..*run });
        }
    }
    if block != AIR {
        next.push(Run { from, to, block });
    }
    next.sort_by_key(|run| run.from);
    column.clear();
    for run in next {
        match column.last_mut() {
            Some(last) if last.to == run.from && last.block == run.block => last.to = run.to,
            _ => column.push(run),
        }
    }
}

/// Whether cell `y` of `column` holds ground: a block that is not water.
fn holds(column: &[Run], y: i32, faces: &[BlockFaces]) -> bool {
    column.iter().any(|run| run.from <= y && y < run.to && !is_water(faces, run.block))
}

/// Block `index` of a chunk, as `(x, y, z)`: kept along x, then z, then up.
fn place(index: usize) -> [i32; 3] {
    let (index, side) = (index as i32, CHUNK as i32);
    [index % side, index / (side * side), (index / side) % side]
}

/// Each block `change` sets in its chunk, with what it sets it to.
fn each_set(change: &ChunkChange, mut set: impl FnMut([i32; 3], BlockId)) {
    match change {
        ChunkChange::Blocks(changed) => changed.iter().for_each(|&(index, block)| set(place(index.into()), block)),
        ChunkChange::Cells(changed) => changed.iter().for_each(|&(index, block, _)| set(place(index.into()), block)),
        ChunkChange::Whole(chunk) => {
            for index in 0..(CHUNK * CHUNK * CHUNK) as usize {
                let [x, y, z] = place(index);
                set([x, y, z], chunk.get(x as u32, y as u32, z as u32));
            }
        }
    }
}

/// How many of each of a level's cells' blocks were built and dug, and the
/// block built last.
type Cells = HashMap<[i32; 3], (u64, u64, BlockId)>;

/// Counts block `at` of the world's own blocks, set to `block`, into its
/// cell of `level`.
fn count(cells: &mut Cells, level: u32, at: [i32; 3], block: BlockId) {
    let cell = cells.entry(at.map(|v| v >> level)).or_insert((0, 0, AIR));
    match block {
        AIR => cell.1 += 1,
        _ => (cell.0, cell.2) = (cell.0 + 1, block),
    }
}

/// Puts `cells` into a far tile's `columns`. At level 0 each block is as
/// it was left. Coarser, a cell is built where an eighth of it was, of the
/// block built last, and dug where over half of it was taken out.
fn put_cells((columns, layered): (&mut [Vec<Run>], &mut [bool]), cells: Cells, far: &FarTile, faces: &[BlockFaces]) {
    let (level, side) = (far.level, CHUNK as i32);
    let whole = 1u64 << (3 * level);
    for ([x, y, z], (built, dug, block)) in cells {
        let (x, z) = (x - far.at[0] * side, z - far.at[1] * side);
        if x < 0 || z < 0 || x >= side || z >= side {
            continue;
        }
        let column = &mut columns[z as usize * MAP_TILE + x as usize];
        let builds = if level == 0 { block != AIR } else { built * BUILT_SHOWS >= whole && !holds(column, y, faces) };
        layered[z as usize * MAP_TILE + x as usize] |= builds;
        if level == 0 || builds {
            put(column, y, y + 1, block);
        } else if dug * 2 > whole {
            put(column, y, y + 1, AIR);
        }
    }
}

/// Every block the structures of `making`'s generator build over the
/// ground of `far`, as they leave it, by its place in the world's own
/// blocks: air where they clear the ground. None past the level they still
/// show at, and none far under the ground.
fn structures_over(making: &BlockMaking, far: &FarTile) -> HashMap<[i32; 3], BlockId> {
    let mut sets: HashMap<[i32; 3], BlockId> = HashMap::new();
    let (file, generate) = (&making.file, &making.generate);
    let built = generate.built.as_deref().filter(|built| !built.list.is_empty());
    let Some(built) = built.filter(|_| far.level <= STRUCTURES_TO) else { return sets };
    let biomes = generate.placed.as_deref().map(|placed| &placed.biomes).filter(|biomes| !biomes.is_empty());
    let under = file.ground.under;
    let under = if usize::from(under) <= file.blocks.len() { under } else { AIR };
    let (level, side) = (far.level, CHUNK as i32);
    let (first, end) = (far.at.map(|tile| tile << level), far.at.map(|tile| (tile + 1) << level));
    // The ground's surface under a block, in the world's own blocks.
    let surface = |x: i32, z: i32| {
        let cell = [(x >> level) - far.at[0] * side, (z >> level) - far.at[1] * side];
        let inside = cell.iter().all(|at| (0..side).contains(at));
        inside.then(|| (far.surface(cell.map(|at| at as u32)) * (1u32 << level) as f32).floor() as i32)
    };
    let mut set = |at: [i32; 3], block: BlockId| {
        if surface(at[0], at[2]).is_some_and(|surface| at[1] >= surface - UNDER_SHOWS) {
            sets.insert(at, block);
        }
    };
    let at = (file.block_size.max(0.01), generate.position, generate.bottom);
    super::structure::build_over(&mut set, built, &making.world, biomes, at, (first, end), under);
    sets
}

/// What a structure stands as on one column of the world's own blocks, seen
/// from above: from its lowest block, `bottom`, up to `top`; its top block
/// and how many blocks deep that lies, as a roof or a crown does; and the
/// block most of what is under that is, its walls' or its trunk's, where
/// there is any.
#[derive(Clone, Copy)]
struct Stood {
    bottom: i32,
    top: i32,
    roof: BlockId,
    cap: i32,
    wall: Option<BlockId>,
}

/// The block most of `blocks` are, the lowest number among equals: none of
/// none.
fn most_of(blocks: impl Iterator<Item = BlockId>) -> Option<BlockId> {
    let mut counts: HashMap<BlockId, u32> = HashMap::new();
    blocks.for_each(|block| *counts.entry(block).or_default() += 1);
    counts.into_iter().max_by_key(|&(block, count)| (count, std::cmp::Reverse(block))).map(|(block, _)| block)
}

/// What `built` stands as on each column of blocks it builds on.
fn stood(built: &HashMap<[i32; 3], BlockId>) -> HashMap<[i32; 2], Stood> {
    let mut columns: HashMap<[i32; 2], Vec<(i32, BlockId)>> = HashMap::new();
    for (&[x, y, z], &block) in built.iter().filter(|(_, block)| **block != AIR) {
        columns.entry([x, z]).or_default().push((y, block));
    }
    let seen = |(at, mut blocks): ([i32; 2], Vec<(i32, BlockId)>)| {
        blocks.sort_unstable_by_key(|(y, _)| std::cmp::Reverse(*y));
        let (top, roof) = blocks[0];
        // Its top block as far down as it lies on itself.
        let capped = |(down, (y, block)): &(usize, &(i32, BlockId))| *y == top - *down as i32 && *block == roof;
        let cap = blocks.iter().enumerate().take_while(capped).count();
        let wall = most_of(blocks[cap..].iter().map(|(_, block)| *block));
        (at, Stood { bottom: blocks[blocks.len() - 1].0, top: top + 1, roof, cap: cap as i32, wall })
    };
    columns.into_iter().map(seen).collect()
}

/// Stands what `built` builds on the cells of a far tile coarser than the
/// world's own blocks, as it looks from above. A cell is built up where
/// enough of its columns of blocks are built on: to the height those
/// stand on average, so a roof keeps its slope; their top block on top, as
/// deep as it lies on them, and under it the block of their walls. Where
/// most of them stand clear of the ground, as a crown does, the cell does
/// too. Where what is built lies in the ground's own top, as a road does,
/// the cell's top is its block. So a house is its footprint, its walls
/// and its roof, whole, and not the cells its blocks happen to fill.
fn stand_built(
    (columns, layered): (&mut [Vec<Run>], &mut [bool]),
    built: &HashMap<[i32; 3], BlockId>,
    far: &FarTile,
    faces: &[BlockFaces],
) {
    let (level, side) = (far.level, CHUNK as i32);
    let mut cells: HashMap<[i32; 2], Vec<Stood>> = HashMap::new();
    for ([x, z], on) in stood(built) {
        cells.entry([(x >> level) - far.at[0] * side, (z >> level) - far.at[1] * side]).or_default().push(on);
    }
    // How many of a cell's columns of blocks, of all it has, it takes to build it up: fewer where a house is
    // no larger than a cell, so it is not lost between four.
    let all = 1usize << (2 * level);
    let raised = if level < STRUCTURES_TO { all * 3 / 8 } else { all * 3 / 16 };
    let scale = (1u32 << level) as f32;
    for ([x, z], on) in cells {
        if x < 0 || z < 0 || x >= side || z >= side {
            continue;
        }
        let runs = &mut columns[z as usize * MAP_TILE + x as usize];
        let Some(ground) = ground_top(runs, faces) else { continue };
        let mean = |of: fn(&Stood) -> i32| on.iter().map(|on| of(on) as f32).sum::<f32>() / on.len() as f32;
        let cells_of = |blocks: f32| (blocks / scale).round() as i32;
        let top = cells_of(mean(|on| on.top));
        let roof = most_of(on.iter().map(|on| on.roof)).unwrap_or(AIR);
        if top > ground && on.len() >= raised.max(1) {
            // Clear of the ground where most of it is, by a cell at the least: all of it its top block then.
            let hangs = |on: &&Stood| on.bottom as f32 / scale >= ground as f32 + 1.0;
            if top - ground >= 2 && on.iter().filter(hangs).count() * 2 > on.len() {
                put(runs, cells_of(mean(|on| on.bottom)).clamp(ground + 1, top - 1), top, roof);
                continue;
            }
            let wall = most_of(on.iter().filter_map(|on| on.wall)).unwrap_or(roof);
            let cap = cells_of(mean(|on| on.cap)).clamp(1, top - ground);
            put(runs, ground, top - cap, wall);
            put(runs, top - cap, top, roof);
            layered[z as usize * MAP_TILE + x as usize] = true;
        } else if top <= ground && on.len() * 4 >= all {
            put(runs, ground - 1, ground, roof);
        }
    }
}

/// Where the ground of `column` ends: its highest block that is not water.
fn ground_top(column: &[Run], faces: &[BlockFaces]) -> Option<i32> {
    column.iter().rev().find(|run| !is_water(faces, run.block)).map(|run| run.to)
}

/// Stands each plant `making`'s world grows by `plants` on a far tile's
/// `columns`, where a plant is at least a cell across: a column of its
/// block as tall as it stands, and its crown round that, from two fifths
/// of the way up, as far as it reaches. Left out where `built` or the
/// world's `changes` took its ground or filled the block over it.
fn stand_plants(
    columns: &mut [Vec<Run>],
    (making, far): (&BlockMaking, &FarTile),
    (plants, built, changes): (&[Woods], &HashMap<[i32; 3], BlockId>, &BlockEdits),
) {
    let (level, side) = (far.level, CHUNK as i32);
    let size = making.file.block_size.max(0.01);
    let cell = size * (1u32 << level) as f32;
    let ground = FarGround { file: &making.file, generate: &making.generate, world: &making.world };
    // A crown reaches over a tile's edge: the plants of the chunk columns round the tile's own too.
    let (first, end) = (far.at.map(|tile| (tile << level) - 1), far.at.map(|tile| ((tile + 1) << level) + 1));
    for kind in plants.iter().filter(|kind| kind.width >= cell && kind.block != AIR) {
        let middle = ((kind.rule.scale[0] + kind.rule.scale[1]) * 0.5).max(0.01);
        for column in (first[1]..end[1]).flat_map(|cz| (first[0]..end[0]).map(move |cx| [cx, cz])) {
            let grown = grow_far(&kind.rule, &ground, column, 0);
            let gone = gone_far(&grown, changes);
            for (number, plant) in grown.iter().enumerate() {
                let stands = built.get(&plant.ground).is_none_or(|block| *block == plant.block)
                    && built.get(&plant.over()).is_none_or(|block| *block == AIR);
                if gone.contains(number as u32) || !stands {
                    continue;
                }
                let sized = plant.plant.scale / middle;
                let tall = ((kind.height * sized / cell).round() as i32).max(1);
                let reach = kind.width * sized * 0.5 / cell;
                // Its foot in the map's cells, and where it stands among the tile's.
                let foot = ((plant.ground[1] + 1) as f32 / (1u32 << level) as f32).round() as i32;
                let at = [plant.plant.position[0] / cell, plant.plant.position[2] / cell];
                let own = [at[0].floor() as i32 - far.at[0] * side, at[1].floor() as i32 - far.at[1] * side];
                let around = reach.ceil() as i32;
                for (dx, dz) in (-around..=around).flat_map(|dz| (-around..=around).map(move |dx| (dx, dz))) {
                    let (x, z) = (own[0] + dx, own[1] + dz);
                    if x < 0 || z < 0 || x >= side || z >= side {
                        continue;
                    }
                    let runs = &mut columns[z as usize * MAP_TILE + x as usize];
                    if (dx, dz) == (0, 0) {
                        put(runs, foot, foot + tall, kind.block);
                        continue;
                    }
                    // Its crown over the cells whose middles it reaches, no lower than their own ground.
                    let off =
                        [(far.at[0] * side + x) as f32 + 0.5 - at[0], (far.at[1] * side + z) as f32 + 0.5 - at[1]];
                    if off[0] * off[0] + off[1] * off[1] > reach * reach {
                        continue;
                    }
                    let from = foot + (tall * 2 / 5).max(1);
                    let from = from.max(ground_top(runs, &making.faces).unwrap_or(from));
                    if from < foot + tall {
                        put(runs, from, foot + tall, kind.block);
                    }
                }
            }
        }
    }
}

/// The map tile a far tile makes with what stands on its ground put into
/// it: its generator's structures, the world's `changes`, and the plants
/// it grows.
fn made_tile(far: &FarTile, making: &BlockMaking, (changes, plants): (&BlockEdits, &[Woods])) -> MapTile {
    let (level, side) = (far.level, CHUNK as i32);
    let faces = &making.faces[..];
    let mut columns = runs_of(far);
    // The columns something was built on: their sides show their layers.
    let mut layered = vec![false; columns.len()];
    // What the generator builds on the ground, as the world's own blocks have it: block for block up close. In larger cells, what they clear is dug by its share of a cell, and what they
    // build stands as it looks from above.
    let built = structures_over(making, far);
    let mut cells = Cells::new();
    let counted = built.iter().filter(|(_, block)| level == 0 || **block == AIR);
    counted.for_each(|(at, block)| count(&mut cells, level, *at, *block));
    put_cells((&mut columns, &mut layered), cells, far, faces);
    if level > 0 {
        stand_built((&mut columns, &mut layered), &built, far, faces);
    }
    // The chunks of the world's own blocks this tile's cells lie over, as they were changed.
    let (first, end) = (far.at.map(|tile| tile << level), far.at.map(|tile| (tile + 1) << level));
    let within = changes.chunks.range([first[0], i32::MIN, i32::MIN]..[end[0], i32::MIN, i32::MIN]);
    let within = within.filter(|(key, _)| key[2] >= first[1] && key[2] < end[1]);
    let mut cells = Cells::new();
    for (key, change) in within {
        each_set(change, |local, block| {
            count(&mut cells, level, [0, 1, 2].map(|axis| key[axis] * side + local[axis]), block);
        });
    }
    put_cells((&mut columns, &mut layered), cells, far, faces);
    stand_plants(&mut columns, (making, far), (plants, &built, changes));
    let column = |(runs, layered): (&Vec<Run>, bool)| MapColumn { layered, ..MapColumn::of_runs(runs, faces) };
    let columns = columns.iter().zip(layered).map(column).collect();
    MapTile { level, at: far.at, columns }
}

/// The runs of the column of blocks at `[x, z]` of chunk column `chunks`,
/// its chunks' heights `heights`, lowest first.
fn runs_at(blocks: &BlockMap, [cx, cz]: [i32; 2], heights: &[i32], [x, z]: [u32; 2]) -> Vec<Run> {
    let side = CHUNK as i32;
    let mut runs: Vec<Run> = Vec::new();
    let mut add = |from: i32, to: i32, block: BlockId| match runs.last_mut() {
        _ if block == AIR => {}
        Some(last) if last.to == from && last.block == block => last.to = to,
        _ => runs.push(Run { from, to, block }),
    };
    for &cy in heights {
        let Some(chunk) = blocks.chunk([cx, cy, cz]) else { continue };
        if one_block(chunk) {
            add(cy * side, (cy + 1) * side, chunk.get(0, 0, 0));
            continue;
        }
        for y in 0..CHUNK {
            add(cy * side + y as i32, cy * side + y as i32 + 1, chunk.get(x, y, z));
        }
    }
    runs
}

/// Whether a chunk is one block all through.
fn one_block(chunk: &Chunk) -> bool {
    chunk.parts().1 == 0
}

/// A held world's tile: each cell the column of blocks at its middle, its
/// heights rounded to the level's cells.
fn held_tile(blocks: &BlockMap, stands: &ChunkColumns, faces: &[BlockFaces], level: u32, at: [i32; 2]) -> MapTile {
    let (side, scale) = (CHUNK as i32, 1i32 << level);
    let mut columns = Vec::with_capacity(MAP_TILE * MAP_TILE);
    for z in 0..side {
        for x in 0..side {
            let block = [at[0] * side + x, at[1] * side + z].map(|cell| cell * scale + scale / 2);
            let chunk = block.map(|v| v.div_euclid(side));
            let Some(heights) = stands.heights.get(&chunk) else {
                columns.push(MapColumn::NONE);
                continue;
            };
            let local = block.map(|v| v.rem_euclid(side) as u32);
            let runs = runs_at(blocks, chunk, heights, local);
            columns.push(MapColumn::of_runs(&coarser(runs, scale, faces), faces));
        }
    }
    MapTile { level, at, columns }
}

/// Runs of blocks as runs of cells `scale` blocks high. Each stretch of
/// ground, and each of water, has its ends rounded to the nearest cell, and
/// each of its cells is the block highest in it: so a top block one block
/// thick is still the top. A stretch rounded to nothing is left out.
fn coarser(runs: Vec<Run>, scale: i32, faces: &[BlockFaces]) -> Vec<Run> {
    if scale == 1 {
        return runs;
    }
    let cell = |y: i32| (y + scale / 2).div_euclid(scale);
    let mut coarse: Vec<Run> = Vec::with_capacity(runs.len());
    let mut at = 0;
    while at < runs.len() {
        let first = at;
        let water = is_water(faces, runs[at].block);
        while at + 1 < runs.len() && runs[at + 1].from == runs[at].to && is_water(faces, runs[at + 1].block) == water {
            at += 1;
        }
        let stretch = &runs[first..=at];
        let (low, high) = (stretch[0].from, stretch[stretch.len() - 1].to);
        for y in cell(low)..cell(high) {
            // The block highest in the cell, within the stretch.
            let highest = ((y + 1) * scale).clamp(low + 1, high) - 1;
            let block = stretch.iter().find(|run| run.from <= highest && highest < run.to).map_or(AIR, |run| run.block);
            match coarse.last_mut() {
                Some(last) if last.to == y && last.block == block => last.to = y + 1,
                _ => coarse.push(Run { from: y, to: y + 1, block }),
            }
        }
        at += 1;
    }
    coarse
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::{
            chunk::FULL,
            file::{BlockFile, BlockKind, Ground},
            generate::{BlockGenerate, far::far_tile},
        },
        generate::Preset,
    };

    const STONE: BlockId = 1;
    const DIRT: BlockId = 2;
    const GRASS: BlockId = 3;
    const SEA: BlockId = 4;
    const WOOD: BlockId = 5;
    const LEAF: BlockId = 6;

    fn faces() -> Vec<BlockFaces> {
        let mut faces = vec![BlockFaces::default(); 7];
        faces[usize::from(SEA)] = BlockFaces::water(0);
        faces
    }

    fn run(from: i32, to: i32, block: BlockId) -> Run {
        Run { from, to, block }
    }

    #[test]
    fn a_column_is_its_ground_with_two_layers_under_its_top_and_the_water_over_it() {
        let runs = [run(-40, 6, STONE), run(6, 9, DIRT), run(9, 10, GRASS)];
        let column = MapColumn::of_runs(&runs, &faces());
        assert_eq!((column.top, column.water), (10, 10), "no water: its water ends where its ground does");
        assert_eq!((column.blocks, column.depths), ([GRASS, DIRT, STONE], [1, 3]));
        assert_eq!(column.highest(), 10);

        let under_sea = [run(-40, 2, STONE), run(2, 4, DIRT), run(4, 12, SEA)];
        let column = MapColumn::of_runs(&under_sea, &faces());
        assert_eq!((column.top, column.water, column.water_block), (4, 12, SEA));
        assert_eq!(column.blocks, [DIRT, STONE, STONE], "with no third layer, the second runs on");
        assert_eq!(column.highest(), 12);

        assert_eq!(MapColumn::of_runs(&[], &faces()), MapColumn::NONE);
        assert!(!MapColumn::NONE.is_some());
        let sea_alone = MapColumn::of_runs(&[run(-8, 3, SEA)], &faces());
        assert_eq!((sea_alone.top, sea_alone.water, sea_alone.blocks[0]), (-8, 3, SEA), "a sea with no floor made");
    }

    #[test]
    fn what_stands_free_of_the_ground_is_a_piece_and_thick_ground_over_a_cave_is_ground() {
        // A trunk's column is ground all the way up; the crown beside it stands free.
        let crown = [run(-40, 9, STONE), run(9, 10, GRASS), run(14, 17, LEAF)];
        let column = MapColumn::of_runs(&crown, &faces());
        assert_eq!(column.top, 10);
        assert_eq!(column.pieces[0], MapPiece { from: 14, to: 17, top: LEAF, side: LEAF });
        assert!(!column.pieces[1].is_some());
        assert_eq!(column.highest(), 17);

        let trunk = [run(-40, 9, STONE), run(9, 10, GRASS), run(10, 14, WOOD), run(14, 17, LEAF)];
        let column = MapColumn::of_runs(&trunk, &faces());
        assert_eq!((column.top, column.blocks, column.depths), (17, [LEAF, WOOD, GRASS], [3, 4]));

        // A roof of wood under leaves is one piece: wood most of its sides, leaves its top.
        let roof = [run(0, 4, STONE), run(8, 11, WOOD), run(11, 12, LEAF)];
        assert_eq!(MapColumn::of_runs(&roof, &faces()).pieces[0], MapPiece { from: 8, to: 12, top: LEAF, side: WOOD });

        // Twenty cells of ground over a cave are the ground, the cave left out; a bridge over them is a piece.
        let cave = [run(-40, -10, STONE), run(-4, 15, STONE), run(15, 16, GRASS), run(20, 21, WOOD)];
        let column = MapColumn::of_runs(&cave, &faces());
        assert_eq!((column.top, column.blocks), (16, [GRASS, STONE, STONE]));
        assert_eq!(column.pieces[0], MapPiece { from: 20, to: 21, top: WOOD, side: WOOD });

        // Of three pieces the two highest are kept.
        let three = [run(0, 4, STONE), run(6, 7, WOOD), run(9, 10, WOOD), run(12, 13, LEAF)];
        let column = MapColumn::of_runs(&three, &faces());
        assert_eq!((column.pieces[0].from, column.pieces[1].from), (9, 12));
    }

    fn making() -> BlockMaking {
        let kind = |name: &str| BlockKind::colored(name, [0.5; 3]);
        let file = BlockFile {
            block_size: 1.0,
            blocks: vec![kind("Stone"), kind("Dirt"), kind("Grass"), BlockKind { water: true, ..kind("Sea") }],
            ground: Ground { size: [0, 0], top: GRASS, under: DIRT, bottom: STONE, ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("hills".into()),
                caves: 0.0,
                bottom: -32,
                sea: SEA,
                ..BlockGenerate::default()
            }),
            ..BlockFile::default()
        };
        let (_, faces) = file.faces();
        let generate = file.generate.clone().unwrap();
        let world = Arc::new(Preset::RollingHills.world());
        BlockMaking { file, generate, world, faces: faces.into(), far_folder: None, far_built: Vec::new() }
    }

    #[test]
    fn a_world_without_ends_tile_is_the_far_views_ground_at_any_level() {
        let making = making();
        let ground = MapGround::made(making.clone(), Arc::new(BlockEdits::default()));
        for level in [0, 2, 5] {
            let tile = map_tile(&ground, level, [1, -2]);
            let far = far_tile(&making.file, &making.generate, &making.world, level, [1, -2]);
            assert_eq!((tile.level, tile.at, tile.columns.len()), (level, [1, -2], MAP_TILE * MAP_TILE));
            for at in [[0, 0], [31, 0], [7, 19], [31, 31]] {
                let (column, runs) = (tile.column(at), far.column(at));
                let top = runs.iter().rev().find(|run| !is_water(&making.faces, run.block)).unwrap();
                assert_eq!((column.top, column.blocks[0]), (top.to, top.block), "level {level}, column {at:?}");
                assert_eq!(column.highest(), runs.last().unwrap().to);
                assert!(!column.pieces[0].is_some(), "plain ground has nothing standing free");
            }
            let [low, high] = tile.heights().unwrap();
            assert!(low < high, "hills are not flat: {low}..{high}");
        }
    }

    /// [`making`] with wood and leaves among its blocks, and a generator that builds huts of wood, five blocks
    /// a side and four high, solid, their top blocks `roof`, one to a square of forty metres.
    fn making_huts(roof: BlockId) -> BlockMaking {
        use crate::{
            blocks::structure::{GroundFit, PlacedStructure, StructureBlocks, WorldStructures},
            generate::structures::Placement,
        };
        let mut making = making();
        let kind = |name: &str| BlockKind::colored(name, [0.5; 3]);
        making.file.blocks.extend([kind("Wood"), kind("Leaf")]);
        making.faces = making.file.faces().1.into();
        let hut = PlacedStructure {
            placement: Placement { spacing: 40.0, separation: 12.0, chance: 1.0, slope: 80.0, ..Placement::default() },
            ground: GroundFit::Foundation,
            sink: 0,
            blocks: StructureBlocks::Model { size: [5, 4, 5], blocks: [vec![WOOD; 75], vec![roof; 25]].concat() },
            biomes: Vec::new(),
        };
        making.generate.built = Some(Arc::new(WorldStructures { list: vec![hut] }));
        making
    }

    #[test]
    fn what_a_worlds_generator_builds_on_its_ground_stands_on_its_map() {
        let making = making_huts(WOOD);
        let bare = BlockMaking { generate: BlockGenerate { built: None, ..making.generate.clone() }, ..making.clone() };
        let plain = map_tile(&MapGround::made(bare, Arc::default()), 0, [0, 0]);
        let tile = map_tile(&MapGround::made(making.clone(), Arc::default()), 0, [0, 0]);
        // Up close a hut is its own blocks: columns of wood four blocks high, on the ground or a foundation.
        let places = || (0..MAP_TILE as u32).flat_map(|z| (0..MAP_TILE as u32).map(move |x| [x, z]));
        let huts: Vec<[u32; 2]> = places().filter(|at| tile.column(*at).blocks[0] == WOOD).collect();
        assert!(!huts.is_empty() && huts.len() % 5 == 0, "{} columns of huts in the tile", huts.len());
        for at in &huts {
            let (hut, ground) = (tile.column(*at), plain.column(*at));
            assert!(hut.depths[0] == 4 && hut.top >= ground.top + 4, "{at:?}: {hut:?} on {ground:?}");
        }
        let bare_ground = places().filter(|at| !huts.contains(at));
        assert!(bare_ground.take(200).all(|at| tile.column(at) == plain.column(at)), "ground beside them is as it was");
        // Its changes go in over what the generator built: a hut's top dug off is a block lower.
        let roof = [huts[0][0] as i32, tile.column(huts[0]).top - 1, huts[0][1] as i32];
        let dug = map_tile(&MapGround::made(making.clone(), changes([(roof, AIR)])), 0, [0, 0]);
        assert_eq!(dug.column(huts[0]).top, tile.column(huts[0]).top - 1);
        // From further off a hut is the cells an eighth of whose blocks it fills, four at the most: a tile
        // there is sixteen of these, with a hut to each forty metres. In cells of sixteen blocks a hut is too
        // small a part of any, and nothing is built at all.
        let wood = |level: u32| {
            let tile = map_tile(&MapGround::made(making.clone(), Arc::default()), level, [0, 0]);
            tile.columns.iter().filter(|column| column.blocks[0] == WOOD).count()
        };
        assert!((1..=64).contains(&wood(2)), "{} cells of huts at four blocks a cell", wood(2));
        assert_eq!(wood(STRUCTURES_TO + 1), 0);
    }

    #[test]
    fn from_further_off_a_house_is_its_footprint_with_its_roof_over_its_walls() {
        // Huts of wood roofed with leaves, a block deep.
        let making = making_huts(LEAF);
        let ground = MapGround::made(making, Arc::default());
        let roofed = |level: u32| -> Vec<MapColumn> {
            let tile = map_tile(&ground, level, [0, 0]);
            tile.columns.iter().copied().filter(|column| column.blocks[0] == LEAF).collect()
        };
        // Block for block up close: the roof a block deep over three of wall.
        let near = roofed(0);
        assert!(!near.is_empty() && near.len() % 5 == 0, "{} columns of huts", near.len());
        assert!(near.iter().all(|hut| hut.blocks[1] == WOOD && hut.depths[0] == 1 && hut.layered), "{:?}", near[0]);
        // In cells of two blocks and of four, a hut is whole cells: its roof a cell deep over its walls, none
        // of it standing free, its sides drawn as those layers are. Not the cells its blocks happen to fill,
        // of whichever block was counted last.
        for level in [1, 2] {
            let huts = roofed(level);
            assert!(!huts.is_empty(), "no hut in cells of {} blocks", 1 << level);
            for hut in &huts {
                assert!(hut.layered && hut.depths[0] == 1 && !hut.pieces[0].is_some(), "level {level}: {hut:?}");
                assert!(hut.blocks[1] == WOOD || level == 2, "level {level}: walls under the roof: {hut:?}");
            }
        }
        // What a structure stands as on a column of blocks, seen from above: a roof two deep over a hollow
        // room has the floor's block for its walls, and stands from the floor up; a crown beside a trunk hangs.
        let room = [([0, 10, 0], WOOD), ([0, 14, 0], LEAF), ([0, 15, 0], LEAF), ([3, 14, 0], LEAF)];
        let seen = stood(&room.into_iter().collect());
        let house = seen[&[0, 0]];
        assert_eq!((house.bottom, house.top, house.roof, house.cap, house.wall), (10, 16, LEAF, 2, Some(WOOD)));
        let crown = seen[&[3, 0]];
        assert_eq!((crown.bottom, crown.top, crown.cap, crown.wall), (14, 15, 1, None));
    }

    #[test]
    fn the_plants_a_world_grows_stand_on_its_map_each_where_it_grows_and_as_woods_from_further_off() {
        use crate::blocks::grow::GrowRule;
        let making = making_huts(WOOD);
        let making = BlockMaking { generate: BlockGenerate { built: None, ..making.generate.clone() }, ..making };
        let rule = GrowRule {
            spacing: 10.0,
            separation: 4.0,
            chance: 1.0,
            slope: 80.0,
            scale: [1.0; 2],
            ..GrowRule::default()
        };
        // Trees six metres tall and five across, drawn as leaves.
        let trees = Woods { rule: rule.clone(), block: LEAF, height: 6.0, width: 5.0, from: 1 };
        let grows = |edits: Arc<BlockEdits>| MapGround::made(making.clone(), edits).growing(vec![trees.clone()]);
        let plain = map_tile(&MapGround::made(making.clone(), Arc::default()), 0, [0, 0]);
        let tile = map_tile(&grows(Arc::default()), 0, [0, 0]);
        let far = FarGround { file: &making.file, generate: &making.generate, world: &making.world };
        let grown = grow_far(&rule, &far, [0, 0], 0);
        assert!(grown.len() >= 4, "{} trees grow on the tile", grown.len());
        for tree in &grown {
            // Its trunk's column is leaves as tall as it stands; the column beside it has its crown over it.
            let [x, y, z] = tree.ground;
            let trunk = tile.column([x as u32, z as u32]);
            assert!(trunk.blocks[0] == LEAF && trunk.top >= y + 7, "the tree on {:?}: {trunk:?}", tree.ground);
            if x + 1 < MAP_TILE as i32 {
                let beside = tile.column([x as u32 + 1, z as u32]);
                assert!(beside.highest() >= y + 7 || beside.top > y + 3, "beside {:?}: {beside:?}", tree.ground);
            }
        }
        // A tree whose ground was dug away is gone from the map, as it is from the world.
        let [x, y, z] = grown[0].ground;
        let dug = map_tile(&grows(changes([([x, y, z], AIR)])), 0, [0, 0]);
        let stump = dug.column([x as u32, z as u32]);
        assert!(stump.blocks[0] != LEAF && stump.top < plain.column([x as u32, z as u32]).top, "{stump:?}");
        // In cells larger than a tree they show as the far view's woods do: leaves over the ground they grow on.
        let leaves = |tile: &MapTile| tile.columns.iter().filter(|column| column.blocks[0] == LEAF).count();
        let coarse = map_tile(&grows(Arc::default()), 3, [0, 0]);
        assert!(leaves(&coarse) > 20, "{} cells of wood at eight metres a cell", leaves(&coarse));
        // A world that grows nothing shows none.
        assert_eq!(leaves(&plain), 0);
    }

    /// Changes that set each of `cells` of level 0.
    fn changes(cells: impl IntoIterator<Item = ([i32; 3], BlockId)>) -> Arc<BlockEdits> {
        let fill = |block| if block == AIR { 0 } else { FULL };
        Arc::new(BlockEdits::of_cells(cells.into_iter().map(|(at, block)| (at, block, fill(block)))))
    }

    #[test]
    fn what_was_built_and_dug_shows_block_for_block_up_close() {
        let making = making();
        let plain = map_tile(&MapGround::made(making.clone(), Arc::default()), 0, [0, 0]);
        let top = plain.column([5, 5]).top;
        // A tower four blocks up, a pit three down beside it, and a slab hung in the air.
        let tower = (0..4).map(|y| ([5, top + y, 5], STONE));
        let pit = (1..=3).map(|y| ([6, plain.column([6, 5]).top - y, 5], AIR));
        let slab = [([9, plain.column([9, 9]).top + 6, 9], DIRT)];
        let built = changes(tower.chain(pit).chain(slab));
        let tile = map_tile(&MapGround::made(making, built), 0, [0, 0]);

        let column = tile.column([5, 5]);
        assert_eq!((column.top, column.blocks[0], column.depths[0]), (top + 4, STONE, 4));
        assert_eq!(tile.column([6, 5]).top, plain.column([6, 5]).top - 3, "the pit is as deep as it was dug");
        let hung = tile.column([9, 9]);
        assert_eq!(hung.top, plain.column([9, 9]).top);
        assert_eq!(hung.pieces[0], MapPiece { from: hung.top + 6, to: hung.top + 7, top: DIRT, side: DIRT });
        assert_eq!(tile.column([20, 20]), plain.column([20, 20]), "ground not changed is as the generator made it");
    }

    #[test]
    fn from_far_off_a_cell_is_built_where_an_eighth_of_it_was_and_dug_where_over_half_was() {
        let making = making();
        let plain = map_tile(&MapGround::made(making.clone(), Arc::default()), 2, [0, 0]);
        // Level 2: a cell is 4 blocks a side, 64 blocks. Cell [3, _, 3] gets 8 blocks built over it (an eighth),
        // cell [5, _, 5] gets 7 (too few).
        let over = |cell: [i32; 2]| plain.column([cell[0] as u32, cell[1] as u32]).top;
        let stack = |cell: [i32; 2], count: i32| {
            let y = over(cell) * 4;
            (0..count).map(move |at| ([cell[0] * 4 + at % 4, y + at / 4, cell[1] * 4], STONE))
        };
        // Cell [8, _, 8]'s top cell loses 33 of its 64 blocks (over half); cell [10, _, 10]'s loses 32.
        let dig = |cell: [i32; 2], count: i32| {
            let y = (over(cell) - 1) * 4;
            (0..count).map(move |at| ([cell[0] * 4 + at % 4, y + at / 16, cell[1] * 4 + (at / 4) % 4], AIR))
        };
        let built = changes(stack([3, 3], 8).chain(stack([5, 5], 7)).chain(dig([8, 8], 33)).chain(dig([10, 10], 32)));
        let tile = map_tile(&MapGround::made(making, built), 2, [0, 0]);
        assert_eq!((tile.column([3, 3]).top, tile.column([3, 3]).blocks[0]), (over([3, 3]) + 1, STONE));
        assert_eq!(tile.column([5, 5]), plain.column([5, 5]), "seven blocks of sixty-four do not show");
        assert_eq!(tile.column([8, 8]).top, over([8, 8]) - 1);
        assert_eq!(tile.column([10, 10]), plain.column([10, 10]), "half a cell dug is still there");
    }

    #[test]
    fn a_held_worlds_tile_is_read_from_its_blocks_and_is_empty_past_its_edge() {
        let mut blocks = BlockMap::default();
        // Ground 40 by 40 blocks: stone to 3, dirt to 5, grass on top; a bridge of wood 4 over it along x.
        for z in 0..40 {
            for x in 0..40 {
                for y in -8..6 {
                    let block = if y == 5 {
                        GRASS
                    } else if y >= 3 {
                        DIRT
                    } else {
                        STONE
                    };
                    blocks.set([x, y, z], block);
                }
            }
        }
        (0..40).for_each(|x| blocks.set([x, 10, 7], WOOD));
        let ground = MapGround::held(Arc::new(blocks), faces().into());

        let tile = map_tile(&ground, 0, [0, 0]);
        let column = tile.column([4, 4]);
        assert_eq!((column.top, column.blocks, column.depths), (6, [GRASS, DIRT, STONE], [1, 2]));
        assert_eq!(tile.column([4, 7]).pieces[0], MapPiece { from: 10, to: 11, top: WOOD, side: WOOD });
        assert_eq!(tile.heights(), Some([6, 11]));

        // The next tile along x holds the ground's last 8 columns, then nothing.
        let next = map_tile(&ground, 0, [1, 0]);
        assert!(next.column([7, 0]).is_some() && !next.column([8, 0]).is_some());
        assert_eq!(map_tile(&ground, 0, [5, 5]).heights(), None);

        // Twice as coarse, a cell is the column at its middle, heights halved.
        let coarse = map_tile(&ground, 1, [0, 0]);
        let column = coarse.column([2, 2]);
        assert_eq!((column.top, column.blocks[0]), (3, GRASS));
        assert!(coarse.column([19, 0]).is_some() && !coarse.column([20, 0]).is_some());
    }

    #[test]
    #[ignore = "prints how long tiles take; run by hand"]
    fn tile_time() {
        let ground = MapGround::made(making(), Arc::default());
        for level in [0, 2, 4] {
            let start = std::time::Instant::now();
            let tiles: Vec<MapTile> = (0..16).map(|at| map_tile(&ground, level, [at % 4, at / 4])).collect();
            println!("level {level}: {:.2} ms a tile", start.elapsed().as_secs_f64() * 1000.0 / tiles.len() as f64);
        }
    }
}
