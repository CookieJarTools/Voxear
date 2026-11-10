//! A planet's generated ground: all round a middle at its world's corner,
//! its generator's heights and climate, and what its Volume reads of the
//! ground, read round it once, on the six faces of a cube about a block
//! apart, and every block's depth measured out from the middle.
//!
//! Where the Volume is more than the ground's height made solid, a block's
//! layer is counted in from the open air along the way out from the
//! middle; and a world drawn smooth measures each fill against the blocks
//! round it and lays its top layer along slopes as a skin, a block further
//! than the chunk's own read on every side for both.

use super::{
    BlockGenerate, BlockPlanet,
    cells::{Sea, cell, distance, level, skinned, source},
    columns::Column,
};
use crate::{
    blocks::{
        BlockMap,
        chunk::{AIR, BlockId, CHUNK, Chunk},
        file::BlockFile,
    },
    generate::{Cells, Volume, WorldGen},
};

/// How deep under a planet's surface its caves reach, in metres: below
/// them it is solid, so its middle need not be read block by block.
const PLANET_CAVES: f32 = 64.0;

/// How many blocks further into or out of the ground than a block's middle
/// its fill is no longer measured against the blocks round it.
const MEASURED: f32 = 8.0;

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
/// sea, how low and high its surface goes, and the rows the generator's
/// Volume reads of the ground, a number a point in each.
pub(super) struct Round {
    steps: usize,
    points: Vec<RoundPoint>,
    carried: Vec<Vec<f32>>,
    lowest: f32,
    highest: f32,
}

/// Where between a planet's points a way out from its middle passes: the
/// four points round it, and how far from the first towards the others.
pub(super) struct Between {
    corners: [usize; 4],
    across: f32,
    along: f32,
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
    /// A planet's ground as `world` shapes it, with what `volume` reads of
    /// it.
    pub(super) fn of(world: &WorldGen, volume: Volume, radius: f32, size: f32) -> Self {
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
            let points = (0..steps)
                .map(|point| RoundPoint {
                    surface: radius + heights[point] - world.sea_level(),
                    height: heights[point],
                    warmth: warmth[point],
                    humidity: wetness[point],
                })
                .collect::<Vec<_>>();
            (points, volume.carried_round(&at, &heights, radius))
        });
        let mut points = Vec::with_capacity(rows.len() * steps);
        let mut carried: Vec<Vec<f32>> = Vec::new();
        for (row, read) in made {
            points.extend(row);
            carried.resize_with(read.rows().len(), Vec::new);
            for (all, part) in carried.iter_mut().zip(read.rows()) {
                all.extend(part);
            }
        }
        let lowest = points.iter().map(|p| p.surface).fold(f32::MAX, f32::min);
        let highest = points.iter().map(|p| p.surface).fold(f32::MIN, f32::max);
        Self { steps, points, carried, lowest, highest }
    }

    /// Where the way `way` goes out passes between the points.
    pub(super) fn between(&self, way: [f32; 3]) -> Between {
        let (face, a, b) = face_of(way);
        let last = (self.steps - 1) as f32;
        let (x, y) = (((a + 1.0) * 0.5 * last).clamp(0.0, last), ((b + 1.0) * 0.5 * last).clamp(0.0, last));
        let (x0, y0) = ((x.floor() as usize).min(self.steps - 2), (y.floor() as usize).min(self.steps - 2));
        let point = |column: usize, row: usize| (face * self.steps + row) * self.steps + column;
        Between {
            corners: [point(x0, y0), point(x0 + 1, y0), point(x0, y0 + 1), point(x0 + 1, y0 + 1)],
            across: x - x0 as f32,
            along: y - y0 as f32,
        }
    }

    /// The ground at `between`, read between the points round it.
    pub(super) fn point(&self, between: &Between) -> RoundPoint {
        let [a, b, c, d] = between.corners.map(|corner| self.points[corner]);
        let mix = |a: RoundPoint, b: RoundPoint, t: f32| RoundPoint {
            surface: a.surface + (b.surface - a.surface) * t,
            height: a.height + (b.height - a.height) * t,
            warmth: a.warmth + (b.warmth - a.warmth) * t,
            humidity: a.humidity + (b.humidity - a.humidity) * t,
        };
        mix(mix(a, b, between.across), mix(c, d, between.across), between.along)
    }

    /// The ground the way `way` goes out, read between the points round it.
    #[cfg(test)]
    pub(super) fn at(&self, way: [f32; 3]) -> RoundPoint {
        self.point(&self.between(way))
    }

    /// Row `row` of what the Volume reads of the ground at `between`, read
    /// between the points round it as the ground is.
    fn carried(&self, row: usize, between: &Between) -> f32 {
        let [a, b, c, d] = between.corners.map(|corner| self.carried[row][corner]);
        let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
        mix(mix(a, b, between.across), mix(c, d, between.across), between.along)
    }
}

