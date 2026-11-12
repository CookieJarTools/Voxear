//! The Spires node: towers of rock standing up out of the ground, from
//! squat stacks to needles many times as tall as they are wide, sheer-sided
//! and banded, their tops rounded and grown over.
//!
//! Size From and Size To are how wide a spire is at its foot, Height From
//! and Height To how high its top stands over the ground at its foot, and
//! How Often how many of the places one could stand have one. Spacing is
//! how far apart those places are. Taper narrows a spire towards its top,
//! from a column to a point; Lean tips it over, every spire its own way;
//! Bands cuts grooves round it every so many metres, as layered rock
//! weathers. Clusters gathers spires into groups with open ground between,
//! from scattered alone to crowded together.
//!
//! A spire's top is ground like any other: it takes its column's layers,
//! counted down from the open air over it, so it is grown over as the
//! ground round it is.
//!
//! The ground is shared out in cells Spacing apart, each with one try at a
//! spire, by the seed, so the same spire comes from any chunk. The node
//! says, a column at a time, between which heights a spire stands, so a
//! world of blocks asks about no more air than that, and a far view shows
//! the spires.

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
    noise::{gradient, gradient3},
};

/// Spires from `size_from` to `size_to` metres wide at their foot, their
/// tops `height_from` to `height_to` metres over it, one try every
/// `spacing` metres kept as often as `often` says, 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spires {
    pub size_from: f32,
    pub size_to: f32,
    pub height_from: f32,
    pub height_to: f32,
    pub spacing: f32,
    pub often: f32,
    pub taper: f32,
    pub lean: f32,
    pub bands: f32,
    pub clusters: f32,
    pub seed: u32,
}

impl Default for Spires {
    fn default() -> Self {
        Self {
            size_from: 18.0,
            size_to: 60.0,
            height_from: 60.0,
            height_to: 180.0,
            spacing: 120.0,
            often: 0.5,
            taper: 0.35,
            lean: 0.1,
            bands: 12.0,
            clusters: 0.5,
            seed: 0,
        }
    }
}

impl Spires {
    /// Its ground, among its inputs.
    pub const GROUND: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size_from", "Size From", 4.0, 1000.0, "m"),
    ValueSpec::number("size_to", "Size To", 4.0, 1000.0, "m"),
    ValueSpec::number("height_from", "Height From", 4.0, 4000.0, "m"),
    ValueSpec::number("height_to", "Height To", 4.0, 4000.0, "m"),
    ValueSpec::number("spacing", "Spacing", 8.0, 4000.0, "m"),
    ValueSpec::number("often", "How Often", 0.0, 1.0, ""),
    ValueSpec::number("taper", "Taper", 0.0, 1.0, ""),
    ValueSpec::number("lean", "Lean", 0.0, 1.0, ""),
    ValueSpec::number("bands", "Bands", 0.0, 200.0, "m"),
    ValueSpec::number("clusters", "Clusters", 0.0, 1.0, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for Spires {
    const TITLE: &'static str = "Spires";
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
            4 => Value::Number(self.spacing),
            5 => Value::Number(self.often),
            6 => Value::Number(self.taper),
            7 => Value::Number(self.lean),
            8 => Value::Number(self.bands),
            9 => Value::Number(self.clusters),
            10 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size_from = number,
            (1, Value::Number(number)) => self.size_to = number,
            (2, Value::Number(number)) => self.height_from = number,
            (3, Value::Number(number)) => self.height_to = number,
            (4, Value::Number(number)) => self.spacing = number,
            (5, Value::Number(number)) => self.often = number,
            (6, Value::Number(number)) => self.taper = number,
            (7, Value::Number(number)) => self.lean = number,
            (8, Value::Number(number)) => self.bands = number,
            (9, Value::Number(number)) => self.clusters = number,
            (10, Value::Whole(whole)) => self.seed = whole,
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
        let end = |cx: &mut Compiler, what: u32| {
            let finder = finder.clone();
            cx.column(at, |cx, places| cx.run(Over { finder, what }, &[places.x, places.z]))
        };
        // All a spire may reach in a column is read; only its core, whole from foot to top, shows far off.
        let (foot, top, core) = (end(cx, 0), end(cx, 1), end(cx, 2));
        cx.note_span(at, foot, top, false);
        cx.note_span(at, foot, core, true);
        let row = cx.run(Towered { finder }, &[ground, at.x, at.y, at.z, foot, top]);
        cx.set_rise(row, cx.rise(ground));
        cx.set_depth(row, cx.depth(ground));
        row
    }
}

/// One spire: where its foot's middle is, half how wide its foot is, the
/// height of its foot and of its top, how far it leans along x and z for
/// every metre up, and the seed of its own shape.
#[derive(Clone, Copy, Debug)]
struct Spire {
    at: [f32; 2],
    radius: f32,
    foot: f32,
    top: f32,
    lean: [f32; 2],
    seed: u32,
}

/// How much wider a spire's lobes and flared foot let it be than its
/// radius: as far from its axis as any of it reaches.
const WIDEST: f32 = 1.6;

impl Spire {
    /// Where its axis is at height `y`.
    fn axis(&self, y: f32) -> [f32; 2] {
        let up = y - self.foot;
        [self.at[0] + self.lean[0] * up, self.at[1] + self.lean[1] * up]
    }

