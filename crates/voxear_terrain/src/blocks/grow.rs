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
//! Past the world's own blocks, where only the coarser ones of a far ring
//! are made, the same plants are worked out from the generator's heights
//! ([`grow_far`]), so trees stand on far hills and are still there, each
//! where it was, when the ground under them is made.
//!
//! Further still, past where any plant is drawn, a rule's plants show as a
//! wood of blocks in the far view's tiles ([`Woods`]).
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
    edits::{BlockEdits, ChunkChange},
    file::BlockFile,
    generate::{BlockGenerate, far::FarTile},
    is_water,
    stream::{BlockMaking, FarBuilt},
};
use crate::generate::{
    WorldGen,
    structures::{Placement, Site},
};

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
    /// Whether it grows on the bed under water, as seaweed and coral do,
    /// in place of dry ground: its height is then how far under the
    /// water's top its bed lies, less than nothing.
    pub under_water: bool,
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
            under_water: false,
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
    /// The block over the one it stands on, as it grew: air, or the water
    /// of a plant of the sea's bed.
    pub above: BlockId,
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

/// The highest block of each place of one chunk column that is not air: how
/// high it is, which block, and how full. Read once from the column's
/// chunks, for every rule that grows on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tops {
    tops: Vec<Option<(i32, BlockId, u8)>>,
}

impl Tops {
    /// Those of chunk column `[cx, cz]` of `blocks`.
    pub fn of(blocks: &BlockMap, [cx, cz]: [i32; 2]) -> Self {
        let mut chunks: Vec<(i32, &Chunk)> = blocks
            .chunks()
            .filter(|(key, chunk)| key[0] == cx && key[2] == cz && !chunk.is_air())
            .map(|(key, chunk)| (key[1], &**chunk))
            .collect();
        chunks.sort_unstable_by_key(|(y, _)| std::cmp::Reverse(*y));
        let mut tops = vec![None; (CHUNK * CHUNK) as usize];
        // Chunk by chunk from the highest, each place's blocks from the top down, until every place has its own.
        let mut left = tops.len();
        for &(cy, chunk) in &chunks {
            for z in 0..CHUNK {
                for x in 0..CHUNK {
                    let top = &mut tops[(z * CHUNK + x) as usize];
                    if top.is_some() {
                        continue;
                    }
                    let mut y = CHUNK;
                    while y > 0 && chunk.get(x, y - 1, z) == AIR {
                        y -= 1;
                    }
                    if y > 0 {
                        *top =
                            Some((cy * CHUNK as i32 + y as i32 - 1, chunk.get(x, y - 1, z), chunk.fill(x, y - 1, z)));
                        left -= 1;
                    }
                }
            }
            if left == 0 {
                break;
            }
        }
        Self { tops }
    }

    /// Whether the column has no block at all.
    pub fn is_empty(&self) -> bool {
        self.tops.iter().all(Option::is_none)
    }

