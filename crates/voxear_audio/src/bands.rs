//! A voice heard past walls: its sound split into Steam Audio's three bands
//! of pitch, below 800 Hz, up to 8 kHz and above, each at the volume that
//! gets through (see [`Muffle`]). Two one-pole filters split it, so the
//! three add up to the sound as it was, and a voice in the open is not
//! changed. A new muffle slides in over a few milliseconds.

use std::f32::consts::TAU;

use crate::steam_audio::Muffle;

/// How far each sample moves the bands' volumes to their new ones.
const SLIDE: f32 = 1.0 / 256.0;

pub(crate) struct Bands {
    target: [f32; 3],
    now: [f32; 3],
    /// The sound below each split, as the filters have it so far.
    below_middle: f32,
    below_high: f32,
    middle_step: f32,
    high_step: f32,
}

impl Bands {
    pub fn new(sample_rate: u32, muffle: Muffle) -> Box<Self> {
        let step = |hz: f32| 1.0 - (-TAU * hz / sample_rate.max(1) as f32).exp();
        Box::new(Self {
            target: muffle.0,
            now: muffle.0,
            below_middle: 0.0,
            below_high: 0.0,
            middle_step: step(800.0),
            high_step: step(8_000.0),
        })
    }

    pub fn set(&mut self, muffle: Muffle) {
        self.target = muffle.0;
    }

    pub fn target(&self) -> Muffle {
        Muffle(self.target)
    }

    pub fn apply(&mut self, sample: f32) -> f32 {
        for (now, target) in self.now.iter_mut().zip(self.target) {
            *now += (target - *now) * SLIDE;
        }
        self.below_middle += (sample - self.below_middle) * self.middle_step;
        self.below_high += (sample - self.below_high) * self.high_step;
        let [low, middle, high] = [self.below_middle, self.below_high - self.below_middle, sample - self.below_high];
        low * self.now[0] + middle * self.now[1] + high * self.now[2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, at: usize) -> f32 {
        (TAU * hz * at as f32 / 48_000.0).sin()
    }

    /// How loud a sine at `hz` comes out, once the filters have settled.
    fn level(muffle: Muffle, hz: f32) -> f32 {
        let mut bands = Bands::new(48_000, muffle);
        let out: Vec<f32> = (0..9_600).map(|at| bands.apply(sine(hz, at))).collect();
        out[4_800..].iter().fold(0.0f32, |most, s| most.max(s.abs()))
    }

    #[test]
    fn in_the_open_a_sound_is_not_changed() {
        let mut bands = Bands::new(48_000, Muffle::NONE);
        for at in 0..1_000 {
            let sample = sine(440.0, at) * 0.5 + sine(9_000.0, at) * 0.25;
            assert!((bands.apply(sample) - sample).abs() < 1e-5);
        }
    }

    #[test]
    fn a_muffle_turns_down_each_band_by_its_own() {
        let dull = Muffle([1.0, 0.5, 0.0]);
        assert!(level(dull, 100.0) > 0.85, "low notes through: {}", level(dull, 100.0));
        assert!(level(dull, 15_000.0) < 0.35, "high ones mostly not: {}", level(dull, 15_000.0));
        assert!(level(Muffle([0.0; 3]), 1_000.0) < 1e-3, "silence");
    }
}
