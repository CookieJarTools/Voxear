//! Biomes one region at a time: a world whose biomes keep to regions is one
//! biome across a whole region, a few hundred metres to a kilometre or two
//! across, until it crosses into the next. No specks of another biome show
//! inside a region, as they do where each place takes the biome its own
//! climate is nearest.
//!
//! The world is shared out among sites on a grid `spacing` metres apart,
//! each moved up to three tenths of that from its place in the grid by the
//! seed. Every place belongs to the site nearest it, after it is moved a
//! little by a smooth, gentle warp, so the borders between regions wander
//! rather than running straight, and no region is ever broken in two: the
//! warp never folds the land over on itself. A site's biome is the one its
//! climate at the sea's height is nearest, chosen once, by the world's
//! biomes' own rules.
//!
//! Biomes that keep to a height or to sunk ground, the snow over the
//! mountains or a chasm's floor, are not regions of their own: they lie
//! over whichever region they are in, where a place's own climate, height
//! and sinking are within all their ranges, as they always did.
//!
//! What a world's land is shaped by in each biome (see `land`) follows the
//! regions too, blended over a strip `border` metres wide, so one kind of
//! land goes on until the next as one biome does.
//!
//! Every site's biome is worked out once on each thread and kept, and a
//! place asks about the sites round its own alone, so regions cost little
//! more than a place's climate.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use super::{
    WorldGen,
    areas::{Areas, LevelPlan, Sites, border_distance},
    biomes::{Biomes, Place},
};

/// How many sites' biomes a thread keeps before it lets them all go.
const KEPT: usize = 16384;

/// The regions of a world of blocks' biomes.
pub struct Regions {
    spacing: f32,
    border: f32,
    sites: Sites,
    /// Where the world's land lies in levels: its areas, the same as the
    /// regions, and the level of each.
    areas: Option<Arc<Areas>>,
    biomes: Biomes,
    /// The world's generator, whose climate at each site picks its biome.
    world: Arc<WorldGen>,
    /// What tells these regions from any others: their world, spacing,
    /// seed and biomes.
    id: u64,
}

thread_local! {
    /// Sites' biomes worked out on this thread, by regions and site.
    static SITES: RefCell<HashMap<(u64, [i32; 2]), Option<usize>>> = RefCell::new(HashMap::new());
}

impl Regions {
    /// Regions about `spacing` metres across of `biomes`, their sites'
    /// climates read from `world`, land blended over `border` metres.
    pub fn new(spacing: f32, border: f32, biomes: Biomes, world: Arc<WorldGen>) -> Self {
        let spacing = if spacing.is_finite() { spacing.clamp(64.0, 20_000.0) } else { 768.0 };
        let border = if border.is_finite() { border.clamp(0.0, spacing * 0.5) } else { 96.0 };
        let seed = world.seed() ^ 0x5E61_0A11;
        let lie = format!(
            "{:?}",
            biomes
                .list()
                .iter()
                .map(|biome| (biome.temperature, biome.humidity, biome.height, biome.sunk))
                .collect::<Vec<_>>()
        );
        let mut key = Vec::new();
        key.extend(world.hash().to_le_bytes());
        key.extend(spacing.to_bits().to_le_bytes());
        key.extend(seed.to_le_bytes());
        key.extend(lie.into_bytes());
        let id = crate::layers::stable_hash(&key);
        Self { spacing, border, sites: Sites::new(spacing, seed), areas: None, biomes, world, id }
    }

    /// These regions with the world's land in levels by `plan`, each region
    /// an area at a level of its own.
    pub fn with_levels(mut self, plan: LevelPlan) -> Self {
        let areas = Areas::new(self.sites, plan);
        let mut key = self.id.to_le_bytes().to_vec();
        key.extend(format!("{:?}", areas.plan).into_bytes());
        self.id = crate::layers::stable_hash(&key);
        self.areas = Some(Arc::new(areas));
        self
    }

    /// The areas of the world's levels, where its land lies in levels.
    pub fn areas(&self) -> Option<&Arc<Areas>> {
        self.areas.as_ref()
    }

    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    pub fn border(&self) -> f32 {
        self.border
    }

