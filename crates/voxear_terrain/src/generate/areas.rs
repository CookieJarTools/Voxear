//! Areas: the world shared out among sites on a grid, every place belonging
//! to the site nearest it after a gentle warp, so borders wander and no
//! area is ever broken in two. A world's biome regions are areas (see
//! `regions`), and so are its levels (see `nodes::levels`): with the same
//! spacing and seed they are the same areas, so a biome keeps to a level.
//!
//! A world of levels gives each area a level, one of a few heights from
//! its lowest to its highest, and a character: open plain, rolling ground,
//! steps, canyons, a field of spires, a cluster of peaks or a sunken basin.
//! Areas near one another tend to stand at like heights, a broad pattern
//! deciding, with an area here and there a level above or below it.
//!
//! Arithmetic alone, so every machine lays the same areas.

use crate::noise;

/// How far a site may be moved from its place in the grid, as a share of
/// the spacing: under a third, so the nine sites round a place's own cell
/// always hold the nearest.
const JITTER: f32 = 0.3;

/// The warp that makes borders wander: how far it moves a place, as a share
/// of the spacing, and how far apart its rises and falls are, as a number of
/// spacings. Gentle enough over its length that it never folds.
const WARP: f32 = 0.22;
const WARP_SIZE: f32 = 2.5;

/// A second, nearer warp that bends each border along its length, so none
/// runs straight from corner to corner: a share of the spacing, over a
/// spacing or so.
const BEND: f32 = 0.13;
const BEND_SIZE: f32 = 1.3;

/// A finer warp, so a border is ragged up close: how far, in metres, and
/// over how many metres. Under a third of a metre for every metre along, so
/// it never folds either.
const FINE_WARP: f32 = 6.0;
const FINE_SIZE: f32 = 64.0;

/// Sites on a grid `spacing` metres apart, each moved by the seed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sites {
    spacing: f32,
    seed: u32,
}

impl Sites {
    pub fn new(spacing: f32, seed: u32) -> Self {
        Self { spacing, seed }
    }

    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// The place at `(x, z)` moved by the warps.
    pub fn warped(&self, x: f32, z: f32) -> [f32; 2] {
        let size = self.spacing * WARP_SIZE;
        let (u, v) = (x / size, z / size);
        let wide = self.spacing * WARP;
        let (fu, fv) = (x / FINE_SIZE, z / FINE_SIZE);
        let bend_size = self.spacing * BEND_SIZE;
        let (bu, bv) = (x / bend_size, z / bend_size);
        let bend = self.spacing * BEND;
        let dx = wide * noise::gradient(u, v, self.seed ^ 0x11)
            + bend * noise::gradient(bu, bv, self.seed ^ 0x55)
            + FINE_WARP * noise::gradient(fu, fv, self.seed ^ 0x33);
        let dz = wide * noise::gradient(u, v, self.seed ^ 0x22)
            + bend * noise::gradient(bu, bv, self.seed ^ 0x66)
            + FINE_WARP * noise::gradient(fu, fv, self.seed ^ 0x44);
        [x + dx, z + dz]
    }

    /// The site nearest `at`, a warped place, and the nine round it.
    pub fn nearest(&self, at: [f32; 2]) -> ([i32; 2], [[i32; 2]; 9]) {
        let cell = [(at[0] / self.spacing).floor() as i32, (at[1] / self.spacing).floor() as i32];
        let mut sites = [[0; 2]; 9];
        let mut best = (cell, f32::MAX);
        for (k, site) in sites.iter_mut().enumerate() {
            *site = [cell[0] + (k % 3) as i32 - 1, cell[1] + (k / 3) as i32 - 1];
            let place = self.site(*site);
            let (dx, dz) = (place[0] - at[0], place[1] - at[1]);
            let away = dx * dx + dz * dz;
            if away < best.1 {
                best = (*site, away);
            }
        }
        (best.0, sites)
    }

