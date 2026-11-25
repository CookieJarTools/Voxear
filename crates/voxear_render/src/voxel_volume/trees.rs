//! The trees of an endless Voxel Volume ([`VoxelTrees`]), as the GPU reads
//! them (`shaders/voxel_world.slang`).
//!
//! Each kind of tree is grown from a few seeds on the CPU, and each plant
//! grown becomes a grid of its blocks, a byte a block: a log, which way it
//! lies and how thick it is; leaves; or nothing. The grids go to the GPU
//! once, which rounds the leaves into clumps and fills only a thin shell
//! of them. The trees themselves are placed there, at most
//! one in each [`CELL`] metres square, by hash.

use std::collections::HashMap;

use voxear_terrain::blocks::{plant::PlantPart, shape::BlockFacing};

use super::VoxelTrees;

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
            },
            VoxelTrees {
                plant: Arc::new(PlantDef { kind: PlantKind::Conifer, ..PlantDef::default() }),
                per_hectare: 40.0,
                wood: [80, 50, 30],
                leaves: [30, 80, 40],
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
}
