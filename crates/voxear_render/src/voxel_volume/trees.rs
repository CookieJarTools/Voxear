//! The trees of an endless Voxel Volume ([`VoxelTrees`]), as the GPU reads
//! them (`shaders/voxel_world.slang`).
//!
//! Each kind of tree is grown from a few seeds on the CPU, and each plant
//! grown becomes a grid of its blocks, a byte a block: a log, which way it
//! lies and how thick it is; leaves; or nothing. Each plant's voxels are
//! made from its grid once ([`models`]), its leaves rounded into clumps of
//! which only a thin shell is filled, in each of four turns. The trees are
//! placed on the GPU, at most one in each [`CELL`] metres square, by hash,
//! each plant's box on the grid of the volume's bricks: a brick of the
//! world that is one tree's alone is its plant's brick, named, and only
//! where trees cross or stand in the grass does a column keep bricks of
//! its own.

use std::collections::{HashMap, HashSet};

use voxear_terrain::blocks::{plant::PlantPart, shape::BlockFacing};

use super::{VoxelTrees, bricks};

/// How many plants a kind of tree is grown as, each turned four ways.
pub(crate) const VARIANTS: u32 = 4;
/// The most kinds of tree an endless volume scatters.
pub(crate) const MOST_KINDS: usize = 8;
/// How many metres a side the squares are that hold a tree each at most.
pub(crate) const CELL: f32 = 5.0;
/// How many colours a kind of tree has: its wood in two shades, its leaves
/// in three.
pub(crate) const COLORS_A_KIND: usize = 5;
/// Where a kind's record starts, and a plant's.
const KINDS_AT: usize = 4;
const VARIANTS_AT: usize = KINDS_AT + MOST_KINDS * 4;
const VARIANT_WORDS: usize = 8;

/// A block's byte: a log, with which way it lies in its two lowest bits
/// (X, Y, Z) and how thick it is over them, in eighths of a block.
const WOOD: u8 = 0x80;
/// Leaves; or with [`WOOD`], a log beside leaves, which the crown's leaves
/// round over as if it were leaves, so a branch inside a crown leaves no
/// hollow in it.
const LEAVES: u8 = 0x40;

/// An endless volume's trees as the GPU reads them, and their colours.
pub(crate) struct Forest {
    /// Its kinds (each its chance in a cell, its first plant, how many and
    /// its first colour), its plants (each the least corner of its grid in
    /// blocks from the foot of its trunk, its size, where its bytes start
    /// and how far it reaches across from the middle of its trunk) and their
    /// bytes, four a word.
    pub words: Vec<u32>,
    pub colors: Vec<[u8; 4]>,
    /// How far the furthest of its plants reaches across from the middle of
    /// its trunk, in blocks.
    pub reach: f32,
}