    /// How wide it is at height `y`, from its axis, before its lobes and
    /// bands: tapering up its height, flared at its foot, and rounded over
    /// at its top. Nothing over its top.
    fn plain(&self, y: f32, taper: f32) -> f32 {
        let tall = self.top - self.foot;
        if y >= self.top || tall <= 0.0 {
            return 0.0;
        }
        let up = ((y - self.foot) / tall).max(0.0);
        let mut width = self.radius * (1.0 - taper * up);
        // Its top rounded over, in the last of its height as tall as it is wide there.
        let cap = (width * 1.2).min(tall * 0.5);
        let under_top = self.top - y;
        if under_top < cap {
            let t = 1.0 - under_top / cap;
            width *= (1.0 - t * t).max(0.0).sqrt();
        }
        // Its foot flares into the ground round it.
        let flare = 1.0 - (y - self.foot) / (self.radius * 1.5);
        if flare > 0.0 {
            width *= 1.0 + 0.45 * flare * flare;
        }
        width
    }

    /// How far inside it the place `x`, `y`, `z` is, in metres: less than
    /// nothing outside it.
    fn inside(&self, x: f32, y: f32, z: f32, taper: f32, bands: f32) -> f32 {
        let width = self.plain(y, taper);
        let axis = self.axis(y);
        let (dx, dz) = (x - axis[0], z - axis[1]);
        let away = (dx * dx + dz * dz).sqrt();
        if width <= 0.0 {
            return (self.top - y).min(-away);
        }
        // Lobes and fluting run up it, wandering a little as they go.
        let lobe = self.radius * 0.8;
        let rough = gradient3(x / lobe, y / (lobe * 2.5), z / lobe, self.seed);
        let mut width = width * (1.0 + 0.2 * rough);
        // Grooves round it where softer layers weathered back.
        if bands > 0.0 {
            let phase = ((y - self.foot) / bands).fract();
            let groove = 1.0 - (phase - 0.5).abs() * 2.0;
            let groove = groove * groove * groove * groove;
            width *= 1.0 - 0.14 * groove * groove;
        }
        width - away
    }
}

/// What finds a generator's spires.
struct Finder {
    set: Spires,
    seed: u32,
    ground: Arc<Program>,
    id: u64,
}

thread_local! {
    /// The spires last found on this thread, by finder and cell: those of the cells reaching it.
    static FOUND: RefCell<Vec<(u64, [i32; 2], Rc<Vec<Spire>>)>> = const { RefCell::new(Vec::new()) };
}

const KEPT: usize = 48;

impl Finder {
    fn new(set: &Spires, seed: u32, ground: Arc<Program>) -> Self {
        let mut set = set.clone();
        set.size_from = set.size_from.clamp(4.0, 1000.0);
        set.size_to = set.size_to.clamp(set.size_from, 1000.0);
        set.height_from = set.height_from.clamp(4.0, 4000.0);
        set.height_to = set.height_to.clamp(set.height_from, 4000.0);
        set.spacing = set.spacing.clamp(8.0, 4000.0);
        set.taper = set.taper.clamp(0.0, 1.0);
        set.lean = set.lean.clamp(0.0, 1.0);
        set.bands = set.bands.clamp(0.0, 200.0);
        set.clusters = set.clusters.clamp(0.0, 1.0);
        let mut key = Key::default();
        key.name("spires").number(set.size_from).number(set.size_to).number(set.height_from);
        key.number(set.height_to).number(set.spacing).number(set.often).number(set.taper).number(set.lean);
        key.number(set.bands).number(set.clusters);
        key.whole(seed).whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        Self { set, seed, ground, id: key.hash() }
    }

