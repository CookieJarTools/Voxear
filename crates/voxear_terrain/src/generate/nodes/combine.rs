//! The Combine node: two Volumes made one. **Union** is solid wherever
//! either is. **Subtract** takes B out of A: solid where A is and B is
//! not. **Smooth** is a union whose two parts run into one another over
//! Blend metres rather than meeting at a crease.
//!
//! What a Subtract takes away counts as caves: far levels of a world of
//! blocks are made without it, a sea does not flood it, and the ground's
//! layers run through it as if it were not there.
//!
//! Union and Subtract take the greater or the lesser of two numbers and
//! nothing more, so what they make at a place never depends on anything
//! but the two Volumes there. A node that gathers many separate things of
//! its own (rooms, tunnels) joins them the same plain way, since blending
//! them would let those out of reach of a chunk change it.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Cells, Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// A Volume's number where there is nothing at all: further out of the
/// ground than anything is.
pub(super) const NOTHING: f32 = -1.0e9;

/// How a Combine node makes two Volumes one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combining {
    #[default]
    Union,
    Subtract,
    Smooth,
}

impl Combining {
    pub const ALL: [Combining; 3] = [Combining::Union, Combining::Subtract, Combining::Smooth];

    pub fn name(self) -> &'static str {
        match self {
            Combining::Union => "Union",
            Combining::Subtract => "Subtract",
            Combining::Smooth => "Smooth",
        }
    }
}

/// Two Volumes made one by `operation`, a Smooth one over `blend` metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Combine {
    pub operation: Combining,
    pub blend: f32,
}

impl Default for Combine {
    fn default() -> Self {
        Self { operation: Combining::Union, blend: 4.0 }
    }
}

impl Combine {
    /// Its inputs, among its inputs.
    pub const A: usize = 0;
    pub const B: usize = 1;

    /// A Combine node that takes B out of A.
    pub fn subtract() -> Self {
        Self { operation: Combining::Subtract, ..Self::default() }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("operation", "Operation", &["Union", "Subtract", "Smooth"]),
    ValueSpec::number("blend", "Blend", 0.0, 1000.0, "m"),
];

const INPUTS: &[Port] = &[Port::volume("a", "A"), Port::volume("b", "B")];

const OUTPUTS: &[Port] = &[Port::volume("result", "Result")];

impl Kind for Combine {
    const TITLE: &'static str = "Combine";
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
            0 => Value::Choice(Combining::ALL.iter().position(|&operation| operation == self.operation).unwrap_or(0)),
            1 => Value::Number(self.blend),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.operation = Combining::ALL[choice.min(Combining::ALL.len() - 1)],
            (1, Value::Number(number)) => self.blend = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let a = cx.volume(node, Self::A, at);
        // The ground under the caves is what they are taken from.
        let taking = self.operation == Combining::Subtract;
        let b = if taking && !cx.carving() { None } else { cx.volume(node, Self::B, at) };
        match (a, b) {
            (Some(a), Some(b)) => {
                let row = cx.run(Join { operation: self.operation, blend: self.blend.max(0.0) }, &[a, b]);
                let rise = match self.operation {
                    Combining::Union => cx.rise(a).max(cx.rise(b)),
                    Combining::Subtract => cx.rise(a),
                    Combining::Smooth => cx.rise(a).max(cx.rise(b)) + self.blend.max(0.0) * 0.25,
                };
                cx.set_rise(row, rise);
                row
            }
            (Some(a), None) => a,
            (None, Some(b)) if !taking => b,
            _ => cx.constant(NOTHING),
        }
    }
}

/// Two rows made one.
#[derive(Debug)]
struct Join {
    operation: Combining,
    blend: f32,
}

impl Kernel for Join {
    fn key(&self, key: &mut Key) {
        key.name("combine").text(self.operation.name());
        if self.operation == Combining::Smooth {
            key.number(self.blend);
        }
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let pairs = out.iter_mut().zip(inputs[0].iter().zip(inputs[1]));
        let blend = self.blend;
        // The greater and the lesser by comparing, which says the same on every machine.
        match self.operation {
            Combining::Union => pairs.for_each(|(out, (&a, &b))| *out = if a > b { a } else { b }),
            Combining::Subtract => pairs.for_each(|(out, (&a, &b))| *out = if a < -b { a } else { -b }),
            Combining::Smooth => pairs.for_each(|(out, (&a, &b))| {
                let (greater, apart) = if a > b { (a, a - b) } else { (b, b - a) };
                // Where the two are within the blend of one another, a little more than either.
                let near = if apart < blend { (blend - apart) / blend } else { 0.0 };
                *out = greater + near * near * blend * 0.25;
            }),
        }
    }
}