/// The trees as the GPU reads them, `first_color` the palette's place of
/// the first of their colours: none for none.
pub(crate) fn forest(trees: &[VoxelTrees], first_color: u32) -> Option<Forest> {
    let trees: Vec<&VoxelTrees> = trees.iter().filter(|kind| kind.per_hectare > 0.0).take(MOST_KINDS).collect();
    if trees.is_empty() {
        return None;
    }
    let mut words = vec![0u32; VARIANTS_AT + trees.len() * VARIANTS as usize * VARIANT_WORDS];
    let mut bytes: Vec<u8> = Vec::new();
    let mut colors = Vec::new();
    let mut reach: f32 = 0.0;
    let mut chance = 0.0;
    words[0] = trees.len() as u32;
    for (k, kind) in trees.iter().enumerate() {
        chance = (chance + kind.per_hectare * CELL * CELL / 10_000.0).min(1.0);
        let first = (k as u32) * VARIANTS;
        let color = first_color + (k * COLORS_A_KIND) as u32;
        words[KINDS_AT + k * 4..KINDS_AT + k * 4 + 4].copy_from_slice(&[chance.to_bits(), first, VARIANTS, color]);
        let shade = |c: [u8; 3], tone: f32| {
            let [r, g, b] = c.map(|part| (f32::from(part) * tone).round().clamp(0.0, 255.0) as u8);
            [r, g, b, 255]
        };
        colors.extend([0.82, 1.0].map(|tone| shade(kind.wood, tone)));
        colors.extend([0.8, 1.0, 1.18].map(|tone| shade(kind.leaves, tone)));
        for v in 0..VARIANTS {
            let model = kind.plant.grow(0x51ED_0000 + v * 7919 + k as u32 * 104_729);
            let Some((low, high)) = model.bounds() else { continue };
            let size = [0, 1, 2].map(|a| (high[a] - low[a] + 1) as u32);
            let mut parts: HashMap<[i32; 3], u8> = HashMap::new();
            for &(at, part, state) in &model.blocks {
                let byte = match part {
                    PlantPart::Leaf => LEAVES,
                    PlantPart::Vine => continue,
                    _ => {
                        let axis = match state.facing.axis() {
                            BlockFacing::East => 0,
                            BlockFacing::Up => 1,
                            _ => 2,
                        };
                        let thick = if state.size == 0 { 8 } else { state.size.div_ceil(2).min(8) };
                        WOOD | axis | (thick << 2)
                    }
                };
                parts.entry(at).or_insert(byte);
            }
            let start = bytes.len();
            bytes.resize(start + (size[0] * size[1] * size[2]) as usize, 0);
            for (&at, &byte) in &parts {
                let sides = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
                let leafy = sides.iter().any(|side| parts.get(&[0, 1, 2].map(|a| at[a] + side[a])) == Some(&LEAVES));
                let byte = if byte & WOOD != 0 && leafy { byte | LEAVES } else { byte };
                let [x, y, z] = [0, 1, 2].map(|a| (at[a] - low[a]) as u32);
                bytes[start + (x + size[0] * (y + size[1] * z)) as usize] = byte;
            }
            let across = [low[0], high[0] + 1, low[2], high[2] + 1].iter().map(|&b| b.abs()).max().unwrap_or(0);
            let plant_reach = across as f32 + 0.5;
            reach = reach.max(plant_reach);
            let at = VARIANTS_AT + (first + v) as usize * VARIANT_WORDS;
            words[at..at + VARIANT_WORDS].copy_from_slice(&[
                low[0] as u32,
                low[1] as u32,
                low[2] as u32,
                size[0],
                size[1],
                size[2],
                start as u32,
                plant_reach.to_bits(),
            ]);
        }
    }
    // The bytes after the records, four a word, each plant's start counted from the first of them.
    let base = words.len() as u32 * 4;
    for v in 0..trees.len() * VARIANTS as usize {
        words[VARIANTS_AT + v * VARIANT_WORDS + 6] += base;
    }
    bytes.resize(bytes.len().div_ceil(4) * 4, 0);
    words.extend(bytes.chunks(4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])));
    Some(Forest { words, colors, reach })
}

/// Where the models' records start in their words, and the words each takes.
pub(crate) const MODELS_AT: usize = 4;
pub(crate) const MODEL_WORDS: usize = 8;

/// Each plant of a forest as voxels, in each of its four turns: what the
/// passes making columns read of them, and their bricks and materials, which
/// go at the start of the volume's own.
pub(crate) struct Models {
    /// How many models there are (four a plant, a plant's turns one after
    /// another) and how many voxels a block is; for each its sectors a side,
    /// where its sectors start among the words, the least corner of its box
    /// in voxels from the least corner of its trunk's foot block as it is
    /// turned, and the size of its box; then their sectors, whose first
    /// bricks are their places among all the models' bricks.
    pub words: Vec<u32>,
    pub bricks: Vec<u32>,
    pub materials: Vec<u8>,
}