    fn cell_of(&self, x: f32, z: f32) -> [i32; 2] {
        [(x / self.set.spacing).floor() as i32, (z / self.set.spacing).floor() as i32]
    }

    /// How many cells either side of a column's own a spire may reach it from.
    fn reach(&self) -> i32 {
        let widest = self.set.size_to * 0.5 * WIDEST + self.set.lean * 0.5 * self.set.height_to;
        (widest / self.set.spacing).ceil() as i32 + 1
    }

    /// The spire of `cell`, where the seed gives it one.
    fn found(&self, cell: [i32; 2]) -> Option<Spire> {
        let set = &self.set;
        let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
        // Clusters: a broad pattern says where spires gather, and how many of the tries there are kept.
        let wide = set.spacing * 5.0;
        let middle = [(cell[0] as f32 + 0.5) * set.spacing, (cell[1] as f32 + 0.5) * set.spacing];
        let gathered = 0.5 + 0.5 * gradient(middle[0] / wide, middle[1] / wide, self.seed.wrapping_add(7));
        let gathered = gathered.clamp(0.0, 1.0);
        let chance = set.often * (1.0 - set.clusters + set.clusters * 2.0 * gathered * gathered);
        if pick(0) >= chance {
            return None;
        }
        let at = [middle[0] + (pick(1) - 0.5) * set.spacing * 0.8, middle[1] + (pick(2) - 0.5) * set.spacing * 0.8];
        let radius = (set.size_from + (set.size_to - set.size_from) * pick(3)) * 0.5;
        // The lowest of the ground round its foot, so it stands on all of it.
        let xs = [at[0], at[0] + radius, at[0] - radius, at[0], at[0]];
        let zs = [at[1], at[1], at[1], at[1] + radius, at[1] - radius];
        let mut heights = [0.0; 5];
        self.ground.run(5, Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut heights]);
        let lowest = heights.iter().fold(heights[0], |least, &height| if height < least { height } else { least });
        let foot = lowest - 2.0;
        let tall = set.height_from + (set.height_to - set.height_from) * pick(4);
        let lean = set.lean * 0.5 * pick(5);
        let (lx, lz) = (pick(6) - 0.5, pick(7) - 0.5);
        let length = (lx * lx + lz * lz).sqrt().max(0.01);
        let lean = [lean * lx / length, lean * lz / length];
        let seed = mix(self.seed, [cell[0], cell[1], 8]);
        Some(Spire { at, radius, foot, top: heights[0] + tall, lean, seed })
    }

