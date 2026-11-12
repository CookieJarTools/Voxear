//! A `.voxplant` file: a tree or a bush of blocks, grown from a few numbers
//! and a seed. The file says what kind of plant it is and how it grows: how
//! tall its trunk is and how it tapers and bends, how wide and high its
//! crown is and how thick with branches, how large its clumps of leaves
//! are, how many roots it has, and which blocks its wood and its leaves
//! are. Every seed grows another plant of the same kind, so a forest of one
//! file has no two trees alike.
//!
//! ```toml
//! kind = "broadleaf"             # or "conifer", "bush"
//! height = [9.0, 14.0]           # blocks, from and to: a seed picks within
//! trunk = 1.4                    # how thick at the foot, in blocks across
//! taper = 0.6                    # how much thinner at the top, 0 to 1
//! bend = 0.25                    # how far it leans and wanders, 0 for straight
//! crown_width = [8.0, 12.0]
//! crown_height = [6.0, 9.0]
//! branches = 0.6                 # how thick with branches, 0 to 1
//! leaf_size = 4.0                # how large a clump of leaves is, in blocks
//! leaf_density = 0.8             # how full a clump is, 0 to 1
//! roots = 4
//! wood = "Blocks/Oak Log.block"
//! leaves = "Blocks/Oak Leaves.block"
//! ```
//!
//! A broadleaf's crown is grown towards points scattered through its shape:
//! each branch end grows a step towards the points nearest it, and a point
//! is taken away once a branch reaches it, so branches fork to fill the
//! crown and never cross it twice. A conifer has a ring of branches every
//! so often up its trunk, each ring shorter than the one below. A bush is
//! stems from the ground with a clump at each end.
//!
//! It is all whole-number hashes, sums, products and square roots, so a
//! seed grows the same plant on every machine.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The kind of plant, which sets how it grows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlantKind {
    /// A trunk and a rounded crown of forking branches.
    #[default]
    Broadleaf,
    /// A straight trunk with rings of branches, shorter towards the top.
    Conifer,
    /// Stems from the ground with a clump of leaves at each end.
    Bush,
}

impl PlantKind {
    pub const ALL: [PlantKind; 3] = [PlantKind::Broadleaf, PlantKind::Conifer, PlantKind::Bush];

    pub fn name(self) -> &'static str {
        match self {
            PlantKind::Broadleaf => "Broadleaf",
            PlantKind::Conifer => "Conifer",
            PlantKind::Bush => "Bush",
        }
    }
}

/// What a `.voxplant` file holds. Lengths are in blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlantDef {
    pub kind: PlantKind,
    /// How tall, from and to: a seed picks within.
    pub height: [f32; 2],
    /// How thick its trunk is at the foot, across.
    pub trunk: f32,
    /// How much thinner it is at the top, from 0 for none to 1 for a point.
    pub taper: f32,
    /// How far the trunk leans and wanders: 0 for straight.
    pub bend: f32,
    pub crown_width: [f32; 2],
    pub crown_height: [f32; 2],
    /// How thick with branches the crown is, 0 to 1.
    pub branches: f32,
    /// How large a clump of leaves is.
    pub leaf_size: f32,
    /// How full a clump is, 0 to 1.
    pub leaf_density: f32,
    pub roots: u32,
    /// The blocks its wood and leaves are, each a reference to a `.block`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaves: Option<String>,
}

impl Default for PlantDef {
    fn default() -> Self {
        Self {
            kind: PlantKind::Broadleaf,
            height: [9.0, 14.0],
            trunk: 1.4,
            taper: 0.6,
            bend: 0.25,
            crown_width: [8.0, 12.0],
            crown_height: [6.0, 9.0],
            branches: 0.6,
            leaf_size: 4.0,
            leaf_density: 0.8,
            roots: 4,
            wood: None,
            leaves: None,
        }
    }
}

/// What a block of a grown plant is part of. Later kinds give way to
/// earlier ones where two would share a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlantPart {
    Trunk,
    Branch,
    Root,
    Leaf,
}

impl PlantPart {
    /// Whether it is wood: the plant's `wood` block rather than its `leaves`.
    pub fn is_wood(self) -> bool {
        self != PlantPart::Leaf
    }
}

