//! Animators: an Obj drawing a model made to animate plays one of the
//! model's clips, posing the model's skinned parts (`voxear_render::Pose`).
//! A change of clip never pops: the pose the Obj had and how it was moving
//! are kept as an offset from the new clip's, which dies away over the
//! blend time, as a spring settling (inertialization). With root motion,
//! the clip's root bone's travel along the ground moves the Obj itself.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, ModelAnimation, ModelRenderer, Pose};
use voxear_scene::{Destroying, Transform};

/// Plays one of its Obj's model's clips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Animator {
    /// The clip it plays, by its name in the model; none holds the model at
    /// rest.
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
    pub playing: Option<usize>,
    pub time: f32,
    shown: Vec<BonePose>,
    before: Vec<BonePose>,
    settling: Option<Settling>,
}

impl Component for AnimatorState {}

/// Each bone's offset from the new clip, and how fast it was changing, when
/// the clip changed, and how long ago.
#[derive(Clone, Debug)]
struct Settling {
    offsets: Vec<(Vec3, Vec3, Vec3, Vec3)>,
    since: f32,
    halflife: f32,
}

/// Plays every Animator `delta` seconds on, and poses its model.
pub fn animate(world: &mut World, delta: f32) {
    let animators: Vec<(Entity, Animator, std::sync::Arc<ModelAnimation>)> = world
        .query_ref::<(Entity, &Animator, &ModelRenderer)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, animator, model)| Some((entity, animator.clone(), model.animation.clone()?)))
        .collect();
    for (entity, animator, animation) in animators {
        let mut state = world.get::<AnimatorState>(entity).cloned().unwrap_or_default();
        let wanted = animation.clips.iter().position(|clip| clip.name == animator.clip);
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

/// Moves `state` on by `delta`, to `wanted` among the model's clips: its
/// new pose in `state.shown`, and with root motion, how far its root
/// travelled along the ground in the model's space, and turned about Y.
fn step(
    state: &mut AnimatorState,
    animator: &Animator,
    animation: &ModelAnimation,
    wanted: Option<usize>,
    delta: f32,
) -> (Vec3, f32) {
    let rest = animation.skeleton.rest();
    if state.shown.len() != rest.len() {
        state.shown = rest.clone();
        state.before = rest.clone();
    }
    // A change of clip starts the new one, settling from where the old one was.
    if wanted != state.playing {
        let mut target = rest.clone();
        if let Some(clip) = wanted.and_then(|w| animation.clips.get(w)) {
            clip.sample(0.0, animator.looping, &mut target);
        }
        state.settling = (animator.blend > 0.0 && delta > 0.0)
            .then(|| Settling::between(&state.shown, &state.before, &target, delta, animator.blend * 0.5));
        state.playing = wanted;
        state.time = 0.0;
    }
    let root = root_bone(animation);
    let mut pose = rest.clone();
    let mut travelled = (Vec3::ZERO, 0.0);
    if let Some(clip) = state.playing.and_then(|p| animation.clips.get(p)) {
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
    if let Some(settling) = &mut state.settling {
        settling.since += delta;
        settling.apply(&mut pose);
        if settling.since > settling.halflife * 8.0 {
            state.settling = None;
        }
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

impl Settling {
    /// Each bone's offset from `target` and how fast it was changing,
    /// from the last two poses shown, `delta` apart.
    fn between(shown: &[BonePose], before: &[BonePose], target: &[BonePose], delta: f32, halflife: f32) -> Self {
        let offsets = shown
            .iter()
            .zip(before)
            .zip(target)
            .map(|((now, then), target)| {
                let moved = now.translation - target.translation;
                let speed = (now.translation - then.translation) / delta;
                let turned = turn(now.rotation, target.rotation);
                let spin = turn(now.rotation, then.rotation) / delta;
                (moved, speed, turned, spin)
            })
            .collect();
        Self { offsets, since: 0.0, halflife: halflife.max(1e-3) }
    }

    /// Adds what is left of each bone's offset to `pose`, the offsets dying
    /// away as a critically damped spring does.
    fn apply(&self, pose: &mut [BonePose]) {
        let y = 4.0 * std::f32::consts::LN_2 / self.halflife;
        let t = self.since;
        let fade = |x: Vec3, v: Vec3| (x + (v + x * y) * t) * (-y * t).exp();
        for (bone, &(moved, speed, turned, spin)) in pose.iter_mut().zip(&self.offsets) {
            bone.translation += fade(moved, speed);
            bone.rotation = (Quat::from_scaled_axis(fade(turned, spin)) * bone.rotation).normalize();
        }
    }
}

/// The turn from `to` to `from`, as an axis scaled by its angle, the
/// shorter way round.
fn turn(from: Quat, to: Quat) -> Vec3 {
    let mut q = from * to.inverse();
    if q.w < 0.0 {
        q = -q;
    }
    q.to_scaled_axis()
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
            step(&mut state, &animator, &animation, Some(1), dt);
        }
        let before = state.shown[0].translation.z;
        assert!((before - 0.5).abs() < 0.02, "half way along: {before}");
        animator.clip = "Still".into();
        step(&mut state, &animator, &animation, Some(0), dt);
        let after = state.shown[0].translation.z;
        assert!((after - before).abs() < 0.03, "no jump back to the start: {before} then {after}");
        for _ in 0..60 {
            step(&mut state, &animator, &animation, Some(0), dt);
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
            travelled += step(&mut state, &animator, &animation, Some(1), 1.0 / 60.0).0;
        }
        assert!((travelled.z - 1.5).abs() < 0.03, "a metre and a half, round the loop: {travelled}");
        assert!(state.shown[0].translation.z.abs() < 1e-5, "the root stays over the Obj");
    }
}
