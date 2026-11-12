//! The Raised Land node: land standing high over the land below. Islands
//! of ground, flat on top and tapering underneath, each over its own patch
//! of the ground and clear of everything under it.
//!
//! Size From and Size To are how far across an island may be, Height From
//! and Height To how far over the ground under it its top stands, and How
//! Often how many of the places one could be have one. Thickness is how
//! deep an island is at its middle. Joined says whether an island floats
//! or stands on a stalk of rock from the ground under its middle.
//!
//! An island's top is ground like any other: it takes its column's layers,
//! counted down from the open air over it.
//!
//! The ground is shared out in regions a little wider than the widest
//! island, and a region has one island or none, by the seed, so the same
//! island comes from any chunk. The node says, a column at a time, between
//! which heights an island stands, so a world of blocks asks about no more
//! air than that, and a far view shows the islands.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Solid, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Cells, Compiler, Kernel, Key, Places, V},
        eval::{Givens, Program},
        graph::NodeId,
    },
    noise::gradient,
};

/// Whether an island floats or stands on a stalk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Joined {
    #[default]
    Floating,
    Stalk,
}

impl Joined {
    pub const ALL: [Joined; 2] = [Joined::Floating, Joined::Stalk];
}

/// Islands from `size_from` to `size_to` metres across, their tops from
/// `height_from` to `height_to` metres over the ground under them,
/// `thickness` deep at the middle, at as many of the places one could be
/// as `often` says, 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RaisedLand {
    pub size_from: f32,
    pub size_to: f32,
    pub height_from: f32,
    pub height_to: f32,
    pub often: f32,
    pub thickness: f32,
    pub joined: Joined,
    pub seed: u32,
}

impl Default for RaisedLand {
    fn default() -> Self {
        Self {
            size_from: 60.0,
            size_to: 220.0,
            height_from: 90.0,
            height_to: 220.0,
            often: 0.4,
            thickness: 45.0,
            joined: Joined::Floating,
            seed: 0,
        }
    }
}

impl RaisedLand {
    /// Its ground, among its inputs.
    pub const GROUND: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size_from", "Size From", 8.0, 2000.0, "m"),
    ValueSpec::number("size_to", "Size To", 8.0, 2000.0, "m"),
    ValueSpec::number("height_from", "Height From", 0.0, 4000.0, "m"),
    ValueSpec::number("height_to", "Height To", 0.0, 4000.0, "m"),
    ValueSpec::number("often", "How Often", 0.0, 1.0, ""),
    ValueSpec::number("thickness", "Thickness", 3.0, 1000.0, "m"),
    ValueSpec::choice("joined", "Joined", &["Floating", "Stalk"]),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for RaisedLand {
    const TITLE: &'static str = "Raised Land";
    const FOLDER: &'static str = "Voxels";

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
            0 => Value::Number(self.size_from),
            1 => Value::Number(self.size_to),
            2 => Value::Number(self.height_from),
            3 => Value::Number(self.height_to),
            4 => Value::Number(self.often),
            5 => Value::Number(self.thickness),
            6 => Value::Choice(Joined::ALL.iter().position(|&joined| joined == self.joined).unwrap_or(0)),
            7 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size_from = number,
            (1, Value::Number(number)) => self.size_to = number,
            (2, Value::Number(number)) => self.height_from = number,
            (3, Value::Number(number)) => self.height_to = number,
            (4, Value::Number(number)) => self.often = number,
            (5, Value::Number(number)) => self.thickness = number,
            (6, Value::Choice(choice)) => self.joined = Joined::ALL[choice.min(Joined::ALL.len() - 1)],
            (7, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        if self.often <= 0.0 || self.often.is_nan() {
            return ground;
        }
        let finder = Arc::new(Finder::new(self, seed_for(cx, node, self.seed), cx.own_ground()));
        let end = |cx: &mut Compiler, top: bool| {
            let finder = finder.clone();
            cx.column(at, |cx, places| cx.run(Over { finder, top }, &[places.x, places.z]))
        };
        let (foot, top) = (end(cx, false), end(cx, true));
        cx.note_span(at, foot, top, true);
        let row = cx.run(Raised { finder }, &[ground, at.x, at.y, at.z, foot, top]);
        cx.set_rise(row, cx.rise(ground));
        cx.set_depth(row, cx.depth(ground));
        row
    }
}

/// One island: where its middle is, half how far across it is, how high
/// its top, how high the ground under its middle, how deep it is, whether
/// it stands on a stalk, and the seed of its own shape.
#[derive(Clone, Copy, Debug)]
struct Island {
    at: [f32; 2],
    radius: f32,
    top: f32,
    ground: f32,
    thick: f32,
    stalk: bool,
    seed: u32,
}

impl Island {
    /// Half how wide its stalk is.
    fn stalk(&self) -> f32 {
        self.radius * 0.1 + 3.0
    }

