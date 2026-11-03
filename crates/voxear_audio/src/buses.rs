//! What the music and effects buses do to everything on them, once mixed:
//! a low-pass and a high-pass filter each, and ducking, which turns the
//! music down while the effects are loud, as a game does under dialogue
//! or a big explosion. Volumes and filters slide to new settings over the
//! time a snapshot asks for (see `Audio::set_transition`).

use std::f32::consts::TAU;

/// The cutoffs at which a filter does nothing, in Hz: a low-pass at the
/// top of hearing, a high-pass at the bottom.
pub const OPEN_LOW_PASS: f32 = 22_000.0;
pub const OPEN_HIGH_PASS: f32 = 10.0;
/// The quality of the filters' resonance: Butterworth, flat, with no peak.
const Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How long the effects' level takes to fall once they go quiet, in
/// seconds, before the music starts to come back.
const LEVEL_FALL: f32 = 0.05;

/// How the music bus ducks under the effects bus: by `amount` decibels,
/// down from 0, while the effects are louder than `threshold` (from 0 to
/// 1, full), going down over `attack` seconds and back over `release`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ducking {
    pub amount: f32,
    pub threshold: f32,
    pub attack: f32,
    pub release: f32,
}

impl Default for Ducking {
    fn default() -> Self {
        Self { amount: -12.0, threshold: 0.1, attack: 0.05, release: 0.5 }
    }
}

/// A value moving to a new one at a steady pace: at once, or over the
/// seconds it was given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slide {
    pub now: f32,
    target: f32,
    per_second: f32,
}

impl Slide {
    pub fn at(value: f32) -> Self {
        Self { now: value, target: value, per_second: f32::INFINITY }
    }

    pub fn set(&mut self, target: f32, seconds: f32) {
        self.target = target;
        self.per_second = (target - self.now).abs() / seconds;
        if seconds <= 0.0 {
            self.now = target;
        }
    }

    /// Moves on by `seconds`, true while it still moves.
    pub fn advance(&mut self, seconds: f32) -> bool {
        if self.now == self.target {
            return false;
        }
        let step = self.per_second * seconds;
        self.now = if (self.target - self.now).abs() <= step {
            self.target
        } else {
            self.now + step.copysign(self.target - self.now)
        };
        self.now != self.target
    }
}

/// A two-pole filter, low-pass or high-pass, on both sides.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    /// The last two inputs and outputs, a pair for each side.
    x: [[f32; 2]; 2],
    y: [[f32; 2]; 2],
}

impl Biquad {
    /// Sets the filter's cutoff, keeping what it holds, as the Audio EQ
    /// Cookbook has its coefficients.
    fn tune(&mut self, high_pass: bool, hz: f32, rate: f32) {
        let w = TAU * hz.clamp(1.0, rate * 0.49) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * Q);
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        self.b = b.map(|b| b / a0);
        self.a = [-2.0 * cos / a0, (1.0 - alpha) / a0];
    }

    fn apply(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let mut out = [0.0; 2];
        for side in 0..2 {
            let [x1, x2] = self.x[side];
            let [y1, y2] = self.y[side];
            let x = frame[side];
            let y = self.b[0] * x + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
            self.x[side] = [x, x1];
            self.y[side] = [y, y1];
            out[side] = y;
        }
        out
    }
}

/// A bus's filters, and where their cutoffs are going.
pub(crate) struct BusFilters {
    pub low_pass: Slide,
    pub high_pass: Slide,
    low: Biquad,
    high: Biquad,
    tuned: Option<[f32; 2]>,
}

impl Default for BusFilters {
    fn default() -> Self {
        Self {
            low_pass: Slide::at(OPEN_LOW_PASS),
            high_pass: Slide::at(OPEN_HIGH_PASS),
            low: Biquad::default(),
            high: Biquad::default(),
            tuned: None,
        }
    }
}

impl BusFilters {
    /// Filters a block of the bus's mix in place, after moving the cutoffs
    /// on by the block's time.
    pub fn apply(&mut self, mix: &mut [[f32; 2]], rate: f32) {
        let seconds = mix.len() as f32 / rate;
        self.low_pass.advance(seconds);
        self.high_pass.advance(seconds);
        let cutoffs = [self.low_pass.now, self.high_pass.now];
        let low = cutoffs[0] < OPEN_LOW_PASS;
        let high = cutoffs[1] > OPEN_HIGH_PASS;
        if !low && !high {
            self.tuned = None;
            return;
        }
        if self.tuned != Some(cutoffs) {
            self.low.tune(false, cutoffs[0], rate);
            self.high.tune(true, cutoffs[1], rate);
            self.tuned = Some(cutoffs);
        }
        for frame in mix {
            if low {
                *frame = self.low.apply(*frame);
            }
            if high {
                *frame = self.high.apply(*frame);
            }
        }
    }
}

