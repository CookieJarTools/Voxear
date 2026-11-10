//! The Tunnels node: long winding caves, as a Volume of how many metres
//! inside one each place is. They run where two noises through space are
//! both near nothing, which is along lines; Amount says how near, and so
//! how wide they are, and Size how far apart their bends lie.
//!
//! Taken out of the ground by a Combine node's Subtract, they hollow it.
//! They keep Roof metres under the ground wired in, so they open onto the
//! surface only here and there; with nothing wired in, under the
//! generator's own ground.
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

/// Tunnels as wide as `amount` says, from 0 for none to 1, winding over
/// `size` metres, no nearer the ground's surface than `roof` metres. With
/// `seed` above 0 their noise is seeded that much past the generator's own
/// seed rather than from the node's number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tunnels {
    pub amount: f32,
    pub size: f32,
    pub roof: f32,
    pub seed: u32,
}

impl Default for Tunnels {
    fn default() -> Self {
        Self { amount: 0.3, size: 22.0, roof: 3.0, seed: 0 }
    }
}

impl Tunnels {
    /// Its ground, among its inputs, and its amount, among its values.
    pub const GROUND: usize = 0;
    pub const AMOUNT: usize = 0;

    /// How much further on the second of its two noises is seeded.
    const SECOND: u32 = 14;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("amount", "Amount", 0.0, 1.0, ""),
    ValueSpec::number("size", "Size", 1.0, 10000.0, "m"),
    ValueSpec::number("roof", "Roof", 0.0, 10000.0, "m"),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("tunnels", "Tunnels")];

impl Kind for Tunnels {
    const TITLE: &'static str = "Tunnels";
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
        let seed = seed_for(cx, node, self.seed);
        let wind = Wind {
            size: self.size.max(1.0),
            width: 0.14 * amount.sqrt(),
            roof: self.roof.max(0.0),
            seeds: [seed, seed.wrapping_add(Self::SECOND)],
        };
        let size = cx.size();
        cx.run(wind, &[ground, at.x, at.y, at.z, size])
    }
}

/// How far inside a tunnel each cell is, from rows of how far into the
/// ground it is, its x, y and z, and the cells' size.
#[derive(Debug)]
struct Wind {
    size: f32,
    /// How near nothing the two noises must add up to, inside a tunnel.
    width: f32,
    roof: f32,
    seeds: [u32; 2],
}

impl Kernel for Wind {
    fn key(&self, key: &mut Key) {
        key.name("tunnels").number(self.size).number(self.width).number(self.roof);
        key.whole(self.seeds[0]).whole(self.seeds[1]);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let t = self.size;
        for (at, out) in out.iter_mut().enumerate() {
            // Above the roof nothing is hollow, and a cell or more above it the noise changes nothing a cell holds.
            let under = inputs[0][at] - self.roof;
            if under <= -inputs[4][at] {
                *out = under;
                continue;
            }
            let (x, y, z) = (inputs[1][at] / t, inputs[2][at] / t, inputs[3][at] / t);
            let apart = gradient3(x, y, z, self.seeds[0]).abs() + gradient3(x, y, z, self.seeds[1]).abs();
            // The noise's distance from where a tunnel starts reads as about half its size a unit.
            let inside = -((apart - self.width) * t * 0.5);
            *out = if inside < under { inside } else { under };
        }
    }
}
