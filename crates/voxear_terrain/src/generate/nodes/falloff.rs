//! The Falloff node: 1 round a middle, easing to nothing further out. It
//! is whole within Radius metres of the middle and gone Feather metres
//! beyond that. Multiplied into a height it leaves an island, or a level's
//! ground sinking into the sea at its edges; through a curve it is a
//! volcano's cone or a crater's bowl.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    layers::smoothstep,
};

/// How a falloff measures how far a place is from its middle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FalloffShape {
    /// The same distance all round: a disc.
    #[default]
    Round,
    /// The further of how far it is along each axis: a square, its sides
    /// along x and z.
    Square,
}

impl FalloffShape {
    pub const ALL: [FalloffShape; 2] = [FalloffShape::Round, FalloffShape::Square];

    pub fn name(self) -> &'static str {
        match self {
            FalloffShape::Round => "Round",
            FalloffShape::Square => "Square",
        }
    }
}

/// A falloff round `(x, z)`, in metres: whole within `radius` of it and
/// gone `feather` further out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Falloff {
    pub shape: FalloffShape,
    pub x: f32,
    pub z: f32,
    pub radius: f32,
    pub feather: f32,
}

impl Default for Falloff {
    fn default() -> Self {
        Self { shape: FalloffShape::Round, x: 0.0, z: 0.0, radius: 500.0, feather: 500.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("shape", "Shape", &["Round", "Square"]),
    ValueSpec::number("x", "X", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("z", "Z", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("radius", "Radius", 0.0, f32::MAX, "m"),
    ValueSpec::number("feather", "Feather", 0.0, f32::MAX, "m"),
];

const OUTPUTS: &[Port] = &[Port::field("falloff", "Falloff")];

impl Kind for Falloff {
    const TITLE: &'static str = "Falloff";
    const FOLDER: &'static str = "Sources";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(FalloffShape::ALL.iter().position(|&shape| shape == self.shape).unwrap_or(0)),
            1 => Value::Number(self.x),
            2 => Value::Number(self.z),
            3 => Value::Number(self.radius),
            4 => Value::Number(self.feather),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.shape = FalloffShape::ALL[choice.min(FalloffShape::ALL.len() - 1)],
            (1, Value::Number(number)) => self.x = number,
            (2, Value::Number(number)) => self.z = number,
            (3, Value::Number(number)) => self.radius = number,
            (4, Value::Number(number)) => self.feather = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, at: Places) -> V {
        // Everything along the ground is as large as the generator's scale makes it: where its middle is as well.
        let scale = cx.world.scale.max(0.01);
        let fall = Fall {
            square: self.shape == FalloffShape::Square,
            middle: [self.x * scale, self.z * scale],
            radius: self.radius * scale,
            feather: self.feather * scale,
        };
        cx.run(fall, &[at.x, at.z])
    }
}

/// A falloff from rows of x and z.
#[derive(Debug)]
struct Fall {
    square: bool,
    middle: [f32; 2],
    radius: f32,
    feather: f32,
}

impl Fall {
    /// How far a place is from the middle, by how far it is along each
    /// axis.
    fn far(&self, along: &[f32]) -> f32 {
        if self.square {
            let mut furthest = 0.0;
            for &apart in along {
                if apart.abs() > furthest {
                    furthest = apart.abs();
                }
            }
            return furthest;
        }
        along.iter().map(|apart| apart * apart).sum::<f32>().sqrt()
    }
}

impl Kernel for Fall {
    fn key(&self, key: &mut Key) {
        key.name("falloff").whole(u32::from(self.square)).number(self.middle[0]).number(self.middle[1]);
        key.number(self.radius).number(self.feather);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let [mx, mz] = self.middle;
        for (at, out) in out.iter_mut().enumerate() {
            let far = self.far(&[inputs[0][at] - mx, inputs[1][at] - mz]);
            *out = 1.0 - smoothstep(self.radius, self.radius + self.feather, far);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing};

    #[test]
    fn a_falloff_is_whole_at_its_middle_and_gone_past_its_feather() {
        let falloff = Falloff { x: 200.0, z: -100.0, radius: 300.0, feather: 400.0, ..Falloff::default() };
        let world = testing::world_of(NodeKind::Falloff(falloff.clone()), 0, &[]);
        let (low, high) = testing::checked(&world);
        assert_eq!((low, high), (0.0, 1.0));
        assert_eq!(world.height_at(200.0, -100.0), 1.0);
        assert_eq!(world.height_at(200.0, 199.0), 1.0, "whole within its radius");
        assert!((world.height_at(700.0, -100.0) - 0.5).abs() < 1e-5, "half way through its feather");
        assert_eq!(world.height_at(200.0, 601.0), 0.0, "gone past it");
        // A square's corners are as whole as its sides.
        let square = Falloff { shape: FalloffShape::Square, ..falloff.clone() };
        let square = testing::world_of(NodeKind::Falloff(square), 0, &[]);
        testing::checked(&square);
        assert_eq!(square.height_at(495.0, 195.0), 1.0);
        assert!(world.height_at(495.0, 195.0) < 0.9);
        // Twice the scale, the same falloff twice as wide, its middle twice as far out.
        assert_eq!(world.with_scale(2.0).height_at(1400.0, -200.0), world.height_at(700.0, -100.0));
    }
}
