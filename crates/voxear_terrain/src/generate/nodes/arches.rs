//! The Arches node: natural bridges. Where two high places lie within a
//! span of one another across lower ground, a span of ground is laid from
//! one to the other, with an opening under it and a top to walk over.
//!
//! Span From and Span To are how short and how long a bridge may be,
//! Thickness how thick it is at its middle (its underside is an arch,
//! thicker by a sixth of its span where it springs from the ground),
//! Width how wide its top is, and
//! How Often how many of the places one could stand get one.
//!
//! The ground is looked over a region at a time, each a little larger
//! than the longest span: a few lines are tried across each, picked by the
//! seed, and the first with high ground at both ends and low ground
//! between gets the bridge. A region's bridge is the same whichever chunk
//! asks, so one made a chunk at a time is whole.
//!
//! Windows is how many smaller openings are cut clean through thin ridges
//! and walls: where ground stands high with lower ground close on both
//! sides of it, a round way through at the height of the lower ground,
//! with the ridge left whole over it.
//!
//! It says, a column at a time, between which heights a bridge stands, so
//! a world of blocks asks about no more air than that, and a far view
//! shows the bridges.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Solid, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::generate::{
    compile::{Cells, Compiler, Kernel, Key, Places, V},
    eval::{Givens, Program},
    graph::NodeId,
};

/// How many lines are tried across a region, and at how many places the
/// ground is read along each: before its start, at nine places along it,
/// and past its end.
const TRIES: usize = 12;
const READ: usize = 11;

/// How far past each end a bridge runs into the ground it springs from, as
/// a share of its span.
const PAST: f32 = 0.2;

/// The ways a bridge may run: eight, a half of a quarter turn apart.
const WAYS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.923_879_5, 0.382_683_43],
    [0.707_106_77, 0.707_106_77],
    [0.382_683_43, 0.923_879_5],
    [0.0, 1.0],
    [-0.382_683_43, 0.923_879_5],
    [-0.707_106_77, 0.707_106_77],
    [-0.923_879_5, 0.382_683_43],
];

/// Bridges from `span_from` to `span_to` metres long, `thickness` thick at
/// the middle and `width` wide, at as many of the places one could stand
/// as `often` says, 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Arches {
    pub span_from: f32,
    pub span_to: f32,
    pub thickness: f32,
    pub often: f32,
    pub width: f32,
    pub windows: f32,
    pub seed: u32,
}

impl Default for Arches {
    fn default() -> Self {
        Self { span_from: 60.0, span_to: 180.0, thickness: 8.0, often: 0.6, width: 22.0, windows: 0.3, seed: 0 }
    }
}

