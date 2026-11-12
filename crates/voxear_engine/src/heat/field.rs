//! How hot or cold it is at a point, and what makes it so.
//!
//! A place's temperature is its climate, warmer in the afternoon and colder
//! in the small hours, drawn towards the ground's own milder temperature
//! where blocks shelter it, or the water's where it is in water. Heat Zones
//! then set or add theirs, and hot and cold things near it, Heat Sources,
//! short-lived heat, hot and cold blocks and lava, warm or cool it: the
//! hottest of them and the coldest, each faded by how far away it is.
//!
//! Everything here is sums and products, the same bits on every machine.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::Liquid;
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::{
    blocks::{
        BlockWorld,
        block::MAX_HEAT_REACH,
        chunk::{BlockId, CHUNK, Chunk},
        is_ground, is_water,
    },
    generate::WorldGen,
};

use super::parts::{Climate, HeatSource, HeatZone, ZoneMode, ZoneShape};
use crate::blocks::BlockWorldAsset;

/// How hot lava is, Pools and Streams of it.
pub const LAVA: f32 = 1100.0;
/// How far a Pool or Stream of lava warms what is over it, in metres.
const LAVA_REACH: f32 = 4.0;
/// How far up blocks are looked for, to shelter a place, in metres.
const SHELTER_REACH: f32 = 32.0;
/// How much rock over a place shelters it wholly, in metres.
const SHELTER_DEPTH: f32 = 6.0;
/// What the temperature is lapsed by with height where no generator says,
/// in °C a metre.
const LAPSE: f32 = 0.0065;
/// The temperature deep ground keeps all year, which caves are drawn to.
const DEEP: f32 = 10.0;

/// A place's temperature, and what it is made of: `celsius` is the sum of
/// the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeatSample {
    pub celsius: f32,
    /// The climate's, there.
    pub climate: f32,
    /// What the time of day adds.
    pub day: f32,
    /// How sheltered it is, from 0 in the open to 1 deep under ground.
    pub shelter: f32,
    /// What shelter, or water, adds.
    pub sheltered: f32,
    /// What Heat Zones add.
    pub zones: f32,
    /// What hot and cold things near add.
    pub sources: f32,
    /// How wet it is, from 0 to 1.
    pub humidity: f32,
}

impl HeatSample {
    /// Its numbers as scripts read them, in this order.
    pub fn to_array(&self) -> [f32; 8] {
        [self.celsius, self.climate, self.day, self.shelter, self.sheltered, self.zones, self.sources, self.humidity]
    }
}

/// Heat set off for a while: `celsius` at `at`, fading by `radius` metres,
/// until the game's time is `until`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    pub at: Vec3,
    pub celsius: f32,
    pub radius: f32,
    pub until: f64,
}

/// What heat keeps between frames, as a world resource: each world of
/// blocks' generator, its hot and cold blocks by chunk, and heat set off
/// for a while.
#[derive(Default)]
pub struct HeatWorld {
    /// Each world of blocks' generator, and where its corner is in it.
    pub(crate) generators: HashMap<Entity, (Arc<WorldGen>, [f32; 2])>,
    hot: HashMap<Entity, HotBlocks>,
    pub(crate) bursts: Vec<Burst>,
    /// The game's time, for what is set off for a while.
    pub(crate) now: f64,
}

/// One world's hot and cold blocks, by chunk, each list kept with the
/// chunk it was read from: a chunk changed is another, read again.
#[derive(Default)]
struct HotBlocks {
    chunks: HashMap<[i32; 3], (Arc<Chunk>, Arc<[(u32, BlockId)]>)>,
}

/// The level's first Climate, which holds its clock.
pub fn climate(world: &World) -> Option<Climate> {
    let mut found: Vec<(Entity, Climate)> =
        world.query_ref::<(Entity, &Climate)>().without::<Destroying>().into_iter().map(|(e, c)| (e, *c)).collect();
    found.sort_by_key(|(entity, _)| entity.to_bits());
    found.first().map(|(_, climate)| *climate)
}