/// A plant grown: its blocks, each by where it is from the foot of its
/// trunk (x and z along the ground, y up; roots reach below 0) and what it
/// is part of, in a fixed order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlantModel {
    pub blocks: Vec<([i32; 3], PlantPart)>,
}

impl PlantModel {
    /// The lowest and highest block along each axis; none for no blocks.
    pub fn bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let first = self.blocks.first()?.0;
        Some(self.blocks.iter().fold((first, first), |(low, high), (at, _)| {
            ([0, 1, 2].map(|k| low[k].min(at[k])), [0, 1, 2].map(|k| high[k].max(at[k])))
        }))
    }

    pub fn count(&self, part: PlantPart) -> usize {
        self.blocks.iter().filter(|(_, kind)| *kind == part).count()
    }
}

/// Numbers from a seed, each from the last: the same on every machine.
struct Random(u32);

impl Random {
    fn new(seed: u32) -> Self {
        Self(seed ^ 0x9E37_79B9)
    }

    fn next(&mut self) -> u32 {
        let mut h = self.0.wrapping_add(0x6D2B_79F5);
        self.0 = h;
        h = (h ^ (h >> 15)).wrapping_mul(h | 1);
        h ^= h.wrapping_add((h ^ (h >> 7)).wrapping_mul(h | 61));
        h ^ (h >> 14)
    }

    /// From 0 up to 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }

    fn between(&mut self, [from, to]: [f32; 2]) -> f32 {
        from + (to - from) * self.unit()
    }

    /// From -1 to 1.
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// A point within a ball of radius 1, evenly.
    fn in_ball(&mut self) -> [f32; 3] {
        loop {
            let p = [self.signed(), self.signed(), self.signed()];
            if p[0] * p[0] + p[1] * p[1] + p[2] * p[2] <= 1.0 {
                return p;
            }
        }
    }
}

/// Eight ways along the ground, a turn of an eighth apart.
const AROUND: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.7071, 0.7071],
    [0.0, 1.0],
    [-0.7071, 0.7071],
    [-1.0, 0.0],
    [-0.7071, -0.7071],
    [0.0, -1.0],
    [0.7071, -0.7071],
];

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn add(a: [f32; 3], b: [f32; 3], by: f32) -> [f32; 3] {
    [a[0] + b[0] * by, a[1] + b[1] * by, a[2] + b[2] * by]
}

/// A plant as it grows: its blocks so far.
struct Growing {
    blocks: HashMap<[i32; 3], PlantPart>,
}

impl Growing {
    fn put(&mut self, at: [i32; 3], part: PlantPart) {
        let kept = self.blocks.entry(at).or_insert(part);
        if part < *kept {
            *kept = part;
        }
    }

    /// Every block whose middle is within `radius` of `at`; the one `at` is
    /// in at the least.
    fn ball(&mut self, at: [f32; 3], radius: f32, part: PlantPart) {
        self.put(at.map(|v| v.floor() as i32), part);
        let reach = radius.ceil() as i32;
        let middle = at.map(|v| v.floor() as i32);
        for dy in -reach..=reach {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let block = [middle[0] + dx, middle[1] + dy, middle[2] + dz];
                    let off = [0, 1, 2].map(|k| block[k] as f32 + 0.5 - at[k]);
                    if length(off) <= radius {
                        self.put(block, part);
                    }
                }
            }
        }
    }

    /// Blocks along the line from `from` to `to`, `radius` thick.
    fn limb(&mut self, from: [f32; 3], to: [f32; 3], radius: f32, part: PlantPart) {
        let along = [0, 1, 2].map(|k| to[k] - from[k]);
        let steps = (length(along) * 2.0).ceil().max(1.0) as u32;
        for step in 0..=steps {
            self.ball(add(from, along, step as f32 / steps as f32), radius, part);
        }
    }

    /// A clump of leaves round `at`: a ball `size` across, a little flat,
    /// each block of it there or not by `density`, fuller in its middle.
    fn clump(&mut self, at: [f32; 3], size: f32, density: f32, random: &mut Random) {
        let radius = (size * 0.5).max(0.5);
        let reach = radius.ceil() as i32;
        let middle = at.map(|v| v.floor() as i32);
        for dy in -reach..=reach {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let block = [middle[0] + dx, middle[1] + dy, middle[2] + dz];
                    let off = [0, 1, 2].map(|k| block[k] as f32 + 0.5 - at[k]);
                    let away = length([off[0], off[1] * 1.3, off[2]]) / radius;
                    // Drawn for every block, kept or not, so a clump's blocks do not depend on those before it.
                    let chance = random.unit();
                    if away <= 1.0 && chance < density + (1.0 - away) * (1.0 - density) * 0.8 {
                        self.put(block, PlantPart::Leaf);
                    }
                }
            }
        }
    }
}

