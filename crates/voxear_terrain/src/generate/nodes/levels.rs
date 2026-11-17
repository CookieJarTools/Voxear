//! The Levels node: land that climbs in escarpments, long gentle slopes
//! rising one way, now and then ending in a sheer wall, rubble at the
//! wall's foot and now and then a ramp up it. Walk up the slope and the
//! land ends at an edge, with the next slope's foot below; come the other
//! way and the wall stands across the way. The walls wind for kilometres,
//! all facing much the same way. Most of an escarpment fades away into the
//! slope, and no wall stands more than 55 m tall, however far apart the
//! levels are.
//!
//! Under the escarpments the land climbs from Lowest to Highest over a
//! broad pattern many areas wide, most of it lying low, and swells gently
//! over hundreds of metres. A biome's land rolls less over the lowland
//! than over the uplands. Walls are cut back by gullies between buttresses, with more
//! rubble at a gully's foot; here and there a stretch of wall is broken
//! into steep slope, and the ground rounds over a wall's brow.
//!
//! Size is how wide an area is; Levels how many escarpments the land
//! climbs between Lowest and Highest, each as tall as the gap between
//! them. Wall is how steep a wall stands and Ramps how much of the walls'
//! length is a ramp instead. The ground wired into Ground says where land
//! is and where the sea: land rises out of the sea over Coast metres of
//! that ground's height, high land in a cliff no more than 25 m tall, and
//! slopes down to the shore as that ground does, a few times as steep.
//! Oceans is how much of the world lies under open ocean: broad basins
//! tens of kilometres across, where that ground is sunk under the sea over
//! a wide shelf, so the land between stands as continents and great
//! islands, their coasts still that ground's own. None lies within a few
//! kilometres of a world's start.
//!
//! Each area has a character, picked by the weights: Plains are nearly
//! level, Rolling ground rolls as its biome's land does, Steps rise in
//! broad steps, Canyons are cut by ravines, Spires stand rock spires,
//! Peaks raise a range of mountains 550 to 1200 m tall, kept a kilometre
//! or two from a world's start, and Basins sink towards the lowest level.
//! Areas' borders bend hard, so they wind and send out lobes. Nearly half
//! the areas have a lone round hill.
//!
//! Its Walls is 1 on the walls, the rubble and the peaks' rock, for an
//! Output's Rock. Relief is how much of a biome's land should roll over
//! each place, by its area's character; Spires and Bridges are masks of
//! the areas of spires and of canyons, for a Spires and an Arches node.
//! Floor is the height of the escarpments' ground at each place, before
//! anything stands on it, for a climate that cools with the height over
//! that and not with its own, so high land is as green as low.
//!
//! A world of blocks with levels lays this node for itself, its areas the
//! same as its biomes' regions (see `areas`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, noise3d::seed_for};
use crate::{
    generate::{
        areas::{Areas, Character, LevelPlan, MOST_SINK, SINK_EDGE, Sites, border_distance},
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::{self, NoiseStyle},
};

/// Land at a few set heights, an area at a time, walls between.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub size: f32,
    pub levels: u32,
    pub low: f32,
    pub high: f32,
    /// How steep a wall stands, as rise over run.
    pub wall: f32,
    pub ramps: f32,
    pub coast: f32,
    pub plains: f32,
    pub rolling: f32,
    pub steps: f32,
    pub canyons: f32,
    pub spires: f32,
    pub peaks: f32,
    pub basins: f32,
    pub oceans: f32,
    pub seed: u32,
    /// The areas, where a world of blocks lays the node for itself: its
    /// biomes' regions. Never kept in a file.
    #[serde(skip)]
    pub areas: Option<Arc<Areas>>,
    /// Kept in a file in place of its areas: that its areas are the biome
    /// regions of the world of blocks the generator makes ground for, which
    /// the world gives it as it loads. Elsewhere it lays out areas of its
    /// own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub world_areas: bool,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            size: 2400.0,
            levels: 4,
            low: 8.0,
            high: 400.0,
            wall: 12.0,
            ramps: 0.12,
            coast: 6.0,
            plains: 1.0,
            rolling: 1.5,
            steps: 0.0,
            canyons: 0.15,
            spires: 0.1,
            peaks: 0.3,
            basins: 0.15,
            oceans: 0.35,
            seed: 0,
            areas: None,
            world_areas: false,
        }
    }
}