/// The voxels of each plant of `forest`, `block` voxels a block, turned each
/// of four ways. A plant's box is its grid and a block round it, which its
/// leaves round over into.
pub(crate) fn models(forest: &Forest, block: u32, palette: &[[u8; 4]]) -> Models {
    let plants = (forest.words[0] * VARIANTS) as usize;
    let b = block.max(1) as i32;
    let mut words = vec![0u32; MODELS_AT + plants * 4 * MODEL_WORDS];
    words[0] = (plants * 4) as u32;
    words[1] = b as u32;
    let bytes: Vec<u8> = forest.words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let made: Vec<Vec<(bricks::Packed, [i32; 3])>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..plants)
            .map(|plant| {
                let bytes = &bytes;
                let words = &forest.words;
                scope.spawn(move || {
                    let colours = words[KINDS_AT + plant / VARIANTS as usize * 4 + 3];
                    let turns = plant_turns(words, bytes, plant, b, colours);
                    turns.into_iter().map(|(turned, low, size)| (turned.pack(size.map(|a| a as u32), palette), low)).collect()
                })
            })
            .collect();
        jobs.into_iter().map(|job| job.join().expect("a plant's voxels")).collect()
    });
    let mut all_bricks = Vec::new();
    let mut materials: Vec<u8> = Vec::new();
    for (plant, turns) in made.into_iter().enumerate() {
        // Its wood's two colours: a brick with none of them sways in the wind.
        let wood = forest.words[KINDS_AT + plant / VARIANTS as usize * 4 + 3];
        for (turn, (packed, low)) in turns.into_iter().enumerate() {
            let first = (all_bricks.len() / bricks::BRICK_WORDS) as u32;
            let sectors = words.len() as u32;
            for sector in packed.sectors.chunks(bricks::SECTOR_WORDS) {
                let mut sector = sector.to_vec();
                if sector[24] != bricks::EMPTY {
                    sector[24] += first;
                }
                words.extend_from_slice(&sector);
            }
            for brick in packed.bricks.chunks(bricks::BRICK_WORDS) {
                let mut brick = brick.to_vec();
                let count: u32 = brick[..16].iter().map(|word| word.count_ones()).sum();
                let own = &packed.materials[brick[18] as usize..(brick[18] + count) as usize];
                if own.iter().all(|&m| u32::from(m) != wood && u32::from(m) != wood + 1) {
                    brick[19] |= bricks::SWAY_LEAVES;
                }
                brick[18] += materials.len() as u32;
                bricks::mark_leafy(&mut brick, (all_bricks.len() / bricks::BRICK_WORDS) as u32);
                all_bricks.extend_from_slice(&brick);
            }
            materials.extend_from_slice(&packed.materials);
            let count = packed.sector_count.map(|a| a as u32);
            let at = MODELS_AT + (plant * 4 + turn) * MODEL_WORDS;
            words[at..at + MODEL_WORDS].copy_from_slice(&[
                count[0],
                count[1],
                count[2],
                sectors,
                low[0] as u32,
                low[1] as u32,
                low[2] as u32,
                0,
            ]);
        }
    }
    Models { words, bricks: all_bricks, materials }
}

/// Plant `plant` of a forest's `words` (and those words as `bytes`) as
/// voxels, `b` a block, its colours from palette place `colours`, in each
/// of four turns: the voxels, and the least corner and size of their box in
/// voxels from the least corner of the trunk's foot block as turned. None
/// for a plant with nothing in it.
fn plant_turns(
    words: &[u32],
    bytes: &[u8],
    plant: usize,
    b: i32,
    colours: u32,
) -> Vec<(bricks::Builder, [i32; 3], [i32; 3])> {
    let record = &words[VARIANTS_AT + plant * VARIANT_WORDS..][..VARIANT_WORDS];
    let grid = Grid {
        low: [0, 1, 2].map(|a| record[a] as i32),
        size: [3, 4, 5].map(|a| record[a] as i32),
        bytes: &bytes[record[6] as usize..],
    };
    if grid.size.iter().any(|&a| a <= 0) {
        return Vec::new();
    }
    let (voxels, low, size) = grid.voxels(b, colours, plant as u32);
    (0..4).map(|turns| turned(&voxels, low, size, b, turns)).collect()
}

