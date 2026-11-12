//! The Rivers node: rivers running down the ground wired into it, lakes in
//! its hollows, and waterfalls where a river drops over a cliff.
//!
//! Water finds its way over a grid of points Spacing apart, each a little
//! off its place by the seed: each drains to whichever of the eight round
//! it is lowest, the steepest way down, and a point with none lower is a
//! hollow. How much ground drains through a point is counted from the
//! points within a kilometre or so up its way. That is all worked out from
//! the points alone, so any chunk anywhere in an endless world finds the
//! same rivers, and they never stop at a seam.
//!
//! Where as much ground as Source drains into a point, in hectares, a river
//! runs from it down the way the water goes, wider and deeper the more
//! drains into it: Width and Depth are how wide and deep it is where a
//! square kilometre does. Its water falls along every stretch, so it always
//! runs downhill, and it winds as Meander says. The ground is cut away for
//! its bed and slopes down to it over Valley metres either side; and where
//! the ground by it is lower than its water, raised into banks, so its
//! water never stands over the air beside it. Where a stretch drops more
//! than Falls metres, the river runs nearly level to a cliff halfway along
//! it and pours over into a pool at its foot. A hollow that as much as
//! Lakes hectares drains into holds a lake as high as the lowest of the
//! ground round it; none at 0.
//!
//! Beside the ground it gives Water, the water's height where there is a
//! river or a lake (far under everything elsewhere), and Falls, how many
//! metres of a waterfall pour down over its pool: wire both into the
//! Output's, and a world of blocks fills its rivers and lakes with its
//! sea's block, and a waterfall's curtain with falling water. Banks is 1 on
//! a river and its banks, fading out across its valley, for a biome to grow
//! sand or reeds by water.
//!
//! It is sums, products, square roots and `floor`, so every machine makes
//! the same rivers.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        eval::{Givens, Program},
        graph::NodeId,
    },
    noise::gradient,
};

/// Rivers where `source` hectares drain into a point, `width` metres wide
/// and `depth` deep where a square kilometre does, over points `spacing`
/// metres apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rivers {
    pub spacing: f32,
    pub source: f32,
    pub width: f32,
    pub depth: f32,
    pub valley: f32,
    pub falls: f32,
    pub lakes: f32,
    pub meander: f32,
    pub seed: u32,
}

impl Default for Rivers {
    fn default() -> Self {
        Self {
            spacing: 64.0,
            source: 10.0,
            width: 14.0,
            depth: 3.0,
            valley: 40.0,
            falls: 5.0,
            lakes: 12.0,
            meander: 0.6,
            seed: 0,
        }
    }
}

impl Rivers {
    /// Its input.
    pub const GROUND: usize = 0;
    /// Its outputs.
    pub const HEIGHT: usize = 0;
    pub const WATER: usize = 1;
    pub const FALLS: usize = 2;
    pub const BANKS: usize = 3;
}

/// The water's height where there is none: under everything.
pub const NO_WATER: f32 = -1.0e6;

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("source", "Source", 0.5, 100000.0, "ha"),
    ValueSpec::number("width", "Width", 1.0, 1000.0, "m"),
    ValueSpec::number("depth", "Depth", 0.2, 100.0, "m"),
    ValueSpec::number("valley", "Valley", 0.0, 2000.0, "m"),
    ValueSpec::number("falls", "Falls", 1.0, 1000.0, "m"),
    ValueSpec::number("lakes", "Lakes", 0.0, 100000.0, "ha"),
    ValueSpec::number("meander", "Meander", 0.0, 1.0, ""),
    ValueSpec::number("spacing", "Spacing", 16.0, 1000.0, "m").advanced(),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::field("ground", "Ground")];

const OUTPUTS: &[Port] = &[
    Port::field("height", "Height"),
    Port::field("water", "Water"),
    Port::field("falls", "Falls"),
    Port::field("banks", "Banks"),
];

