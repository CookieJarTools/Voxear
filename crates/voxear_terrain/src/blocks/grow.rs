//! Plants a world of blocks grows by itself: flowers, bushes and trees that
//! stand on its ground wherever a rule says, with nobody placing them.
//!
//! A rule ([`GrowRule`]) says which blocks its plants stand on, the climate
//! and heights they like, how steep the ground may be, how near two may
//! stand, and whether they grow everywhere or in patches. Where they stand comes from a grid over the world with one try a
//! cell (see `crate::generate::structures::Placement`), so a chunk column's
//! plants are worked out from that column alone ([`grow`]) and are the same
//! on every machine and every run: the ground of an endless world grows
//! its plants as it is made round the camera.
//!
//! Plants grow on the ground as it was made. Each keeps the block it stands
//! on ([`Grown`]); once that block is dug away or changed, or the block
//! over it is filled, the plant is gone ([`gone`]), and it is back when the
//! ground is put back as it was. So nothing about them is saved: whoever
//! has the world's blocks has its plants.

use std::collections::HashSet;

use voxear_scene::{Plant, PlantSet};

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId, CHUNK, Chunk, depth_of},
    generate::BlockGenerate,
    is_water,
};
use crate::generate::{WorldGen, structures::Placement};

/// Where a kind of plant grows on a world of blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct GrowRule {
    /// The blocks it stands on, by the world's numbers; none for any
    /// ground.
    pub on: Vec<BlockId>,
    /// How warm it likes it, in °C, from and to; none for any.
    pub temperature: Option<[f32; 2]>,
    /// How wet, from 0 for dry to 1; none for any.
    pub humidity: Option<[f32; 2]>,
    /// How high over the sea its ground is, in metres, from and to; none
    /// for any. Over the world's corner where its ground is not generated.
    pub height: Option<[f32; 2]>,
    /// The steepest ground it stands on, in degrees.
    pub slope: f32,
    /// How far apart its tries are, in metres: one a square that size.
    pub spacing: f32,
    /// The least between two of its plants, in metres: under its spacing.
    pub separation: f32,
    /// How many of its tries grow, from 0 for none to 1 for all.
    pub chance: f32,
    /// How many metres across the patches it grows in are, with as much
    /// bare ground between them; nothing for everywhere.
    pub patches: f32,
    /// What tells its plants' places from another rule's with the same
    /// numbers.
    pub seed: u32,
    /// The smallest and largest size.
    pub scale: [f32; 2],
    /// Whether a plant stands on the middle of its block, turned by quarter
    /// turns, as the blocks are; else anywhere on it, turned any way.
    pub aligned: bool,
}

impl Default for GrowRule {
    fn default() -> Self {
        Self {
            on: Vec::new(),
            temperature: None,
            humidity: None,
            height: None,
            slope: 30.0,
            spacing: 4.0,
            separation: 2.0,
            chance: 0.5,
            patches: 0.0,
            seed: 0,
            scale: [0.8, 1.2],
            aligned: true,
        }
    }
}

impl GrowRule {
    /// Its grid of tries.
    fn placement(&self) -> Placement {
        Placement {
            spacing: self.spacing,
            separation: self.separation,
            chance: self.chance,
            salt: self.seed,
            ..Placement::default()
        }
    }

    /// Whether ground `above_sea` metres up, rising `slope` for every metre
    /// along, is ground it grows on.
    fn stands_on(&self, above_sea: f32, slope: f32) -> bool {
        let high = self.height.is_none_or(|[from, to]| (from.min(to)..=from.max(to)).contains(&above_sea));
        high && slope <= self.slope.clamp(0.0, 89.0).to_radians().tan()
    }

    /// Whether it likes a place `temperature` °C warm and `humidity` wet.
    fn likes(&self, (temperature, humidity): (f32, f32)) -> bool {
        let within = |range: Option<[f32; 2]>, v: f32| range.is_none_or(|[a, b]| (a.min(b)..=a.max(b)).contains(&v));
        within(self.temperature, temperature) && within(self.humidity, humidity)
    }
}