/// The cells of a chunk and those round it asked of the Volume at once:
/// where each one's middle is, and what is carried to it.
#[derive(Default)]
struct Asked {
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    carried: Vec<Vec<f32>>,
}

impl Asked {
    fn new(rows: usize) -> Self {
        Self { carried: vec![Vec::new(); rows], ..Self::default() }
    }

    fn add(&mut self, place: [f32; 3], round: &Round, between: &Between) {
        self.x.push(place[0]);
        self.y.push(place[1]);
        self.z.push(place[2]);
        for (row, carried) in self.carried.iter_mut().enumerate() {
            carried.push(round.carried(row, between));
        }
    }

    /// How far into the ground the Volume puts each, in metres, and the
    /// same with the caves left whole; with `coarse`, the second for both.
    fn read(&self, volume: Volume, radius: f32, size: f32, coarse: bool) -> (Vec<f32>, Vec<f32>) {
        let (mut inside, mut under) = (vec![0.0; self.x.len()], vec![0.0; self.x.len()]);
        let carried: Vec<&[f32]> = self.carried.iter().map(Vec::as_slice).collect();
        let cells = Cells { x: &self.x, y: &self.y, z: &self.z, carried: &carried, size };
        volume.densities_round(cells, radius, coarse, &mut inside, &mut under);
        (inside, under)
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
    let source = source(generate, world.unwrap_or_else(|| level()), size);
    let world = source.world();
    let mut map = BlockMap { round: true, ..BlockMap::default() };
    let Some(volume) = world.volume() else { return map };
    let round = Round::of(world, volume, radius, size);
    let plain = volume.plain();
    // A plain height with no caves in it needs nothing asked of the Volume.
    let ask = !plain || volume.caved();
    // How far the ground may be pushed out past its height, in metres.
    let push = volume.reach();
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    // Its sea, its radius out from its middle, where it has one.
    let sea = known(generate.sea);
    let sea_out = if sea == AIR { f32::MIN } else { radius };
    let outer = (round.highest + push).max(sea_out) + 2.0 * size;
    let caves = if source.own() || generate.caves > 0.0 { PLANET_CAVES } else { 0.0 };
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
    // Fills measured against the blocks round them read a block further than the chunk on every side.
    let ring = i32::from(file.smooth && !plain);
    let wide = (CHUNK as i32 + 2 * ring) as usize;
    // How many blocks out from one of ground its layer is counted: as deep as any layer goes.
    let covers = file.blocks.iter().filter_map(|kind| kind.cover).map(|cover| cover.depth.max(1)).max().unwrap_or(1);
    let counted_out = (covers + ground.under_depth).clamp(1, 16) as usize;
    let made = crate::workers::each(&keys, |&key| {
        if distances(key).1 < solid {
            return (bottom != AIR).then(|| Chunk::filled(bottom));
        }
        let base = key.map(|v| v * CHUNK as i32);
        let middle_of = |at: [i32; 3]| [base[0] + at[0], base[1] + at[1], base[2] + at[2]].map(|v| v as f32 + 0.5);
        // Every cell of the chunk, and of the ring round it, that the surface, overhangs or caves may reach:
        // asked of the Volume at once. Those further out are air and those further in ground, as far as they lie.
        let cells = (-ring..CHUNK as i32 + ring).flat_map(|y| {
            (-ring..CHUNK as i32 + ring).flat_map(move |z| (-ring..CHUNK as i32 + ring).map(move |x| [x, y, z]))
        });
        let mut asked = Asked::new(round.carried.len());
        let mut which: Vec<(usize, Between)> = Vec::new();
        let (mut inside, mut under) = (vec![0.0f32; wide * wide * wide], vec![0.0f32; wide * wide * wide]);
        for (at, cell) in cells.enumerate() {
            let place = middle_of(cell).map(|v| v * size);
            let out = place.iter().map(|v| v * v).sum::<f32>().sqrt();
            if out > outer || out < solid {
                let nearest = if out > outer { round.highest } else { round.lowest };
                (inside[at], under[at]) = ((nearest - out) / size, (nearest - out) / size);
                continue;
            }
            let between = round.between(place.map(|v| v / out.max(1e-6)));
            if ask {
                asked.add(place, &round, &between);
            } else {
                // A plain height with no caves in it: as far in as its surface is further out.
                (inside[at], under[at]) =
                    ((round.point(&between).surface - out) / size, (round.point(&between).surface - out) / size);
            }
            which.push((at, between));
        }
        if ask {
            let (whole, ground_only) = asked.read(volume, radius, size, false);
            for (read, (at, _)) in which.iter().enumerate() {
                under[*at] = ground_only[read] / size;
                // Under its caves' reach the ground is whole.
                inside[*at] = if under[*at] < caves / size { whole[read] / size } else { under[*at] };
            }
        }
        // Where the ground is more than a height, how deep each block of it near the surface is, for its layer:
        // counted in from the open air along the way out, read a block at a time.
        let mut counts: Vec<f32> = Vec::new();
        if !plain {
            let mut outward = Asked::new(round.carried.len());
            let own = |at: usize| {
                let [x, y, z] = [at % wide, at / (wide * wide), at / wide % wide].map(|v| v as i32 - ring);
                [x, y, z].iter().all(|v| (0..CHUNK as i32).contains(v))
            };
            // Only those the open air can be that near: the rest are deeper than any layer.
            let reached = counted_out as f32 + push / size + 1.0;
            let near: Vec<usize> = (0..which.len())
                .filter(|&read| {
                    let (at, between) = &which[read];
                    let place = [asked.x[read], asked.y[read], asked.z[read]];
                    let out = place.iter().map(|v| v * v).sum::<f32>().sqrt();
                    own(*at) && inside[*at] > 0.0 && (round.point(between).surface - out) / size < reached
                })
                .collect();
            for &read in &near {
                let place = [asked.x[read], asked.y[read], asked.z[read]];
                let out = place.iter().map(|v| v * v).sum::<f32>().sqrt();
                for step in 1..=counted_out {
                    let further = place.map(|v| v + v / out.max(1e-6) * size * step as f32);
                    outward.add(further, &round, &which[read].1);
                }
            }
            let (_, grounds) = outward.read(volume, radius, size, true);
            counts = vec![(counted_out + 1) as f32; which.len()];
            for (order, &read) in near.iter().enumerate() {
                let mut last = under[which[read].0];
                for step in 1..=counted_out {
                    let further = grounds[order * counted_out + step - 1] / size;
                    if further <= 0.0 {
                        // The block before the open air is as deep as its middle, a block at most.
                        counts[read] = (step - 1) as f32 + if last < 1.0 { last } else { 1.0 };
                        break;
                    }
                    last = further;
                }
            }
        }
        let mut chunk = Chunk::filled(AIR);
        let mut any = false;
        let mut next = which.iter().enumerate().peekable();
        for ly in 0..CHUNK {
            for lz in 0..CHUNK {
                for lx in 0..CHUNK {
                    let at = ((ly as usize + ring as usize) * wide + lz as usize + ring as usize) * wide
                        + lx as usize
                        + ring as usize;
                    while next.peek().is_some_and(|(_, (asked, _))| *asked < at) {
                        next.next();
                    }
                    let place = middle_of([lx as i32, ly as i32, lz as i32]).map(|v| v * size);
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
                    let Some((read, (_, between))) = next.peek().filter(|(_, (asked, _))| *asked == at) else {
                        continue;
                    };
                    let point = round.point(between);
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
                    let (within, ground_in) = (inside[at], under[at]);
                    let near = ring > 0 && within.abs() < MEASURED;
                    let depth = match (plain, near && ground_in > 0.0) {
                        (true, _) => ground_in,
                        (false, false) => counts[*read],
                        (false, true) => {
                            let beside = |step: usize| [under[at - step], under[at + step]];
                            let beside = [beside(1), beside(wide * wide), beside(wide)];
                            skinned(counts[*read], ground_in, beside, place.map(|v| v / out.max(1e-6)))
                        }
                    };
                    let filled = if near {
                        let beside = |step: usize| [inside[at - step], inside[at + step]];
                        distance(within, [beside(1), beside(wide * wide), beside(wide)])
                    } else {
                        within
                    };
                    let sea = Sea { block: sea, over: (sea_out - out) / size, eighths: None };
                    if let Some((block, fill)) = cell(&column, within, ground_in, depth, filled, sea, file.smooth) {
                        match fill {
                            Some(fill) => chunk.set_cell(lx, ly, lz, block, fill),
                            None => chunk.set(lx, ly, lz, block),
                        }
                        any = true;
                    }
                }
            }
        }
        any.then_some(chunk)
    });
    for (key, chunk) in keys.into_iter().zip(made) {
        if let Some(chunk) = chunk {
            map.put_chunk(key, chunk);
        }
    }
    map
}
