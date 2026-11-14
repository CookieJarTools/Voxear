//! The Levels node: land at a few set heights, each area of it, a few
//! kilometres across, level ground at one of them, and a sheer wall where
//! two areas of unlike height meet, rubble at its foot and now and then a
//! ramp up it. From the edge of a high area one looks down a wall onto the
//! next area's land, far below.
//!
//! Size is how wide an area is; Levels how many heights there are, from
//! Lowest to Highest. Wall is how steep a wall stands and Ramps how much of
//! the walls' length is a ramp instead. The ground wired into Ground says
//! where land is and where the sea: land rises out of the sea to its level
//! over Coast metres of that ground's height, so a high area meets the sea
//! in a cliff.
//!
//! Each area has a character, picked by the weights: Plains are nearly
//! level, Rolling ground rolls as its biome's land does, Steps rise in
//! broad steps, Canyons are cut by ravines, Spires stand rock spires,
//! Peaks raise a cluster of lone peaks and Basins sink to the lowest
//! level. Now and then a peak stands at the end of a high area, over the
//! wall.
//!
//! Its Walls is 1 on the walls, the rubble and the peaks' rock, for an
//! Output's Rock. Relief is how much of a biome's land should roll over
//! each place, by its area's character; Spires and Bridges are masks of
//! the areas of spires and of canyons, for a Spires and an Arches node.
//! Floor is the height of each place's area's floor, for a climate that
//! cools with the height over the floor and not with the floor's own, so
//! a high level is as green as a low one.
//!
//! A world of blocks with levels lays this node for itself, its areas the
//! same as its biomes' regions (see `areas`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, noise3d::seed_for};
use crate::{
    generate::{
        areas::{Areas, Character, LevelPlan, Sites, border_distance},
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
            steps: 0.6,
            canyons: 0.6,
            spires: 0.4,
            peaks: 0.5,
            basins: 0.25,
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
        LevelPlan { levels: self.levels, low: self.low, high: self.high, characters: self.characters(), keep: None }
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
            14 => Value::Whole(self.seed),
            15 => Value::Choice(usize::from(self.world_areas)),
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
            (14, Value::Whole(whole)) => self.seed = whole,
            (15, Value::Choice(choice)) => self.world_areas = choice == 1,
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
        let ground = cx.input(node, Self::GROUND, at).unwrap_or_else(|| cx.constant(sea + coast * 2.0));
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

/// How much of a wall's drop lies as rubble at its foot, at most, in metres.
const TALUS: f32 = 0.06;
const MOST_TALUS: f32 = 24.0;

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

/// How far over a border one character of area blends into the next, in
/// metres.
const CHARACTER_BLEND: f32 = 200.0;

/// A step's height, in metres, and how many there are.
const STEP: f32 = 26.0;
const STEPS: f32 = 4.0;

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
        for (at, out) in out.iter_mut().enumerate() {
            *out = self.at(inputs[0][at], inputs[1][at], inputs[2][at])[self.port];
        }
    }
}

