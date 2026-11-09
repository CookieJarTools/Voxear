//! Making ground from a seed: a world generator, as a `.worldgen` file
//! keeps it, and the ground it makes at any point.
//!
//! Plain noise makes ground equally bumpy everywhere, with no plains beside
//! mountains and no valleys that lead anywhere. A generator instead reads a
//! few broad fields over the world, each meaning one thing, and shapes the
//! ground from them by curves:
//!
//! - **Continents**: from deep sea, through the coast, to far inland.
//! - **Ruggedness**: from smooth country to rugged.
//! - **Ridges**: ridged noise, from the valleys between ranges to their
//!   crests, which run in long lines that wind and branch.
//!
//! The ground stands at the **Coast** curve's height for its place across
//! the continents, and rises and falls from there by its relief (the
//! **Inland** curve, how much relief land so far inland has, times the
//! **Relief** curve, how much its ruggedness gives it) as the **Peaks**
//! curve shapes its peaks and valleys. Crags roughen high peaks, and bumps
//! the ground everywhere. Every field is read through the same bending of
//! the world (its warp), so coasts and ranges wind rather than run straight.
//!
//! It is all sums, products, square roots and `floor`, with no trigonometry
//! or powers, so a seed makes the same ground on every machine, and a world
//! made from one need only send its seed.

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use crate::noise::{self, NoiseStyle};

/// A world generator: its seed, how large and high its ground is, and how
/// it is shaped.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldGen {
    pub seed: u32,
    /// How large everything is: 1 for each field's own size, 2 for twice
    /// as large.
    pub scale: f32,
    /// The sea's height, in metres: where the Coast curve's 0 is.
    pub sea_level: f32,
    /// How far the curves' 1 is above the sea, in metres: the height of
    /// the highest mountains.
    pub height: f32,
    pub continents: Field,
    pub ruggedness: Field,
    pub ridges: Field,
    /// How sharp ridges' crests are: 0 rounded, 1 knife-edged.
    pub sharpness: f32,
    pub warp: Warp,
    /// The ground's height across the continents, from the deepest sea (0)
    /// to furthest inland (1), as a share of `height`.
    pub coast: Curve,
    /// How much relief the land has across the continents, 0 to 1.
    pub inland: Curve,
    /// How much relief it has from smooth (0) to rugged (1), 0 to 1.
    pub relief: Curve,
    /// How its relief rises and falls from valleys (0) to peaks (1), from
    /// -1 to 1.
    pub peaks: Curve,
    /// How rough high peaks are, as a share of `height`.
    pub crags: f32,
    /// How high the small bumps over all the ground are, in metres.
    pub bumps: f32,
}

/// One of a generator's fields: how wide its largest features are, in
/// metres, and in how many octaves, each half as large.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub size: f32,
    pub octaves: u32,
}

/// How a generator bends the world before it reads its fields: over how
/// many metres, and how many metres at most.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warp {
    pub size: f32,
    pub strength: f32,
}

/// What a generator reads and makes at a point: its fields there, each
/// from -1 to 1 (peaks and valleys, folded from its ridges), and the
/// ground's height, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub continents: f32,
    pub ruggedness: f32,
    pub peaks: f32,
    pub height: f32,
}

/// Ground a generator starts from, to change from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Wide, nearly level farmland rolling gently, no sea: the ground for
    /// a battle.
    BattlefieldPlains,
    /// Hills and dales, no sea.
    RollingHills,
    /// Ranges of sharp, high mountains with deep valleys, coasts at their
    /// feet.
    Alpine,
    /// Islands scattered over a sea.
    Archipelago,
    /// High, flat plateaus cut by steep-sided canyons.
    Canyons,
}

impl Preset {
    pub const ALL: [Preset; 5] =
        [Preset::BattlefieldPlains, Preset::RollingHills, Preset::Alpine, Preset::Archipelago, Preset::Canyons];