impl Arches {
    /// Its ground, among its inputs.
    pub const GROUND: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("span_from", "Span From", 8.0, 2000.0, "m"),
    ValueSpec::number("span_to", "Span To", 8.0, 2000.0, "m"),
    ValueSpec::number("thickness", "Thickness", 2.0, 200.0, "m"),
    ValueSpec::number("often", "How Often", 0.0, 1.0, ""),
    ValueSpec::number("width", "Width", 3.0, 500.0, "m"),
    ValueSpec::number("windows", "Windows", 0.0, 1.0, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for Arches {
    const TITLE: &'static str = "Arches";
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
            0 => Value::Number(self.span_from),
            1 => Value::Number(self.span_to),
            2 => Value::Number(self.thickness),
            3 => Value::Number(self.often),
            4 => Value::Number(self.width),
            5 => Value::Number(self.windows),
            6 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.span_from = number,
            (1, Value::Number(number)) => self.span_to = number,
            (2, Value::Number(number)) => self.thickness = number,
            (3, Value::Number(number)) => self.often = number,
            (4, Value::Number(number)) => self.width = number,
            (5, Value::Number(number)) => self.windows = number,
            (6, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        let none = |chance: f32| chance <= 0.0 || chance.is_nan();
        if none(self.often) && none(self.windows) {
            return ground;
        }
        let finder = Arc::new(Finder::new(self, seed_for(cx, node, self.seed), cx.own_ground()));
        let end = |cx: &mut Compiler, what: u32| {
            let finder = finder.clone();
            cx.column(at, |cx, places| cx.run(Over { finder, what }, &[places.x, places.z]))
        };
        let (foot, top) = (end(cx, 0), end(cx, 1));
        cx.note_span(at, foot, top, true);
        // The windows are hollows in the ground: read between their own heights, and no part of a far view.
        let (low, high) = (end(cx, 2), end(cx, 3));
        cx.note_span(at, low, high, false);
        let row = cx.run(Bridged { finder }, &[ground, at.x, at.y, at.z, foot, top, low, high]);
        cx.set_rise(row, cx.rise(ground));
        cx.set_depth(row, cx.depth(ground));
        row
    }
}

/// One bridge: where it starts, the way it runs, how long it is, how high
/// its top is at each end, and how thick and half how wide at its middle.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bridge {
    pub from: [f32; 2],
    pub way: [f32; 2],
    pub span: f32,
    pub tops: [f32; 2],
    pub thick: f32,
    pub half: f32,
}

impl Bridge {
    /// Where the bridge is over the column at `x` and `z`: its underside
    /// and its top there, in metres up, and how far inside its sides and
    /// ends the column is. None where it does not reach.
    pub fn over(&self, x: f32, z: f32) -> Option<[f32; 3]> {
        let (rx, rz) = (x - self.from[0], z - self.from[1]);
        let along = (rx * self.way[0] + rz * self.way[1]) / self.span;
        if !(-PAST..=1.0 + PAST).contains(&along) {
            return None;
        }
        let beside = rx * self.way[1] - rz * self.way[0];
        let beside = if beside < 0.0 { -beside } else { beside };
        // From -1 at one end to 1 at the other: wider and thicker towards the ends.
        let out = (along * 2.0 - 1.0).clamp(-1.0, 1.0);
        let half = self.half * (1.0 + 0.8 * out * out);
        if beside > half {
            return None;
        }
        let top = self.tops[0] + (self.tops[1] - self.tops[0]) * along.clamp(0.0, 1.0);
        let top = top + self.span * 0.03 * (1.0 - out * out);
        // Its underside is an arch: thicker towards the ground it springs from, by a sixth of its span.
        let thick = self.thick + self.span * 0.17 * out * out;
        // Thinner towards its sides, so its underside is rounded across as well as along.
        let across = beside / half;
        let thick = thick * (1.0 - 0.5 * across * across);
        let ends = if along < 0.5 { along + PAST } else { 1.0 + PAST - along } * self.span;
        Some([top - thick, top, if half - beside < ends { half - beside } else { ends }])
    }
}

/// One window: the middle of its way through, the way it runs, half how
/// long it is, and half how wide.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Window {
    pub at: [f32; 3],
    pub way: [f32; 2],
    pub reach: f32,
    pub radius: f32,
}

impl Window {
    /// How far the column at `x` and `z` is along the window and off its
    /// middle line, where the window reaches over it.
    fn over(&self, x: f32, z: f32) -> Option<[f32; 2]> {
        let (rx, rz) = (x - self.at[0], z - self.at[2]);
        let along = rx * self.way[0] + rz * self.way[1];
        let beside = rx * self.way[1] - rz * self.way[0];
        let within = |v: f32, most: f32| v <= most && v >= -most;
        (within(along, self.reach + self.radius) && within(beside, self.radius)).then_some([along, beside])
    }

    /// How far inside its opening the place `y` up the column `beside`
    /// its middle line is, in metres: round, with a level floor.
    fn inside(&self, beside: f32, y: f32) -> f32 {
        let up = y - self.at[1];
        let round = self.radius - (beside * beside + up * up).sqrt();
        let floor = up + self.radius * 0.8;
        if round < floor { round } else { floor }
    }
}

/// How many windows are tried for across a region, along each side, and
/// how many lines are tried for each.
const PANES: i32 = 3;
const WINDOW_TRIES: i32 = 4;

/// What finds a generator's bridges.
pub(crate) struct Finder {
    set: Arches,
    seed: u32,
    ground: Arc<Program>,
    id: u64,
    /// How wide a region is.
    region: f32,
}

