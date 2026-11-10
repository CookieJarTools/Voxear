//! The Blend node: A where its mask is nothing, B where it is 1, and a mix
//! of the two between. Each is what is wired in, or the node's own value
//! with no wire.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// What a Blend node's inputs are worth with nothing wired into them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Blend {
    pub a: f32,
    pub b: f32,
    pub mask: f32,
}

impl Default for Blend {
    fn default() -> Self {
        Self { a: 0.0, b: 1.0, mask: 0.5 }
    }
}

impl Blend {
    /// Its inputs, among its inputs.
    pub const A: usize = 0;
    pub const B: usize = 1;
    pub const MASK: usize = 2;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("a", "A", f32::MIN, f32::MAX, "").for_input(0),
    ValueSpec::number("b", "B", f32::MIN, f32::MAX, "").for_input(1),
    ValueSpec::number("mask", "Mask", 0.0, 1.0, "").for_input(2),
];

const INPUTS: &[Port] = &[Port::field("a", "A"), Port::field("b", "B"), Port::field("mask", "Mask")];

const OUTPUTS: &[Port] = &[Port::field("result", "Result")];

impl Kind for Blend {
    const TITLE: &'static str = "Blend";
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
            0 => Value::Number(self.a),
            1 => Value::Number(self.b),
            2 => Value::Number(self.mask),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.a = number,
            (1, Value::Number(number)) => self.b = number,
            (2, Value::Number(number)) => self.mask = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let a = cx.input(node, Self::A, at).unwrap_or_else(|| cx.constant(self.a));
        let b = cx.input(node, Self::B, at).unwrap_or_else(|| cx.constant(self.b));
        let mask = cx.input(node, Self::MASK, at).unwrap_or_else(|| cx.constant(self.mask));
        cx.run(Mixed, &[a, b, mask])
    }
}

/// Two rows mixed by a third, held from 0 to 1.
#[derive(Debug)]
struct Mixed;

impl Kernel for Mixed {
    fn key(&self, key: &mut Key) {
        key.name("blend");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let (a, b) = (inputs[0][at], inputs[1][at]);
            *out = a + (b - a) * inputs[2][at].clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Constant, Noise, testing},
    };

    #[test]
    fn a_blend_is_one_input_where_its_mask_is_nothing_and_the_other_where_it_is_one() {
        let constant = |value: f32| Some(NodeKind::Constant(Constant { value }));
        let mask = Noise { size: 250.0, low: -2.0, high: 3.0, ..Noise::default() };
        let blend = NodeKind::Blend(Blend::default());
        let world =
            testing::world_of(blend.clone(), 0, &[constant(-30.0), constant(50.0), Some(NodeKind::Noise(mask))]);
        // A mask past either end is held to it, so the ground never leaves what lies between its two inputs.
        assert_eq!(testing::checked(&world), (-30.0, 50.0));
        let heights = testing::flat(&world, 4000);
        assert!(heights.iter().any(|&height| height > -20.0 && height < 40.0), "and mixes them between");
        assert_eq!(testing::round_range(&world), (-30.0, 50.0), "the same round a planet");
        // With nothing wired in, its own values: half way from A to B.
        let own = Blend { a: 10.0, b: 30.0, mask: 0.25 };
        assert_eq!(testing::world_of(NodeKind::Blend(own), 0, &[]).height_at(5.0, 5.0), 15.0);
        let half = testing::world_of(blend, 0, &[constant(4.0), constant(8.0)]);
        assert_eq!((half.height_at(1.0, 2.0), half.height_round([0.0, 900.0, 0.0])), (6.0, 6.0));
    }
}
