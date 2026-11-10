//! The Warp node: what is wired in, read a little way from where it is
//! asked for, so its straight lines wind and its round shapes turn ragged.
//! Each place is moved by noise of the node's own: up to Strength metres,
//! changing over Size metres. Everything behind the node is worked out at
//! the moved places, so one warp bends a whole chain of nodes together.
//!
//! Round a planet the places are moved in three dimensions, as its fields
//! are read.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, node_seed};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
        landmass::{seed_of, signed},
    },
    noise::{self, NoiseStyle},
};

/// A warp of up to `strength` metres, changing over `size` metres, its
/// noise in `octaves` octaves each half as large.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bend {
    pub size: f32,
    pub strength: f32,
    pub octaves: u32,
}

impl Default for Bend {
    fn default() -> Self {
        Self { size: 400.0, strength: 60.0, octaves: 3 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size", "Size", 0.01, f32::MAX, "m"),
    ValueSpec::number("strength", "Strength", 0.0, f32::MAX, "m"),
    ValueSpec::whole("octaves", "Octaves", 1, 12),
];

const INPUTS: &[Port] = &[Port::field("in", "In")];

const OUTPUTS: &[Port] = &[Port::field("out", "Out")];

impl Kind for Bend {
    const TITLE: &'static str = "Warp";
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
            0 => Value::Number(self.size),
            1 => Value::Number(self.strength),
            2 => Value::Whole(self.octaves),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size = number,
            (1, Value::Number(number)) => self.strength = number,
            (2, Value::Whole(whole)) => self.octaves = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let scale = cx.world.scale.max(0.01);
        let seed = node_seed(cx.world, node);
        let moved = |cx: &mut Compiler, axis: usize, round: bool, rows: &[V]| {
            let bent = Bent {
                axis,
                size: (self.size * scale).max(0.01),
                reach: self.strength * scale,
                octaves: self.octaves,
                seed: seed_of(seed, axis as u32 + 1),
                round,
            };
            cx.run(bent, rows)
        };
        let from = match at {
            _ if self.strength <= 0.0 => at,
            Places::Flat { x, z } => Places::Flat { x: moved(cx, 0, false, &[x, z]), z: moved(cx, 1, false, &[x, z]) },
            Places::Round { x, y, z } => {
                let rows = [x, y, z];
                Places::Round {
                    x: moved(cx, 0, true, &rows),
                    y: moved(cx, 1, true, &rows),
                    z: moved(cx, 2, true, &rows),
                }
            }
        };
        cx.input(node, 0, from).unwrap_or_else(|| cx.constant(0.0))
    }
}

/// One axis of places, each moved along it by noise read at the place: from
/// rows of x and z, or of x, y and z round a planet.
#[derive(Debug)]
struct Bent {
    axis: usize,
    size: f32,
    reach: f32,
    octaves: u32,
    seed: u32,
    round: bool,
}

impl Kernel for Bent {
    fn key(&self, key: &mut Key) {
        key.name("warp").whole(self.axis as u32).number(self.size).number(self.reach).whole(self.octaves);
        key.whole(self.seed).whole(u32::from(self.round));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let size = self.size;
        for (at, out) in out.iter_mut().enumerate() {
            let noise = match self.round {
                false => noise::fractal(
                    NoiseStyle::Fractal,
                    inputs[0][at] / size,
                    inputs[1][at] / size,
                    self.octaves,
                    self.seed,
                ),
                true => {
                    let place = [inputs[0][at] / size, inputs[1][at] / size, inputs[2][at] / size];
                    noise::fractal3(NoiseStyle::Fractal, place, self.octaves, self.seed)
                }
            };
            *out = inputs[self.axis][at] + signed(noise) * self.reach;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Falloff, Noise, testing},
    };

    #[test]
    fn a_warp_reads_its_input_no_further_off_than_its_strength() {
        let hills = Noise { size: 300.0, low: 10.0, high: 90.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]);
        let warp = |strength: f32| {
            let kind = NodeKind::Warp(Bend { size: 200.0, strength, octaves: 3 });
            testing::world_of(kind, 0, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (bent, straight) = (warp(80.0), warp(0.0));
        let (low, high) = testing::checked(&bent);
        assert!(low >= 10.0 && high <= 90.0 && high - low > 30.0, "its input's range still: {low} to {high}");
        let (was, now, left) =
            (testing::flat(&source, 4000), testing::flat(&bent, 4000), testing::flat(&straight, 4000));
        assert_eq!(left, was, "no strength, no warp");
        let moved = (0..4000).filter(|&i| (now[i] - was[i]).abs() > 1.0).count();
        assert!(moved > 2000, "bent nearly everywhere: {moved} of 4000 places");
        // A disc 500 m across, bent by up to 80 m each way along x and z: still whole well inside, still gone well
        // outside, and ragged between.
        let disc = Falloff { radius: 500.0, feather: 0.0, ..Falloff::default() };
        let disc = |strength: f32| {
            let kind = NodeKind::Warp(Bend { size: 200.0, strength, octaves: 3 });
            testing::world_of(kind, 0, &[Some(NodeKind::Falloff(disc.clone()))])
        };
        let (ragged, mut inside, mut outside) = (disc(80.0), 0, 0);
        for i in 0..720 {
            let (sin, cos) = (i as f32 * 0.5).to_radians().sin_cos();
            assert_eq!(ragged.height_at(cos * 380.0, sin * 380.0), 1.0, "within 500 less 80 each way");
            assert_eq!(ragged.height_at(cos * 620.0, sin * 620.0), 0.0);
            let edge = ragged.height_at(cos * 500.0, sin * 500.0);
            (inside, outside) = (inside + usize::from(edge == 1.0), outside + usize::from(edge == 0.0));
        }
        assert!(inside > 150 && outside > 150, "its edge winds in and out: {inside} in, {outside} out");
        // Twice the scale, the same warp twice as wide.
        assert_eq!(bent.with_scale(2.0).height_at(260.0, -90.0), bent.height_at(130.0, -45.0));
        // Round a planet it moves places in three dimensions: the disc over the north pole, ragged the same way.
        assert_eq!(ragged.height_round([0.0, 3000.0, 0.0]), 1.0);
        assert_eq!(ragged.height_round([0.0, -3000.0, 0.0]), 0.0);
        let (mut inside, mut outside) = (0, 0);
        for i in 0..720 {
            // 500 m through the planet from its pole, all round it.
            let (sin, cos) = (i as f32 * 0.5).to_radians().sin_cos();
            let (out, up) = (500.0 * (1.0 - 500.0f32 * 500.0 / 36_000_000.0).sqrt(), 3000.0 - 500.0 * 500.0 / 6000.0);
            let edge = ragged.height_round([cos * out, up, sin * out]);
            (inside, outside) = (inside + usize::from(edge == 1.0), outside + usize::from(edge == 0.0));
        }
        assert!(inside > 150 && outside > 150, "round a planet: {inside} in, {outside} out");
        let (low, high) = testing::round_range(&bent);
        assert!(low >= 10.0 && high <= 90.0 && high - low > 30.0, "{low} to {high}");
    }
}
