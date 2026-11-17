//! A model posed by another Obj's skeleton: each of its bones put where the
//! other skeleton's bone of the same name is this frame, as a part worn on
//! a body follows it, with no script. Bones the other skeleton has not
//! stand at rest from the bones they hang from. Run after the pose stage,
//! so what scripts set in `PoseUpdate` is followed, and before Bone
//! Attachments follow their bones.

use std::sync::{Arc, Weak};

use voxear_core::math::Mat4;
use voxear_ecs::{Component, Entity, World};
use voxear_render::{HiddenBones, ModelAnimation, ModelRenderer, Pose};
use voxear_scene::{Destroying, Parent, world_matrix};

use crate::scene_file::ModelSource;

/// Has an Obj's model posed by another Obj's skeleton than its parent's,
/// which its Model Renderer's `follow_parent` asks for. Set by scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FollowSkeleton {
    pub of: Entity,
}

impl Component for FollowSkeleton {}

/// Which bone of the followed skeleton each bone of the model is, kept
/// while the two skeletons stay the ones it was made for.
struct FollowMap {
    ours: Weak<ModelAnimation>,
    theirs: Weak<ModelAnimation>,
    bones: Vec<Option<usize>>,
}

impl Component for FollowMap {}

/// The Obj whose skeleton poses `entity`'s model, if one does.
pub fn followed(world: &World, entity: Entity) -> Option<Entity> {
    if let Some(follow) = world.get::<FollowSkeleton>(entity) {
        return Some(follow.of);
    }
    let follows_parent = world.get::<ModelSource>(entity).is_some_and(|source| source.0.follow_parent);
    follows_parent.then(|| world.get::<Parent>(entity).map(|parent| parent.0)).flatten()
}

/// Poses every model that follows another Obj's skeleton, as that skeleton
/// is posed now, and hides from the cameras what the other hides.
pub fn follow_skeletons(world: &mut World) {
    let followers: Vec<Entity> = world
        .query_ref::<(Entity, &ModelRenderer)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, drawn)| drawn.animation.is_some())
        .map(|(entity, _)| entity)
        .filter(|&entity| followed(world, entity).is_some())
        .collect();
    for entity in followers {
        follow(world, entity);
    }
}

fn follow(world: &mut World, entity: Entity) -> Option<()> {
    let target = followed(world, entity).filter(|&target| target != entity && world.is_alive(target))?;
    let ours = world.get::<ModelRenderer>(entity)?.animation.clone()?;
    let theirs = world.get::<ModelRenderer>(target)?.animation.clone()?;
    let fresh = world.get::<FollowMap>(entity).is_some_and(|kept| {
        Weak::ptr_eq(&kept.ours, &Arc::downgrade(&ours)) && Weak::ptr_eq(&kept.theirs, &Arc::downgrade(&theirs))
    });
    if !fresh {
        let bones = ours.skeleton.bones.iter().map(|bone| theirs.skeleton.find(&bone.name)).collect();
        world.insert(entity, FollowMap { ours: Arc::downgrade(&ours), theirs: Arc::downgrade(&theirs), bones });
    }
    let map = world.get::<FollowMap>(entity)?.bones.clone();
    let posed = super::bones::posed(world, target, &theirs.skeleton);
    // From the followed model's space into this one's.
    let across = world_matrix(world, entity).inverse() * world_matrix(world, target);
    let mut bones: Vec<Mat4> = Vec::with_capacity(ours.skeleton.bones.len());
    for (bone, found) in ours.skeleton.bones.iter().zip(&map) {
        let matrix = match found.and_then(|found| posed.get(found)) {
            Some(theirs) => across * *theirs,
            None => {
                let parent = bone.parent.and_then(|parent| bones.get(parent)).copied().unwrap_or(Mat4::IDENTITY);
                parent * bone.rest.matrix()
            }
        };
        bones.push(matrix);
    }
    // What the followed model hides from the cameras, as its head from the eyes inside it, this one hides too.
    let hidden = world.get::<HiddenBones>(target).map(|hidden| {
        map.iter().map(|found| found.is_some_and(|found| hidden.bones.get(found).copied().unwrap_or(false))).collect()
    });
    world.insert(entity, Pose { bones: bones.into() });
    match hidden {
        Some(bones) => drop(world.insert(entity, HiddenBones { bones })),
        None => drop(world.remove::<HiddenBones>(entity)),
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use voxear_core::math::{Quat, Vec3};
    use voxear_render::{Bone, BonePose, Skeleton};
    use voxear_scene::Transform;

    use super::*;
    use crate::scene_file::ModelData;

    fn animation(bones: &[(&str, Option<usize>, Vec3)]) -> Arc<ModelAnimation> {
        let bones = bones
            .iter()
            .map(|&(name, parent, translation)| Bone {
                name: name.into(),
                parent,
                rest: BonePose { translation, ..BonePose::default() },
            })
            .collect();
        Arc::new(ModelAnimation { skeleton: Skeleton { bones }, skins: Vec::new(), clips: Vec::new() })
    }

    #[test]
    fn a_worn_part_is_posed_by_its_parents_skeleton_by_name() {
        let mut world = World::new();
        let body = animation(&[("Body", None, Vec3::Y), ("Head", Some(0), Vec3::Y * 0.5)]);
        let drawn = ModelRenderer { animation: Some(body.clone()), ..ModelRenderer::new(Vec::new()) };
        let turned = Quat::from_rotation_z(0.5);
        let bones = vec![Mat4::from_translation(Vec3::Y), Mat4::from_rotation_translation(turned, Vec3::Y * 1.5)];
        let character = world.spawn((Transform::default(), drawn, Pose { bones: bones.into() }));
        world.insert(character, HiddenBones { bones: vec![false, true] });

        // A hat on the head, with a feather of its own on it.
        let hat = animation(&[("Head", None, Vec3::Y * 1.5), ("Feather", Some(0), Vec3::X * 0.1)]);
        let worn = ModelRenderer { animation: Some(hat), ..ModelRenderer::new(Vec::new()) };
        let source = ModelSource(ModelData { follow_parent: true, ..ModelData::of("Parts/Hat.boxmodel") });
        let part = world.spawn((Transform::default(), worn, source, Parent(character)));
        assert_eq!(followed(&world, part), Some(character));

        follow_skeletons(&mut world);
        let pose = world.get::<Pose>(part).expect("posed by the body");
        let head = pose.bones[0].to_scale_rotation_translation();
        assert!((head.2 - Vec3::Y * 1.5).length() < 1e-5 && head.1.angle_between(turned) < 1e-4, "{head:?}");
        let feather = pose.bones[1].w_axis.truncate();
        assert!((feather - (Vec3::Y * 1.5 + turned * Vec3::X * 0.1)).length() < 1e-5, "{feather}");
        assert_eq!(world.get::<HiddenBones>(part).unwrap().bones, vec![true, false], "hidden as the head is");

        // Not following any more, it keeps the pose it was last given.
        world.get_mut::<ModelSource>(part).unwrap().0.follow_parent = false;
        assert_eq!(followed(&world, part), None);
    }
}