impl Kind for Rivers {
    const TITLE: &'static str = "Rivers";
    const FOLDER: &'static str = "Shape";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Number(self.source),
            1 => Value::Number(self.width),
            2 => Value::Number(self.depth),
            3 => Value::Number(self.valley),
            4 => Value::Number(self.falls),
            5 => Value::Number(self.lakes),
            6 => Value::Number(self.meander),
            7 => Value::Number(self.spacing),
            8 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.source = number,
            (1, Value::Number(number)) => self.width = number,
            (2, Value::Number(number)) => self.depth = number,
            (3, Value::Number(number)) => self.valley = number,
            (4, Value::Number(number)) => self.falls = number,
            (5, Value::Number(number)) => self.lakes = number,
            (6, Value::Number(number)) => self.meander = number,
            (7, Value::Number(number)) => self.spacing = number,
            (8, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let ground = cx.input(node, Self::GROUND, at);
        let program = cx.input_program(node, Self::GROUND);
        let (Some(ground), Some(program)) = (ground, program) else {
            return match port {
                Self::HEIGHT => cx.constant(0.0),
                Self::WATER => cx.constant(NO_WATER),
                _ => cx.constant(0.0),
            };
        };
        let seed = seed_for(cx, node, self.seed);
        let finder = Arc::new(Finder::new(self, cx.world.scale.max(0.01), cx.world.sea_level, seed, program));
        cx.run(Ran { finder, what: port.min(Self::BANKS) as u8 }, &[at.x, at.z, ground])
    }
}

/// How many points a side a tile of them is, worked out at once.
const TILE: i32 = 32;

/// How far up a point's way the ground draining through it is counted, in
/// points.
const REACH: i32 = 8;

/// How many points either way of its own cell a place looks at for rivers
/// running near it.
const LOOKED: i32 = 2;

/// How many points round a tile what drains into each is counted for: those
/// a place in it looks at, and the hollows they run into.
const KNOWN: i32 = LOOKED + SPILL as i32 + 1;

/// How far past a tile the points are read to count what drains into those
/// in it and round it: as far as any point counted can be, the way from
/// there, and as far as water spilt from a hollow on it is followed, so
/// every tile finds the same ways for its points.
const MARGIN: i32 = KNOWN + 2 * REACH + SPILL_STEPS as i32 + SPILL as i32 + 4;

/// How far water spilt from a hollow is followed to see whether it comes
/// back to it, in steps.
const SPILL_STEPS: usize = 8;

/// How many points from a hollow it may spill over to.
const SPILL: usize = 2;

/// How far off its place in the grid a point is, at most, as a share of
/// the spacing.
const JITTER: f32 = 0.7;

/// How high banks stand over their river's water, in metres.
const BANK: f32 = 1.25;

/// How far past a river's edge its banks are raised where the ground is
/// lower than they are, in metres: level for the first of them, then down
/// to the ground over the rest.
const BANK_TOP: f32 = 2.5;
const LEVEE: f32 = 4.0;

/// How far a lake reaches from its hollow's point, in spacings.
const LAKE_REACH: f32 = 1.6;

/// How thick a waterfall's curtain is, in metres.
const CURTAIN: f32 = 1.6;

/// How far past a waterfall its pool is dug deeper, in metres.
const POOL: f32 = 8.0;

/// How much of a waterfall's drop its river falls over the stretches before
/// and after it, each.
const RUN_DOWN: f32 = 0.04;

/// How much each point round one, and the point itself, counts towards the
/// ground it is taken to stand on.
const AVERAGED: [(i32, i32, f32); 9] = [
    (-1, -1, 1.0 / 16.0),
    (0, -1, 2.0 / 16.0),
    (1, -1, 1.0 / 16.0),
    (-1, 0, 2.0 / 16.0),
    (0, 0, 4.0 / 16.0),
    (1, 0, 2.0 / 16.0),
    (-1, 1, 1.0 / 16.0),
    (0, 1, 2.0 / 16.0),
    (1, 1, 1.0 / 16.0),
];

/// The eight points round one, as steps across the grid.
const ROUND: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// One point of the grid water finds its way over.
#[derive(Clone, Copy, Debug, Default)]
struct Point {
    at: [f32; 2],
    /// The point it drains to, as a step across the grid; none for a hollow.
    down: Option<[i8; 2]>,
    /// Whether a river runs from it down its way.
    river: bool,
    /// Whether it is a hollow holding a lake.
    lake: bool,
    /// Whether a river runs from a point as near as a place in its cell
    /// looks, or a lake lies at one: where none does, its cell is left be.
    near: bool,
    /// How high its river's water stands at it, or its lake's.
    level: f32,
    width: f32,
    depth: f32,
}

/// A Rivers node's settings held to their ranges, for the ground its
/// program makes, and the points worked out from them.
#[derive(Debug)]
struct Finder {
    spacing: f32,
    /// How many points must drain into one for a river to start there, and
    /// for a hollow to hold a lake (none at 0).
    source: u32,
    lakes: u32,
    width: f32,
    depth: f32,
    valley: f32,
    falls: f32,
    meander: f32,
    sea: f32,
    seed: u32,
    ground: Arc<Program>,
    id: u64,
}

