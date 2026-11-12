//! The Curvature node: where the ground wired in bends. **Ridges** is 1
//! along crests and on peaks, where the ground falls away either side;
//! **Valleys** is 1 along floors and in hollows, where it rises either
//! side. Both are nothing on level ground and on an even slope, however
//! steep.
//!
//! How sharply the ground bends is measured as an angle: how steeply it
//! falls away from a crest on each side, or rises from a floor. A mask is
//! whole from Bend up, and eases to nothing over Feather below it. Size is
//! how wide the crests and floors it finds are: the ground is read half of
//! it away each way, so four more times than the place itself.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// Masks of crests and floors `size` metres wide that bend by `bend` or
/// more, eased in over `feather` below it, each an angle kept as its rise
/// over run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curvature {
    pub size: f32,
    pub bend: f32,
    pub feather: f32,
}

impl Default for Curvature {
    fn default() -> Self {
        // Whole from 10°, with 10° of feather: from nothing on ground that does not bend.
        Self { size: 20.0, bend: 0.176_326_98, feather: 0.176_326_98 }
    }
}

impl Curvature {
    /// Its outputs, among its outputs.
    pub const RIDGES: usize = 0;
    pub const VALLEYS: usize = 1;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size", "Size", 0.01, f32::MAX, "m"),
    ValueSpec::angle("bend", "Bend"),
    ValueSpec::angle("feather", "Feather"),
];

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::field("ridges", "Ridges"), Port::field("valleys", "Valleys")];

impl Kind for Curvature {
    const TITLE: &'static str = "Curvature";
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
            0 => Value::Number(self.size),
            1 => Value::Number(self.bend),
            2 => Value::Number(self.feather),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size = number,
            (1, Value::Number(number)) => self.bend = number,
            (2, Value::Number(number)) => self.feather = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let Some(middle) = cx.input(node, 0, at) else { return cx.constant(0.0) };
        let step = (self.size * cx.world.scale.max(0.01) * 0.5).max(0.01);
        let [a, b, c, d] = around::four(cx, at, step).map(|from| cx.input(node, 0, from).unwrap_or(middle));
        let bent = Bent {
            valleys: port == Self::VALLEYS,
            slopes: around::Slopes::new(self.bend, super::STEEPEST, self.feather),
            step,
        };
        cx.run(bent, &[middle, a, b, c, d])
    }
}

/// How much of a mask of crests, or of floors, covers the ground: from
/// rows of its height at each place and `step` metres off it four ways.
#[derive(Debug)]
struct Bent {
    valleys: bool,
    slopes: around::Slopes,
    step: f32,
}

impl Kernel for Bent {
    fn key(&self, key: &mut Key) {
        key.name("curvature").whole(u32::from(self.valleys)).number(self.step);
        self.slopes.key(key);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        // How far the place stands over the four round it, on average of each pair across it, as a slope: the
        // slope of a crest's two sides, and twice that of a peak's.
        let across = 2.0 * self.step;
        for (at, out) in out.iter_mut().enumerate() {
            let round = inputs[1][at] + inputs[2][at] + inputs[3][at] + inputs[4][at];
            let over = (inputs[0][at] * 4.0 - round) / across;
            *out = self.slopes.cover(if self.valleys { -over } else { over });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Constant, Falloff, FalloffShape, Noise, rise_of, testing},
    };

    #[test]
    fn curvature_finds_crests_and_floors_and_nothing_on_an_even_slope() {
        let hills = Noise { size: 150.0, low: 0.0, high: 80.0, ..Noise::default() };
        let curvature = Curvature { size: 30.0, bend: rise_of(8.0), feather: rise_of(4.0) };
        let world = |port: usize| {
            testing::world_of(NodeKind::Curvature(curvature.clone()), port, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (ridges, valleys) = (world(Curvature::RIDGES), world(Curvature::VALLEYS));
        assert_eq!(testing::checked(&ridges), (0.0, 1.0));
        assert_eq!(testing::checked(&valleys), (0.0, 1.0));
        // No place is a crest and a floor at once, and some are neither.
        let (crests, floors) = (testing::flat(&ridges, 4000), testing::flat(&valleys, 4000));
        assert!((0..4000).all(|i| crests[i] == 0.0 || floors[i] == 0.0));
        let count = |masks: &[f32], of: f32| masks.iter().filter(|&&mask| mask == of).count();
        let neither = (0..4000).filter(|&i| crests[i] == 0.0 && floors[i] == 0.0).count();
        assert!(neither > 300 && count(&crests, 1.0) > 300 && count(&floors, 1.0) > 300, "{neither} neither");
        // A square falloff is a ridge with a level top, even sides and level ground beyond: bent only at its
        // shoulders and its feet. 400 m of feather eased over makes them gentle, so the masks are read finely.
        let ridge = Falloff { shape: FalloffShape::Square, radius: 100.0, feather: 400.0, ..Falloff::default() };
        let fine = Curvature { size: 100.0, bend: rise_of(2.0), feather: rise_of(1.0) };
        let ridge = |port: usize, height: f32| {
            let mut graph = crate::generate::Graph::default();
            let fall = graph.add(NodeKind::Falloff(ridge.clone()), [0.0; 2]);
            let high = graph.add(
                NodeKind::Math(crate::generate::nodes::Math {
                    operation: crate::generate::nodes::Operation::Multiply,
                    b: height,
                    ..Default::default()
                }),
                [0.0; 2],
            );
            let bent = graph.add(NodeKind::Curvature(fine.clone()), [0.0; 2]);
            let output = graph.add(NodeKind::Output(Default::default()), [0.0; 2]);
            assert!(graph.connect(fall, 0, high, 0) && graph.connect(high, 0, bent, 0));
            assert!(graph.connect(bent, port, output, 0));
            crate::generate::WorldGen::new(1, 1.0, 0.0, graph)
        };
        let (ridges, valleys) = (ridge(Curvature::RIDGES, 300.0), ridge(Curvature::VALLEYS, 300.0));
        let at = |world: &crate::generate::WorldGen, x: f32| world.height_at(x, 0.0);
        assert_eq!((at(&ridges, 0.0), at(&valleys, 0.0)), (0.0, 0.0), "its level top");
        assert_eq!((at(&ridges, 170.0), at(&valleys, 170.0)), (1.0, 0.0), "its shoulder, where it starts down");
        assert_eq!((at(&ridges, 300.0), at(&valleys, 300.0)), (0.0, 0.0), "half way down, the slope even");
        assert_eq!((at(&ridges, 430.0), at(&valleys, 430.0)), (0.0, 1.0), "its foot");
        assert_eq!((at(&ridges, 700.0), at(&valleys, 700.0)), (0.0, 0.0), "the level ground beyond");
        // Turned over it is a trench, and each mask the other.
        let (ridges, valleys) = (ridge(Curvature::RIDGES, -300.0), ridge(Curvature::VALLEYS, -300.0));
        assert_eq!((at(&ridges, 170.0), at(&valleys, 170.0)), (0.0, 1.0));
        assert_eq!((at(&ridges, 430.0), at(&valleys, 430.0)), (1.0, 0.0));
        // Level ground does not bend, and nothing wired in is nothing.
        let level = testing::world_of(
            NodeKind::Curvature(curvature.clone()),
            0,
            &[Some(NodeKind::Constant(Constant { value: 4.0 }))],
        );
        assert_eq!(level.height_at(2.0, 2.0), 0.0);
        assert_eq!(testing::world_of(NodeKind::Curvature(curvature), 1, &[]).height_at(2.0, 2.0), 0.0);
    }
}