/// Where the day is, from -1 in the small hours, at five, to 1 in the
/// afternoon, at three, rising and falling smoothly between.
pub fn day_curve(hours: f32) -> f32 {
    let since = (hours - 5.0).rem_euclid(24.0);
    if since < 10.0 { -1.0 + 2.0 * smooth(since / 10.0) } else { 1.0 - 2.0 * smooth((since - 10.0) / 14.0) }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How much of something `radius` across reaches `distance` away: all of
/// it at nothing, none at `radius`.
fn falloff(distance: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return if distance <= 0.0 { 1.0 } else { 0.0 };
    }
    1.0 - smooth(distance / radius)
}

/// A world of blocks to sample, with its corner.
struct Blocks<'a> {
    entity: Entity,
    blocks: &'a BlockWorld,
    asset: Option<&'a BlockWorldAsset>,
    corner: Vec3,
}

fn block_worlds(world: &World) -> Vec<Blocks<'_>> {
    let mut found: Vec<Blocks> = world
        .query_ref::<(Entity, &BlockWorld, Option<&BlockWorldAsset>, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, blocks, asset, local, placed)| Blocks {
            entity,
            blocks,
            asset,
            corner: voxear_terrain::corner(local, placed),
        })
        .collect();
    found.sort_by_key(|blocks| blocks.entity.to_bits());
    found
}

/// How hot or cold it is at `at`, and what makes it so.
pub fn sample(world: &mut World, at: Vec3) -> HeatSample {
    let mut heat = world.remove_resource::<HeatWorld>().unwrap_or_default();
    let sample = sample_with(world, &mut heat, at);
    world.insert_resource(heat);
    sample
}

pub(crate) fn sample_with(world: &World, heat: &mut HeatWorld, at: Vec3) -> HeatSample {
    let level = climate(world);
    let worlds = block_worlds(world);

    // The climate: the generator's of the first world of blocks reaching here, else the level's.
    let generated = worlds.iter().find_map(|blocks| {
        let (generator, offset) = heat.generators.get(&blocks.entity)?;
        let local = at - blocks.corner;
        let asset = blocks.asset?;
        let endless = asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
        let size = asset.file.ground.size.map(|blocks| blocks as f32 * asset.file.block_size);
        let within = (0.0..size[0]).contains(&local.x) && (0.0..size[1]).contains(&local.z);
        (endless || within).then(|| generator.climate(local.x + offset[0], local.z + offset[1], local.y))
    });
    let (climate, humidity) = generated.unwrap_or_else(|| {
        let level = level.unwrap_or_default();
        (level.celsius - LAPSE * at.y, level.humidity.clamp(0.0, 1.0))
    });
    let humidity = humidity.clamp(0.0, 1.0);

    // The day: none without a clock, and wider where it is dry.
    let day = level.map_or(0.0, |level| level.day_swing * (1.5 - humidity) * day_curve(level.time_of_day));
    let open = climate + day;

    // Shelter, and water: drawn to the ground's own temperature, or the water's.
    let ground = DEEP + (climate - DEEP) * 0.35;
    let water = (DEEP + (climate - DEEP) * 0.5).max(0.5);
    let (shelter, wet) = shelter_at(&worlds, at);
    let water_there = crate::water::sample(world, at);
    let liquid = water_there.filter(|point| point.height > 0.0).map(|point| point.liquid);
    let sheltered = if wet || liquid == Some(Liquid::Water) { water - open } else { (ground - open) * shelter };
    let mut celsius = open + sheltered;

    // Zones, in the order they were made.
    let before = celsius;
    for (zone, place) in zones(world) {
        let weight = zone_weight(&zone, place, at);
        if weight <= 0.0 {
            continue;
        }
        celsius = match zone.mode {
            ZoneMode::Set => celsius + (zone.celsius - celsius) * weight,
            ZoneMode::Add => celsius + zone.celsius * weight,
        };
    }
    let zoned = celsius - before;

    // Hot and cold things near: the hottest and the coldest of them count.
    let (mut hottest, mut coldest) = (0.0f32, 0.0f32);
    let mut warm = |celsius_there: f32, weight: f32| {
        let by = (celsius_there - celsius) * weight;
        hottest = hottest.max(by);
        coldest = coldest.min(by);
    };
    let mut sources: Vec<(Entity, HeatSource, Vec3)> = world
        .query_ref::<(Entity, &HeatSource, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, source, local, placed)| {
            (entity, *source, placed.map_or(local.position, |w| w.0.w_axis.truncate()))
        })
        .collect();
    sources.sort_by_key(|(entity, ..)| entity.to_bits());
    for (_, source, place) in sources {
        warm(source.celsius, falloff(place.distance(at), source.radius));
    }
    let now = heat.now;
    for burst in heat.bursts.iter().filter(|burst| burst.until > now) {
        warm(burst.celsius, falloff(burst.at.distance(at), burst.radius));
    }
    if let Some(point) = water_there.filter(|point| point.liquid == Liquid::Lava) {
        let distance = (-point.height).max(0.0);
        warm(LAVA, falloff(distance, LAVA_REACH));
    }
    for blocks in &worlds {
        let Some(asset) = blocks.asset.filter(|asset| asset.drawn.has_heat()) else { continue };
        let hot = heat.hot.entry(blocks.entity).or_default();
        hot_blocks(hot, blocks.blocks, asset, at - blocks.corner, &mut warm);
    }
    // What is no longer there keeps nothing.
    let alive: Vec<Entity> = worlds.iter().map(|blocks| blocks.entity).collect();
    heat.hot.retain(|entity, _| alive.contains(entity));

    let added = hottest + coldest;
    HeatSample { celsius: celsius + added, climate, day, shelter, sheltered, zones: zoned, sources: added, humidity }
}