/// The ground plants grow on: a world's blocks as they were made, how its
/// blocks are drawn, and the generator that made it, where one did.
#[derive(Clone, Copy)]
pub struct Ground<'a> {
    pub blocks: &'a BlockMap,
    pub faces: &'a [BlockFaces],
    /// How many metres a block is a side.
    pub block_size: f32,
    /// Whether the world is drawn smooth, its surface between its blocks'
    /// middles.
    pub smooth: bool,
    pub generated: Option<(&'a BlockGenerate, &'a WorldGen)>,
}

/// A plant a rule grew: where it stands, in metres from the world's corner,
/// and the block it stands on, as it was made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grown {
    pub plant: Plant,
    pub ground: [i32; 3],
    pub block: BlockId,
}

impl Grown {
    /// The block over the one it stands on: where it is.
    pub fn over(&self) -> [i32; 3] {
        [self.ground[0], self.ground[1] + 1, self.ground[2]]
    }
}

/// A number from `seed` and `part`, well mixed: whole numbers alone, so the
/// same everywhere.
fn mixed(seed: u32, part: u32) -> u32 {
    let mut h = seed ^ part.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// From 0 up to 1.
fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / 16_777_216.0
}

/// A number from 0 to 1 that changes smoothly over the ground, `size` metres
/// from one of its own to the next: each corner of a grid that size has a
/// number of its own, from whole numbers alone, and a place between four
/// has theirs blended.
fn patchy(seed: u32, [x, z]: [f32; 2], size: f32) -> f32 {
    let (x, z) = (x / size, z / size);
    let (cx, cz) = (x.floor(), z.floor());
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, tz) = (ease(x - cx), ease(z - cz));
    let corner = |dx: i32, dz: i32| {
        let across = mixed(seed ^ (cx as i32 + dx) as u32, 0x51);
        unit(mixed(across ^ (cz as i32 + dz) as u32, 0x52))
    };
    let along = |dz: i32| corner(0, dz) + (corner(1, dz) - corner(0, dz)) * tx;
    along(0) + (along(1) - along(0)) * tz
}

/// The chunks of one chunk column, highest first.
struct Column<'a> {
    chunks: Vec<(i32, &'a Chunk)>,
}

impl<'a> Column<'a> {
    fn of(blocks: &'a BlockMap, [cx, cz]: [i32; 2]) -> Self {
        let mut chunks: Vec<(i32, &Chunk)> = blocks
            .chunks()
            .filter(|(key, chunk)| key[0] == cx && key[2] == cz && !chunk.is_air())
            .map(|(key, chunk)| (key[1], &**chunk))
            .collect();
        chunks.sort_unstable_by_key(|(y, _)| std::cmp::Reverse(*y));
        Self { chunks }
    }

    /// The highest block of column `[x, z]` of its chunks' own that is not
    /// air: how high it is, which block, and how full.
    fn top(&self, [x, z]: [u32; 2]) -> Option<(i32, BlockId, u8)> {
        for &(cy, chunk) in &self.chunks {
            for y in (0..CHUNK).rev() {
                let block = chunk.get(x, y, z);
                if block != AIR {
                    return Some((cy * CHUNK as i32 + y as i32, block, chunk.fill(x, y, z)));
                }
            }
        }
        None
    }

    /// How much the ground rises for every block along at `[x, z]`, whose
    /// top is `top`: from the tops beside it, those in the column alone, so
    /// a column on its own gives what it gives among its neighbours.
    fn slope(&self, [x, z]: [u32; 2], top: i32) -> f32 {
        let last = CHUNK - 1;
        let beside = |x: u32, z: u32| self.top([x, z]).map_or(top, |(y, ..)| y) as f32;
        let along = |low: f32, high: f32, both: bool| (high - low) / if both { 2.0 } else { 1.0 };
        let dx = along(beside(x.saturating_sub(1), z), beside((x + 1).min(last), z), x > 0 && x < last);
        let dz = along(beside(x, z.saturating_sub(1)), beside(x, (z + 1).min(last)), z > 0 && z < last);
        (dx * dx + dz * dz).sqrt()
    }
}

