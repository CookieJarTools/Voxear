//! Animators: an Obj drawing a model made to animate plays a clip, from any
//! model file whose skeleton names its bones as the model's does (see
//! [`clips`] and [`retarget`]), posing the model's skinned parts
//! (`voxear_render::Pose`).
//! A change of clip never pops: the pose the Obj had and how it was moving
//! are kept as an offset from the new clip's, which dies away over the
//! blend time, as a spring settling (inertialization). With root motion,
//! the clip's root bone's travel along the ground moves the Obj itself.

mod bone_attachment;
mod clips;
mod motion_database;
mod motion_matching;
mod motion_set;
mod retarget;
mod settling;
mod states;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, Clip, ModelAnimation, ModelRenderer, Pose};
use voxear_scene::{Destroying, Transform};

pub use bone_attachment::{BoneAttachment, attach_to_bones, register as register_bone_attachments};
pub use clips::{CLIP_PART, ClipSource, Clips, give_clips};
pub use motion_database::MotionDatabase;
pub use motion_matching::{MotionMatcher, MotionMatcherState, StateChoice, match_motion, register};
pub use states::StatePlay;
pub use motion_set::{ActionMark, ActionMovement, BodyPart, MotionAction, MotionSet, MotionState, StateClip, StateRole};
use settling::Settling;

/// Plays a clip on its Obj's model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Animator {
    /// The clip it plays, `Models/Walks.glb#clip:Walk_Loop`, or one of the
    /// model's own by its name alone; none holds the model at rest.
    pub clip: String,
    /// How fast it plays: 1 as made, 2 twice as fast.
    pub speed: f32,
    /// Going round from the end to the start, or holding the last frame.
    pub looping: bool,
    /// How long, in seconds, a change of clip takes to settle into the new.
    pub blend: f32,
    /// Moving the Obj along the ground as the clip's root bone travels,
    /// rather than the model away from the Obj.
    pub root_motion: bool,
}

impl Component for Animator {}

impl Default for Animator {
    fn default() -> Self {
        Self { clip: String::new(), speed: 1.0, looping: true, blend: 0.2, root_motion: false }
    }
}

/// What an Animator is doing, kept by the engine: the clip playing and how
/// far in, the last two poses it showed, and the offset a change of clip is
/// still settling.
#[derive(Clone, Debug, Default)]
pub struct AnimatorState {
    /// The clip playing, as its reference names it.
    pub playing: Option<String>,
    pub time: f32,
    shown: Vec<BonePose>,
    before: Vec<BonePose>,
    settling: Option<Settling>,
}

impl Component for AnimatorState {}

/// Plays every Animator `delta` seconds on, and poses its model, but for
/// Objs a Motion Matcher animates.
pub fn animate(world: &mut World, delta: f32) {
    let animators: Vec<(Entity, Animator, Arc<ModelAnimation>)> = world
        .query_ref::<(Entity, &Animator, &ModelRenderer)>()
        .without::<Destroying>()
        .without::<MotionMatcher>()
        .into_iter()
        .filter_map(|(entity, animator, model)| Some((entity, animator.clone(), model.animation.clone()?)))
        .collect();
    for (entity, animator, animation) in animators {
        let mut state = world.get::<AnimatorState>(entity).cloned().unwrap_or_default();
        let clip = clip_for(world, &animator.clip, &animation);
        let wanted = clip.as_ref().map(|clip| (animator.clip.as_str(), clip.as_ref()));
        let travelled = step(&mut state, &animator, &animation, wanted, delta);
        if animator.root_motion && travelled != (Vec3::ZERO, 0.0) {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                let (moved, turned) = travelled;
                transform.position += transform.rotation * moved;
                transform.rotation = (transform.rotation * Quat::from_rotation_y(turned)).normalize();
            }
        }
        let bones = animation.skeleton.model_matrices(&state.shown).into();
        world.insert(entity, Pose { bones });
        world.insert(entity, state);
    }
}

/// The clip a reference names, made to fit `model`'s skeleton: a clip
/// asset the game resolved, or one of the model's own by its name alone.
fn clip_for(world: &mut World, reference: &str, model: &Arc<ModelAnimation>) -> Option<Arc<Clip>> {
    if reference.is_empty() {
        return None;
    }
    if let Some(source) = world.resource::<Clips>().and_then(|clips| clips.get(reference)).cloned() {
        return Some(retarget::fitted(world, &source, model));
    }
    let index = model.clips.iter().position(|clip| clip.name == reference)?;
    let own = ClipSource { animation: model.clone(), index, settings: Default::default() };
    Some(retarget::fitted(world, &own, model))
}