    /// The spires that can reach into the column of `cell`.
    fn near(&self, cell: [i32; 2]) -> Rc<Vec<Spire>> {
        let kept = FOUND.with_borrow(|kept| {
            kept.iter().find(|(id, known, _)| *id == self.id && *known == cell).map(|kept| kept.2.clone())
        });
        if let Some(kept) = kept {
            return kept;
        }
        let reach = self.reach();
        let mut made = Vec::new();
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                made.extend(self.found([cell[0] + dx, cell[1] + dz]));
            }
        }
        let made = Rc::new(made);
        FOUND.with_borrow_mut(|kept| {
            if kept.len() >= KEPT {
                kept.remove(0);
            }
            kept.push((self.id, cell, made.clone()));
        });
        made
    }

    /// The spires near the column at `x`, `z`, kept from the last column
    /// while it is in the same cell.
    fn near_held(&self, x: f32, z: f32, held: &mut Option<([i32; 2], Rc<Vec<Spire>>)>) -> Rc<Vec<Spire>> {
        let cell = self.cell_of(x, z);
        let near = match held.take() {
            Some((known, near)) if known == cell => near,
            _ => self.near(cell),
        };
        *held = Some((cell, near.clone()));
        near
    }
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Finder({:#x})", self.id)
    }
}

/// For each column, from rows of its x and z: with `what` 0 the lowest
/// foot of a spire reaching it, with 1 the highest it may reach there, and
/// with 2 how high it is surely solid all the way up from that foot, in
/// metres up. A top under its foot where none is.
#[derive(Debug)]
struct Over {
    finder: Arc<Finder>,
    what: u32,
}

impl Kernel for Over {
    fn key(&self, key: &mut Key) {
        key.name("spire over").whole(self.what).whole(self.finder.id as u32);
        key.whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held = None;
        let set = &self.finder.set;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let near = self.finder.near_held(x, z, &mut held);
            let (mut foot, mut top, mut core) = (0.0f32, -1.0f32, -1.0f32);
            for spire in near.iter() {
                // Too far from its foot for any of it to reach the column.
                let far = spire.radius * WIDEST + 2.0 + set.lean * 0.5 * (spire.top - spire.foot);
                if (x - spire.at[0]).abs() > far || (z - spire.at[1]).abs() > far {
                    continue;
                }
                // How high up it the column is still within its widest reach, and how high well inside it.
                let (mut reach, mut solid, mut whole) = (f32::MIN, f32::MIN, true);
                let mut y = spire.foot;
                while y <= spire.top {
                    let axis = spire.axis(y);
                    let (dx, dz) = (x - axis[0], z - axis[1]);
                    let away = (dx * dx + dz * dz).sqrt();
                    let width = spire.plain(y, set.taper);
                    if away <= width * WIDEST + 1.0 {
                        reach = y;
                    }
                    // Well within its plain width, lobes and grooves leave it solid.
                    if whole && away <= width * 0.6 - 1.0 {
                        solid = y;
                    } else {
                        whole = false;
                    }
                    y += 1.0;
                }
                if reach == f32::MIN {
                    continue;
                }
                let none = top < foot;
                foot = if none || spire.foot < foot { spire.foot } else { foot };
                top = if none || reach + 2.0 > top { reach + 2.0 } else { top };
                if solid > core {
                    core = solid;
                }
            }
            *out = match self.what {
                0 => foot,
                1 => top,
                _ if core > foot => core,
                _ => foot - 1.0,
            };
        }
    }
}

/// The ground with its spires, from rows of how far into the ground each
/// cell is, its x, y and z, and the heights its column's spires reach
/// between.
#[derive(Debug)]
struct Towered {
    finder: Arc<Finder>,
}

impl Kernel for Towered {
    fn key(&self, key: &mut Key) {
        key.name("spires").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held = None;
        let set = &self.finder.set;
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, y) = (inputs[0][at], inputs[2][at]);
            *out = ground;
            let (foot, top) = (inputs[4][at], inputs[5][at]);
            if top <= foot || y < foot - 2.0 || y > top + 2.0 {
                continue;
            }
            let (x, z) = (inputs[1][at], inputs[3][at]);
            for spire in self.finder.near_held(x, z, &mut held).iter() {
                if y < spire.foot - 2.0 || y > spire.top + 2.0 {
                    continue;
                }
                let inside = spire.inside(x, y, z, set.taper, set.bands);
                if inside > *out {
                    *out = inside;
                }
            }
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

