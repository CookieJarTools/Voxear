//! The sound reaching the listener from the room round it, as rays traced
//! from where it stands find it: an energy field, a histogram of the energy
//! arriving in each 10 milliseconds after a sound, in each of the three
//! bands of pitch and from each side, laid out as Steam Audio's own
//! reflection simulation keeps one. The renderer's sound rays fill it on
//! the GPU, against the same scene the ray-traced GI holds (see
//! `voxear_render::sound_rays`); a reverb probe's bake fills one for each
//! probe.
//!
//! From a field come [`ReverbTimes`], fitted to how its energy decays as
//! Steam Audio fits its own, and the room's first few [`EarlyReflection`]s:
//! the loudest arrivals in its first 80 milliseconds, each with its delay,
//! how loud it is and which way it comes from, for the mixer's early
//! reflections to play.

use voxear_core::math::Vec3;

use crate::reverb::ReverbTimes;

/// How long one bin of the field is, in seconds: Steam Audio's 10 ms.
pub const BIN_SECONDS: f32 = 0.01;
/// How long after a sound the field follows, in seconds, and so its bins.
pub const FIELD_SECONDS: f32 = 2.0;
pub const BINS: usize = 200;
/// Its bands of pitch, low (below 800 Hz), middle and high (above 8 kHz).
pub const BANDS: usize = 3;
/// Its channels: all round, then how much more comes from +X, +Y and +Z
/// than from the other way, as first-order Ambisonics weighs them.
pub const CHANNELS: usize = 4;
/// The numbers in one field.
pub const FIELD_LEN: usize = CHANNELS * BANDS * BINS;
/// How long the early reflections last, as Steam Audio's reverb estimate
/// has them.
pub const EARLY_SECONDS: f32 = 0.08;
/// The most early reflections the mixer plays.
pub const MOST_EARLY: usize = 6;
/// Steam Audio's speed of sound, in metres a second.
pub const SPEED: f32 = 340.0;

/// What Steam Audio's default air absorption takes from each band, a metre.
const AIR: [f32; BANDS] = [0.0002, 0.0017, 0.0182];
/// The part of the decay a reverb time is fitted to: from a third of the
/// energy left (-5 dB) to a three-hundredth (-25 dB), as Steam Audio does.
const FIT: (f32, f32) = (-2.5, -0.5);

/// The energy arriving at a point after a sound made there, bin by bin
/// (see the module's notes). Laid out channel by channel, each band by
/// band, each bin by bin.
#[derive(Clone, Debug, PartialEq)]
pub struct EnergyField(pub Vec<f32>);

impl Default for EnergyField {
    fn default() -> Self {
        Self(vec![0.0; FIELD_LEN])
    }
}

impl EnergyField {
    /// A field from what the GPU added up: whole numbers, each `scale` of
    /// a unit of energy.
    pub fn from_fixed(numbers: &[i32], scale: f32) -> Self {
        let mut field = Self::default();
        for (energy, &number) in field.0.iter_mut().zip(numbers) {
            *energy = number as f32 / scale;
        }
        field
    }

    pub fn at(&self, channel: usize, band: usize, bin: usize) -> f32 {
        self.0[(channel * BANDS + band) * BINS + bin]
    }

    pub fn at_mut(&mut self, channel: usize, band: usize, bin: usize) -> &mut f32 {
        &mut self.0[(channel * BANDS + band) * BINS + bin]
    }

    /// The energy all round in one band, bin by bin.
    pub fn band(&self, band: usize) -> &[f32] {
        &self.0[band * BINS..(band + 1) * BINS]
    }

    /// The whole energy all round, in every band.
    pub fn total(&self) -> f32 {
        self.0[..BANDS * BINS].iter().sum()
    }

    /// Blends `newer` in, keeping `keep` of what it held: fields traced a
    /// frame at a time, with few rays each, add up to a steady one.
    pub fn blend(&mut self, newer: &Self, keep: f32) {
        for (held, &new) in self.0.iter_mut().zip(&newer.0) {
            *held = *held * keep + new * (1.0 - keep);
        }
    }

    /// How long the room rings in each band, fitted to how its energy
    /// falls away as Steam Audio fits its own; 0 in a band that holds
    /// next to nothing, as in the open.
    pub fn reverb_times(&self) -> ReverbTimes {
        ReverbTimes(std::array::from_fn(|band| reverb_time(self.band(band), band)))
    }

    /// The loudest arrivals in its first [`EARLY_SECONDS`], loudest first,
    /// at most [`MOST_EARLY`]. The sound itself is not in the field, only
    /// what the room sends back.
    pub fn early_reflections(&self) -> Vec<EarlyReflection> {
        let early = ((EARLY_SECONDS / BIN_SECONDS) as usize).min(BINS);
        let mut found: Vec<EarlyReflection> = (0..early)
            .filter_map(|bin| {
                let energy = self.at(0, 1, bin);
                (energy > 1e-6).then(|| {
                    let way = Vec3::new(self.at(1, 1, bin), self.at(2, 1, bin), self.at(3, 1, bin));
                    let bands = std::array::from_fn(|band| (self.at(0, band, bin) / energy).clamp(0.0, 4.0));
                    EarlyReflection {
                        delay: (bin as f32 + 0.5) * BIN_SECONDS,
                        // Amplitude is the root of energy.
                        gain: energy.sqrt().min(0.7),
                        bands,
                        from: way.try_normalize(),
                    }
                })
            })
            .collect();
        found.sort_by(|a, b| b.gain.total_cmp(&a.gain));
        found.truncate(MOST_EARLY);
        found
    }
}

