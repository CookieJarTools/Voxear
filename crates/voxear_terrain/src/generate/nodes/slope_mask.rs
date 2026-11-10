//! The Slope Mask node: 1 where the ground wired in is between Min and Max
//! steep, easing to nothing over Feather outside each. It says where
//! something belongs by how steep the ground is: rock on cliffs, grass on
//! what is level enough to hold it.
//!
//! The slope is read from the height Step metres off each way, so four
//! more times than the place itself: everything behind the node is worked
//! out at each. A longer Step reads the lie of the land and passes over
//! small bumps.
//!
//! Its angles are kept as rise over run, and its feathered edges turned
//! from them with sums and products, so no place needs trigonometry. Round
//! a planet the four places lie along two tangents of its surface.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// A mask of slopes from `min` to `max`, eased in over `feather` outside
/// them, each an angle kept as its rise over run; the slope read from the
/// ground `step` metres off each way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlopeMask {
    pub min: f32,
    pub max: f32,
    pub feather: f32,
    pub step: f32,
}

impl Default for SlopeMask {
    fn default() -> Self {
        // From 30° to straight up, with 5° of feather.
        Self { min: 0.577_350_26, max: super::STEEPEST, feather: 0.087_488_66, step: 4.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::angle("min", "Min"),
    ValueSpec::angle("max", "Max"),
    ValueSpec::angle("feather", "Feather"),
    ValueSpec::number("step", "Step", 0.01, f32::MAX, "m").advanced(),
];

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::field("mask", "Mask")];

impl Kind for SlopeMask {
    const TITLE: &'static str = "Slope Mask";
    const FOLDER: &'static str = "Masks";

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
            0 => Value::Number(self.min),
            1 => Value::Number(self.max),
            2 => Value::Number(self.feather),
            3 => Value::Number(self.step),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.min = number,
            (1, Value::Number(number)) => self.max = number,
            (2, Value::Number(number)) => self.feather = number,
            (3, Value::Number(number)) => self.step = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let step = (self.step * cx.world.scale.max(0.01)).max(0.01);
        let off = around::four(cx, at, step);
        let heights = off.map(|from| cx.input(node, 0, from).unwrap_or_else(|| cx.constant(0.0)));
        cx.run(Steep { slopes: around::Slopes::new(self.min, self.max, self.feather), step }, &heights)
    }
}

/// How much of a mask of slopes covers the ground, from rows of its height
/// `step` metres off: further along one way, back along it, further along
/// the other, back along it.
#[derive(Debug)]
struct Steep {
    slopes: around::Slopes,
    step: f32,
}

impl Kernel for Steep {
    fn key(&self, key: &mut Key) {
        key.name("slope mask").number(self.step);
        self.slopes.key(key);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let across = 2.0 * self.step;
        for (at, out) in out.iter_mut().enumerate() {
            let (one, other) = ((inputs[0][at] - inputs[1][at]) / across, (inputs[2][at] - inputs[3][at]) / across);
            *out = self.slopes.cover((one * one + other * other).sqrt());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Constant, Falloff, Noise, rise_of, testing},
    };

    #[test]
    fn a_slope_mask_covers_ground_as_steep_as_it_says() {
        // A cone: 1000 m high at its middle, falling evenly to nothing 1000 m out, so 45° all the way down.
        // Scaled to 300 m high it falls at 16.7°, to 2000 m at 63.4°.
        let cone = |height: f32, mask: SlopeMask| {
            let mut graph = crate::generate::Graph::default();
            let cone = graph.add(NodeKind::Falloff(Falloff { radius: 0.0, feather: 1000.0, ..Falloff::default() }), [0.0; 2]);
            // A falloff eases; a straight curve from its distance would be a cone. Its middle stretch is steepest:
            // 1.5 times the even slope half way down.
            let high = graph.add(
                NodeKind::Math(crate::generate::nodes::Math {
                    operation: crate::generate::nodes::Operation::Multiply,
                    b: height,
                    ..Default::default()
                }),
                [0.0; 2],
            );
            let masked = graph.add(NodeKind::SlopeMask(mask), [0.0; 2]);
            let output = graph.add(NodeKind::Output(Default::default()), [0.0; 2]);
            assert!(graph.connect(cone, 0, high, 0) && graph.connect(high, 0, masked, 0));
            assert!(graph.connect(masked, 0, output, 0));
            crate::generate::WorldGen::new(1, 1.0, 0.0, graph)
        };
        let from = |low: f32, high: f32| SlopeMask { min: rise_of(low), max: rise_of(high), ..SlopeMask::default() };
        // Half way down, the hill 400 m high falls at 1.5 × 0.4 = 0.6, about 31°.
        let world = cone(400.0, from(25.0, 90.0));
        assert_eq!(testing::checked(&world), (0.0, 1.0));
        assert_eq!(world.height_at(500.0, 0.0), 1.0, "31° is within 25° to 90°");
        assert_eq!(world.height_at(0.0, -500.0), 1.0, "the same all round");
        assert_eq!(world.height_at(353.6, 353.6), 1.0);
        assert_eq!(world.height_at(0.0, 0.0), 0.0, "level at its top");
        assert_eq!(world.height_at(1500.0, 300.0), 0.0, "and on the level ground beyond");
        assert_eq!(cone(400.0, from(40.0, 90.0)).height_at(500.0, 0.0), 0.0, "31° is under 40° less its feather");
        assert_eq!(cone(400.0, from(0.0, 20.0)).height_at(500.0, 0.0), 0.0, "and over 20° and its feather");
        let edge = cone(400.0, from(33.0, 90.0)).height_at(500.0, 0.0);
        assert!(edge > 0.1 && edge < 0.9, "31° is in the feather under 33°: {edge}");
        assert_eq!(cone(400.0, from(0.0, 20.0)).height_at(0.0, 0.0), 1.0, "level ground is within 0° to 20°");
        // Twice the scale, the same hill twice as wide is half as steep.
        assert_eq!(world.with_scale(2.0).height_at(1000.0, 0.0), 0.0, "0.3, about 17°");
        // Round a planet the hill is over its north pole, and as steep along its surface.
        assert_eq!(world.height_round([0.0, 3000.0, 0.0]), 0.0);
        assert_eq!(world.height_round([497.7, 2958.4, 0.0]), 1.0, "500 m from the pole");
        assert_eq!(world.height_round([0.0, 2958.4, -497.7]), 1.0);
        assert_eq!(world.height_round([3000.0, 0.0, 0.0]), 0.0);
        // Hills of noise: steep in places and level in others, and the same bits in any batch round a planet too.
        let hills = Noise { size: 120.0, low: 0.0, high: 60.0, ..Noise::default() };
        let rough = testing::world_of(NodeKind::SlopeMask(from(20.0, 90.0)), 0, &[Some(NodeKind::Noise(hills))]);
        assert_eq!(testing::checked(&rough), (0.0, 1.0));
        assert_eq!(testing::round_range(&rough), (0.0, 1.0));
        // Level ground has no slope, and nothing wired in is level.
        let level = testing::world_of(NodeKind::SlopeMask(from(5.0, 90.0)), 0, &[Some(NodeKind::Constant(Constant { value: 9.0 }))]);
        assert_eq!((level.height_at(1.0, 1.0), level.height_round([0.0, 0.0, 800.0])), (0.0, 0.0));
        assert_eq!(testing::world_of(NodeKind::SlopeMask(from(0.0, 90.0)), 0, &[]).height_at(1.0, 1.0), 1.0);
    }
}