impl Levels {
    /// Its input, among its inputs, and its outputs.
    pub const GROUND: usize = 0;
    pub const HEIGHT: usize = 0;
    pub const WALLS: usize = 1;
    pub const RELIEF: usize = 2;
    pub const SPIRES: usize = 3;
    pub const BRIDGES: usize = 4;
    pub const FLOOR: usize = 5;

    /// The weights of its characters, in the order of [`Character::ALL`].
    pub fn characters(&self) -> [f32; 7] {
        [self.plains, self.rolling, self.steps, self.canyons, self.spires, self.peaks, self.basins]
    }

    /// Its levels and characters as areas lay them out.
    pub fn plan(&self) -> LevelPlan {
        LevelPlan {
            levels: self.levels,
            low: self.low,
            high: self.high,
            characters: self.characters(),
            oceans: self.oceans,
            keep: None,
        }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size", "Size", 200.0, 50_000.0, "m"),
    ValueSpec::whole("levels", "Levels", 1, 12),
    ValueSpec::number("low", "Lowest", -500.0, 5000.0, "m"),
    ValueSpec::number("high", "Highest", -500.0, 8000.0, "m"),
    ValueSpec::angle("wall", "Wall"),
    ValueSpec::number("ramps", "Ramps", 0.0, 1.0, ""),
    ValueSpec::number("coast", "Coast", 0.5, 200.0, "m").advanced(),
    ValueSpec::number("plains", "Plains", 0.0, 10.0, ""),
    ValueSpec::number("rolling", "Rolling", 0.0, 10.0, ""),
    ValueSpec::number("steps", "Steps", 0.0, 10.0, ""),
    ValueSpec::number("canyons", "Canyons", 0.0, 10.0, ""),
    ValueSpec::number("spires", "Spires", 0.0, 10.0, ""),
    ValueSpec::number("peaks", "Peaks", 0.0, 10.0, ""),
    ValueSpec::number("basins", "Basins", 0.0, 10.0, ""),
    ValueSpec::number("oceans", "Oceans", 0.0, 0.9, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
    ValueSpec::choice("areas", "Areas", AREAS).advanced(),
];

/// Whose areas it lays its levels in.
const AREAS: &[&str] = &["Its Own", "The World's Regions"];

const INPUTS: &[Port] = &[Port::field("ground", "Ground")];

const OUTPUTS: &[Port] = &[
    Port::field("height", "Height"),
    Port::field("walls", "Walls"),
    Port::field("relief", "Relief"),
    Port::field("spires", "Spires"),
    Port::field("bridges", "Bridges"),
    Port::field("floor", "Floor"),
];

impl Kind for Levels {
    const TITLE: &'static str = "Levels";
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
            6 => Value::Number(self.coast),
            7..=13 => Value::Number(self.characters()[at - 7]),
            14 => Value::Number(self.oceans),
            15 => Value::Whole(self.seed),
            16 => Value::Choice(usize::from(self.world_areas)),
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
            (6, Value::Number(number)) => self.coast = number,
            (7, Value::Number(number)) => self.plains = number,
            (8, Value::Number(number)) => self.rolling = number,
            (9, Value::Number(number)) => self.steps = number,
            (10, Value::Number(number)) => self.canyons = number,
            (11, Value::Number(number)) => self.spires = number,
            (12, Value::Number(number)) => self.peaks = number,
            (13, Value::Number(number)) => self.basins = number,
            (14, Value::Number(number)) => self.oceans = number,
            (15, Value::Whole(whole)) => self.seed = whole,
            (16, Value::Choice(choice)) => self.world_areas = choice == 1,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let areas = match &self.areas {
            Some(areas) => areas.clone(),
            None => {
                let size = (self.size * cx.world.scale.max(0.01)).clamp(64.0, 100_000.0);
                Arc::new(Areas::new(Sites::new(size, seed_for(cx, node, self.seed)), self.plan()))
            }
        };
        let sea = cx.world.sea_level;
        let coast = if self.coast.is_finite() { self.coast.clamp(0.5, 200.0) } else { 6.0 };
        // With no ground wired in, all of it is land.
        let ground = cx.input(node, Self::GROUND, at).unwrap_or_else(|| cx.constant(sea + 2000.0));
        let lay = Lay {
            areas,
            wall: if self.wall.is_finite() { self.wall.clamp(0.3, 100.0) } else { 12.0 },
            ramps: if self.ramps.is_finite() { self.ramps.clamp(0.0, 1.0) } else { 0.0 },
            coast,
            sea,
            port: port.min(Self::FLOOR),
        };
        cx.run(lay, &[at.x, at.z, ground])
    }
}

/// How much of a wall's drop lies as a slope of rubble at its foot, and
/// how high that slope stands at most, in metres: a wall stands over a long
/// apron, bare scree near it, grown over and wooded further down.
const TALUS: f32 = 0.2;
const MOST_TALUS: f32 = 70.0;

/// How steep rubble lies, and a ramp, as rise over run.
const RUBBLE: f32 = 0.7;
const RAMP: f32 = 0.45;

/// How high a wall rises between ledges, in metres, and how wide a ledge
/// is at most: a strip of level ground grass can take, where harder rock
/// stands out.
const LEDGE_EVERY: f32 = 28.0;
const LEDGE: f32 = 6.0;

/// How far from an area's border what stands within it fades out, in
/// metres: a ravine, a step, a field of spires.
const KEEP_IN: f32 = 180.0;

/// How much of its biome's land rolls over the lowest land, as a share of
/// what its area's character says, and how far up the broad climb, in
/// metres, it rolls in full: wide lowland lies flatter than the uplands.
const LOW_ROLLS: f32 = 0.45;
const ROLLS_FROM: f32 = 160.0;

/// How far over a border one character of area blends into the next, in
/// metres.
const CHARACTER_BLEND: f32 = 200.0;

/// A step's height, in metres, and how many there are.
const STEP: f32 = 26.0;
const STEPS: f32 = 4.0;

/// How far apart two areas' ground must be over their border before it
/// starts to end in a wall, and over how many metres more it becomes a
/// whole wall; and over how many metres either side of a border two areas
/// alike run into each other.
const CLIFF_FROM: f32 = 25.0;

/// How tall a range of mountains is, from and to, in metres, and how high
/// up it the rock is bare.
const RANGE_FROM: f32 = 550.0;
const RANGE_TO: f32 = 1200.0;
const RANGE_BARE: f32 = 260.0;

/// How steep a mountain's flank must be to be bare rock, rise over run.
const RANGE_STEEP: f32 = 0.8;

/// How far past its area's border a range of mountains reaches, in metres.
const RANGE_SPILL: f32 = 900.0;
const CLIFF_OVER: f32 = 45.0;

/// The tallest a cliff stands where high land meets the sea, in metres, and
/// how many metres past that the land may stand over the ground wired in
/// for each metre that ground stands over the sea.
const SEA_CLIFF: f32 = 25.0;
const SLOPE_BACK: f32 = 3.5;

/// How many areas have a lone hill, and how tall one stands, from and to,
/// in metres.
const HILLS: f32 = 0.45;
const HILL_FROM: f32 = 50.0;
const HILL_TO: f32 = 130.0;
const RUN_ON: f32 = 260.0;

/// How steep a gully cut back into a wall is, and a stretch of wall broken
/// into steep slope, as rise over run; and how far apart gullies are, about.
const GULLY: f32 = 1.5;
const BROKEN: f32 = 2.4;
const GULLY_SIZE: f32 = 70.0;

/// How far under the higher ground a wall stops at most, in metres, and
/// over how many metres that ground rounds down to it.
const BROW: f32 = 9.0;
const BROW_WIDTH: f32 = 34.0;

/// What a wall is like at a place: how much in a gully and in a broken
/// stretch, how much of a ramp, 0 to 1 each, and how wide its ledges are.
#[derive(Clone, Copy, Debug)]
struct Face {
    gully: f32,
    broken: f32,
    ramp: f32,
    ledge: f32,
}

/// The levels' ground, walls and masks at each place, from rows of its x,
/// z and the ground wired in.
#[derive(Debug)]
struct Lay {
    areas: Arc<Areas>,
    wall: f32,
    ramps: f32,
    coast: f32,
    sea: f32,
    port: usize,
}

impl Kernel for Lay {
    fn key(&self, key: &mut Key) {
        let id = self.areas.id();
        key.name("levels").whole(id as u32).whole((id >> 32) as u32);
        key.number(self.wall).number(self.ramps).number(self.coast).number(self.sea).whole(self.port as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        // A Levels node's outputs each ask the same places in turn: they are laid once, for all of them.
        let key = (self.areas.id(), [self.wall, self.ramps, self.coast, self.sea].map(f32::to_bits));
        LAID.with_borrow_mut(|laid| {
            let same = laid.key == key && laid.xs == inputs[0] && laid.zs == inputs[1] && laid.grounds == inputs[2];
            if !same {
                laid.key = key;
                (laid.xs.clear(), laid.zs.clear(), laid.grounds.clear(), laid.made.clear());
                laid.xs.extend_from_slice(inputs[0]);
                laid.zs.extend_from_slice(inputs[1]);
                laid.grounds.extend_from_slice(inputs[2]);
                laid.made.extend((0..out.len()).map(|at| self.at(inputs[0][at], inputs[1][at], inputs[2][at])));
            }
            for (out, made) in out.iter_mut().zip(&laid.made) {
                *out = made[self.port];
            }
        });
    }
}

/// The places a Levels node last laid on this thread, by its areas and
/// settings, and all it made of each.
#[derive(Default)]
struct Laid {
    key: (u64, [u32; 4]),
    xs: Vec<f32>,
    zs: Vec<f32>,
    grounds: Vec<f32>,
    made: Vec<[f32; 6]>,
}

thread_local! {
    static LAID: std::cell::RefCell<Laid> = std::cell::RefCell::new(Laid::default());
}

impl Lay {
    /// The height, walls, relief, spires, bridges and floor at `(x, z)`
    /// over ground `ground`.
    fn at(&self, x: f32, z: f32, ground: f32) -> [f32; 6] {
        // The open oceans sink the ground wired in under the sea, which then says where the coasts are.
        let ground = ground - self.areas.ocean(x, z);
        let sites = &self.areas.sites;
        let spacing = sites.spacing();
        let seed = sites.seed();
        let at = self.areas.place(x, z);
        let cell_at = self.areas.region_place(x, z);
        let lie = self.areas.lie(at, cell_at);
        let (own, nine) = sites.nearest(cell_at);
        let own_at = sites.site(own);
        let character = self.areas.info(own).1;

        // Ledges along the walls, wider in some stretches, none in others.
        let ledges = noise::gradient(at[0] / 140.0, at[1] / 140.0, seed ^ 0x1ED6);
        let ledge = LEDGE * ((ledges + 0.2) / 0.8).clamp(0.0, 1.0);
        // The ramps: stretches of wall where a broad pattern is high.
        let pattern = noise::gradient(at[0] / (spacing * 0.22), at[1] / (spacing * 0.22), seed ^ 0x2A3F);
        // Gullies cut back into the walls between buttresses, and stretches of wall broken into steep slopes.
        let gullies = noise::fractal(NoiseStyle::Ridged, at[0] / GULLY_SIZE, at[1] / GULLY_SIZE, 2, seed ^ 0x6011);
        let broken = noise::gradient(at[0] / 380.0, at[1] / 380.0, seed ^ 0x6012);
        let face = Face {
            gully: ((gullies - 0.45) / 0.25).clamp(0.0, 1.0),
            broken: ((0.1 - broken) / 0.4).clamp(0.0, 1.0),
            ramp: ((pattern - (0.62 - 1.25 * self.ramps)) / 0.15).clamp(0.0, 1.0),
            ledge,
        };

        // The escarpment: up its long slope, then over the wall at its edge onto the foot of the next.
        let foot = lie.climb + lie.swell;
        let tread = |slope: f32| foot + lie.rise * (0.65 * slope + 0.35 * smooth(slope));
        let (lower, higher, into) = match lie.slope > 0.5 {
            true => (foot, tread(lie.slope), (1.0 - lie.slope) * lie.length),
            false => (tread(lie.slope), foot + lie.rise, -lie.slope * lie.length),
        };
        let soft = lower + (higher - lower) * smooth(0.5 + into / (2.0 * RUN_ON));
        let cliff = smooth((lie.rise - CLIFF_FROM) / CLIFF_OVER);
        let mut rock: f32 = 0.0;
        let mut height = soft;
        if cliff > 0.0 {
            let (walled, bare) = self.pair(lower, higher, into, &face);
            height += (walled - soft) * cliff;
            rock = bare * cliff;
        }
        // Basins and the area round the start sink in from their edge, the steep of the sides bare.
        if lie.sunk > 0.0 {
            let drop = (height - (self.areas.plan.low + lie.swell)).clamp(0.0, MOST_SINK);
            height -= drop * smooth(lie.sunk);
            if drop * 6.0 * lie.sunk * (1.0 - lie.sunk) / (spacing * SINK_EDGE) > 1.0 {
                rock = 1.0;
            }
        }

        let mut inside = f32::MAX;
        let mut other_character = (f32::MAX, character);
        for &cell in nine.iter().filter(|&&cell| cell != own) {
            let distance = border_distance(cell_at, own_at, sites.site(cell));
            inside = inside.min(distance);
            let its_character = self.areas.info(cell).1;
            if its_character != character && distance < other_character.0 {
                other_character = (distance, its_character);
            }
        }
        let laid = height;

        // What stands within an area, faded out towards its borders.
        let kept = ((inside - 40.0) / KEEP_IN).clamp(0.0, 1.0);
        match character {
            Character::Steps => {
                let broad = noise::gradient(at[0] / 520.0, at[1] / 520.0, seed ^ 0x57E9) * 0.6 + 0.5;
                let k = broad.clamp(0.0, 0.999) * STEPS;
                let (whole, part) = (k.floor(), k - k.floor());
                let rise = smooth((part - 0.86) / 0.14);
                height += STEP * (whole + rise) * kept;
                if rise > 0.0 && rise < 1.0 {
                    rock = rock.max(kept);
                }
            }
            Character::Canyons => {
                // Long winding ravines along where a broad pattern crosses its middle, wider in some stretches.
                let size = spacing * 0.3;
                let wind =
                    |part: u32| size * 0.25 * noise::gradient(at[0] / (size * 0.4), at[1] / (size * 0.4), seed ^ part);
                let (x, z) = (at[0] + wind(0xCA42), at[1] + wind(0xCA43));
                let line = noise::gradient(x / size, z / size, seed ^ 0xCA41).abs();
                let wide = 0.03 + 0.02 * noise::gradient(at[0] / 300.0, at[1] / 300.0, seed ^ 0xCA44);
                let cut = ((wide - line) / 0.012).clamp(0.0, 1.0);
                let deep = 35.0 + 35.0 * sites.pick(own, 11);
                height -= deep * cut * kept;
                if cut > 0.0 && cut < 1.0 {
                    rock = rock.max(kept);
                }
            }
            _ => {}
        }
        let hill = self.hills(at, &nine);
        if hill > 0.0 {
            height += hill;
        }
        // A range of mountains in an area of peaks, spreading a little past its border.
        let (range, of, into) = nine
            .iter()
            .filter(|&&cell| self.areas.info(cell).1 == Character::Peaks)
            .map(|&cell| {
                let into = match cell == own {
                    true => inside,
                    false => border_distance(cell_at, sites.site(cell), own_at),
                };
                (self.range(at, cell, into) * lie.open, cell, into)
            })
            .fold((0.0, own, 0.0), |best, next| if next.0 > best.0 { next } else { best });
        if range > 0.0 {
            height += range;
            // Bare rock where its flanks are steep, grass and snow where they lie back.
            let step = 4.0;
            let steep =
                |dx: f32, dz: f32| (self.range([at[0] + dx, at[1] + dz], of, into) * lie.open - range).abs() / step;
            if range > RANGE_BARE * 0.25 && steep(step, 0.0).max(steep(0.0, step)) > RANGE_STEEP {
                rock = 1.0;
            }
        }

        // Out of the sea: the ground wired in says where the land is. High land meets it in a cliff of no more than
        // 25 m, and stands over it no more than a few times as high as that ground stands over the sea, so mountains
        // and high levels slope down to the shore as that ground does, only steeper.
        let land = smooth((ground - self.sea) / self.coast);
        let over = height - ground;
        let stands = over.min(SEA_CLIFF + SLOPE_BACK * (ground - self.sea).max(0.0));
        if land < 1.0 && stands > 20.0 {
            rock = rock.max(1.0 - land);
        }
        let height = ground + stands * land;

        // How much of a biome's land rolls here, blended into the next area's character over its border.
        let relief = match other_character.0 < CHARACTER_BLEND {
            true => {
                let own_share = 0.5 + 0.5 * smooth(other_character.0 / CHARACTER_BLEND);
                character.relief() * own_share + other_character.1.relief() * (1.0 - own_share)
            }
            false => character.relief(),
        };
        // Lowland lies flatter than the uplands: a biome's land rolls less where the broad climb is low.
        let relief = relief * (LOW_ROLLS + (1.0 - LOW_ROLLS) * smooth((lie.climb - self.areas.plan.low) / ROLLS_FROM));
        let mask = |of: Character| if character == of { kept } else { 0.0 };
        let floor_here = ground + (laid - ground) * land;
        [height, rock, relief * land, mask(Character::Spires) * land, mask(Character::Canyons) * land, floor_here]
    }

