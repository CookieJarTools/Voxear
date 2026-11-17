/// Longest frame the simulation will step, so a hitch or breakpoint does not
/// explode physics and animation.
pub const MAX_DELTA: f32 = 0.25;

/// Frame timing. Scripts read this struct in place, so its layout is part of
/// the C# ABI (`Voxear.Time`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Time {
    pub delta: f32,
    pub unscaled_delta: f32,
    pub fixed_delta: f32,
    pub time_scale: f32,
    pub elapsed: f64,
    pub frame: u64,
}

const _: () = assert!(size_of::<Time>() == 32);

impl Default for Time {
    fn default() -> Self {
        Self::new(1.0 / 50.0)
    }
}

impl Time {
    pub fn new(fixed_delta: f32) -> Self {
        Self { delta: 0.0, unscaled_delta: 0.0, fixed_delta, time_scale: 1.0, elapsed: 0.0, frame: 0 }
    }

    pub fn advance(&mut self, unscaled_delta: f32) {
        self.advance_within(unscaled_delta, MAX_DELTA);
    }

    /// Moves time on as [`advance`](Self::advance) does, a frame counting as
    /// no longer than `longest` seconds.
    pub fn advance_within(&mut self, unscaled_delta: f32, longest: f32) {
        self.unscaled_delta = unscaled_delta.clamp(0.0, longest.max(0.0));
        self.delta = self.unscaled_delta * self.time_scale;
        self.elapsed += f64::from(self.delta);
        self.frame += 1;
    }
}
