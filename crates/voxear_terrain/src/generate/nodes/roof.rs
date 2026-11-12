//! The Roof node: ground laid back over low ground as a lid at the height
//! wired into Top, so a chasm becomes a cavern under land that looks like
//! any other, open to the sky where its roof has fallen in.
//!
//! Wire a Chasms node's Rim into Top, and the roof's top lies where the
//! ground was before the chasm was cut: walking over it, the land goes on
//! as it did, until a hole in it shows the floor far below. A roof is laid
//! only where Top is higher over the ground than the roof is thick, so it
//! meets a chasm's walls where they are shallow and leaves the land round
//! alone.
//!
//! Thickness is how thick the roof is. Cover is how much of the low ground
//! is roofed over, 0 to 1, in broad stretches, so some chasms are open and
//! some are caverns. Openings is how much of a roof has fallen in, 0 to 1,
//! in holes about Opening Size across. Pillars is how many columns of rock
//! stand from the floor up to the roof, 0 to 1.
//!
//! A roof's top is ground like any other: it takes its column's layers, so
//! it is grown over as the land round it is, and the floor under it takes
//! its own. A mask wired into Mask keeps the roof to where it is over a
//! half: a Chasms node's Floor and Walls, say, or a Height Mask.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Solid, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Cells, Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::{gradient, gradient3},
};

/// A roof `thickness` metres thick over as much of the low ground as
/// `cover` says, `openings` of it fallen in, in holes `opening_size`
/// metres across, with as many pillars under it as `pillars` says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Roof {
    pub thickness: f32,
    pub cover: f32,
    pub openings: f32,
    pub opening_size: f32,
    pub pillars: f32,
    pub seed: u32,
}

impl Default for Roof {
    fn default() -> Self {
        Self { thickness: 18.0, cover: 0.6, openings: 0.2, opening_size: 40.0, pillars: 0.3, seed: 0 }
    }
}

impl Roof {
    /// Its inputs.
    pub const GROUND: usize = 0;
    pub const TOP: usize = 1;
    pub const MASK: usize = 2;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("thickness", "Thickness", 2.0, 500.0, "m"),
    ValueSpec::number("cover", "Cover", 0.0, 1.0, ""),
    ValueSpec::number("openings", "Openings", 0.0, 1.0, ""),
    ValueSpec::number("opening_size", "Opening Size", 4.0, 2000.0, "m"),
    ValueSpec::number("pillars", "Pillars", 0.0, 1.0, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground"), Port::field("top", "Top"), Port::field("mask", "Mask")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for Roof {
    const TITLE: &'static str = "Roof";
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
            0 => Value::Number(self.thickness),
            1 => Value::Number(self.cover),
            2 => Value::Number(self.openings),
            3 => Value::Number(self.opening_size),
            4 => Value::Number(self.pillars),
            5 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.thickness = number,
            (1, Value::Number(number)) => self.cover = number,
            (2, Value::Number(number)) => self.openings = number,
            (3, Value::Number(number)) => self.opening_size = number,
            (4, Value::Number(number)) => self.pillars = number,
            (5, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        if self.cover <= 0.0 || self.cover.is_nan() {
            return ground;
        }
        let lid = Arc::new(Lid::new(self, seed_for(cx, node, self.seed)));
        // What each column's roof stands on and reaches to, made once a column.
        let column = |cx: &mut Compiler, what: u32| {
            let lid = lid.clone();
            cx.column(at, |cx, places| {
                let top = cx.input(node, Self::TOP, places).unwrap_or_else(|| cx.ground_at(places));
                let mask = cx.input(node, Self::MASK, places).unwrap_or_else(|| cx.constant(1.0));
                let floor = cx.ground_at(places);
                cx.run(Over { lid, what }, &[places.x, places.z, top, floor, mask])
            })
        };
        let (foot, top, core_foot, core_top) = (column(cx, 0), column(cx, 1), column(cx, 2), column(cx, 3));
        let (within, floor) = (column(cx, 4), cx.column(at, |cx, places| cx.ground_at(places)));
        cx.note_span(at, foot, top, false);
        cx.note_span(at, core_foot, core_top, true);
        let row = cx.run(Roofed { lid }, &[ground, at.x, at.y, at.z, foot, top, within, floor]);
        cx.set_rise(row, cx.rise(ground));
        cx.set_depth(row, cx.depth(ground));
        row
    }
}

/// A Roof node's settings, held to their ranges, and its seed.
#[derive(Debug)]
struct Lid {
    set: Roof,
    seed: u32,
    id: u64,
}

impl Lid {
    fn new(set: &Roof, seed: u32) -> Self {
        let mut set = set.clone();
        set.thickness = set.thickness.clamp(2.0, 500.0);
        set.cover = set.cover.clamp(0.0, 1.0);
        set.openings = if set.openings.is_nan() { 0.0 } else { set.openings.clamp(0.0, 1.0) };
        set.opening_size = set.opening_size.clamp(4.0, 2000.0);
        set.pillars = if set.pillars.is_nan() { 0.0 } else { set.pillars.clamp(0.0, 1.0) };
        let mut key = Key::default();
        key.name("roof").number(set.thickness).number(set.cover).number(set.openings).number(set.opening_size);
        key.number(set.pillars).whole(seed);
        Self { set, seed, id: key.hash() }
    }