    pub fn name(self) -> &'static str {
        match self {
            Preset::BattlefieldPlains => "Battlefield Plains",
            Preset::RollingHills => "Rolling Hills",
            Preset::Alpine => "Alpine",
            Preset::Archipelago => "Archipelago",
            Preset::Canyons => "Canyons",
        }
    }

    /// The generator it is, with seed 1.
    pub fn world(self) -> WorldGen {
        let field = |size: f32, octaves: u32| Field { size, octaves };
        let warp = |size: f32, strength: f32| Warp { size, strength };
        let base = WorldGen {
            seed: 1,
            scale: 1.0,
            sea_level: 0.0,
            height: 120.0,
            continents: field(4000.0, 4),
            ruggedness: field(1500.0, 3),
            ridges: field(1100.0, 3),
            sharpness: 0.3,
            warp: warp(900.0, 160.0),
            coast: Curve::new(&[[0.0, 0.35], [1.0, 0.5]]),
            inland: Curve::flat(1.0),
            relief: Curve::new(&[[0.0, 0.15], [0.5, 0.45], [1.0, 0.8]]),
            peaks: Curve::new(&[[0.0, -0.4], [0.5, 0.1], [1.0, 0.6]]),
            crags: 0.0,
            bumps: 1.5,
        };
        match self {
            Preset::RollingHills => base,
            Preset::BattlefieldPlains => WorldGen {
                height: 40.0,
                ruggedness: field(1200.0, 3),
                ridges: field(900.0, 2),
                sharpness: 0.2,
                warp: warp(700.0, 100.0),
                coast: Curve::new(&[[0.0, 0.2], [1.0, 0.3]]),
                relief: Curve::new(&[[0.0, 0.05], [0.6, 0.2], [1.0, 0.4]]),
                peaks: Curve::new(&[[0.0, -0.3], [0.5, 0.0], [1.0, 0.4]]),
                bumps: 0.8,
                ..base
            },
            Preset::Alpine => WorldGen {
                height: 900.0,
                continents: field(6000.0, 4),
                ruggedness: field(2500.0, 3),
                ridges: field(2600.0, 3),
                sharpness: 0.9,
                warp: warp(2000.0, 300.0),
                coast: Curve::new(&[[0.0, -0.3], [0.35, -0.05], [0.45, 0.03], [0.6, 0.15], [1.0, 0.3]]),
                inland: Curve::new(&[[0.0, 0.0], [0.4, 0.0], [0.5, 0.3], [0.7, 0.9], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.2], [0.4, 0.5], [1.0, 1.0]]),
                peaks: Curve::new(&[[0.0, -0.12], [0.3, -0.05], [0.6, 0.35], [0.85, 0.75], [1.0, 0.85]]),
                crags: 0.12,
                bumps: 2.0,
                ..base
            },
            Preset::Archipelago => WorldGen {
                height: 140.0,
                continents: field(2500.0, 4),
                ruggedness: field(1200.0, 3),
                ridges: field(900.0, 3),
                sharpness: 0.6,
                warp: warp(1000.0, 220.0),
                coast: Curve::new(&[[0.0, -0.6], [0.5, -0.25], [0.65, 0.0], [0.75, 0.08], [1.0, 0.3]]),
                inland: Curve::new(&[[0.0, 0.0], [0.6, 0.0], [0.75, 0.6], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.3], [1.0, 0.8]]),
                peaks: Curve::new(&[[0.0, -0.2], [0.5, 0.1], [1.0, 0.6]]),
                crags: 0.03,
                bumps: 1.0,
                ..base
            },
            Preset::Canyons => WorldGen {
                height: 220.0,
                continents: field(5000.0, 3),
                ruggedness: field(2000.0, 3),
                ridges: field(1400.0, 4),
                sharpness: 0.8,
                warp: warp(1200.0, 220.0),
                coast: Curve::new(&[[0.0, 0.9], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.4], [1.0, 0.9]]),
                peaks: Curve::new(&[[0.0, 0.2], [0.6, 0.15], [0.72, -0.2], [0.85, -0.9], [1.0, -1.0]]),
                bumps: 1.0,
                ..base
            },
        }
    }
}

impl Default for WorldGen {
    fn default() -> Self {
        Preset::RollingHills.world()
    }
}

/// How far noise's usual spread is stretched to fill -1 to 1: fractal
/// noise seldom strays far from its middle.
const SPREAD: f32 = 2.6;

/// Where ridged noise's usual values sit, and how far they are stretched to
/// fill -1 to 1, from the valleys between ridges to their crests.
const RIDGE_MIDDLE: f32 = 0.45;
const RIDGE_SPREAD: f32 = 2.4;

/// Noise from 0 to 1 as -1 to 1, stretched by `SPREAD` and held within.
fn signed(value: f32) -> f32 {
    ((value - 0.5) * 2.0 * SPREAD).clamp(-1.0, 1.0)
}