    /// The highest block of place `[x, z]` of the column's own.
    fn top(&self, [x, z]: [u32; 2]) -> Option<(i32, BlockId, u8)> {
        self.tops[(z * CHUNK + x) as usize]
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

/// One of a rule's tries in a chunk column: its site, the block it falls
/// on along x and z, and where it is in metres from the world's corner.
struct Try {
    site: Site,
    block: [i32; 2],
    at: [f32; 2],
}

/// A rule's tries that fall in chunk column `column` of a world whose
/// blocks are `size` metres, lying at `position` in a generator's world
/// with `seed`: the same from the near ground and the far.
fn tries(rule: &GrowRule, (seed, position): (u32, [f32; 2]), size: f32, column: [i32; 2]) -> Vec<Try> {
    let side = CHUNK as i32;
    // Every try that may fall in the column, a block further than its edges; kept where its block is the column's.
    let low = [0, 1].map(|axis| position[axis] + (column[axis] * side) as f32 * size);
    let high = low.map(|v| v + side as f32 * size);
    let patch = seed ^ rule.seed.wrapping_mul(0x9E37_79B9);
    let mut tries = Vec::new();
    for site in rule.placement().tries_near(seed, low, high, size) {
        let at = [site.at[0] - position[0], site.at[1] - position[1]];
        let block = at.map(|v| (v / size).floor() as i32);
        if block[0].div_euclid(side) != column[0] || block[1].div_euclid(side) != column[1] {
            continue;
        }
        // In patches: where a number that wanders over the ground is in its upper half.
        if rule.patches > 0.0 && patchy(patch, site.at, rule.patches.max(1.0)) < 0.5 {
            continue;
        }
        tries.push(Try { site, block, at });
    }
    tries
}

/// A try's plant with its foot `top` metres up: on the middle of its block
/// and turned by quarter turns where the rule aligns them, sized by its
/// site.
fn plant_of(rule: &GrowRule, found: &Try, size: f32, top: f32) -> Plant {
    let (x, z, yaw) = match rule.aligned {
        true => (
            (found.block[0] as f32 + 0.5) * size,
            (found.block[1] as f32 + 0.5) * size,
            f32::from(found.site.turns) * std::f32::consts::FRAC_PI_2,
        ),
        false => (found.at[0], found.at[1], unit(mixed(found.site.seed, 2)) * std::f32::consts::TAU),
    };
    let scale = rule.scale[0] + (rule.scale[1] - rule.scale[0]) * unit(mixed(found.site.seed, 1));
    Plant::new([x, top, z], yaw, scale)
}

/// Those of `grown` whose climate the rule likes, asked of `world` at once
/// where each stands, `feet` metres up, a world lying at `position` in it.
fn liked(rule: &GrowRule, world: Option<&WorldGen>, position: [f32; 2], grown: Vec<(f32, Grown)>) -> Vec<Grown> {
    let Some(world) = world.filter(|_| rule.temperature.is_some() || rule.humidity.is_some()) else {
        return grown.into_iter().map(|(_, grown)| grown).collect();
    };
    let xs: Vec<f32> = grown.iter().map(|(_, grown)| position[0] + grown.plant.position[0]).collect();
    let zs: Vec<f32> = grown.iter().map(|(_, grown)| position[1] + grown.plant.position[2]).collect();
    let heights: Vec<f32> = grown.iter().map(|(feet, _)| *feet).collect();
    let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
    let climates = warmth.into_iter().zip(wetness);
    grown.into_iter().zip(climates).filter(|(_, climate)| rule.likes(*climate)).map(|((_, grown), _)| grown).collect()
}

/// The plants `rule` grows on chunk column `column` of `ground`, in an order
/// that is always the same: a plant's place in it is its number. Each stands
/// on the highest block of its place, under the open sky, where that block
/// is ground the rule grows on.
pub fn grow(rule: &GrowRule, ground: &Ground, column: [i32; 2]) -> Vec<Grown> {
    grow_on(rule, ground, column, &Tops::of(ground.blocks, column))
}

/// As [`grow`], on the column's highest blocks read already (`Tops::of` its
/// column of `ground`'s blocks), as for one rule after another.
pub fn grow_on(rule: &GrowRule, ground: &Ground, column: [i32; 2], blocks: &Tops) -> Vec<Grown> {
    let size = ground.block_size.max(0.01);
    let side = CHUNK as i32;
    if blocks.is_empty() {
        return Vec::new();
    }
    let (position, world) = match ground.generated {
        Some((generate, world)) => (generate.position, Some(world)),
        None => ([0.0; 2], None),
    };
    let (seed, sea) = world.map_or((0, 0.0), |world| (world.seed(), world.sea_level()));
    let mut taken: HashSet<[i32; 2]> = HashSet::new();
    let mut grown = Vec::new();
    // The bed under the water at a place of the column: the highest block that is not water, with water over it.
    let bed = |[x, z]: [i32; 2]| {
        let inside = [x.rem_euclid(side) as u32, z.rem_euclid(side) as u32];
        let (top, water, _) = blocks.top(inside).filter(|(_, block, _)| is_water(ground.faces, *block))?;
        let mut y = top;
        loop {
            y -= 1;
            match ground.blocks.get([x, y, z]) {
                AIR => return None,
                block if is_water(ground.faces, block) => continue,
                block => return Some((y, block, top, water)),
            }
        }
    };
    for found in tries(rule, (seed, position), size, column) {
        let [bx, bz] = found.block;
        let inside = [bx.rem_euclid(side) as u32, bz.rem_euclid(side) as u32];
        if rule.under_water {
            let Some((y, block, water_top, water)) = bed([bx, bz]) else { continue };
            if !(rule.on.is_empty() || rule.on.contains(&block)) {
                continue;
            }
            // How its bed rises to the beds beside it, those in the column alone.
            let within = |v: i32, axis: usize| v.clamp(column[axis] * side, column[axis] * side + side - 1);
            let beside = |x: i32, z: i32| bed([within(x, 0), within(z, 1)]).map_or(y, |(y, ..)| y) as f32;
            let (dx, dz) = ((beside(bx + 1, bz) - beside(bx - 1, bz)) * 0.5, (beside(bx, bz + 1) - beside(bx, bz - 1)) * 0.5);
            let top = (y + 1) as f32 * size;
            let under = top - (water_top + 1) as f32 * size;
            if !rule.stands_on(under, (dx * dx + dz * dz).sqrt()) || rule.aligned && !taken.insert([bx, bz]) {
                continue;
            }
            let plant = plant_of(rule, &found, size, top);
            grown.push((top, Grown { plant, ground: [bx, y, bz], block, above: water }));
            continue;
        }
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
        grown.push((top, Grown { plant: plant_of(rule, &found, size, top), ground: [bx, y, bz], block, above: AIR }));
    }
    liked(rule, world, position, grown)
}

/// Generated ground too far off to have its own blocks: the world's file,
/// how it generates and its generator, from which its surface is known.
#[derive(Clone, Copy)]
pub struct FarGround<'a> {
    pub file: &'a BlockFile,
    pub generate: &'a BlockGenerate,
    pub world: &'a WorldGen,
}

/// The plants `rule` grows on chunk column `column` of ground that has no
/// blocks of its own made, only the coarser ones of a far ring: the plants
/// [`grow`] gives once its blocks are made, wherever nothing pushes or
/// hollows the ground, from the generator's heights at the same tries. Each
/// stands no higher than the ground of any ring out to level `levels` puts
/// its place, so none hangs over coarser ground: sunk a little into it at
/// worst, until its own blocks are made.
pub fn grow_far(rule: &GrowRule, ground: &FarGround, column: [i32; 2], levels: u32) -> Vec<Grown> {
    let size = ground.file.block_size.max(0.01);
    let side = CHUNK as i32;
    let position = ground.generate.position;
    let found = tries(rule, (ground.world.seed(), position), size, column);
    if found.is_empty() {
        return Vec::new();
    }
    // Each try's block and the four beside it, kept to the column's own as its blocks' slope is.
    let within = |v: i32, axis: usize| v.clamp(column[axis] * side, column[axis] * side + side - 1);
    let around = |[x, z]: [i32; 2]| {
        [[x, z], [within(x - 1, 0), z], [within(x + 1, 0), z], [x, within(z - 1, 1)], [x, within(z + 1, 1)]]
    };
    let middle = |block: i32, axis: usize, blocks: f32| position[axis] + (block as f32 + 0.5) * blocks;
    let places: Vec<[i32; 2]> = found.iter().flat_map(|found| around(found.block)).collect();
    let xs: Vec<f32> = places.iter().map(|place| middle(place[0], 0, size)).collect();
    let zs: Vec<f32> = places.iter().map(|place| middle(place[1], 1, size)).collect();
    let surfaces = super::generate::surfaces(ground.file, ground.generate, ground.world, &xs, &zs);
    // The highest block of a place, as its own blocks would have it: the ground's, or the sea's over it.
    let ground_top = |at: usize| (surfaces.heights[at] / size - 0.5).ceil() as i32 - 1;
    let sea_top = surfaces.sea.map(|sea| sea.ceil() as i32 - 1);
    let top = |at: usize| ground_top(at).max(sea_top.unwrap_or(i32::MIN));
    // The ground of each coarser ring under each try, where that ring's blocks have their middles.
    let mut lowest: Vec<f32> = (0..found.len()).map(|at| (ground_top(at * 5) + 1) as f32 * size).collect();
    for level in 1..=levels.min(super::stream::MOST_LEVELS - 1) {
        let coarse = size * (1u32 << level) as f32;
        let block = |v: i32| v.div_euclid(1 << level);
        let xs: Vec<f32> = found.iter().map(|found| middle(block(found.block[0]), 0, coarse)).collect();
        let zs: Vec<f32> = found.iter().map(|found| middle(block(found.block[1]), 1, coarse)).collect();
        let mut heights = vec![0.0; xs.len()];
        ground.world.heights(&xs, &zs, &mut heights);
        for (lowest, height) in lowest.iter_mut().zip(heights) {
            *lowest = lowest.min((height / coarse - 0.5).ceil() * coarse);
        }
    }
    let mut taken: HashSet<[i32; 2]> = HashSet::new();
    let mut grown = Vec::new();
    for (at, found) in found.iter().enumerate() {
        let [bx, bz] = found.block;
        let (y, block) = (ground_top(at * 5), surfaces.tops[at * 5]);
        // Under the sea, or no ground at all; a plant of the sea's bed, only under the sea.
        let under_sea = sea_top.is_some_and(|sea| sea > y);
        if under_sea != rule.under_water || block == AIR {
            continue;
        }
        if !(rule.on.is_empty() || rule.on.contains(&block)) {
            continue;
        }
        let inside = [bx.rem_euclid(side), bz.rem_euclid(side)];
        let last = side - 1;
        // Under the sea, its bed's slope, not the flat sea's over it.
        let top = |at: usize| if rule.under_water { ground_top(at) } else { top(at) };
        let along = |low: usize, high: usize, both: bool| (top(high) - top(low)) as f32 / if both { 2.0 } else { 1.0 };
        let dx = along(at * 5 + 1, at * 5 + 2, inside[0] > 0 && inside[0] < last);
        let dz = along(at * 5 + 3, at * 5 + 4, inside[1] > 0 && inside[1] < last);
        let feet = match ground.file.smooth {
            true => surfaces.heights[at * 5],
            false => (y + 1) as f32 * size,
        };
        if !rule.stands_on(feet - ground.world.sea_level(), (dx * dx + dz * dz).sqrt()) {
            continue;
        }
        if rule.aligned && !taken.insert([bx, bz]) {
            continue;
        }
        let stood = if ground.file.smooth { feet } else { lowest[at] };
        let above = if rule.under_water { ground.generate.sea } else { AIR };
        grown.push((feet, Grown { plant: plant_of(rule, found, size, stood), ground: [bx, y, bz], block, above }));
    }
    liked(rule, Some(ground.world), position, grown)
}

/// A rule's plants as the far view shows them, too far off for plants of
/// their own: a wood as blocks. Where a far tile's column is ground the
/// rule grows on, `block` lies over it as high as the plants stand; or, in
/// a ring whose blocks are more than twice as high as they are, in place of
/// its top block, so far woods are at least their colour. As many such
/// columns are wooded as would hold a plant, so a thin wood shows gaps. Only plants that
/// stand thick enough to close over the ground show: a lone tree to a
/// field is no wood.
#[derive(Clone, Debug, PartialEq)]
pub struct Woods {
    pub rule: GrowRule,
    /// The block they show as: leaves, say.
    pub block: BlockId,
    /// How tall a plant stands and how far across it is, in metres, at
    /// its middle size.
    pub height: f32,
    pub width: f32,
    /// The first ring it shows in, by its level: the first past the ones
    /// its plants themselves are drawn on (see [`Woods::first_past`]).
    pub from: u32,
}

/// How much of the ground a rule's plants must cover between them, each
/// its width across, to show far off as a wood.
const WOODED: f32 = 0.08;

impl Woods {
    /// The first ring a wood shows in where its plants are drawn out to
    /// `reach` metres in a world whose own blocks reach `distance`: the
    /// first whose inner edge the plants reach with a quarter to spare, so
    /// plants stand on every ring inside it and the wood begins where they
    /// end, with no bare ground between and none wooded twice.
    pub fn first_past(reach: f32, distance: f32) -> u32 {
        let rings = (reach / (1.25 * distance.max(1.0))).log2().floor();
        (rings.max(0.0) as u32 + 1).min(super::stream::MOST_LEVELS - 1)
    }

