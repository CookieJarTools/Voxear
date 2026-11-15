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
//!
//! Each point also keeps what grows thick enough there to colour the grass
//! where its plants are too far off to be drawn (see
//! `blocks::grow::Cover`): their colour and how much of their side they
//! show, in the bytes its two tints leave over.

use super::BlockGenerate;
use crate::{
    blocks::grow::Cover,
    generate::{WorldGen, biomes::Place},
};

/// How many blocks apart the lattice's points are.
pub const TINT_STEP: i32 = 4;

/// How many points a tile is a side.
pub const TINT_TILE: i32 = 16;

/// How many times further apart the far view's tint points are: a point
/// every 64 blocks, made as a lattice of blocks as many times larger.
pub const TINT_FAR: i32 = 16;

/// How many points each way a point's tints are averaged over.
pub const TINT_BLEND: i32 = 3;

/// The most side its plants show for each square metre of ground that a
/// point keeps; more counts as this.
pub const MOST_COVER: f32 = 0.5;

/// The tints of tile `[tx, tz]` of the lattice of the world `generate`
/// makes of blocks `block_size` metres a side with generator `world`: for
/// each of its points, row by row along x, its grass tint and then its
/// foliage tint, each as three bytes, red, green and blue, each part out of
/// 2 (see `generate::BiomeTints`). Their fourth bytes keep what `covers`
/// grow there (see [`cover_of`]). None where the world's biomes have no
/// tints and nothing covers its ground; white where only plants do.
pub fn tint_tile(
    generate: &BlockGenerate,
    world: &WorldGen,
    block_size: f32,
    [tx, tz]: [i32; 2],
    covers: &[Cover],
) -> Option<Vec<u32>> {
    let tinted = |placed: &&crate::generate::WorldBiomes| placed.biomes.list().iter().any(|b| !b.tints.is_none());
    let placed = generate.placed.as_deref().filter(tinted);
    if placed.is_none() && covers.is_empty() {
        return None;
    }
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
    let sunk = placed.filter(|placed| placed.any_sunk()).and_then(|_| world.sunk(&xs, &zs, &heights));
    let sea = world.sea_level();
    // Each point's own tints, the grass's three parts then the foliage's.
    let own: Vec<[f32; 6]> = (0..xs.len())
        .map(|k| {
            let Some(placed) = placed else { return [1.0; 6] };
            let biomes = &placed.biomes;
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
    // What covers each of the tile's own points, from where it is alone, as each machine works it out alike.
    let across = TINT_STEP as f32 * size;
    let seed = world.seed();
    let cover = |x: usize, z: usize| {
        let k = (z + reach) * wide + x + reach;
        let rise = |a: usize, b: usize| (heights[b] - heights[a]) / (2.0 * across);
        let slope = rise(k - 1, k + 1).hypot(rise(k - wide, k + wide));
        let at =
            |cover: &Cover| cover.at(seed, [xs[k], zs[k]], across, (heights[k] - sea, slope), (warmth[k], wetness[k]));
        let amounts: Vec<f32> = covers.iter().map(at).collect();
        let total: f32 = amounts.iter().sum();
        let color = match total > 0.0 {
            true => covers.iter().zip(&amounts).fold([0.0; 3], |sum, (cover, &amount)| {
                std::array::from_fn(|part| sum[part] + cover.color[part] * amount / total)
            }),
            false => [0.0; 3],
        };
        cover_of(color, total)
    };
    let mut tints = Vec::with_capacity(tile * tile * 2);
    for z in 0..tile {
        for x in 0..tile {
            let mut sum = [0.0f32; 6];
            for k in z..=z + 2 * reach {
                let row = along[k * tile + x];
                (0..6).for_each(|part| sum[part] += row[part]);
            }
            let mean = sum.map(|part| part / count);
            let [grass_over, foliage_over] = if covers.is_empty() { [0; 2] } else { cover(x, z) };
            tints.extend([pack(&mean[..3]) | grass_over << 24, pack(&mean[3..]) | foliage_over << 24]);
        }
    }
    Some(tints)
}

/// What covers a point as the fourth bytes of its grass tint and its
/// foliage tint keep it: the plants' colour, linear, red and green in the
/// first and blue in the second, four bits each, and how much side they
/// show for each square metre, out of [`MOST_COVER`], in the second's last
/// four. Each kept as its square root, finer where it is small.
pub fn cover_of(color: [f32; 3], amount: f32) -> [u32; 2] {
    let nibble = |part: f32| (part.clamp(0.0, 1.0).sqrt() * 15.0).round() as u32;
    let [r, g, b] = color.map(nibble);
    [r | g << 4, b | nibble(amount / MOST_COVER) << 4]
}

/// A point's cover from the fourth bytes of its two tints, as [`cover_of`]
/// keeps it: the plants' colour and how much side they show.
pub fn cover_from(grass: u32, foliage: u32) -> ([f32; 3], f32) {
    let part = |word: u32, shift: u32| {
        let nibble = ((word >> 24 >> shift) & 15) as f32 / 15.0;
        nibble * nibble
    };
    ([part(grass, 0), part(grass, 4), part(foliage, 0)], part(foliage, 4) * MOST_COVER)
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
        let placed = WorldBiomes { regions: None, biomes: Biomes::new(list, 15.0, 0.3, world.seed()), blocks: Vec::new() };
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
                let tile = tint_tile(&generate, &world, 1.0, [tx, tz], &[]).expect("tints");
                assert_eq!(tile.len(), (TINT_TILE * TINT_TILE * 2) as usize);
                assert_eq!(tile, tint_tile(&generate, &world, 1.0, [tx, tz], &[]).unwrap(), "the same every time");
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
        assert!(tint_tile(&plain, &world, 1.0, [0, 0], &[]).is_none());
    }

    #[test]
    fn plants_colour_the_points_they_grow_round_in_their_patches_and_a_world_of_no_tints_gets_them_too() {
        use crate::blocks::grow::{Cover, GrowRule};
        let (_, world) = split();
        // Red flowers in patches 14 m across, and grass everywhere, on a world whose biomes have no tints.
        let flowers = Cover {
            rule: GrowRule { patches: 14.0, slope: 89.0, ..GrowRule::default() },
            color: [0.8, 0.1, 0.05],
            amount: 0.04,
        };
        let grass =
            Cover { rule: GrowRule { slope: 89.0, ..GrowRule::default() }, color: [0.1, 0.3, 0.05], amount: 0.3 };
        let plain = BlockGenerate::default();
        let tile =
            tint_tile(&plain, &world, 1.0, [0, 0], std::slice::from_ref(&flowers)).expect("tints for the plants alone");
        let covers: Vec<([f32; 3], f32)> = tile.chunks_exact(2).map(|point| cover_from(point[0], point[1])).collect();
        assert!(tile.chunks_exact(2).all(|point| point[0] & 0xFF_FFFF == 0x80_8080), "white tints");
        // Some points in a patch and some out of one, each as its place has it, the same every time.
        let (inside, outside) =
            (covers.iter().filter(|c| c.1 > 0.0).count(), covers.iter().filter(|c| c.1 == 0.0).count());
        assert!(inside > 10 && outside > 10, "{inside} in patches, {outside} not");
        assert_eq!(tile, tint_tile(&plain, &world, 1.0, [0, 0], std::slice::from_ref(&flowers)).unwrap());
        let (color, amount) = covers.iter().copied().find(|c| c.1 > 0.0).unwrap();
        assert!((amount - 0.04).abs() < 0.01 && color[0] > 0.6 && color[1] < 0.2, "{color:?} {amount}");
        // With grass too, every point is covered, mostly green, the flowers' red in it where they grow.
        let both = tint_tile(&plain, &world, 1.0, [0, 0], &[flowers, grass]).unwrap();
        let covers: Vec<([f32; 3], f32)> = both.chunks_exact(2).map(|point| cover_from(point[0], point[1])).collect();
        assert!(covers.iter().all(|(color, amount)| *amount > 0.25 && color[1] > color[0] * 0.5));
        // Seen far off, the far view's points, 64 blocks apart, take in what share of each is in a patch.
        let far = tint_tile(&plain, &world, 16.0, [0, 0], &[Cover { amount: 0.4, ..flowers_at_cover() }]).unwrap();
        let shares: Vec<f32> = far.chunks_exact(2).map(|point| cover_from(point[0], point[1]).1).collect();
        assert!(shares.iter().any(|&share| share > 0.0 && share < 0.35), "some partly in patches: {shares:?}");
    }

    fn flowers_at_cover() -> crate::blocks::grow::Cover {
        let rule = crate::blocks::grow::GrowRule { patches: 14.0, slope: 89.0, ..Default::default() };
        crate::blocks::grow::Cover { rule, color: [0.8, 0.1, 0.05], amount: 0.04 }
    }

    #[test]
    fn a_points_cover_reads_back_as_it_was_kept() {
        // Kept in the fourth bytes of a point's two tints, beside their own parts.
        let kept = |color: [f32; 3], amount: f32| {
            let [grass, foliage] = cover_of(color, amount);
            cover_from(0x80_8080 | grass << 24, 0x80_8080 | foliage << 24)
        };
        let (color, amount) = kept([0.25, 0.5, 1.0], 0.2);
        assert!(color.iter().zip([0.25, 0.5, 1.0]).all(|(a, b)| (a - b).abs() < 0.05), "{color:?}");
        assert!((amount - 0.2).abs() < 0.02, "{amount}");
        assert_eq!(kept([0.0; 3], 0.0), ([0.0; 3], 0.0));
        assert_eq!(kept([1.0; 3], 9.0).1, MOST_COVER, "more is the most a point keeps");
    }
}