impl PlantDef {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// The plant `seed` grows.
    pub fn grow(&self, seed: u32) -> PlantModel {
        let mut random = Random::new(seed);
        let mut plant = Growing { blocks: HashMap::new() };
        let height = random.between(self.height).max(1.0);
        let width = random.between(self.crown_width).max(1.0);
        let crown = random.between(self.crown_height).clamp(1.0, height.max(1.0) * 2.0);
        match self.kind {
            PlantKind::Broadleaf => {
                let top = self.trunk_up(&mut plant, &mut random, height);
                self.crown(&mut plant, &mut random, top, (width, crown));
            }
            PlantKind::Conifer => {
                let top = self.trunk_up(&mut plant, &mut random, height);
                self.whorls(&mut plant, &mut random, (height, top), (width, crown));
            }
            PlantKind::Bush => self.stems(&mut plant, &mut random, (width, height.min(crown))),
        }
        if self.kind != PlantKind::Bush {
            self.roots_out(&mut plant, &mut random);
        }
        let mut blocks: Vec<([i32; 3], PlantPart)> = plant.blocks.into_iter().collect();
        blocks.sort_by_key(|(at, _)| (at[1], at[2], at[0]));
        PlantModel { blocks }
    }

    /// Half how thick the trunk is `share` of the way up it.
    fn trunk_radius(&self, share: f32) -> f32 {
        (self.trunk.max(0.5) * 0.5 * (1.0 - self.taper.clamp(0.0, 1.0) * share)).max(0.3)
    }

    /// The trunk, from the ground up `height` blocks, leaning and wandering
    /// by its bend; where its top is.
    fn trunk_up(&self, plant: &mut Growing, random: &mut Random, height: f32) -> [f32; 3] {
        let bend = self.bend.clamp(0.0, 1.0);
        let lean = [random.signed() * bend * 0.35, random.signed() * bend * 0.35];
        let mut at = [0.5, 0.0, 0.5];
        let steps = (height * 2.0).ceil() as u32;
        for step in 0..=steps {
            let share = step as f32 / steps as f32;
            plant.ball(at, self.trunk_radius(share), PlantPart::Trunk);
            // A lean one way, and a wander that grows with height.
            let wander = bend * 0.3 * share;
            at = [
                at[0] + (lean[0] + random.signed() * wander) * 0.5,
                at[1] + 0.5,
                at[2] + (lean[1] + random.signed() * wander) * 0.5,
            ];
        }
        [at[0], height, at[2]]
    }

