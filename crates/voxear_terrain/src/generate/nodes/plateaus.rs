//! The Plateaus node: land in wide, level tablelands at uneven heights,
//! with walls wherever two of unlike height meet. Levels is how many
//! heights there are, from the valley floor at Height From to the highest
//! at Height To; Size is how wide a tableland runs before the next. Ones of
//! the same height side by side are one, so they come in every shape.
//!
//! Wall is how steep the walls stand, and Talus how high the slope of
//! rubble at their foot. Ramps is how many of the walls have a ramp built
//! against them: a stair of steps, each Step high, climbing at Ramp Slope,
//! that a player walks up from one level to the next. Rim raises a lip
//! along each level's edge, or under nothing rounds the edge off. Valleys
//! is how much of the land the lowest level takes, and River how deep a
//! winding bed is cut into it. Canyon is how wide a canyon the river cuts
//! through the tablelands it meets, down to the valley floor, so the
//! valleys are joined; with none the river keeps to the valleys. Wander
//! bends the walls' lines.
//!
//! A mask wired into Ramps keeps the ramps to where it is over a half.
//!
//! Beside the ground's height it gives two masks: Walls, 1 on the walls
//! and their rubble, and Floor, 1 on the lowest level clear of the walls.
//!
//! The land is shared out among places a Size apart, each moved a little
//! by the seed, every place on the ground belonging to the nearest. Each
//! has a level, near ones alike more often than not. The ground is as high
//! as the highest thing reaching it: its own level, or a neighbour's wall,
//! rubble or ramp, each falling away from the line where the two meet.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, noise3d::seed_for};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::{self, NoiseStyle},
};

/// The most levels there are.
const MOST_LEVELS: usize = 12;

/// How far either side of its line a river's bed reaches, in metres, and
/// how far a line is said to be where there is none.
const BED: f32 = 14.0;
const FAR: f32 = 1.0e9;

/// How steep a wall's rubble lies, as rise over run.
const RUBBLE: f32 = 0.7;

/// Land in `levels` level steps from `low` to `high` metres, `size` metres
/// wide. `wall`, and `ramp_slope`, are angles kept as rise over run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plateaus {
    pub size: f32,
    pub levels: u32,
    pub low: f32,
    pub high: f32,
    pub wall: f32,
    pub ramps: f32,
    pub rim: f32,
    pub talus: f32,
    pub valleys: f32,
    pub river: f32,
    pub canyon: f32,
    pub wander: f32,
    pub ramp_slope: f32,
    pub step: f32,
    pub seed: u32,
}

impl Default for Plateaus {
    fn default() -> Self {
        Self {
            size: 700.0,
            levels: 4,
            low: 0.0,
            high: 160.0,
            wall: 5.0,
            ramps: 0.35,
            rim: 0.0,
            talus: 8.0,
            valleys: 0.35,
            river: 0.0,
            canyon: 0.0,
            wander: 0.5,
            ramp_slope: 0.5,
            step: 1.0,
            seed: 0,
        }
    }
}

