//! Springs: a value that follows a goal as a weight on a spring does, with
//! the overshoot, settling and lag that make procedural motion look alive.
//!
//! A spring's feel is three numbers ([`SpringFeel`]):
//! - **frequency**, in hertz: how fast it moves;
//! - **damping**: below 1 it overshoots and rings, 1 settles as fast as it
//!   can without overshooting, above 1 creeps in;
//! - **response**: how it answers the goal jumping. 0 starts from rest, 1
//!   heads off at once, above 1 overshoots at the start, and below 0 starts
//!   the wrong way first, as anticipation.
//!
//! Each step is solved exactly rather than integrated, so a spring moves the
//! same at any frame rate and never blows up on a long frame.

use glam::{Quat, Vec2, Vec3};

/// How a spring moves: see the module's notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringFeel {
    pub frequency: f32,
    pub damping: f32,
    pub response: f32,
}

impl SpringFeel {
    pub const fn new(frequency: f32, damping: f32, response: f32) -> Self {
        Self { frequency, damping, response }
    }

    /// Settles without overshooting, halfway to its goal in `halflife`
    /// seconds.
    pub fn halflife(halflife: f32) -> Self {
        let omega = 2.0 * std::f32::consts::LN_2 / halflife.max(1e-5);
        Self { frequency: omega / std::f32::consts::TAU, damping: 1.0, response: 0.0 }
    }

    fn omega(&self) -> f32 {
        std::f32::consts::TAU * self.frequency.max(1e-4)
    }

    /// How a step of `dt` carries an offset from the goal and a velocity:
    /// the offset after is `a·offset + b·velocity`, the velocity after
    /// `c·offset + d·velocity`.
    fn step(&self, dt: f32) -> [f32; 4] {
        let (w, z, t) = (self.omega(), self.damping.max(0.0), dt.max(0.0));
        if (z - 1.0).abs() < 1e-4 {
            let e = (-w * t).exp();
            return [e * (1.0 + w * t), e * t, -e * w * w * t, e * (1.0 - w * t)];
        }
        if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            let (e, cos, sin) = ((-z * w * t).exp(), (wd * t).cos(), (wd * t).sin());
            return [e * (cos + z * w / wd * sin), e * sin / wd, -e * w * w / wd * sin, e * (cos - z * w / wd * sin)];
        }
        let root = (z * z - 1.0).sqrt();
        let (r1, r2) = (-w * (z - root), -w * (z + root));
        let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
        let gap = r1 - r2;
        [(r1 * e2 - r2 * e1) / gap, (e1 - e2) / gap, r1 * r2 * (e2 - e1) / gap, (r1 * e1 - r2 * e2) / gap]
    }

    /// The velocity a goal's jump of `by` gives at once, by the response.
    fn kick(&self) -> f32 {
        self.response * self.damping * self.omega()
    }
}

impl Default for SpringFeel {
    /// Quick, with a little overshoot.
    fn default() -> Self {
        Self::new(3.0, 0.6, 0.0)
    }
}

/// What a spring can carry: anything that adds and scales.
pub trait SpringValue: Copy + PartialEq + std::ops::Add<Output = Self> + std::ops::Sub<Output = Self> {
    fn scaled(self, by: f32) -> Self;
    fn zero() -> Self;
}

impl SpringValue for f32 {
    fn scaled(self, by: f32) -> Self {
        self * by
    }
    fn zero() -> Self {
        0.0
    }
}

impl SpringValue for Vec2 {
    fn scaled(self, by: f32) -> Self {
        self * by
    }
    fn zero() -> Self {
        Vec2::ZERO
    }
}

impl SpringValue for Vec3 {
    fn scaled(self, by: f32) -> Self {
        self * by
    }
    fn zero() -> Self {
        Vec3::ZERO
    }
}

/// A value following a goal on a spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring<T: SpringValue> {
    pub value: T,
    pub velocity: T,
    pub feel: SpringFeel,
    goal: T,
    /// A push spread over the next frames, so a kick has no one-frame pop:
    /// the velocity still to give, and over how long.
    pending: T,
    pending_for: f32,
}

impl<T: SpringValue> Spring<T> {
    /// A spring at rest at `value`.
    pub fn new(value: T, feel: SpringFeel) -> Self {
        Self { value, velocity: T::zero(), feel, goal: value, pending: T::zero(), pending_for: 0.0 }
    }

    /// The goal it last followed.
    pub fn goal(&self) -> T {
        self.goal
    }

    /// Moves `dt` seconds toward `goal`, and gives back where it is.
    pub fn update(&mut self, goal: T, dt: f32) -> T {
        if goal != self.goal {
            self.velocity = self.velocity + (goal - self.goal).scaled(self.feel.kick());
            self.goal = goal;
        }
        if self.pending_for > 0.0 {
            let share = (dt / self.pending_for).min(1.0);
            let given = self.pending.scaled(share);
            self.velocity = self.velocity + given;
            self.pending = self.pending - given;
            self.pending_for = (self.pending_for - dt).max(0.0);
        }
        let [a, b, c, d] = self.feel.step(dt);
        let offset = self.value - goal;
        self.value = goal + offset.scaled(a) + self.velocity.scaled(b);
        self.velocity = offset.scaled(c) + self.velocity.scaled(d);
        self.value
    }

    /// Adds to its velocity at once, as a blow does.
    pub fn kick(&mut self, velocity: T) {
        self.velocity = self.velocity + velocity;
    }

