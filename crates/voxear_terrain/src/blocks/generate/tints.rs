//! Tints: the colours a generated world's tinted blocks take from the biomes
//! they stand in, grass green in a forest and yellowed on a savanna, fading
//! from one biome's to the next where they meet.
//!
//! They are kept on a lattice, a point every [`TINT_STEP`] blocks, in tiles
//! of [`TINT_TILE`] points a side. Each point takes the tints of the biome
//! there, averaged with those of the points round it [`TINT_BLEND`] each
//! way, so a border fades over a few dozen blocks rather than stopping at a
//! line. A point's tints come from where it is alone, so every tile, and
//! every machine, agrees on each. The renderer reads between the points, so
//! a face's colour changes smoothly across it.

use super::BlockGenerate;
use crate::generate::{WorldGen, biomes::Place};

/// How many blocks apart the lattice's points are.
pub const TINT_STEP: i32 = 4;

/// How many points a tile is a side.
pub const TINT_TILE: i32 = 16;

/// How many times further apart the far view's tint points are: a point
/// every 64 blocks, made as a lattice of blocks as many times larger.
pub const TINT_FAR: i32 = 16;

/// How many points each way a point's tints are averaged over.
pub const TINT_BLEND: i32 = 3;

/// The tints of tile `[tx, tz]` of the lattice of the world `generate`
/// makes of blocks `block_size` metres a side with generator `world`: for
/// each of its points, row by row along x, its grass tint and then its
/// foliage tint, each as four bytes, red, green, blue and none, each part
/// out of 2 (see `generate::BiomeTints`). None where the world's biomes
/// have no tints.
pub fn tint_tile(generate: &BlockGenerate, world: &WorldGen, block_size: f32, [tx, tz]: [i32; 2]) -> Option<Vec<u32>> {
    let placed = generate.placed.as_deref().filter(|placed| placed.biomes.list().iter().any(|b| !b.tints.is_none()))?;
    let biomes = &placed.biomes;
    let size = block_size.max(0.01);
    // The tile's points and as many again round it as each is averaged over.
    let wide = (TINT_TILE + 2 * TINT_BLEND) as usize;
    let first = [tx * TINT_TILE - TINT_BLEND, tz * TINT_TILE - TINT_BLEND];
    let at = |point: i32, axis: usize| generate.position[axis] + (point * TINT_STEP) as f32 * size;
    let (xs, zs): (Vec<f32>, Vec<f32>) =
        (0..wide * wide).map(|k| (at(first[0] + (k % wide) as i32, 0), at(first[1] + (k / wide) as i32, 1))).unzip();
    let mut heights = vec![0.0; xs.len()];
    world.heights(&xs, &zs, &mut heights);
    let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
    let sunk = placed.any_sunk().then(|| world.sunk(&xs, &zs, &heights)).flatten();
    let sea = world.sea_level();
    // Each point's own tints, the grass's three parts then the foliage's.
    let own: Vec<[f32; 6]> = (0..xs.len())
        .map(|k| {
            let sunk = sunk.as_ref().map_or(0.0, |sunk| sunk[k]);
            let place = Place { temperature: warmth[k], humidity: wetness[k], above_sea: heights[k] - sea, sunk };
            let [grass, foliage] =
                biomes.at(xs[k], zs[k], place).map_or([[1.0; 3]; 2], |b| biomes.list()[b].tints.colors());
            [grass[0], grass[1], grass[2], foliage[0], foliage[1], foliage[2]]
        })
        .collect();
    // Averaged over the points round each, along x and then along z.
    let reach = TINT_BLEND as usize;
    let count = (2 * reach + 1) as f32;
    let tile = TINT_TILE as usize;
    let mut along = vec![[0.0f32; 6]; tile * wide];
    for z in 0..wide {
        for x in 0..tile {
            let mut sum = [0.0f32; 6];
            for k in x..=x + 2 * reach {
                let point = own[z * wide + k];
                (0..6).for_each(|part| sum[part] += point[part]);
            }
            along[z * tile + x] = sum.map(|part| part / count);
        }
    }
    let byte = |part: f32| ((part / crate::generate::BiomeTints::MOST).clamp(0.0, 1.0) * 255.0).round() as u32;
    let pack = |rgb: &[f32]| byte(rgb[0]) | byte(rgb[1]) << 8 | byte(rgb[2]) << 16;
    let mut tints = Vec::with_capacity(tile * tile * 2);
    for z in 0..tile {
        for x in 0..tile {
            let mut sum = [0.0f32; 6];
            for k in z..=z + 2 * reach {
                let row = along[k * tile + x];
                (0..6).for_each(|part| sum[part] += row[part]);
            }
            let mean = sum.map(|part| part / count);
            tints.extend([pack(&mean[..3]), pack(&mean[3..])]);
        }
    }
    Some(tints)
}

