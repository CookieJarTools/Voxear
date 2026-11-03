//! The room the listener is in, heard: a stereo reverb in the manner of
//! Freeverb, eight comb filters and four all-pass filters a side, whose
//! decay follows how long sound lingers there in each band of pitch, as
//! Steam Audio works out from the level's walls (see
//! [`steam_audio::RoomReverb`](crate::steam_audio::RoomReverb)). A hall
//! rings on for seconds; a small room of carpet barely at all; outdoors
//! there is next to nothing.
//!
//! The 3D voices on the effects bus are sent to it in mono, and what it
//! gives back is mixed in on both sides. A change of room slides in over a
//! few blocks, so walking through a door does not click.

/// Freeverb's delays at 44.1 kHz, in samples, and how many more the right
/// side's are, so the two sides are not the same.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALL_PASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const ALL_PASS_FEEDBACK: f32 = 0.5;
/// How loud the reverb is against what is sent to it: Freeverb's own
/// scaling of its input.
const INPUT: f32 = 0.015;
/// How far each sample moves the decay to the room's new one.
const SLIDE: f32 = 1.0 / 4096.0;
/// The longest a room may ring for, in seconds, so a level open to the
/// sky that Steam Audio finds nothing in cannot ring for ever.
const LONGEST: f32 = 8.0;
/// A tenth of a second, as Steam Audio has it in the open, is no room.
const SHORTEST: f32 = 0.1;

/// How long sound lingers, in seconds, in each of Steam Audio's three bands
/// of pitch, low, middle and high: the time it takes to fall by 60 dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbTimes(pub [f32; 3]);

impl ReverbTimes {
    /// No room at all: nothing rings.
    pub const NONE: Self = Self([0.0; 3]);
}

struct Comb {
    line: Vec<f32>,
    at: usize,
    /// The feedback's lowpass, which dulls each pass round, as walls soak
    /// up the high notes first.
    stored: f32,
}

struct AllPass {
    line: Vec<f32>,
    at: usize,
}

pub(crate) struct Reverb {
    sample_rate: f32,
    combs: [Vec<Comb>; 2],
    all_passes: [Vec<AllPass>; 2],
    target: ReverbTimes,
    now: ReverbTimes,
}

impl Reverb {
    pub fn new(sample_rate: u32, times: ReverbTimes) -> Box<Self> {
        let scale = sample_rate.max(1) as f32 / 44_100.0;
        let length = |samples: usize, side: usize| ((samples + side * SPREAD) as f32 * scale).max(1.0) as usize;
        let combs = [0, 1]
            .map(|side| COMBS.iter().map(|&n| Comb { line: vec![0.0; length(n, side)], at: 0, stored: 0.0 }).collect());
        let all_passes = [0, 1]
            .map(|side| ALL_PASSES.iter().map(|&n| AllPass { line: vec![0.0; length(n, side)], at: 0 }).collect());
        Box::new(Self { sample_rate: sample_rate.max(1) as f32, combs, all_passes, target: times, now: times })
    }

    pub fn set(&mut self, times: ReverbTimes) {
        self.target = ReverbTimes(times.0.map(|seconds| seconds.clamp(0.0, LONGEST)));
    }

    /// Takes the next sample sent to it, and gives the left and right of
    /// the room's answer.
    pub fn apply(&mut self, sent: f32) -> [f32; 2] {
        for (now, target) in self.now.0.iter_mut().zip(self.target.0) {
            *now += (target - *now) * SLIDE;
        }
        let [_, middle, high] = self.now.0;
        // The high band dying sooner than the middle one is what the feedback's lowpass is for.
        let damping = if middle > 1e-3 { (1.0 - high / middle).clamp(0.0, 0.9) } else { 0.0 };
        // A room that hardly rings fades to nothing rather than passing each sound once round. Steam
        // Audio's shortest time, outdoors, is SHORTEST.
        let input = sent * INPUT * ((middle - SHORTEST) * 5.0).clamp(0.0, 1.0);
        let rate = self.sample_rate;
        [0, 1].map(|side| {
            let mut out = 0.0;
            for comb in &mut self.combs[side] {
                out += comb.pass(input, feedback(comb.line.len(), middle, rate), damping);
            }
            self.all_passes[side].iter_mut().fold(out, |sound, all_pass| all_pass.pass(sound))
        })
    }
}

/// How much of a comb's sound comes round again, for it to fall by 60 dB
/// in `seconds`.
fn feedback(delay: usize, seconds: f32, rate: f32) -> f32 {
    if seconds <= 1e-3 {
        return 0.0;
    }
    10f32.powf(-3.0 * delay as f32 / (seconds * rate)).min(0.98)
}

impl Comb {
    fn pass(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let out = self.line[self.at];
        self.stored = out * (1.0 - damping) + self.stored * damping;
        self.line[self.at] = input + self.stored * feedback;
        self.at = (self.at + 1) % self.line.len();
        out
    }
}

impl AllPass {
    fn pass(&mut self, input: f32) -> f32 {
        let held = self.line[self.at];
        self.line[self.at] = input + held * ALL_PASS_FEEDBACK;
        self.at = (self.at + 1) % self.line.len();
        held - input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How loud the reverb still is `after` seconds from a click.
    fn ringing(times: ReverbTimes, after: f32) -> f32 {
        let mut reverb = Reverb::new(48_000, times);
        let mut loudest = 0.0f32;
        let start = (after * 48_000.0) as usize;
        for at in 0..start + 4_800 {
            let [left, right] = reverb.apply(if at == 0 { 1.0 } else { 0.0 });
            if at >= start {
                loudest = loudest.max(left.abs().max(right.abs()));
            }
        }
        loudest
    }

    #[test]
    fn a_long_reverb_rings_on_and_none_is_silent() {
        assert!(ringing(ReverbTimes([0.1; 3]), 0.0) < 1e-6, "outdoors, as Steam Audio has it");
        let hall = ringing(ReverbTimes([3.0; 3]), 1.0);
        let closet = ringing(ReverbTimes([0.3; 3]), 1.0);
        assert!(hall > closet * 100.0, "a hall still rings a second on: {hall} against {closet}");
        assert!(ringing(ReverbTimes::NONE, 0.0) < 1e-6, "no room, no reverb");
    }

    #[test]
    fn the_two_sides_differ() {
        let mut reverb = Reverb::new(48_000, ReverbTimes([2.0; 3]));
        let out: Vec<[f32; 2]> = (0..9_600).map(|at| reverb.apply(if at == 0 { 1.0 } else { 0.0 })).collect();
        assert!(out.iter().any(|[left, right]| (left - right).abs() > 1e-4));
    }
}
