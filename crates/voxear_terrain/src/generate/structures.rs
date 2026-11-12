//! Where structures stand in a generated world: a house, a tower, a ruin, a
//! dungeon's way in. A structure's placement says how far apart its sites
//! are and what ground a site needs; the sites it gives are the same from
//! anywhere they are asked for, so a chunk made alone knows every structure
//! that reaches it, and so does the far view.
//!
//! The world is cut into square cells `spacing` metres a side. Each cell
//! has one try at a site, at a place of its own within the cell that keeps
//! `separation` metres clear of the next cell's, so no two sites of a
//! placement are nearer than that. A try is kept with `chance`, and then
//! only where the ground fits: over or under the sea as asked, within its
//! heights, no steeper than its slope, in one of its biomes.
//!
//! ```toml
//! spacing = 320.0        # metres between tries
//! separation = 96.0      # the least between two sites
//! chance = 0.6           # how many tries are kept, 0 to 1
//! apart = 200.0          # the least between a site and another structure's, in metres
//! on = "land"            # or "water", "underground"
//! height = [2.0, 180.0]  # metres over the sea; left out for any
//! slope = 12.0           # the steepest ground it stands on, in degrees
//! biomes = ["Biomes/Grassland.biome", "Biomes/Forest.biome"]  # none for any
//! ```
//!
//! Whole-number hashes, sums and products alone: the same sites on every
//! machine.

use serde::{Deserialize, Serialize};

/// What a site stands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SiteOn {
    /// Ground over the sea.
    #[default]
    Land,
    /// The sea, where the ground lies under it.
    Water,
    /// Under the ground, wherever the ground is.
    Underground,
}

/// Where a structure's sites are, and what ground one needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Placement {
    /// How far apart its tries are, in metres.
    pub spacing: f32,
    /// The least between two of its sites, in metres: under its spacing.
    pub separation: f32,
    /// How many of its tries are kept, from 0 for none to 1 for all.
    pub chance: f32,
    /// The least between one of its sites and another structure's, in
    /// metres, so a world is not crowded with them: the larger of the two
    /// structures' is kept.
    #[serde(skip_serializing_if = "is_zero_metres")]
    pub apart: f32,
    /// What tells its sites from another placement's with the same numbers.
    #[serde(skip_serializing_if = "is_zero")]
    pub salt: u32,
    pub on: SiteOn,
    /// How high over the sea its ground is, in metres, from and to; none
    /// for any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<[f32; 2]>,
    /// The steepest ground it stands on, in degrees.
    pub slope: f32,
    /// The biomes it stands in, each a reference to a `.biome`; none for
    /// any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub biomes: Vec<String>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn is_zero_metres(value: &f32) -> bool {
    *value == 0.0
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            spacing: 320.0,
            separation: 96.0,
            chance: 0.6,
            apart: 0.0,
            salt: 0,
            on: SiteOn::Land,
            height: None,
            slope: 15.0,
            biomes: Vec::new(),
        }
    }
}

/// A place a structure stands: which cell's try it is, where, in metres in
/// the generator's world, how many quarter turns it is turned, whether it
/// is mirrored, and a seed of its own for what is built there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub cell: [i32; 2],
    pub at: [f32; 2],
    pub turns: u8,
    pub mirrored: bool,
    pub seed: u32,
}

/// The ground at a site, as its world's generator gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteGround {
    /// Metres over the sea; under it, below zero.
    pub above_sea: f32,
    /// How much it rises for every metre along.
    pub slope: f32,
    /// Which of its world's biomes it is, by its place in the world's list;
    /// none where the world lists no biomes.
    pub biome: Option<usize>,
}