/// The plants `rule` grows on chunk column `column` of `ground`, in an order
/// that is always the same: a plant's place in it is its number. Each stands
/// on the highest block of its place, under the open sky, where that block
/// is ground the rule grows on.
pub fn grow(rule: &GrowRule, ground: &Ground, column: [i32; 2]) -> Vec<Grown> {
    let size = ground.block_size.max(0.01);
    let side = CHUNK as i32;
    let blocks = Column::of(ground.blocks, column);
    if blocks.chunks.is_empty() {
        return Vec::new();
    }
    let (position, world) = match ground.generated {
        Some((generate, world)) => (generate.position, Some(world)),
        None => ([0.0; 2], None),
    };
    let (seed, sea) = world.map_or((0, 0.0), |world| (world.seed(), world.sea_level()));
    // Every try that may fall in the column, a block further than its edges; kept where its block is the column's.
    let low = [0, 1].map(|axis| position[axis] + (column[axis] * side) as f32 * size);
    let high = low.map(|v| v + side as f32 * size);
    let sites = rule.placement().tries_near(seed, low, high, size);
    let mut taken: HashSet<[i32; 2]> = HashSet::new();
    let mut grown = Vec::new();
    for site in sites {
        let at = [site.at[0] - position[0], site.at[1] - position[1]];
        let [bx, bz] = at.map(|v| (v / size).floor() as i32);
        if bx.div_euclid(side) != column[0] || bz.div_euclid(side) != column[1] {
            continue;
        }
        // In patches: where a number that wanders over the ground is in its upper half.
        if rule.patches > 0.0
            && patchy(seed ^ rule.seed.wrapping_mul(0x9E37_79B9), site.at, rule.patches.max(1.0)) < 0.5
        {
            continue;
        }
        let inside = [bx.rem_euclid(side) as u32, bz.rem_euclid(side) as u32];
        let Some((y, block, fill)) = blocks.top(inside) else { continue };
        if is_water(ground.faces, block) || !(rule.on.is_empty() || rule.on.contains(&block)) {
            continue;
        }
        // The top of a cube; drawn smooth, where the surface crosses the block.
        let top = match ground.smooth {
            true => (y as f32 + 0.5 + depth_of(fill).clamp(-0.5, 0.5)) * size,
            false => (y + 1) as f32 * size,
        };
        if !rule.stands_on(top - sea, blocks.slope(inside, y)) {
            continue;
        }
        if rule.aligned && !taken.insert([bx, bz]) {
            continue;
        }
        let (x, z, yaw) = match rule.aligned {
            true => (
                (bx as f32 + 0.5) * size,
                (bz as f32 + 0.5) * size,
                f32::from(site.turns) * std::f32::consts::FRAC_PI_2,
            ),
            false => (at[0], at[1], unit(mixed(site.seed, 2)) * std::f32::consts::TAU),
        };
        let scale = rule.scale[0] + (rule.scale[1] - rule.scale[0]) * unit(mixed(site.seed, 1));
        let place = [position[0] + x, position[1] + z];
        grown.push((place, Grown { plant: Plant::new([x, top, z], yaw, scale), ground: [bx, y, bz], block }));
    }
    // The climate where each stands, asked of the generator at once.
    let Some(world) = world.filter(|_| rule.temperature.is_some() || rule.humidity.is_some()) else {
        return grown.into_iter().map(|(_, grown)| grown).collect();
    };
    let (xs, zs): (Vec<f32>, Vec<f32>) = grown.iter().map(|(at, _)| (at[0], at[1])).unzip();
    let heights: Vec<f32> = grown.iter().map(|(_, grown)| grown.plant.position[1]).collect();
    let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
    let liked = grown.into_iter().zip(warmth.into_iter().zip(wetness));
    liked.filter(|(_, climate)| rule.likes(*climate)).map(|((_, grown), _)| grown).collect()
}

/// Which of `grown` are gone as `blocks` stand now, by their numbers: those
/// whose block is no longer the one they grew on, or has a block over it.
pub fn gone(grown: &[Grown], blocks: &BlockMap) -> PlantSet {
    let stands = |plant: &Grown| blocks.get(plant.ground) == plant.block && blocks.get(plant.over()) == AIR;
    grown.iter().enumerate().filter(|(_, plant)| !stands(plant)).map(|(number, _)| number as u32).collect()
}