/// A forest's trees as the CPU finds them, to collide with and to meet with
/// rays: each kind's chance in a cell, first plant and how many plants; and
/// each plant's voxels in each of its four turns (a plant's turns one after
/// another), as the GPU has them.
pub(crate) struct Woods {
    kinds: Vec<(f32, u32, u32)>,
    models: Vec<Model>,
    /// How far the furthest plant reaches across from the middle of its
    /// trunk, in metres, with the block its leaves round over and the brick
    /// a tree stands aside to keep to the grid.
    pub reach: f32,
    /// How many voxels a block is.
    pub block: i32,
}

/// A plant turned, as voxels: 1 and 2 its wood, 3 to 5 its leaves, from
/// the least corner of their box; that corner from the least corner of its
/// trunk's foot block; and the box's size.
struct Model {
    voxels: bricks::Builder,
    low: [i32; 3],
    size: [i32; 3],
    /// Its logs' voxels, from the least corner of its box.
    logs: Vec<[i32; 3]>,
}

/// What a voxel is to the CPU: open air (or grass and flowers, which
/// nothing meets), the ground, a tree's wood, or its leaves.
pub(crate) const AIR: u8 = 0;
pub(crate) const GROUND: u8 = 1;
pub(crate) const LOG: u8 = 2;
pub(crate) const LEAF: u8 = 3;

/// A tree placed: where its plant's box's least corner is, in voxels from
/// the volume's corner, on the bricks' grid; and which model it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tree {
    pub least: [i32; 3],
    pub model: usize,
}