/// A number for cell `(x, z)` of a placement with `salt` in a world with
/// `seed`, well mixed.
fn cell_hash(seed: u32, salt: u32, [x, z]: [i32; 2], part: u32) -> u32 {
    let mut h = seed ^ salt.wrapping_mul(0x9E37_79B9) ^ part.wrapping_mul(0x85EB_CA6B);
    h ^= (x as u32).wrapping_mul(0x8DA6_B343);
    h = h.rotate_left(13).wrapping_mul(0xC2B2_AE35);
    h ^= (z as u32).wrapping_mul(0xD816_3841);
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

impl Placement {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// Its spacing and separation as they are used: a metre at the least,
    /// and the separation no more than the spacing.
    fn grid(&self) -> (f32, f32) {
        let spacing = self.spacing.max(1.0);
        (spacing, self.separation.clamp(0.0, spacing))
    }

    /// The try of `cell` in a world with `seed`, where it is kept by its
    /// chance: before its ground is looked at.
    pub fn try_at(&self, seed: u32, cell: [i32; 2]) -> Option<Site> {
        let (spacing, separation) = self.grid();
        if unit(cell_hash(seed, self.salt, cell, 0)) >= self.chance.clamp(0.0, 1.0) {
            return None;
        }
        // Anywhere in the cell but its last `separation` metres each way, so the next cell's is that far off.
        let free = spacing - separation;
        let within = |part: u32| unit(cell_hash(seed, self.salt, cell, part)) * free;
        let at = [cell[0] as f32 * spacing + within(1), cell[1] as f32 * spacing + within(2)];
        let turned = cell_hash(seed, self.salt, cell, 3);
        Some(Site {
            cell,
            at,
            turns: (turned & 3) as u8,
            mirrored: turned & 4 != 0,
            seed: cell_hash(seed, self.salt, cell, 4),
        })
    }

    /// Every try kept by its chance whose place is within `reach` metres of
    /// the box from `low` to `high`, in metres in the generator's world, in
    /// a fixed order: what a chunk asks to find the structures that may
    /// reach it, `reach` the furthest a structure reaches from its site.
    pub fn tries_near(&self, seed: u32, low: [f32; 2], high: [f32; 2], reach: f32) -> Vec<Site> {
        let (spacing, _) = self.grid();
        let reach = reach.max(0.0);
        let first = [0, 1].map(|axis| ((low[axis] - reach) / spacing).floor() as i32);
        let last = [0, 1].map(|axis| ((high[axis] + reach) / spacing).floor() as i32);
        let mut sites = Vec::new();
        for z in first[1]..=last[1] {
            for x in first[0]..=last[0] {
                let Some(site) = self.try_at(seed, [x, z]) else { continue };
                let near = (0..2).all(|axis| site.at[axis] >= low[axis] - reach && site.at[axis] <= high[axis] + reach);
                if near {
                    sites.push(site);
                }
            }
        }
        sites
    }

    /// Whether `ground` is ground a site of it can stand on, `biome_fits`
    /// saying whether a biome of the world's list is one of its own.
    pub fn fits(&self, ground: SiteGround, biome_fits: impl Fn(usize) -> bool) -> bool {
        let on = match self.on {
            SiteOn::Land => ground.above_sea > 0.0,
            SiteOn::Water => ground.above_sea <= 0.0,
            SiteOn::Underground => true,
        };
        let height =
            self.height.is_none_or(|[from, to]| ground.above_sea >= from.min(to) && ground.above_sea <= from.max(to));
        // Its slope as rise over run: the tangent by its series, good to a part in a thousand under 60 degrees.
        let x = self.slope.clamp(0.0, 80.0) * (std::f32::consts::PI / 180.0);
        let x2 = x * x;
        let sin = x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
        let cos = 1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0)));
        let level = self.on != SiteOn::Land || ground.slope <= sin / cos.max(1e-3);
        let biome = self.biomes.is_empty() || ground.biome.is_some_and(biome_fits);
        on && height && level && biome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placement_is_written_and_read_back_and_an_unknown_key_is_refused() {
        let towers = Placement {
            spacing: 500.0,
            on: SiteOn::Water,
            height: Some([-30.0, -4.0]),
            biomes: vec!["Biomes/Tundra.biome".into()],
            salt: 7,
            ..Placement::default()
        };
        let text = towers.to_text();
        assert!(text.contains("on = \"water\""), "{text}");
        assert_eq!(Placement::parse(&text).unwrap(), towers);
        assert_eq!(Placement::parse("").unwrap(), Placement::default());
        assert!(Placement::parse("spacng = 3.0").is_err());
        let plain = Placement::default().to_text();
        assert!(!plain.contains("salt") && !plain.contains("biomes") && !plain.contains("height"), "{plain}");
    }

    #[test]
    fn sites_are_the_same_from_anywhere_never_nearer_than_their_separation_and_as_many_as_their_chance() {
        let houses = Placement { spacing: 200.0, separation: 60.0, chance: 0.5, ..Placement::default() };
        // Every try within 4 km of the middle, asked for at once.
        let all = houses.tries_near(9, [-4000.0; 2], [4000.0; 2], 0.0);
        // About half of 1600 cells.
        assert!((650..950).contains(&all.len()), "{} sites", all.len());
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let apart = ((a.at[0] - b.at[0]).powi(2) + (a.at[1] - b.at[1]).powi(2)).sqrt();
                assert!(apart >= 60.0, "{:?} and {:?} are {apart} m apart", a.cell, b.cell);
            }
        }
        // Asked for a chunk at a time with a reach, every site within reach of a chunk is found from it, as it was.
        let chunk = 32.0;
        let mut found = 0;
        for cz in -20..20 {
            for cx in -20..20 {
                let (low, high) =
                    ([cx as f32 * chunk, cz as f32 * chunk], [(cx + 1) as f32 * chunk, (cz + 1) as f32 * chunk]);
                let near = houses.tries_near(9, low, high, 24.0);
                let meant: Vec<&Site> = all
                    .iter()
                    .filter(|site| (0..2).all(|k| site.at[k] >= low[k] - 24.0 && site.at[k] <= high[k] + 24.0))
                    .collect();
                assert_eq!(near.iter().collect::<Vec<_>>(), meant, "chunk ({cx}, {cz})");
                found += near.len();
            }
        }
        assert!(found > 20);
        // Another seed or another salt, other sites; turns and mirrors of every kind.
        assert_ne!(houses.tries_near(10, [-4000.0; 2], [4000.0; 2], 0.0), all);
        let salted = Placement { salt: 1, ..houses.clone() };
        assert_ne!(salted.tries_near(9, [-4000.0; 2], [4000.0; 2], 0.0), all);
        for turns in 0..4 {
            assert!(all.iter().any(|site| site.turns == turns && site.mirrored));
            assert!(all.iter().any(|site| site.turns == turns && !site.mirrored));
        }
        // None with no chance, one a cell with every chance.
        let none = Placement { chance: 0.0, ..houses.clone() };
        assert!(none.tries_near(9, [-4000.0; 2], [4000.0; 2], 0.0).is_empty());
        let every = Placement { chance: 1.0, ..houses };
        assert_eq!(every.tries_near(9, [0.0; 2], [1999.0; 2], 0.0).len(), 100);
    }

    #[test]
    fn a_site_needs_the_ground_its_placement_asks_for() {
        let ground = |above_sea: f32, slope: f32, biome: Option<usize>| SiteGround { above_sea, slope, biome };
        let any = |_: usize| true;
        let land = Placement { height: Some([2.0, 100.0]), slope: 10.0, ..Placement::default() };
        assert!(land.fits(ground(20.0, 0.1, None), any));
        assert!(!land.fits(ground(-3.0, 0.0, None), any), "under the sea");
        assert!(!land.fits(ground(1.0, 0.0, None), any), "too low");
        assert!(!land.fits(ground(150.0, 0.0, None), any), "too high");
        // 10 degrees is 0.176 in 1.
        assert!(land.fits(ground(20.0, 0.17, None), any) && !land.fits(ground(20.0, 0.19, None), any));
        let water = Placement { on: SiteOn::Water, ..Placement::default() };
        assert!(water.fits(ground(-8.0, 2.0, None), any) && !water.fits(ground(8.0, 0.0, None), any));
        let under = Placement { on: SiteOn::Underground, ..Placement::default() };
        assert!(under.fits(ground(-8.0, 3.0, None), any) && under.fits(ground(80.0, 3.0, None), any));
        // In its biomes alone, where it names any: the second of the world's here.
        let forest = Placement { biomes: vec!["Biomes/Forest.biome".into()], ..Placement::default() };
        let second = |biome: usize| biome == 1;
        assert!(forest.fits(ground(20.0, 0.0, Some(1)), second));
        assert!(!forest.fits(ground(20.0, 0.0, Some(0)), second));
        assert!(!forest.fits(ground(20.0, 0.0, None), second), "a world with no biomes has none of its");
    }
}