/// Ridged fractal noise at `(x, z)`, 0 to 1, as `noise::fractal` makes it,
/// but each octave's crease rounded off where the noise under it is within
/// `soft` of its middle, so crests can be rounded.
fn ridged(x: f32, z: f32, octaves: u32, seed: u32, soft: f32) -> f32 {
    let (mut sum, mut total, mut amplitude, mut frequency, mut weight) = (0.0, 0.0, 1.0, 1.0, 1.0f32);
    for octave in 0..octaves.clamp(1, 12) {
        let n = noise::gradient(x * frequency, z * frequency, seed.wrapping_add(octave.wrapping_mul(0x632B_E5AB)));
        // |n|, rounded into a curve within `soft` of the middle, the same as it outside.
        let crease = if n.abs() < soft { (n * n / soft + soft) * 0.5 } else { n.abs() };
        let ridge = (1.0 - crease).max(0.0);
        let ridge = ridge * ridge * weight;
        weight = (ridge * 2.0).clamp(0.0, 1.0);
        sum += ridge * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (sum / total).clamp(0.0, 1.0)
}

/// The seed of a generator's part `part`, from its own.
fn seed_of(seed: u32, part: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ part.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// `curve` at `t`, 0 to 1, through its keys smoothly: a curve of cubics,
/// each between two keys, that never overshoots them, so a value between
/// two keys stays between them and the ground has no crease at a key.
/// `empty` for a curve with none.
pub fn smooth(curve: &Curve, t: f32, empty: f32) -> f32 {
    let keys = curve.keys();
    let (first, last) = match keys {
        [] => return empty,
        [only] => return only[1],
        [first, .., last] => (*first, *last),
    };
    if t <= first[0] {
        return first[1];
    }
    if t >= last[0] {
        return last[1];
    }
    let at = keys.windows(2).position(|pair| t <= pair[1][0]).unwrap_or(keys.len() - 2);
    // The slope across each stretch between keys, and at each key the average of those either side, held flat
    // where they turn and kept small enough not to overshoot.
    let secant = |i: usize| {
        let (a, b) = (keys[i], keys[i + 1]);
        if b[0] - a[0] > 1e-6 { (b[1] - a[1]) / (b[0] - a[0]) } else { 0.0 }
    };
    let tangent = |i: usize| {
        if i == 0 {
            return secant(0);
        }
        if i == keys.len() - 1 {
            return secant(i - 1);
        }
        let (before, after) = (secant(i - 1), secant(i));
        if before * after <= 0.0 { 0.0 } else { (before + after) * 0.5 }
    };
    let slope = secant(at);
    let (mut m0, mut m1) = (tangent(at), tangent(at + 1));
    if slope == 0.0 {
        (m0, m1) = (0.0, 0.0);
    } else {
        let (alpha, beta) = (m0 / slope, m1 / slope);
        let size = alpha * alpha + beta * beta;
        if size > 9.0 {
            let shrink = 3.0 / size.sqrt();
            (m0, m1) = (alpha * shrink * slope, beta * shrink * slope);
        }
    }
    let (a, b) = (keys[at], keys[at + 1]);
    let span = b[0] - a[0];
    if span <= 1e-6 {
        return b[1];
    }
    let s = (t - a[0]) / span;
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    h00 * a[1] + h10 * span * m0 + h01 * b[1] + h11 * span * m1
}

impl WorldGen {
    /// The preset it is shaped as, whatever its seed, scale, sea level and
    /// height; none once its shape is its own.
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|preset| {
            let shaped = WorldGen {
                seed: self.seed,
                scale: self.scale,
                sea_level: self.sea_level,
                height: self.height,
                ..preset.world()
            };
            shaped == *self
        })
    }

    /// Shaped as `preset`, keeping its seed and scale.
    pub fn with_preset(&self, preset: Preset) -> Self {
        WorldGen { seed: self.seed, scale: self.scale, ..preset.world() }
    }

    /// What it reads and makes at `(x, z)`, in metres in its own world.
    pub fn sample(&self, x: f32, z: f32) -> Sample {
        let scale = self.scale.max(0.01);
        let fractal = |style: NoiseStyle, size: f32, octaves: u32, part: u32, (x, z): (f32, f32)| {
            let size = (size * scale).max(1.0);
            noise::fractal(style, x / size, z / size, octaves, seed_of(self.seed, part))
        };
        // The world bent before its fields are read, the same for all of them.
        let (wx, wz) = match self.warp.strength > 0.0 {
            true => {
                let reach = self.warp.strength * scale;
                let bend = |part: u32| signed(fractal(NoiseStyle::Fractal, self.warp.size, 3, part, (x, z))) * reach;
                (x + bend(1), z + bend(2))
            }
            false => (x, z),
        };
        let read =
            |field: Field, part: u32| signed(fractal(NoiseStyle::Fractal, field.size, field.octaves, part, (wx, wz)));
        let continents = read(self.continents, 3);
        let ruggedness = read(self.ruggedness, 4);
        // Ridged noise runs its ridges along where the noise under it crosses its middle: long lines that wind and
        // branch, as ranges do.
        let size = (self.ridges.size * scale).max(1.0);
        let soft = (1.0 - self.sharpness.clamp(0.0, 1.0)) * 0.35;
        let ridged = ridged(wx / size, wz / size, self.ridges.octaves, seed_of(self.seed, 5), soft);
        let peaks = ((ridged - RIDGE_MIDDLE) * RIDGE_SPREAD).clamp(-1.0, 1.0);
        let across = (continents + 1.0) * 0.5;
        let ground = smooth(&self.coast, across, 0.0);
        let relief =
            (smooth(&self.inland, across, 1.0) * smooth(&self.relief, (ruggedness + 1.0) * 0.5, 1.0)).clamp(0.0, 1.0);
        let shape = smooth(&self.peaks, (peaks + 1.0) * 0.5, 0.0);
        let mut share = ground + relief * shape;
        if self.crags > 0.0 && peaks > 0.0 {
            let crag = fractal(NoiseStyle::Ridged, self.ridges.size / 8.0, 3, 6, (wx, wz));
            share += self.crags * relief * peaks * peaks * crag;
        }
        let mut height = self.sea_level + share * self.height;
        if self.bumps != 0.0 {
            let bump = signed(fractal(NoiseStyle::Fractal, 30.0, 4, 7, (x, z)));
            height += self.bumps * bump * (0.3 + 0.7 * relief);
        }
        Sample { continents, ruggedness, peaks, height }
    }

    /// The ground's height at `(x, z)`, in metres in its own world.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.sample(x, z).height
    }

    /// As read from a `.worldgen` file's text.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// As a `.worldgen` file's text.
    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_makes_the_same_ground_to_the_bit_and_another_seed_other_ground() {
        let world = Preset::Alpine.world();
        let heights: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        let again: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        assert_eq!(heights, again);
        // The same bits on every machine: a hash of them as they were made here.
        let hash = crate::layers::stable_hash(bytemuck::cast_slice(&heights));
        assert_eq!(hash, 0x6820014bf4529923, "the ground changed: was it meant to? {hash:#x}");
        let other = WorldGen { seed: 2, ..world.clone() };
        assert_ne!(other.height_at(100.0, 100.0), world.height_at(100.0, 100.0));
    }

    #[test]
    fn a_smooth_curve_goes_through_its_keys_and_never_past_them() {
        let curve = Curve::new(&[[0.0, 0.0], [0.3, 1.0], [0.6, 1.0], [1.0, -0.5]]);
        for key in curve.keys() {
            assert!((smooth(&curve, key[0], 9.0) - key[1]).abs() < 1e-6);
        }
        for i in 0..=1000 {
            let t = i as f32 / 1000.0;
            let v = smooth(&curve, t, 9.0);
            assert!((-0.5..=1.0).contains(&v), "{t}: {v}");
        }
        // Flat between two keys of the same value, and nothing at all for no keys.
        assert!((smooth(&curve, 0.45, 9.0) - 1.0).abs() < 1e-6);
        assert_eq!(smooth(&Curve::default(), 0.5, 9.0), 9.0);
    }

    #[test]
    fn fields_fill_their_range_and_presets_are_found_by_their_shape() {
        let world = Preset::Archipelago.world();
        let (mut low, mut high, mut sea, mut peaks_low, mut peaks_high) = (0, 0, 0, 0, 0);
        for i in 0..20_000 {
            let s = world.sample((i % 200) as f32 * 61.0, (i / 200) as f32 * 61.0);
            peaks_low += usize::from(s.peaks < -0.6);
            peaks_high += usize::from(s.peaks > 0.6);
            low += usize::from(s.continents < -0.6);
            high += usize::from(s.continents > 0.6);
            sea += usize::from(s.height < world.sea_level);
            assert!((-1.0..=1.0).contains(&s.peaks));
        }
        assert!(low > 1000 && high > 1000, "continents reach both ends: {low} low, {high} high");
        assert!(peaks_low > 1000 && peaks_high > 1000, "valleys and crests: {peaks_low} low, {peaks_high} high");
        assert!((5_000..17_000).contains(&sea), "an archipelago is mostly sea: {sea} of 20,000");
        // Changing its seed, scale, sea level or height keeps its preset; changing its shape does not.
        let moved = WorldGen { seed: 9, scale: 2.0, sea_level: 10.0, height: 50.0, ..world.clone() };
        assert_eq!(WorldGen { sharpness: 0.1, ..world.clone() }.preset(), None);
        assert_eq!(moved.preset(), Some(Preset::Archipelago));
        assert_eq!(WorldGen { crags: 0.5, ..world }.preset(), None);
        // Read back from its file as written.
        let alpine = Preset::Alpine.world();
        assert_eq!(WorldGen::parse(&alpine.to_text()).unwrap(), alpine);
    }
}
