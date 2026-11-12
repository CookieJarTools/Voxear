//! The Chasms node: the ground wired into it cut open by rifts and
//! sinkholes, sheer-walled and deep, with ground at their floors as open
//! to the sky as any other.
//!
//! Kind says what is cut: Rifts, long winding cracks that run for
//! kilometres and taper out at their ends; Sinkholes, round pits with
//! ragged rims; or both. Size is how far a rift runs before it bends;
//! sinkholes' places are a quarter of that apart. Width From and Width To are how
//! wide one is, Depth From and Depth To how far down its floor lies under
//! the ground it cuts, and How Often how much of the land has one. Wall is
//! how steep the walls stand; Ledges cuts them into shelves every so many
//! metres down; Floor rolls the floor in low hills as many metres high.
//!
//! A chasm's floor is ground like any other: the generator's height is
//! there, so a world's surfaces, biomes, plants and structures take it as
//! they take the land over it. Beside the height it gives Floor, 1 on the
//! floors; Walls, 1 on the walls and their ledges; Sunk, how far under the
//! ground it cut the ground lies, in metres, which a biome can ask for; and
//! Rim, the ground as it was before it was cut, for a Roof to cover a chasm
//! over at.
//!
//! A mask wired into Mask keeps the chasms to where it is over a half.

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, STEEPEST, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::gradient,
};

/// What a Chasms node cuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChasmKind {
    #[default]
    Rifts,
    Sinkholes,
    Both,
}

impl ChasmKind {
    pub const ALL: [ChasmKind; 3] = [ChasmKind::Rifts, ChasmKind::Sinkholes, ChasmKind::Both];
}

/// Rifts or sinkholes `width_from` to `width_to` metres wide with floors
/// `depth_from` to `depth_to` metres down, `size` metres to a bend or
/// apart, over as much of the land as `often` says. `wall` is an angle kept
/// as rise over run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chasms {
    pub shape: ChasmKind,
    pub size: f32,
    pub width_from: f32,
    pub width_to: f32,
    pub depth_from: f32,
    pub depth_to: f32,
    pub often: f32,
    pub wall: f32,
    pub ledges: f32,
    pub floor: f32,
    pub seed: u32,
}

impl Default for Chasms {
    fn default() -> Self {
        Self {
            shape: ChasmKind::Rifts,
            size: 900.0,
            width_from: 30.0,
            width_to: 120.0,
            depth_from: 80.0,
            depth_to: 220.0,
            often: 0.4,
            // About 88 degrees.
            wall: 28.0,
            ledges: 0.0,
            floor: 6.0,
            seed: 0,
        }
    }
}

impl Chasms {
    /// Its inputs, and its outputs.
    pub const GROUND: usize = 0;
    pub const MASK: usize = 1;
    pub const HEIGHT: usize = 0;
    pub const FLOOR: usize = 1;
    pub const WALLS: usize = 2;
    pub const SUNK: usize = 3;
    pub const RIM: usize = 4;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("shape", "Kind", &["Rifts", "Sinkholes", "Both"]),
    ValueSpec::number("size", "Size", 10.0, f32::MAX, "m"),
    ValueSpec::number("width_from", "Width From", 2.0, 10000.0, "m"),
    ValueSpec::number("width_to", "Width To", 2.0, 10000.0, "m"),
    ValueSpec::number("depth_from", "Depth From", 1.0, 10000.0, "m"),
    ValueSpec::number("depth_to", "Depth To", 1.0, 10000.0, "m"),
    ValueSpec::number("often", "How Often", 0.0, 1.0, ""),
    ValueSpec::angle("wall", "Wall"),
    ValueSpec::number("ledges", "Ledges", 0.0, 1000.0, "m"),
    ValueSpec::number("floor", "Floor", 0.0, 1000.0, "m"),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::field("ground", "Ground"), Port::field("mask", "Mask")];

const OUTPUTS: &[Port] = &[
    Port::field("height", "Height"),
    Port::field("floor", "Floor"),
    Port::field("walls", "Walls"),
    Port::field("sunk", "Sunk"),
    Port::field("rim", "Rim"),
];