thread_local! {
    /// The bridges last found on this thread, by finder and region: those of the region and the eight round it.
    static FOUND: RefCell<Vec<(u64, [i32; 2], Rc<Near>)>> = const { RefCell::new(Vec::new()) };
}

const KEPT: usize = 48;

/// The bridges and windows that can reach into a region.
#[derive(Debug, Default)]
pub(crate) struct Near {
    pub bridges: Vec<Bridge>,
    pub windows: Vec<Window>,
}

impl Finder {
    fn new(set: &Arches, seed: u32, ground: Arc<Program>) -> Self {
        let mut set = set.clone();
        set.span_from = set.span_from.clamp(8.0, 2000.0);
        set.span_to = set.span_to.clamp(set.span_from, 2000.0);
        set.thickness = set.thickness.clamp(2.0, 200.0);
        set.width = set.width.clamp(3.0, 500.0);
        let mut key = Key::default();
        key.name("arches 2").number(set.span_from).number(set.span_to).number(set.thickness).number(set.often);
        key.number(set.windows);
        key.number(set.width).whole(seed).whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        let region = set.span_to * (1.0 + 2.0 * PAST) + set.width * 2.0 + 16.0;
        Self { set, seed, ground, id: key.hash(), region }
    }

    fn region_of(&self, x: f32, z: f32) -> [i32; 2] {
        [(x / self.region).floor() as i32, (z / self.region).floor() as i32]
    }