    /// How far inside the roof's edges the column at `x`, `z` is, in
    /// metres along the ground, where its top is `top` and the ground
    /// under it `floor`: less than nothing where it has none. A roof lies
    /// where the land is low enough under its top, where its broad
    /// stretches cover, and not in its holes.
    fn within(&self, x: f32, z: f32, top: f32, floor: f32, mask: f32) -> f32 {
        let set = &self.set;
        if mask < 0.5 {
            return -1.0;
        }
        let under = top - floor - set.thickness - 4.0;
        let broad = set.opening_size * 12.0;
        let stretch = 0.5 + 0.5 * gradient(x / broad, z / broad, self.seed.wrapping_add(1));
        let covered = (set.cover - stretch) * broad * 0.3;
        // Holes where a pattern of their size is highest, their edges ragged.
        let size = set.opening_size;
        let hole = 0.5 + 0.5 * gradient(x / size, z / size, self.seed.wrapping_add(2));
        let hole = hole + 0.06 * gradient(x / 6.0, z / 6.0, self.seed.wrapping_add(3));
        let solid = (1.0 - set.openings * 1.15 - hole) * size * 0.8;
        under.min(covered).min(solid)
    }

    /// How thick the roof is at the column at `x`, `z`, before what hangs
    /// from its underside.
    fn thick(&self, x: f32, z: f32) -> f32 {
        let set = &self.set;
        set.thickness * (0.8 + 0.2 * gradient(x / 50.0, z / 50.0, self.seed.wrapping_add(4)))
    }

    /// How far down from the roof's underside rock hangs at `x`, `y`, `z`:
    /// knobs and points, longer where the roof is thick.
    fn hanging(&self, x: f32, y: f32, z: f32) -> f32 {
        let knob = gradient3(x / 9.0, y / 14.0, z / 9.0, self.seed.wrapping_add(5));
        self.set.thickness * 0.35 * knob.max(0.0)
    }

    /// The pillar near the column at `x`, `z`: how far inside it the
    /// column is, given how high over the floor the place is as a share of
    /// the way to the roof's underside. Less than nothing outside one.
    fn pillar(&self, x: f32, z: f32, up: f32) -> f32 {
        let set = &self.set;
        if set.pillars <= 0.0 {
            return -1.0;
        }
        let spacing = set.opening_size * 2.0;
        let cell = [(x / spacing).floor() as i32, (z / spacing).floor() as i32];
        let mut best = f32::MIN;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = [cell[0] + dx, cell[1] + dz];
                let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
                if pick(0) >= set.pillars {
                    continue;
                }
                let at = [
                    (cell[0] as f32 + 0.2 + 0.6 * pick(1)) * spacing,
                    (cell[1] as f32 + 0.2 + 0.6 * pick(2)) * spacing,
                ];
                let radius = 3.0 + 7.0 * pick(3);
                // Narrowest at its middle, flaring into the floor and the roof.
                let waist = (up - 0.5) * 2.0;
                let width = radius * (0.7 + 0.6 * waist * waist);
                let (ox, oz) = (x - at[0], z - at[1]);
                best = best.max(width - (ox * ox + oz * oz).sqrt());
            }
        }
        best
    }
}

