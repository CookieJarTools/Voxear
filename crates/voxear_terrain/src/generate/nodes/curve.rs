//! The Curve node: what is wired in, read through a curve. From and To say
//! which numbers lie at the curve's two ends; the curve's own values are
//! what comes out, passing through its keys smoothly and never past them
//! (see `landmass::smooth`).

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
    landmass::smooth,
};

/// A curve read from `low` at its start to `high` at its end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Remap {
    pub curve: Curve,
    pub low: f32,
    pub high: f32,
}

impl Default for Remap {
    fn default() -> Self {
        Self { curve: Curve::new(&[[0.0, 0.0], [1.0, 1.0]]), low: 0.0, high: 1.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::curve("curve", "Curve", 0.0),
    ValueSpec::number("low", "From", f32::MIN, f32::MAX, ""),
    ValueSpec::number("high", "To", f32::MIN, f32::MAX, ""),
];

const INPUTS: &[Port] = &[Port::field("in", "In")];

const OUTPUTS: &[Port] = &[Port::field("out", "Out")];

impl Kind for Remap {
    const TITLE: &'static str = "Curve";
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
            0 => Value::Curve(self.curve),
            1 => Value::Number(self.low),
            2 => Value::Number(self.high),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Curve(curve)) => self.curve = curve,
            (1, Value::Number(number)) => self.low = number,
            (2, Value::Number(number)) => self.high = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let read = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(0.0));
        cx.run(Through { curve: self.curve, low: self.low, span: self.high - self.low }, &[read])
    }
}

/// A row read through a curve, `low` at its start and `low + span` at its
/// end.
#[derive(Debug)]
struct Through {
    curve: Curve,
    low: f32,
    span: f32,
}

impl Kernel for Through {
    fn key(&self, key: &mut Key) {
        key.name("curve").number(self.low).number(self.span).whole(self.curve.keys().len() as u32);
        for [place, value] in self.curve.keys() {
            key.number(*place).number(*value);
        }
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, &read) in out.iter_mut().zip(inputs[0]) {
            let along = if self.span == 0.0 { 0.0 } else { (read - self.low) / self.span };
            *out = smooth(&self.curve, along, 0.0);
        }
    }
}