/// How long one band rings: the energy decay curve, the energy still to
/// come at each bin, weighed by what the air takes on the way, and a line
/// fitted through its log from -5 to -25 dB; where that line reaches
/// -60 dB is the time.
fn reverb_time(histogram: &[f32], band: usize) -> f32 {
    let air = |seconds: f32| (-AIR[band] * seconds * SPEED).exp();
    let total: f32 = histogram.iter().enumerate().map(|(bin, e)| e * air(bin as f32 * BIN_SECONDS)).sum();
    if total < 1e-4 {
        return 0.0;
    }
    let (mut left, mut n) = (0.0f32, 0.0f32);
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for (bin, energy) in histogram.iter().enumerate().rev() {
        let x = bin as f32 * BIN_SECONDS;
        left += energy * air(x);
        let y = (left / total).max(1e-12).log10();
        if (FIT.0..=FIT.1).contains(&y) {
            (sx, sy, sxx, sxy, n) = (sx + x, sy + y, sxx + x * x, sxy + x * y, n + 1.0);
        }
    }
    let rise = n * sxy - sx * sy;
    let run = n * sxx - sx * sx;
    if n < 2.0 || rise.abs() <= f32::MIN_POSITIVE {
        return 0.0;
    }
    // The slope is in decades a second; -60 dB is six decades down.
    (-6.0 * run / rise).max(0.0)
}

/// One of the room's first echoes: how long after the sound it comes, in
/// seconds, how loud it is against the sound, how much of that each band
/// keeps against the middle one, and which way it comes from, in the
/// world, when it comes more from one side than from all round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarlyReflection {
    pub delay: f32,
    pub gain: f32,
    pub bands: [f32; BANDS],
    pub from: Option<Vec3>,
}

/// How much sound rays trace for a project or a level: how many rays from
/// the listener a frame, how many times each bounces, and how many rays
/// a volumetric source's occlusion takes. Higher hears smaller rooms and
/// passages more steadily, for more of the GPU's time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RayQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl RayQuality {
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// The most rays from the listener a frame; the budget may trace fewer.
    pub fn rays(self) -> u32 {
        match self {
            Self::Low => 256,
            Self::Medium => 1024,
            Self::High => 4096,
        }
    }

    /// How many times a ray bounces, and so how late it can still be heard.
    pub fn bounces(self) -> u32 {
        match self {
            Self::Low => 6,
            Self::Medium => 10,
            Self::High => 16,
        }
    }

    /// The rays across a volumetric source's sphere.
    pub fn occlusion_rays(self) -> u32 {
        match self {
            Self::Low => 4,
            Self::Medium => 8,
            Self::High => 16,
        }
    }

    /// How much of the field the frames before keep, as a new frame's is
    /// blended in: fewer rays need more frames to add up.
    pub fn keep(self) -> f32 {
        match self {
            Self::Low => 0.9,
            Self::Medium => 0.8,
            Self::High => 0.6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field whose energy all round falls by 60 dB in `seconds`, in each band.
    fn decaying(seconds: [f32; 3]) -> EnergyField {
        let mut field = EnergyField::default();
        for (band, rt) in seconds.into_iter().enumerate() {
            for bin in 1..BINS {
                let t = bin as f32 * BIN_SECONDS;
                *field.at_mut(0, band, bin) = 10f32.powf(-6.0 * t / rt);
            }
        }
        field
    }

    #[test]
    fn a_fields_reverb_times_are_fitted_to_how_it_decays() {
        let times = decaying([1.2, 0.8, 0.4]).reverb_times().0;
        // The air takes a little of the high band, so it rings a little shorter than its histogram says.
        assert!((times[0] - 1.2).abs() < 0.06 && (times[1] - 0.8).abs() < 0.04, "{times:?}");
        assert!(times[2] < 0.4 && times[2] > 0.3, "{times:?}");
        assert_eq!(EnergyField::default().reverb_times(), ReverbTimes::NONE, "nothing heard, nothing rings");
    }

    #[test]
    fn early_reflections_are_the_loudest_first_arrivals_from_their_sides() {
        let mut field = EnergyField::default();
        // A wall 3.4 m to the right answers at 20 ms; a fainter one behind at 50 ms.
        for (bin, energy, way) in [(2, 0.04, Vec3::X), (5, 0.01, -Vec3::Z), (30, 0.5, Vec3::Y)] {
            *field.at_mut(0, 1, bin) = energy;
            *field.at_mut(0, 0, bin) = energy * 2.0;
            for (channel, part) in [way.x, way.y, way.z].into_iter().enumerate() {
                *field.at_mut(channel + 1, 1, bin) = energy * part;
            }
        }
        let early = field.early_reflections();
        assert_eq!(early.len(), 2, "the one at 300 ms is the reverb's: {early:?}");
        assert!((early[0].delay - 0.025).abs() < 1e-6 && (early[0].gain - 0.2).abs() < 1e-6, "{early:?}");
        assert_eq!(early[0].from, Some(Vec3::X));
        assert_eq!(early[1].from, Some(-Vec3::Z));
        assert_eq!(early[0].bands, [2.0, 1.0, 0.0], "the low band twice the middle, no high band");
    }

    #[test]
    fn fields_traced_a_frame_at_a_time_blend_together() {
        let mut held = decaying([1.0; 3]);
        held.blend(&EnergyField::default(), 0.75);
        assert!((held.at(0, 1, 1) - decaying([1.0; 3]).at(0, 1, 1) * 0.75).abs() < 1e-6);
        let fixed = EnergyField::from_fixed(&[1 << 24, -(1 << 23)], (1 << 24) as f32);
        assert_eq!((fixed.0[0], fixed.0[1], fixed.0[2]), (1.0, -0.5, 0.0));
    }
}