    /// Adds to its velocity over `seconds`, a softer blow.
    pub fn push(&mut self, velocity: T, seconds: f32) {
        if seconds <= 0.0 {
            return self.kick(velocity);
        }
        self.pending = self.pending + velocity;
        self.pending_for = self.pending_for.max(seconds);
    }

    /// Puts it at `value`, at rest, following the same goal.
    pub fn snap(&mut self, value: T) {
        self.value = value;
        self.goal = value;
        self.velocity = T::zero();
        self.pending = T::zero();
        self.pending_for = 0.0;
    }
}

/// A turn following a goal on a spring, as a turn by an axis and angle
/// from the goal that springs to nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotationSpring {
    pub value: Quat,
    offset: Spring<Vec3>,
}

impl RotationSpring {
    pub fn new(value: Quat, feel: SpringFeel) -> Self {
        Self { value, offset: Spring::new(Vec3::ZERO, feel) }
    }

    pub fn feel_mut(&mut self) -> &mut SpringFeel {
        &mut self.offset.feel
    }

    /// Moves `dt` seconds toward `goal`, and gives back where it is.
    pub fn update(&mut self, goal: Quat, dt: f32) -> Quat {
        let mut from = (self.value * goal.inverse()).normalize();
        if from.w < 0.0 {
            from = -from;
        }
        self.offset.value = from.to_scaled_axis();
        let offset = self.offset.update(Vec3::ZERO, dt);
        self.value = (Quat::from_scaled_axis(offset) * goal).normalize();
        self.value
    }

    /// Spins it at once by an angular velocity, as an axis scaled by
    /// radians a second.
    pub fn kick(&mut self, spin: Vec3) {
        self.offset.kick(spin);
    }

    pub fn push(&mut self, spin: Vec3, seconds: f32) {
        self.offset.push(spin, seconds);
    }

    pub fn snap(&mut self, value: Quat) {
        self.value = value;
        self.offset.snap(Vec3::ZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(feel: SpringFeel, fps: f32, seconds: f32) -> f32 {
        let mut spring = Spring::new(0.0, feel);
        let steps = (seconds * fps).round() as usize;
        for _ in 0..steps {
            spring.update(1.0, 1.0 / fps);
        }
        spring.value
    }

    #[test]
    fn a_spring_moves_the_same_at_any_frame_rate_and_stays_put_on_a_long_frame() {
        for feel in [SpringFeel::new(2.0, 0.3, 0.0), SpringFeel::new(4.0, 1.0, 0.5), SpringFeel::new(1.5, 2.5, -0.5)] {
            let (thirty, sixty, fast) = (run(feel, 30.0, 0.6), run(feel, 60.0, 0.6), run(feel, 150.0, 0.6));
            assert!((thirty - sixty).abs() < 1e-4 && (sixty - fast).abs() < 1e-4, "{feel:?}: {thirty} {sixty} {fast}");
            let mut spring = Spring::new(0.0, feel);
            let long = spring.update(1.0, 30.0);
            assert!((long - 1.0).abs() < 1e-3 && long.is_finite(), "{feel:?} settles on a long frame: {long}");
        }
    }

    #[test]
    fn damping_below_one_overshoots_and_a_negative_response_starts_the_wrong_way() {
        let mut bouncy = Spring::new(0.0, SpringFeel::new(2.0, 0.2, 0.0));
        let highest = (0..120).map(|_| bouncy.update(1.0, 1.0 / 60.0)).fold(0.0, f32::max);
        assert!(highest > 1.3, "{highest}");
        let mut firm = Spring::new(0.0, SpringFeel::halflife(0.1));
        let highest = (0..120).map(|_| firm.update(1.0, 1.0 / 60.0)).fold(0.0, f32::max);
        assert!(highest <= 1.0 + 1e-4, "critical damping never overshoots: {highest}");
        assert!((firm.value - 1.0).abs() < 1e-3);
        let mut halfway = Spring::new(0.0, SpringFeel::halflife(0.2));
        for _ in 0..12 {
            halfway.update(1.0, 1.0 / 60.0);
        }
        assert!(halfway.value > 0.3 && halfway.value < 0.7, "about half in a half-life: {}", halfway.value);
        let mut wary = Spring::new(0.0, SpringFeel::new(3.0, 0.8, -1.0));
        let first = wary.update(1.0, 1.0 / 60.0);
        assert!(first < 0.0, "anticipation: {first}");
    }

    #[test]
    fn a_push_spreads_over_frames_and_a_rotation_springs_home() {
        let mut kicked = Spring::new(Vec3::ZERO, SpringFeel::new(4.0, 0.5, 0.0));
        let mut pushed = kicked;
        kicked.kick(Vec3::X * 2.0);
        pushed.push(Vec3::X * 2.0, 0.05);
        let (k, p) = (kicked.update(Vec3::ZERO, 1.0 / 60.0), pushed.update(Vec3::ZERO, 1.0 / 60.0));
        assert!(p.x > 0.0 && p.x < k.x, "a push starts gentler: {} {}", p.x, k.x);
        let mut turn = RotationSpring::new(Quat::from_rotation_y(1.0), SpringFeel::halflife(0.05));
        for _ in 0..60 {
            turn.update(Quat::IDENTITY, 1.0 / 60.0);
        }
        assert!(turn.value.angle_between(Quat::IDENTITY) < 1e-3);
    }
}