    /// Where site `cell` lies, in metres.
    pub fn site(&self, cell: [i32; 2]) -> [f32; 2] {
        let pick = |part: u32| {
            let hash = mix(self.seed, cell, part);
            (hash >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        [
            (cell[0] as f32 + 0.5 + 2.0 * JITTER * pick(0)) * self.spacing,
            (cell[1] as f32 + 0.5 + 2.0 * JITTER * pick(1)) * self.spacing,
        ]
    }

    /// A number from 0 to 1 picked for site `cell` by the seed and `part`.
    pub fn pick(&self, cell: [i32; 2], part: u32) -> f32 {
        (mix(self.seed, cell, part) >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// How far `at` is from the border between the area of the site at `own`,
/// which it is in, and that of the site at `other`, in metres: the border
/// halfway between them, square to the line joining them.
pub fn border_distance(at: [f32; 2], own: [f32; 2], other: [f32; 2]) -> f32 {
    let (bx, bz) = (other[0] - own[0], other[1] - own[1]);
    let apart = (bx * bx + bz * bz).sqrt().max(1e-3);
    let away = |site: [f32; 2]| (at[0] - site[0]) * (at[0] - site[0]) + (at[1] - site[1]) * (at[1] - site[1]);
    (away(other) - away(own)) / (2.0 * apart)
}

/// A hash of `seed`, a cell and a part, the same on every machine.
pub fn mix(seed: u32, [x, z]: [i32; 2], part: u32) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    for v in [x as u32, z as u32, part] {
        h = (h ^ v).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^= h >> 16;
    }
    h
}

/// What an area of a world of levels is like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Character {
    /// Open, nearly level ground.
    Plain,
    /// Rolling ground, as its biome's land rolls.
    Rolling,
    /// Broad steps a few tens of metres high.
    Steps,
    /// Cut by ravines, natural bridges over them.
    Canyons,
    /// A field of spires of rock.
    Spires,
    /// A cluster of lone peaks.
    Peaks,
    /// Sunk to the lowest level, walled all round.
    Basin,
}

impl Character {
    pub const ALL: [Character; 7] = [
        Character::Plain,
        Character::Rolling,
        Character::Steps,
        Character::Canyons,
        Character::Spires,
        Character::Peaks,
        Character::Basin,
    ];

    /// How much of its biome's own land rolls over an area of this
    /// character, from 0 to 1.
    pub fn relief(self) -> f32 {
        match self {
            Character::Plain => 0.15,
            Character::Rolling => 1.0,
            Character::Steps => 0.3,
            Character::Canyons => 0.35,
            Character::Spires => 0.5,
            Character::Peaks => 0.6,
            Character::Basin => 0.4,
        }
    }
}

/// How a world's land lies in levels: how many, from what height to what,
/// and how often each character of area comes, each a weight.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelPlan {
    pub levels: u32,
    pub low: f32,
    pub high: f32,
    /// Weights of the characters, in the order of [`Character::ALL`].
    pub characters: [f32; 7],
    /// A place whose area is open plain at the lowest level, where the
    /// world starts, so the ground kept round it meets level land.
    pub keep: Option<[f32; 2]>,
}

/// How far a world of levels' ragged warp moves a place, in metres, over
/// how many metres, and again close up: under a third of a metre for every
/// metre along all told, so it never folds.
const RAGGED: f32 = 9.0;
const RAGGED_SIZE: f32 = 56.0;
const ROUGH: f32 = 3.5;
const ROUGH_SIZE: f32 = 14.0;

/// How far apart the broad pattern that sets areas' levels rises and
/// falls, in spacings: several areas side by side stand at like heights.
const LEVEL_SIZE: f32 = 3.2;

/// How many areas in a hundred stand a level above or below what the broad
/// pattern says.
const ODD_LEVEL: f32 = 0.3;

/// The areas of a world of levels.
#[derive(Clone, Debug, PartialEq)]
pub struct Areas {
    pub sites: Sites,
    pub plan: LevelPlan,
    /// The site of the area kept open and low, where there is one.
    kept: Option<[i32; 2]>,
    /// What tells these areas from any others.
    id: u64,
}

/// How many areas' floors and characters a thread keeps before it lets
/// them all go.
const KEPT: usize = 4096;

thread_local! {
    /// Areas' floors and characters worked out on this thread, by areas and site.
    static KNOWN: std::cell::RefCell<std::collections::HashMap<(u64, [i32; 2]), (f32, Character)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

impl Areas {
    pub fn new(sites: Sites, plan: LevelPlan) -> Self {
        let levels = plan.levels.clamp(1, 12);
        let low = if plan.low.is_finite() { plan.low.clamp(-500.0, 5000.0) } else { 0.0 };
        let high = if plan.high.is_finite() { plan.high.clamp(low, 8000.0) } else { low };
        let mut characters = plan.characters.map(|weight| if weight.is_finite() { weight.max(0.0) } else { 0.0 });
        if characters.iter().all(|&weight| weight <= 0.0) {
            characters[Character::Rolling as usize] = 1.0;
        }
        let keep = plan.keep.filter(|keep| keep.iter().all(|v| v.is_finite()));
        let kept = keep.map(|[x, z]| sites.nearest(sites.warped(x, z)).0);
        let plan = LevelPlan { levels, low, high, characters, keep };
        let id = crate::layers::stable_hash(format!("{sites:?} {plan:?}").as_bytes());
        Self { sites, plan, kept, id }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// The floor and character of site `cell`'s area, worked out once on
    /// each thread.
    pub fn info(&self, cell: [i32; 2]) -> (f32, Character) {
        if let Some(known) = KNOWN.with_borrow(|known| known.get(&(self.id, cell)).copied()) {
            return known;
        }
        let info = (self.floor(cell), self.character(cell));
        KNOWN.with_borrow_mut(|known| {
            if known.len() >= KEPT {
                known.clear();
            }
            known.insert((self.id, cell), info);
        });
        info
    }

    /// The level of site `cell`'s area, from 0, the lowest.
    pub fn level(&self, cell: [i32; 2]) -> u32 {
        let top = self.plan.levels - 1;
        if top == 0 || self.character(cell) == Character::Basin || self.kept == Some(cell) {
            return 0;
        }
        let [x, z] = self.sites.site(cell);
        let size = self.sites.spacing() * LEVEL_SIZE;
        // About -1 to 1, made a little wider so the highest and lowest levels come as often as the rest.
        let broad = (noise::gradient(x / size, z / size, self.sites.seed() ^ 0x1E7E) * 0.75 + 0.5).clamp(0.0, 0.999);
        let mut level = (broad * self.plan.levels as f32) as i32;
        let odd = self.sites.pick(cell, 7);
        if odd < ODD_LEVEL * 0.5 {
            level -= 1;
        } else if odd < ODD_LEVEL {
            level += 1;
        }
        level.clamp(0, top as i32) as u32
    }

    /// The height of site `cell`'s area's floor, in metres.
    pub fn floor(&self, cell: [i32; 2]) -> f32 {
        let top = (self.plan.levels - 1).max(1) as f32;
        self.plan.low + (self.plan.high - self.plan.low) * self.level(cell) as f32 / top
    }

    /// The character of site `cell`'s area.
    pub fn character(&self, cell: [i32; 2]) -> Character {
        if self.kept == Some(cell) {
            return Character::Plain;
        }
        let total: f32 = self.plan.characters.iter().sum();
        let mut left = self.sites.pick(cell, 5) * total;
        for (at, &weight) in self.plan.characters.iter().enumerate() {
            if left < weight {
                return Character::ALL[at];
            }
            left -= weight;
        }
        Character::Rolling
    }

    /// The place at `(x, z)` moved by the sites' warps and a raggeder one
    /// of its own, so the walls between levels, and the borders of the
    /// regions that keep to them, are ragged up close.
    pub fn place(&self, x: f32, z: f32) -> [f32; 2] {
        let [wx, wz] = self.sites.warped(x, z);
        let seed = self.sites.seed();
        let shift = |part: u32| {
            RAGGED * noise::gradient(x / RAGGED_SIZE, z / RAGGED_SIZE, seed ^ part)
                + ROUGH * noise::gradient(x / ROUGH_SIZE, z / ROUGH_SIZE, seed ^ (part + 1))
        };
        [wx + shift(0x4AC3), wz + shift(0x4AD3)]
    }

    /// The site of the area the place at `(x, z)` is in.
    pub fn area_at(&self, x: f32, z: f32) -> [i32; 2] {
        self.sites.nearest(self.place(x, z)).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn areas() -> Areas {
        let plan = LevelPlan { levels: 4, low: 10.0, high: 400.0, characters: [1.0; 7], keep: None };
        Areas::new(Sites::new(2000.0, 77), plan)
    }

    #[test]
    fn areas_stand_at_every_level_and_have_every_character_neighbours_mostly_alike() {
        let areas = areas();
        let cells: Vec<[i32; 2]> = (0..40 * 40).map(|k| [k % 40 - 20, k / 40 - 20]).collect();
        let mut levels = [0usize; 4];
        let mut characters = [0usize; 7];
        for &cell in &cells {
            levels[areas.level(cell) as usize] += 1;
            characters[areas.character(cell) as usize] += 1;
            let floor = areas.floor(cell);
            assert!([10.0, 140.0, 270.0, 400.0].iter().any(|&f: &f32| (f - floor).abs() < 0.01), "{floor}");
        }
        assert!(levels.iter().all(|&count| count > 40), "every level comes: {levels:?}");
        assert!(characters.iter().all(|&count| count > 100), "every character comes: {characters:?}");
        // Neighbours are more often within a level of each other than not.
        let pairs: Vec<[i32; 2]> = cells.iter().copied().filter(|&[x, _]| x < 19).collect();
        let near = pairs
            .iter()
            .filter(|&&[x, z]| (areas.level([x, z]) as i32 - areas.level([x + 1, z]) as i32).abs() <= 1)
            .count();
        assert!(near * 10 > pairs.len() * 7, "{near} of {}", pairs.len());
        // A basin is the lowest level.
        assert!(
            cells.iter().filter(|&&cell| areas.character(cell) == Character::Basin).all(|&cell| areas.level(cell) == 0)
        );
    }
}
