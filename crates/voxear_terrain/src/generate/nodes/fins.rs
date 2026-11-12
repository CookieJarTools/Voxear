//! The Fins node: blades of rock jutting from the ground, thin and tilted,
//! standing in rows side by side as tilted layers of hard rock stand where
//! the softer ground round them has worn away.
//!
//! Length From and Length To are how long a fin runs along the ground,
//! Height From and Height To how high it stands, and Thickness how thick it
//! is at its foot, thinning to an edge along its top. Dip is how far it
//! leans over, from upright, and every fin of a field leans the same way.
//! Rows is how many stand side by side in a group, and Spacing how far
//! apart the groups' places are, How Often how many of those have one.
//! Jagged is how much its top is broken into teeth.
//!
//! Neighbouring groups run the same way as one another, turning slowly
//! across the land, so a field of fins reads as one tilted layer of rock.
//!
//! The ground is shared out in cells Spacing apart, each with one try at a
//! group, by the seed, so the same fin comes from any chunk. The node says,
//! a column at a time, between which heights a fin stands, so a world of
//! blocks asks about no more air than that, and a far view shows them.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, STEEPEST, Solid, Value, ValueSpec,
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

/// Fins `length_from` to `length_to` metres long and `height_from` to
/// `height_to` high, `thickness` thick at the foot, leaning to `dip` (an
/// angle kept as rise over run, upright for the steepest), in groups of up
/// to `rows`, one try every `spacing` metres kept as often as `often` says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fins {
    pub length_from: f32,
    pub length_to: f32,
    pub height_from: f32,
    pub height_to: f32,
    pub thickness: f32,
    pub dip: f32,
    pub rows: u32,
    pub spacing: f32,
    pub often: f32,
    pub jagged: f32,
    pub seed: u32,
}

impl Default for Fins {
    fn default() -> Self {
        Self {
            length_from: 40.0,
            length_to: 140.0,
            height_from: 25.0,
            height_to: 80.0,
            thickness: 10.0,
            // About 60 degrees.
            dip: 1.73,
            rows: 3,
            spacing: 150.0,
            often: 0.5,
            jagged: 0.5,
            seed: 0,
        }
    }
}

impl Fins {
    /// Its ground, among its inputs.
    pub const GROUND: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("length_from", "Length From", 4.0, 2000.0, "m"),
    ValueSpec::number("length_to", "Length To", 4.0, 2000.0, "m"),
    ValueSpec::number("height_from", "Height From", 2.0, 2000.0, "m"),
    ValueSpec::number("height_to", "Height To", 2.0, 2000.0, "m"),
    ValueSpec::number("thickness", "Thickness", 1.0, 200.0, "m"),
    ValueSpec::angle("dip", "Dip"),
    ValueSpec::whole("rows", "Rows", 1, 12),
    ValueSpec::number("spacing", "Spacing", 8.0, 4000.0, "m"),
    ValueSpec::number("often", "How Often", 0.0, 1.0, ""),
    ValueSpec::number("jagged", "Jagged", 0.0, 1.0, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for Fins {
    const TITLE: &'static str = "Fins";
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
            0 => Value::Number(self.length_from),
            1 => Value::Number(self.length_to),
            2 => Value::Number(self.height_from),
            3 => Value::Number(self.height_to),
            4 => Value::Number(self.thickness),
            5 => Value::Number(self.dip),
            6 => Value::Whole(self.rows),
            7 => Value::Number(self.spacing),
            8 => Value::Number(self.often),
            9 => Value::Number(self.jagged),
            10 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.length_from = number,
            (1, Value::Number(number)) => self.length_to = number,
            (2, Value::Number(number)) => self.height_from = number,
            (3, Value::Number(number)) => self.height_to = number,
            (4, Value::Number(number)) => self.thickness = number,
            (5, Value::Number(number)) => self.dip = number,
            (6, Value::Whole(whole)) => self.rows = whole,
            (7, Value::Number(number)) => self.spacing = number,
            (8, Value::Number(number)) => self.often = number,
            (9, Value::Number(number)) => self.jagged = number,
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
        // All a fin may reach in a column is read; only where it is whole across the column shows far off.
        let (foot, top, core_foot, core) = (end(cx, 0), end(cx, 1), end(cx, 3), end(cx, 2));
        cx.note_span(at, foot, top, false);
        cx.note_span(at, core_foot, core, true);
        let row = cx.run(Bladed { finder }, &[ground, at.x, at.y, at.z, foot, top]);
        cx.set_rise(row, cx.rise(ground));
        cx.set_depth(row, cx.depth(ground));
        row
    }
}

/// One fin: its middle at its foot, the way it runs along the ground and
/// the way it leans (across it), half how long it is, the height of its
/// foot and how high its top stands over it, half how thick it is at its
/// foot, how far it leans across for every metre up, and its own seed.
#[derive(Clone, Copy, Debug)]
struct Fin {
    at: [f32; 2],
    along: [f32; 2],
    half: f32,
    foot: f32,
    tall: f32,
    thick: f32,
    lean: f32,
    seed: u32,
}

impl Fin {
    /// Where the place `x`, `z` is from its middle: how far along it, and
    /// how far across it at its foot.
    fn local(&self, x: f32, z: f32) -> (f32, f32) {
        let (dx, dz) = (x - self.at[0], z - self.at[1]);
        (dx * self.along[0] + dz * self.along[1], dz * self.along[0] - dx * self.along[1])
    }