/// How sheltered `at` is by blocks over it, from 0 to 1, and whether it is
/// in a world of blocks' water.
fn shelter_at(worlds: &[Blocks], at: Vec3) -> (f32, bool) {
    let mut most = 0.0f32;
    for blocks in worlds {
        let world = blocks.blocks;
        let size = world.block_size.max(0.01);
        let local = at - blocks.corner;
        let (block, here) = world.block_at(local.to_array());
        if is_water(&world.faces, here) {
            return (0.0, true);
        }
        let steps = (SHELTER_REACH / size).ceil() as i32;
        // Straight up, and four ways slanting up: how many meet ground.
        let ways = [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]];
        let mut met = 0;
        let mut rock = 0;
        for (index, [dx, dz]) in ways.into_iter().enumerate() {
            let mut hit = false;
            for step in 1..=steps {
                let there = [block[0] + dx * step, block[1] + step, block[2] + dz * step];
                if is_ground(&world.faces, world.blocks.get(there)) {
                    hit = true;
                    if index == 0 {
                        rock += 1;
                    } else {
                        break;
                    }
                }
            }
            met += i32::from(hit);
        }
        let open = met as f32 / ways.len() as f32;
        let deep = (rock as f32 * size / SHELTER_DEPTH).min(1.0);
        most = most.max(open * deep);
    }
    (most, false)
}

/// Every Heat Zone, where it is and how it is turned, in the order they
/// were made.
fn zones(world: &World) -> Vec<(HeatZone, (Vec3, Quat))> {
    let mut found: Vec<(Entity, HeatZone, (Vec3, Quat))> = world
        .query_ref::<(Entity, &HeatZone, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, zone, local, placed)| {
            let place = placed.map_or((local.position, local.rotation), |w| {
                let (_, rotation, position) = w.0.to_scale_rotation_translation();
                (position, rotation)
            });
            (entity, *zone, place)
        })
        .collect();
    found.sort_by_key(|(entity, ..)| entity.to_bits());
    found.into_iter().map(|(_, zone, place)| (zone, place)).collect()
}

/// How much of a zone's heat reaches `at`: all of it `edge` metres inside,
/// none outside.
fn zone_weight(zone: &HeatZone, (position, rotation): (Vec3, Quat), at: Vec3) -> f32 {
    let local = rotation.inverse() * (at - position);
    let inside = match zone.shape {
        ZoneShape::Sphere => zone.size[0].max(0.0) * 0.5 - local.length(),
        ZoneShape::Box => {
            let half = Vec3::from(zone.size).max(Vec3::ZERO) * 0.5;
            let past = local.abs() - half;
            -(past.max(Vec3::ZERO).length() + past.max_element().min(0.0))
        }
    };
    if inside < 0.0 {
        return 0.0;
    }
    if zone.edge <= 0.0 { 1.0 } else { smooth(inside / zone.edge) }
}