    /// Whether its plants stand thick enough to show as a wood.
    pub fn shows(&self) -> bool {
        let spacing = self.rule.spacing.max(1.0);
        let crown = std::f32::consts::FRAC_PI_4 * self.width * self.width;
        self.block != AIR && self.rule.chance.clamp(0.0, 1.0) * crown / (spacing * spacing) >= WOODED
    }
}

impl FarBuilt for Woods {
    fn stamp(&self) -> u64 {
        crate::layers::stable_hash(format!("{self:?}").as_bytes())
    }

    fn from_level(&self) -> u32 {
        self.from
    }

    fn build(&self, making: &BlockMaking, tile: &mut FarTile) {
        if !self.shows() {
            return;
        }
        let rule = &self.rule;
        let side = CHUNK;
        let size = making.file.block_size.max(0.01) * (1u32 << tile.level.min(16)) as f32;
        let position = making.generate.position;
        let (sea, world) = (tile.sea.0, &*making.world);
        // The top of each column's ground, in the tile's blocks, and its block: none under the sea.
        let ground = |x: u32, z: u32| {
            let top = tile.column([x, z]).last()?;
            (top.block != AIR && (sea == AIR || top.block != sea)).then_some((top.to, top.block))
        };
        let patch = world.seed() ^ rule.seed.wrapping_mul(0x9E37_79B9);
        // How many plants a column of the tile holds, where its ground grows them: one at the most counts.
        let spacing = rule.spacing.max(1.0);
        let holds = rule.chance.clamp(0.0, 1.0) * size * size / (spacing * spacing);
        let mut wooded: Vec<([u32; 2], i32)> = Vec::new();
        let (mut xs, mut zs, mut heights) = (Vec::new(), Vec::new(), Vec::new());
        for z in 0..side {
            for x in 0..side {
                let Some((top, block)) = ground(x, z) else { continue };
                if !(rule.on.is_empty() || rule.on.contains(&block)) {
                    continue;
                }
                // How the ground rises there, from the columns beside it, the tile's own.
                let beside = |x: u32, z: u32| ground(x, z).map_or(top, |(top, _)| top) as f32;
                let along = |low: f32, high: f32, both: bool| (high - low) / if both { 2.0 } else { 1.0 };
                let last = side - 1;
                let dx = along(beside(x.saturating_sub(1), z), beside((x + 1).min(last), z), x > 0 && x < last);
                let dz = along(beside(x, z.saturating_sub(1)), beside(x, (z + 1).min(last)), z > 0 && z < last);
                let height = top as f32 * size;
                if !rule.stands_on(height - world.sea_level(), (dx * dx + dz * dz).sqrt()) {
                    continue;
                }
                let middle = |cell: u32, axis: usize| {
                    position[axis] + ((tile.at[axis] * side as i32 + cell as i32) as f32 + 0.5) * size
                };
                let place = [middle(x, 0), middle(z, 1)];
                if rule.patches > 0.0 && patchy(patch, place, rule.patches.max(1.0)) < 0.5 {
                    continue;
                }
                // As many columns wooded as would hold a plant: a thin wood is a broken one far off too.
                let cell = [tile.at[0] * side as i32 + x as i32, tile.at[1] * side as i32 + z as i32];
                let drawn = mixed(mixed(patch ^ cell[0] as u32, tile.level + 0x60) ^ cell[1] as u32, 0x61);
                if unit(drawn) >= holds {
                    continue;
                }
                wooded.push(([x, z], top));
                xs.push(place[0]);
                zs.push(place[1]);
                heights.push(height);
            }
        }
        // Their climate, asked of the generator at once.
        let mut liked = vec![true; wooded.len()];
        if rule.temperature.is_some() || rule.humidity.is_some() {
            let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
            world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
            liked = warmth.into_iter().zip(wetness).map(|climate| rule.likes(climate)).collect();
        }
        // As high as the plants stand, in whole blocks; in blocks far higher than they are, the top block's place.
        let tall = (self.height / size).round() as i32;
        for ((column, top), _) in wooded.into_iter().zip(liked).filter(|(_, liked)| *liked) {
            match tall >= 1 {
                true => tile.add(column, top, top + tall, self.block),
                false => tile.add(column, top - 1, top, self.block),
            }
        }
    }
}

/// The block at `at` as changes kept for ground not made now have it: none
/// where they change nothing of it.
fn kept_change(edits: &BlockEdits, at: [i32; 3]) -> Option<BlockId> {
    let (key, [x, y, z]) = BlockMap::chunk_of(at);
    let index = ((y * CHUNK + z) * CHUNK + x) as u16;
    match edits.chunks.get(&key)? {
        ChunkChange::Blocks(changed) => changed.iter().find(|(place, _)| *place == index).map(|(_, block)| *block),
        ChunkChange::Cells(changed) => changed.iter().find(|(place, ..)| *place == index).map(|(_, block, _)| *block),
        ChunkChange::Whole(chunk) => Some(chunk.get(x, y, z)),
    }
}

/// Which of `grown`, on ground not made now, are gone by the changes kept
/// for that ground (see `stream`): those whose block was dug or changed, or
/// built over, before the ground was let go.
pub fn gone_far(grown: &[Grown], kept: &BlockEdits) -> PlantSet {
    if kept.chunks.is_empty() {
        return PlantSet::default();
    }
    let stands = |plant: &Grown| {
        kept_change(kept, plant.ground).is_none_or(|block| block == plant.block)
            && kept_change(kept, plant.over()).is_none_or(|block| block == plant.above)
    };
    grown.iter().enumerate().filter(|(_, plant)| !stands(plant)).map(|(number, _)| number as u32).collect()
}

/// Which of `grown` are gone as `blocks` stand now, by their numbers: those
/// whose block is no longer the one they grew on, or has a block over it.
pub fn gone(grown: &[Grown], blocks: &BlockMap) -> PlantSet {
    let stands = |plant: &Grown| blocks.get(plant.ground) == plant.block && blocks.get(plant.over()) == plant.above;
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
    fn plants_of_the_seas_bed_grow_under_water_as_deep_as_their_rule_says_and_go_with_their_bed_or_their_water() {
        // Flat ground 12 blocks up with a pool in it, its bed stepping down from 1 block deep in the west to 8.
        let mut file = BlockFile { ground: FlatGround { size: [32, 32], height: 12, ..FlatGround::default() }, ..BlockFile::default() };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.1, 0.3, 0.6]) });
        let water = file.blocks.len() as BlockId;
        let mut map = file.make_ground(None);
        for z in 4..28 {
            for x in 4..28 {
                let deep = 1 + (x - 4) / 3;
                for y in (12 - deep)..12 {
                    map.set([x, y, z], water);
                }
            }
        }
        let (_, faces) = file.faces();
        let ground = Ground { blocks: &map, faces: &faces, block_size: 1.0, smooth: false, generated: None };
        let kelp = GrowRule {
            under_water: true,
            height: Some([-20.0, -4.0]),
            spacing: 1.0,
            separation: 0.0,
            chance: 1.0,
            slope: 60.0,
            ..GrowRule::default()
        };
        let grown = grow(&kelp, &ground, [0, 0]);
        assert!(grown.len() > 100, "{} plants on the bed", grown.len());
        for plant in &grown {
            let [x, y, z] = plant.ground;
            assert!((4..28).contains(&x) && (4..28).contains(&z), "in the pool: {x}, {z}");
            assert!(12 - (y + 1) >= 4, "four blocks or more under the water's top: {}", 12 - (y + 1));
            assert_eq!((plant.above, map.get(plant.over())), (water, water));
            assert_eq!(plant.plant.position[1], (y + 1) as f32, "standing on its bed");
            assert!(!is_water(&faces, plant.block) && plant.block != AIR);
        }
        assert_eq!(grown, grow(&kelp, &ground, [0, 0]), "the same every time");
        // Plants of dry ground keep to it, and those of the bed grow on none of it.
        let grass = GrowRule { under_water: false, ..kelp.clone() };
        let dry = grow(&GrowRule { height: None, ..grass }, &ground, [0, 0]);
        assert!(!dry.is_empty() && dry.iter().all(|plant| plant.above == AIR && map.get(plant.over()) == AIR));
        assert!(dry.iter().all(|plant| !(4..28).contains(&plant.ground[0]) || !(4..28).contains(&plant.ground[2])));
        // Its bed dug away, or its water gone from over it, and a plant is gone.
        assert!(gone(&grown, &map).is_empty());
        let (first, second) = (grown[0], grown[1]);
        map.set(first.ground, AIR);
        map.set(second.over(), AIR);
        let lost: Vec<u32> = gone(&grown, &map).iter().collect();
        assert_eq!(lost, vec![0, 1]);
    }

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
    fn ground_too_far_off_for_blocks_of_its_own_grows_the_plants_it_grows_once_they_are_made() {
        let (file, world, map) = hills();
        let (_, faces) = file.faces();
        let near = ground_of(&file, &world, &map, &faces);
        let far = FarGround { file: &file, generate: file.generate.as_ref().unwrap(), world: &world };
        let rules = [
            GrowRule { on: vec![1], spacing: 2.0, separation: 1.0, chance: 0.6, ..GrowRule::default() },
            GrowRule { spacing: 6.0, separation: 3.0, slope: 12.0, humidity: Some([0.3, 1.0]), ..GrowRule::default() },
            GrowRule { aligned: false, patches: 10.0, height: Some([1.0, 500.0]), seed: 5, ..GrowRule::default() },
        ];
        let mut grown = 0;
        for rule in &rules {
            for column in COLUMNS {
                // Plant for plant what its own blocks grow: where, how turned, how large, and on which block.
                let (made, far_off) = (grow(rule, &near, column), grow_far(rule, &far, column, 0));
                assert_eq!(far_off, made, "column {column:?}");
                grown += made.len();
                // Among the coarser blocks of rings out to the third, each stands where it did, no higher than
                // its own ground and no more than a coarse block and a half under it.
                let sunk = grow_far(rule, &far, column, 2);
                assert_eq!(sunk.len(), made.len());
                for (far_off, made) in sunk.iter().zip(&made) {
                    let (low, high) = (far_off.plant.position, made.plant.position);
                    assert_eq!(
                        ([low[0], low[2]], far_off.ground, far_off.block),
                        ([high[0], high[2]], made.ground, made.block)
                    );
                    assert!(low[1] <= high[1] && low[1] >= high[1] - 6.0, "{} for {}", low[1], high[1]);
                }
            }
        }
        assert!(grown > 500, "{grown} plants");
        // Changes kept for ground let go: dug from under the first, built over the second, a block beside the third.
        let plants = grow_far(&rules[0], &far, [1, 1], 2);
        let mut now = map.clone();
        now.set(plants[0].ground, AIR);
        now.set(plants[1].over(), 3);
        let [x, y, z] = plants[2].ground;
        now.set([x, y + 1, z + 1], 3);
        let kept = BlockEdits::between(&map, &now);
        assert_eq!(gone_far(&plants, &kept).iter().collect::<Vec<_>>(), [0, 1]);
        assert!(gone_far(&plants, &BlockEdits::default()).is_empty());
    }

    #[test]
    fn a_wood_shows_in_the_far_view_as_blocks_over_the_ground_its_trees_grow_on() {
        use crate::blocks::generate::far::far_tile;
        use std::sync::Arc;
        let (file, world, _) = hills();
        let (_, faces) = file.faces();
        let making = BlockMaking {
            generate: file.generate.clone().unwrap(),
            file: file.clone(),
            world: Arc::new(world),
            faces: faces.into(),
            far_folder: None,
            far_built: Vec::new(),
        };
        let trees = GrowRule { on: vec![1], spacing: 7.0, separation: 4.0, chance: 0.6, ..GrowRule::default() };
        let wood = Woods { rule: trees.clone(), block: 2, height: 6.5, width: 4.5, from: 3 };
        assert!(wood.shows());
        // Trees drawn to 500 m in a world of 128 m stand on its first two rings, and the wood begins with the
        // third; drawn no further than the world's own blocks, with the first ring.
        assert_eq!(
            [500.0, 400.0, 300.0, 128.0, 60.0, 700.0].map(|reach| Woods::first_past(reach, 128.0)),
            [2, 2, 1, 1, 1, 3]
        );
        // A tile of the fourth ring, its blocks 8 m: a block of the wood over every column of grass above the sea.
        let bare = far_tile(&making.file, &making.generate, &making.world, 3, [0, 0]);
        let mut tile = bare.clone();
        wood.build(&making, &mut tile);
        let (mut wooded, mut open) = (0, 0);
        for z in 0..32 {
            for x in 0..32 {
                let (was, now) = (bare.column([x, z]), tile.column([x, z]));
                let top = was.last().unwrap();
                if top.block == 1 {
                    let over = now.last().unwrap();
                    let grown = (over.block, over.from, over.to) == (2, top.to, top.to + 1);
                    assert!(grown || now == was, "a column of grass with or without its wood at {x}, {z}");
                    wooded += usize::from(grown);
                } else {
                    assert_eq!(now, was, "nothing over the sea or bare ground at {x}, {z}");
                    open += 1;
                }
            }
        }
        assert!(wooded > 200, "{wooded} columns wooded, {open} not grass");
        // In blocks of 32 m the wood is the top block's colour, the ground no higher.
        let far_bare = far_tile(&making.file, &making.generate, &making.world, 5, [0, 0]);
        let mut far_off = far_bare.clone();
        wood.build(&making, &mut far_off);
        let recoloured = (0..32u32).flat_map(|z| (0..32u32).map(move |x| [x, z])).filter(|at| {
            let (was, now) = (far_bare.column(*at).last().unwrap(), far_off.column(*at).last().unwrap());
            (now.block, now.to) == (2, was.to) && was.block == 1
        });
        assert!(recoloured.count() > 50);
        // A rule that likes no climate here, one with trees too few to close over the ground, and a wood with no
        // block: nothing built; and each another stamp, so tiles are built on again when a rule changes.
        let cold = Woods { rule: GrowRule { temperature: Some([-80.0, -60.0]), ..trees.clone() }, ..wood.clone() };
        let lone = Woods { rule: GrowRule { spacing: 60.0, ..trees.clone() }, ..wood.clone() };
        let none = Woods { block: AIR, ..wood.clone() };
        for other in [&cold, &lone, &none] {
            let mut tile = bare.clone();
            other.build(&making, &mut tile);
            assert!(tile == bare && other.stamp() != wood.stamp());
        }
        assert!(!lone.shows() && !none.shows() && cold.shows());
        assert_eq!(wood.clone().stamp(), wood.stamp());
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
