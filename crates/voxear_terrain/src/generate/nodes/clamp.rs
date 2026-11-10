//! The Clamp node: what is wired in, held between Min and Max. Smooth
//! rounds the corner where it meets either, over that much of what is
//! wired in (metres, for a height), so a sea bed held level or a plateau
//! cut flat has no crease round it.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// Held from `min` to `max`, the corners rounded over `smooth`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clamp {
    pub min: f32,
    pub max: f32,
    pub smooth: f32,
}

impl Default for Clamp {
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, smooth: 0.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("min", "Min", f32::MIN, f32::MAX, ""),
    ValueSpec::number("max", "Max", f32::MIN, f32::MAX, ""),
    ValueSpec::number("smooth", "Smooth", 0.0, f32::MAX, ""),
];

const INPUTS: &[Port] = &[Port::field("in", "In")];

const OUTPUTS: &[Port] = &[Port::field("out", "Out")];

impl Kind for Clamp {
    const TITLE: &'static str = "Clamp";
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
            0 => Value::Number(self.min),
            1 => Value::Number(self.max),
            2 => Value::Number(self.smooth),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.min = number,
            (1, Value::Number(number)) => self.max = number,
            (2, Value::Number(number)) => self.smooth = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let read = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(0.0));
        // A Max under its Min holds everything at the Min. The corners are rounded over no more than half of what
        // lies between the two, so that rounding one never reaches past the other.
        let max = if self.max > self.min { self.max } else { self.min };
        let half = (max - self.min) * 0.5;
        let smooth = if self.smooth < half { self.smooth } else { half };
        cx.run(Held { min: self.min, max, smooth: if smooth > 0.0 { smooth } else { 0.0 } }, &[read])
    }
}

/// A row held from `min` to `max`, the corners rounded over `smooth`.
#[derive(Debug)]
struct Held {
    min: f32,
    max: f32,
    smooth: f32,
}

impl Held {
    fn hold(&self, value: f32) -> f32 {
        let (min, max, smooth) = (self.min, self.max, self.smooth);
        if smooth <= 0.0 {
            return if value < min {
                min
            } else if value > max {
                max
            } else {
                value
            };
        }
        // Within `smooth` of an end the straight line bends into the level one along a parabola, meeting each
        // without a crease, and never past the end.
        let raised = if value <= min - smooth {
            min
        } else if value >= min + smooth {
            value
        } else {
            let in_bend = value - min + smooth;
            min + in_bend * in_bend / (4.0 * smooth)
        };
        if raised >= max + smooth {
            max
        } else if raised <= max - smooth {
            raised
        } else {
            let in_bend = max + smooth - raised;
            max - in_bend * in_bend / (4.0 * smooth)
        }
    }
}

impl Kernel for Held {
    fn key(&self, key: &mut Key) {
        key.name("clamp").number(self.min).number(self.max).number(self.smooth);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, &read) in out.iter_mut().zip(inputs[0]) {
            *out = self.hold(read);
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
    fn a_clamp_holds_ground_between_its_ends_and_rounds_the_corners() {
        let hills = Noise { size: 300.0, low: -50.0, high: 150.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]);
        let clamp = |smooth: f32| {
            let kind = NodeKind::Clamp(Clamp { min: 20.0, max: 70.0, smooth });
            testing::world_of(kind, 0, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (hard, soft) = (clamp(0.0), clamp(10.0));
        assert_eq!(testing::checked(&hard), (20.0, 70.0));
        let (low, high) = testing::checked(&soft);
        assert!(low >= 20.0 && high <= 70.0 && high - low > 45.0, "{low} to {high}");
        let (was, held, rounded) =
            (testing::flat(&source, 4000), testing::flat(&hard, 4000), testing::flat(&soft, 4000));
        for i in 0..4000 {
            assert_eq!(held[i], was[i].clamp(20.0, 70.0));
            // Rounded, it is the same away from the ends and never further from them than a quarter of Smooth.
            let away = was[i] > 30.0 && was[i] < 60.0 || was[i] < 10.0 || was[i] > 80.0;
            assert!(!away || rounded[i] == held[i], "{} held at {}", was[i], rounded[i]);
            assert!((rounded[i] - held[i]).abs() <= 2.5 + 1e-4, "{} held at {}", was[i], rounded[i]);
        }
        // The bend has no crease: it meets the level and the slope where it starts and ends, and rises all the way.
        let held = Held { min: 20.0, max: 70.0, smooth: 10.0 };
        assert_eq!([held.hold(10.0), held.hold(30.0), held.hold(60.0), held.hold(80.0)], [20.0, 30.0, 60.0, 70.0]);
        assert_eq!((held.hold(20.0), held.hold(70.0)), (22.5, 67.5));
        let mut last = held.hold(0.0);
        for i in 1..=1000 {
            let next = held.hold(i as f32 * 0.1);
            assert!(next >= last && next - last <= 0.1 + 1e-4, "{last} then {next}");
            last = next;
        }
        // A Max under the Min holds everything at the Min; Smooth wider than the ends are apart is held to half.
        assert_eq!(
            testing::flat(&testing::world_of(NodeKind::Clamp(Clamp { min: 5.0, max: 1.0, smooth: 3.0 }), 0, &[]), 9)[4],
            5.0
        );
        let narrow = Clamp { min: 20.0, max: 30.0, smooth: 400.0 };
        let (low, high) =
            testing::checked(&testing::world_of(NodeKind::Clamp(narrow), 0, &[Some(NodeKind::Noise(hills))]));
        assert!(low >= 20.0 && high <= 30.0, "{low} to {high}");
        // Round a planet each place is held as on flat ground.
        let (was, round) = (testing::round(&source, 2000), testing::round(&hard, 2000));
        assert!((0..2000).all(|i| round[i] == was[i].clamp(20.0, 70.0)));
    }
}
