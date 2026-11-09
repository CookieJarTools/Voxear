//! Easing: how far along something has gone at a time from 0 to 1, for
//! blends, reaches and paths that start and stop softly.

use glam::Vec3;

/// `t` from 0 to 1, eased in and out: slow to start, slow to arrive.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The share of the way a person's reach has gone at `t`, 0 to 1: quick to
/// start, quickest midway, and soft to arrive, as hands move.
pub fn minimum_jerk(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (10.0 - 15.0 * t + 6.0 * t * t)
}

/// A smooth path through four points, from the first to the last, leaving
/// toward the second and coming in from the third, `s` of the way along.
pub fn bezier([a, b, c, d]: [Vec3; 4], s: f32) -> Vec3 {
    let r = 1.0 - s;
    a * (r * r * r) + b * (3.0 * r * r * s) + c * (3.0 * r * s * s) + d * (s * s * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reach_starts_and_ends_at_rest_and_is_quickest_midway() {
        assert_eq!((minimum_jerk(0.0), minimum_jerk(1.0)), (0.0, 1.0));
        let speed = |t: f32| (minimum_jerk(t + 0.01) - minimum_jerk(t)) / 0.01;
        assert!(speed(0.0) < 0.01 && speed(0.99) < 0.05);
        assert!(speed(0.5) > speed(0.25) && speed(0.5) > speed(0.75));
    }

    #[test]
    fn eased_values_stay_between_0_and_1_and_a_path_meets_its_ends() {
        assert_eq!((smooth(-1.0), smooth(0.5), smooth(2.0)), (0.0, 0.5, 1.0));
        let ends = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::ONE];
        assert_eq!((bezier(ends, 0.0), bezier(ends, 1.0)), (Vec3::ZERO, Vec3::ONE));
    }
}