    /// How high its top stands over its foot, `along` metres along it:
    /// highest near its middle, falling away to its ends, broken into
    /// teeth as `jagged` says.
    fn crest(&self, along: f32, jagged: f32) -> f32 {
        let end = 1.0 - (along / self.half).abs();
        if end <= 0.0 {
            return 0.0;
        }
        let tooth = self.half * 0.18;
        let teeth =
            gradient(along / tooth, 0.5, self.seed) * 0.6 + gradient(along / (tooth * 0.4), 3.5, self.seed) * 0.4;
        self.tall * end.sqrt() * (1.0 + jagged * 0.35 * teeth).max(0.1)
    }

    /// How far inside it the place `x`, `y`, `z` is, in metres: less than
    /// nothing outside it.
    fn inside(&self, x: f32, y: f32, z: f32, jagged: f32) -> f32 {
        let (along, across) = self.local(x, z);
        let up = y - self.foot;
        let crest = self.crest(along, jagged);
        if crest <= 0.0 || up >= crest {
            return -(up - crest).abs().max(1.0);
        }
        // Thinning from its foot to an edge along its top, and leaning over as it rises.
        let half = 0.6 + (self.thick - 0.6) * (1.0 - up / crest).max(0.0).sqrt();
        let off = across - self.lean * up;
        let rough = 0.6 * gradient3(x / 7.0, y / 7.0, z / 7.0, self.seed.wrapping_add(1));
        let sides = half - off.abs() + rough;
        let ends = (self.half - along.abs()) * 2.0;
        sides.min(crest - up).min(ends)
    }

