//! Steering along a way: heading for its next corner, past each as it is
//! reached, slowing to stop at its end, and changing speed no faster than
//! it may.

use voxear_core::math::{Vec2, Vec3};

/// How something steers along a way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steering {
    /// Its fastest, in metres a second.
    pub speed: f32,
    /// How quickly it speeds up and slows down, in metres a second each
    /// second.
    pub acceleration: f32,
    /// How near the end it stops, in metres.
    pub stopping: f32,
    /// Whether it slows down to stop at the end, or arrives at speed.
    pub brakes: bool,
    /// How near a corner it is past it, in metres.
    pub corner: f32,
}

impl Default for Steering {
    fn default() -> Self {
        Self { speed: 3.5, acceleration: 8.0, stopping: 0.0, brakes: true, corner: 0.3 }
}
}

/// Along the ground: `v` without its height.
pub fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// How far it is from `at` to the end of the way along its corners from
/// `next` on, on the ground.
pub fn remaining(at: Vec3, corners: &[Vec3], next: usize) -> f32 {
    let mut last = flat(at);
    let mut far = 0.0;
    for corner in corners.iter().skip(next) {
        far += (flat(*corner) - last).length();
        last = flat(*corner);
    }
    far
}

/// The next corner to head for from `at`: past `next` while it is within
/// `steering.corner`, or behind as the way goes on, so one overshot is not
/// walked back to.
pub fn next_corner(at: Vec3, corners: &[Vec3], mut next: usize, steering: &Steering) -> usize {
    while next + 1 < corners.len() {
        let corner = flat(corners[next]);
        let to = corner - flat(at);
        let past = next > 0 && {
            let leg = corner - flat(corners[next - 1]);
            leg.dot(to) < 0.0
        };
        if to.length() > steering.corner && !past {
            break;
        }
        next += 1;
    }
    next
}

/// The velocity it wants on the ground, heading for corner `next` of
/// `corners` from `at`: at its speed, less where it must slow to stop at
/// the end in time.
pub fn wanted(at: Vec3, corners: &[Vec3], next: usize, steering: &Steering) -> Vec2 {
    let Some(corner) = corners.get(next) else { return Vec2::ZERO };
    let way = flat(*corner) - flat(at);
    let left = remaining(at, corners, next) - steering.stopping;
    if left <= 0.01 || way.length() < 1e-4 {
        return Vec2::ZERO;
    }
    let mut speed = steering.speed;
    if steering.brakes {
        // As fast as it can go and still stop by the end.
        speed = speed.min((2.0 * steering.acceleration.max(0.1) * left).sqrt());
    }
    way.normalize() * speed.max(0.0)
}

/// `velocity` changed towards `wanted` by no more than `acceleration` for
/// `delta` seconds allows.
pub fn accelerate(velocity: Vec2, wanted: Vec2, acceleration: f32, delta: f32) -> Vec2 {
    let change = wanted - velocity;
    velocity + change.clamp_length_max(acceleration.max(0.0) * delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_heads_for_each_corner_in_turn_and_slows_to_stop_at_the_end() {
        let corners = [Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 6.0)];
        let steering = Steering::default();
        let mut at = Vec3::ZERO;
        let mut velocity = Vec2::ZERO;
        let mut next = 1;
        let delta = 1.0 / 60.0;
        let mut fastest: f32 = 0.0;
        for _ in 0..600 {
            next = next_corner(at, &corners, next, &steering);
            velocity = accelerate(velocity, wanted(at, &corners, next, &steering), steering.acceleration, delta);
            fastest = fastest.max(velocity.length());
            at += Vec3::new(velocity.x, 0.0, velocity.y) * delta;
        }
        assert_eq!(next, 2);
        assert!((at - corners[2]).length() < 0.05, "stopped at the end: {at}");
        assert!(velocity.length() < 0.05);
        assert!((fastest - steering.speed).abs() < 0.05, "at its speed between: {fastest}");
        assert_eq!(remaining(at, &corners, 2), (flat(corners[2]) - flat(at)).length());
    }
}