impl Woods {
    /// The trees of `trees`, `block` voxels a block: none for none.
    pub fn new(trees: &[VoxelTrees], block: u32) -> Option<Self> {
        let forest = forest(trees, 0)?;
        let b = block.max(1) as i32;
        let bytes: Vec<u8> = forest.words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let kinds = (0..forest.words[0] as usize)
            .map(|k| {
                let record = &forest.words[KINDS_AT + k * 4..][..4];
                (f32::from_bits(record[0]), record[1], record[2])
            })
            .collect();
        let plants = (forest.words[0] * VARIANTS) as usize;
        let made: Vec<Vec<(bricks::Builder, [i32; 3], [i32; 3])>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..plants)
                .map(|plant| {
                    let (words, bytes) = (&forest.words, &bytes);
                    scope.spawn(move || plant_turns(words, bytes, plant, b, 0))
                })
                .collect();
            jobs.into_iter().map(|job| job.join().expect("a plant's voxels")).collect()
        });
        let mut models = Vec::with_capacity(plants * 4);
        for turns in made {
            let empty = || Model { voxels: bricks::Builder::default(), low: [0; 3], size: [0; 3], logs: Vec::new() };
            let model = |(voxels, low, size): (bricks::Builder, [i32; 3], [i32; 3])| {
                let mut logs = Vec::new();
                voxels.each(|cell, material| {
                    if matches!(material, 1 | 2) {
                        logs.push(cell);
                    }
                });
                Model { voxels, low, size, logs }
            };
            let mut turns: Vec<Model> = turns.into_iter().map(model).collect();
            turns.resize_with(4, empty);
            models.extend(turns);
        }
        Some(Self { kinds, models, reach: forest.reach + 1.5, block: b })
    }

    /// Which tree, if any, cell `cell` (of [`CELL`] metres a side) may
    /// hold, by the hashes the GPU's trees pass takes (`voxelWorldTrees`):
    /// its kind's plant's model, and the voxel column of its foot. Whether
    /// the ground there grows one is for the caller.
    pub fn in_cell(&self, cell: [i32; 2], seed: u32, voxel: f32) -> Option<(usize, [i32; 2])> {
        let chance = unit(hash_of([cell[0], 7, cell[1]], seed));
        let kind = self.kinds.iter().position(|&(below, ..)| chance < below)?;
        let within = [8, 9].map(|n| unit(hash_of([cell[0], n, cell[1]], seed)));
        let foot = [0, 1].map(|a| ((cell[a] as f32 + 0.15 + 0.7 * within[a]) * CELL / voxel).floor() as i32);
        let pick = hash_of([cell[0], 10, cell[1]], seed);
        let (_, first, count) = self.kinds[kind];
        let plant = first + pick % count.max(1);
        Some(((plant * 4 + ((pick >> 8) & 3)) as usize, foot))
    }

    /// Model `model` stood with its trunk on the ground `h` voxels high at
    /// voxel column `foot`, its box on the bricks' grid.
    pub fn stood(&self, model: usize, foot: [i32; 2], h: i32) -> Tree {
        let b = self.block;
        let trunk = [foot[0] - b / 2, h + 1, foot[1] - b / 2];
        let low = self.models[model].low;
        Tree { least: [0, 1, 2].map(|a| (trunk[a] + low[a]) & !7), model }
    }

    /// The least and most corners of a tree's box, in voxels.
    pub fn bounds(&self, tree: &Tree) -> ([i32; 3], [i32; 3]) {
        let size = self.models[tree.model].size;
        (tree.least, [0, 1, 2].map(|a| tree.least[a] + size[a].max(1) - 1))
    }

    /// Each voxel of tree `tree`'s logs within the box from `low` to `high`.
    pub fn logs_in(&self, tree: &Tree, low: [i32; 3], high: [i32; 3], mut each: impl FnMut([i32; 3])) {
        let (least, most) = self.bounds(tree);
        if (0..3).any(|a| most[a] < low[a] || least[a] > high[a]) {
            return;
        }
        for log in &self.models[tree.model].logs {
            let c = [0, 1, 2].map(|a| log[a] + tree.least[a]);
            if (0..3).all(|a| c[a] >= low[a] && c[a] <= high[a]) {
                each(c);
            }
        }
    }

    /// What tree `tree` has at voxel `c`: [`LOG`], [`LEAF`] or [`AIR`].
    pub fn at(&self, tree: &Tree, c: [i32; 3]) -> u8 {
        let model = &self.models[tree.model];
        let local = [0, 1, 2].map(|a| c[a] - tree.least[a]);
        if (0..3).any(|a| local[a] < 0 || local[a] >= model.size[a]) {
            return AIR;
        }
        match model.voxels.get(local) {
            0 => AIR,
            1 | 2 => LOG,
            _ => LEAF,
        }
    }
}

/// The CPU's trees for `trees` at `block` voxels a block, made once and
/// kept for every volume with the same: the last few kinds of wood made.
pub(crate) fn woods(trees: &[VoxelTrees], block: u32) -> Option<std::sync::Arc<Woods>> {
    use std::sync::{Arc, Mutex};
    /// How many woods are kept.
    const KEPT_WOODS: usize = 4;
    type Kept = Vec<(Vec<VoxelTrees>, u32, Arc<Woods>)>;
    static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
    let trees: Vec<VoxelTrees> = trees.iter().filter(|kind| kind.per_hectare > 0.0).take(MOST_KINDS).cloned().collect();
    if trees.is_empty() {
        return None;
    }
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(at) = kept.iter().position(|(t, b, _)| *t == trees && *b == block) {
        // The last asked for goes to the end, so the one asked for longest ago goes first.
        let found = kept.remove(at);
        let woods = found.2.clone();
        kept.push(found);
        return Some(woods);
    }
    let woods = Arc::new(Woods::new(&trees, block)?);
    if kept.len() >= KEPT_WOODS {
        kept.remove(0);
    }
    kept.push((trees, block, woods.clone()));
    Some(woods)
}