    /// Where the island is over the column at `x` and `z`: its underside
    /// and its top there, in metres up, how far inside its rim the column
    /// is, and how far inside its stalk (less than nothing where it has
    /// none there). None where neither reaches.
    fn over(&self, x: f32, z: f32) -> Option<[f32; 4]> {
        let (dx, dz) = (x - self.at[0], z - self.at[1]);
        let reach = self.radius * 1.05;
        if dx > reach || dx < -reach || dz > reach || dz < -reach {
            return None;
        }
        let away = (dx * dx + dz * dz).sqrt();
        // Its rim wanders in and out, its top rolls a little, and its underside is rough.
        let lobes = self.radius * 0.7;
        let rim = self.radius * (0.8 + 0.2 * gradient(x / lobes, z / lobes, self.seed));
        if away >= rim {
            return None;
        }
        let out = away / rim;
        let top = self.top + 1.5 * gradient(x / 40.0, z / 40.0, self.seed.wrapping_add(1));
        let rough = 0.75 + 0.25 * gradient(x / 22.0, z / 22.0, self.seed.wrapping_add(2));
        let deep = 3.0 + (self.thick - 3.0) * (1.0 - out * out) * rough;
        let stalk = if self.stalk { self.stalk() - away } else { f32::MIN };
        Some([top - deep, top, rim - away, stalk])
    }
}

/// What finds a generator's islands.
struct Finder {
    set: RaisedLand,
    seed: u32,
    ground: Arc<Program>,
    id: u64,
    /// How wide a region is.
    region: f32,
}

thread_local! {
    /// The islands last found on this thread, by finder and region: those of the region and the eight round it.
    static FOUND: RefCell<Vec<(u64, [i32; 2], Rc<Vec<Island>>)>> = const { RefCell::new(Vec::new()) };
}

const KEPT: usize = 48;

/// Where round an island's middle the ground under it is read, as shares
/// of half its width: its middle, and eight places round it.
const UNDER: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [0.8, 0.0],
    [-0.8, 0.0],
    [0.0, 0.8],
    [0.0, -0.8],
    [0.55, 0.55],
    [-0.55, 0.55],
    [0.55, -0.55],
    [-0.55, -0.55],
];

impl Finder {
    fn new(set: &RaisedLand, seed: u32, ground: Arc<Program>) -> Self {
        let mut set = set.clone();
        set.size_from = set.size_from.clamp(8.0, 2000.0);
        set.size_to = set.size_to.clamp(set.size_from, 2000.0);
        set.height_to = if set.height_to > set.height_from { set.height_to } else { set.height_from };
        set.thickness = set.thickness.clamp(3.0, 1000.0);
        let mut key = Key::default();
        key.name("raised land").number(set.size_from).number(set.size_to).number(set.height_from);
        key.number(set.height_to).number(set.often).number(set.thickness).whole(set.joined as u32);
        key.whole(seed).whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        let region = set.size_to * 1.15 + 16.0;
        Self { set, seed, ground, id: key.hash(), region }
    }

    fn region_of(&self, x: f32, z: f32) -> [i32; 2] {
        [(x / self.region).floor() as i32, (z / self.region).floor() as i32]
    }

