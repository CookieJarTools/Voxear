//! Hands closing on what they hold, with no clips: each finger curls
//! toward the palm by its share, the thumb across it. A grip is five
//! curls, thumb first; one grip fits any gun's grip or handguard, as each
//! finger only bends further for something smaller.

use voxear_core::math::{Quat, Vec3};

use crate::animation::ik::{PoseEdit, hand_axes};
use crate::animation::rig::Arm;

/// How far a finger's joints bend at a full curl, from its root out, in
/// degrees, and the thumb's.
const FINGER_BEND: [f32; 3] = [75.0, 95.0, 60.0];
const THUMB_BEND: [f32; 3] = [25.0, 35.0, 40.0];

/// A hand's grip: how far each finger curls, from 0 straight to 1 a fist,
/// thumb first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip(pub [f32; 5]);

impl Grip {
    /// Round a pistol grip, the index finger along the trigger.
    pub const TRIGGER: Grip = Grip([0.55, 0.35, 0.85, 0.88, 0.9]);
    /// Round a handguard or fore-end from below.
    pub const SUPPORT: Grip = Grip([0.45, 0.7, 0.75, 0.78, 0.8]);
    /// Pinching something small, as a bolt's knob or a round.
    pub const PINCH: Grip = Grip([0.6, 0.6, 0.85, 0.9, 0.9]);
    /// Open, as when reaching.
    pub const OPEN: Grip = Grip([0.1, 0.1, 0.12, 0.14, 0.16]);

    /// `t` of the way to `other`.
    pub fn lerp(self, other: Grip, t: f32) -> Grip {
        Grip(std::array::from_fn(|i| self.0[i] + (other.0[i] - self.0[i]) * t))
    }
}

/// Curls an arm's fingers by a grip, toward its palm.
pub fn close(edit: &mut PoseEdit, arm: &Arm, grip: Grip) {
    let Some((_, palm)) = hand_axes(edit, arm) else { return };
    for (finger, bones) in arm.fingers.iter().enumerate() {
        let bends = if finger == 0 { THUMB_BEND } else { FINGER_BEND };
        for (joint, &bone) in bones.iter().enumerate().take(3) {
            let from = edit.position(bone);
            // Along the finger: to the next joint, or on from the last for its tip.
            let along = match (bones.get(joint + 1), joint.checked_sub(1).map(|before| bones[before])) {
                (Some(&next), _) => edit.position(next) - from,
                (None, Some(before)) => from - edit.position(before),
                (None, None) => edit.rotation(bone) * Vec3::Z,
            };
            let axis = along.cross(palm).try_normalize();
            if let Some(axis) = axis {
                edit.turn(
                    bone,
                    Quat::from_axis_angle(axis, (bends[joint] * grip.0[finger].clamp(0.0, 1.0)).to_radians()),
                );
            }
        }
    }
}