    /// The bridge of `region`, where one of the lines tried across it has
    /// high ground at both ends and low ground between.
    fn found(&self, region: [i32; 2]) -> Option<Bridge> {
        let set = &self.set;
        let pick = |part: i32| share(mix(self.seed, [region[0], region[1], part]));
        if pick(0) >= set.often || set.often.is_nan() {
            return None;
        }
        // Every line's places, read of the ground at once.
        let mut lines = [([0.0f32; 2], [0.0f32; 2], 0.0f32); TRIES];
        let (mut xs, mut zs) = (Vec::with_capacity(TRIES * READ), Vec::with_capacity(TRIES * READ));
        for (try_at, line) in lines.iter_mut().enumerate() {
            let part = 1 + try_at as i32 * 4;
            let span = set.span_from + (set.span_to - set.span_from) * pick(part);
            let way = WAYS[(pick(part + 1) * 8.0) as usize % 8];
            let middle =
                [(region[0] as f32 + pick(part + 2)) * self.region, (region[1] as f32 + pick(part + 3)) * self.region];
            let from = [middle[0] - way[0] * span * 0.5, middle[1] - way[1] * span * 0.5];
            *line = (from, way, span);
            for read in 0..READ {
                let along = Self::along(read) * span;
                xs.push(from[0] + way[0] * along);
                zs.push(from[1] + way[1] * along);
            }
        }
        let mut heights = vec![0.0; xs.len()];
        self.ground.run(xs.len(), Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut heights]);
        for (try_at, (from, way, span)) in lines.into_iter().enumerate() {
            let read = &heights[try_at * READ..(try_at + 1) * READ];
            let (start, end) = (read[1], read[READ - 2]);
            let lower = if start < end { start } else { end };
            let apart = if start > end { start - end } else { end - start };
            // The ground the bridge springs from runs on behind each end, and the two ends are near as high.
            let level = if span * 0.08 > 3.0 { span * 0.08 } else { 3.0 };
            if apart > level || read[0] < start - 3.0 || read[READ - 1] < end - 3.0 {
                continue;
            }
            // Under its middle half the ground lies well below: its thickness, and room to see through.
            let room = if span * 0.2 > 8.0 { span * 0.2 } else { 8.0 };
            if read[3..READ - 3].iter().any(|&height| height > lower - set.thickness - room) {
                continue;
            }
            return Some(Bridge { from, way, span, tops: [start, end], thick: set.thickness, half: set.width * 0.5 });
        }
        None
    }

    /// The windows of `region`: in each ninth of it the seed picks, the
    /// first line tried whose middle stands high with lower ground close
    /// on both sides.
    fn windows(&self, region: [i32; 2]) -> Vec<Window> {
        let mut found = Vec::new();
        if self.set.windows <= 0.0 || self.set.windows.is_nan() {
            return found;
        }
        let pane = self.region / PANES as f32;
        let mut lines: Vec<([f32; 2], [f32; 2], f32)> = Vec::new();
        let (mut xs, mut zs) = (Vec::new(), Vec::new());
        for at in 0..PANES * PANES {
            let pick = |part: i32| share(mix(self.seed, [region[0], region[1], 1000 + at * 64 + part]));
            if pick(0) >= self.set.windows {
                continue;
            }
            for tried in 0..WINDOW_TRIES {
                let part = 1 + tried * 4;
                let low = [
                    region[0] as f32 * self.region + (at % PANES) as f32 * pane,
                    region[1] as f32 * self.region + (at / PANES) as f32 * pane,
                ];
                let middle = [low[0] + pick(part) * pane, low[1] + pick(part + 1) * pane];
                let way = WAYS[(pick(part + 2) * 8.0) as usize % 8];
                let reach = 7.0 + 20.0 * pick(part + 3);
                lines.push((middle, way, reach));
                for step in [0.0, -reach, reach] {
                    xs.push(middle[0] + way[0] * step);
                    zs.push(middle[1] + way[1] * step);
                }
            }
        }
        let mut heights = vec![0.0; xs.len()];
        self.ground.run(xs.len(), Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut heights]);
        let mut taken = -1;
        for (line, (middle, way, reach)) in lines.into_iter().enumerate() {
            // One window a ninth: the first of its lines that will do.
            let pane = line as i32 / WINDOW_TRIES;
            let [over, before, after] = [0, 1, 2].map(|read| heights[line * 3 + read]);
            let (lower, higher) = if before < after { (before, after) } else { (after, before) };
            let stands = over - higher;
            let radius = (stands * 0.35).clamp(3.0, reach * 0.7);
            // The ridge stands twice the opening's half width and a roof over the higher side, whose ground is
            // near as high as the lower's: the way through is level.
            if pane == taken || stands < radius * 2.0 + 4.0 || higher - lower > radius {
                continue;
            }
            taken = pane;
            found.push(Window { at: [middle[0], higher + radius * 0.8, middle[1]], way, reach, radius });
        }
        found
    }

    /// How far along a line, 0 to 1 and a little past each end, the ground is read at place `read`.
    fn along(read: usize) -> f32 {
        match read {
            0 => -PAST,
            read if read == READ - 1 => 1.0 + PAST,
            read => (read - 1) as f32 / (READ - 3) as f32,
        }
    }

    /// The bridges that can reach into `region`: its own and those of the
    /// eight round it, found now or as they were kept.
    pub fn near(&self, region: [i32; 2]) -> Rc<Near> {
        let kept = FOUND.with_borrow(|kept| {
            kept.iter().find(|(id, known, _)| *id == self.id && *known == region).map(|kept| kept.2.clone())
        });
        if let Some(kept) = kept {
            return kept;
        }
        let round = (0..9).map(|at| [region[0] + at % 3 - 1, region[1] + at / 3 - 1]);
        let mut made = Near::default();
        for region in round {
            made.bridges.extend(self.found(region));
            made.windows.extend(self.windows(region));
        }
        let made = Rc::new(made);
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

/// What stands or is cut over each column, in metres up, from rows of
/// the columns' x and z: by `what`, a bridge's underside, its top, the
/// foot of a window's opening, or its top. A top under its foot where
/// there is none.
#[derive(Debug)]
struct Over {
    finder: Arc<Finder>,
    what: u32,
}

impl Kernel for Over {
    fn key(&self, key: &mut Key) {
        key.name("arch over").whole(self.what).whole(self.finder.id as u32);
        key.whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held: Option<([i32; 2], Rc<Near>)> = None;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let region = self.finder.region_of(x, z);
            let near = match held.take() {
                Some((known, near)) if known == region => near,
                _ => self.finder.near(region),
            };
            let (mut foot, mut top) = (0.0f32, -1.0f32);
            let mut reach = |under: f32, over: f32| {
                // The comparing way round, which says the same on every machine.
                let none = top < foot;
                foot = if none || under < foot { under } else { foot };
                top = if none || over > top { over } else { top };
            };
            if self.what < 2 {
                near.bridges
                    .iter()
                    .filter_map(|bridge| bridge.over(x, z))
                    .for_each(|[under, over, _]| reach(under, over));
            } else {
                for window in near.windows.iter().filter(|window| window.over(x, z).is_some()) {
                    reach(window.at[1] - window.radius - 2.0, window.at[1] + window.radius + 2.0);
                }
            }
            *out = if self.what % 2 == 1 { top } else { foot };
            held = Some((region, near));
        }
    }
}