impl Kind for Chasms {
    const TITLE: &'static str = "Chasms";
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
            0 => Value::Choice(ChasmKind::ALL.iter().position(|&kind| kind == self.shape).unwrap_or(0)),
            1 => Value::Number(self.size),
            2 => Value::Number(self.width_from),
            3 => Value::Number(self.width_to),
            4 => Value::Number(self.depth_from),
            5 => Value::Number(self.depth_to),
            6 => Value::Number(self.often),
            7 => Value::Number(self.wall),
            8 => Value::Number(self.ledges),
            9 => Value::Number(self.floor),
            10 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.shape = ChasmKind::ALL[choice.min(ChasmKind::ALL.len() - 1)],
            (1, Value::Number(number)) => self.size = number,
            (2, Value::Number(number)) => self.width_from = number,
            (3, Value::Number(number)) => self.width_to = number,
            (4, Value::Number(number)) => self.depth_from = number,
            (5, Value::Number(number)) => self.depth_to = number,
            (6, Value::Number(number)) => self.often = number,
            (7, Value::Number(number)) => self.wall = number,
            (8, Value::Number(number)) => self.ledges = number,
            (9, Value::Number(number)) => self.floor = number,
            (10, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let ground = cx.input(node, Self::GROUND, at).unwrap_or_else(|| cx.constant(0.0));
        if port == Self::RIM {
            return ground;
        }
        let mask = cx.input(node, Self::MASK, at).unwrap_or_else(|| cx.constant(1.0));
        let scale = cx.world.scale.max(0.01);
        let cut = Cut::new(self, scale, seed_for(cx, node, self.seed));
        // How far down the ground is cut at each place, and how far down its floor lies there.
        let sunk = cx.run(Sunk { cut, what: 0 }, &[at.x, at.z, mask]);
        let deepest = cx.run(Sunk { cut, what: 1 }, &[at.x, at.z, mask]);
        match port {
            Self::HEIGHT => cx.run(Lowered, &[ground, sunk]),
            Self::FLOOR => cx.run(Part { floor: true }, &[sunk, deepest]),
            Self::WALLS => cx.run(Part { floor: false }, &[sunk, deepest]),
            _ => sunk,
        }
    }
}

/// What a Chasms node's settings make of the ground, held to their ranges.
#[derive(Clone, Copy, Debug)]
struct Cut {
    kind: ChasmKind,
    size: f32,
    width: [f32; 2],
    depth: [f32; 2],
    often: f32,
    wall: f32,
    ledges: f32,
    floor: f32,
    seed: u32,
}

impl Cut {
    fn new(set: &Chasms, scale: f32, seed: u32) -> Self {
        let width_from = set.width_from.clamp(2.0, 10000.0) * scale;
        let depth_from = set.depth_from.clamp(1.0, 10000.0);
        Self {
            kind: set.shape,
            size: (set.size * scale).max(10.0),
            width: [width_from, (set.width_to * scale).clamp(width_from, 10000.0 * scale)],
            depth: [depth_from, set.depth_to.clamp(depth_from, 10000.0)],
            often: if set.often.is_nan() { 0.0 } else { set.often.clamp(0.0, 1.0) },
            wall: set.wall.clamp(0.2, STEEPEST),
            ledges: set.ledges.clamp(0.0, 1000.0),
            floor: set.floor.clamp(0.0, 1000.0),
            seed,
        }
    }

    fn key(&self, key: &mut Key) {
        key.whole(self.kind as u32).number(self.size).number(self.width[0]).number(self.width[1]);
        key.number(self.depth[0]).number(self.depth[1]).number(self.often).number(self.wall);
        key.number(self.ledges).number(self.floor).whole(self.seed);
    }

    /// How far inside a rift's rim the place is, in metres along the
    /// ground: less than nothing outside one.
    fn rift(&self, x: f32, z: f32) -> f32 {
        let size = self.size;
        // The rift runs where this is nothing; how far off it is comes from how fast it changes.
        let fine = size * 0.23;
        let read = |x: f32, z: f32| {
            gradient(x / size, z / size, self.seed.wrapping_add(0x21))
                + 0.25 * gradient(x / fine, z / fine, self.seed.wrapping_add(0x22))
        };
        let here = read(x, z);
        let (dx, dz) = ((read(x + 2.0, z) - here) / 2.0, (read(x, z + 2.0) - here) / 2.0);
        let climb = (dx * dx + dz * dz).sqrt().max(0.2 / size);
        let away = if here < 0.0 { -here } else { here } / climb;
        // Rifts run in stretches, tapering out where a broad pattern says they end.
        let stretch = gradient(x / (size * 1.9), z / (size * 1.9), self.seed.wrapping_add(0x23));
        let open = (stretch + 1.0) * 0.5 - (1.0 - self.often);
        let taper = smooth(open / 0.12);
        if taper <= 0.0 {
            return -away;
        }
        let wide = 0.5 + 0.5 * gradient(x / (size * 0.45), z / (size * 0.45), self.seed.wrapping_add(0x24));
        let half = (self.width[0] + (self.width[1] - self.width[0]) * wide.clamp(0.0, 1.0)) * 0.5 * taper;
        // Its rim ragged a little.
        let ragged = half * 0.12 * gradient(x / 9.0, z / 9.0, self.seed.wrapping_add(0x25));
        half + ragged - away
    }

