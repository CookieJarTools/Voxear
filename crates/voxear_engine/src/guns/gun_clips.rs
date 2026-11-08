//! A gun's own clips, for a gun from an animation pack drawn by one skinned
//! model whose bones are its parts: the clip of the part of an action under
//! way, else the clip a shot plays for as long as it lasts, else the clip
//! the gun stands in between, else the model's rest pose, posing its bones
//! each frame. A clip from the gun's own file plays as it is; one from
//! another file of the same rig keeps the moves of the bones that sit where
//! the gun's do (see `retarget`).

use std::sync::Arc;

use voxear_ecs::{Entity, World};
use voxear_render::{BonePose, ModelAnimation, ModelRenderer, Pose};

use super::handling::PackClips;
use super::playing::ActionPlay;
use crate::animation::{Animator, Clips};

/// Poses the gun's bones by the clip that plays now, `since_shot` seconds
/// after the last shot, if there was one. An Animator on the gun poses it
/// instead.
pub fn pose_gun(world: &mut World, gun: Entity, pack: &PackClips, play: Option<&ActionPlay>, since_shot: Option<f32>) {
    if world.has::<Animator>(gun) {
        return;
    }
    let Some(animation) = world.get::<ModelRenderer>(gun).and_then(|model| model.animation.clone()) else { return };
    let part = play.and_then(ActionPlay::part).filter(|(part, _)| !part.gun.is_empty());
    let fired = since_shot.filter(|&since| since < length(world, &pack.gun_fire));
    let wanted = match (part, fired) {
        (Some((part, at)), _) => Some((part.gun.clone(), at)),
        (None, Some(since)) if !pack.gun_fire.is_empty() => Some((pack.gun_fire.clone(), since)),
        _ => (!pack.rest.is_empty()).then(|| (pack.rest.clone(), 0.0)),
    };
    let skeleton = &animation.skeleton;
    let pose = wanted.and_then(|(clip, at)| sample(world, &clip, &animation, at)).unwrap_or_else(|| skeleton.rest());
    world.insert(gun, Pose { bones: skeleton.model_matrices(&pose).into() });
}

/// How long a clip lasts, in seconds; none before it is read.
pub fn length(world: &World, reference: &str) -> f32 {
    let source = world.resource::<Clips>().and_then(|clips| clips.get(reference));
    source.map_or(0.0, |source| source.clip().duration())
}

/// A clip's pose at `at` seconds, made to fit `target`'s skeleton.
fn sample(world: &mut World, reference: &str, target: &Arc<ModelAnimation>, at: f32) -> Option<Vec<BonePose>> {
    let source = world.resource::<Clips>()?.get(reference)?.clone();
    let clip = crate::animation::retarget::fitted(world, &source, target);
    let mut pose = target.skeleton.rest();
    clip.sample(at, false, &mut pose);
    Some(pose)
}
