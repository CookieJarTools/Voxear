//! A scene-view camera for the development window, with Unity's controls:
//! hold the right mouse button to look around, and while holding it use WASD
//! to move, Q and E to go down and up, Shift to go faster, and the mouse
//! wheel to change speed. It flies faster the longer it flies without a
//! stop, so a nudge is fine and a long flight covers ground. It moves the
//! scene's camera Obj directly; the editor will give it a camera of its own.

use std::collections::HashSet;

use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::Camera;
use voxear_scene::Transform;
use winit::keyboard::KeyCode;

/// Radians of turn per pixel of mouse movement.
const LOOK_SENSITIVITY: f32 = 0.0025;
const FAST_MULTIPLIER: f32 = 3.0;
/// Each wheel notch scales speed by this much.
const SPEED_STEP: f32 = 1.25;
const SPEED_RANGE: (f32, f32) = (0.25, 250.0);
/// How many times faster it flies for each second flown without a stop.
const SPEED_UP: f32 = 1.8;
/// The most flying without a stop multiplies its speed by, reached in about
/// six seconds.
const SPEED_UP_LIMIT: f32 = 32.0;
const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;

pub struct FlyCamera {
    looking: bool,
    held: HashSet<KeyCode>,
    speed: f32,
    /// Whether it flies faster the longer it flies without a stop.
    speeds_up: bool,
    /// Seconds flown since it last stood still.
    flown: f32,
    yaw: f32,
    pitch: f32,
    /// Mouse movement not yet applied, since the camera's angles are only
    /// read from the world on the first update after looking starts.
    pending: (f32, f32),
    synced: bool,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            looking: false,
            held: HashSet::new(),
            speed: 6.0,
            speeds_up: true,
            flown: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            pending: (0.0, 0.0),
            synced: false,
        }
    }
}

impl FlyCamera {
    pub fn is_looking(&self) -> bool {
        self.looking
    }

    pub fn set_looking(&mut self, looking: bool) {
        self.looking = looking;
        if !looking {
            self.held.clear();
            self.flown = 0.0;
            self.synced = false;
            self.pending = (0.0, 0.0);
        }
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool) {
        if pressed && self.looking {
            self.held.insert(code);
        } else {
            self.held.remove(&code);
        }
    }

    pub fn look(&mut self, dx: f64, dy: f64) {
        if self.looking {
            self.pending.0 += dx as f32 * LOOK_SENSITIVITY;
            self.pending.1 += dy as f32 * LOOK_SENSITIVITY;
        }
    }