/// Warms or cools by every hot or cold block of `blocks` within its reach
/// of `local`, a point in metres from the world's corner.
fn hot_blocks(
    hot: &mut HotBlocks,
    blocks: &BlockWorld,
    asset: &BlockWorldAsset,
    local: Vec3,
    warm: &mut impl FnMut(f32, f32),
) {
    let size = blocks.block_size.max(0.01);
    let drawn = &asset.drawn;
    let reach = drawn.heat_reach().min(MAX_HEAT_REACH);
    if reach <= 0.0 {
        return;
    }
    let side = CHUNK as f32 * size;
    let low = ((local - Vec3::splat(reach)) / side).floor();
    let high = ((local + Vec3::splat(reach)) / side).floor();
    for cz in low.z as i32..=high.z as i32 {
        for cy in low.y as i32..=high.y as i32 {
            for cx in low.x as i32..=high.x as i32 {
                let key = [cx, cy, cz];
                let Some(chunk) = blocks.blocks.chunk(key) else { continue };
                let listed = listed(hot, key, chunk, drawn);
                for &(index, block) in listed.iter() {
                    let Some(heat) = drawn.heat(block) else { continue };
                    let (x, y, z) = (index % CHUNK, (index / CHUNK) % CHUNK, index / (CHUNK * CHUNK));
                    let least = Vec3::new(
                        (cx * CHUNK as i32 + x as i32) as f32,
                        (cy * CHUNK as i32 + y as i32) as f32,
                        (cz * CHUNK as i32 + z as i32) as f32,
                    ) * size;
                    let nearest = local.clamp(least, least + Vec3::splat(size));
                    warm(heat.celsius, falloff(nearest.distance(local), heat.reach));
                }
            }
        }
    }
}

/// A chunk's hot and cold blocks, by their place in it, read again only
/// once the chunk has changed.
fn listed(
    hot: &mut HotBlocks,
    key: [i32; 3],
    chunk: &Arc<Chunk>,
    drawn: &crate::blocks::Resolved,
) -> Arc<[(u32, BlockId)]> {
    if let Some((kept, list)) = hot.chunks.get(&key)
        && Arc::ptr_eq(kept, chunk)
    {
        return list.clone();
    }
    let has = |block: BlockId| drawn.heat(block).is_some();
    let list: Arc<[(u32, BlockId)]> = if !chunk.may_hold(has) {
        Arc::from([])
    } else {
        let mut found = Vec::new();
        for z in 0..CHUNK {
            for y in 0..CHUNK {
                for x in 0..CHUNK {
                    let block = chunk.get(x, y, z);
                    if has(block) {
                        found.push((x + y * CHUNK + z * CHUNK * CHUNK, block));
                    }
                }
            }
        }
        found.into()
    };
    hot.chunks.insert(key, (chunk.clone(), list.clone()));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_day_is_coldest_before_dawn_and_warmest_in_the_afternoon() {
        assert_eq!(day_curve(5.0), -1.0);
        assert_eq!(day_curve(15.0), 1.0);
        assert!(day_curve(12.0) > 0.5 && day_curve(2.0) < -0.5);
        assert_eq!(day_curve(29.0), day_curve(5.0), "a day comes round again");
        let mut last = day_curve(5.0);
        for hour in 6..=15 {
            assert!(day_curve(hour as f32) > last, "it warms till three");
            last = day_curve(hour as f32);
        }
    }

    #[test]
    fn a_zone_comes_on_over_its_edge_and_a_sphere_is_round() {
        let zone = HeatZone { size: [10.0, 4.0, 10.0], edge: 1.0, ..HeatZone::default() };
        let here = (Vec3::ZERO, Quat::IDENTITY);
        assert_eq!(zone_weight(&zone, here, Vec3::ZERO), 1.0);
        assert_eq!(zone_weight(&zone, here, Vec3::new(6.0, 0.0, 0.0)), 0.0);
        let near_edge = zone_weight(&zone, here, Vec3::new(4.5, 0.0, 0.0));
        assert!(near_edge > 0.0 && near_edge < 1.0);
        let ball = HeatZone { shape: ZoneShape::Sphere, size: [4.0; 3], edge: 0.0, ..HeatZone::default() };
        assert_eq!(zone_weight(&ball, here, Vec3::new(1.9, 0.0, 0.0)), 1.0);
        assert_eq!(zone_weight(&ball, here, Vec3::new(1.5, 1.5, 0.0)), 0.0);
    }
}
