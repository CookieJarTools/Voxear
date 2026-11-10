//! The Terrace node: what is wired in cut into steps, each Step high. A
//! step is a level tread and a riser up to the next; Sharpness says how
//! much of each step is tread. With none the ground is left as it was, with
//! all of it the steps are level with sheer risers between.
//!
//! A tread lies at each whole number of steps, so a Step of 20 has treads
//! at 0, 20 and 40. Add something to the height before it and take it away
//! after to move them.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// Steps `step` high, as much of each level as `sharpness` says, 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terrace {
    pub step: f32,
    pub sharpness: f32,
}

impl Default for Terrace {
    fn default() -> Self {
        Self { step: 10.0, sharpness: 0.8 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("step", "Step", 0.001, f32::MAX, "m"),
    ValueSpec::number("sharpness", "Sharpness", 0.0, 1.0, ""),
];

const INPUTS: &[Port] = &[Port::field("in", "In")];

const OUTPUTS: &[Port] = &[Port::field("out", "Out")];

impl Kind for Terrace {
    const TITLE: &'static str = "Terrace";
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
            0 => Value::Number(self.step),
            1 => Value::Number(self.sharpness),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.step = number,
            (1, Value::Number(number)) => self.sharpness = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let read = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(0.0));
        let step = if self.step > 0.001 { self.step } else { 0.001 };
        cx.run(Steps { step, riser: 1.0 - self.sharpness.clamp(0.0, 1.0) }, &[read])
    }
}

/// A row cut into steps `step` high, the share `riser` of each rising
/// between two treads.
#[derive(Debug)]
struct Steps {
    step: f32,
    riser: f32,
}

impl Kernel for Steps {
    fn key(&self, key: &mut Key) {
        key.name("terrace").number(self.step).number(self.riser);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        // A tread reaches half of what is level either side of each whole step, the riser between two of them.
        let tread = (1.0 - self.riser) * 0.5;
        for (out, &read) in out.iter_mut().zip(inputs[0]) {
            let steps = read / self.step;
            let whole = steps.floor();
            let past = steps - whole;
            let risen = if past <= tread {
                0.0
            } else if past >= 1.0 - tread {
                1.0
            } else {
                (past - tread) / self.riser
            };
            *out = (whole + risen) * self.step;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Noise, testing},
    };

    #[test]
    fn a_terrace_cuts_ground_into_steps_and_never_leaves_its_step() {
        let hills = Noise { size: 300.0, low: -40.0, high: 90.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]);
        let terrace = |sharpness: f32| {
            let kind = NodeKind::Terrace(Terrace { step: 20.0, sharpness });
            testing::world_of(kind, 0, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (sheer, half, none) = (terrace(1.0), terrace(0.5), terrace(0.0));
        let (low, high) = testing::checked(&half);
        assert!(low >= -40.0 && high <= 100.0 && high - low > 60.0, "{low} to {high}");
        testing::checked(&sheer);
        // The same hills under each: a node's noise is its own by its number, and the hills are node 1 in each.
        let was = testing::flat(&source, 4000);
        let (cut, steps, left) = (testing::flat(&half, 4000), testing::flat(&sheer, 4000), testing::flat(&none, 4000));
        let mut level = 0;
        for i in 0..4000 {
            // Every place stays within the step it was in; all sharpness leaves whole steps and none the ground.
            let under = (was[i] / 20.0).floor() * 20.0;
            assert!(cut[i] >= under - 1e-3 && cut[i] <= under + 20.0 + 1e-3, "{} cut to {}", was[i], cut[i]);
            assert!((steps[i] / 20.0 - (steps[i] / 20.0).round()).abs() < 1e-4, "{} is no whole step", steps[i]);
            assert!((left[i] - was[i]).abs() < 1e-3, "{} was {}", left[i], was[i]);
            level += usize::from((cut[i] / 20.0 - (cut[i] / 20.0).round()).abs() < 1e-4);
        }
        assert!((1500..2500).contains(&level), "half of each step is tread: {level} of 4000 places");
        // A tread lies at each whole step, reaching a quarter of a step either side with half of it tread.
        let flat = |height: f32| {
            let mut out = [0.0];
            Steps { step: 20.0, riser: 0.5 }.run(&[&[height]], &mut out);
            out[0]
        };
        assert_eq!([flat(36.0), flat(40.0), flat(44.9), flat(50.0), flat(-3.0)], [40.0, 40.0, 40.0, 50.0, 0.0]);
        // Round a planet each place is cut as it is on flat ground: the hills read round it, in the same steps.
        let (was, cut) = (testing::round(&source, 2000), testing::round(&half, 2000));
        for i in 0..2000 {
            let mut out = [0.0];
            Steps { step: 20.0, riser: 0.5 }.run(&[&[was[i]]], &mut out);
            assert_eq!(cut[i].to_bits(), out[0].to_bits());
        }
    }
}
