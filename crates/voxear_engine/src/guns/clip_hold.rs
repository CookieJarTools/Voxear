//! A gun held by clips, for a gun from an animation pack: its arms clips
//! say where the gun and the hands go, and the holder's own body reaches
//! there. A clip is read on its own skeleton, so it holds the gun alike on
//! any body with arms, hands and fingers the rig finds. From each frame it
//! gives the gun's place before the eyes, as the bone carrying it is from
//! the clip's head; each hand's palm and the ways its fingers point and
//! its palm faces, on the gun; the way each elbow bends; and how far each
//! finger's joints bend. The holder's arms then reach as they always do,
//! so aiming, sway and recoil still move the gun, and the hands with it.
//!
//! The clips hold the gun where they have it before the eyes, and nothing
//! moves it from there but the layers the holder adds on top: the camera
//! stays where the clips' camera was from the body's shoulders, so the
//! body's arms reach the gun as the clip's do.

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::Skeleton;

use super::handling::{PackClips, Place};
use super::hands::{arm_reach, finger_bends, finger_ways, knuckles, palm_of};
use crate::animation::Clips;
use crate::animation::ik::{PoseEdit, hand_axes};
use crate::animation::rig::Rig;

/// Where a clip holds the gun and has the hands, at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPose {
    /// The gun's place in the eyes' space.
    pub gun: Mat4,
    /// The left hand, then the right.
    pub hands: [ClipHand; 2],
    /// Each shoulder, left then right, in the eyes' space.
    pub shoulders: [Vec3; 2],
    /// How the clip's chest, then its left and right collarbones, are turned
    /// from how they stand at rest, in its model's space: the stance its arms
    /// hold the gun from.
    pub stance: [Quat; 3],
}

/// A hand as a clip has it: the middle of its palm and of its knuckles, the way its fingers
/// point and its palm faces, in the gun's space; the way its elbow is from
/// its shoulder, in the eyes' space; and how far each finger's joints bend
/// toward the palm, thumb first, in radians (see `finger_bends`), and the
/// way each points in the hand's own frame (see `finger_ways`); and how far
/// its arm reaches, from the shoulder to the middle of the palm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipHand {
    pub contact: Vec3,
    pub knuckles: Vec3,
    pub fingers: Vec3,
    pub palm: Vec3,
    pub elbow: Vec3,
    pub bends: [[f32; 3]; 5],
    pub ways: [[Vec3; 3]; 5],
    pub reach: f32,
}

impl ClipPose {
    /// `t` of the way to `other`.
    pub fn lerp(&self, other: &ClipPose, t: f32) -> ClipPose {
        let (_, turn, at) = self.gun.to_scale_rotation_translation();
        let (_, other_turn, other_at) = other.gun.to_scale_rotation_translation();
        let gun = Mat4::from_rotation_translation(turn.slerp(other_turn, t), at.lerp(other_at, t));
        let hands = std::array::from_fn(|hand| self.hands[hand].lerp(&other.hands[hand], t));
        let shoulders = std::array::from_fn(|arm| self.shoulders[arm].lerp(other.shoulders[arm], t));
        let stance = std::array::from_fn(|k| self.stance[k].slerp(other.stance[k], t));
        ClipPose { gun, hands, shoulders, stance }
    }
}

impl ClipHand {
    fn lerp(&self, other: &ClipHand, t: f32) -> ClipHand {
        let way = |a: Vec3, b: Vec3| a.lerp(b, t).normalize_or(a);
        ClipHand {
            contact: self.contact.lerp(other.contact, t),
            knuckles: self.knuckles.lerp(other.knuckles, t),
            fingers: way(self.fingers, other.fingers),
            palm: way(self.palm, other.palm),
            elbow: way(self.elbow, other.elbow),
            bends: std::array::from_fn(|f| {
                std::array::from_fn(|j| self.bends[f][j] * (1.0 - t) + other.bends[f][j] * t)
            }),
            ways: std::array::from_fn(|f| std::array::from_fn(|j| way(self.ways[f][j], other.ways[f][j]))),
            reach: self.reach + (other.reach - self.reach) * t,
        }
    }
}