    /// How far inside a sinkhole's rim the place is: less than nothing
    /// outside one.
    fn sinkhole(&self, x: f32, z: f32) -> f32 {
        let size = (self.size * 0.25).max(self.width[1] * 2.5);
        let cell = [(x / size).floor() as i32, (z / size).floor() as i32];
        let mut best = f32::MIN;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = [cell[0] + dx, cell[1] + dz];
                let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
                if pick(0) >= self.often {
                    continue;
                }
                let radius = (self.width[0] + (self.width[1] - self.width[0]) * pick(1)) * 0.5;
                let free = (size - radius * 2.2).max(0.0);
                let at = [
                    (cell[0] as f32 + 0.5) * size + (pick(2) - 0.5) * free,
                    (cell[1] as f32 + 0.5) * size + (pick(3) - 0.5) * free,
                ];
                let (ox, oz) = (x - at[0], z - at[1]);
                let away = (ox * ox + oz * oz).sqrt();
                let lobe = radius * 0.6;
                let seed = mix(self.seed, [cell[0], cell[1], 4]);
                let rim = radius * (1.0 + 0.18 * gradient(x / lobe, z / lobe, seed));
                best = best.max(rim - away);
            }
        }
        best
    }

    /// How far down its floor lies under the ground it cuts at the place.
    fn depth_at(&self, x: f32, z: f32) -> f32 {
        let deep = 0.5 + 0.5 * gradient(x / (self.size * 0.6), z / (self.size * 0.6), self.seed.wrapping_add(0x26));
        let depth = self.depth[0] + (self.depth[1] - self.depth[0]) * deep.clamp(0.0, 1.0);
        // Its floor rolls in low hills.
        let hills = self.floor * (0.5 + 0.5 * gradient(x / 60.0, z / 60.0, self.seed.wrapping_add(0x27)));
        (depth - hills).max(1.0)
    }

    /// How far down the wall has come `inside` metres in from the rim: as
    /// steep as the wall stands, in shelves where it has ledges.
    fn wall_at(&self, inside: f32) -> f32 {
        if inside <= 0.0 {
            return 0.0;
        }
        if self.ledges <= 0.0 {
            return inside * self.wall;
        }
        // A drop of a ledge's height, then a shelf two fifths as wide as that is high.
        let drop = self.ledges / self.wall;
        let shelf = self.ledges * 0.4;
        let period = drop + shelf;
        let steps = (inside / period).floor();
        let into = inside - steps * period;
        steps * self.ledges + (into * self.wall).min(self.ledges)
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// From rows of x, z and the mask: with `what` 0 how far down the ground is
/// cut at each place, in metres, and with 1 how far down the floor lies
/// there, whether the place is on it or not.
#[derive(Debug)]
struct Sunk {
    cut: Cut,
    what: u32,
}

impl Kernel for Sunk {
    fn key(&self, key: &mut Key) {
        key.name("chasms").whole(self.what);
        self.cut.key(key);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let cut = &self.cut;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z, mask) = (inputs[0][at], inputs[1][at], inputs[2][at]);
            let kept = smooth((mask - 0.3) / 0.4);
            if kept <= 0.0 || cut.often <= 0.0 {
                *out = 0.0;
                continue;
            }
            let depth = cut.depth_at(x, z) * kept;
            if self.what == 1 {
                *out = depth;
                continue;
            }
            let inside = match cut.kind {
                ChasmKind::Rifts => cut.rift(x, z),
                ChasmKind::Sinkholes => cut.sinkhole(x, z),
                ChasmKind::Both => cut.rift(x, z).max(cut.sinkhole(x, z)),
            };
            *out = cut.wall_at(inside).min(depth);
        }
    }
}

/// The ground, from rows of it and how far down it is cut.
#[derive(Debug)]
struct Lowered;