/// The tiles of points last worked out, by finder and tile, shared by every
/// thread: a tile takes a while to work out, and the chunks round it are
/// made on many threads at once.
static TILES: Mutex<Vec<(u64, [i32; 2], Arc<Vec<Point>>)>> = Mutex::new(Vec::new());

/// How many tiles are kept.
const KEPT: usize = 48;

/// The ground at the points of blocks of them `GROUND_BLOCK` a side, by the
/// ground's program, the seed and the spacing, and the block: kept, shared
/// by every thread, so the tiles round one need not read again what their
/// margins share.
static GROUNDS: Mutex<Vec<(u64, [i32; 2], Arc<Vec<f32>>)>> = Mutex::new(Vec::new());
const GROUND_BLOCK: i32 = 16;
const GROUNDS_KEPT: usize = 1024;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Finder {
    fn new(set: &Rivers, scale: f32, sea: f32, seed: u32, ground: Arc<Program>) -> Self {
        let number = |value: f32, low: f32, high: f32| if value.is_nan() { low } else { value.clamp(low, high) };
        let spacing = number(set.spacing, 16.0, 1000.0) * scale;
        // A point drains this many hectares.
        let hectares = spacing * spacing / 10_000.0;
        let points = |area: f32| (area / hectares).round().max(1.0) as u32;
        let mut key = Key::default();
        key.name("rivers").number(spacing).number(set.source).number(set.lakes).number(set.width);
        key.number(set.depth).number(set.valley).number(set.falls).number(set.meander).number(sea).whole(seed);
        key.whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        Self {
            spacing,
            source: points(number(set.source, 0.5, 100000.0)),
            lakes: if set.lakes > 0.0 { points(number(set.lakes, 0.0, 100000.0)) } else { 0 },
            width: number(set.width, 1.0, 1000.0) * scale,
            depth: number(set.depth, 0.2, 100.0),
            valley: number(set.valley, 0.0, 2000.0) * scale,
            falls: number(set.falls, 1.0, 1000.0),
            meander: number(set.meander, 0.0, 1.0),
            sea,
            seed,
            ground,
            id: key.hash(),
        }
    }

    /// Where point `cell` of the grid lies.
    fn place(&self, cell: [i32; 2]) -> [f32; 2] {
        let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
        [
            (cell[0] as f32 + 0.5 + JITTER * (pick(0) - 0.5)) * self.spacing,
            (cell[1] as f32 + 0.5 + JITTER * (pick(1) - 0.5)) * self.spacing,
        ]
    }

    /// The points of tile `tile`, kept from the last time they were worked
    /// out where they can be.
    fn tile(&self, tile: [i32; 2]) -> Arc<Vec<Point>> {
        let kept = |tiles: &Vec<(u64, [i32; 2], Arc<Vec<Point>>)>| {
            tiles.iter().find(|(id, known, _)| *id == self.id && *known == tile).map(|kept| kept.2.clone())
        };
        if let Some(kept) = kept(&TILES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())) {
            return kept;
        }
        // Worked out with the tiles let go of, so other threads go on; two threads may both work one out, alike.
        let made = Arc::new(self.make_tile(tile));
        let mut tiles = TILES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(kept) = kept(&tiles) {
            return kept;
        }
        if tiles.len() >= KEPT {
            tiles.remove(0);
        }
        tiles.push((self.id, tile, made.clone()));
        made
    }

    /// The ground at the points of the `side` by `side` square from point
    /// `first`, row after row: read from the blocks of points kept, those
    /// not kept read now, all at once, and kept.
    fn grounds(&self, first: [i32; 2], side: usize) -> Vec<f32> {
        let block = |v: i32| v.div_euclid(GROUND_BLOCK);
        let (low, high) = (
            [block(first[0]), block(first[1])],
            [block(first[0] + side as i32 - 1), block(first[1] + side as i32 - 1)],
        );
        let wanted: Vec<[i32; 2]> = (low[1]..=high[1]).flat_map(|z| (low[0]..=high[0]).map(move |x| [x, z])).collect();
        let mut found: Vec<([i32; 2], Arc<Vec<f32>>)> = {
            let kept = GROUNDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let id = self.ground.hash ^ u64::from(self.seed) ^ u64::from(self.spacing.to_bits()) << 32;
            wanted
                .iter()
                .filter_map(|&at| Some((at, kept.iter().find(|(k, b, _)| *k == id && *b == at)?.2.clone())))
                .collect()
        };
        let missing: Vec<[i32; 2]> =
            wanted.iter().copied().filter(|at| found.iter().all(|(known, _)| known != at)).collect();
        if !missing.is_empty() {
            let each = (GROUND_BLOCK * GROUND_BLOCK) as usize;
            let (xs, zs): (Vec<f32>, Vec<f32>) = missing
                .iter()
                .flat_map(|at| {
                    (0..each as i32).map(move |k| {
                        self.place([at[0] * GROUND_BLOCK + k % GROUND_BLOCK, at[1] * GROUND_BLOCK + k / GROUND_BLOCK])
                    })
                })
                .map(|at| (at[0], at[1]))
                .unzip();
            let mut read = vec![0.0; xs.len()];
            self.ground.run(xs.len(), Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut read]);
            let id = self.ground.hash ^ u64::from(self.seed) ^ u64::from(self.spacing.to_bits()) << 32;
            let mut kept = GROUNDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for (k, &at) in missing.iter().enumerate() {
                let values = Arc::new(read[k * each..(k + 1) * each].to_vec());
                if kept.len() >= GROUNDS_KEPT {
                    kept.remove(0);
                }
                kept.push((id, at, values.clone()));
                found.push((at, values));
            }
        }
        let mut grounds = vec![0.0; side * side];
        for (at, ground) in grounds.iter_mut().enumerate() {
            let (x, z) = (first[0] + (at % side) as i32, first[1] + (at / side) as i32);
            let held = [block(x), block(z)];
            let values = &found.iter().find(|(known, _)| *known == held).expect("read above").1;
            *ground = values[(z.rem_euclid(GROUND_BLOCK) * GROUND_BLOCK + x.rem_euclid(GROUND_BLOCK)) as usize];
        }
        grounds
    }

    fn make_tile(&self, tile: [i32; 2]) -> Vec<Point> {
        let first = [tile[0] * TILE - MARGIN, tile[1] * TILE - MARGIN];
        let side = (TILE + 2 * MARGIN) as usize;
        let index = |x: usize, z: usize| z * side + x;
        let places: Vec<[f32; 2]> = (0..side * side)
            .map(|at| self.place([first[0] + (at % side) as i32, first[1] + (at / side) as i32]))
            .collect();
        let read = self.grounds(first, side);
        // Each point's ground averaged with the eight round it, so water finds the broad valleys and not the gullies
        // and bumps between points, which leave no hollows to stop it. The window's outer ring keeps its own.
        let mut ground = read.clone();
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let mut sum = 0.0;
                for (dx, dz, weight) in AVERAGED {
                    sum += read[index((x as i32 + dx) as usize, (z as i32 + dz) as usize)] * weight;
                }
                ground[index(x, z)] = sum;
            }
        }
        // Where each point drains: the steepest way down to one of the eight round it. One with none lower is a
        // hollow.
        let mut down: Vec<Option<[i8; 2]>> = vec![None; places.len()];
        let mut hollow = vec![false; places.len()];
        for z in 2..side - 2 {
            for x in 2..side - 2 {
                let here = index(x, z);
                let mut steepest = 0.0f32;
                for (dx, dz) in ROUND {
                    let there = index((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                    let fall = ground[here] - ground[there];
                    if fall <= 0.0 {
                        continue;
                    }
                    let (ax, az) = (places[there][0] - places[here][0], places[there][1] - places[here][1]);
                    let slope = fall / (ax * ax + az * az).sqrt().max(1e-3);
                    if slope > steepest {
                        (steepest, down[here]) = (slope, Some([dx as i8, dz as i8]));
                    }
                }
                hollow[here] = down[here].is_none();
            }
        }
        // A hollow spills over to the lowest point within two of it whose way down does not lead back into it, its
        // river cutting through the ground between; one with none keeps its water, a sink. Each hollow is decided
        // by the ways down alone, near it, so every tile decides it alike.
        let steepest = down.clone();
        let mut spill: Vec<(usize, f32)> = Vec::with_capacity(24);
        for z in SPILL + 1..side - SPILL - 1 {
            for x in SPILL + 1..side - SPILL - 1 {
                let here = index(x, z);
                if !hollow[here] {
                    continue;
                }
                spill.clear();
                let reach = SPILL as i32;
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        if (dx, dz) != (0, 0) {
                            spill.push((index((x as i32 + dx) as usize, (z as i32 + dz) as usize), 0.0));
                        }
                    }
                }
                for candidate in spill.iter_mut() {
                    candidate.1 = ground[candidate.0];
                }
                spill.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                let leads_back = |from: usize| {
                    let (mut vx, mut vz) = (from % side, from / side);
                    for _ in 0..SPILL_STEPS {
                        let Some([dx, dz]) = steepest[index(vx, vz)] else { return index(vx, vz) == here };
                        (vx, vz) = ((vx as i32 + i32::from(dx)) as usize, (vz as i32 + i32::from(dz)) as usize);
                        if vx < 1 || vz < 1 || vx >= side - 1 || vz >= side - 1 {
                            return false;
                        }
                    }
                    index(vx, vz) == here
                };
                if let Some(&(over, _)) = spill.iter().find(|(candidate, _)| !leads_back(*candidate)) {
                    let (ox, oz) = ((over % side) as i32 - x as i32, (over / side) as i32 - z as i32);
                    down[here] = Some([ox as i8, oz as i8]);
                }
            }
        }
        // How many points drain through each of the tile's, and those round it: every point within reach up its
        // way, whichever tile counts it.
        let counted = |x: usize, z: usize| {
            let inner = (MARGIN - KNOWN) as usize..(MARGIN + TILE + KNOWN) as usize;
            inner.contains(&x) && inner.contains(&z)
        };
        let mut catch = vec![0u32; places.len()];
        let mut been: Vec<(i32, i32)> = Vec::new();
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let (mut vx, mut vz) = (x as i32, z as i32);
                been.clear();
                loop {
                    // Water spilling from a hollow may come round to it again: it is counted once.
                    if been.contains(&(vx, vz)) {
                        break;
                    }
                    been.push((vx, vz));
                    let (ox, oz) = (vx - x as i32, vz - z as i32);
                    if ox * ox + oz * oz > REACH * REACH {
                        break;
                    }
                    if counted(vx as usize, vz as usize) {
                        catch[index(vx as usize, vz as usize)] += 1;
                    }
                    let Some([dx, dz]) = down[index(vx as usize, vz as usize)] else { break };
                    (vx, vz) = (vx + i32::from(dx), vz + i32::from(dz));
                    if vx < 1 || vz < 1 || vx >= side as i32 - 1 || vz >= side as i32 - 1 {
                        break;
                    }
                }
            }
        }
        // A hollow's lake, as high as the lowest of the ground round it, where enough drains into it.
        let lake = |x: usize, z: usize| -> Option<f32> {
            let here = index(x, z);
            if self.lakes == 0 || !hollow[here] || catch[here] < self.lakes || ground[here] < self.sea {
                return None;
            }
            // As high as the lowest of the ground round it, and no higher than where it spills.
            let mut rim = f32::MAX;
            for (dx, dz) in ROUND {
                rim = rim.min(ground[index((x as i32 + dx) as usize, (z as i32 + dz) as usize)]);
            }
            if let Some([dx, dz]) = down[here] {
                rim = rim.min(ground[index((x as i32 + i32::from(dx)) as usize, (z as i32 + i32::from(dz)) as usize)]);
            }
            let level = rim - 0.3;
            (level > ground[here] + 0.5).then_some(level)
        };
        let size = |catch: u32| {
            let area = catch as f32 * self.spacing * self.spacing / 1_000_000.0;
            let root = area.sqrt();
            ((self.width * root).clamp(2.5, 120.0), (self.depth * root.sqrt()).clamp(1.0, 20.0))
        };
        let point = |x: usize, z: usize| {
            let here = index(x, z);
            let (width, depth) = size(catch[here]);
            let mut point = Point { at: places[here], down: down[here], width, depth, ..Point::default() };
            if let Some(level) = lake(x, z) {
                // Its water runs on from it where it spills, as a river where enough drains into it.
                (point.lake, point.level) = (true, level);
                point.river = catch[here] >= self.source;
            } else {
                point.river = catch[here] >= self.source && ground[here] >= self.sea && down[here].is_some();
                let mut level = ground[here] - depth;
                // Running into a lake, as high as it.
                if let Some([dx, dz]) = down[here]
                    && let Some(lake) = lake((x as i32 + i32::from(dx)) as usize, (z as i32 + i32::from(dz)) as usize)
                {
                    level = level.max(lake);
                }
                point.level = level.max(self.sea);
            }
            point
        };
        // The tile's points and those round it as far as a place in it looks.
        let from = (MARGIN - LOOKED) as usize;
        let wide = (TILE + 2 * LOOKED) as usize;
        let round: Vec<Point> = (0..wide * wide).map(|at| point(from + at % wide, from + at / wide)).collect();
        let wet = |x: usize, z: usize| round[z * wide + x].river || round[z * wide + x].lake;
        let mut points = Vec::with_capacity((TILE * TILE) as usize);
        for z in LOOKED as usize..(LOOKED + TILE) as usize {
            for x in LOOKED as usize..(LOOKED + TILE) as usize {
                let reach = LOOKED as usize;
                let near = (z - reach..=z + reach).any(|z| (x - reach..=x + reach).any(|x| wet(x, z)));
                points.push(Point { near, ..round[z * wide + x] });
            }
        }
        points
    }
}

