//! Ground of blocks made by a world generator (see `crate::generate`): its
//! heights a column at a time, the same mountains, coasts and climate a
//! terrain's Generate layer lays, pushed in and out by noise in three
//! dimensions so cliffs lean and overhang, and hollowed below by caves: long
//! tunnels where two noises are both near nothing, and chambers where a
//! third is high.
//!
//! Each column takes its world's ground layers (`file::Ground`'s top, under
//! and bottom blocks), but where a kind of block's cover takes its climate
//! and height (`file::BlockCover`), that kind lies on top instead. Every
//! chunk is made apart, on every core, from where its blocks are alone, so
//! the same ground comes from the same file anywhere.
//!
//! A planet's ground lies all round a middle at its world's corner (see
//! [`BlockPlanet`]): its generator's heights and climate read round it, once,
//! on the six faces of a cube about a block apart, and every block's depth
//! measured out from the middle.

use serde::{Deserialize, Serialize};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK, Chunk, fill_of},
    file::BlockFile,
};
use crate::{generate::WorldGen, noise::gradient3};

/// How a world of blocks' ground is generated: by the world generator
/// `world` names (a reference to a `.worldgen`), from `position` metres in
/// its world, down to block `bottom` (counted from the world's corner, so
/// negative goes below it), its surface pushed in and out by as many metres
/// as `overhangs`, and as much of it below hollowed out as `caves` says,
/// from 0 for none to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockGenerate {
    pub world: Option<String>,
    pub position: [f32; 2],
    pub bottom: i32,
    pub overhangs: f32,
    pub caves: f32,
    /// For a world without end, made round the camera as it goes: how far
    /// (see `stream`). Its ground's size is then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endless: Option<BlockEndless>,
    /// For a planet, its ground all round a middle at the world's corner: how
    /// large. Its ground's size, its place in its generator's world, its
    /// lowest block and going on without end are then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planet: Option<BlockPlanet>,
}

impl Default for BlockGenerate {
    fn default() -> Self {
        Self { world: None, position: [0.0, 0.0], bottom: -16, overhangs: 0.0, caves: 0.3, endless: None, planet: None }
    }
}

/// A planet: its sea `radius` metres from its middle, and its ground as high
/// over that as its generator's heights are over its sea level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockPlanet {
    pub radius: f32,
}

impl Default for BlockPlanet {
    fn default() -> Self {
        Self { radius: 100.0 }
    }
}

/// How deep under a planet's surface its caves reach, in metres: below
/// them it is solid, so its middle need not be read block by block.
const PLANET_CAVES: f32 = 64.0;

/// How far round the camera a world of blocks without end is made: its own
/// chunks out to `distance` metres, and coarser ones drawn out to `far`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockEndless {
    pub distance: f32,
    pub far: f32,
}

impl Default for BlockEndless {
    fn default() -> Self {
        Self { distance: 128.0, far: 1024.0 }
    }
}

/// How far apart the bumps of the ground's push in and out are, in metres.
const OVERHANG_SIZE: f32 = 14.0;

/// How wide caves' tunnels wind, and how far apart their chambers are, in
/// metres.
const TUNNEL_SIZE: f32 = 22.0;
const CHAMBER_SIZE: f32 = 40.0;

/// How many blocks under the surface caves keep clear of, so they open onto
/// it only here and there.
const CAVE_ROOF: f32 = 3.0;

/// One column of the ground: its surface's height in blocks, and the blocks
/// its top layers are, deepest last, as `(block, layers)`.
struct Column {
    surface: f32,
    layers: [(BlockId, f32); 2],
    bottom: BlockId,
}

/// The ground `generate` makes for `file` with generator `world`: its
/// ground's size of chunk columns, each on every core.
pub fn generate(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen) -> BlockMap {
    if let Some(planet) = generate.planet {
        return planet_ground(file, generate, Some(world), planet);
    }
    let ground = &file.ground;
    let columns_x = ground.size[0].div_ceil(CHUNK) as i32;
    let columns_z = ground.size[1].div_ceil(CHUNK) as i32;
    let keys: Vec<[i32; 2]> = (0..columns_z).flat_map(|z| (0..columns_x).map(move |x| [x, z])).collect();
    let made = crate::workers::each(&keys, |&key| column_chunks(file, generate, world, 0, key, Some(ground.size)));
    let mut map = BlockMap::default();
    for (key, chunk) in made.into_iter().flatten() {
        map.put_chunk(key, chunk);
    }
    map
}