/// The ground with its bridges and windows, from rows of how far into the
/// ground each cell is, its x, y and z, its column's bridge's underside
/// and top, and its column's window's foot and top.
#[derive(Debug)]
struct Bridged {
    finder: Arc<Finder>,
}

impl Kernel for Bridged {
    fn key(&self, key: &mut Key) {
        key.name("arches").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held: Option<([i32; 2], Rc<Near>)> = None;
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, y) = (inputs[0][at], inputs[2][at]);
            *out = ground;
            let (foot, top) = (inputs[4][at], inputs[5][at]);
            let (low, high) = (inputs[6][at], inputs[7][at]);
            let bridged = top > foot && y >= foot - 2.0 && y <= top + 2.0;
            let cut = high > low && y >= low && y <= high;
            if !bridged && !cut {
                continue;
            }
            let (x, z) = (inputs[1][at], inputs[3][at]);
            let region = self.finder.region_of(x, z);
            let near = match held.take() {
                Some((known, near)) if known == region => near,
                _ => self.finder.near(region),
            };
            for [under, over, within] in near.bridges.iter().filter_map(|bridge| bridge.over(x, z)) {
                let (down, up) = (over - y, y - under);
                let inside = if down < up { down } else { up };
                let inside = if inside < within { inside } else { within };
                if bridged && inside > *out {
                    *out = inside;
                }
            }
            for window in near.windows.iter().filter(|_| cut) {
                let Some([_, beside]) = window.over(x, z) else { continue };
                let hollow = -window.inside(beside, y);
                if hollow < *out {
                    *out = hollow;
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
        nodes::{Output, Plateaus},
    };

    /// Tablelands with a canyon cut through them, and bridges over it.
    fn world(seed: u32) -> WorldGen {
        let mut graph = Graph::default();
        let land = Plateaus { size: 500.0, low: 0.0, high: 200.0, ramps: 0.0, canyon: 90.0, ..Plateaus::default() };
        let land = graph.add(NodeKind::Plateaus(land), [0.0, 0.0]);
        let arches = Arches { span_from: 90.0, span_to: 200.0, often: 1.0, windows: 0.0, ..Arches::default() };
        let arches = graph.add(NodeKind::Arches(arches), [200.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(land, 0, output, Output::HEIGHT) && graph.connect(arches, 0, output, Output::VOLUME));
        let world = WorldGen::new(seed, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    #[test]
    fn a_window_is_a_way_through_a_thin_ridge_with_the_ridge_whole_over_it() {
        use crate::{generate::nodes::Noise, noise::NoiseStyle};
        // Sharp ridges 70 m high, a few tens of metres through.
        let mut graph = Graph::default();
        let ridges = Noise { style: NoiseStyle::Ridged, size: 160.0, octaves: 1, low: -60.0, high: 70.0 };
        let ridges = graph.add(NodeKind::Noise(ridges), [0.0, 0.0]);
        let arches = graph.add(NodeKind::Arches(Arches { often: 0.0, windows: 1.0, ..Arches::default() }), [0.0; 2]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(ridges, 0, output, Output::HEIGHT) && graph.connect(arches, 0, output, Output::VOLUME));
        let world = WorldGen::new(6, 1.0, -100.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        let volume = world.volume().expect("a Volume");
        let (side, step) = (700usize, 2.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step, (at / side) as f32 * step)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let carried = volume.carried(&xs, &zs, &heights);
        assert!(volume.standing(&xs, &zs, &heights).iter().all(Vec::is_empty), "no bridges, and windows stand nowhere");
        let (mut through, mut roofed) = (0, 0);
        for at in 0..xs.len() {
            let Some([foot, top]) = volume.spans(&carried, at).next() else { continue };
            let ys = [(foot + top) * 0.5, top + 1.0, foot - 1.0];
            let (mut inside, mut ground) = ([0.0; 3], [0.0; 3]);
            let cells = Asked { x: &xs[at..=at], y: &ys, z: &zs[at..=at], carried: &carried.column(at), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            // Open at its middle wherever the ground was there to cut; and where the ridge stands over it, rock
            // over the opening and under its floor.
            if heights[at] > ys[0] + 1.0 && inside[0] < 0.0 {
                through += 1;
                assert_eq!(inside[0], ground[0], "a window is open air, no cave");
            }
            if heights[at] > top + 3.0 {
                roofed += 1;
                assert!(inside[1] > 0.0 && inside[2] > 0.0, "the ridge is whole over a window and under it");
            }
        }
        assert!(through > 200 && roofed > 100, "{through} columns cut through, {roofed} with the ridge over them");
    }

    #[test]
    fn an_arch_has_an_opening_under_it_and_ground_to_walk_across_it() {
        let world = world(3);
        let volume = world.volume().expect("a Volume");
        assert!(volume.spanned() && volume.depth() == 0.0 && volume.reach() == 0.0);
        // Columns 3 m apart over 3 km each way: those a bridge stands over.
        let (side, step) = (1000usize, 3.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 1500.0, (at / side) as f32 * step - 1500.0)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let standing = volume.standing(&xs, &zs, &heights);
        let over: Vec<usize> = (0..xs.len()).filter(|&at| !standing[at].is_empty()).collect();
        assert!(over.len() > 300, "bridges stand over {} columns", over.len());
        // Each bridge is the columns that touch: gathered from one of them.
        let mut bridge = vec![usize::MAX; xs.len()];
        let mut bridges = 0;
        for &first in &over {
            if bridge[first] != usize::MAX {
                continue;
            }
            bridge[first] = bridges;
            let mut next = vec![first];
            while let Some(at) = next.pop() {
                for to in [at.wrapping_sub(1), at + 1, at.wrapping_sub(side), at + side] {
                    if to < xs.len() && !standing[to].is_empty() && bridge[to] == usize::MAX {
                        bridge[to] = bridges;
                        next.push(to);
                    }
                }
            }
            bridges += 1;
        }
        assert!(bridges >= 3, "{bridges} bridges in 9 square kilometres");
        let inside = |at: usize, y: f32| {
            let carried = volume.carried(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
            let (mut inside, mut ground) = ([0.0], [0.0]);
            let cells = Asked { x: &xs[at..=at], y: &[y], z: &zs[at..=at], carried: &carried.column(0), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            inside[0]
        };
        let (mut open, mut flush) = (vec![0.0f32; bridges], vec![f32::MAX; bridges]);
        for &at in &over {
            let [foot, top] = standing[at][0];
            // Ground all the way across: solid under its top, and its top a step at most from the column beside.
            assert!(inside(at, top - 0.6) > 0.0, "a bridge's top is ground");
            for to in [at + 1, at + side].into_iter().filter(|&to| to < xs.len() && !standing[to].is_empty()) {
                assert!((standing[to][0][1] - top).abs() < 1.5, "a bridge's top is walked along");
            }
            // An opening under it: air from over the ground to under the bridge.
            let room = foot - heights[at];
            if room > 4.0 {
                assert!(inside(at, foot - 1.5) < 0.0 && inside(at, heights[at] + 1.5) < 0.0, "air under a bridge");
            }
            open[bridge[at]] = open[bridge[at]].max(room);
            flush[bridge[at]] = flush[bridge[at]].min((top - heights[at]).abs());
        }
        for at in 0..bridges {
            // Every bridge is clear of the ground by 16 m or more somewhere, and meets it at its ends.
            assert!(
                open[at] >= 16.0 && flush[at] < 4.0,
                "bridge {at}: {} m clear, {} m off the ground",
                open[at],
                flush[at]
            );
        }
        // The same cells asked in any order hold the same: a bridge is its region's, whoever asks.
        let other = self::world(3);
        let again = other.volume().unwrap().standing(&xs[..5000], &zs[..5000], &heights[..5000]);
        assert_eq!(again, standing[..5000]);
    }
}