/// Points looked up by their place in the grid, the tiles they are in kept
/// at hand while a row is worked out.
struct Near<'a> {
    finder: &'a Finder,
    tiles: Vec<([i32; 2], Arc<Vec<Point>>)>,
}

impl Near<'_> {
    fn point(&mut self, cell: [i32; 2]) -> Point {
        let tile = [cell[0].div_euclid(TILE), cell[1].div_euclid(TILE)];
        let local = (cell[1].rem_euclid(TILE) * TILE + cell[0].rem_euclid(TILE)) as usize;
        if let Some((_, points)) = self.tiles.iter().find(|(known, _)| *known == tile) {
            return points[local];
        }
        let points = self.finder.tile(tile);
        let point = points[local];
        if self.tiles.len() >= 8 {
            self.tiles.remove(0);
        }
        self.tiles.push((tile, points));
        point
    }
}

/// What a river makes of a place near it: how far past its edge the place
/// is (less than nothing in it), its water's height there, how high its
/// banks are kept over, its bed, and how many metres of a waterfall pour
/// down there.
#[derive(Clone, Copy, Debug)]
struct Reached {
    past: f32,
    water: f32,
    level: f32,
    bed: f32,
    falls: f32,
}

/// The ground, water, falls and banks a place gets.
struct Made {
    height: f32,
    water: f32,
    falls: f32,
    banks: f32,
}