/// The chunks of chunk column `[cx, cz]` of the ground `generate` makes for
/// `file` with generator `world`, at level `level` of coarseness: each level
/// blocks twice as large as the one before, counted in its own blocks and
/// chunks, for far views; its columns within `limit` blocks along x and z
/// from the world's corner where it has one. Coarser levels leave out caves.
pub fn column_chunks(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &WorldGen,
    level: u32,
    [cx, cz]: [i32; 2],
    limit: Option<[u32; 2]>,
) -> Vec<([i32; 3], Chunk)> {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let ground = &file.ground;
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    let seed = world.seed;
    // Each column's surface and layers, worked out once.
    let column = |x: i32, z: i32| -> Column {
        let (px, pz) = (generate.position[0] + (x as f32 + 0.5) * size, generate.position[1] + (z as f32 + 0.5) * size);
        let height = world.height_at(px, pz);
        let climate = world.climate(px, pz, height);
        let cover = file.blocks.iter().enumerate().rev().find_map(|(at, kind)| {
            kind.cover.filter(|c| c.covers(height, climate)).map(|c| ((at + 1) as BlockId, c.depth))
        });
        // Layers as many metres deep at every level, at least a block.
        let top = match cover {
            Some((block, depth)) => (block, (depth.max(1) as f32 / scale).max(1.0)),
            None => (known(ground.top), 1.0),
        };
        Column {
            surface: height / size,
            layers: [top, (known(ground.under), ground.under_depth as f32 / scale)],
            bottom: known(ground.bottom),
        }
    };
    let side = CHUNK as i32;
    let reach = |v: i32, limit: Option<u32>| {
        let end = (v + 1) * side;
        v * side..limit.map_or(end, |limit| end.min(limit.div_ceil(scale as u32) as i32))
    };
    let mut columns = Vec::with_capacity((side * side) as usize);
    for z in reach(cz, limit.map(|l| l[1])) {
        for x in reach(cx, limit.map(|l| l[0])) {
            columns.push(((x, z), column(x, z)));
        }
    }
    if columns.is_empty() {
        return Vec::new();
    }
    let push = generate.overhangs.max(0.0) / size;
    let coarse = BlockGenerate { caves: if level == 0 { generate.caves } else { 0.0 }, ..generate.clone() };
    let highest = columns.iter().map(|(_, c)| c.surface).fold(f32::MIN, f32::max) + push + 1.0;
    let low = generate.bottom.div_euclid(scale as i32);
    let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
    let (low_key, high_key) = (low.div_euclid(side), (highest.ceil() as i32).div_euclid(side));
    for cy in low_key..=high_key {
        let mut chunk = Chunk::filled(AIR);
        let mut any = false;
        for ((x, z), column) in &columns {
            for ly in 0..side {
                let y = cy * side + ly;
                if y < low {
                    continue;
                }
                let middle = [*x as f32 + 0.5, y as f32 + 0.5, *z as f32 + 0.5];
                let depth = column.surface - middle[1];
                let place = middle.map(|v| v * size);
                let (block, depth) = cell_at(column, depth, place, size, &coarse, push, seed, f32::INFINITY);
                let (lx, lz) = ((x - cx * side) as u32, (z - cz * side) as u32);
                // Drawn smooth, the air a block from the surface keeps how far it is from it too.
                if file.smooth && (block != AIR || depth > -1.0) {
                    chunk.set_cell(lx, ly as u32, lz, block, fill_of(depth, block != AIR));
                    any = true;
                } else if block != AIR {
                    chunk.set(lx, ly as u32, lz, block);
                    any = true;
                }
            }
        }
        if any {
            chunks.push(([cx, cy, cz], chunk));
        }
    }
    chunks
}

