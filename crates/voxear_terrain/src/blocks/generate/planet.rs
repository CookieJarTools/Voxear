//! A planet's generated ground: all round a middle at its world's corner,
//! its generator's heights and climate read round it once, on the six
//! faces of a cube about a block apart, and every block's depth measured
//! out from the middle.

use super::{
    BlockGenerate, BlockPlanet,
    cells::{cell_at, pushed},
    columns::Column,
};
use crate::{
    blocks::{
        BlockMap,
        chunk::{AIR, BlockId, CHUNK, Chunk, fill_of},
        file::BlockFile,
        flow::surface_fill,
    },
    generate::WorldGen,
};

/// How deep under a planet's surface its caves reach, in metres: below
/// them it is solid, so its middle need not be read block by block.
const PLANET_CAVES: f32 = 64.0;

/// One point of a planet's ground all round: how far out its surface is
/// from the middle, its height in its generator's world, and its climate.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RoundPoint {
    pub surface: f32,
    height: f32,
    warmth: f32,
    humidity: f32,
}

/// A planet's ground all round, read once: on each face of a cube round its
/// middle, a grid of points evenly apart by angle, about a block apart at its
/// sea, and how low and high its surface goes.
pub(super) struct Round {
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

    pub(super) fn of(world: &WorldGen, radius: f32, size: f32) -> Self {
        let steps = ((radius * std::f32::consts::FRAC_PI_2 / size).ceil() as usize + 1).clamp(8, 4096);
        let rows: Vec<(usize, usize)> = (0..6).flat_map(|face| (0..steps).map(move |row| (face, row))).collect();
        let angle = |i: usize| -1.0 + 2.0 * i as f32 / (steps - 1) as f32;
        // A row of points' ground and climate, asked of the generator at once.
        let made = crate::workers::each(&rows, |&(face, row)| {
            let at: Vec<[f32; 3]> =
                (0..steps).map(|column| way_out(face, angle(column), angle(row)).map(|v| v * radius)).collect();
            let (mut heights, mut warmth, mut wetness) = (vec![0.0; steps], vec![0.0; steps], vec![0.0; steps]);
            world.heights_round(&at, &mut heights);
            world.climates_round(&at, &heights, &mut warmth, &mut wetness);
            (0..steps)
                .map(|point| RoundPoint {
                    surface: radius + heights[point] - world.sea_level(),
                    height: heights[point],
                    warmth: warmth[point],
                    humidity: wetness[point],
                })
                .collect::<Vec<_>>()
        });
        let points: Vec<RoundPoint> = made.into_iter().flatten().collect();
        let lowest = points.iter().map(|p| p.surface).fold(f32::MAX, f32::min);
        let highest = points.iter().map(|p| p.surface).fold(f32::MIN, f32::max);
        Self { steps, points, lowest, highest }
    }

    /// The ground the way `way` goes out, read between the points round it.
    pub(super) fn at(&self, way: [f32; 3]) -> RoundPoint {
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
    let seed = world.map_or(0, |world| world.seed());
    let push = generate.overhangs.max(0.0);
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    // Its sea, its radius out from its middle, where it has one.
    let sea = known(generate.sea);
    let sea_out = if sea == AIR { f32::MIN } else { radius };
    let outer = (round.highest + push).max(sea_out) + 2.0 * size;
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
                    let under = (point.surface - out) / size;
                    let (block, depth, open) =
                        cell_at(&column, under, place, size, generate, push / size, seed, caves / size);
                    let over = (sea_out - out) / size;
                    // Of the ground, the sea takes the top block alone, as on flat ground.
                    let shore =
                        file.smooth && block != AIR && over <= 1.0 && pushed(under, place, push / size, seed) < over;
                    let block = if over > 0.0 && (open || shore) { sea } else { block };
                    if file.smooth && sea != AIR && block == sea {
                        chunk.set_cell(lx, ly, lz, sea, surface_fill(over));
                        any = true;
                    } else if file.smooth && (block != AIR || depth > -1.0) {
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
    let mut map = BlockMap { round: true, ..BlockMap::default() };
    for (key, chunk) in keys.into_iter().zip(made) {
        if let Some(chunk) = chunk {
            map.put_chunk(key, chunk);
        }
    }
    map
}
