//! The Noise 3D node: fractal noise through space, from Low where it is
//! least to High where it is most, as a Volume. Taken from the ground by a
//! Combine node it hollows the ground wherever it is above nothing; joined
//! to the ground it leaves rock in the air.
//!
//! Its size is in metres whatever the generator's scale, which stretches
//! the ground sideways only. A Volume is read no more than 64 m over its
//! ground (`compile::MOST_OVER`), so noise joined to the ground ends there.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::{
    generate::{
        compile::{Cells, Compiler, Kernel, Key, Places, V},
        graph::NodeId,
        landmass::seed_of,
    },
    noise::{self, NoiseStyle},
};

/// Noise of `style` through space, its largest features `size` metres
/// wide, in `octaves` octaves each half as large, from `low` to `high`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Noise3d {
    pub style: NoiseStyle,
    pub size: f32,
    pub octaves: u32,
    pub low: f32,
    pub high: f32,
}

impl Default for Noise3d {
    fn default() -> Self {
        Self { style: NoiseStyle::Fractal, size: 40.0, octaves: 3, low: -1.0, high: 1.0 }
    }
}

/// The seed of the noise of Volume node `node`: `own` more than the
/// generator's where the node has a seed of its own, else from the node's
/// number, as a Noise node's is.
pub(super) fn seed_for(cx: &Compiler, node: NodeId, own: u32) -> u32 {
    match own {
        0 => seed_of(cx.world.seed, 0x1000u32.wrapping_add(node.0)),
        own => cx.world.seed.wrapping_add(own),
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("style", "Style", &["Fractal", "Ridged", "Billow"]),
    ValueSpec::number("size", "Size", 0.01, f32::MAX, "m"),
    ValueSpec::whole("octaves", "Octaves", 1, 12),
    ValueSpec::number("low", "Low", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("high", "High", f32::MIN, f32::MAX, "m"),
];

const OUTPUTS: &[Port] = &[Port::volume("noise", "Noise")];

impl Kind for Noise3d {
    const TITLE: &'static str = "Noise 3D";
    const FOLDER: &'static str = "Voxels";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(NoiseStyle::ALL.iter().position(|&style| style == self.style).unwrap_or(0)),
            1 => Value::Number(self.size),
            2 => Value::Whole(self.octaves),
            3 => Value::Number(self.low),
            4 => Value::Number(self.high),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.style = NoiseStyle::ALL[choice.min(NoiseStyle::ALL.len() - 1)],
            (1, Value::Number(number)) => self.size = number,
            (2, Value::Whole(whole)) => self.octaves = whole,
            (3, Value::Number(number)) => self.low = number,
            (4, Value::Number(number)) => self.high = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let through = Through {
            style: self.style,
            size: self.size.max(0.01),
            octaves: self.octaves,
            seed: seed_for(cx, node, 0),
            low: self.low,
            span: self.high - self.low,
        };
        let row = cx.run(through, &[at.x, at.y, at.z]);
        // It may be solid at any height, where it is above nothing at all.
        if self.low > 0.0 || self.high > 0.0 {
            cx.set_rise(row, f32::INFINITY);
        }
        row
    }
}

/// Fractal noise through space, from rows of x, y and z.
#[derive(Debug)]
struct Through {
    style: NoiseStyle,
    size: f32,
    octaves: u32,
    seed: u32,
    low: f32,
    span: f32,
}

impl Kernel for Through {
    fn key(&self, key: &mut Key) {
        key.name("noise 3d").text(self.style.name()).number(self.size).whole(self.octaves).whole(self.seed);
        key.number(self.low).number(self.span);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let size = self.size;
        for (at, out) in out.iter_mut().enumerate() {
            let place = [inputs[0][at] / size, inputs[1][at] / size, inputs[2][at] / size];
            *out = self.low + noise::fractal3(self.style, place, self.octaves, self.seed) * self.span;
        }
    }
}
