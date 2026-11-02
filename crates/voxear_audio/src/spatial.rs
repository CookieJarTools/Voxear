//! Where a 3D voice is heard from: how much quieter its distance makes it,
//! and how far to the listener's left or right it sits.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

use voxear_core::math::{Quat, Vec3};

/// Who hears the game: the camera, or an Obj with an Audio Listener. Like
/// every Obj it faces its +Z, with +X to its right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Default for Listener {
    fn default() -> Self {
        Self { position: Vec3::ZERO, rotation: Quat::IDENTITY }
    }
}

/// How a 3D voice fades with distance: at full volume out to `min`, then
/// quieter as sound gets in the open (half as loud each time the distance
/// doubles), and silent from `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distance {
    pub min: f32,
    pub max: f32,
}

impl Default for Distance {
    fn default() -> Self {
        Self { min: 1.0, max: 50.0 }
    }
}

impl Distance {
    /// The voice's volume at `distance`, from 1 to 0. The curve is the
    /// inverse of the distance, moved down to reach nothing at `max`, so a
    /// voice stops being heard at the edge rather than being cut off there.
    pub fn falloff(&self, distance: f32) -> f32 {
        let min = self.min.max(0.01);
        let max = self.max.max(min + 0.01);
        if distance <= min {
            return 1.0;
        }
        if distance >= max {
            return 0.0;
        }
        let edge = min / max;
        ((min / distance - edge) / (1.0 - edge)).clamp(0.0, 1.0)
    }
}

/// The left and right volumes of a voice at `at`, before distance: equal
/// power across, so it is as loud panned as in front. A voice inside its
/// `min` distance pans less the nearer it is, as a sound all around the
/// listener does not come from one side.
pub fn pan(listener: &Listener, at: Vec3, distance: &Distance) -> [f32; 2] {
    let offset = at - listener.position;
    let length = offset.length();
    if length < 1e-4 {
        return [FRAC_1_SQRT_2; 2];
    }
    let right = listener.rotation * Vec3::X;
    let near = (length / distance.min.max(0.01)).min(1.0);
    let side = (offset / length).dot(right).clamp(-1.0, 1.0) * near;
    // 0 is hard left, a quarter turn hard right.
    let angle = (side + 1.0) * FRAC_PI_4;
    [angle.cos(), angle.sin()]
}

/// A 3D voice's left and right volumes from the listener: its pan, then
/// its distance.
pub fn gains(listener: &Listener, at: Vec3, distance: &Distance) -> [f32; 2] {
    let fade = distance.falloff(listener.position.distance(at));
    pan(listener, at, distance).map(|side| side * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn falloff_is_full_inside_min_halves_with_distance_and_ends_at_max() {
        let distance = Distance { min: 1.0, max: 1000.0 };
        assert_eq!(distance.falloff(0.5), 1.0);
        assert_eq!(distance.falloff(1.0), 1.0);
        // Far from max, the curve is close to 1 / distance.
        assert!((distance.falloff(2.0) - 0.5).abs() < 0.001, "{}", distance.falloff(2.0));
        assert!((distance.falloff(4.0) - 0.25).abs() < 0.001);
        assert_eq!(distance.falloff(1000.0), 0.0);
        assert_eq!(distance.falloff(5000.0), 0.0);

        let near = Distance { min: 2.0, max: 10.0 };
        let mut last = 1.0;
        for step in 1..=100 {
            let volume = near.falloff(2.0 + step as f32 * 0.08);
            assert!(volume <= last, "never louder further away");
            last = volume;
        }
        assert!(close(last, 0.0), "reaches nothing at max: {last}");
    }

    #[test]
    fn panning_follows_the_listeners_right_hand() {
        let listener = Listener::default();
        let distance = Distance::default();
        let [left, right] = pan(&listener, Vec3::new(10.0, 0.0, 0.0), &distance);
        assert!(close(left, 0.0) && close(right, 1.0), "to the right: {left} {right}");
        let [left, right] = pan(&listener, Vec3::new(-10.0, 0.0, 0.0), &distance);
        assert!(close(left, 1.0) && close(right, 0.0), "to the left: {left} {right}");
        let [left, right] = pan(&listener, Vec3::new(0.0, 0.0, 10.0), &distance);
        assert!(close(left, right) && close(left * left + right * right, 1.0), "ahead, equal power: {left} {right}");

        // Turned to face +X, the same sound is now ahead.
        let turned = Listener { rotation: Quat::from_rotation_y(90f32.to_radians()), ..listener };
        let [left, right] = pan(&turned, Vec3::new(10.0, 0.0, 0.0), &distance);
        assert!(close(left, right), "{left} {right}");
        // And one at -Z is on its right.
        let [left, right] = pan(&turned, Vec3::new(0.0, 0.0, -10.0), &distance);
        assert!(right > 0.99 && left < 0.01, "{left} {right}");
    }

    #[test]
    fn a_voice_inside_its_min_distance_pans_less() {
        let listener = Listener::default();
        let distance = Distance { min: 4.0, max: 50.0 };
        let [far_left, far_right] = pan(&listener, Vec3::new(4.0, 0.0, 0.0), &distance);
        let [near_left, near_right] = pan(&listener, Vec3::new(1.0, 0.0, 0.0), &distance);
        assert!(far_right > near_right && near_right > near_left && near_left > far_left);
        assert_eq!(pan(&listener, Vec3::ZERO, &distance), [FRAC_1_SQRT_2; 2], "on top of the listener: centred");
    }

    #[test]
    fn gains_fade_the_pan_by_distance() {
        let listener = Listener::default();
        let distance = Distance { min: 1.0, max: 20.0 };
        let [left, right] = gains(&listener, Vec3::new(30.0, 0.0, 0.0), &distance);
        assert_eq!((left, right), (0.0, 0.0), "beyond max: silent");
        let [left, right] = gains(&listener, Vec3::new(0.0, 0.0, 5.0), &distance);
        let fade = distance.falloff(5.0);
        assert!(close(left, FRAC_1_SQRT_2 * fade) && close(right, left));
    }
}
