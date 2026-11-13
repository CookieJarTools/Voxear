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
//! branch = 0.375                 # how thick a branch is, in blocks across
//! bend = 0.25                    # how far it leans and wanders, 0 for straight
//! crown_width = [8.0, 12.0]
//! crown_height = [6.0, 9.0]
//! branches = 0.6                 # how thick with branches, 0 to 1
//! leaf_size = 4.0                # how large a clump of leaves is, in blocks
//! leaf_density = 0.8             # how full a clump is, 0 to 1
//! roots = 4
//! vines = 0.3                    # how many of its trees have vines, 0 to 1
//! wood = "Blocks/Oak Log.block"
//! leaves = "Blocks/Oak Leaves.block"
//! vine = "Blocks/Vine.block"
//! sapling = "Blocks/Oak Sapling.block"
//! saplings = [0, 2]              # how many it scatters as it is felled, from and to
//! grow_time = 1200.0             # seconds for a sapling to grow into it
//! stages = 3                     # the sizes it grows through on the way
//! light = 0.5                    # the least light a sapling needs, 0 to 1
//! ```
//!
//! A broadleaf's crown is grown towards points scattered through its shape:
//! each branch end grows a step towards the points nearest it, and a point
//! is taken away once a branch reaches it, so branches fork to fill the
//! crown and never cross it twice. A conifer has a ring of branches every
//! so often up its trunk, each ring shorter than the one below. A bush is
//! stems from the ground with a clump at each end.
//!
//! A tree with vines has one or two climbing its trunk from the ground,
//! and some hanging from under the edge of its crown.
//!
//! Where its trunk is thinner than a block it is a column of logs, each as
//! thick as the trunk is there, so a trunk of whole logs at its foot thins
//! to posts towards its top; and its branches are logs lying along the way
//! they reach, as thick as `branch`, each joined to the next face to face.
//! A plant grown keeps how thick each block of its wood is and which way it
//! lies ([`PlantModel::blocks`]); a world whose wood block is a log
//! (`block::BlockShape::Log`) builds it so, and one whose wood is a cube
//! builds every block of it whole.
//!
//! It is all whole-number hashes, sums, products and square roots, so a
//! seed grows the same plant on every machine.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::shape::{BlockFacing, BlockState, log_size};

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
    /// How thick its branches are, across: a log as thin as that where its
    /// wood is a log.
    pub branch: f32,
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
    /// How many of the plants it grows have vines, from 0 for none to 1 for
    /// every one: up the trunk, and hanging from under the crown.
    #[serde(skip_serializing_if = "is_nothing")]
    pub vines: f32,
    /// The blocks its wood and leaves are, each a reference to a `.block`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaves: Option<String>,
    /// The block its vines are: one whose shape is a vine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vine: Option<String>,
    /// The block its saplings are: one whose shape is a sapling, and which
    /// grows into it (see `sapling`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sapling: Option<String>,
    /// How many saplings it scatters where its crown lands when it is
    /// felled, from and to.
    #[serde(skip_serializing_if = "is_none_scattered")]
    pub saplings: [u32; 2],
    /// How long a sapling takes to grow into it, in seconds of play.
    #[serde(skip_serializing_if = "is_twenty_minutes")]
    pub grow_time: f32,
    /// How many sizes it grows through after the sapling, the last the
    /// whole plant: 1 for at once.
    #[serde(skip_serializing_if = "is_three")]
    pub stages: u32,
    /// The least light a sapling of it needs to grow, from 0 for none to 1
    /// for open sky (see `sapling::light_at`).
    #[serde(skip_serializing_if = "is_half")]
    pub light: f32,
}

fn is_none_scattered(range: &[u32; 2]) -> bool {
    *range == [0, 0]
}

fn is_twenty_minutes(seconds: &f32) -> bool {
    *seconds == 1200.0
}

fn is_three(stages: &u32) -> bool {
    *stages == 3
}

fn is_half(light: &f32) -> bool {
    *light == 0.5
}

fn is_nothing(amount: &f32) -> bool {
    *amount == 0.0
}