/// Where the clip `reference` holds the gun and has the hands `at` seconds
/// in, looping or not: the gun carried by the bone `carrier` of its
/// skeleton, or by one named `gun_carrier`, or by the right hand, and sitting
/// `offset` from it; its eyes `eyes` from its head, in its space, which go
/// where the holder's eyes are. None
/// before the clip is read, or for a skeleton the rig finds no arms in.
pub fn clip_pose(
    world: &World,
    reference: &str,
    at: f32,
    looping: bool,
    (carrier, offset): (&str, &Place),
    eyes: Vec3,
) -> Option<ClipPose> {
    let source = world.resource::<Clips>()?.get(reference)?;
    let skeleton = &source.animation.skeleton;
    let rig = Rig::find(skeleton)?;
    let mut pose = skeleton.rest();
    source.clip().sample(at, looping, &mut pose);
    let posed = skeleton.model_matrices(&pose);
    let edit = PoseEdit::new(skeleton, &posed);
    let carried = posed[carried_by(skeleton, &rig, carrier)]
        * Mat4::from_rotation_translation(offset.rotation(), Vec3::from(offset.at));
    let eye = Mat4::from_translation(posed[rig.head].w_axis.truncate() + eyes);
    let into_gun = carried.inverse();
    let at_rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
    let hand = |arm: usize| -> Option<ClipHand> {
        let arm = &rig.arms[arm];
        let (palm, (fingers, facing)) = (palm_of(&edit, arm)?, hand_axes(&edit, arm)?);
        let elbow = (edit.position(arm.forearm) - edit.position(arm.upper)).normalize_or_zero();
        Some(ClipHand {
            contact: into_gun.transform_point3(palm.point),
            knuckles: into_gun.transform_point3(knuckles(&edit, arm)?),
            fingers: into_gun.transform_vector3(fingers).normalize_or_zero(),
            palm: into_gun.transform_vector3(facing).normalize_or_zero(),
            elbow,
            bends: finger_bends(&edit, arm),
            ways: finger_ways(&edit, &at_rest, arm),
            reach: arm_reach(&at_rest, arm)?,
        })
    };
    let into_eye = eye.inverse();
    let shoulders = [0, 1].map(|arm| into_eye.transform_point3(edit.position(rig.arms[arm].upper)));
    let turned = |bone: Option<usize>| {
        bone.map_or(Quat::IDENTITY, |bone| (edit.rotation(bone) * at_rest.rotation(bone).inverse()).normalize())
    };
    let stance = [turned(Some(rig.chest())), turned(rig.arms[0].clavicle), turned(rig.arms[1].clavicle)];
    Some(ClipPose { gun: into_eye * carried, hands: [hand(0)?, hand(1)?], shoulders, stance })
}

/// The bone that carries the gun: the one named, else one named
/// `gun_carrier`, else the right hand.
fn carried_by(skeleton: &Skeleton, rig: &Rig, named: &str) -> usize {
    let named = (!named.is_empty()).then(|| skeleton.find(named)).flatten();
    named.or_else(|| skeleton.find("gun_carrier")).unwrap_or(rig.arms[1].hand)
}

/// How much each holding clip holds the gun now, idle, walking, running
/// and sprinting, as a holder going `speed` metres a second, sprinting or
/// not, would have them: idle to walking to running as it speeds up, the
/// nearest there is taking the share of one there is not.
pub fn holding_weights(pack: &PackClips, speed: f32, sprinting: bool) -> [f32; 4] {
    let has = [&pack.idle, &pack.walk, &pack.run, &pack.sprint].map(|clip| !clip.is_empty());
    let mut wanted = [0.0; 4];
    if sprinting && has[3] {
        wanted[3] = 1.0;
        return wanted;
    }
    let walk = (speed / pack.walk_speed.max(0.01)).clamp(0.0, 1.0);
    let run = ((speed - pack.walk_speed) / (pack.run_speed - pack.walk_speed).max(0.01)).clamp(0.0, 1.0);
    let shares = [1.0 - walk, walk * (1.0 - run), walk * run];
    for (clip, share) in shares.into_iter().enumerate() {
        // A clip there is not gives its share to the nearest one there is, slower first.
        let to = [clip, clip.saturating_sub(1), clip.saturating_sub(2), clip + 1, clip + 2]
            .into_iter()
            .find(|&other| other < 3 && has[other]);
        if let Some(to) = to {
            wanted[to] += share;
        }
    }
    wanted
}