impl Kernel for Lowered {
    fn key(&self, key: &mut Key) {
        key.name("chasms lowered");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            *out = inputs[0][at] - inputs[1][at];
        }
    }
}

/// With `floor` 1 on a chasm's floor, and without it 1 on its walls, from
/// rows of how far down the ground is cut and how far down the floor lies.
#[derive(Debug)]
struct Part {
    floor: bool,
}

impl Kernel for Part {
    fn key(&self, key: &mut Key) {
        key.name("chasm part").whole(u32::from(self.floor));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let (sunk, deepest) = (inputs[0][at], inputs[1][at]);
            let on_floor = sunk > 0.5 && sunk >= deepest - 0.5;
            *out = match self.floor {
                true => f32::from(u8::from(on_floor)),
                false => f32::from(u8::from(sunk > 0.5 && !on_floor)),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Constant, testing},
    };

    fn world(set: Chasms, port: usize) -> crate::generate::WorldGen {
        let ground = Some(NodeKind::Constant(Constant { value: 100.0 }));
        testing::world_of(NodeKind::Chasms(set), port, &[ground])
    }

    fn square(world: &crate::generate::WorldGen, low: [f32; 2], side: usize, step: f32) -> Vec<f32> {
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| (low[0] + (at % side) as f32 * step, low[1] + (at / side) as f32 * step)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        heights
    }

    #[test]
    fn rifts_and_sinkholes_cut_deep_sheer_sided_chasms_into_the_ground() {
        for kind in ChasmKind::ALL {
            let set = Chasms { shape: kind, often: 0.6, ..Chasms::default() };
            let world = world(set.clone(), Chasms::HEIGHT);
            let (low, high) = testing::checked(&world);
            assert!(high <= 100.0 + 0.01 && low < 100.0 - 60.0, "{kind:?}: ground from {low} to {high}");
            let heights = square(&world, [-1500.0, -1500.0], 300, 10.0);
            let cut = heights.iter().filter(|&&height| height < 99.0).count() as f32 / heights.len() as f32;
            assert!(cut > 0.02 && cut < 0.4, "{kind:?}: {cut} of the land cut");
            // Sheer: few places part way down, against those at the floor.
            let floor = world_floor(&set);
            let floors = floor.iter().filter(|&&on| on > 0.5).count();
            let walls = world_part(&set, Chasms::WALLS).iter().filter(|&&on| on > 0.5).count();
            assert!(floors > walls, "{kind:?}: {floors} on floors, {walls} on walls");
        }
    }

    fn world_part(set: &Chasms, port: usize) -> Vec<f32> {
        square(&world(set.clone(), port), [-1500.0, -1500.0], 300, 10.0)
    }

    fn world_floor(set: &Chasms) -> Vec<f32> {
        world_part(set, Chasms::FLOOR)
    }

    #[test]
    fn sunk_is_how_far_under_the_rim_the_ground_lies() {
        let set = Chasms { shape: ChasmKind::Both, often: 0.6, ledges: 20.0, ..Chasms::default() };
        let heights = world_part(&set, Chasms::HEIGHT);
        let sunk = world_part(&set, Chasms::SUNK);
        let rim = world_part(&set, Chasms::RIM);
        for at in 0..heights.len() {
            assert!((rim[at] - sunk[at] - heights[at]).abs() < 0.01, "{} - {} != {}", rim[at], sunk[at], heights[at]);
            assert!(sunk[at] >= 0.0 && sunk[at] <= 220.0);
        }
        // Ledges: some places on the walls at a ledge's depth, a shelf.
        let shelves = sunk.iter().filter(|&&sunk| sunk > 1.0 && (sunk % 20.0 < 0.01 || sunk % 20.0 > 19.99)).count();
        assert!(shelves > 20, "{shelves} places on shelves");
    }

    #[test]
    fn a_mask_keeps_chasms_to_where_it_is_over_a_half() {
        let ground = Some(NodeKind::Constant(Constant { value: 100.0 }));
        let none = Some(NodeKind::Constant(Constant { value: 0.0 }));
        let set = Chasms { often: 0.8, ..Chasms::default() };
        let world = testing::world_of(NodeKind::Chasms(set), Chasms::HEIGHT, &[ground, none]);
        let heights = square(&world, [-1500.0, -1500.0], 100, 30.0);
        assert!(heights.iter().all(|&height| height == 100.0));
    }
}