/// Moves `state` on by `delta`, to the `wanted` clip, by its reference: its
/// new pose in `state.shown`, and with root motion, how far its root
/// travelled along the ground in the model's space, and turned about Y.
fn step(
    state: &mut AnimatorState,
    animator: &Animator,
    animation: &ModelAnimation,
    wanted: Option<(&str, &Clip)>,
    delta: f32,
) -> (Vec3, f32) {
    let rest = animation.skeleton.rest();
    if state.shown.len() != rest.len() {
        state.shown = rest.clone();
        state.before = rest.clone();
    }
    // A change of clip starts the new one, settling from where the old one was.
    if wanted.map(|(reference, _)| reference) != state.playing.as_deref() {
        let (mut target, mut target_before) = (rest.clone(), rest.clone());
        if let Some((_, clip)) = wanted {
            clip.sample(0.0, animator.looping, &mut target);
            clip.sample(-delta, animator.looping, &mut target_before);
        }
        let old = [state.shown.as_slice(), &state.before];
        state.settling = (animator.blend > 0.0 && delta > 0.0)
            .then(|| Settling::between(old, [&target, &target_before], delta, animator.blend * 0.5));
        state.playing = wanted.map(|(reference, _)| reference.to_string());
        state.time = 0.0;
    }
    let root = root_bone(animation);
    let mut pose = rest.clone();
    let mut travelled = (Vec3::ZERO, 0.0);
    if let Some((_, clip)) = wanted {
        let was = state.time;
        state.time += delta * animator.speed;
        clip.sample(state.time, animator.looping, &mut pose);
        if animator.root_motion
            && let Some(root) = root
        {
            travelled = root_travel(clip, root, was, state.time, animator.looping);
            // The root kept over the Obj, as the Obj itself moves.
            pose[root].translation.x = rest[root].translation.x;
            pose[root].translation.z = rest[root].translation.z;
            pose[root].rotation = rest[root].rotation;
        }
    }
    if let Some(settling) = &mut state.settling
        && !settling.settle(delta, &mut pose)
    {
        state.settling = None;
    }
    state.before = std::mem::replace(&mut state.shown, pose);
    travelled
}

/// The bone root motion is taken from: the one named "root", or else the
/// first.
fn root_bone(animation: &ModelAnimation) -> Option<usize> {
    let bones = &animation.skeleton.bones;
    bones.iter().position(|bone| bone.name.eq_ignore_ascii_case("root")).or((!bones.is_empty()).then_some(0))
}

/// How far a clip's root bone travels along the ground, and turns about Y,
/// from `from` to `to` seconds in, going round its loop when it does.
fn root_travel(clip: &voxear_render::Clip, root: usize, from: f32, to: f32, looping: bool) -> (Vec3, f32) {
    let at = |time: f32| {
        let frames = clip.duration() * voxear_render::CLIP_RATE;
        let frame = (time.clamp(0.0, clip.duration()) * voxear_render::CLIP_RATE).round().clamp(0.0, frames) as usize;
        let pose = clip.frame(frame)[root];
        let (yaw, _, _) = pose.rotation.to_euler(voxear_core::math::EulerRot::YXZ);
        (Vec3::new(pose.translation.x, 0.0, pose.translation.z), yaw)
    };
    let duration = clip.duration();
    if !looping || duration <= 0.0 || (from / duration).floor() == (to / duration).floor() {
        let wrap = |t: f32| if looping && duration > 0.0 { t.rem_euclid(duration) } else { t };
        let (a, b) = (at(wrap(from)), at(wrap(to)));
        return (b.0 - a.0, b.1 - a.1);
    }
    // Round the end of the loop: to its end, then from its start.
    let (start, end) = (at(0.0), at(duration));
    let (a, b) = (at(from.rem_euclid(duration)), at(to.rem_euclid(duration)));
    ((end.0 - a.0) + (b.0 - start.0), (end.1 - a.1) + (b.1 - start.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::{Bone, Clip, Skeleton};

    /// One bone and two clips: standing still at x = 0, and stepping along
    /// Z a metre a second.
    fn walker() -> ModelAnimation {
        let rest = BonePose::default();
        let skeleton = Skeleton { bones: vec![Bone { name: "root".into(), parent: None, rest }] };
        let frames = 61;
        let still = Clip { name: "Still".into(), frames, poses: vec![rest; frames] };
        let poses =
            (0..frames).map(|f| BonePose { translation: Vec3::new(0.0, 0.0, f as f32 / 60.0), ..rest }).collect();
        let walk = Clip { name: "Walk".into(), frames, poses };
        ModelAnimation { skeleton, skins: Vec::new(), clips: vec![still, walk] }
    }

    #[test]
    fn a_change_of_clip_settles_without_a_jump() {
        let animation = walker();
        let mut state = AnimatorState::default();
        let mut animator = Animator { clip: "Walk".into(), ..Animator::default() };
        let dt = 1.0 / 60.0;
        for _ in 0..30 {
            step(&mut state, &animator, &animation, Some(("Walk", &animation.clips[1])), dt);
        }
        let before = state.shown[0].translation.z;
        assert!((before - 0.5).abs() < 0.02, "half way along: {before}");
        animator.clip = "Still".into();
        step(&mut state, &animator, &animation, Some(("Still", &animation.clips[0])), dt);
        let after = state.shown[0].translation.z;
        assert!((after - before).abs() < 0.03, "no jump back to the start: {before} then {after}");
        for _ in 0..60 {
            step(&mut state, &animator, &animation, Some(("Still", &animation.clips[0])), dt);
        }
        assert!(state.shown[0].translation.z.abs() < 0.01, "settled into the new clip");
    }

    #[test]
    fn root_motion_moves_the_obj_and_keeps_the_root_over_it() {
        let animation = walker();
        let mut state = AnimatorState::default();
        let animator = Animator { clip: "Walk".into(), root_motion: true, blend: 0.0, ..Animator::default() };
        let mut travelled = Vec3::ZERO;
        for _ in 0..90 {
            travelled += step(&mut state, &animator, &animation, Some(("Walk", &animation.clips[1])), 1.0 / 60.0).0;
        }
        assert!((travelled.z - 1.5).abs() < 0.03, "a metre and a half, round the loop: {travelled}");
        assert!(state.shown[0].translation.z.abs() < 1e-5, "the root stays over the Obj");
    }
}