/// For each column, from rows of its x and z, the roof's top there, the
/// ground under it and the mask: with `what` 0 and 1 the lowest and
/// highest any of the roof or a pillar may reach, and with 2 and 3 those of
/// where it is surely solid all the way, in metres up, a top under its
/// foot where none is; with 4 how far inside the roof's edges it is.
#[derive(Debug)]
struct Over {
    lid: Arc<Lid>,
    what: u32,
}

impl Kernel for Over {
    fn key(&self, key: &mut Key) {
        key.name("roof over").whole(self.what).whole(self.lid.id as u32).whole((self.lid.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let lid = &self.lid;
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z, top, floor, mask) = (inputs[0][at], inputs[1][at], inputs[2][at], inputs[3][at], inputs[4][at]);
            let within = lid.within(x, z, top, floor, mask);
            let thick = lid.thick(x, z);
            // A pillar reaches the column if it is within one at its widest.
            let pillar = within > -lid.set.opening_size && lid.pillar(x, z, 0.0) > -1.0;
            let (foot, high, core_foot, core_top) = match (within > -2.0, pillar) {
                (false, _) => (0.0, -1.0, 0.0, -1.0),
                (true, true) => (floor - 2.0, top + 1.0, floor - 1.0, top - 1.0),
                (true, false) => {
                    // Wherever it has any roof, the column is solid from its underside to its top.
                    let core = if within > 0.0 { (top - thick + 1.0, top - 1.0) } else { (0.0, -1.0) };
                    (top - thick * 1.4 - 2.0, top + 1.0, core.0, core.1)
                }
            };
            *out = match self.what {
                0 => foot,
                1 => high,
                2 => core_foot,
                3 => core_top,
                _ => within,
            };
        }
    }
}

/// The ground with its roof, from rows of how far into the ground each
/// cell is, its x, y and z, the heights its column's roof reaches between,
/// how far inside the roof's edges the column is, and the ground under it.
#[derive(Debug)]
struct Roofed {
    lid: Arc<Lid>,
}

impl Kernel for Roofed {
    fn key(&self, key: &mut Key) {
        key.name("roofed").whole(self.lid.id as u32).whole((self.lid.id >> 32) as u32);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let lid = &self.lid;
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, y) = (inputs[0][at], inputs[2][at]);
            *out = ground;
            let (foot, top) = (inputs[4][at], inputs[5][at]);
            if top <= foot || y < foot - 1.0 || y > top + 1.0 {
                continue;
            }
            let (x, z) = (inputs[1][at], inputs[3][at]);
            // The roof's top is where the column's span ends, a block over it.
            let roof_top = top - 1.0;
            let (within, floor) = (inputs[6][at], inputs[7][at]);
            let thick = lid.thick(x, z);
            let underside = roof_top - thick - lid.hanging(x, y, z);
            let mut inside = (roof_top - y).min(y - underside).min(within);
            // A pillar from the floor up into the roof, where the column has one.
            if floor < roof_top - thick && y < roof_top - thick * 0.5 {
                let up = ((y - floor) / (roof_top - thick - floor)).clamp(0.0, 1.0);
                inside = inside.max(lid.pillar(x, z, up));
            }
            if inside > *out {
                *out = inside;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Cells as Asked, Graph, NodeKind, WorldGen,
        nodes::{ChasmKind, Chasms, Constant, Output},
    };