    /// How far from its middle any of it may reach along the ground.
    fn reach(&self) -> f32 {
        self.half + self.thick + self.lean.abs() * self.tall + 2.0
    }
}

/// What finds a generator's fins.
struct Finder {
    set: Fins,
    seed: u32,
    ground: Arc<Program>,
    id: u64,
}

thread_local! {
    /// The fins last found on this thread, by finder and cell: those of the cells reaching it.
    static FOUND: RefCell<Vec<(u64, [i32; 2], Rc<Vec<Fin>>)>> = const { RefCell::new(Vec::new()) };
}

const KEPT: usize = 48;

impl Finder {
    fn new(set: &Fins, seed: u32, ground: Arc<Program>) -> Self {
        let mut set = set.clone();
        set.length_from = set.length_from.clamp(4.0, 2000.0);
        set.length_to = set.length_to.clamp(set.length_from, 2000.0);
        set.height_from = set.height_from.clamp(2.0, 2000.0);
        set.height_to = set.height_to.clamp(set.height_from, 2000.0);
        set.thickness = set.thickness.clamp(1.0, 200.0);
        set.dip = set.dip.clamp(0.2, STEEPEST);
        set.rows = set.rows.clamp(1, 12);
        set.spacing = set.spacing.clamp(8.0, 4000.0);
        set.jagged = set.jagged.clamp(0.0, 1.0);
        let mut key = Key::default();
        key.name("fins").number(set.length_from).number(set.length_to).number(set.height_from);
        key.number(set.height_to).number(set.thickness).number(set.dip).whole(set.rows).number(set.spacing);
        key.number(set.often).number(set.jagged);
        key.whole(seed).whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        Self { set, seed, ground, id: key.hash() }
    }

    fn cell_of(&self, x: f32, z: f32) -> [i32; 2] {
        [(x / self.set.spacing).floor() as i32, (z / self.set.spacing).floor() as i32]
    }

    /// How many cells either side of a column's own a fin may reach it from.
    fn reach(&self) -> i32 {
        let set = &self.set;
        let rows = set.rows as f32 * set.thickness * 3.0;
        let widest = set.length_to * 0.5 + rows + set.thickness + set.height_to / set.dip + set.spacing * 0.4;
        (widest / set.spacing).ceil() as i32
    }

    /// The fins of `cell`'s group, where the seed gives it one.
    fn found(&self, cell: [i32; 2], into: &mut Vec<Fin>) {
        let set = &self.set;
        let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
        if pick(0) >= set.often {
            return;
        }
        let at = [
            (cell[0] as f32 + 0.5 + (pick(1) - 0.5) * 0.8) * set.spacing,
            (cell[1] as f32 + 0.5 + (pick(2) - 0.5) * 0.8) * set.spacing,
        ];
        // The way they run turns slowly across the land, so neighbouring groups run alike.
        let broad = set.spacing * 6.0;
        let (ax, az) = (
            gradient(at[0] / broad, at[1] / broad, self.seed.wrapping_add(11)),
            gradient(at[0] / broad, at[1] / broad, self.seed.wrapping_add(12)),
        );
        let length = (ax * ax + az * az).sqrt();
        let along = if length > 0.01 { [ax / length, az / length] } else { [1.0, 0.0] };
        let lean = if set.dip >= STEEPEST { 0.0 } else { 1.0 / set.dip };
        let rows = 1 + (pick(3) * set.rows as f32) as u32;
        let gap = set.thickness * (2.2 + pick(4));
        let across = [-along[1], along[0]];
        for row in 0..rows.min(set.rows) {
            let part = 10 + row as i32 * 8;
            let shift = (row as f32 - (rows - 1) as f32 * 0.5) * gap;
            let slide = (pick(part) - 0.5) * set.length_from * 0.6;
            let middle = [at[0] + across[0] * shift + along[0] * slide, at[1] + across[1] * shift + along[1] * slide];
            let half = (set.length_from + (set.length_to - set.length_from) * pick(part + 1)) * 0.5;
            let tall = set.height_from + (set.height_to - set.height_from) * pick(part + 2);
            // Its foot at the lowest of the ground along it.
            let xs = [middle[0], middle[0] + along[0] * half * 0.8, middle[0] - along[0] * half * 0.8];
            let zs = [middle[1], middle[1] + along[1] * half * 0.8, middle[1] - along[1] * half * 0.8];
            let mut heights = [0.0; 3];
            self.ground.run(3, Givens { x: &xs, z: &zs, ..Givens::default() }, &mut [&mut heights]);
            let lowest = heights.iter().fold(heights[0], |least, &height| if height < least { height } else { least });
            let thick = set.thickness * 0.5 * (0.7 + 0.6 * pick(part + 3));
            let seed = mix(self.seed, [cell[0], cell[1], part + 4]);
            into.push(Fin { at: middle, along, half, foot: lowest - 3.0, tall: tall + 3.0, thick, lean, seed });
        }
    }