impl Finder {
    /// What the rivers and lakes near `(x, z)` make of ground `ground` high
    /// there.
    fn at(&self, near: &mut Near, x: f32, z: f32, ground: f32) -> Made {
        let mut made = Made { height: ground, water: NO_WATER, falls: 0.0, banks: 0.0 };
        // Read where the world is bent, so rivers wind between their points.
        let bend = self.meander * self.spacing * 0.45;
        let wide = self.spacing * 1.7;
        let (wx, wz) = (
            x + bend * gradient(x / wide, z / wide, self.seed.wrapping_add(11)),
            z + bend * gradient(x / wide, z / wide, self.seed.wrapping_add(12)),
        );
        let cell = [(wx / self.spacing).floor() as i32, (wz / self.spacing).floor() as i32];
        if !near.point(cell).near {
            return made;
        }
        let mut nearest: Option<Reached> = None;
        let mut lakes: Vec<Point> = Vec::new();
        for dz in -LOOKED..=LOOKED {
            for dx in -LOOKED..=LOOKED {
                let from = [cell[0] + dx, cell[1] + dz];
                let point = near.point(from);
                if point.lake {
                    lakes.push(point);
                }
                let Some([sx, sz]) = point.down.filter(|_| point.river) else { continue };
                let to = near.point([from[0] + i32::from(sx), from[1] + i32::from(sz)]);
                if let Some(river) = self.stretch(&point, &to, [wx, wz]) {
                    if nearest.is_none_or(|nearest| river.past < nearest.past) {
                        nearest = Some(river);
                    }
                }
            }
        }
        if let Some(river) = nearest {
            let top = river.level + BANK;
            if river.past < 0.0 {
                made.height = ground.min(river.bed);
                (made.water, made.falls, made.banks) = (river.water, river.falls, 1.0);
            } else if ground > top {
                // The valley's sides slope down to the banks.
                let valley = self.valley.max(1.0);
                made.height = top + (ground - top) * smooth(river.past / valley);
                made.banks = 1.0 - smooth(river.past / (valley * 0.5));
            } else {
                // Banks raised where the ground is lower than the water, level for a few metres from its edge.
                made.height = top + (ground - top) * smooth((river.past - BANK_TOP) / LEVEE);
                made.banks = 1.0 - smooth(river.past / (self.valley.max(1.0) * 0.5));
            }
        }
        for lake in lakes {
            let (ox, oz) = (wx - lake.at[0], wz - lake.at[1]);
            let from = (ox * ox + oz * oz).sqrt();
            let reach = LAKE_REACH * self.spacing;
            if from < reach && made.height < lake.level {
                made.water = made.water.max(lake.level);
            } else if from < reach + BANK_TOP + LEVEE && made.water == NO_WATER {
                // A rim round it where the ground would let it out, but where a river runs in.
                made.height = made.height.max(lake.level + BANK);
            }
        }
        made
    }