/// The chunks whose blocks decide whether any of `grown` still stands, each
/// once, in order.
pub fn chunks_under(grown: &[Grown]) -> Vec<[i32; 3]> {
    let mut keys: Vec<[i32; 3]> =
        grown.iter().flat_map(|plant| [plant.ground, plant.over()]).map(|at| BlockMap::chunk_of(at).0).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::{
            file::{BlockFile, BlockKind, Ground as FlatGround},
            generate::column_chunks,
        },
        generate::Preset,
    };

    /// A world of metre blocks 96 across on rolling hills, with a sea, and its ground.
    fn hills() -> (BlockFile, WorldGen, BlockMap) {
        let world = Preset::RollingHills.world();
        let mut file = BlockFile {
            ground: FlatGround { size: [96, 96], ..FlatGround::default() },
            generate: Some(BlockGenerate {
                world: Some("hills".into()),
                position: [300.0, -150.0],
                caves: 0.0,
                ..BlockGenerate::default()
            }),
            ..BlockFile::default()
        };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.1, 0.3, 0.6]) });
        file.generate.as_mut().unwrap().sea = 4;
        let map = file.make_ground(Some(&world));
        (file, world, map)
    }

    fn ground_of<'a>(
        file: &'a BlockFile,
        world: &'a WorldGen,
        map: &'a BlockMap,
        faces: &'a [BlockFaces],
    ) -> Ground<'a> {
        Ground {
            blocks: map,
            faces,
            block_size: file.block_size,
            smooth: false,
            generated: Some((file.generate.as_ref().unwrap(), world)),
        }
    }

    const COLUMNS: [[i32; 2]; 9] = [[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1], [0, 2], [1, 2], [2, 2]];

    #[test]
    fn plants_stand_on_the_blocks_their_rule_names_under_the_open_sky_the_same_every_time() {
        let (file, world, map) = hills();
        let (_, faces) = file.faces();
        let ground = ground_of(&file, &world, &map, &faces);
        let flowers = GrowRule { on: vec![1], spacing: 2.0, separation: 1.0, chance: 0.6, ..GrowRule::default() };
        let all: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&flowers, &ground, column)).collect();
        assert!(all.len() > 400, "{} flowers on 96 by 96 blocks", all.len());
        for flower in &all {
            let [x, y, z] = flower.ground;
            assert_eq!((map.get(flower.ground), flower.block), (1, 1), "on grass at {x}, {y}, {z}");
            assert!((y + 1..y + 40).all(|over| map.get([x, over, z]) == AIR), "open sky over {x}, {y}, {z}");
            // On the middle of its block, its foot on the block's top, turned by quarter turns.
            assert_eq!(flower.plant.position, [x as f32 + 0.5, (y + 1) as f32, z as f32 + 0.5]);
            let turns = flower.plant.yaw / std::f32::consts::FRAC_PI_2;
            assert!((turns - turns.round()).abs() < 1e-5 && (0.8..=1.2).contains(&flower.plant.scale));
        }
        // No two on one block, and none in the sea.
        let places: HashSet<[i32; 3]> = all.iter().map(|flower| flower.ground).collect();
        assert_eq!(places.len(), all.len());
        // The same again; another seed, other places; no chance, none.
        let again: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&flowers, &ground, column)).collect();
        assert_eq!(again, all);
        let other = GrowRule { seed: 1, ..flowers.clone() };
        assert_ne!(grow(&other, &ground, [1, 1]), grow(&flowers, &ground, [1, 1]));
        assert!(grow(&GrowRule { chance: 0.0, ..flowers.clone() }, &ground, [1, 1]).is_empty());
        // On stone alone, which lies under the ground here: nowhere.
        assert!(grow(&GrowRule { on: vec![3], ..flowers.clone() }, &ground, [1, 1]).is_empty());
        // In patches 12 m across: fewer, none where the ground between patches is, and some ground bare for
        // metres on end.
        let clumped = GrowRule { patches: 12.0, ..flowers.clone() };
        let clumps: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&clumped, &ground, column)).collect();
        assert!(clumps.len() > all.len() / 5 && clumps.len() < all.len() * 4 / 5, "{} of {}", clumps.len(), all.len());
        assert!(clumps.iter().all(|plant| all.contains(plant)), "each where the rule grows one anyway");
        let bare = |at: [i32; 2]| {
            let near = |plant: &Grown| (plant.ground[0] - at[0]).abs() < 4 && (plant.ground[2] - at[1]).abs() < 4;
            !clumps.iter().any(near) && all.iter().any(near)
        };
        assert!((0..12).any(|x| (0..12).any(|z| bare([x * 8 + 4, z * 8 + 4]))), "no bare ground between patches");
        // Not aligned, a plant stands anywhere on its block, turned any way.
        let loose = grow(&GrowRule { aligned: false, ..flowers }, &ground, [1, 1]);
        assert!(loose.iter().any(|plant| (plant.plant.position[0].fract() - 0.5).abs() > 0.1));
        for plant in &loose {
            let [x, _, z] = plant.plant.position;
            assert_eq!([x.floor() as i32, z.floor() as i32], [plant.ground[0], plant.ground[2]]);
        }
    }

    #[test]
    fn a_column_made_alone_grows_what_it_grows_among_its_neighbours() {
        let (file, world, map) = hills();
        let (_, faces) = file.faces();
        let trees = GrowRule { spacing: 6.0, separation: 3.0, chance: 0.7, slope: 20.0, ..GrowRule::default() };
        let among = grow(&trees, &ground_of(&file, &world, &map, &faces), [1, 1]);
        assert!(among.len() > 10, "{} trees in a column", among.len());
        let mut alone = BlockMap::default();
        for (key, chunk) in column_chunks(&file, file.generate.as_ref().unwrap(), &world, 0, [1, 1], Some([96, 96])) {
            alone.put_chunk(key, chunk);
        }
        assert_eq!(grow(&trees, &ground_of(&file, &world, &alone, &faces), [1, 1]), among);
        // Every tree of every column at least its rule's separation from the next, across columns' edges too.
        let ground = ground_of(&file, &world, &map, &faces);
        let loose = GrowRule { aligned: false, ..trees };
        let all: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&loose, &ground, column)).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let (dx, dz) = (a.plant.position[0] - b.plant.position[0], a.plant.position[2] - b.plant.position[2]);
                assert!((dx * dx + dz * dz).sqrt() >= 3.0 - 1e-3, "{:?} and {:?}", a.ground, b.ground);
            }
        }
    }

    #[test]
    fn plants_keep_to_the_climate_heights_and_slopes_their_rule_likes() {
        let (file, world, map) = hills();
        let (_, faces) = file.faces();
        let ground = ground_of(&file, &world, &map, &faces);
        let any = GrowRule { spacing: 2.0, separation: 1.0, chance: 1.0, slope: 89.0, ..GrowRule::default() };
        let everywhere: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&any, &ground, column)).collect();
        let climate = |plant: &Grown| {
            let [x, y, z] = plant.plant.position;
            world.climate(300.0 + x, -150.0 + z, y)
        };
        // Split where half of them are wetter: each half its own rule's, and together all of them.
        let mut wet: Vec<f32> = everywhere.iter().map(|plant| climate(plant).1).collect();
        wet.sort_by(f32::total_cmp);
        let middle = wet[wet.len() / 2];
        assert!(wet[0] < middle && middle < wet[wet.len() - 1], "humidity from {} to {}", wet[0], wet[wet.len() - 1]);
        let dry_rule = GrowRule { humidity: Some([0.0, middle]), ..any.clone() };
        let wet_rule = GrowRule { humidity: Some([middle + 1e-6, 1.0]), ..any.clone() };
        let dry: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&dry_rule, &ground, column)).collect();
        let wetter: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&wet_rule, &ground, column)).collect();
        assert!(dry.iter().all(|plant| climate(plant).1 <= middle) && !dry.is_empty());
        assert!(wetter.iter().all(|plant| climate(plant).1 > middle) && !wetter.is_empty());
        assert_eq!(dry.len() + wetter.len(), everywhere.len());
        // Nowhere this warm; and by height over the sea.
        let hot = GrowRule { temperature: Some([60.0, 90.0]), ..any.clone() };
        assert!(COLUMNS.iter().all(|&column| grow(&hot, &ground, column).is_empty()));
        let mut tops: Vec<f32> = everywhere.iter().map(|plant| plant.plant.position[1] - world.sea_level()).collect();
        tops.sort_by(f32::total_cmp);
        let half = tops[tops.len() / 2];
        let high_rule = GrowRule { height: Some([half, 10_000.0]), ..any.clone() };
        let high: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&high_rule, &ground, column)).collect();
        assert!(!high.is_empty() && high.len() < everywhere.len());
        assert!(high.iter().all(|plant| plant.plant.position[1] - world.sea_level() >= half));
        // On ground that rises no way alone: fewer, each with the blocks either side of it as high as each other.
        let level_rule = GrowRule { slope: 0.0, ..any };
        let level: Vec<Grown> = COLUMNS.iter().flat_map(|&column| grow(&level_rule, &ground, column)).collect();
        assert!(!level.is_empty() && level.len() < everywhere.len(), "{} of {}", level.len(), everywhere.len());
        let top = |x: i32, z: i32| (-16..200).rev().find(|&y| map.get([x, y, z]) != AIR);
        let inner = |v: i32| (1..31).contains(&v.rem_euclid(32));
        for plant in level.iter().filter(|plant| inner(plant.ground[0]) && inner(plant.ground[2])) {
            let [x, _, z] = plant.ground;
            assert_eq!(top(x - 1, z), top(x + 1, z), "level along x at {x}, {z}");
            assert_eq!(top(x, z - 1), top(x, z + 1), "level along z at {x}, {z}");
        }
    }

    #[test]
    fn a_plant_is_gone_once_its_ground_is_dug_or_built_over_and_back_when_the_ground_is() {
        // Flat ground with no generator: every block's top is grass, eight blocks up.
        let file = BlockFile::default();
        let (_, faces) = file.faces();
        let base = file.make_ground(None);
        let ground = Ground { blocks: &base, faces: &faces, block_size: 1.0, smooth: false, generated: None };
        let rule = GrowRule { spacing: 4.0, separation: 2.0, chance: 1.0, ..GrowRule::default() };
        let grown = grow(&rule, &ground, [0, 0]);
        assert_eq!(grown.len(), 64, "one a cell of four blocks, on 32 by 32");
        assert!(grown.iter().all(|plant| plant.ground[1] == 7 && plant.plant.position[1] == 8.0));
        assert!(gone(&grown, &base).is_empty());
        assert_eq!(chunks_under(&grown), [[0, 0, 0]]);
        // Dug from under the fourth, built over the tenth, dirt in place of the twentieth's grass.
        let mut now = base.clone();
        now.set(grown[3].ground, AIR);
        now.set(grown[9].over(), 3);
        now.set(grown[19].ground, 2);
        assert_eq!(gone(&grown, &now).iter().collect::<Vec<_>>(), [3, 9, 19]);
        // A block beside one changes nothing of it; the ground put back as it was, they are back.
        let [x, y, z] = grown[30].ground;
        now.set([x + 1, y + 1, z], 3);
        assert!(!gone(&grown, &now).contains(30));
        now.set(grown[3].ground, 1);
        now.set(grown[9].over(), AIR);
        now.set(grown[19].ground, 1);
        assert!(gone(&grown, &now).is_empty());
        // Nothing grows on what was built: plants are grown from the ground as it was made.
        assert_eq!(grow(&rule, &ground, [0, 0]), grown);
        // Drawn smooth, a plant's foot is where the surface crosses its block.
        let smooth = Ground { smooth: true, ..ground };
        let foot = grow(&rule, &smooth, [0, 0])[0].plant.position[1];
        assert!((7.0..=8.0).contains(&foot), "{foot}");
    }
}