    /// How fast it flies, in metres a second.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(SPEED_RANGE.0, SPEED_RANGE.1);
    }

    /// Whether it flies faster the longer it flies without a stop.
    pub fn speeds_up(&self) -> bool {
        self.speeds_up
    }

    pub fn set_speeds_up(&mut self, speeds_up: bool) {
        self.speeds_up = speeds_up;
    }

    /// How many times its speed it flies at now: 1 from a stop, more the
    /// longer it has flown.
    pub fn speed_up(&self) -> f32 {
        if self.speeds_up { SPEED_UP.powf(self.flown).min(SPEED_UP_LIMIT) } else { 1.0 }
    }

    /// Flies on for `delta` seconds, or stops: the seconds it flew at its own
    /// speed that cover what it flew, the same however many frames they came
    /// in.
    fn fly_on(&mut self, moving: bool, delta: f32) -> f32 {
        if !moving {
            self.flown = 0.0;
            return 0.0;
        }
        let (from, to) = (self.flown, self.flown + delta.max(0.0));
        self.flown = to;
        if !self.speeds_up {
            return to - from;
        }
        // The speed-up summed over the time: as it grows up to its limit, and level after.
        let rate = SPEED_UP.ln();
        let level = SPEED_UP_LIMIT.ln() / rate;
        let grown = |seconds: f32| SPEED_UP.powf(seconds.min(level)) / rate;
        grown(to) - grown(from) + (to.max(level) - from.max(level)) * SPEED_UP_LIMIT
    }

    /// Wheel notches up speed it up; down slows it.
    pub fn scroll(&mut self, notches: f32) {
        if self.looking {
            self.speed = (self.speed * SPEED_STEP.powf(notches)).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
        }
    }

    /// Moves the first camera in the world. Call before the engine ticks.
    pub fn update(&mut self, world: &mut World, delta: f32) {
        if !self.looking {
            return;
        }
        let Some((_, transform)) = world.query::<(&Camera, &mut Transform)>().into_iter().next() else { return };
        self.update_transform(transform, delta);
    }

    /// Moves one transform, such as the editor's own Scene camera.
    pub fn update_transform(&mut self, transform: &mut Transform, delta: f32) {
        if !self.looking {
            return;
        }
        if !self.synced {
            let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
            (self.yaw, self.pitch) = (yaw, pitch);
            self.synced = true;
        }
        self.yaw += self.pending.0;
        self.pitch = (self.pitch + self.pending.1).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.pending = (0.0, 0.0);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);

        let axis = |plus: KeyCode, minus: KeyCode| {
            f32::from(u8::from(self.held.contains(&plus))) - f32::from(u8::from(self.held.contains(&minus)))
        };
        let local = Vec3::new(axis(KeyCode::KeyD, KeyCode::KeyA), 0.0, axis(KeyCode::KeyW, KeyCode::KeyS));
        let direction = transform.rotation * local + Vec3::Y * axis(KeyCode::KeyE, KeyCode::KeyQ);
        let fast = self.held.contains(&KeyCode::ShiftLeft) || self.held.contains(&KeyCode::ShiftRight);
        let speed = self.speed * if fast { FAST_MULTIPLIER } else { 1.0 };
        let direction = direction.normalize_or_zero();
        let seconds = self.fly_on(direction != Vec3::ZERO, delta);
        transform.position += direction * speed * seconds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(Vec3::ZERO)));
        world
    }

    fn camera(world: &World) -> Transform {
        *world.query_ref::<(&Camera, &Transform)>().into_iter().next().unwrap().1
    }

    #[test]
    fn moves_only_while_looking() {
        let mut world = world();
        let mut fly = FlyCamera::default();
        fly.key(KeyCode::KeyW, true);
        fly.update(&mut world, 1.0);
        assert_eq!(camera(&world).position, Vec3::ZERO);

        fly.set_looking(true);
        fly.set_speeds_up(false);
        fly.key(KeyCode::KeyW, true);
        fly.update(&mut world, 1.0);
        assert!((camera(&world).position - Vec3::new(0.0, 0.0, 6.0)).length() < 1e-4, "W moves along +Z");
    }

    #[test]
    fn flies_faster_the_longer_it_flies_and_from_its_own_speed_again_after_a_stop() {
        let flown = |frames: u32, seconds: f32| {
            let mut world = world();
            let mut fly = FlyCamera::default();
            fly.set_looking(true);
            fly.key(KeyCode::KeyW, true);
            for _ in 0..frames {
                fly.update(&mut world, seconds / frames as f32);
            }
            (camera(&world).position.z, fly)
        };
        // A nudge is at its own speed, near enough; a second in, 1.8 times; and it levels off at 32 times.
        let (nudge, _) = flown(1, 0.1);
        assert!((0.6..0.63).contains(&nudge), "{nudge}");
        let (second, fly) = flown(60, 1.0);
        assert!((fly.speed_up() - 1.8).abs() < 1e-3, "{}", fly.speed_up());
        assert!((second - 6.0 * 0.8 / 1.8_f32.ln()).abs() < 1e-2, "{second}");
        let (long, fly) = flown(600, 10.0);
        assert_eq!(fly.speed_up(), 32.0);
        let (longer, _) = flown(660, 11.0);
        assert!((longer - long - 6.0 * 32.0).abs() < 0.5, "a second at the limit: {long} to {longer}");
        // The same ground however many frames it came in.
        let (few, _) = flown(7, 3.0);
        let (many, _) = flown(700, 3.0);
        assert!((few - many).abs() < 1e-2, "{few} and {many}");

        // Letting go of every key is a stop: it starts from its own speed again.
        let (_, mut fly) = flown(60, 3.0);
        let mut world = world();
        fly.key(KeyCode::KeyW, false);
        fly.update(&mut world, 0.016);
        assert_eq!(fly.speed_up(), 1.0);
        // And so is letting go of the mouse button.
        let (_, mut fly) = flown(60, 3.0);
        fly.set_looking(false);
        assert_eq!(fly.speed_up(), 1.0);
    }

    #[test]
    fn mouse_right_turns_right_and_down_looks_down() {
        let mut world = world();
        let mut fly = FlyCamera::default();
        fly.set_looking(true);
        fly.look(200.0, 100.0);
        fly.update(&mut world, 0.0);
        let forward = camera(&world).rotation * Vec3::Z;
        assert!(forward.x > 0.1, "turned right: {forward}");
        assert!(forward.y < -0.1, "looking down: {forward}");
    }
}