/// A plant's voxels turned `turns` quarter turns about the middle of its
/// trunk's foot block, `b` voxels a side, as the world sees a tree turned
/// so: and the least corner and size of their box as turned.
fn turned(
    voxels: &bricks::Builder,
    low: [i32; 3],
    size: [i32; 3],
    b: i32,
    turns: u32,
) -> (bricks::Builder, [i32; 3], [i32; 3]) {
    // In doubled voxels from the middle of the foot block, so a turn keeps to whole numbers.
    let turn = |l: [i32; 3]| {
        let mut d = [2 * l[0] + 1 - b, 2 * l[2] + 1 - b];
        for _ in 0..turns {
            d = [-d[1], d[0]];
        }
        [(d[0] + b - 1) / 2, l[1], (d[1] + b - 1) / 2]
    };
    let a = turn(low);
    let c = turn([0, 1, 2].map(|i| low[i] + size[i] - 1));
    let least = [0, 1, 2].map(|i| a[i].min(c[i]));
    let most = [0, 1, 2].map(|i| a[i].max(c[i]));
    let mut out = bricks::Builder::default();
    voxels.each(|cell, material| {
        let w = turn([0, 1, 2].map(|i| cell[i] + low[i]));
        out.set([0, 1, 2].map(|i| w[i] - least[i]), material);
    });
    (out, least, [0, 1, 2].map(|i| most[i] - least[i] + 1))
}

/// A plant's grid of blocks: its least block from the foot of its trunk, its
/// size, and its bytes from its first.
struct Grid<'a> {
    low: [i32; 3],
    size: [i32; 3],
    bytes: &'a [u8],
}

impl Grid<'_> {
    /// The byte of the block at `block` from the foot of the trunk: none
    /// outside the grid.
    fn at(&self, block: [i32; 3]) -> u8 {
        let at = [0, 1, 2].map(|a| block[a] - self.low[a]);
        if (0..3).any(|a| at[a] < 0 || at[a] >= self.size[a]) {
            return 0;
        }
        self.bytes[(at[0] + self.size[0] * (at[1] + self.size[1] * at[2])) as usize]
    }

    /// How much of the leaves are round point `g`, in blocks from the middle
    /// of the trunk's foot block: from 0 for none to 1 for all, the eight
    /// blocks round it blended, so a crown of blocks is rounded into clumps.
    fn leaves_at(&self, g: [f32; 3]) -> f32 {
        let base = g.map(|a| a.floor() as i32);
        let t = [0, 1, 2].map(|a| g[a] - base[a] as f32);
        let mut sum = 0.0;
        for c in 0..8 {
            let corner = [c & 1, (c >> 1) & 1, c >> 2];
            if self.at([0, 1, 2].map(|a| base[a] + corner[a])) & LEAVES != 0 {
                sum += (0..3).map(|a| if corner[a] == 1 { t[a] } else { 1.0 - t[a] }).product::<f32>();
            }
        }
        sum
    }

    /// The plant's voxels, `b` a block, its colours from palette place
    /// `colours`, and the least corner and size of their box in voxels from
    /// the least corner of the trunk's foot block. A log is round, as thick
    /// as its byte says. Leaves are a shell a few voxels deep where the
    /// blended leaves cross a half, the half moved a little by hash at each
    /// voxel so their edge is ragged, loosely filled: clumps of loose cubes
    /// with nothing inside.
    fn voxels(&self, b: i32, colours: u32, plant: u32) -> (bricks::Builder, [i32; 3], [i32; 3]) {
        let low = self.low.map(|a| (a - 1) * b);
        let size = self.size.map(|a| (a + 2) * b);
        // Only the blocks with something in them or leaves beside them can have voxels.
        let mut near = HashSet::new();
        for z in 0..self.size[2] {
            for y in 0..self.size[1] {
                for x in 0..self.size[0] {
                    let block = [x + self.low[0], y + self.low[1], z + self.low[2]];
                    let byte = self.at(block);
                    if byte & WOOD != 0 {
                        near.insert(block);
                    }
                    if byte & LEAVES != 0 {
                        for n in 0..27 {
                            near.insert([block[0] + n % 3 - 1, block[1] + (n / 3) % 3 - 1, block[2] + n / 9 - 1]);
                        }
                    }
                }
            }
        }
        let seed = 0x7EE5 + plant * 977;
        let shell = (2.0f32).max(b as f32 * 2.0 / 16.0) / b as f32;
        let mut builder = bricks::Builder::default();
        for block in near {
            let byte = self.at(block);
            for i in 0..b * b * b {
                let f = [i % b, (i / b) % b, i / (b * b)];
                let l = [0, 1, 2].map(|a| block[a] * b + f[a]);
                let m = [0, 1, 2].map(|a| l[a] - low[a]);
                let h = hash_of(m, seed + 41);
                let material = if byte & WOOD != 0 {
                    let axis = byte & 3;
                    let thick = f32::from((byte >> 2) & 15) / 8.0;
                    let across = match axis {
                        0 => [f[1], f[2]],
                        1 => [f[0], f[2]],
                        _ => [f[0], f[1]],
                    };
                    let c = across.map(|a| a as f32 + 0.5 - b as f32 * 0.5);
                    let r = thick * b as f32 * 0.5;
                    if thick < 1.0 && c[0] * c[0] + c[1] * c[1] > r * r {
                        continue;
                    }
                    colours + 1 + (h & 1)
                } else {
                    let leaves = self.leaves_at(l.map(|a| (a as f32 + 0.5) / b as f32 - 0.5));
                    let edge = 0.5 + (unit(hash_of(m, seed + 43)) - 0.5) * 0.3;
                    if leaves < edge || leaves > edge + shell || unit(h) > 0.6 {
                        continue;
                    }
                    colours + 3 + (h >> 8) % 3
                };
                builder.set(m, material.min(255) as u8);
            }
        }
        (builder, low, size)
    }
}

