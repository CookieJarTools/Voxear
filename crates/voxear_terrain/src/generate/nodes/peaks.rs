//! The Peaks node: great mountains. Ranges of sharp ridges rise from low
//! land, and over them stand summits far higher than the rest, horns with
//! ridges running down from them, with level land between the ranges.
//!
//! Size is how far apart the great summits are. Height is how high the
//! tallest stand over Base, the height of the land between the ranges.
//! Ranges is how much of the land is mountains, 0 to 1. Sharpness makes
//! the summits pointed and the ridges knife-edged, from rounded at 0.
//! Ridges is how much the ridges running from a summit stand out against
//! the summit itself.
//!
//! Beside the height it gives Crests, 1 along the ridges' crests, for a
//! Rock mask or snow.

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    noise::{NoiseStyle, fractal, gradient},
};

/// Mountains up to `height` metres over land at `base`, their great
/// summits `size` metres apart, over as much of the land as `ranges` says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Peaks {
    pub size: f32,
    pub height: f32,
    pub base: f32,
    pub ranges: f32,
    pub sharpness: f32,
    pub ridges: f32,
    pub seed: u32,
}

impl Default for Peaks {
    fn default() -> Self {
        Self { size: 3000.0, height: 1600.0, base: 30.0, ranges: 0.5, sharpness: 0.6, ridges: 0.5, seed: 0 }
    }
}

impl Peaks {
    /// Its outputs.
    pub const HEIGHT: usize = 0;
    pub const CRESTS: usize = 1;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("size", "Size", 50.0, f32::MAX, "m"),
    ValueSpec::number("height", "Height", 0.0, 20000.0, "m"),
    ValueSpec::number("base", "Base", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("ranges", "Ranges", 0.0, 1.0, ""),
    ValueSpec::number("sharpness", "Sharpness", 0.0, 1.0, ""),
    ValueSpec::number("ridges", "Ridges", 0.0, 1.0, ""),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const OUTPUTS: &[Port] = &[Port::field("height", "Height"), Port::field("crests", "Crests")];

impl Kind for Peaks {
    const TITLE: &'static str = "Peaks";
    const FOLDER: &'static str = "Sources";

    fn inputs(&self) -> &'static [Port] {
        &[]
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
            1 => Value::Number(self.height),
            2 => Value::Number(self.base),
            3 => Value::Number(self.ranges),
            4 => Value::Number(self.sharpness),
            5 => Value::Number(self.ridges),
            6 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size = number,
            (1, Value::Number(number)) => self.height = number,
            (2, Value::Number(number)) => self.base = number,
            (3, Value::Number(number)) => self.ranges = number,
            (4, Value::Number(number)) => self.sharpness = number,
            (5, Value::Number(number)) => self.ridges = number,
            (6, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let set = Mountains {
            size: (self.size * cx.world.scale.max(0.01)).max(50.0),
            height: self.height.clamp(0.0, 20000.0),
            base: self.base,
            ranges: if self.ranges.is_nan() { 0.0 } else { self.ranges.clamp(0.0, 1.0) },
            sharpness: if self.sharpness.is_nan() { 0.0 } else { self.sharpness.clamp(0.0, 1.0) },
            ridges: if self.ridges.is_nan() { 0.0 } else { self.ridges.clamp(0.0, 1.0) },
            seed: seed_for(cx, node, self.seed),
            crests: port == Self::CRESTS,
        };
        cx.run(set, &[at.x, at.z])
    }
}

/// A Peaks node's settings held to their ranges, making its height or,
/// with `crests`, where its crests are, from rows of x and z.
#[derive(Clone, Copy, Debug)]
struct Mountains {
    size: f32,
    height: f32,
    base: f32,
    ranges: f32,
    sharpness: f32,
    ridges: f32,
    seed: u32,
    crests: bool,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Mountains {
    /// How much of a mountain the place is, 0 on the land between the
    /// ranges to 1 well within one.
    fn range(&self, x: f32, z: f32) -> f32 {
        let wide = self.size * 2.2;
        let broad = 0.5 + 0.5 * gradient(x / wide, z / wide, self.seed.wrapping_add(1));
        smooth((broad - (1.0 - self.ranges)) / 0.22 + 0.5)
    }

    /// How near a great summit the place is, 0 far from any to 1 at one,
    /// sharpened as the node says.
    fn summit(&self, x: f32, z: f32) -> f32 {
        let cell = [(x / self.size).floor() as i32, (z / self.size).floor() as i32];
        let mut best = 0.0f32;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = [cell[0] + dx, cell[1] + dz];
                let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
                let at = [
                    (cell[0] as f32 + 0.15 + 0.7 * pick(0)) * self.size,
                    (cell[1] as f32 + 0.15 + 0.7 * pick(1)) * self.size,
                ];
                let tall = 0.55 + 0.45 * pick(2);
                let (ox, oz) = (x - at[0], z - at[1]);
                let near = 1.0 - (ox * ox + oz * oz).sqrt() / (self.size * 0.62);
                if near <= 0.0 {
                    continue;
                }
                // Steeper towards the top the sharper it is: a dome at none, a horn at the most.
                let horn = near * near * near;
                let shape = near * (1.0 - self.sharpness) + horn * self.sharpness;
                best = best.max(shape * tall);
            }
        }
        best
    }

    /// Sharp ridges across the ranges, 0 to 1, highest along their crests.
    fn ridged(&self, x: f32, z: f32) -> f32 {
        // The ridges wander, so they do not run along the noise's grid.
        let warp = self.size * 0.35;
        let (wx, wz) = (
            warp * gradient(x / (self.size * 0.9), z / (self.size * 0.9), self.seed.wrapping_add(2)),
            warp * gradient(x / (self.size * 0.9), z / (self.size * 0.9), self.seed.wrapping_add(3)),
        );
        let scale = self.size * 0.45;
        let ridge = fractal(NoiseStyle::Ridged, (x + wx) / scale, (z + wz) / scale, 7, self.seed.wrapping_add(4));
        // Knife-edged the sharper the node is.
        ridge * (1.0 - self.sharpness * 0.5) + ridge * ridge * self.sharpness * 0.5
    }
}

impl Kernel for Mountains {
    fn key(&self, key: &mut Key) {
        key.name("peaks").number(self.size).number(self.height).number(self.base).number(self.ranges);
        key.number(self.sharpness).number(self.ridges).whole(self.seed).whole(u32::from(self.crests));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let (x, z) = (inputs[0][at], inputs[1][at]);
            let range = self.range(x, z);
            let ridged = self.ridged(x, z);
            if self.crests {
                *out = smooth((ridged - 0.62) / 0.15) * smooth(range * 2.0);
                continue;
            }
            // Low rolling land between the ranges.
            let rolling = 12.0 * fractal(NoiseStyle::Fractal, x / 400.0, z / 400.0, 4, self.seed.wrapping_add(5));
            if range <= 0.0 {
                *out = self.base + rolling;
                continue;
            }
            let summit = self.summit(x, z);
            // The summits carry the ridges up with them; the ridges alone make the lower ranges.
            let lift = 0.25 + 0.75 * summit;
            let ridges = ridged * (0.5 + 0.5 * self.ridges);
            let mass = summit * (1.0 - 0.5 * self.ridges) + ridges * lift;
            *out = self.base + rolling + self.height * range * mass * 0.75;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing};