    /// What tells these regions from any others.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The biome of the place at `(x, z)`, in metres in the generator's
    /// world, with `climate` there: a biome that keeps to a height or to
    /// sunk ground where the place is within all its ranges, else its
    /// region's.
    ///
    /// Where the land lies in levels, a place's height counts from its
    /// area's floor, so the snow lies on what rises over a level and not on
    /// every high level.
    pub fn at(&self, x: f32, z: f32, mut climate: Place) -> Option<usize> {
        if let Some(areas) = &self.areas {
            let floor = areas.info(areas.area_at(x, z)).0 - areas.plan.low;
            let over_lowest = climate.above_sea + self.world.sea_level() - areas.plan.low;
            climate.above_sea -= floor.min(over_lowest).max(0.0);
        }
        self.biomes.over(climate).or_else(|| self.region(x, z))
    }

    /// The biome of the region the place at `(x, z)` is in.
    pub fn region(&self, x: f32, z: f32) -> Option<usize> {
        let at = self.warped(x, z);
        let (nearest, _) = self.nearest(at);
        self.site_biome(nearest)
    }

    /// How much the place at `(x, z)` is of `biome`'s region, from 0 to 1:
    /// 1 within it but within half the border of its edge, 0 as far out of
    /// it, a half on the edge itself, and eased between.
    pub fn weight(&self, x: f32, z: f32, biome: usize) -> f32 {
        let at = self.warped(x, z);
        let (own, sites) = self.nearest(at);
        let mine = self.site_biome(own) == Some(biome);
        let own_at = self.site(own);
        // How far the place is from the nearest border between its own region and one of the other kind.
        let mut nearest = f32::MAX;
        for &other in sites.iter().filter(|&&other| other != own) {
            if (self.site_biome(other) == Some(biome)) == mine {
                continue;
            }
            nearest = nearest.min(border_distance(at, own_at, self.site(other)));
        }
        if nearest == f32::MAX {
            return if mine { 1.0 } else { 0.0 };
        }
        let half = (self.border * 0.5).max(1e-3);
        let signed = if mine { nearest } else { -nearest };
        let t = ((signed / half) * 0.5 + 0.5).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// The place at `(x, z)` moved by the warps: the areas' own, where the
    /// land lies in levels, so a region's border is its level's wall.
    fn warped(&self, x: f32, z: f32) -> [f32; 2] {
        match &self.areas {
            Some(areas) => areas.place(x, z),
            None => self.sites.warped(x, z),
        }
    }

    /// The site nearest `at`, and the nine round it.
    fn nearest(&self, at: [f32; 2]) -> ([i32; 2], [[i32; 2]; 9]) {
        self.sites.nearest(at)
    }

    /// Where site `cell` lies, in metres.
    fn site(&self, cell: [i32; 2]) -> [f32; 2] {
        self.sites.site(cell)
    }

    /// The biome of site `cell`'s region: the one its climate at the sea's
    /// height is nearest, among those that are regions of their own.
    fn site_biome(&self, cell: [i32; 2]) -> Option<usize> {
        if let Some(kept) = SITES.with_borrow(|kept| kept.get(&(self.id, cell)).copied()) {
            return kept;
        }
        let [x, z] = self.site(cell);
        let sea = self.world.sea_level();
        let (temperature, humidity) = self.world.climate(x, z, sea);
        let biome = self.biomes.of_region(Place { temperature, humidity, above_sea: 0.0, sunk: 0.0 });
        SITES.with_borrow_mut(|kept| {
            if kept.len() >= KEPT {
                kept.clear();
            }
            kept.insert((self.id, cell), biome);
        });
        biome
    }
}

impl std::fmt::Debug for Regions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Regions({} m, {:#x})", self.spacing, self.id)
    }
}

impl PartialEq for Regions {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.border == other.border
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{BiomeDef, Preset};

    /// Three biomes of a cold, a mild and a warm world, as regions 400 m
    /// apart over the Rolling Hills, and snow over 120 m.
    fn regions(spacing: f32) -> (Regions, Arc<WorldGen>) {
        let world = Arc::new(Preset::RollingHills.world());
        let biome = |temperature: [f32; 2]| BiomeDef { temperature, humidity: [0.0, 1.0], ..BiomeDef::default() };
        let snow = BiomeDef { height: Some([120.0, 10_000.0]), ..biome([-60.0, 60.0]) };
        let list = vec![biome([-60.0, 8.0]), biome([8.0, 13.0]), biome([13.0, 60.0]), snow];
        (Regions::new(spacing, 96.0, Biomes::new(list, 15.0, 0.3, world.seed()), world.clone()), world)
    }