    /// Flat land 100 m up cut by sinkholes, roofed over at the land's height.
    fn world(roof: Roof) -> WorldGen {
        let mut graph = Graph::default();
        let land = graph.add(NodeKind::Constant(Constant { value: 100.0 }), [0.0; 2]);
        let chasms =
            Chasms { shape: ChasmKind::Sinkholes, often: 0.7, width_from: 120.0, width_to: 260.0, ..Chasms::default() };
        let chasms = graph.add(NodeKind::Chasms(chasms), [200.0, 0.0]);
        let roof = graph.add(NodeKind::Roof(roof), [400.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [600.0, 0.0]);
        assert!(graph.connect(land, 0, chasms, Chasms::GROUND));
        assert!(graph.connect(chasms, Chasms::HEIGHT, output, Output::HEIGHT));
        assert!(graph.connect(chasms, Chasms::RIM, roof, Roof::TOP));
        assert!(graph.connect(roof, 0, output, Output::VOLUME));
        let world = WorldGen::new(4, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    #[test]
    fn a_roof_covers_a_chasm_at_the_lands_height_with_holes_down_to_its_floor() {
        let world = world(Roof { cover: 1.0, openings: 0.25, pillars: 0.0, ..Roof::default() });
        let volume = world.volume().expect("a Volume");
        let (side, step) = (200usize, 8.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 800.0, (at / side) as f32 * step - 800.0)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let deep: Vec<usize> = (0..xs.len()).filter(|&at| heights[at] < 100.0 - 60.0).collect();
        assert!(deep.len() > 100, "{} columns down in chasms", deep.len());
        let standing = volume.standing(&xs, &zs, &heights);
        let roofed: Vec<usize> = deep.iter().copied().filter(|&at| !standing[at].is_empty()).collect();
        let open = deep.len() - roofed.len();
        assert!(roofed.len() > deep.len() / 3 && open > deep.len() / 20, "{} roofed, {open} open", roofed.len());
        let inside = |at: usize, ys: &[f32]| {
            let carried = volume.carried(&xs[at..=at], &zs[at..=at], &heights[at..=at]);
            let (mut inside, mut ground) = (vec![0.0; ys.len()], vec![0.0; ys.len()]);
            let cells = Asked { x: &xs[at..=at], y: ys, z: &zs[at..=at], carried: &carried.column(0), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            inside
        };
        for &at in roofed.iter().step_by(11) {
            let [foot, top] = standing[at][0];
            assert!((top - 99.0).abs() < 2.5, "a roof's top at {top}, the land at 100");
            // The roof solid, the land open over it, and a cavern of air between it and the floor.
            let read = inside(at, &[top - 0.5, top + 1.5, (foot + heights[at]) * 0.5, heights[at] - 0.5]);
            assert!(read[0] > 0.0 && read[1] < 0.0 && read[2] < 0.0 && read[3] > 0.0, "{read:?}");
        }
        // Where the sky shows over a chasm, it is open all the way down to the floor.
        let mut holes = 0;
        for &at in deep.iter().filter(|&&at| standing[at].is_empty()).step_by(3) {
            let ys: Vec<f32> =
                (0..6).map(|step| heights[at] + 1.5 + (99.5 - heights[at]) * step as f32 / 5.0).collect();
            let read = inside(at, &ys);
            if read[5] < 0.0 {
                holes += 1;
                assert!(read.iter().all(|&inside| inside < 0.0), "a hole open to its floor: {ys:?} {read:?}");
            }
        }
        assert!(holes > 5, "{holes} holes");
    }

    #[test]
    fn pillars_stand_from_a_caverns_floor_up_to_its_roof() {
        let world = world(Roof { cover: 1.0, openings: 0.0, pillars: 1.0, ..Roof::default() });
        let volume = world.volume().expect("a Volume");
        let (side, step) = (200usize, 4.0f32);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * step - 400.0, (at / side) as f32 * step - 400.0)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let standing = volume.standing(&xs, &zs, &heights);
        // Columns of a pillar stand whole from the floor to the roof's top.
        let pillars = (0..xs.len())
            .filter(|&at| heights[at] < 40.0 && !standing[at].is_empty())
            .filter(|&at| standing[at][0][0] < heights[at] + 1.0)
            .count();
        assert!(pillars > 4, "{pillars} columns of pillar");
    }
}