    /// A broadleaf's crown round the trunk's `top`, `width` across and
    /// `high` tall: branches grown towards points scattered through it,
    /// with a clump of leaves at each end and along the outer branches.
    fn crown(&self, plant: &mut Growing, random: &mut Random, top: [f32; 3], (width, high): (f32, f32)) {
        let radii = [width * 0.5, high * 0.5, width * 0.5];
        // The trunk's top stands two thirds of the way up the crown.
        let middle = [top[0], top[1] - high * 0.5 + high * 0.33, top[2]];
        let branches = self.branches.clamp(0.0, 1.0);
        let wanted = (8.0 + branches * 0.5 * width * width * high / 12.0).min(400.0) as usize;
        let mut points: Vec<[f32; 3]> = (0..wanted)
            .map(|_| {
                let p = random.in_ball();
                [middle[0] + p[0] * radii[0], middle[1] + p[1] * radii[1], middle[2] + p[2] * radii[2]]
            })
            // None under where the trunk enters the crown.
            .filter(|p| p[1] > top[1] - high * 0.45)
            .collect();
        // Branch ends so far: where each is, the end it grew from, and how many grew from it.
        // They start from the trunk's top and from two places below it, so the crown's lower half fills too.
        let on_trunk = |down: f32| {
            let share = ((top[1] - down) / top[1].max(1.0)).clamp(0.0, 1.0);
            [0.5 + (top[0] - 0.5) * share, top[1] - down, 0.5 + (top[2] - 0.5) * share]
        };
        let mut ends: Vec<([f32; 3], usize, u32)> =
            vec![(top, 0, 0), (on_trunk(high * 0.2), 0, 0), (on_trunk(high * 0.4), 0, 0)];
        let (step, reach, near) = (1.2f32, width.max(high), 1.6f32);
        for _ in 0..48 {
            if points.is_empty() {
                break;
            }
            // Each point pulls the end nearest it.
            let mut pulls: Vec<([f32; 3], u32)> = vec![([0.0; 3], 0); ends.len()];
            for point in &points {
                let nearest = ends
                    .iter()
                    .enumerate()
                    .map(|(at, end)| (at, length([0, 1, 2].map(|k| point[k] - end.0[k]))))
                    .fold((0, f32::MAX), |best, found| if found.1 < best.1 { found } else { best });
                if nearest.1 <= reach {
                    let towards = [0, 1, 2].map(|k| (point[k] - ends[nearest.0].0[k]) / nearest.1.max(1e-4));
                    pulls[nearest.0].0 = add(pulls[nearest.0].0, towards, 1.0);
                    pulls[nearest.0].1 += 1;
                }
            }
            let mut grew = false;
            for (from, (pull, count)) in pulls.into_iter().enumerate() {
                let size = length(pull);
                if count == 0 || size < 1e-4 {
                    continue;
                }
                let next = add(ends[from].0, pull, step / size);
                plant.limb(ends[from].0, next, 0.35, PlantPart::Branch);
                ends[from].2 += 1;
                ends.push((next, from, 0));
                grew = true;
            }
            let before = points.len();
            points.retain(|point| ends.iter().all(|end| length([0, 1, 2].map(|k| point[k] - end.0[k])) > near));
            if !grew && points.len() == before {
                break;
            }
        }
        // Over the trunk's top always, so no trunk stands bare out of its crown.
        plant.clump(add(top, [0.0, 1.0, 0.0], self.leaf_size * 0.25), self.leaf_size, 1.0, random);
        for (at, _, grown) in &ends {
            let outer = length([0, 1, 2].map(|k| (at[k] - middle[k]) / radii[k].max(0.5)));
            if *grown == 0 || outer > 0.35 {
                plant.clump(*at, self.leaf_size, self.leaf_density.clamp(0.05, 1.0), random);
            }
        }
    }

    /// A conifer's rings of branches up its trunk, from where its crown
    /// begins to its `top`, each ring shorter than the one below, with
    /// leaves along every branch and a tip over the top.
    fn whorls(
        &self,
        plant: &mut Growing,
        random: &mut Random,
        (height, top): (f32, [f32; 3]),
        (width, high): (f32, f32),
    ) {
        let begins = (height - high).max(height * 0.15);
        let branches = self.branches.clamp(0.0, 1.0);
        let apart = (2.6 - branches * 1.4).max(1.0);
        let per_ring = if branches > 0.5 { 8 } else { 4 };
        let leaf = (self.leaf_size * 0.5).clamp(0.6, 1.6);
        let density = self.leaf_density.clamp(0.05, 1.0);
        let mut up = begins;
        let mut ring = 0u32;
        while up < height {
            let share = ((up - begins) / (height - begins).max(1.0)).clamp(0.0, 1.0);
            // Where the trunk is at this height: it leans evenly from its foot to its top.
            let along = up / height.max(1.0);
            let trunk = [0.5 + (top[0] - 0.5) * along, up, 0.5 + (top[2] - 0.5) * along];
            let reach = width * 0.5 * (1.0 - share) + 0.6;
            // Every other ring turned an eighth, when it has four branches.
            let turned = if per_ring == 4 { (ring % 2) as usize } else { 0 };
            for branch in 0..per_ring {
                let way = AROUND[(branch * (8 / per_ring) + turned) % 8];
                let long = reach * (0.8 + 0.2 * random.unit());
                // Out and a little down.
                let end = [trunk[0] + way[0] * long, up - long * 0.18, trunk[2] + way[1] * long];
                plant.limb(trunk, end, 0.3, PlantPart::Branch);
                let clumps = (long / (leaf * 1.2)).ceil().max(1.0) as u32;
                for clump in 1..=clumps {
                    let at = add(trunk, [0, 1, 2].map(|k| end[k] - trunk[k]), clump as f32 / clumps as f32);
                    plant.clump(at, leaf * 2.0, density, random);
                }
            }
            up += apart;
            ring += 1;
        }
        plant.clump([top[0], height + leaf * 0.5, top[2]], leaf * 2.0, 1.0, random);
        plant.put([top[0].floor() as i32, (height + leaf).floor() as i32, top[2].floor() as i32], PlantPart::Leaf);
    }