    /// The ground two areas at `lower` and `higher` make at a place `into`
    /// metres into the higher, out of it where negative, with the wall's
    /// `face` there: a wall standing over rubble, stepped back at its
    /// ledges, cut back in gullies, or a ramp; and how bare.
    fn pair(&self, lower: f32, higher: f32, into: f32, face: &Face) -> (f32, f32) {
        let Face { gully, broken, ramp, ledge } = *face;
        let drop = higher - lower;
        let talus = (drop * TALUS * (1.0 + 1.5 * gully)).min(MOST_TALUS * (1.0 + gully));
        if into >= 0.0 {
            let wall = self.wall + (self.wall.min(BROKEN) - self.wall) * broken;
            let wall = wall + (wall.min(GULLY) - wall) * gully;
            let slope = (wall + (RAMP - wall) * ramp).max(0.05);
            let face = drop - talus;
            // Each band of the wall rises sheer, then a ledge runs back before the next.
            let ledge = ledge * (1.0 - ramp);
            let band = (LEDGE_EVERY / slope + ledge).max(0.5);
            // The wall stops short of the top, and the higher ground rounds over its brow down to it.
            let brow = (face * 0.1).min(BROW) * (1.0 - ramp);
            let under = face - brow;
            let bands = (into / band).floor();
            let up = into - bands * band;
            let rise = bands * LEDGE_EVERY + (up * slope).min(LEDGE_EVERY);
            if rise < under {
                let on_face = up * slope < LEDGE_EVERY;
                let bare = if on_face { (1.0 - ramp) * (1.0 - 0.6 * gully) } else { 0.0 };
                return (lower + talus + rise, bare);
            }
            let whole = (under / LEDGE_EVERY).floor();
            let top_at = whole * band + (under - whole * LEDGE_EVERY) / slope;
            let over = (1.0 - (into - top_at) / BROW_WIDTH).clamp(0.0, 1.0);
            (lower + talus + under + brow * (1.0 - over * over), 0.0)
        } else {
            let reach = (talus / RUBBLE).max(0.5);
            let left = (1.0 + into / reach).max(0.0);
            // The rubble is bare where it lies steep against the wall, grown over at its foot.
            let bare = if left > 0.78 { 1.0 - ramp } else { 0.0 };
            (lower + talus * left * left, bare)
        }
    }

