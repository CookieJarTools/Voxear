//! The Overhangs node: the ground's surface pushed in and out by noise
//! through space, so cliffs lean, bulge and overhang. Amount is how far it
//! pushes at most, in metres, and Size how far apart the bulges are, with
//! Octaves finer ones over them, each half as large. Ground hangs over air
//! once Amount is about two thirds of Size or more; under that it leans.
//!
//! It pushes through a mask, from 0 for ground left as it is to 1. With
//! nothing wired into the mask it pushes steep ground alone: where the
//! generator's own ground climbs steeper than about 30°, and wholly past
//! about 45°, so hillsides stay as their height made them.
//!
//! Cells too large to hold a bulge are not pushed by it: an octave fades
//! as the cells asked about pass a quarter of its size and is gone at half,
//! so the far levels of a world of blocks keep a plain surface rather than
//! one broken at random.
//!
//! Its sizes are in metres whatever the generator's scale, which stretches
//! the ground sideways only.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Solid, Value, ValueSpec, noise3d::seed_for};
use crate::{
    generate::{
        compile::{Cells, Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::gradient3,
};

/// The ground's climb, as rise over run, where the mask of steep ground
/// starts and where it is whole.
const STEEP_FROM: f32 = 0.55;
const STEEP_WHOLE: f32 = 1.0;

/// How far from a place its ground's climb is measured, for its size.
const CLIMB_REACH: f32 = 0.4;

/// How many cells further into or out of the ground than it can push the
/// push is left out: nothing a cell or those beside it hold changes there.
const BEYOND: f32 = 3.0;

/// The ground pushed in and out as far as `amount` metres by noise `size`
/// metres wide in `octaves` octaves. With `seed` above 0 the noise is
/// seeded that much past the generator's own seed rather than from the
/// node's number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overhangs {
    pub amount: f32,
    pub size: f32,
    pub octaves: u32,
    pub seed: u32,
}

impl Default for Overhangs {
    fn default() -> Self {
        Self { amount: 6.0, size: 14.0, octaves: 2, seed: 0 }
    }
}

impl Overhangs {
    /// Its inputs, among its inputs, and its amount, among its values.
    pub const GROUND: usize = 0;
    pub const MASK: usize = 1;
    pub const AMOUNT: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("amount", "Amount", 0.0, 1000.0, "m"),
    ValueSpec::number("size", "Size", 1.0, 10000.0, "m"),
    ValueSpec::whole("octaves", "Octaves", 1, 6),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground"), Port::field("mask", "Mask")];

const OUTPUTS: &[Port] = &[Port::volume("ground", "Ground")];

impl Kind for Overhangs {
    const TITLE: &'static str = "Overhangs";
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
            0 => Value::Number(self.amount),
            1 => Value::Number(self.size),
            2 => Value::Whole(self.octaves),
            3 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.amount = number,
            (1, Value::Number(number)) => self.size = number,
            (2, Value::Whole(whole)) => self.octaves = whole,
            (3, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        // Pushing by nothing leaves the ground the very row it was, a plain height still where it was one.
        if self.amount.is_nan() || self.amount <= 0.0 {
            return ground;
        }
        let size = self.size.max(1.0);
        let mask = match cx.field(node, Self::MASK, at) {
            Some(mask) => mask,
            None => cx.column_between(at, |cx, places| steep(cx, places, size * CLIMB_REACH)),
        };
        let push =
            Push { amount: self.amount, size, octaves: self.octaves.clamp(1, 6), seed: seed_for(cx, node, self.seed) };
        let cell = cx.size();
        let row = cx.run(push, &[ground, mask, at.x, at.y, at.z, cell]);
        cx.set_rise(row, cx.rise(ground) + self.amount);
        row
    }
}

/// A mask of where the generator's own ground is steep at `at`: from its
/// height at four places `reach` metres either side of each along two
/// ways, whose difference is the ground's climb there.
fn steep(cx: &mut Compiler, at: Places, reach: f32) -> V {
    let heights = [0, 1, 2, 3].map(|corner| {
        let moved = Places {
            x: cx.run(Moved(reach * ROUND[corner][0]), &[at.x]),
            z: cx.run(Moved(reach * ROUND[corner][1]), &[at.z]),
        };
        cx.ground_at(moved)
    });
    cx.run(Steep { reach }, &heights)
}

/// Four ways a quarter turn apart: how far along two axes each goes.
const ROUND: [[f32; 2]; 4] = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]];

/// A row moved by so much.
#[derive(Debug)]
struct Moved(f32);

impl Kernel for Moved {
    fn key(&self, key: &mut Key) {
        key.name("moved").number(self.0);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, at) in out.iter_mut().zip(inputs[0]) {
            *out = at + self.0;
        }
    }
}

/// How steep the ground is, 0 to 1, from rows of its height at four
/// places `reach` metres from each place: either side of it along one way,
/// then along the other.
#[derive(Debug)]
struct Steep {
    reach: f32,
}

impl Kernel for Steep {
    fn key(&self, key: &mut Key) {
        key.name("steep").number(self.reach).number(STEEP_FROM).number(STEEP_WHOLE);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let along = (inputs[0][at] - inputs[1][at]) / (2.0 * self.reach);
            let across = (inputs[2][at] - inputs[3][at]) / (2.0 * self.reach);
            let climb = (along * along + across * across).sqrt();
            let t = ((climb - STEEP_FROM) / (STEEP_WHOLE - STEEP_FROM)).clamp(0.0, 1.0);
            *out = t * t * (3.0 - 2.0 * t);
        }
    }
}

/// The ground pushed in and out, from rows of how far into it each cell
/// is, the mask, the cell's x, y and z, and the cells' size.
#[derive(Debug)]
struct Push {
    amount: f32,
    size: f32,
    octaves: u32,
    seed: u32,
}

impl Kernel for Push {
    fn key(&self, key: &mut Key) {
        key.name("overhangs").number(self.amount).number(self.size).whole(self.octaves).whole(self.seed);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let (ground, mask, cell) = (inputs[0][at], inputs[1][at], inputs[5][at]);
            *out = ground;
            let beyond = self.amount + BEYOND * cell;
            if mask <= 0.0 || mask.is_nan() || ground >= beyond || ground <= -beyond {
                continue;
            }
            let (x, y, z) = (inputs[2][at], inputs[3][at], inputs[4][at]);
            let (mut sum, mut total, mut strength, mut size) = (0.0, 0.0, 1.0, self.size);
            for octave in 0..self.octaves {
                // An octave fades as the cells pass a quarter of its size, and is gone at half.
                let held = if cell > 0.0 { ((size / cell - 2.0) * 0.5).clamp(0.0, 1.0) } else { 1.0 };
                if held > 0.0 {
                    let seed = self.seed.wrapping_add(octave.wrapping_mul(0x632B_E5AB));
                    sum += gradient3(x / size, y / size, z / size, seed) * strength * held;
                }
                total += strength;
                strength *= 0.5;
                size *= 0.5;
            }
            let noise = (sum / total).clamp(-1.0, 1.0);
            *out = ground + noise * self.amount * mask.clamp(0.0, 1.0);
        }
    }
}
