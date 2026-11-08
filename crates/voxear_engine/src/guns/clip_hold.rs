//! A gun held by clips, for a gun from an animation pack: its arms clips
//! say where the gun goes and how the arms hold it. A clip is read on its
//! own skeleton, for the gun's place before the eyes, as the bone carrying
//! it is from the clip's head, and each hand's palm, knuckles and the ways
//! its fingers point and its palm faces, on the gun. It is also played on
//! the holder's own body, as any clip is made to fit another skeleton, for
//! its arms: each bone from the upper arm out to the fingertips turns as
//! the clip turns it, twist and all. The holder's hands are then brought
//! the rest of the way onto the gun, only as far as a body built otherwise
//! needs, and as aiming, sway and recoil move the gun.
//!
//! The clips hold the gun where they have it before the eyes, and nothing
//! moves it from there but the layers the holder adds on top: the camera
//! stays where the clips' camera was from the body's shoulders, and the gun
//! as far before it, both scaled to the body's arms, so they reach the gun
//! bent as the clip's do.

use std::sync::Arc;

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{ModelAnimation, Skeleton};

use super::handling::{PackClips, Place};
use super::hands::{arm_reach, knuckles, palm_of};
use super::playing::ActionPlay;
use crate::animation::{ClipSource, Clips};
use crate::animation::ik::{PoseEdit, hand_axes};
use crate::animation::rig::{Arm, Rig};

/// Where a clip holds the gun and has the hands, at one moment.
#[derive(Clone, Debug, PartialEq)]
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
    /// The left arm, then the right, as the clip poses them on the holder's
    /// body; none until the clip is made to fit it (see `fit_clips`).
    pub arms: Option<[ArmPose; 2]>,
}

/// An arm as a clip poses it on the holder's body: its upper arm's turn in
/// the eyes' space, and the turn of each bone under it, as the body's
/// skeleton orders them, in its parent's.
#[derive(Clone, Debug, PartialEq)]
pub struct ArmPose {
    pub upper: Quat,
    pub under: Vec<Quat>,
}

/// A hand as a clip has it: the middle of its palm and of its knuckles,
/// the way its fingers point and its palm faces, in the gun's space; and
/// how far its arm reaches, from the shoulder to the middle of the palm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipHand {
    pub contact: Vec3,
    pub knuckles: Vec3,
    pub fingers: Vec3,
    pub palm: Vec3,
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
        let arms = match (&self.arms, &other.arms) {
            (Some(arms), Some(others)) => Some(std::array::from_fn(|arm| arms[arm].lerp(&others[arm], t))),
            (arms, others) => arms.clone().or_else(|| others.clone()),
        };
        ClipPose { gun, hands, shoulders, stance, arms }
    }
}

impl ArmPose {
    fn lerp(&self, other: &ArmPose, t: f32) -> ArmPose {
        let under = self.under.iter().zip(&other.under).map(|(a, b)| a.slerp(*b, t)).collect();
        ArmPose { upper: self.upper.slerp(other.upper, t), under }
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
            reach: self.reach + (other.reach - self.reach) * t,
        }
    }
}

/// Where the clip `reference` holds the gun and has the hands `at` seconds
/// in, looping or not: the gun carried by the bone `carrier` of its
/// skeleton, or by one named `gun_carrier`, or by the right hand, and sitting
/// `offset` from it; its eyes `eyes` from its head as the clip starts, in
/// its space, which go where the holder's eyes are; and with the holder's
/// `body`, its arms as it poses them there. None before the clip is read, or for a skeleton
/// the rig finds no arms in.
pub fn clip_pose(
    world: &World,
    reference: &str,
    at: f32,
    looping: bool,
    (carrier, offset): (&str, &Place),
    eyes: Vec3,
    body: Option<&Arc<ModelAnimation>>,
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
    // The eyes stay where the clip's head starts, as the holder's camera stays put on the body: a clip whose head
    // leans back as the bolt is worked moves the gun against the body as it has it, not against the leaning head.
    let mut first = skeleton.rest();
    source.clip().sample(0.0, false, &mut first);
    let head = skeleton.model_matrices(&first)[rig.head].w_axis.truncate();
    let eye = Mat4::from_translation(head + eyes);
    let into_gun = carried.inverse();
    let at_rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
    let hand = |arm: usize| -> Option<ClipHand> {
        let arm = &rig.arms[arm];
        let (palm, (fingers, facing)) = (palm_of(&edit, arm)?, hand_axes(&edit, arm)?);
        Some(ClipHand {
            contact: into_gun.transform_point3(palm.point),
            knuckles: into_gun.transform_point3(knuckles(&edit, arm)?),
            fingers: into_gun.transform_vector3(fingers).normalize_or_zero(),
            palm: into_gun.transform_vector3(facing).normalize_or_zero(),
            reach: arm_reach(&at_rest, arm)?,
        })
    };
    let into_eye = eye.inverse();
    let shoulders = [0, 1].map(|arm| into_eye.transform_point3(edit.position(rig.arms[arm].upper)));
    let turned = |bone: Option<usize>| {
        bone.map_or(Quat::IDENTITY, |bone| (edit.rotation(bone) * at_rest.rotation(bone).inverse()).normalize())
    };
    let stance = [turned(Some(rig.chest())), turned(rig.arms[0].clavicle), turned(rig.arms[1].clavicle)];
    let arms = body.and_then(|body| arms_on(world, source, at, looping, body));
    Some(ClipPose { gun: into_eye * carried, hands: [hand(0)?, hand(1)?], shoulders, stance, arms })
}