    fn world(spires: Spires) -> WorldGen {
        let mut graph = Graph::default();
        let hills =
            graph.add(NodeKind::Noise(Noise { size: 400.0, low: 0.0, high: 30.0, ..Noise::default() }), [0.0; 2]);
        let spires = graph.add(NodeKind::Spires(spires), [200.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(hills, 0, output, Output::HEIGHT) && graph.connect(spires, 0, output, Output::VOLUME));
        let world = WorldGen::new(4, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    #[test]
    fn spires_stand_tall_and_narrow_out_of_the_ground_with_ground_on_top() {
        let world = world(Spires { often: 0.8, ..Spires::default() });
        let volume = world.volume().expect("a Volume");
        assert!(volume.spanned());
        let (side, step) = (300usize, 4.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 600.0, (at / side) as f32 * step)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let standing = volume.standing(&xs, &zs, &heights);
        let over: Vec<usize> = (0..xs.len()).filter(|&at| !standing[at].is_empty()).collect();
        let share = over.len() as f32 / xs.len() as f32;
        assert!(share > 0.01 && share < 0.3, "spires stand over {share} of the land");
        let inside = |at: usize, ys: &[f32]| {
            let carried = volume.carried(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
            let (mut inside, mut ground) = (vec![0.0; ys.len()], vec![0.0; ys.len()]);
            let cells = Asked { x: &xs[at..=at], y: ys, z: &zs[at..=at], carried: &carried.column(0), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            inside
        };
        let mut tall = 0;
        for &at in &over {
            let [foot, top] = standing[at][0];
            assert!(foot <= heights[at] + 1.0, "a spire's core starts at the ground: {foot} over {}", heights[at]);
            // Solid all the way up its core, and open air over its top.
            let ys: Vec<f32> = (0..8).map(|step| foot + 1.0 + (top - foot - 2.0) * step as f32 / 7.0).collect();
            let read = inside(at, &ys);
            assert!(read.iter().all(|&inside| inside > 0.0), "{read:?} up a spire from {foot} to {top}");
            tall += usize::from(top - heights[at] > 50.0);
        }
        assert!(tall > over.len() / 4, "{tall} of {} columns stand 50 m over the ground", over.len());
        // Open air beside a spire, near its top.
        let highest = over.iter().copied().max_by(|&a, &b| standing[a][0][1].total_cmp(&standing[b][0][1])).unwrap();
        let top = standing[highest][0][1];
        let beside = (0..xs.len())
            .filter(|&at| standing[at].is_empty())
            .filter(|&at| (xs[at] - xs[highest]).abs() < 80.0 && (zs[at] - zs[highest]).abs() < 80.0)
            .take(5)
            .all(|at| inside(at, &[top - 4.0])[0] < 0.0);
        assert!(beside, "air beside a spire's top");
    }

    #[test]
    fn spires_are_the_same_from_any_chunk_and_none_without_a_chance() {
        let world = world(Spires { often: 0.8, ..Spires::default() });
        let volume = world.volume().unwrap();
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..2000).map(|at| ((at % 50) as f32 * 7.0, (at / 50) as f32 * 9.0)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let all = volume.standing(&xs, &zs, &heights);
        for at in (0..xs.len()).step_by(37) {
            let alone = volume.standing(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
            assert_eq!(alone[0], all[at], "column {at} alone");
        }
        let none = world_without();
        assert!(none.volume().is_none_or(|volume| !volume.spanned()));
    }

    fn world_without() -> WorldGen {
        world(Spires { often: 0.0, ..Spires::default() })
    }
}