impl Lay {
    /// The height, walls, relief, spires, bridges and floor at `(x, z)`
    /// over ground `ground`.
    fn at(&self, x: f32, z: f32, ground: f32) -> [f32; 6] {
        let sites = &self.areas.sites;
        let spacing = sites.spacing();
        let seed = sites.seed();
        let at = self.areas.place(x, z);
        let (own, nine) = sites.nearest(at);
        let own_at = sites.site(own);
        let (floor, character) = self.areas.info(own);

        // The ramps: stretches of wall where a broad pattern is high, the same on both sides of a border.
        let pattern = noise::gradient(at[0] / (spacing * 0.22), at[1] / (spacing * 0.22), seed ^ 0x2A3F);
        let ramp = ((pattern - (0.62 - 1.25 * self.ramps)) / 0.15).clamp(0.0, 1.0);
        // Ledges along the walls, wider in some stretches, none in others.
        let ledges = noise::gradient(at[0] / 140.0, at[1] / 140.0, seed ^ 0x1ED6);
        let ledge = LEDGE * ((ledges + 0.2) / 0.8).clamp(0.0, 1.0);

        let mut low_side = floor;
        let mut high_side = f32::MIN;
        let mut rock: f32 = 0.0;
        let mut inside = f32::MAX;
        let mut other_character = (f32::MAX, character);
        for &cell in nine.iter().filter(|&&cell| cell != own) {
            let distance = border_distance(at, own_at, sites.site(cell));
            if distance > spacing {
                continue;
            }
            inside = inside.min(distance);
            let (beside, its_character) = self.areas.info(cell);
            if its_character != character && distance < other_character.0 {
                other_character = (distance, its_character);
            }
            if (beside - floor).abs() < 0.5 {
                continue;
            }
            // How far into the higher of the two areas the place is, and the ground the pair of them makes there.
            let (lower, higher, into_higher) = match beside < floor {
                true => (beside, floor, distance),
                false => (floor, beside, -distance),
            };
            let (height, bare) = self.pair(lower, higher, into_higher, ramp, ledge);
            rock = rock.max(bare);
            if beside < floor {
                low_side = low_side.min(height);
            } else {
                high_side = high_side.max(height);
            }
        }
        let mut height = low_side.max(high_side);

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
                let lines = noise::fractal(
                    NoiseStyle::Ridged,
                    at[0] / (spacing * 0.3),
                    at[1] / (spacing * 0.3),
                    2,
                    seed ^ 0xCA41,
                );
                let cut = ((lines - 0.8) / 0.035).clamp(0.0, 1.0);
                let deep = 70.0 + 70.0 * sites.pick(own, 11);
                height -= deep * cut * kept;
                if cut > 0.0 && cut < 1.0 {
                    rock = rock.max(kept);
                }
            }
            _ => {}
        }
        let peak = self.peaks(at, own, own_at, &nine, floor, character);
        if peak > 0.0 {
            height += peak;
            rock = rock.max(((peak - 15.0) / 30.0).clamp(0.0, 1.0));
        }

        // Out of the sea: the ground wired in says where the land is.
        let land = smooth((ground - self.sea) / self.coast);
        if land < 1.0 && height - ground > 20.0 {
            rock = rock.max(1.0 - land);
        }
        let height = ground + (height - ground) * land;

        // How much of a biome's land rolls here, blended into the next area's character over its border.
        let relief = match other_character.0 < CHARACTER_BLEND {
            true => {
                let own_share = 0.5 + 0.5 * smooth(other_character.0 / CHARACTER_BLEND);
                character.relief() * own_share + other_character.1.relief() * (1.0 - own_share)
            }
            false => character.relief(),
        };
        let mask = |of: Character| if character == of { kept } else { 0.0 };
        let floor_here = ground + (floor - ground) * land;
        [height, rock, relief * land, mask(Character::Spires) * land, mask(Character::Canyons) * land, floor_here]
    }

    /// The ground two areas at `lower` and `higher` make at a place `into`
    /// metres into the higher, out of it where negative, with `ramp` of a
    /// ramp there and ledges `ledge` metres wide: a wall standing over
    /// rubble, stepped back at its ledges, or a ramp; and how bare.
    fn pair(&self, lower: f32, higher: f32, into: f32, ramp: f32, ledge: f32) -> (f32, f32) {
        let drop = higher - lower;
        let talus = (drop * TALUS).min(MOST_TALUS);
        if into >= 0.0 {
            let slope = (self.wall + (RAMP - self.wall) * ramp).max(0.05);
            let face = drop - talus;
            // Each band of the wall rises sheer, then a ledge runs back before the next.
            let ledge = ledge * (1.0 - ramp);
            let band = (LEDGE_EVERY / slope + ledge).max(0.5);
            let bands = (into / band).floor();
            let up = into - bands * band;
            let rise = (bands * LEDGE_EVERY + (up * slope).min(LEDGE_EVERY)).min(face);
            let on_face = rise < face && up * slope < LEDGE_EVERY;
            let bare = if on_face { 1.0 - ramp } else { 0.0 };
            (lower + talus + rise, bare)
        } else {
            let reach = (talus / RUBBLE).max(0.5);
            let left = (1.0 + into / reach).max(0.0);
            // The rubble is bare where it lies steep against the wall, grown over at its foot.
            let bare = if left > 0.5 { 1.0 - ramp } else { 0.0 };
            (lower + talus * left * left, bare)
        }
    }

    /// How high the peaks of an area stand over its floor at `at`: a
    /// cluster where it is an area of peaks, and now and then one at the
    /// end of a high area, over the wall down to a lower.
    fn peaks(
        &self,
        at: [f32; 2],
        own: [i32; 2],
        own_at: [f32; 2],
        nine: &[[i32; 2]; 9],
        floor: f32,
        character: Character,
    ) -> f32 {
        let sites = &self.areas.sites;
        let spacing = sites.spacing();
        let mut tallest: f32 = 0.0;
        let seed = sites.seed();
        let mut stand = |middle: [f32; 2], height: f32| {
            let radius = height * 1.1;
            // Its outline bent by a warp a third of its width, so no two are round alike.
            let bend = radius * 0.32;
            let (wx, wz) = (at[0] / (radius * 0.7), at[1] / (radius * 0.7));
            let (dx, dz) = (
                at[0] + bend * noise::gradient(wx, wz, seed ^ 0x9E01) - middle[0],
                at[1] + bend * noise::gradient(wx, wz, seed ^ 0x9E02) - middle[1],
            );
            let away = dx * dx + dz * dz;
            if away >= radius * radius {
                return;
            }
            let t = away.sqrt() / radius;
            // Ridges running down its flanks and a sharp, broken top, in bands of cliff.
            let ridges =
                noise::fractal(NoiseStyle::Ridged, at[0] / (radius * 0.35), at[1] / (radius * 0.35), 3, seed ^ 0x9EA6);
            let rise = height * (1.0 - t) * (1.0 - t) * (1.0 + 0.6 * t) * (0.7 + 0.45 * ridges);
            let band = 14.0;
            let k = rise / band;
            let rise = band * (k.floor() + smooth((k - k.floor() - 0.55) / 0.45));
            tallest = tallest.max(rise);
        };
        if character == Character::Peaks {
            let count = 3 + (sites.pick(own, 13) * 4.0) as u32;
            for k in 0..count {
                let offset = [sites.pick(own, 20 + k) - 0.5, sites.pick(own, 40 + k) - 0.5];
                let middle = [own_at[0] + offset[0] * spacing * 0.5, own_at[1] + offset[1] * spacing * 0.5];
                stand(middle, 160.0 + 150.0 * sites.pick(own, 60 + k));
            }
        }
        // A peak at the end of a high area, standing back from its wall down to a lower one.
        if sites.pick(own, 17) < 0.35 {
            let start = (sites.pick(own, 18) * 9.0) as usize;
            let lower = (0..9)
                .map(|k| nine[(start + k) % 9])
                .find(|&cell| cell != own && self.areas.info(cell).0 < floor - 1.0);
            if let Some(lower) = lower {
                let there = sites.site(lower);
                let middle = [own_at[0] + (there[0] - own_at[0]) * 0.36, own_at[1] + (there[1] - own_at[1]) * 0.36];
                stand(middle, 180.0 + 140.0 * sites.pick(own, 19));
            }
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
    fn levels_lay_level_land_at_set_heights_with_sheer_walls_between() {
        let levels = Levels { size: 1200.0, peaks: 0.0, steps: 0.0, canyons: 0.0, ..Levels::default() };
        let world = testing::world_of(NodeKind::Levels(levels), 0, &[]);
        // A row 12 km long at 2 m: most of it at one of the four levels, or close over one.
        let xs: Vec<f32> = (0..6000).map(|k| k as f32 * 2.0).collect();
        let zs = vec![300.0; xs.len()];
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let floors = [8.0, 138.67, 269.33, 400.0];
        let near_floor =
            heights.iter().filter(|&&h| floors.iter().any(|&f: &f32| h >= f - 1.0 && h < f + 45.0)).count();
        assert!(near_floor * 10 > heights.len() * 9, "{near_floor} of {} on a level", heights.len());
        let seen: Vec<bool> = floors.iter().map(|&f| heights.iter().any(|&h| (h - f).abs() < 1.0)).collect();
        assert!(seen.iter().filter(|&&s| s).count() >= 3, "{seen:?}");
        // Walls: somewhere the land drops 100 m within 20 m, and the drops are walls, not long slopes.
        let sheer = heights.windows(10).any(|w| (w[0] - w[9]).abs() > 100.0);
        assert!(sheer, "a sheer wall");
        let long_slopes = heights.windows(50).filter(|w| {
            let d = (w[0] - w[49]).abs();
            d > 60.0 && w.windows(2).all(|p| (p[0] - p[1]).abs() < 2.5)
        });
        assert!(long_slopes.count() < 400, "few long even slopes: ramps only");
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