impl Plateaus {
    /// Its input, among its inputs, and its outputs.
    pub const RAMPS: usize = 0;
    pub const HEIGHT: usize = 0;
    pub const WALLS: usize = 1;
    pub const FLOOR: usize = 2;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size", "Size", 1.0, f32::MAX, "m"),
    ValueSpec::whole("levels", "Levels", 2, MOST_LEVELS as u32),
    ValueSpec::number("low", "Height From", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("high", "Height To", f32::MIN, f32::MAX, "m"),
    ValueSpec::angle("wall", "Wall"),
    ValueSpec::number("ramps", "Ramps", 0.0, 1.0, ""),
    ValueSpec::number("rim", "Rim", -100.0, 100.0, "m"),
    ValueSpec::number("talus", "Talus", 0.0, 1000.0, "m"),
    ValueSpec::number("valleys", "Valleys", 0.0, 1.0, ""),
    ValueSpec::number("river", "River", 0.0, 1000.0, "m"),
    ValueSpec::number("canyon", "Canyon", 0.0, 10000.0, "m"),
    ValueSpec::number("wander", "Wander", 0.0, 1.0, ""),
    ValueSpec::angle("ramp_slope", "Ramp Slope").advanced(),
    ValueSpec::number("step", "Step", 0.1, 100.0, "m").advanced(),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::field("ramps", "Ramps")];

const OUTPUTS: &[Port] =
    &[Port::field("height", "Height"), Port::field("walls", "Walls"), Port::field("floor", "Floor")];

impl Kind for Plateaus {
    const TITLE: &'static str = "Plateaus";
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
            0 => Value::Number(self.size),
            1 => Value::Whole(self.levels),
            2 => Value::Number(self.low),
            3 => Value::Number(self.high),
            4 => Value::Number(self.wall),
            5 => Value::Number(self.ramps),
            6 => Value::Number(self.rim),
            7 => Value::Number(self.talus),
            8 => Value::Number(self.valleys),
            9 => Value::Number(self.river),
            10 => Value::Number(self.canyon),
            11 => Value::Number(self.wander),
            12 => Value::Number(self.ramp_slope),
            13 => Value::Number(self.step),
            14 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size = number,
            (1, Value::Whole(whole)) => self.levels = whole,
            (2, Value::Number(number)) => self.low = number,
            (3, Value::Number(number)) => self.high = number,
            (4, Value::Number(number)) => self.wall = number,
            (5, Value::Number(number)) => self.ramps = number,
            (6, Value::Number(number)) => self.rim = number,
            (7, Value::Number(number)) => self.talus = number,
            (8, Value::Number(number)) => self.valleys = number,
            (9, Value::Number(number)) => self.river = number,
            (10, Value::Number(number)) => self.canyon = number,
            (11, Value::Number(number)) => self.wander = number,
            (12, Value::Number(number)) => self.ramp_slope = number,
            (13, Value::Number(number)) => self.step = number,
            (14, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let seed = seed_for(cx, node, self.seed);
        let size = (self.size * cx.world.scale.max(0.01)).max(1.0);
        let ramps = cx.input(node, Self::RAMPS, at).unwrap_or_else(|| cx.constant(1.0));
        let river = match self.river > 0.0 || self.canyon > 0.0 {
            true => cx.run(Winding { size: size * 1.6, seed }, &[at.x, at.z]),
            false => cx.constant(FAR),
        };
        cx.run(Cut::new(self, size, seed, port), &[at.x, at.z, ramps, river])
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far from a river's winding line each place is, in metres, from
/// rows of x and z: the line is where noise is nothing, and how far it is
/// comes from how fast the noise changes.
#[derive(Debug)]
struct Winding {
    size: f32,
    seed: u32,
}

impl Kernel for Winding {
    fn key(&self, key: &mut Key) {
        key.name("plateau river").number(self.size).whole(self.seed);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            // A wide line with smaller bends along it.
            let fine = self.size * 0.27;
            let read = |x: f32, z: f32| {
                noise::gradient(x / self.size, z / self.size, self.seed.wrapping_add(0x5A))
                    + 0.22 * noise::gradient(x / fine, z / fine, self.seed.wrapping_add(0x5B))
            };
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let noise = read(x, z);
            let (along, across) = ((read(x + 4.0, z) - noise) / 4.0, (read(x, z + 4.0) - noise) / 4.0);
            let climb = (along * along + across * across).sqrt();
            let off = if noise < 0.0 { -noise } else { noise };
            // Where the noise runs level the line is far off, however near nothing the noise is.
            *out = off / if climb > 0.2 / self.size { climb } else { 0.2 / self.size };
        }
    }
}

/// A number from 0 to 1 from a whole number.
pub(super) fn share(hash: u32) -> f32 {
    (hash >> 8) as f32 / (1u32 << 24) as f32
}

pub(super) fn mix(seed: u32, values: [i32; 3]) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    for value in values {
        h = (h ^ value as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^= h >> 13;
    }
    h
}

/// One tableland's place: where it is, and how high.
#[derive(Clone, Copy, Debug)]
struct Site {
    cell: [i32; 2],
    at: [f32; 2],
    level: usize,
}

/// The ground cut into tablelands, from rows of x and z, where ramps may
/// be, and how far from a river's line each place is.
#[derive(Debug)]
struct Cut {
    size: f32,
    seed: u32,
    /// Each level's height, lowest first, and the number each starts at.
    heights: Vec<f32>,
    edges: Vec<f32>,
    wall: f32,
    talus: f32,
    rim: f32,
    river: f32,
    canyon: f32,
    ramps: f32,
    ramp_slope: f32,
    step: f32,
    wander: f32,
    /// Which output it makes.
    port: usize,
}

/// How many places' tablelands a run keeps at hand, by where they are.
const HELD: usize = 64;

impl Cut {
    fn new(set: &Plateaus, size: f32, seed: u32, port: usize) -> Self {
        let levels = (set.levels as usize).clamp(2, MOST_LEVELS);
        let between = (levels - 1) as f32;
        // A number from -1 to 1 for each level, its own by the seed.
        let own = |level: usize, part: i32| share(mix(seed, [level as i32, part, 7])) * 2.0 - 1.0;
        let heights = (0..levels).map(|level| {
            let along = match level {
                0 => 0.0,
                level if level == levels - 1 => 1.0,
                level => (level as f32 + own(level, 0) * 0.3) / between,
            };
            set.low + (set.high - set.low) * along
        });
        // The lowest level takes what Valleys says of the numbers; the rest share what is over it.
        let first = 0.15 + 0.5 * set.valleys.clamp(0.0, 1.0);
        let edges = (0..levels).map(|level| match level {
            0 => f32::MIN,
            level => first + (1.0 - first) * (level - 1) as f32 / between,
        });
        Self {
            size,
            seed,
            heights: heights.collect(),
            edges: edges.collect(),
            wall: if set.wall > 0.05 { set.wall } else { 0.05 },
            talus: set.talus.max(0.0),
            rim: set.rim,
            river: set.river.max(0.0),
            canyon: set.canyon.max(0.0),
            ramps: set.ramps.clamp(0.0, 1.0),
            ramp_slope: if set.ramp_slope > 0.02 { set.ramp_slope } else { 0.02 },
            step: if set.step > 0.1 { set.step } else { 0.1 },
            wander: set.wander.clamp(0.0, 1.0),
            port,
        }
    }