    /// The island of `region`, where the seed gives it one.
    fn found(&self, region: [i32; 2]) -> Option<Island> {
        let set = &self.set;
        let pick = |part: i32| share(mix(self.seed, [region[0], region[1], part]));
        if pick(0) >= set.often {
            return None;
        }
        let radius = (set.size_from + (set.size_to - set.size_from) * pick(1)) * 0.5;
        // Its middle anywhere its whole width stays within the region and a little of those beside it.
        let free = (self.region - radius * 1.6).max(0.0);
        let middle = |axis: usize| (region[axis] as f32 + 0.5) * self.region + (pick(2 + axis as i32) - 0.5) * free;
        let at = [middle(0), middle(1)];
        // It stands over the highest ground under it.
        let xs = UNDER.map(|[x, _]| at[0] + x * radius);
        let zs = UNDER.map(|[_, z]| at[1] + z * radius);
        let mut heights = [0.0; 9];
        self.ground.run(9, Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut heights]);
        let highest = heights.iter().fold(heights[0], |most, &height| if height > most { height } else { most });
        let over = set.height_from + (set.height_to - set.height_from) * pick(4);
        let thick = if set.thickness < over { set.thickness } else { over };
        let top = highest + if over > thick + 4.0 { over } else { thick + 4.0 };
        let seed = mix(self.seed, [region[0], region[1], 5]);
        Some(Island { at, radius, top, ground: heights[0], thick, stalk: set.joined == Joined::Stalk, seed })
    }

    /// The islands that can reach into `region`: its own and those of the
    /// eight round it, found now or as they were kept.
    fn near(&self, region: [i32; 2]) -> Rc<Vec<Island>> {
        let kept = FOUND.with_borrow(|kept| {
            kept.iter().find(|(id, known, _)| *id == self.id && *known == region).map(|kept| kept.2.clone())
        });
        if let Some(kept) = kept {
            return kept;
        }
        let round = (0..9).map(|at| [region[0] + at % 3 - 1, region[1] + at / 3 - 1]);
        let made: Rc<Vec<Island>> = Rc::new(round.filter_map(|region| self.found(region)).collect());
        FOUND.with_borrow_mut(|kept| {
            if kept.len() >= KEPT {
                kept.remove(0);
            }
            kept.push((self.id, region, made.clone()));
        });
        made
    }
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Finder({:#x})", self.id)
    }
}

/// The underside, or with `top` the top, of the island over each column,
/// in metres up, from rows of the columns' x and z: a top under its foot
/// where none is. Over a stalk the underside is the ground the stalk
/// stands on.
#[derive(Debug)]
struct Over {
    finder: Arc<Finder>,
    top: bool,
}

impl Kernel for Over {
    fn key(&self, key: &mut Key) {
        key.name("island over").whole(u32::from(self.top)).whole(self.finder.id as u32);
        key.whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held: Option<([i32; 2], Rc<Vec<Island>>)> = None;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let region = self.finder.region_of(x, z);
            let near = match held.take() {
                Some((known, near)) if known == region => near,
                _ => self.finder.near(region),
            };
            let (mut foot, mut top) = (0.0f32, -1.0f32);
            for island in near.iter() {
                let Some([under, over, _, stalk]) = island.over(x, z) else { continue };
                // A stalk, and a little round it, is read from the ground it stands on.
                let under = if stalk > -2.0 { island.ground - island.thick } else { under };
                let none = top < foot;
                foot = if none || under < foot { under } else { foot };
                top = if none || over > top { over } else { top };
            }
            *out = if self.top { top } else { foot };
            held = Some((region, near));
        }
    }
}

/// The ground with its islands, from rows of how far into the ground each
/// cell is, its x, y and z, and its column's island's underside and top.
#[derive(Debug)]
struct Raised {
    finder: Arc<Finder>,
}