    fn world(set: Peaks, port: usize) -> crate::generate::WorldGen {
        testing::world_of(NodeKind::Peaks(set), port, &[])
    }

    #[test]
    fn peaks_stand_far_over_level_land_between_their_ranges() {
        let world = world(Peaks::default(), Peaks::HEIGHT);
        let (low, high) = testing::checked(&world);
        assert!(low >= 30.0 - 1.0, "the land between at {low}");
        let side = 240;
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * 50.0 - 6000.0, (at / side) as f32 * 50.0 - 6000.0)).unzip();
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let highest = heights.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            highest > 900.0 && highest < 30.0 + 1600.0 + 20.0,
            "the highest summit {highest} m ({high} over flat places)"
        );
        let level = heights.iter().filter(|&&height| height < 30.0 + 13.0).count() as f32 / heights.len() as f32;
        assert!(level > 0.15 && level < 0.85, "{level} of the land level between the ranges");
        // Steep: somewhere the ground climbs a hundred metres in fifty.
        let steep =
            (1..heights.len()).filter(|&at| at % side != 0).any(|at| (heights[at] - heights[at - 1]).abs() > 100.0);
        assert!(steep, "no steep mountainside");
    }

    #[test]
    fn sharper_peaks_have_narrower_tops_and_crests_lie_along_the_ridges() {
        let side = 160;
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..side * side).map(|at| ((at % side) as f32 * 40.0 - 3200.0, (at / side) as f32 * 40.0 - 3200.0)).unzip();
        let high_share = |sharpness: f32| {
            let world = world(Peaks { sharpness, ranges: 1.0, ..Peaks::default() }, Peaks::HEIGHT);
            let mut heights = vec![0.0; xs.len()];
            world.heights(&xs, &zs, &mut heights);
            let highest = heights.iter().copied().fold(f32::MIN, f32::max);
            heights.iter().filter(|&&height| height > highest * 0.7).count()
        };
        let (round, sharp) = (high_share(0.0), high_share(1.0));
        assert!(sharp < round, "{sharp} places near the top of sharp peaks, {round} of round ones");
        let crests = world(Peaks { ranges: 1.0, ..Peaks::default() }, Peaks::CRESTS);
        let mut on = vec![0.0; xs.len()];
        crests.heights(&xs, &zs, &mut on);
        let share = on.iter().filter(|&&on| on > 0.5).count() as f32 / on.len() as f32;
        assert!(share > 0.01 && share < 0.4, "crests over {share} of the ranges");
    }
}