    /// The tableland whose place is in `cell`.
    fn site(&self, cell: [i32; 2]) -> Site {
        let moved = |part: i32| share(mix(self.seed, [cell[0], cell[1], part])) * 0.8 + 0.1;
        let at = [(cell[0] as f32 + moved(0)) * self.size, (cell[1] as f32 + moved(1)) * self.size];
        // Near ones are alike more often than not: a level from noise several of them wide, moved by its own.
        let wide = self.size * 3.2;
        let noise = noise::fractal(NoiseStyle::Fractal, at[0] / wide, at[1] / wide, 2, self.seed.wrapping_add(0x56));
        let number = 0.5 + (noise - 0.5) * 2.4 + (moved(2) - 0.5) * 0.3;
        let level = (1..self.heights.len()).rev().find(|&level| number >= self.edges[level]).unwrap_or(0);
        Site { cell, at, level }
    }
}

impl Kernel for Cut {
    fn key(&self, key: &mut Key) {
        key.name("plateaus").whole(self.port as u32).whole(self.heights.len() as u32);
        for (height, edge) in self.heights.iter().zip(&self.edges) {
            key.number(*height).number(*edge);
        }
        key.number(self.size).whole(self.seed).number(self.wall).number(self.talus).number(self.rim);
        key.number(self.river)
            .number(self.canyon)
            .number(self.ramps)
            .number(self.ramp_slope)
            .number(self.step)
            .number(self.wander);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let (raised, rounded) =
            (if self.rim > 0.0 { self.rim } else { 0.0 }, if self.rim < 0.0 { -self.rim } else { 0.0 });
        let mut held: [Option<Site>; HELD] = [None; HELD];
        let mut site = |cell: [i32; 2]| {
            let slot = (cell[0].rem_euclid(8) * 8 + cell[1].rem_euclid(8)) as usize;
            match held[slot] {
                Some(site) if site.cell == cell => site,
                _ => {
                    let site = self.site(cell);
                    held[slot] = Some(site);
                    site
                }
            }
        };
        // Three sizes of bend, the widest moving a line furthest.
        let bends = [(1.3, 0.2), (0.35, 0.06), (0.1, 0.015)]
            .map(|(wide, far)| (self.size * wide, self.wander * self.size * far));
        let wide = self.size * 0.1 + 12.0;
        for (at, out) in out.iter_mut().enumerate() {
            // The walls' lines are bent by reading the land a little way off.
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let (mut px, mut pz) = (x, z);
            for (octave, (bend, far)) in bends.into_iter().enumerate() {
                let seed = self.seed.wrapping_add(0x51 + octave as u32 * 2);
                px += far * noise::gradient(x / bend, z / bend, seed);
                pz += far * noise::gradient(x / bend, z / bend, seed.wrapping_add(1));
            }
            let cell = [(px / self.size).floor() as i32, (pz / self.size).floor() as i32];
            // The nearest place of the twenty-five round here is the tableland this is on.
            let mut near: [(Site, f32); 25] = [(Site { cell, at: [0.0; 2], level: 0 }, 0.0); 25];
            let mut own = 0;
            for k in 0..25 {
                let found = site([cell[0] + k as i32 % 5 - 2, cell[1] + k as i32 / 5 - 2]);
                let (dx, dz) = (px - found.at[0], pz - found.at[1]);
                near[k] = (found, dx * dx + dz * dz);
                if near[k].1 < near[own].1 {
                    own = k;
                }
            }
            let (mine, least) = near[own];
            let level = self.heights[mine.level];
            // What reaches here from each higher one: its wall, the rubble at its foot, a ramp against it.
            let (mut high, mut walled, mut clear) = (level, 0.0, f32::MAX);
            let mut edge = f32::MAX;
            for (other, apart) in near.iter().filter(|(other, _)| other.cell != mine.cell) {
                let (ax, az) = (other.at[0] - mine.at[0], other.at[1] - mine.at[1]);
                let between = (ax * ax + az * az).sqrt();
                // How far the line where the two meet is.
                let away = (apart - least) / (2.0 * between);
                if other.level < mine.level {
                    edge = if away < edge { away } else { edge };
                    continue;
                }
                if other.level == mine.level {
                    continue;
                }
                let top = self.heights[other.level] - rounded;
                let rise = top - level;
                let rubble = if self.talus < rise * 0.4 { self.talus } else { rise * 0.4 };
                let sheer = top - away * self.wall;
                let foot = (rise - rubble) / self.wall;
                let scree = level + rubble - (away - foot) * RUBBLE;
                let wall = if sheer > scree { sheer } else { scree };
                clear = if away - foot - rubble / RUBBLE < clear { away - foot - rubble / RUBBLE } else { clear };
                if wall > high {
                    (high, walled) = (wall, 1.0);
                }
                // A ramp against this wall, for the pairs the seed picks: a stair as wide as `wide` down from the
                // line, about the middle of it.
                let (low, up) = if mine.cell < other.cell { (mine.cell, other.cell) } else { (other.cell, mine.cell) };
                let pair =
                    mix(self.seed, [low[0] ^ up[0].wrapping_mul(0x51ED), low[1] ^ up[1].wrapping_mul(0x2F0B), 3]);
                if share(pair) >= self.ramps || inputs[2][at] < 0.5 {
                    continue;
                }
                let (ux, uz) = (ax / between, az / between);
                let along = share(mix(pair, [1, 2, 3])) - 0.5;
                let (mx, mz) = (
                    (mine.at[0] + other.at[0]) * 0.5 - uz * along * 0.3 * between,
                    (mine.at[1] + other.at[1]) * 0.5 + ux * along * 0.3 * between,
                );
                let beside = (px - mx) * -uz + (pz - mz) * ux;
                let beside = if beside < 0.0 { -beside } else { beside };
                if beside > wide * 0.5 {
                    continue;
                }
                let stair = top - (away * self.ramp_slope / self.step).ceil() * self.step;
                if stair > high {
                    (high, walled) = (stair, 0.0);
                }
                let reach = away - rise / self.ramp_slope;
                clear = if reach < clear { reach } else { clear };
            }
            // The level's own edge over lower ground: a lip raised along it, or the edge rounded off.
            let mut lip = 0.0;
            if walled == 0.0 && high == level && edge < f32::MAX {
                if raised > 0.0 {
                    let part = edge / (raised * 3.0 + 3.0);
                    lip = if part < 1.0 { raised * 4.0 * part * (1.0 - part) } else { 0.0 };
                } else if rounded > 0.0 {
                    let part = edge / (rounded * 4.0);
                    lip = if part < 1.0 { -rounded * (1.0 - part) * (1.0 - part) } else { 0.0 };
                }
            }
            let mut ground = high + lip;
            let mut floor = if mine.level == 0 && high == level { smooth(clear / 24.0) } else { 0.0 };
            // The river's canyon through whatever stands in its way, down to the valley floor.
            let off = inputs[3][at];
            let half = self.canyon * 0.5;
            if self.canyon > 0.0 {
                let cut = self.heights[0] + if off > half { (off - half) * self.wall } else { 0.0 };
                if cut < ground {
                    ground = cut;
                    (walled, floor) = if off > half { (1.0, 0.0) } else { (0.0, 1.0) };
                }
            }
            // The river's bed along its line, on the lowest level clear of the walls.
            if self.river > 0.0 && floor > 0.0 && off < BED {
                ground -= self.river * (1.0 - off * off / (BED * BED)) * floor;
            }
            *out = match self.port {
                Plateaus::WALLS => walled,
                Plateaus::FLOOR => floor,
                _ => ground,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing};

    fn world(set: Plateaus, port: usize) -> crate::generate::WorldGen {
        testing::world_of(NodeKind::Plateaus(set), port, &[])
    }

    /// The ground of `world` over a square `side` metres along each way from `low`, a metre apart.
    fn square(world: &crate::generate::WorldGen, low: [f32; 2], side: usize) -> Vec<f32> {
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| (low[0] + (at % side) as f32, low[1] + (at / side) as f32)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        heights
    }

    #[test]
    fn plateaus_are_wide_level_steps_at_uneven_heights_with_walls_between() {
        let set = Plateaus { size: 250.0, ramps: 0.0, low: 10.0, high: 250.0, levels: 5, ..Plateaus::default() };
        let land = world(set.clone(), Plateaus::HEIGHT);
        let (low, high) = testing::checked(&land);
        assert!(low >= 10.0 - 1e-3 && high <= 250.0 + 1e-3, "{low} to {high}");
        let side = 1200;
        let heights = square(&land, [-600.0, -600.0], side);
        // Most of the land is level: as high as the place a metre along. The rest is wall or rubble.
        let mut level = 0;
        let mut shares: std::collections::BTreeMap<i32, usize> = std::collections::BTreeMap::new();
        for z in 0..side {
            for x in 0..side - 1 {
                let (here, next) = (heights[z * side + x], heights[z * side + x + 1]);
                if (here - next).abs() < 1e-3 {
                    level += 1;
                    *shares.entry(here.round() as i32).or_default() += 1;
                }
            }
        }
        let share = level as f32 / (side * (side - 1)) as f32;
        assert!(share > 0.8, "{share} of the land is level");
        // Every level is somewhere in 1200 m, each with a fiftieth of the land or more.
        let found: Vec<i32> =
            shares.iter().filter(|(_, count)| **count > side * side / 50).map(|(at, _)| *at).collect();
        assert_eq!(found.len(), 5, "{found:?} of {shares:?}");
        assert_eq!((found[0], found[4]), (10, 250));
        // Uneven: the steps between them are not all as high.
        let steps: Vec<i32> = found.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(steps.iter().any(|step| (step - 60).abs() > 4), "{steps:?}");
        // The Walls mask covers what is not level, and the Floor mask the lowest level.
        let walls = square(&world(set.clone(), Plateaus::WALLS), [-600.0, -600.0], side);
        let floor = square(&world(set, Plateaus::FLOOR), [-600.0, -600.0], side);
        let walled = walls.iter().filter(|&&mask| mask > 0.5).count() as f32 / walls.len() as f32;
        assert!(walled > 0.02 && walled < 0.2, "{walled} of the land is wall");
        for at in 0..heights.len() {
            assert!(floor[at] <= 0.0 || heights[at] < 40.0, "floor at {}", heights[at]);
        }
        assert!(floor.iter().filter(|&&mask| mask >= 1.0).count() > heights.len() / 40);
    }

    #[test]
    fn a_ramp_climbs_from_one_level_to_the_next_in_steps_a_player_walks_up() {
        let set = Plateaus { ramps: 0.6, low: 0.0, high: 120.0, levels: 4, ..Plateaus::default() };
        let land = world(set, Plateaus::HEIGHT);
        testing::checked(&land);
        let side = 1400;
        let heights = square(&land, [-700.0, -700.0], side);
        // Blocks a metre a side: each column's top block. Walked from the lowest ground, stepping up a block
        // at most, a player reaches every level.
        let tops: Vec<i32> = heights.iter().map(|height| (height - 0.5).round() as i32).collect();
        let start = (0..tops.len()).min_by_key(|&at| tops[at]).unwrap();
        let mut reached = vec![false; tops.len()];
        reached[start] = true;
        let mut next = vec![start];
        while let Some(at) = next.pop() {
            let (x, z) = (at % side, at / side);
            let beside = [
                (x > 0).then(|| at - 1),
                (x + 1 < side).then(|| at + 1),
                (z > 0).then(|| at - side),
                (z + 1 < side).then(|| at + side),
            ];
            for to in beside.into_iter().flatten() {
                if !reached[to] && tops[to] - tops[at] <= 1 {
                    reached[to] = true;
                    next.push(to);
                }
            }
        }
        let highest = (0..tops.len()).filter(|&at| reached[at]).map(|at| tops[at]).max().unwrap();
        assert!(highest >= 119, "the highest level is walked to: {highest}");
        // With no ramps the walls keep a player on the lowest level they can fall to.
        let walled = world(Plateaus { ramps: 0.0, low: 0.0, high: 120.0, levels: 4, ..Plateaus::default() }, 0);
        let heights = square(&walled, [-700.0, -700.0], side);
        let sheer = (0..heights.len() - 1).filter(|at| (heights[at + 1] - heights[*at]).abs() > 2.0).count();
        assert!(sheer > 500, "walls stand: {sheer} places step more than two blocks");
    }
}