/// The block `depth` blocks under its column's surface at `place`, in
/// metres from the world's corner, and about how far into the ground its
/// middle is, in blocks (out of it where negative): ground below the
/// surface pushed in and out, but in caves no deeper than `caves_until`
/// blocks, of the column's layer that deep.
#[allow(clippy::too_many_arguments)]
fn cell_at(
    column: &Column,
    depth: f32,
    place: [f32; 3],
    size: f32,
    generate: &BlockGenerate,
    push: f32,
    seed: u32,
    caves_until: f32,
) -> (BlockId, f32) {
    let mut depth = depth;
    if push > 0.0 && depth.abs() < push + 1.0 {
        let s = OVERHANG_SIZE;
        depth += gradient3(place[0] / s, place[1] / s, place[2] / s, seed.wrapping_add(41)) * push;
    }
    if depth <= 0.0 {
        return (AIR, depth);
    }
    // Caves: tunnels where two noises are both near nothing, chambers where a third is high. How far out of one
    // each is, in blocks, reads its noise's distance from where the cave starts as about half its size a unit.
    let mut inside = depth;
    if generate.caves > 0.0 && depth > CAVE_ROOF - 1.0 && depth < caves_until {
        let caves = generate.caves.clamp(0.0, 1.0);
        let t = TUNNEL_SIZE;
        let tunnel = gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(53)).abs()
            + gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(67)).abs();
        let c = CHAMBER_SIZE;
        let chamber = gradient3(place[0] / c, place[1] / (c * 0.6), place[2] / c, seed.wrapping_add(79));
        let out_of_tunnel = (tunnel - 0.14 * caves.sqrt()) * t * 0.5 / size;
        let out_of_chamber = (0.62 - 0.22 * caves - chamber) * c * 0.5 / size;
        // Within the roof under the surface, the ground stays whole.
        inside = inside.min((CAVE_ROOF - depth).max(out_of_tunnel.min(out_of_chamber)));
    }
    if inside <= 0.0 {
        return (AIR, inside);
    }
    let [(top, top_depth), (under, under_depth)] = column.layers;
    let block = if depth <= top_depth {
        top
    } else if depth <= top_depth + under_depth {
        under
    } else {
        column.bottom
    };
    (block, inside)
}

/// One point of a planet's ground all round: how far out its surface is
/// from the middle, its height in its generator's world, and its climate.
#[derive(Clone, Copy, Debug, Default)]
struct RoundPoint {
    surface: f32,
    height: f32,
    warmth: f32,
    humidity: f32,
}

/// A planet's ground all round, read once: on each face of a cube round its
/// middle, a grid of points evenly apart by angle, about a block apart at its
/// sea, and how low and high its surface goes.
struct Round {
    steps: usize,
    points: Vec<RoundPoint>,
    lowest: f32,
    highest: f32,
}

/// The way out from a planet's middle through face `face` of a cube round it
/// (+x, -x, +y, -y, +z, -z) at angles `a` and `b` across it, each from -1 to
/// 1 for a quarter turn.
fn way_out(face: usize, a: f32, b: f32) -> [f32; 3] {
    let (axis, sign) = (face / 2, if face % 2 == 0 { 1.0 } else { -1.0 });
    let quarter = std::f32::consts::FRAC_PI_4;
    let mut way = [0.0f32; 3];
    way[axis] = sign;
    way[(axis + 1) % 3] = (a * quarter).tan();
    way[(axis + 2) % 3] = (b * quarter).tan();
    let length = way.iter().map(|v| v * v).sum::<f32>().sqrt();
    way.map(|v| v / length)
}

/// Which face of a cube `way` leaves through, and at which angles across it.
fn face_of(way: [f32; 3]) -> (usize, f32, f32) {
    let axis = (0..3).max_by(|&i, &j| way[i].abs().total_cmp(&way[j].abs())).unwrap_or(1);
    let along = way[axis].abs().max(1e-9);
    let quarter = std::f32::consts::FRAC_PI_4;
    let angle = |v: f32| (v / along).atan() / quarter;
    (axis * 2 + usize::from(way[axis] < 0.0), angle(way[(axis + 1) % 3]), angle(way[(axis + 2) % 3]))
}

impl Round {
    /// A planet's ground with no generator to read: its sea all round, mild.
    fn level(radius: f32) -> Self {
        let point = RoundPoint { surface: radius, height: 0.0, warmth: 15.0, humidity: 0.5 };
        Self { steps: 2, points: vec![point; 6 * 2 * 2], lowest: radius, highest: radius }
    }