/// The music bus's gain under the effects bus: how loud the effects are,
/// followed, and how far the music is down for it.
pub(crate) struct Ducker {
    pub ducking: Option<Ducking>,
    level: f32,
    gain: f32,
}

impl Default for Ducker {
    fn default() -> Self {
        Self { ducking: None, level: 0.0, gain: 1.0 }
    }
}

impl Ducker {
    /// Turns the music down, a frame at a time, while the effects are loud.
    pub fn apply(&mut self, music: &mut [[f32; 2]], effects: &[[f32; 2]], rate: f32) {
        let Some(ducking) = self.ducking else {
            self.gain = 1.0;
            return;
        };
        let floor = 10f32.powf(ducking.amount.min(0.0) / 20.0);
        let step = |seconds: f32| 1.0 - (-1.0 / (seconds.max(1e-3) * rate)).exp();
        let (attack, release, fall) = (step(ducking.attack), step(ducking.release), step(LEVEL_FALL));
        for (out, [left, right]) in music.iter_mut().zip(effects) {
            let loud = left.abs().max(right.abs());
            // The level rises at once and falls slowly, so a loud sound is loud between its waves' peaks.
            self.level = loud.max(self.level - self.level * fall);
            let target = if self.level > ducking.threshold { floor } else { 1.0 };
            let pace = if target < self.gain { attack } else { release };
            self.gain += (target - self.gain) * pace;
            out[0] *= self.gain;
            out[1] *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, frames: usize) -> Vec<[f32; 2]> {
        (0..frames).map(|at| [(TAU * hz * at as f32 / 48_000.0).sin(); 2]).collect()
    }

    fn peak(frames: &[[f32; 2]]) -> f32 {
        frames.iter().fold(0.0f32, |most, [left, _]| most.max(left.abs()))
    }

    #[test]
    fn a_low_pass_lets_the_low_notes_by_and_holds_the_high_ones_back() {
        let mut filters = BusFilters::default();
        filters.low_pass.set(500.0, 0.0);
        let (mut low, mut high) = (sine(100.0, 9_600), sine(8_000.0, 9_600));
        filters.apply(&mut low, 48_000.0);
        let mut filters = BusFilters { low_pass: Slide::at(500.0), ..BusFilters::default() };
        filters.apply(&mut high, 48_000.0);
        assert!(peak(&low[4_800..]) > 0.9 && peak(&high[4_800..]) < 0.05, "{} {}", peak(&low), peak(&high));
    }

    #[test]
    fn open_filters_leave_the_sound_as_it_is() {
        let mut filters = BusFilters::default();
        let mut sound = sine(3_000.0, 480);
        let before = sound.clone();
        filters.apply(&mut sound, 48_000.0);
        assert_eq!(sound, before);
    }

    #[test]
    fn a_slide_takes_the_time_it_was_given() {
        let mut volume = Slide::at(1.0);
        volume.set(0.0, 2.0);
        assert!(volume.advance(1.0));
        assert!((volume.now - 0.5).abs() < 1e-6);
        assert!(!volume.advance(1.5));
        assert_eq!(volume.now, 0.0);
        volume.set(1.0, 0.0);
        assert!(!volume.advance(0.0) && volume.now == 1.0, "at once");
    }

    #[test]
    fn music_ducks_under_loud_effects_and_comes_back() {
        let mut ducker = Ducker { ducking: Some(Ducking { amount: -20.0, ..Ducking::default() }), ..Ducker::default() };
        let mut music = vec![[1.0f32; 2]; 48_000];
        let mut loud = vec![[0.5f32; 2]; 24_000];
        loud.extend(vec![[0.0f32; 2]; 24_000]);
        ducker.apply(&mut music, &loud, 48_000.0);
        assert!((music[20_000][0] - 0.1).abs() < 0.01, "down 20 dB under the effects: {}", music[20_000][0]);
        assert!(music[47_999][0] > 0.5, "and coming back once they stop: {}", music[47_999][0]);
    }
}