/// The arms of the clip `source` as it poses them on `body` `at` seconds
/// in (see `ArmPose`): none until `fit_clips` has made it fit the body.
/// The fitted clip turns each bone in the model's space as the clip turns
/// its own, so the upper arm's turn there is its turn in the clip's eyes'.
fn arms_on(
    world: &World,
    source: &ClipSource,
    at: f32,
    looping: bool,
    body: &Arc<ModelAnimation>,
) -> Option<[ArmPose; 2]> {
    let clip = crate::animation::retarget::fitted_already(world, source, body)?;
    let skeleton = &body.skeleton;
    let rig = Rig::find(skeleton)?;
    let mut pose = skeleton.rest();
    clip.sample(at, looping, &mut pose);
    let posed = skeleton.model_matrices(&pose);
    let arm = |arm: &Arm| ArmPose {
        upper: posed[arm.upper].to_scale_rotation_translation().1,
        under: under(skeleton, arm.upper).into_iter().map(|bone| pose[bone].rotation).collect(),
    };
    Some([arm(&rig.arms[0]), arm(&rig.arms[1])])
}

/// Makes every arms clip that may hold the gun now fit the body `body`, so
/// `clip_pose` can pose its arms there: the holding clips, and the arms
/// clip of the part of `play` under way.
pub fn fit_clips(world: &mut World, pack: &PackClips, play: Option<&ActionPlay>, body: &Arc<ModelAnimation>) {
    let part = play.and_then(|play| play.part()).map(|(part, _)| part.arms.clone());
    let references = [&pack.idle, &pack.walk, &pack.run, &pack.sprint].into_iter().cloned().chain(part);
    for reference in references.filter(|reference| !reference.is_empty()) {
        if let Some(source) = world.resource::<Clips>().and_then(|clips| clips.get(&reference)).cloned() {
            crate::animation::retarget::fitted(world, &source, body);
        }
    }
}

/// Poses `arm` of a body as a clip poses it (see `ArmPose`): its upper arm
/// turned in the model's space by `eyes`, the eyes' turn there, and every
/// bone under it as the clip turns it in its parent's; each `weight` of
/// the way from how it stands.
pub fn pose_arm(edit: &mut PoseEdit, skeleton: &Skeleton, arm: &Arm, pose: &ArmPose, eyes: Quat, weight: f32) {
    let now = edit.rotation(arm.upper);
    edit.set_rotation(arm.upper, now.slerp(eyes * pose.upper, weight));
    for (bone, turn) in under(skeleton, arm.upper).into_iter().zip(&pose.under) {
        let local = &mut edit.local[bone].rotation;
        *local = local.slerp(*turn, weight).normalize();
    }
}

/// Every bone under `bone`, in the skeleton's order, parents before their
/// children.
fn under(skeleton: &Skeleton, bone: usize) -> Vec<usize> {
    let mut within = vec![false; skeleton.bones.len()];
    let mut found = Vec::new();
    for (index, named) in skeleton.bones.iter().enumerate() {
        if named.parent.is_some_and(|parent| parent == bone || within[parent]) {
            within[index] = true;
            found.push(index);
        }
    }
    found
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

/// The holding clips blended by `weights`, each looped at `time`, with
/// their arms as they pose them on `body`.
pub fn holding_pose(
    world: &World,
    pack: &PackClips,
    weights: [f32; 4],
    time: f32,
    body: Option<&Arc<ModelAnimation>>,
) -> Option<ClipPose> {
    let eyes = Vec3::from(pack.eyes);
    let clips = [&pack.idle, &pack.walk, &pack.run, &pack.sprint];
    let mut blended: Option<(ClipPose, f32)> = None;
    for (clip, weight) in clips.into_iter().zip(weights).filter(|(clip, weight)| !clip.is_empty() && *weight > 1e-3) {
        let length = super::gun_clips::length(world, clip).max(1e-3);
        let carrier = (pack.carrier.as_str(), &pack.carrier_offset);
        let Some(pose) = clip_pose(world, clip, time % length, true, carrier, eyes, body) else {
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
    let (first, now) = (
        clip_pose(world, clip, 0.0, false, carrier, eyes, None)?,
        clip_pose(world, clip, since, false, carrier, eyes, None)?,
    );
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
