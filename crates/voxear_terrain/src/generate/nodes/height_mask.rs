//! The Height Mask node: 1 where the height wired in lies between Min and
//! Max, easing to nothing over Feather metres below the one and above the
//! other, as a terrain layer's mask does. It says where something belongs
//! by how high the ground is: snow above a line, sand along a shore.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// A mask of heights from `min` to `max`, in metres, eased in over
/// `feather` metres outside them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeightMask {
    pub min: f32,
    pub max: f32,
    pub feather: f32,
}

impl Default for HeightMask {
    fn default() -> Self {
        Self { min: 0.0, max: 100.0, feather: 10.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("min", "Min", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("max", "Max", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("feather", "Feather", 0.0, f32::MAX, "m"),
];

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::field("mask", "Mask")];

impl Kind for HeightMask {
    const TITLE: &'static str = "Height Mask";
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
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.min = number,
            (1, Value::Number(number)) => self.max = number,
            (2, Value::Number(number)) => self.feather = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let height = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(0.0));
        cx.run(Between { min: self.min, max: self.max, feather: self.feather }, &[height])
    }
}

/// How much of a mask from `min` to `max`, eased in over `feather`, covers
/// each of a row.
#[derive(Debug)]
struct Between {
    min: f32,
    max: f32,
    feather: f32,
}

impl Kernel for Between {
    fn key(&self, key: &mut Key) {
        key.name("height mask").number(self.min).number(self.max).number(self.feather);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, &height) in out.iter_mut().zip(inputs[0]) {
            *out = around::within(self.min, self.max, self.feather, height);
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
    fn a_height_mask_covers_its_heights_and_eases_out_over_its_feather() {
        let hills = Noise { size: 300.0, low: 0.0, high: 200.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]);
        let mask = HeightMask { min: 80.0, max: 120.0, feather: 20.0 };
        let world = testing::world_of(NodeKind::HeightMask(mask), 0, &[Some(NodeKind::Noise(hills))]);
        assert_eq!(testing::checked(&world), (0.0, 1.0));
        let check = |heights: &[f32], masks: &[f32]| {
            for (&height, &mask) in heights.iter().zip(masks) {
                let meant = match height {
                    height if (80.0..120.0).contains(&height) => 1.0..=1.0,
                    height if height <= 60.0 || height >= 140.0 => 0.0..=0.0,
                    _ => 0.0..=1.0,
                };
                assert!(meant.contains(&mask), "{mask} at {height} m");
                // Half of it half way through its feather, either side.
                assert!((height - 70.0).abs() > 0.5 && (height - 130.0).abs() > 0.5 || (mask - 0.5).abs() < 0.05);
            }
        };
        check(&testing::flat(&source, 4000), &testing::flat(&world, 4000));
        // With no feather its edge is sheer; with nothing wired in it reads a height of nothing.
        let sheer = Between { min: 10.0, max: 20.0, feather: 0.0 };
        let mut out = [0.0; 4];
        sheer.run(&[&[9.99, 10.0, 19.99, 20.0]], &mut out);
        assert_eq!(out, [0.0, 1.0, 1.0, 0.0]);
        assert_eq!(testing::world_of(NodeKind::HeightMask(HeightMask::default()), 0, &[]).height_at(0.0, 0.0), 1.0);
    }
}
