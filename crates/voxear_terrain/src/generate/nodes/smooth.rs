//! The Smooth node: what is wired in, averaged round each place, so its
//! creases round off and its small bumps flatten. It reads its input at
//! the place and at eight more round it, half of Radius away along each
//! axis and at the corners between, and weighs the nearer ones more. Nine
//! reads make it one of the dearer nodes: everything behind it is worked
//! out nine times.
//!
//! Round a planet the eight places lie along two tangents of its surface.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// An average of what lies within `radius` metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Smooth {
    pub radius: f32,
}

impl Default for Smooth {
    fn default() -> Self {
        Self { radius: 8.0 }
    }
}

const VALUES: &[ValueSpec] = &[ValueSpec::number("radius", "Radius", 0.0, f32::MAX, "m")];

const INPUTS: &[Port] = &[Port::field("in", "In")];

const OUTPUTS: &[Port] = &[Port::field("out", "Out")];

impl Kind for Smooth {
    const TITLE: &'static str = "Smooth";
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
        (at == 0).then_some(Value::Number(self.radius))
    }

    fn set(&mut self, at: usize, value: Value) {
        if let (0, Value::Number(number)) = (at, value) {
            self.radius = number;
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let Some(middle) = cx.input(node, 0, at) else { return cx.constant(0.0) };
        let step = self.radius * cx.world.scale.max(0.01) * 0.5;
        if step <= 0.0 {
            return middle;
        }
        // The four along the axes, then the four at the corners between them.
        let ways =
            [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0], [1.0, 1.0], [-1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]];
        let ring = ways.map(|[a, b]| {
            let from = around::moved(cx, at, a * step, b * step);
            cx.input(node, 0, from).unwrap_or(middle)
        });
        let ring = cx.run(Ring, &ring);
        cx.run(Averaged, &[middle, ring])
    }
}

/// Eight rows added: the first four, along the axes, counted twice.
#[derive(Debug)]
struct Ring;

impl Kernel for Ring {
    fn key(&self, key: &mut Key) {
        key.name("smooth ring");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let near = inputs[0][at] + inputs[1][at] + inputs[2][at] + inputs[3][at];
            let far = inputs[4][at] + inputs[5][at] + inputs[6][at] + inputs[7][at];
            *out = near * 2.0 + far;
        }
    }
}

/// The middle, counted four times, and the ring round it, as one average
/// of sixteen.
#[derive(Debug)]
struct Averaged;

impl Kernel for Averaged {
    fn key(&self, key: &mut Key) {
        key.name("smooth");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            *out = (inputs[0][at] * 4.0 + inputs[1][at]) / 16.0;
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
    fn smoothing_flattens_small_bumps_and_keeps_within_what_it_reads() {
        // Fine bumps, 6 m across: an average over 12 m leaves little of them, one over 1 m nearly all.
        let bumps = Noise { size: 6.0, octaves: 1, low: -10.0, high: 10.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(bumps.clone()), 0, &[]);
        let smooth = |radius: f32| {
            testing::world_of(NodeKind::Smooth(Smooth { radius }), 0, &[Some(NodeKind::Noise(bumps.clone()))])
        };
        let (wide, narrow, none) = (smooth(12.0), smooth(1.0), smooth(0.0));
        let (low, high) = testing::checked(&wide);
        assert!(low >= -10.0 && high <= 10.0, "{low} to {high}");
        let rough = |heights: &[f32]| heights.iter().map(|height| height.abs()).sum::<f32>() / heights.len() as f32;
        let (was, flattened, kept) =
            (testing::flat(&source, 4000), testing::flat(&wide, 4000), testing::flat(&narrow, 4000));
        assert!(rough(&flattened) < rough(&was) * 0.35, "{} from {}", rough(&flattened), rough(&was));
        assert!(rough(&kept) > rough(&was) * 0.9, "{} from {}", rough(&kept), rough(&was));
        assert_eq!(testing::flat(&none, 4000), was, "no radius, no smoothing");
        // Level ground stays where it is, on flat ground and round a planet; nothing wired in is nothing.
        let level = testing::world_of(
            NodeKind::Smooth(Smooth::default()),
            0,
            &[Some(NodeKind::Constant(Constant { value: 7.0 }))],
        );
        assert_eq!((level.height_at(3.0, 4.0), level.height_round([10.0, 2000.0, -30.0])), (7.0, 7.0));
        assert_eq!(testing::world_of(NodeKind::Smooth(Smooth::default()), 0, &[]).height_at(3.0, 4.0), 0.0);
        // Round a planet it averages along the surface: the same bumps, read round it, flattened as much.
        let (was, flattened) = (testing::round(&source, 4000), testing::round(&wide, 4000));
        assert!(rough(&flattened) < rough(&was) * 0.45, "{} from {}", rough(&flattened), rough(&was));
        let (low, high) = testing::range(&flattened);
        assert!(low >= -10.0 && high <= 10.0, "{low} to {high}");
        // Twice the scale, the same ground twice as wide.
        assert_eq!(wide.with_scale(2.0).height_at(26.0, -9.0), wide.height_at(13.0, -4.5));
    }
}