    /// How high the range of mountains of site `own`'s area of peaks
    /// stands at `at`, `inside` metres in from its border, out of it where
    /// negative: rising from a little way past its border to a height of
    /// its own, 550 to 1200 m, in ridges with valleys between.
    fn range(&self, at: [f32; 2], own: [i32; 2], inside: f32) -> f32 {
        let sites = &self.areas.sites;
        let spacing = sites.spacing();
        let seed = sites.seed();
        let tall = RANGE_FROM + (RANGE_TO - RANGE_FROM) * sites.pick(own, 60);
        let rise = smooth((inside + RANGE_SPILL) / (spacing * 0.9));
        if rise <= 0.0 {
            return 0.0;
        }
        // Its ridges, bent so they wander, each sharpened by the one before so spurs run off them and crests stand
        // sharp; crags along the crests; and valleys between, grown over.
        let size = spacing * 0.42;
        let bend = size * 0.35;
        let (wx, wz) = (at[0] / size, at[1] / size);
        let (x, z) = (
            at[0] + bend * noise::gradient(wx, wz, seed ^ 0x9A01),
            at[1] + bend * noise::gradient(wx, wz, seed ^ 0x9A02),
        );
        let ridges = noise::fractal(NoiseStyle::Ridged, x / size, z / size, 3, seed ^ 0x9A03);
        let crags = noise::fractal(NoiseStyle::Ridged, x / (size * 0.11), z / (size * 0.11), 2, seed ^ 0x9A04);
        let ridge = ridges * (0.5 + 0.9 * ridges);
        let massif = rise;
        tall * massif * (0.16 + 0.84 * ridge + 0.03 * crags * ridge)
    }