impl Default for PlantDef {
    fn default() -> Self {
        Self {
            kind: PlantKind::Broadleaf,
            height: [9.0, 14.0],
            trunk: 1.4,
            taper: 0.6,
            branch: 0.375,
            bend: 0.25,
            crown_width: [8.0, 12.0],
            crown_height: [6.0, 9.0],
            branches: 0.6,
            leaf_size: 4.0,
            leaf_density: 0.8,
            roots: 4,
            vines: 0.0,
            wood: None,
            leaves: None,
            vine: None,
            sapling: None,
            saplings: [0, 0],
            grow_time: 1200.0,
            stages: 3,
            light: 0.5,
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
    Vine,
}

impl PlantPart {
    /// Whether it is wood: the plant's `wood` block rather than its
    /// `leaves` or its `vine`.
    pub fn is_wood(self) -> bool {
        !matches!(self, PlantPart::Leaf | PlantPart::Vine)
    }
}

/// A plant grown: its blocks, each by where it is from the foot of its
/// trunk (x and z along the ground, y up; roots reach below 0), what it is
/// part of, and for wood, how it stands as a log (which way it lies and
/// how thick it is: see `shape::BlockState`), in a fixed order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlantModel {
    pub blocks: Vec<([i32; 3], PlantPart, BlockState)>,
}

impl PlantModel {
    /// The lowest and highest block along each axis; none for no blocks.
    pub fn bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let first = self.blocks.first()?.0;
        Some(self.blocks.iter().fold((first, first), |(low, high), (at, _, _)| {
            ([0, 1, 2].map(|k| low[k].min(at[k])), [0, 1, 2].map(|k| high[k].max(at[k])))
        }))
    }

    pub fn count(&self, part: PlantPart) -> usize {
        self.blocks.iter().filter(|(_, kind, _)| *kind == part).count()
    }

    /// Its blocks with what each is part of, without how they stand.
    pub fn parts(&self) -> impl Iterator<Item = ([i32; 3], PlantPart)> + '_ {
        self.blocks.iter().map(|&(at, part, _)| (at, part))
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

/// The blocks a line from `from` to `to` passes through, in order, each
/// beside the last face to face: what a limb of logs takes.
fn blocks_along(from: [f32; 3], to: [f32; 3]) -> Vec<[i32; 3]> {
    let mut at = from.map(|v| v.floor() as i32);
    let end = to.map(|v| v.floor() as i32);
    let way = [0, 1, 2].map(|k| to[k] - from[k]);
    let step = way.map(|v| if v > 0.0 { 1 } else { -1 });
    // How far along the line, from 0 to 1, each next border is, and how far between borders.
    let apart = way.map(|v| if v.abs() < 1e-6 { f32::MAX } else { 1.0 / v.abs() });
    let mut next = [0, 1, 2].map(|k| {
        if way[k].abs() < 1e-6 {
            f32::MAX
        } else {
            let border = if way[k] > 0.0 { at[k] as f32 + 1.0 } else { at[k] as f32 };
            (border - from[k]) / way[k]
        }
    });
    let mut found = vec![at];
    while at != end && found.len() < 512 {
        let k = (0..3).min_by(|&a, &b| next[a].total_cmp(&next[b])).expect("three axes");
        if next[k] > 1.0 {
            break;
        }
        at[k] += step[k];
        next[k] += apart[k];
        found.push(at);
    }
    found
}

/// The way along which `way` reaches furthest, as a log lies.
fn lying(way: [f32; 3]) -> BlockFacing {
    let [x, y, z] = way.map(f32::abs);
    if y >= x && y >= z {
        BlockFacing::Up
    } else if x >= z {
        BlockFacing::East
    } else {
        BlockFacing::North
    }
}

/// A plant as it grows: its blocks so far, and how each stands.
struct Growing {
    blocks: HashMap<[i32; 3], (PlantPart, BlockState)>,
}

impl Growing {
    /// A whole block of `part`.
    fn put(&mut self, at: [i32; 3], part: PlantPart) {
        let whole = BlockState { facing: BlockFacing::Up, ..BlockState::default() };
        self.put_as(at, part, whole);
    }

    /// A block of `part` standing as `state`. Wood takes the place of
    /// leaves and vines; where two of wood meet the thicker stays.
    fn put_as(&mut self, at: [i32; 3], part: PlantPart, state: BlockState) {
        let kept = self.blocks.entry(at).or_insert((part, state));
        match (part.is_wood(), kept.0.is_wood()) {
            (true, false) => *kept = (part, state),
            (true, true) => {
                kept.0 = kept.0.min(part);
                if state.across() > kept.1.across() {
                    kept.1 = state;
                }
            }
            (false, false) if part < kept.0 => *kept = (part, state),
            _ => {}
        }
    }