    /// What the stretch of river from `from` down to `to` makes of place
    /// `at`, where it is near enough to change it.
    fn stretch(&self, from: &Point, to: &Point, at: [f32; 2]) -> Option<Reached> {
        let (ax, az) = (to.at[0] - from.at[0], to.at[1] - from.at[1]);
        let long = (ax * ax + az * az).sqrt().max(1e-3);
        let (px, pz) = (at[0] - from.at[0], at[1] - from.at[1]);
        let t = ((px * ax + pz * az) / (long * long)).clamp(0.0, 1.0);
        let (cx, cz) = (px - ax * t, pz - az * t);
        let off = (cx * cx + cz * cz).sqrt();
        // As wide and deep as its upper end, and the lower's where a river goes on from it.
        let (width, depth) = match to.river || to.lake {
            true => (from.width + (to.width - from.width) * t, from.depth + (to.depth - from.depth) * t),
            false => (from.width, from.depth),
        };
        let past = off - width * 0.5;
        if past > self.valley.max(BANK_TOP + LEVEE) {
            return None;
        }
        // Its water falls along every stretch: never higher at its lower end than at its upper.
        let (upper, lower) = (from.level, to.level.min(from.level));
        let drop = upper - lower;
        let along = t * long;
        let across = (off / (width * 0.5).max(0.01)).min(1.0);
        let bed = |level: f32, deeper: f32| level - (depth + deeper) * (1.0 - 0.6 * across * across);
        if drop <= self.falls {
            let level = upper - drop * t;
            return Some(Reached { past, water: level, level, bed: bed(level, 0.0), falls: 0.0 });
        }
        // A waterfall halfway: nearly level to its lip, over it into a pool, nearly level on from there.
        let lip = long * 0.5;
        let (top, foot) = (upper - drop * RUN_DOWN, lower + drop * RUN_DOWN);
        if along < lip {
            let level = upper - (upper - top) * (along / lip);
            return Some(Reached { past, water: level, level, bed: bed(level, 0.0), falls: 0.0 });
        }
        let level = foot - (foot - lower) * ((along - lip) / (long - lip).max(1e-3));
        let pool = depth * 0.8 * (1.0 - (along - lip) / POOL).max(0.0);
        let curtain = along < lip + CURTAIN;
        let (water, falls) = if curtain { (top, top - level) } else { (level, 0.0) };
        Some(Reached { past, water, level, bed: bed(level, pool), falls })
    }
}