    /// How high the lone hills stand at `at`: now and then one in an area,
    /// round and grown over, 50 to 130 m tall and three or four times as wide,
    /// its outline bent and drawn out one way, so none is round alike. Every
    /// area's hill is asked for, so a hill near a border runs on over it.
    fn hills(&self, at: [f32; 2], nine: &[[i32; 2]; 9]) -> f32 {
        let sites = &self.areas.sites;
        let spacing = sites.spacing();
        let seed = sites.seed();
        let mut tallest: f32 = 0.0;
        for &cell in nine {
            if sites.pick(cell, 17) >= HILLS {
                continue;
            }
            let site = sites.site(cell);
            let middle = [
                site[0] + spacing * 0.3 * (sites.pick(cell, 18) - 0.5),
                site[1] + spacing * 0.3 * (sites.pick(cell, 19) - 0.5),
            ];
            // Never so tall that it would reach out of a small area.
            let height = (HILL_FROM + (HILL_TO - HILL_FROM) * sites.pick(cell, 20)).min(spacing * 0.05);
            let reach = height * (3.0 + 1.5 * sites.pick(cell, 21));
            let bend = reach * 0.3;
            let (wx, wz) = (at[0] / (reach * 0.8), at[1] / (reach * 0.8));
            let (mut dx, mut dz) = (
                at[0] + bend * noise::gradient(wx, wz, seed ^ 0x9E01) - middle[0],
                at[1] + bend * noise::gradient(wx, wz, seed ^ 0x9E02) - middle[1],
            );
            // Drawn out along one way, a third shorter across it.
            let (lx, lz) = (sites.pick(cell, 80) - 0.5, sites.pick(cell, 90) - 0.5);
            let length = (lx * lx + lz * lz).sqrt().max(0.01);
            let along = (dx * lx + dz * lz) / length;
            dx -= 0.35 * along * lx / length;
            dz -= 0.35 * along * lz / length;
            let away = (dx * dx + dz * dz) / (reach * reach);
            if away >= 1.0 {
                continue;
            }
            // Round over its top, easing out at its foot; its flanks a little uneven.
            let dome = (1.0 - away) * (1.0 - away);
            let uneven = 1.0 + 0.12 * noise::gradient(at[0] / (reach * 0.35), at[1] / (reach * 0.35), seed ^ 0x9EA6);
            tallest = tallest.max(height * dome * uneven);
        }
        tallest
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing};