    #[test]
    fn a_region_is_one_biome_through_with_no_specks_of_another_and_borders_that_wander() {
        let (regions, _) = regions(400.0);
        // A 4 km square at 8 m: every place's region, and how many regions of each biome hold places.
        let side = 500;
        let at = |i: usize, j: usize| regions.region(i as f32 * 8.0, j as f32 * 8.0);
        let grid: Vec<Option<usize>> = (0..side * side).map(|k| at(k % side, k / side)).collect();
        let kinds: std::collections::BTreeSet<usize> = grid.iter().flatten().copied().collect();
        assert!(kinds.len() >= 2, "more than one biome over 4 km: {kinds:?}");
        assert!(!kinds.contains(&3), "snow is no region of its own");
        // No specks: every place has at least three of its four neighbours alike, but on a border.
        let mut lone = 0;
        for j in 1..side - 1 {
            for i in 1..side - 1 {
                let here = grid[j * side + i];
                let alike = [(i - 1, j), (i + 1, j), (i, j - 1), (i, j + 1)]
                    .iter()
                    .filter(|&&(a, b)| grid[b * side + a] == here)
                    .count();
                if alike == 0 {
                    lone += 1;
                }
            }
        }
        assert_eq!(lone, 0, "places alone among others");
        // Each piece of one biome is large: count the places of the smallest connected piece.
        let mut seen = vec![false; grid.len()];
        let mut smallest = usize::MAX;
        for start in 0..grid.len() {
            if seen[start] {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut count = 0;
            let mut touches_edge = false;
            while let Some(k) = stack.pop() {
                count += 1;
                let (i, j) = (k % side, k / side);
                touches_edge |= i == 0 || j == 0 || i == side - 1 || j == side - 1;
                let next = [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)];
                for (a, b) in next {
                    if a < side && b < side && !seen[b * side + a] && grid[b * side + a] == grid[k] {
                        seen[b * side + a] = true;
                        stack.push(b * side + a);
                    }
                }
            }
            if !touches_edge {
                smallest = smallest.min(count);
            }
        }
        // A piece wholly inside the square is at least a small region: 200 m across, 625 places of 8 m.
        assert!(smallest >= 625, "a piece of only {smallest} places");
        // Borders wander: along a row, the place a border is crossed is not at the cell's straight edge.
        let crossings = (1..side).filter(|&i| grid[100 * side + i] != grid[100 * side + i - 1]).count();
        assert!(crossings >= 2, "{crossings} borders crossed along 4 km");
    }

    #[test]
    fn snow_lies_over_whichever_region_where_the_place_is_high_and_cold_enough() {
        let (regions, _) = regions(400.0);
        let place = |above_sea: f32| Place { temperature: 0.0, humidity: 0.5, above_sea, sunk: 0.0 };
        let low = regions.at(1234.0, 567.0, place(30.0));
        assert_ne!(low, Some(3));
        assert_eq!(low, regions.region(1234.0, 567.0));
        assert_eq!(regions.at(1234.0, 567.0, place(200.0)), Some(3));
    }

    #[test]
    fn a_biomes_weight_is_whole_inside_its_region_none_far_out_and_eases_across_the_border() {
        let (regions, _) = regions(400.0);
        let row: Vec<(Option<usize>, f32)> = (0..2000)
            .map(|i| {
                let x = i as f32 * 2.0;
                let biome = regions.region(x, 300.0);
                (biome, biome.map_or(0.0, |b| regions.weight(x, 300.0, b)))
            })
            .collect();
        // Every place is at least half its own region's, and wholly so but near its borders.
        assert!(row.iter().all(|(_, weight)| *weight >= 0.499), "a place less than half its own region's");
        let whole = row.iter().filter(|(_, weight)| *weight > 0.999).count();
        assert!(whole > row.len() / 2, "{whole} of {} places wholly their region's", row.len());
        // And it eases: no step of more than a small share from one place to the next for any one biome.
        for biome in 0..3 {
            let weights: Vec<f32> = (0..2000).map(|i| regions.weight(i as f32 * 2.0, 300.0, biome)).collect();
            let steepest = weights.windows(2).map(|pair| (pair[1] - pair[0]).abs()).fold(0.0, f32::max);
            assert!(steepest < 0.15, "biome {biome} steps {steepest} in 2 m");
        }
    }
}