/// A Rivers node's ground, water, falls or banks at each place, by `what`
/// (its output's number), from rows of x, z and the ground wired in.
#[derive(Debug)]
struct Ran {
    finder: Arc<Finder>,
    what: u8,
}

impl Kernel for Ran {
    fn key(&self, key: &mut Key) {
        key.name("rivers ran").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
        key.whole(u32::from(self.what));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut near = Near { finder: &self.finder, tiles: Vec::new() };
        for (at, out) in out.iter_mut().enumerate() {
            let made = self.finder.at(&mut near, inputs[0][at], inputs[1][at], inputs[2][at]);
            *out = match usize::from(self.what) {
                Rivers::HEIGHT => made.height,
                Rivers::WATER => made.water,
                Rivers::FALLS => made.falls,
                _ => made.banks,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, WorldGen, nodes::Output, nodes::testing, starters};

    /// Starter `name`'s ground with rivers run down it, their water and
    /// falls wired into the Output.
    pub(crate) fn rivered(name: &str, set: Rivers) -> WorldGen {
        let starter = starters().into_iter().find(|starter| starter.name == name).expect("a starter");
        let mut graph = starter.graph;
        let output = graph.output().expect("an output").id;
        let link = *graph.link_into(output, Output::HEIGHT).expect("its ground");
        let rivers = graph.add(NodeKind::Rivers(set), [0.0, 0.0]);
        assert!(graph.connect(link.from, link.output, rivers, Rivers::GROUND));
        assert!(graph.connect(rivers, Rivers::HEIGHT, output, Output::HEIGHT));
        assert!(graph.connect(rivers, Rivers::WATER, output, Output::WATER));
        assert!(graph.connect(rivers, Rivers::FALLS, output, Output::FALLS));
        WorldGen::new(7, 1.0, starter.sea_level, graph)
    }

    /// Places `step` metres apart over a square `side` places a side from
    /// `(x, z)`.
    fn grid(x: f32, z: f32, step: f32, side: usize) -> (Vec<f32>, Vec<f32>) {
        (0..side * side).map(|at| (x + (at % side) as f32 * step, z + (at / side) as f32 * step)).unzip()
    }

    fn wet(water: f32) -> bool {
        water > NO_WATER * 0.5
    }

    #[test]
    fn rivers_run_down_valleys_between_banks_that_hold_their_water_in() {
        let world = rivered("Rolling Hills", Rivers::default());
        testing::checked(&world);
        let side = 400;
        let (xs, zs) = grid(-800.0, -800.0, 5.0, side);
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let (water, _) = world.water(&xs, &zs).expect("water wired in");
        let wet_places: Vec<usize> = (0..xs.len()).filter(|&at| wet(water[at])).collect();
        let share = wet_places.len() as f32 / xs.len() as f32;
        assert!(share > 0.003 && share < 0.2, "{share} of the land under rivers and lakes");
        // The ground under water lies under it.
        assert!(wet_places.iter().all(|&at| heights[at] <= water[at] + 1e-3));
        // A metre either way from water, the ground is as high as the water or there is water too: no water
        // stands over lower dry ground beside it, as a wall of it would.
        let (mut asked, mut walls) = (0, 0);
        for &at in wet_places.iter().step_by(3) {
            let xs = [xs[at] - 1.0, xs[at] + 1.0, xs[at], xs[at]];
            let zs = [zs[at], zs[at], zs[at] - 1.0, zs[at] + 1.0];
            let mut beside = [0.0; 4];
            world.heights(&xs, &zs, &mut beside);
            let (beside_water, _) = world.water(&xs, &zs).unwrap();
            for k in 0..4 {
                asked += 1;
                if !wet(beside_water[k]) && beside[k] < water[at] - 0.3 {
                    walls += 1;
                }
            }
        }
        assert!(walls * 1000 <= asked, "{walls} of {asked} places beside water lower than it and dry");
        // Rivers run downhill: a few metres on the way the water flows, it stands no higher.
        let mut uphill = 0;
        let mut flowing = 0;
        for &at in wet_places.iter().step_by(2) {
            let Some([fx, fz]) = world.current(xs[at], zs[at]) else { continue };
            let speed = (fx * fx + fz * fz).sqrt();
            if speed < 1e-3 {
                continue;
            }
            flowing += 1;
            let on = [xs[at] + fx / speed * 3.0, zs[at] + fz / speed * 3.0];
            let (there, _) = world.water(&[on[0]], &[on[1]]).unwrap();
            if wet(there[0]) && there[0] > water[at] + 0.05 {
                uphill += 1;
            }
        }
        assert!(flowing > 100, "{flowing} places where the water flows");
        assert!(uphill * 50 <= flowing, "{uphill} of {flowing} run uphill");
    }

    #[test]
    fn a_river_dropping_down_a_mountainside_pours_over_waterfalls_into_pools() {
        let world = rivered("Alpine", Rivers::default());
        let (step, side) = (6.0, 400);
        let (xs, zs) = grid(-1200.0, -1200.0, step, side);
        let (water, _) = world.water(&xs, &zs).unwrap();
        // Where the water a few metres on stands much lower, a waterfall is between: looked for a metre at a time.
        let mut falls = Vec::new();
        for at in (0..xs.len()).filter(|&at| wet(water[at]) && at % side + 1 < side) {
            for next in [at + 1, at + side] {
                if next >= xs.len() || !wet(water[next]) || (water[at] - water[next]).abs() < 4.5 {
                    continue;
                }
                let line: (Vec<f32>, Vec<f32>) = (0..=24)
                    .map(|k| k as f32 / 24.0)
                    .map(|t| (xs[at] + (xs[next] - xs[at]) * t, zs[at] + (zs[next] - zs[at]) * t))
                    .unzip();
                let (high, fall) = world.water(&line.0, &line.1).unwrap();
                let mut ground = vec![0.0; line.0.len()];
                world.heights(&line.0, &line.1, &mut ground);
                falls.extend((0..line.0.len()).filter(|&k| fall[k] > 0.0).map(|k| (high[k], fall[k], ground[k])));
            }
        }
        assert!(!falls.is_empty(), "no waterfall");
        for &(top, fall, ground) in &falls {
            assert!(fall > 4.0, "a waterfall {fall} m high");
            // Its curtain pours into a pool: the pool's water stands over the ground under it.
            assert!(top - fall > ground, "a curtain from {top} falling {fall} onto ground at {ground}");
        }
    }
}