    fn of(world: &WorldGen, radius: f32, size: f32) -> Self {
        let steps = ((radius * std::f32::consts::FRAC_PI_2 / size).ceil() as usize + 1).clamp(8, 4096);
        let rows: Vec<(usize, usize)> = (0..6).flat_map(|face| (0..steps).map(move |row| (face, row))).collect();
        let angle = |i: usize| -1.0 + 2.0 * i as f32 / (steps - 1) as f32;
        let made = crate::workers::each(&rows, |&(face, row)| {
            (0..steps)
                .map(|column| {
                    let at = way_out(face, angle(column), angle(row)).map(|v| v * radius);
                    let height = world.height_round(at);
                    let (warmth, humidity) = world.climate_round(at, height);
                    RoundPoint { surface: radius + height - world.sea_level, height, warmth, humidity }
                })
                .collect::<Vec<_>>()
        });
        let points: Vec<RoundPoint> = made.into_iter().flatten().collect();
        let lowest = points.iter().map(|p| p.surface).fold(f32::MAX, f32::min);
        let highest = points.iter().map(|p| p.surface).fold(f32::MIN, f32::max);
        Self { steps, points, lowest, highest }
    }

    /// The ground the way `way` goes out, read between the points round it.
    fn at(&self, way: [f32; 3]) -> RoundPoint {
        let (face, a, b) = face_of(way);
        let last = (self.steps - 1) as f32;
        let (x, y) = (((a + 1.0) * 0.5 * last).clamp(0.0, last), ((b + 1.0) * 0.5 * last).clamp(0.0, last));
        let (x0, y0) = ((x.floor() as usize).min(self.steps - 2), (y.floor() as usize).min(self.steps - 2));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let point = |column: usize, row: usize| self.points[(face * self.steps + row) * self.steps + column];
        let mix = |a: RoundPoint, b: RoundPoint, t: f32| RoundPoint {
            surface: a.surface + (b.surface - a.surface) * t,
            height: a.height + (b.height - a.height) * t,
            warmth: a.warmth + (b.warmth - a.warmth) * t,
            humidity: a.humidity + (b.humidity - a.humidity) * t,
        };
        let near = mix(point(x0, y0), point(x0 + 1, y0), tx);
        let far = mix(point(x0, y0 + 1), point(x0 + 1, y0 + 1), tx);
        mix(near, far, ty)
    }
}