/// The shaders' hash of a voxel (`hashOf` in `shaders/voxel_world.slang`).
fn hash_of(p: [i32; 3], seed: u32) -> u32 {
    let mut h = (p[0] as u32).wrapping_mul(0x8DA6_B343)
        ^ (p[1] as u32).wrapping_mul(0xD816_3841)
        ^ (p[2] as u32).wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_terrain::blocks::plant::{PlantDef, PlantKind};

    use super::*;

    #[test]
    fn a_kind_of_tree_is_its_plants_logs_and_leaves() {
        let trees = [
            VoxelTrees {
                plant: Arc::new(PlantDef::default()),
                per_hectare: 80.0,
                wood: [90, 60, 40],
                leaves: [60, 120, 40],
                areas: Default::default(),
            },
            VoxelTrees {
                plant: Arc::new(PlantDef { kind: PlantKind::Conifer, ..PlantDef::default() }),
                per_hectare: 40.0,
                wood: [80, 50, 30],
                leaves: [30, 80, 40],
                areas: Default::default(),
            },
        ];
        let forest = forest(&trees, 22).expect("trees");
        assert_eq!(forest.words[0], 2);
        assert_eq!(forest.colors.len(), 2 * COLORS_A_KIND);
        // 80 and 40 to a hectare, in cells of 25 m², taken together.
        let chances = [1, 2].map(|k| f32::from_bits(forest.words[KINDS_AT + (k - 1) * 4]));
        assert!((chances[0] - 0.2).abs() < 1e-6 && (chances[1] - 0.3).abs() < 1e-6, "{chances:?}");
        assert_eq!(forest.words[KINDS_AT + 4 + 3], 22 + COLORS_A_KIND as u32, "the second kind's colours");
        let bytes: Vec<u8> = forest.words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let plant = &forest.words[VARIANTS_AT..VARIANTS_AT + VARIANT_WORDS];
        let size = (plant[3] * plant[4] * plant[5]) as usize;
        let grid = &bytes[plant[6] as usize..plant[6] as usize + size];
        let model = trees[0].plant.grow(0x51ED_0000);
        let wood = grid.iter().filter(|&&b| b & WOOD != 0).count();
        let leaves = grid.iter().filter(|&&b| b == LEAVES).count();
        let grown_wood = model.blocks.iter().filter(|(_, part, _)| part.is_wood()).count();
        assert_eq!(wood, grown_wood, "every block of wood");
        assert_eq!(leaves, model.count(PlantPart::Leaf), "every block of leaves");
        // The foot of its trunk is a log standing up.
        let at = |b: [i32; 3]| {
            let [x, y, z] = [0, 1, 2].map(|a| (b[a] - plant[a] as i32) as u32);
            grid[(x + plant[3] * (y + plant[4] * z)) as usize]
        };
        assert_eq!(at([0, 0, 0]) & (WOOD | 3), WOOD | 1, "the trunk's foot stands up");
        assert!(forest.reach >= f32::from_bits(plant[7]) && forest.reach > 2.0);
    }

    #[test]
    fn a_plants_voxels_are_round_logs_and_a_shell_of_leaves() {
        let oak = VoxelTrees {
            plant: Arc::new(PlantDef::default()),
            per_hectare: 80.0,
            wood: [90, 60, 40],
            leaves: [60, 120, 40],
            areas: Default::default(),
        };
        let forest = forest(&[oak], 22).expect("trees");
        let palette = vec![[100, 100, 100, 255]; 32];
        let made = models(&forest, 16, &palette);
        assert_eq!(made.words[..2], [VARIANTS * 4, 16]);
        let record = &forest.words[VARIANTS_AT..VARIANTS_AT + VARIANT_WORDS];
        // Its box is its grid and a block round it; turned a quarter, its X and Z are turned too.
        let model = |turn: usize| &made.words[MODELS_AT + turn * MODEL_WORDS..][..MODEL_WORDS];
        let low = [0, 1, 2].map(|a| (record[a] as i32 - 1) * 16);
        assert_eq!(model(0)[4..7], low.map(|a| a as u32));
        let size = [3, 4, 5].map(|a| (record[a] as i32 + 2) * 16);
        let turned = [-(low[2] + size[2] - 16), low[1], low[0]];
        assert_eq!(model(1)[4..7], turned.map(|a| a as u32), "turned a quarter");
        assert!(made.bricks.len() > 100 * bricks::BRICK_WORDS && !made.materials.is_empty());
        let bytes: Vec<u8> = forest.words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let grid = Grid {
            low: [0, 1, 2].map(|a| record[a] as i32),
            size: [3, 4, 5].map(|a| record[a] as i32),
            bytes: &bytes[record[6] as usize..],
        };
        let (voxels, low, _) = grid.voxels(16, 22, 0);
        let at = |l: [i32; 3]| voxels.get([0, 1, 2].map(|a| l[a] - low[a]));
        // The middle of the trunk's foot is wood, in the kind's first colours.
        assert!(matches!(at([8, 0, 8]), 23 | 24), "{}", at([8, 0, 8]));
        // Leaves only near the crown's skin: none in a block of leaves with leaves all round it.
        let mut leaves = 0;
        let mut deep = 0;
        for z in grid.low[2]..grid.low[2] + grid.size[2] {
            for y in grid.low[1]..grid.low[1] + grid.size[1] {
                for x in grid.low[0]..grid.low[0] + grid.size[0] {
                    let inside = (0..27).all(|n| grid.at([x + n % 3 - 1, y + n / 3 % 3 - 1, z + n / 9 - 1]) == LEAVES);
                    for i in 0..16 * 16 * 16 {
                        let l = [x * 16 + i % 16, y * 16 + i / 16 % 16, z * 16 + i / 256];
                        if at(l) >= 25 {
                            leaves += 1;
                            deep += usize::from(inside);
                        }
                    }
                }
            }
        }
        assert!(leaves > 1000, "{leaves} voxels of leaves");
        assert_eq!(deep, 0, "leaves deep in the crown");
    }
}