impl Kernel for Raised {
    fn key(&self, key: &mut Key) {
        key.name("raised land").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held: Option<([i32; 2], Rc<Vec<Island>>)> = None;
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, y) = (inputs[0][at], inputs[2][at]);
            *out = ground;
            let (foot, top) = (inputs[4][at], inputs[5][at]);
            if top <= foot || y < foot - 2.0 || y > top + 2.0 {
                continue;
            }
            let (x, z) = (inputs[1][at], inputs[3][at]);
            let region = self.finder.region_of(x, z);
            let near = match held.take() {
                Some((known, near)) if known == region => near,
                _ => self.finder.near(region),
            };
            for island in near.iter() {
                let Some([under, over, within, stalk]) = island.over(x, z) else { continue };
                let (down, up) = (over - y, y - under);
                let inside = if down < up { down } else { up };
                let inside = if inside < within { inside } else { within };
                // Its stalk runs from the ground up into it.
                let inside = if y < under + 1.0 && stalk > inside { stalk } else { inside };
                if inside > *out {
                    *out = inside;
                }
            }
            held = Some((region, near));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Cells as Asked, Graph, NodeKind, WorldGen,
        nodes::{Noise, Output},
    };

    fn world(joined: Joined) -> WorldGen {
        let mut graph = Graph::default();
        let hills =
            graph.add(NodeKind::Noise(Noise { size: 400.0, low: 0.0, high: 80.0, ..Noise::default() }), [0.0; 2]);
        let raised = RaisedLand { size_from: 60.0, size_to: 160.0, often: 0.7, joined, ..RaisedLand::default() };
        let raised = graph.add(NodeKind::RaisedLand(raised), [200.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(hills, 0, output, Output::HEIGHT) && graph.connect(raised, 0, output, Output::VOLUME));
        let world = WorldGen::new(4, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    #[test]
    fn raised_land_stands_clear_of_the_ground_under_it_with_ground_on_top() {
        for joined in Joined::ALL {
            let world = world(joined);
            let volume = world.volume().expect("a Volume");
            assert!(volume.spanned() && volume.depth() == 0.0 && volume.reach() == 0.0);
            let (side, step) = (500usize, 4.0f32);
            let (xs, zs): (Vec<f32>, Vec<f32>) =
                (0..side * side).map(|at| ((at % side) as f32 * step - 1000.0, (at / side) as f32 * step)).unzip();
            let mut heights = vec![0.0; xs.len()];
            world.heights(&xs, &zs, &mut heights);
            let standing = volume.standing(&xs, &zs, &heights);
            let over: Vec<usize> = (0..xs.len()).filter(|&at| !standing[at].is_empty()).collect();
            let share = over.len() as f32 / xs.len() as f32;
            assert!(share > 0.03 && share < 0.4, "islands over {share} of the land");
            let inside = |at: usize, ys: &[f32]| {
                let carried = volume.carried(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
                let (mut inside, mut ground) = (vec![0.0; ys.len()], vec![0.0; ys.len()]);
                let cells = Asked { x: &xs[at..=at], y: ys, z: &zs[at..=at], carried: &carried.column(0), size: 1.0 };
                volume.densities(cells, false, &mut inside, &mut ground);
                inside
            };
            let (mut clear, mut stalks) = (0, 0);
            for &at in over.iter().step_by(7) {
                let [foot, top] = standing[at][0];
                assert!(top - heights[at] >= 40.0, "an island's top is {} m over the ground", top - heights[at]);
                // Ground under its top, open air over it, and under it air down to the ground or a stalk.
                let read = inside(at, &[top - 0.6, top + 1.5, foot - 3.0, heights[at] + 3.0]);
                assert!(read[0] > 0.0 && read[1] < 0.0, "{read:?}");
                let high = standing[at][0][0] - heights[at] > 8.0;
                match joined {
                    Joined::Floating if high => assert!(read[2] < 0.0 && read[3] < 0.0, "air under an island"),
                    Joined::Floating => {}
                    Joined::Stalk => stalks += usize::from(read[3] > 0.0 && foot < heights[at]),
                }
                clear += usize::from(foot - heights[at] > 20.0);
            }
            assert!(clear > over.len() / 7 / 4, "{clear} columns stand well clear");
            assert_eq!(stalks > 0, joined == Joined::Stalk, "{stalks} columns of stalk");
        }
    }
}