    /// A limb of logs `across` sixteenths thick from `from` to `to`, each
    /// lying along the way it reaches and beside the last face to face.
    fn logs(&mut self, from: [f32; 3], to: [f32; 3], across: f32, part: PlantPart) {
        let facing = lying([0, 1, 2].map(|k| to[k] - from[k]));
        let state = BlockState { facing, size: log_size(across), ..BlockState::default() };
        for at in blocks_along(from, to) {
            self.put_as(at, part, state);
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

    /// A trunk's logs from `from` up to `to`, `across` sixteenths thick,
    /// each standing up.
    fn logs_up(&mut self, from: [f32; 3], to: [f32; 3], across: f32) {
        let state = BlockState { facing: BlockFacing::Up, size: log_size(across), ..BlockState::default() };
        for at in blocks_along(from, to) {
            self.put_as(at, PlantPart::Trunk, state);
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
        // Drawn whether it has vines or not, so the rest of a plant does not depend on them.
        let mut vines = Random::new(seed ^ 0x5EED_71AE);
        if vines.unit() < self.vines.clamp(0.0, 1.0) {
            self.vines_on(&mut plant, &mut vines);
        }
        let mut blocks: Vec<([i32; 3], PlantPart, BlockState)> =
            plant.blocks.into_iter().map(|(at, (part, state))| (at, part, state)).collect();
        blocks.sort_by_key(|(at, _, _)| (at[1], at[2], at[0]));
        PlantModel { blocks }
    }

    /// Half how thick the trunk is `share` of the way up it.
    fn trunk_radius(&self, share: f32) -> f32 {
        self.trunk.max(0.25) * 0.5 * (1.0 - self.taper.clamp(0.0, 1.0) * share)
    }

    /// How thick its branches are, in sixteenths across.
    fn branch_across(&self) -> f32 {
        (self.branch * 16.0).clamp(4.0, 16.0)
    }

    /// The trunk, from the ground up `height` blocks, leaning and wandering
    /// by its bend; where its top is.
    fn trunk_up(&self, plant: &mut Growing, random: &mut Random, height: f32) -> [f32; 3] {
        let bend = self.bend.clamp(0.0, 1.0);
        let lean = [random.signed() * bend * 0.35, random.signed() * bend * 0.35];
        let mut at = [0.5, 0.0, 0.5];
        let mut last = at;
        let steps = (height * 2.0).ceil() as u32;
        for step in 0..=steps {
            let share = step as f32 / steps as f32;
            let radius = self.trunk_radius(share);
            match radius * 32.0 >= 15.0 {
                // A block or more across: whole blocks round the middle.
                true => plant.ball(at, radius.max(0.3), PlantPart::Trunk),
                // Thinner: a column of logs, joined to the block below it face to face where it wanders.
                false => plant.logs_up(last, at, (radius * 32.0).max(4.0)),
            }
            last = at;
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
                plant.logs(ends[from].0, next, self.branch_across(), PlantPart::Branch);
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
                plant.logs(trunk, end, self.branch_across(), PlantPart::Branch);
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
            plant.logs(foot, end, self.branch_across(), PlantPart::Branch);
            plant.clump(end, self.leaf_size, self.leaf_density.clamp(0.05, 1.0), random);
        }
        plant.clump([foot[0], high * 0.5, foot[2]], self.leaf_size, self.leaf_density.clamp(0.05, 1.0), random);
    }

    /// Vines on a plant grown: one or two up its trunk from the ground, each
    /// beside the trunk's blocks on one side as far up as it goes, and some
    /// hanging from under the leaves at the edge of its crown, a few blocks
    /// long. Only where no block of the plant is.
    fn vines_on(&self, plant: &mut Growing, random: &mut Random) {
        const SIDES: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];
        let mut trunk: Vec<[i32; 3]> =
            plant.blocks.iter().filter(|(_, (part, _))| *part == PlantPart::Trunk).map(|(at, _)| *at).collect();
        // In a fixed order: the blocks are kept in no order of their own, and of two as far out the last is taken.
        trunk.sort_unstable_by_key(|at| (at[1], at[2], at[0]));
        let top = trunk.iter().map(|at| at[1]).max().unwrap_or(0);
        let mut added: Vec<[i32; 3]> = Vec::new();
        if top >= 3 {
            let first = (random.next() % 4) as usize;
            for climber in 0..1 + (random.next() % 2) as usize {
                let [dx, dz] = SIDES[(first + climber * (1 + (random.next() % 2) as usize)) % 4];
                let high = (top as f32 * (0.45 + 0.5 * random.unit())) as i32;
                for y in 0..=high {
                    // Beside the trunk's block furthest that way at this height.
                    let outer = trunk.iter().filter(|at| at[1] == y).max_by_key(|at| at[0] * dx + at[2] * dz);
                    let Some(outer) = outer else { continue };
                    added.push([outer[0] + dx, y, outer[2] + dz]);
                }
            }
        }
        // Under the crown: a leaf with open air below it, the further from the trunk the likelier.
        let mut leaves: Vec<[i32; 3]> =
            plant.blocks.iter().filter(|(_, (part, _))| *part == PlantPart::Leaf).map(|(at, _)| *at).collect();
        leaves.sort_unstable_by_key(|at| (at[1], at[2], at[0]));
        for [x, y, z] in leaves {
            let draw = (random.unit(), random.unit());
            let out = ((x * x + z * z) as f32).sqrt();
            if plant.blocks.contains_key(&[x, y - 1, z]) || out < 2.0 || draw.0 > 0.16 {
                continue;
            }
            let long = 1 + (draw.1 * (y as f32 * 0.6).clamp(1.0, 6.0)) as i32;
            added.extend((1..=long).map(|down| [x, y - down, z]).take_while(|at| at[1] >= 1));
        }
        for at in added {
            if !plant.blocks.contains_key(&at) {
                plant.blocks.insert(at, (PlantPart::Vine, BlockState::default()));
            }
        }
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
            model.parts().filter(|(_, part)| part.is_wood()).map(|(at, _)| at).collect();
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
                model.parts().filter(|(_, p)| *p == PlantPart::Trunk).map(|(at, _)| at[1]).max().unwrap();
            assert!((8..=14).contains(&trunk_top), "seed {seed}: trunk to {trunk_top}");
            assert!(high[1] > trunk_top && high[1] <= trunk_top + 9, "seed {seed}: crown to {}", high[1]);
            let wide = (high[0] - low[0]).max(high[2] - low[2]);
            assert!((6..=17).contains(&wide), "seed {seed}: {wide} wide");
            assert!(low[1] < 0, "seed {seed}: roots in the ground");
            assert!(model.count(PlantPart::Branch) > 10, "seed {seed}: branches");
            assert!(wood_is_one_piece(&model), "seed {seed}: wood in one piece");
            // Leaves only in the crown: none under the trunk's lower half.
            let lowest_leaf =
                model.parts().filter(|(_, p)| *p == PlantPart::Leaf).map(|(at, _)| at[1]).min().unwrap();
            assert!(lowest_leaf > trunk_top / 4, "seed {seed}: leaves from {lowest_leaf}");
        }
        // Straight with no bend: the trunk's blocks all over its foot.
        let straight = PlantDef { bend: 0.0, trunk: 1.0, ..PlantDef::default() }.grow(3);
        assert!(straight.parts().filter(|(_, p)| *p == PlantPart::Trunk).all(|(at, _)| at[0] == 0 && at[2] == 0));
    }

    /// Each block of wood, how thick it is across and which way it lies.
    fn wood(model: &PlantModel) -> HashMap<[i32; 3], (u8, BlockFacing)> {
        model.blocks.iter().filter(|(_, part, _)| part.is_wood()).map(|(at, _, s)| (*at, (s.across(), s.facing))).collect()
    }

    #[test]
    fn a_trunk_is_whole_logs_at_its_foot_and_thins_as_it_rises_and_its_branches_are_thin_logs_lying_as_they_reach() {
        let oak = PlantDef { taper: 0.7, ..PlantDef::default() };
        for seed in 0..12 {
            let model = oak.grow(seed);
            let wood = wood(&model);
            // Up the trunk, the thickest log at each height: whole at the foot, never thicker higher up, thin at the top.
            let thickest = |y: i32| {
                model.blocks.iter().filter(|(at, p, _)| *p == PlantPart::Trunk && at[1] == y).map(|(_, _, s)| s.across()).max()
            };
            let rising: Vec<u8> = (0..).map_while(thickest).collect();
            assert_eq!(rising[0], 16, "seed {seed}: {rising:?}");
            assert!(rising.windows(2).all(|pair| pair[1] <= pair[0]), "seed {seed}: {rising:?}");
            assert!(*rising.last().unwrap() <= 10, "seed {seed}: {rising:?}");
            // Every log thinner than a block stands up in the trunk, or lies along its branch: as thin as a branch is.
            let branches: Vec<_> = model.blocks.iter().filter(|(_, p, _)| *p == PlantPart::Branch).collect();
            assert!(branches.iter().all(|(_, _, s)| s.across() == 6 || s.across() == 16), "seed {seed}");
            let lying = branches.iter().filter(|(_, _, s)| s.facing != BlockFacing::Up).count();
            assert!(lying * 3 > branches.len(), "seed {seed}: {lying} of {} branch logs lie down", branches.len());
            // Every block of wood beside another face to face: no limb steps from corner to corner.
            for &at in wood.keys().filter(|at| at[1] > 0) {
                let beside = BlockFacing::ALL.iter().any(|way| {
                    let s = way.step();
                    wood.contains_key(&[at[0] + s[0], at[1] + s[1], at[2] + s[2]])
                });
                assert!(beside, "seed {seed}: the log at {at:?} touches no other face to face");
            }
            assert!(wood_is_one_piece(&model), "seed {seed}");
        }
        // Thicker branches when asked; a trunk that does not taper stays whole all the way up.
        let thick = PlantDef { branch: 0.75, ..oak.clone() }.grow(4);
        assert!(thick.blocks.iter().filter(|(_, p, _)| *p == PlantPart::Branch).any(|(_, _, s)| s.across() == 12));
        let even = PlantDef { taper: 0.0, ..oak.clone() }.grow(4);
        assert!(even.blocks.iter().filter(|(_, p, _)| *p == PlantPart::Trunk).all(|(_, _, s)| s.across() == 16));
        assert_eq!(PlantDef::parse("branch = 0.5").unwrap().branch, 0.5);
    }

    #[test]
    fn blocks_along_a_line_are_each_beside_the_last_face_to_face() {
        let found = blocks_along([0.5, 0.5, 0.5], [3.7, 2.2, -1.4]);
        assert_eq!(found.first(), Some(&[0, 0, 0]));
        assert_eq!(found.last(), Some(&[3, 2, -2]));
        assert!(found.windows(2).all(|pair| (0..3).map(|k| (pair[1][k] - pair[0][k]).abs()).sum::<i32>() == 1));
        assert_eq!(lying([0.2, -3.0, 1.0]), BlockFacing::Up);
        assert_eq!(lying([-2.0, 1.0, 1.0]), BlockFacing::East);
        assert_eq!(lying([0.5, 0.2, 1.0]), BlockFacing::North);
    }

    #[test]
    fn a_tree_with_vines_has_them_up_its_trunk_and_under_its_crown_and_is_otherwise_the_same_tree() {
        let bare = PlantDef::default();
        let vined = PlantDef { vines: 1.0, vine: Some("Blocks/Vine.block".into()), ..PlantDef::default() };
        assert!(vined.to_text().contains("vines = 1.0") && !bare.to_text().contains("vine"));
        assert_eq!(PlantDef::parse(&vined.to_text()).unwrap(), vined);
        let mut with_some = 0;
        for seed in 0..12 {
            let (plain, model) = (bare.grow(seed), vined.grow(seed));
            // The same tree, with vines besides.
            let rest: Vec<_> = model.blocks.iter().copied().filter(|(_, part, _)| *part != PlantPart::Vine).collect();
            assert_eq!(rest, plain.blocks, "seed {seed}");
            assert_eq!(model, vined.grow(seed));
            let blocks: std::collections::HashMap<[i32; 3], PlantPart> = model.parts().collect();
            let vines: Vec<[i32; 3]> = model.parts().filter(|(_, part)| *part == PlantPart::Vine).map(|(at, _)| at).collect();
            assert!(vines.len() >= 3, "seed {seed}: {} blocks of vine", vines.len());
            for &[x, y, z] in &vines {
                // Beside the trunk, or hanging from a leaf through vines alone; and never under the ground.
                let on_trunk = [[1, 0], [-1, 0], [0, 1], [0, -1]]
                    .iter()
                    .any(|d| blocks.get(&[x + d[0], y, z + d[1]]) == Some(&PlantPart::Trunk));
                let over = (1..=8).map(|up| blocks.get(&[x, y + up, z])).find(|part| *part != Some(&PlantPart::Vine));
                assert!(on_trunk || over == Some(Some(&PlantPart::Leaf)), "seed {seed}: a vine at {:?}", [x, y, z]);
                assert!(y >= 0, "seed {seed}");
            }
            with_some += usize::from(vines.iter().any(|at| at[1] > 4));
        }
        assert!(with_some >= 8, "most have vines up in the crown: {with_some}");
        // Half the trees of a wood with vines on half of them have some.
        let half = PlantDef { vines: 0.5, ..vined.clone() };
        let grown = (0..200).filter(|&seed| half.grow(seed).count(PlantPart::Vine) > 0).count();
        assert!((70..130).contains(&grown), "{grown} of 200");
        assert!(!PlantPart::Vine.is_wood());
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
                let leaves = model.parts().filter(|(at, p)| *p == PlantPart::Leaf && (from..to).contains(&at[1]));
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