    /// A bush `width` across and `high` tall: stems from the ground leaning
    /// out, a clump of leaves at each end.
    fn stems(&self, plant: &mut Growing, random: &mut Random, (width, high): (f32, f32)) {
        let stems = (3.0 + self.branches.clamp(0.0, 1.0) * 5.0) as usize;
        let foot = [0.5, 0.0, 0.5];
        for stem in 0..stems {
            let way = AROUND[(stem * 3 + (random.next() % 2) as usize) % 8];
            let out = width * 0.5 * (0.3 + 0.7 * random.unit()) - self.leaf_size * 0.25;
            let up = high * (0.55 + 0.45 * random.unit()) - self.leaf_size * 0.25;
            let end = [foot[0] + way[0] * out.max(0.0), up.max(0.5), foot[2] + way[1] * out.max(0.0)];
            plant.limb(foot, end, 0.3, PlantPart::Branch);
            plant.clump(end, self.leaf_size, self.leaf_density.clamp(0.05, 1.0), random);
        }
        plant.clump([foot[0], high * 0.5, foot[2]], self.leaf_size, self.leaf_density.clamp(0.05, 1.0), random);
    }

    /// Roots out from the trunk's foot along the ground and a block or two
    /// into it.
    fn roots_out(&self, plant: &mut Growing, random: &mut Random) {
        let roots = self.roots.min(8) as usize;
        let first = (random.next() % 8) as usize;
        for root in 0..roots {
            let way = AROUND[(first + root * 8 / roots.max(1)) % 8];
            let long = self.trunk.max(1.0) * (0.9 + 0.8 * random.unit());
            let end = [0.5 + way[0] * long, -1.0 - random.unit(), 0.5 + way[1] * long];
            plant.limb([0.5, 0.6, 0.5], end, 0.3, PlantPart::Root);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Whether every block of wood is joined to the trunk's foot through
    /// blocks of wood, corner to corner at the least.
    fn wood_is_one_piece(model: &PlantModel) -> bool {
        let wood: HashSet<[i32; 3]> =
            model.blocks.iter().filter(|(_, part)| part.is_wood()).map(|(at, _)| *at).collect();
        let Some(&start) = wood.iter().min_by_key(|at| (at[1].abs(), at[0].abs() + at[2].abs())) else { return true };
        let (mut reached, mut next) = (HashSet::from([start]), vec![start]);
        while let Some(at) = next.pop() {
            for d in 0..27 {
                let beside = [at[0] + d % 3 - 1, at[1] + (d / 3) % 3 - 1, at[2] + d / 9 - 1];
                if wood.contains(&beside) && reached.insert(beside) {
                    next.push(beside);
                }
            }
        }
        reached.len() == wood.len()
    }

    #[test]
    fn a_plant_is_written_and_read_back_and_an_unknown_key_is_refused() {
        let pine =
            PlantDef { kind: PlantKind::Conifer, wood: Some("Blocks/Pine Log.block".into()), ..PlantDef::default() };
        let text = pine.to_text();
        assert!(text.contains("kind = \"conifer\""), "{text}");
        assert_eq!(PlantDef::parse(&text).unwrap(), pine);
        assert_eq!(PlantDef::parse("").unwrap(), PlantDef::default());
        assert!(PlantDef::parse("hieght = [1.0, 2.0]").is_err());
        assert!(!PlantDef::default().to_text().contains("wood"), "a block left out is not written");
    }

    #[test]
    fn a_seed_grows_the_same_plant_again_and_another_seed_another_of_its_kind() {
        for kind in PlantKind::ALL {
            let plant = PlantDef { kind, ..PlantDef::default() };
            let (first, again, other) = (plant.grow(11), plant.grow(11), plant.grow(12));
            assert_eq!(first, again, "{kind:?}");
            assert_ne!(first, other, "{kind:?}");
            assert!(first.count(PlantPart::Leaf) > 20, "{kind:?} has leaves");
            // In a fixed order, bottom up.
            assert!(first.blocks.windows(2).all(|pair| pair[0].0[1] <= pair[1].0[1]));
        }
    }

    #[test]
    fn a_broadleaf_is_a_trunk_as_tall_as_asked_under_a_crown_as_wide_its_wood_in_one_piece() {
        let oak = PlantDef::default();
        for seed in 0..12 {
            let model = oak.grow(seed);
            let (low, high) = model.bounds().unwrap();
            // The trunk stands 9 to 14 blocks; its crown reaches a third of its height over it, and a clump more.
            let trunk_top =
                model.blocks.iter().filter(|(_, p)| *p == PlantPart::Trunk).map(|(at, _)| at[1]).max().unwrap();
            assert!((8..=14).contains(&trunk_top), "seed {seed}: trunk to {trunk_top}");
            assert!(high[1] > trunk_top && high[1] <= trunk_top + 9, "seed {seed}: crown to {}", high[1]);
            let wide = (high[0] - low[0]).max(high[2] - low[2]);
            assert!((6..=17).contains(&wide), "seed {seed}: {wide} wide");
            assert!(low[1] < 0, "seed {seed}: roots in the ground");
            assert!(model.count(PlantPart::Branch) > 10, "seed {seed}: branches");
            assert!(wood_is_one_piece(&model), "seed {seed}: wood in one piece");
            // Leaves only in the crown: none under the trunk's lower half.
            let lowest_leaf =
                model.blocks.iter().filter(|(_, p)| *p == PlantPart::Leaf).map(|(at, _)| at[1]).min().unwrap();
            assert!(lowest_leaf > trunk_top / 4, "seed {seed}: leaves from {lowest_leaf}");
        }
        // Straight with no bend: the trunk's blocks all over its foot.
        let straight = PlantDef { bend: 0.0, trunk: 1.0, ..PlantDef::default() }.grow(3);
        assert!(straight.blocks.iter().filter(|(_, p)| *p == PlantPart::Trunk).all(|(at, _)| at[0] == 0 && at[2] == 0));
    }

    #[test]
    fn a_conifer_narrows_to_its_top_and_a_bush_is_low_and_has_no_trunk() {
        let pine = PlantDef {
            kind: PlantKind::Conifer,
            height: [14.0, 18.0],
            crown_width: [7.0, 9.0],
            crown_height: [11.0, 13.0],
            bend: 0.0,
            ..PlantDef::default()
        };
        for seed in 0..8 {
            let model = pine.grow(seed);
            let (_, high) = model.bounds().unwrap();
            let wide_at = |from: i32, to: i32| {
                let leaves = model.blocks.iter().filter(|(at, p)| *p == PlantPart::Leaf && (from..to).contains(&at[1]));
                leaves.map(|(at, _)| at[0].abs().max(at[2].abs())).max().unwrap_or(0)
            };
            let (lower, upper) = (wide_at(high[1] - 12, high[1] - 8), wide_at(high[1] - 4, high[1] + 1));
            assert!(lower > upper + 1, "seed {seed}: {lower} wide low down, {upper} at the top");
            assert!(wood_is_one_piece(&model), "seed {seed}");
        }
        let bush = PlantDef {
            kind: PlantKind::Bush,
            height: [2.0, 3.0],
            crown_width: [3.0, 5.0],
            crown_height: [2.0, 3.0],
            leaf_size: 3.0,
            ..PlantDef::default()
        };
        for seed in 0..8 {
            let model = bush.grow(seed);
            let (low, high) = model.bounds().unwrap();
            assert_eq!(model.count(PlantPart::Trunk), 0);
            assert!(low[1] >= -1 && high[1] <= 5, "seed {seed}: from {} to {}", low[1], high[1]);
            assert!(model.count(PlantPart::Leaf) > model.count(PlantPart::Branch), "seed {seed}: mostly leaves");
        }
    }
}