    /// The fins that can reach into the column of `cell`.
    fn near(&self, cell: [i32; 2]) -> Rc<Vec<Fin>> {
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
                self.found([cell[0] + dx, cell[1] + dz], &mut made);
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

    fn near_held(&self, x: f32, z: f32, held: &mut Option<([i32; 2], Rc<Vec<Fin>>)>) -> Rc<Vec<Fin>> {
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
/// foot of a fin reaching it, with 1 the highest it may reach there, and
/// with 3 and 2 the foot and top of where the column runs through one fin
/// well inside it all the way, in metres up. A top under its foot where
/// none is.
#[derive(Debug)]
struct Over {
    finder: Arc<Finder>,
    what: u32,
}

impl Kernel for Over {
    fn key(&self, key: &mut Key) {
        key.name("fin over").whole(self.what).whole(self.finder.id as u32);
        key.whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held = None;
        let jagged = self.finder.set.jagged;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let (mut foot, mut top) = (0.0f32, -1.0f32);
            let mut core = [0.0f32, -1.0f32];
            for fin in self.finder.near_held(x, z, &mut held).iter() {
                let far = fin.reach();
                if (x - fin.at[0]).abs() > far || (z - fin.at[1]).abs() > far {
                    continue;
                }
                let (along, across) = fin.local(x, z);
                let crest = fin.crest(along, jagged);
                if crest <= 0.0 {
                    continue;
                }
                // Up the column: where it is within the fin's reach across, and where well inside it from the foot.
                let (mut reach, mut solid) = (f32::MIN, [f32::MAX, f32::MIN]);
                let mut up = 0.0;
                while up <= crest * (1.0 + jagged * 0.35) + 1.0 {
                    let half = 0.6 + (fin.thick - 0.6) * (1.0 - up / crest).max(0.0).sqrt();
                    let off = (across - fin.lean * up).abs();
                    if off <= half + 1.5 {
                        reach = fin.foot + up;
                    }
                    // From where the column first runs well inside it, for as long as it stays so.
                    let within = up < crest - 1.0 && off <= half - 1.0;
                    let ended = solid[1] > solid[0] && solid[1] < fin.foot + up - 1.0;
                    if within && !ended {
                        solid = [solid[0].min(fin.foot + up), fin.foot + up];
                    }
                    up += 1.0;
                }
                if reach == f32::MIN {
                    continue;
                }
                let none = top < foot;
                foot = if none || fin.foot < foot { fin.foot } else { foot };
                top = if none || reach + 2.0 > top { reach + 2.0 } else { top };
                if solid[1] > solid[0] && solid[1] - solid[0] > core[1] - core[0] {
                    core = solid;
                }
            }
            *out = match self.what {
                0 => foot,
                1 => top,
                2 => core[1],
                _ => core[0],
            };
        }
    }
}

/// The ground with its fins, from rows of how far into the ground each
/// cell is, its x, y and z, and the heights its column's fins reach
/// between.
#[derive(Debug)]
struct Bladed {
    finder: Arc<Finder>,
}

impl Kernel for Bladed {
    fn key(&self, key: &mut Key) {
        key.name("fins").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let mut held = None;
        let jagged = self.finder.set.jagged;
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, y) = (inputs[0][at], inputs[2][at]);
            *out = ground;
            let (foot, top) = (inputs[4][at], inputs[5][at]);
            if top <= foot || y < foot - 2.0 || y > top + 2.0 {
                continue;
            }
            let (x, z) = (inputs[1][at], inputs[3][at]);
            for fin in self.finder.near_held(x, z, &mut held).iter() {
                let far = fin.reach();
                if (x - fin.at[0]).abs() > far || (z - fin.at[1]).abs() > far || y < fin.foot - 2.0 {
                    continue;
                }
                let inside = fin.inside(x, y, z, jagged);
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

    fn world(fins: Fins) -> WorldGen {
        let mut graph = Graph::default();
        let hills =
            graph.add(NodeKind::Noise(Noise { size: 400.0, low: 0.0, high: 20.0, ..Noise::default() }), [0.0; 2]);
        let fins = graph.add(NodeKind::Fins(fins), [200.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(hills, 0, output, Output::HEIGHT) && graph.connect(fins, 0, output, Output::VOLUME));
        let world = WorldGen::new(4, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    #[test]
    fn fins_stand_thin_and_tilted_in_rows_running_alike() {
        let world = world(Fins { often: 0.7, ..Fins::default() });
        let volume = world.volume().expect("a Volume");
        assert!(volume.spanned());
        let (side, step) = (250usize, 4.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 500.0, (at / side) as f32 * step)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let standing = volume.standing(&xs, &zs, &heights);
        let over: Vec<usize> = (0..xs.len()).filter(|&at| !standing[at].is_empty()).collect();
        let share = over.len() as f32 / xs.len() as f32;
        assert!(share > 0.01 && share < 0.35, "fins stand over {share} of the land");
        let inside = |at: usize, ys: &[f32]| {
            let carried = volume.carried(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
            let (mut inside, mut ground) = (vec![0.0; ys.len()], vec![0.0; ys.len()]);
            let cells = Asked { x: &xs[at..=at], y: ys, z: &zs[at..=at], carried: &carried.column(0), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            inside
        };
        let mut tall = 0;
        for &at in over.iter().step_by(3) {
            let [foot, top] = standing[at][0];
            let ys: Vec<f32> = (0..6).map(|step| foot + 0.5 + (top - foot - 1.0) * step as f32 / 5.0).collect();
            let read = inside(at, &ys);
            assert!(read.iter().all(|&inside| inside > 0.0), "{read:?} up a fin from {foot} to {top}");
            tall += usize::from(top - heights[at] > 12.0);
        }
        assert!(tall > 0, "no fin stands 12 m over the ground");
        // Thin: along a row of columns across the land, the fins are crossed in short runs, not wide ones.
        let mut runs = Vec::new();
        for row in (0..side).step_by(10) {
            let mut run = 0;
            for x in 0..side {
                let at = row * side + x;
                if !standing[at].is_empty() && standing[at][0][1] - heights[at] > 6.0 {
                    run += 1;
                } else if run > 0 {
                    runs.push(run);
                    run = 0;
                }
            }
        }
        runs.sort_unstable();
        let middle = runs.get(runs.len() / 2).copied().unwrap_or(0);
        assert!(!runs.is_empty() && middle <= 8, "runs across fins, 4 m a column: {runs:?}");
    }

    #[test]
    fn a_fin_leans_the_way_of_its_dip() {
        let fin = Fin {
            at: [0.0, 0.0],
            along: [1.0, 0.0],
            half: 40.0,
            foot: 0.0,
            tall: 40.0,
            thick: 4.0,
            lean: 0.5,
            seed: 3,
        };
        // Its middle at its foot is under it, and twenty metres up it has moved ten across.
        assert!(fin.inside(0.0, 1.0, 0.0, 0.0) > 0.0);
        assert!(fin.inside(0.0, 20.0, 10.0, 0.0) > 0.0 && fin.inside(0.0, 20.0, 0.0, 0.0) < 0.0);
        assert!(fin.inside(0.0, 20.0, -10.0, 0.0) < 0.0);
        // Nothing past its ends or over its top.
        assert!(fin.inside(45.0, 2.0, 0.0, 0.0) < 0.0 && fin.inside(0.0, 45.0, 22.5, 0.0) < 0.0);
    }
}
