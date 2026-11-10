//! The Noise node: fractal noise over the ground, from Low where it is
//! least to High where it is most. Each node's noise is its own, from its
//! generator's seed and the node's number.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, node_seed};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::{self, NoiseStyle},
};

/// Noise of `style`, its largest features `size` metres wide, in `octaves`
/// octaves each half as large, from `low` to `high`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Noise {
    pub style: NoiseStyle,
    pub size: f32,
    pub octaves: u32,
    pub low: f32,
    pub high: f32,
}

impl Default for Noise {
    fn default() -> Self {
        Self { style: NoiseStyle::Fractal, size: 200.0, octaves: 4, low: 0.0, high: 1.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("style", "Style", &["Fractal", "Ridged", "Billow"]),
    ValueSpec::number("size", "Size", 0.01, f32::MAX, "m"),
    ValueSpec::whole("octaves", "Octaves", 1, 12),
    ValueSpec::number("low", "Low", f32::MIN, f32::MAX, ""),
    ValueSpec::number("high", "High", f32::MIN, f32::MAX, ""),
];

const OUTPUTS: &[Port] = &[Port::field("noise", "Noise")];

impl Kind for Noise {
    const TITLE: &'static str = "Noise";
    const FOLDER: &'static str = "Sources";

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

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let fractal = Fractal {
            style: self.style,
            size: (self.size * cx.world.scale.max(0.01)).max(0.01),
            octaves: self.octaves,
            seed: node_seed(cx.world, node),
            low: self.low,
            span: self.high - self.low,
            round: matches!(at, Places::Round { .. }),
        };
        match at {
            Places::Flat { x, z } => cx.run(fractal, &[x, z]),
            Places::Round { x, y, z } => cx.run(fractal, &[x, y, z]),
        }
    }
}

/// Fractal noise from rows of x and z, or of x, y and z round a planet.
#[derive(Debug)]
struct Fractal {
    style: NoiseStyle,
    size: f32,
    octaves: u32,
    seed: u32,
    low: f32,
    span: f32,
    round: bool,
}

impl Kernel for Fractal {
    fn key(&self, key: &mut Key) {
        key.name("fractal").text(self.style.name()).number(self.size).whole(self.octaves).whole(self.seed);
        key.number(self.low).number(self.span).whole(u32::from(self.round));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let size = self.size;
        for (at, out) in out.iter_mut().enumerate() {
            let noise = match self.round {
                false => {
                    noise::fractal(self.style, inputs[0][at] / size, inputs[1][at] / size, self.octaves, self.seed)
                }
                true => {
                    let place = [inputs[0][at] / size, inputs[1][at] / size, inputs[2][at] / size];
                    noise::fractal3(self.style, place, self.octaves, self.seed)
                }
            };
            *out = self.low + noise * self.span;
        }
    }
}