/// The holding clips blended by `weights`, each looped at `time`.
pub fn holding_pose(world: &World, pack: &PackClips, weights: [f32; 4], time: f32) -> Option<ClipPose> {
    let eyes = Vec3::from(pack.eyes);
    let clips = [&pack.idle, &pack.walk, &pack.run, &pack.sprint];
    let mut blended: Option<(ClipPose, f32)> = None;
    for (clip, weight) in clips.into_iter().zip(weights).filter(|(clip, weight)| !clip.is_empty() && *weight > 1e-3) {
        let length = super::gun_clips::length(world, clip).max(1e-3);
        let Some(pose) = clip_pose(world, clip, time % length, true, (&pack.carrier, &pack.carrier_offset), eyes)
        else {
            continue;
        };
        blended = Some(match blended {
            None => (pose, weight),
            Some((so_far, total)) => (so_far.lerp(&pose, weight / (total + weight)), total + weight),
        });
    }
    blended.map(|(pose, _)| pose)
}

/// How a shot's clip moves the gun `since` seconds after it, from the
/// clip's first frame, in the gun's own space, so it kicks about the gun
/// wherever it is held; none once the clip is over.
pub fn fire_kick(world: &World, pack: &PackClips, since: f32, aimed: bool) -> Option<Mat4> {
    let eyes = Vec3::from(pack.eyes);
    let clip = if aimed && !pack.aimed_fire.is_empty() { &pack.aimed_fire } else { &pack.fire };
    if clip.is_empty() || since >= super::gun_clips::length(world, clip) {
        return None;
    }
    let carrier = (pack.carrier.as_str(), &pack.carrier_offset);
    let (first, now) =
        (clip_pose(world, clip, 0.0, false, carrier, eyes)?, clip_pose(world, clip, since, false, carrier, eyes)?);
    Some(first.gun.inverse() * now.gun)
}

/// Stands a body's upper body as a clip's stance has it (see `ClipPose`):
/// the spine below the chest upright as at rest, the chest and collarbones
/// turned from rest as the clip's are, whatever its own idle or walk did.
pub fn stand(edit: &mut PoseEdit, rig: &Rig, rest: &PoseEdit, stance: &[Quat; 3]) {
    let chest = rig.chest();
    for &bone in rig.spine.iter().filter(|&&bone| bone != chest) {
        edit.set_rotation(bone, rest.rotation(bone));
    }
    let turned = [Some(chest), rig.arms[0].clavicle, rig.arms[1].clavicle];
    for (bone, turn) in turned.into_iter().zip(stance) {
        if let Some(bone) = bone {
            edit.set_rotation(bone, (*turn * rest.rotation(bone)).normalize());
        }
    }
}

/// How much an action played from clips holds the gun `elapsed` seconds in:
/// coming in over `blend`, then wholly, to its very end.
pub fn comes_in(elapsed: f32, blend: f32) -> f32 {
    smooth(elapsed / blend.max(0.01))
}

/// How much an action played from clips still holds the gun `since`
/// seconds after it ended, going out over `blend`.
pub fn goes_out(since: f32, blend: f32) -> f32 {
    1.0 - smooth(since / blend.max(0.01))
}

/// `t` from 0 to 1, eased in and out.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_plays_wholly_to_its_end_and_goes_out_only_once_it_is_over() {
        assert_eq!(comes_in(0.0, 0.2), 0.0);
        assert_eq!(comes_in(0.2, 0.2), 1.0);
        for elapsed in [0.3, 1.0, 1.47, 5.0] {
            assert_eq!(comes_in(elapsed, 0.2), 1.0, "wholly at {elapsed} s, however near its end");
        }
        assert_eq!(goes_out(0.0, 0.2), 1.0);
        assert!(goes_out(0.1, 0.2) > 0.0 && goes_out(0.1, 0.2) < 1.0);
        assert_eq!(goes_out(0.2, 0.2), 0.0);
    }
}