    #[test]
    fn levels_climb_in_long_gentle_slopes_ending_in_sheer_walls() {
        let levels = Levels { size: 1200.0, peaks: 0.0, steps: 0.0, canyons: 0.0, basins: 0.0, ..Levels::default() };
        let world = testing::world_of(NodeKind::Levels(levels), 0, &[]);
        // Four rows 12 km long at 2 m.
        let rows: Vec<Vec<f32>> = [300.0, 4300.0, 8300.0, 12300.0]
            .iter()
            .map(|&z| {
                let xs: Vec<f32> = (0..6000).map(|k| k as f32 * 2.0).collect();
                let zs = vec![z; xs.len()];
                let mut heights = vec![0.0; xs.len()];
                world.heights(&xs, &zs, &mut heights);
                heights
            })
            .collect();
        // Between the lowest level and the highest, with an escarpment's rise of up to 55 m and a lone hill over it.
        let all: Vec<f32> = rows.iter().flatten().copied().collect();
        let (least, most) = all.iter().fold((f32::MAX, f32::MIN), |(l, m), &h| (l.min(h), m.max(h)));
        assert!(least > 7.0 && most < 400.0 + 200.0, "{least} to {most}");
        assert!(most - least > 150.0, "the land climbs: {least} to {most}");
        // Mostly long, gentle slopes: most steps rise less than one in three.
        let steps: Vec<f32> = rows.iter().flat_map(|row| row.windows(2).map(|p| (p[1] - p[0]).abs())).collect();
        let gentle = steps.iter().filter(|&&d| d < 0.66).count();
        assert!(gentle * 100 > steps.len() * 85, "{gentle} of {} gentle", steps.len());
        // Somewhere one ends in a sheer wall: the land drops 35 m within 20 m.
        let sheer = rows.iter().any(|row| row.windows(10).any(|w| (w[0] - w[9]).abs() > 35.0));
        assert!(sheer, "a sheer wall");
    }

