//! The Chambers node: wide low caves, as a Volume of how many metres
//! inside one each place is. They lie where a noise through space is high,
//! flatter than they are wide; Amount says how much of the ground they
//! take, and Size how far apart they are.
//!
//! Taken out of the ground by a Combine node's Subtract, they hollow it.
//! They keep Roof metres under the ground wired in, as Tunnels do; with
//! nothing wired in, under the generator's own ground.
//!
//! Its sizes are in metres whatever the generator's scale, which stretches
//! the ground sideways only.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Solid, Value, ValueSpec, combine::NOTHING, noise3d::seed_for};
use crate::{
    generate::{
        compile::{Cells, Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::gradient3,
};

/// How high a chamber is for its width.
const FLAT: f32 = 0.6;

/// Chambers taking as much of the ground as `amount` says, from 0 for none
/// to 1, `size` metres apart, no nearer the ground's surface than `roof`
/// metres. With `seed` above 0 their noise is seeded that much past the
/// generator's own seed rather than from the node's number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chambers {
    pub amount: f32,
    pub size: f32,
    pub roof: f32,
    pub seed: u32,
}

impl Default for Chambers {
    fn default() -> Self {
        Self { amount: 0.3, size: 40.0, roof: 3.0, seed: 0 }
    }
}

impl Chambers {
    /// Its ground, among its inputs, and its amount, among its values.
    pub const GROUND: usize = 0;
    pub const AMOUNT: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("amount", "Amount", 0.0, 1.0, ""),
    ValueSpec::number("size", "Size", 1.0, 10000.0, "m"),
    ValueSpec::number("roof", "Roof", 0.0, 10000.0, "m"),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("chambers", "Chambers")];

impl Kind for Chambers {
    const TITLE: &'static str = "Chambers";
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
            2 => Value::Number(self.roof),
            3 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.amount = number,
            (1, Value::Number(number)) => self.size = number,
            (2, Value::Number(number)) => self.roof = number,
            (3, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let amount = self.amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return cx.constant(NOTHING);
        }
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        let hollow = Hollow {
            size: self.size.max(1.0),
            level: 0.62 - 0.22 * amount,
            roof: self.roof.max(0.0),
            seed: seed_for(cx, node, self.seed),
        };
        let size = cx.size();
        cx.run(hollow, &[ground, at.x, at.y, at.z, size])
    }
}

/// How far inside a chamber each cell is, from rows of how far into the
/// ground it is, its x, y and z, and the cells' size.
#[derive(Debug)]
struct Hollow {
    size: f32,
    /// How high the noise must be, inside a chamber.
    level: f32,
    roof: f32,
    seed: u32,
}

impl Kernel for Hollow {
    fn key(&self, key: &mut Key) {
        key.name("chambers").number(self.size).number(self.level).number(self.roof).whole(self.seed);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let c = self.size;
        for (at, out) in out.iter_mut().enumerate() {
            // Above the roof nothing is hollow, and a cell or more above it the noise changes nothing a cell holds.
            let under = inputs[0][at] - self.roof;
            if under <= -inputs[4][at] {
                *out = under;
                continue;
            }
            let high = gradient3(inputs[1][at] / c, inputs[2][at] / (c * FLAT), inputs[3][at] / c, self.seed);
            // The noise's distance from where a chamber starts reads as about half its size a unit.
            let inside = -((self.level - high) * c * 0.5);
            *out = if inside < under { inside } else { under };
        }
    }
}
