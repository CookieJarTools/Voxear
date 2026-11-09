//! A base clip under a gun action: a clip from the project, made to fit
//! the character holding the gun, played over the action from its start
//! to its end. What it gives goes on top of what the action does, as the
//! clip moves from where it begins: the body's lean and turn, where the
//! gun goes as the clip's gun hand carries it, and which way the elbows
//! bend. The palms still hold the gun where the action has them, the
//! fingers close round it and the hands stay out of it, so a clip made
//! for any rifle, or for none, holds this one.

use serde::{Deserialize, Serialize};
use voxear_core::ease::smooth;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{ModelRenderer, Pose};
use voxear_scene::world_matrix;

use super::playing::ActionPlay;
use crate::animation::Clips;
use crate::animation::ik::PoseEdit;
use crate::animation::rig::Rig;

/// The clip an action is based on, the part of it the action plays, and
/// how much of it the character follows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BaseClip {
    /// The clip, as `Animations/Rifle.fbx#clip:Reload` names it.
    pub clip: String,
    /// Where in the clip the action starts and ends, in seconds; an end at
    /// or before the start is the clip's own end.
    pub start: f32,
    pub end: f32,
    /// How much it moves the body, carries the gun and bends the elbows:
    /// 0 not at all, 1 as the clip does.
    pub body: f32,
    pub gun: f32,
    pub arms: f32,
    /// How long it takes to come in as the action starts, and to go as it
    /// ends, in seconds.
    pub blend: f32,
}

impl Default for BaseClip {
    fn default() -> Self {
        Self { clip: String::new(), start: 0.0, end: 0.0, body: 1.0, gun: 1.0, arms: 1.0, blend: 0.2 }
    }
}

/// What a base clip gives the gun and the arms once it has moved the
/// body: how the gun is carried, about the eyes, and which way each elbow
/// bends from its shoulder, left then right, in the body's model space,
/// each with how much of it is followed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Given {
    pub gun: Option<(Mat4, f32)>,
    pub elbows: Option<([Vec3; 2], f32)>,
}

/// Plays the base clip of `play` on `body` at how far the action has
/// gone: turns its spine, neck, head and collarbones from how they are
/// posed by as much as the clip has turned them since it began, and gives
/// back what it does to the gun, held before `eye`, and to the elbows.
/// Nothing without a base clip, or before its clip has been read.
pub(super) fn play(world: &mut World, body: Entity, play: &ActionPlay, eye: Mat4) -> Option<Given> {
    let base = play.clip.as_ref().filter(|base| !base.clip.is_empty())?;
    let animation = world.get::<ModelRenderer>(body)?.animation.clone()?;
    let rig = Rig::find(&animation.skeleton)?;
    let source = world.resource::<Clips>()?.get(&base.clip)?.clone();
    let clip = crate::animation::retarget::fitted(world, &source, &animation);
    let skeleton = &animation.skeleton;

    // The part of the clip the action plays, stretched over it, and how far in it has come in or gone.
    let (elapsed, length) = play.progress();
    let end = if base.end > base.start { base.end.min(clip.duration()) } else { clip.duration() };
    let start = base.start.clamp(0.0, end);
    let share = if length > 0.0 { (elapsed / length).clamp(0.0, 1.0) } else { 0.0 };
    let (mut now, mut first) = (skeleton.rest(), skeleton.rest());
    clip.sample(start + (end - start) * share, false, &mut now);
    clip.sample(start, false, &mut first);
    let blend = base.blend.max(1e-3);
    let fade = smooth((elapsed / blend).min((length - elapsed) / blend));

    let body_weight = base.body.clamp(0.0, 1.0) * fade;
    if body_weight > 0.0 {
        let pose = world.get::<Pose>(body)?;
        let mut edit = PoseEdit::new(skeleton, &pose.bones);
        let clavicles = rig.arms.iter().filter_map(|arm| arm.clavicle);
        for bone in rig.spine.iter().copied().chain(rig.neck).chain([rig.head]).chain(clavicles) {
            let turned = now[bone].rotation * first[bone].rotation.inverse();
            let turn = Quat::IDENTITY.slerp(turned, body_weight);
            edit.local[bone].rotation = (turn * edit.local[bone].rotation).normalize();
        }
        world.insert(body, Pose { bones: edit.matrices().into() });
    }

    // The clip's gun hand and elbows from its chest, carried by the chest as it is now posed.
    let (now, first) = (skeleton.model_matrices(&now), skeleton.model_matrices(&first));
    let chest = rig.chest();
    let from_chest = |matrices: &[Mat4], bone: usize| matrices[chest].inverse() * matrices[bone];
    let posed = world.get::<Pose>(body)?.bones.get(chest).copied()?;
    let gun_weight = base.gun.clamp(0.0, 1.0) * fade;
    let gun = (gun_weight > 0.0).then(|| {
        let hand = rig.arms[1].hand;
        let moved = from_chest(&now, hand) * from_chest(&first, hand).inverse();
        let chest = world_matrix(world, body) * posed;
        (eye.inverse() * chest * moved * chest.inverse() * eye, gun_weight)
    });
    let arms_weight = base.arms.clamp(0.0, 1.0) * fade;
    let elbows = (arms_weight > 0.0).then(|| {
        let way = |arm: &crate::animation::rig::Arm| {
            let elbow = from_chest(&now, arm.forearm).w_axis - from_chest(&now, arm.upper).w_axis;
            posed.transform_vector3(elbow.truncate()).normalize_or_zero()
        };
        (rig.arms.each_ref().map(way), arms_weight)
    });
    Some(Given { gun, elbows })
}