    #[test]
    fn walls_are_bare_and_an_area_of_canyons_is_cut_by_ravines_with_bridges_over_them() {
        let canyons = Levels {
            size: 1500.0,
            plains: 0.0,
            rolling: 0.0,
            steps: 0.0,
            spires: 0.0,
            peaks: 0.0,
            basins: 0.0,
            ..Levels::default()
        };
        let world = testing::world_of(NodeKind::Levels(canyons.clone()), 0, &[]);
        let (xs, zs) = testing::flat_places(6000);
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let walls = testing::world_of(NodeKind::Levels(canyons.clone()), Levels::WALLS, &[]);
        let bridges = testing::world_of(NodeKind::Levels(canyons), Levels::BRIDGES, &[]);
        let mut bare = vec![0.0; xs.len()];
        walls.heights(&xs, &zs, &mut bare);
        let mut over = vec![0.0; xs.len()];
        bridges.heights(&xs, &zs, &mut over);
        assert!(bare.iter().all(|&b| (0.0..=1.0).contains(&b)));
        assert!(bare.iter().any(|&b| b > 0.5), "some bare rock");
        assert!(over.iter().any(|&b| b > 0.99), "bridges in the middle of canyon areas");
        // Ravines cut well under every floor somewhere.
        let floors = [8.0, 138.67, 269.33, 400.0];
        let cut = heights.iter().filter(|&&h| floors.iter().all(|&f: &f32| h < f - 30.0 || h > f + 60.0)).count();
        assert!(cut > 0, "ravines under the floors");
    }
}