/// A planet's ground for `file`, all round a middle at the world's corner,
/// as generator `world` shapes it, or level with its sea without one: every
/// chunk its surface, overhangs and caves reach made on every core, and
/// those wholly under its caves its base block alone.
pub fn planet_ground(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: Option<&WorldGen>,
    planet: BlockPlanet,
) -> BlockMap {
    let size = file.block_size.max(0.01);
    let radius = planet.radius.max(size * 4.0);
    let round = world.map_or_else(|| Round::level(radius), |world| Round::of(world, radius, size));
    let seed = world.map_or(0, |world| world.seed);
    let push = generate.overhangs.max(0.0);
    let outer = round.highest + push + 2.0 * size;
    let caves = if generate.caves > 0.0 { PLANET_CAVES } else { 0.0 };
    let solid = round.lowest - push - 2.0 * size - caves;
    let side = CHUNK as f32 * size;
    let reach = (outer / side).ceil() as i32;
    let distances = |key: [i32; 3]| {
        let low = key.map(|v| v as f32 * side);
        let near: f32 = (0..3).map(|i| (low[i].max(0.0) + (-(low[i] + side)).max(0.0)).powi(2)).sum::<f32>();
        let far: f32 = (0..3).map(|i| low[i].abs().max((low[i] + side).abs()).powi(2)).sum::<f32>();
        (near.sqrt(), far.sqrt())
    };
    let keys: Vec<[i32; 3]> = (-reach..reach)
        .flat_map(|y| (-reach..reach).flat_map(move |z| (-reach..reach).map(move |x| [x, y, z])))
        .filter(|&key| distances(key).0 <= outer)
        .collect();
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    let ground = &file.ground;
    let bottom = known(ground.bottom);
    let made = crate::workers::each(&keys, |&key| {
        if distances(key).1 < solid {
            return (bottom != AIR).then(|| Chunk::filled(bottom));
        }
        let mut chunk = Chunk::filled(AIR);
        let mut any = false;
        let base = key.map(|v| v * CHUNK as i32);
        for ly in 0..CHUNK {
            for lz in 0..CHUNK {
                for lx in 0..CHUNK {
                    let middle =
                        [base[0] + lx as i32, base[1] + ly as i32, base[2] + lz as i32].map(|v| v as f32 + 0.5);
                    let place = middle.map(|v| v * size);
                    let out = place.iter().map(|v| v * v).sum::<f32>().sqrt();
                    if out > outer {
                        continue;
                    }
                    if out < solid {
                        if bottom != AIR {
                            chunk.set(lx, ly, lz, bottom);
                            any = true;
                        }
                        continue;
                    }
                    let point = round.at(place.map(|v| v / out.max(1e-6)));
                    let cover = file.blocks.iter().enumerate().rev().find_map(|(at, kind)| {
                        let climate = (point.warmth, point.humidity);
                        kind.cover.filter(|c| c.covers(point.height, climate)).map(|c| ((at + 1) as BlockId, c.depth))
                    });
                    let top = match cover {
                        Some((block, depth)) => (block, depth.max(1) as f32),
                        None => (known(ground.top), 1.0),
                    };
                    let column = Column {
                        surface: 0.0,
                        layers: [top, (known(ground.under), ground.under_depth as f32)],
                        bottom,
                    };
                    let depth = (point.surface - out) / size;
                    let (block, depth) =
                        cell_at(&column, depth, place, size, generate, push / size, seed, caves / size);
                    if file.smooth && (block != AIR || depth > -1.0) {
                        chunk.set_cell(lx, ly, lz, block, fill_of(depth, block != AIR));
                        any = true;
                    } else if block != AIR {
                        chunk.set(lx, ly, lz, block);
                        any = true;
                    }
                }
            }
        }
        any.then_some(chunk)
    });
    let mut map = BlockMap::default();
    for (key, chunk) in keys.into_iter().zip(made) {
        if let Some(chunk) = chunk {
            map.put_chunk(key, chunk);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::file::{BlockCover, BlockKind, Ground},
        generate::Preset,
        surfaces::Biome,
    };

    fn generated(caves: f32, overhangs: f32) -> (BlockFile, WorldGen) {
        // Mountains 300 m high in a cold world, so some of it is snowed.
        let mut world = Preset::Alpine.world();
        world.height = 300.0;
        world.climate.temperature = 0.0;
        let mut file = BlockFile {
            block_size: 2.0,
            ground: Ground { size: [64, 64], ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("alpine".into()),
                position: [500.0, 800.0],
                bottom: -40,
                overhangs,
                caves,
                endless: None,
                planet: None,
            }),
            ..BlockFile::default()
        };
        // Snow where it is below freezing.
        let (temperature, humidity) = Biome::Snow.ranges();
        file.blocks.push(BlockKind {
            cover: Some(BlockCover { temperature, humidity, height: None, depth: 2 }),
            ..BlockKind::colored("Snow", [0.95; 3])
        });
        (file, world)
    }

    #[test]
    fn generated_ground_follows_its_generator_snowed_up_high_and_hollowed_by_caves() {
        let (file, world) = generated(0.0, 0.0);
        let map = file.make_ground(Some(&world));
        // Each column's top block lies at its generator's height there, in 2 m blocks.
        let surface = |map: &BlockMap, x: i32, z: i32| (-40..400).rev().find(|&y| map.get([x, y, z]) != AIR);
        let mut snowed = 0;
        for (x, z) in [(3, 5), (30, 40), (60, 12), (17, 63)] {
            let (px, pz) = (500.0 + (x as f32 + 0.5) * 2.0, 800.0 + (z as f32 + 0.5) * 2.0);
            let height = world.height_at(px, pz);
            let top = surface(&map, x, z).expect("ground in every column");
            assert!(((top + 1) as f32 * 2.0 - height).abs() <= 2.0, "column ({x}, {z}): top {top} for {height} m");
            let cold = world.climate(px, pz, height).0 < -2.0;
            let block = map.get([x, top, z]);
            assert_eq!(block == 4, cold, "snow where it is below freezing: {block} at {height} m");
            snowed += usize::from(cold);
            assert_eq!(map.get([x, -40, z]), 3, "stone at the bottom");
            assert_eq!(map.get([x, -41, z]), AIR, "nothing below it");
        }
        let _ = snowed;
        // The same again from the same file; and with caves, ground hollowed out below.
        assert_eq!(file.make_ground(Some(&world)), map);
        let (caved, _) = generated(0.8, 0.0);
        let hollow = caved.make_ground(Some(&world));
        let solid = |map: &BlockMap| {
            map.chunks()
                .map(|(_, c)| {
                    (0..32)
                        .flat_map(|y| (0..32).flat_map(move |z| (0..32).map(move |x| (x, y, z))))
                        .filter(|&(x, y, z)| c.get(x, y, z) != AIR)
                        .count()
                })
                .sum::<usize>()
        };
        let (full, hollowed) = (solid(&map), solid(&hollow));
        assert!(hollowed < full * 97 / 100 && hollowed > full * 50 / 100, "{hollowed} of {full} blocks left by caves");
        // With overhangs, the surface pushed in and out: some blocks with air under them above ground.
        let (leaning, _) = generated(0.0, 10.0);
        let pushed = leaning.make_ground(Some(&world));
        let overhanging = (0..64).flat_map(|z| (0..64).map(move |x| (x, z))).any(|(x, z)| {
            let top = surface(&pushed, x, z).unwrap_or(-40);
            (top - 6..top).any(|y| pushed.get([x, y, z]) == AIR)
        });
        assert!(overhanging, "air under the ground somewhere above the caves' reach");
    }

    #[test]
    fn a_planets_ground_lies_all_round_its_middle_on_its_generators_heights_read_round_it() {
        let mut world = Preset::RollingHills.world();
        world.height = 12.0;
        let world = WorldGen::parse(&world.to_text()).unwrap();
        let planet = BlockPlanet { radius: 60.0 };
        let generate =
            BlockGenerate { world: Some("hills".into()), caves: 0.0, planet: Some(planet), ..Default::default() };
        let file = BlockFile { generate: Some(generate), smooth: true, ..BlockFile::default() };
        let map = file.make_ground(Some(&world));
        // Its middle is its base block, and far out is air.
        assert_eq!(map.get([0, 0, 0]), file.ground.bottom);
        assert_eq!(map.get([0, 100, 0]), AIR);
        // Out along six ways and between them, the surface where the generator's heights put it, to a tenth of a block.
        let surface = |way: [f32; 3]| {
            let length = way.iter().map(|v| v * v).sum::<f32>().sqrt();
            let way = way.map(|v| v / length);
            let at = |r: f32| way.map(|v| (v * r - 0.5).round() as i32);
            // The last block of ground going out, and how far into the ground its middle is.
            let r = (0..1200).map(|i| i as f32 * 0.1).filter(|&r| map.get(at(r)) != AIR).fold(0.0, f32::max);
            let block = at(r);
            let middle = block.map(|v| v as f32 + 0.5);
            let out = middle.iter().map(|v| v * v).sum::<f32>().sqrt();
            out + super::super::chunk::depth_of(map.fill(block))
        };
        for way in [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.6, 0.7, -0.4], [-0.5, 0.5, 0.5]] {
            let length = way.iter().map(|v: &f32| v * v).sum::<f32>().sqrt();
            let at = way.map(|v| v / length * planet.radius);
            let expected = planet.radius + world.height_round(at) - world.sea_level;
            assert!((surface(way) - expected).abs() < 1.0, "out along {way:?}: {} for {expected}", surface(way));
        }
        // Read on a cube's faces, its ground runs on across their edges: no step where two faces meet.
        let round = Round::of(&world, planet.radius, 1.0);
        let either = |e: f32| round.at([1.0, 1.0 + e, 0.3]).surface;
        assert!((either(0.002) - either(-0.002)).abs() < 0.1, "{} and {}", either(0.002), either(-0.002));
        // Colder at its poles than round its middle.
        let (pole, _) = world.climate_round([0.0, planet.radius, 0.0], 0.0);
        let (middle, _) = world.climate_round([planet.radius, 0.0, 0.0], 0.0);
        assert!(pole < middle - 10.0, "{pole} at a pole, {middle} round its middle");
    }
}