/// A packed tint's three parts, each out of 2.
pub fn unpacked(tint: u32) -> [f32; 3] {
    [0, 8, 16].map(|shift| ((tint >> shift) & 0xFF) as f32 / 255.0 * crate::generate::BiomeTints::MOST)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::generate::{BiomeDef, BiomeTints, Biomes, Preset, WorldBiomes};

    /// Rolling hills split between a cool biome with blue-green grass and a warm one with yellow grass, at
    /// how warm its middle is.
    fn split() -> (BlockGenerate, WorldGen) {
        let world = Preset::RollingHills.world();
        let warmth = world.climate(0.0, 0.0, world.height_at(0.0, 0.0)).0;
        let biome = |temperature: [f32; 2], grass: [f32; 3]| BiomeDef {
            temperature,
            humidity: [0.0, 1.0],
            tints: BiomeTints { grass: Some(grass), foliage: None },
            ..BiomeDef::default()
        };
        let list = vec![biome([-60.0, warmth], [0.6, 1.0, 1.2]), biome([warmth, 60.0], [1.6, 1.2, 0.6])];
        let placed = WorldBiomes { biomes: Biomes::new(list, 15.0, 0.3, world.seed()), blocks: Vec::new() };
        let generate = BlockGenerate { placed: Some(Arc::new(placed)), ..BlockGenerate::default() };
        (generate, world)
    }

    #[test]
    fn tints_fade_from_one_biome_to_the_next_across_tiles_and_are_white_where_a_biome_has_none() {
        let (generate, world) = split();
        // Eight tiles a side round the middle, put together into one lattice of their points.
        let side = (8 * TINT_TILE) as usize;
        let mut lattice = vec![[0u32; 2]; side * side];
        for tz in -4..4 {
            for tx in -4..4 {
                let tile = tint_tile(&generate, &world, 1.0, [tx, tz]).expect("tints");
                assert_eq!(tile.len(), (TINT_TILE * TINT_TILE * 2) as usize);
                assert_eq!(tile, tint_tile(&generate, &world, 1.0, [tx, tz]).unwrap(), "the same every time");
                for k in 0..(TINT_TILE * TINT_TILE) as usize {
                    let (x, z) = (k % TINT_TILE as usize, k / TINT_TILE as usize);
                    let (lx, lz) =
                        ((tx + 4) as usize * TINT_TILE as usize + x, (tz + 4) as usize * TINT_TILE as usize + z);
                    lattice[lz * side + lx] = [tile[2 * k], tile[2 * k + 1]];
                }
            }
        }
        // Both biomes' grass, and the foliage white everywhere as neither has its own.
        let grass: Vec<[f32; 3]> = lattice.iter().map(|tints| unpacked(tints[0])).collect();
        let bluest = grass.iter().map(|tint| tint[2] - tint[0]).fold(f32::MIN, f32::max);
        let yellowest = grass.iter().map(|tint| tint[0] - tint[2]).fold(f32::MIN, f32::max);
        assert!(bluest > 0.5 && yellowest > 0.9, "{bluest} {yellowest}");
        assert!(lattice.iter().all(|tints| unpacked(tints[1]).iter().all(|part| (part - 1.0).abs() < 0.01)));
        // From one point to the next, tile edges too, no more than a share of the way from one to the other.
        let most = 1.0 / (2 * TINT_BLEND + 1) as f32 + 0.02;
        for z in 0..side {
            for x in 0..side {
                let here = grass[z * side + x];
                for (bx, bz) in [(x + 1, z), (x, z + 1)].into_iter().filter(|&(bx, bz)| bx < side && bz < side) {
                    let step = (grass[bz * side + bx][0] - here[0]).abs();
                    assert!(step <= most, "{step} at ({x}, {z})");
                }
            }
        }
        // A world whose biomes have no tints has none.
        let mut plain = generate.clone();
        let mut placed = (*plain.placed.take().unwrap()).clone();
        let list: Vec<BiomeDef> = placed
            .biomes
            .list()
            .iter()
            .map(|biome| BiomeDef { tints: BiomeTints::default(), ..biome.clone() })
            .collect();
        placed.biomes = Biomes::new(list, 15.0, 0.3, world.seed());
        plain.placed = Some(Arc::new(placed));
        assert!(tint_tile(&plain, &world, 1.0, [0, 0]).is_none());
    }
}
