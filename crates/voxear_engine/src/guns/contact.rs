//! What a gun's surfaces are to the hands holding it: a box for each of its
//! parts' meshes, as the renderer bounds them, which moves with its part,
//! and for a gun drawn by a skinned model, a box for each of its bones
//! round the vertices the bone moves, which moves as the bone is posed.
//! A palm is put on the surface under a socket, and fingers close until
//! they touch it, so neither goes into the gun (see `animation::boxes`).

use voxear_core::math::{Mat4, Vec3};

use crate::animation::boxes::Shape;
use voxear_ecs::{Component, Entity, World};
use voxear_render::{MeshRenderer, ModelRenderer, Pose, Renderer};
use voxear_scene::{Hidden, Parent, world_matrix};

use super::gun::Gun;
use super::mechanism::{GunPart, PartRole};

/// A gun's parts' meshes, each with its box in its own space: corners
/// least and most. Found once the renderer can bound them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GunShapes {
    pub pieces: Vec<(Entity, Vec3, Vec3)>,
    /// A skinned model's bones: the Obj drawing it, the bone, and the box
    /// round what the bone moves, in the bone's space.
    pub bones: Vec<(Entity, usize, Vec3, Vec3)>,
}

impl Component for GunShapes {}

/// Bounds each gun's parts' meshes once there is a renderer to ask: all
/// but a spent case and a clip, which are only looks.
pub fn give_shapes(world: &mut World, renderer: Option<&Renderer>) {
    let Some(renderer) = renderer else { return };
    let guns: Vec<Entity> =
        world.query_ref::<(Entity, &Gun)>().without::<GunShapes>().into_iter().map(|(entity, _)| entity).collect();
    for gun in guns {
        let meshes: Vec<(Entity, MeshRenderer)> =
            world.query_ref::<(Entity, &MeshRenderer)>().into_iter().map(|(e, m)| (e, *m)).collect();
        let pieces = meshes
            .into_iter()
            .filter(|(entity, _)| *entity != gun && under(world, *entity, gun))
            .filter(|(entity, _)| {
                !world.get::<GunPart>(*entity).is_some_and(|part| matches!(part.role, PartRole::Case | PartRole::Clip))
            })
            .filter_map(|(entity, mesh)| renderer.mesh_bounds(mesh.mesh).map(|(least, most)| (entity, least, most)))
            .collect();
        world.insert(gun, GunShapes { pieces, bones: bone_boxes(world, gun) });
    }
}

/// The boxes of the bones of each skinned model on or under `gun`.
fn bone_boxes(world: &World, gun: Entity) -> Vec<(Entity, usize, Vec3, Vec3)> {
    let mut boxes = Vec::new();
    for (entity, model) in world.query_ref::<(Entity, &ModelRenderer)>().into_iter() {
        let Some(animation) = model.animation.as_ref().filter(|_| under(world, entity, gun)) else { continue };
        let skins = model.parts.iter().filter_map(|part| part.skin).collect::<std::collections::BTreeSet<_>>();
        for skin in skins.into_iter().filter_map(|skin| animation.skins.get(skin as usize)) {
            let joints = skin.joints.iter().zip(&skin.bounds).filter(|(_, (least, most))| least.cmple(*most).all());
            boxes.extend(joints.map(|(&bone, &(least, most))| (entity, bone, least, most)));
        }
    }
    boxes
}

/// A gun's boxes where they are now, in the gun's space: its shown parts
/// but those `leaving` out, moved as they are, and its bones but the
/// `leaving_bones`, as they are posed.
pub fn shapes_now(world: &World, gun: Entity, leaving: &[Entity], leaving_bones: &[usize]) -> Vec<Shape> {
    let Some(shapes) = world.get::<GunShapes>(gun) else { return Vec::new() };
    let into_gun = world_matrix(world, gun).inverse();
    let parts = shapes
        .pieces
        .iter()
        .filter(|(entity, ..)| !world.has::<Hidden>(*entity) && !leaving.contains(entity))
        .map(|&(entity, least, most)| Shape { frame: into_gun * world_matrix(world, entity), least, most });
    let bones = shapes.bones.iter().filter(|(_, bone, ..)| !leaving_bones.contains(bone)).filter_map(
        |&(entity, bone, least, most)| {
            let placed = posed(world, entity, bone)?;
            Some(Shape { frame: into_gun * world_matrix(world, entity) * placed, least, most })
        },
    );
    parts.chain(bones).collect()
}

/// Where `bone` of the model `entity` draws is, in the model's space: as
/// its Pose has it, or at rest without one.
fn posed(world: &World, entity: Entity, bone: usize) -> Option<Mat4> {
    if let Some(pose) = world.get::<Pose>(entity) {
        return pose.bones.get(bone).copied();
    }
    let skeleton = &world.get::<ModelRenderer>(entity)?.animation.as_ref()?.skeleton;
    skeleton.model_matrices(&skeleton.rest()).get(bone).copied()
}

fn under(world: &World, obj: Entity, owner: Entity) -> bool {
    let mut at = Some(obj);
    while let Some(here) = at {
        if here == owner {
            return true;
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    false
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_core::math::Quat;
    use voxear_render::{Bone, BonePose, MaterialHandle, MeshHandle, ModelAnimation, ModelPart, Skeleton, Skin};
    use voxear_scene::Transform;

    use super::*;
    use crate::animation::boxes::nearest;

    /// A gun drawn by one skinned model, its bolt a bone whose box moves as
    /// the bone is posed, and stays out when the bolt is in hand.
    #[test]
    fn a_skinned_guns_bones_are_boxed_where_they_are_posed() {
        let rest = |translation| BonePose { translation, ..BonePose::default() };
        let skeleton = Skeleton {
            bones: vec![
                Bone { name: "Root".into(), parent: None, rest: rest(Vec3::ZERO) },
                Bone { name: "bolt".into(), parent: Some(0), rest: rest(Vec3::new(0.0, 0.045, 0.04)) },
            ],
        };
        let binds = skeleton.model_matrices(&skeleton.rest()).iter().map(|bone| bone.inverse()).collect();
        let bounds = vec![(Vec3::splat(-0.02), Vec3::splat(0.02)), (Vec3::splat(-0.01), Vec3::splat(0.01))];
        let skin = Skin { joints: vec![0, 1], inverse_binds: binds, bounds };
        let animation = Arc::new(ModelAnimation { skeleton, skins: vec![skin], clips: Vec::new() });
        let part = ModelPart {
            mesh: MeshHandle(0),
            material: MaterialHandle(0),
            transform: Mat4::IDENTITY,
            node: 0,
            skin: Some(0),
        };
        let model = ModelRenderer { animation: Some(animation.clone()), ..ModelRenderer::new(vec![part]) };
        let mut world = World::new();
        let gun = world.spawn((Transform::default(), Gun::default(), model));
        world.insert(gun, GunShapes { pieces: Vec::new(), bones: bone_boxes(&world, gun) });
        assert_eq!(world.get::<GunShapes>(gun).unwrap().bones.len(), 2);

        let at_rest = shapes_now(&world, gun, &[], &[]);
        assert_eq!(nearest(&at_rest, Vec3::new(0.0, 0.045, 0.04)), 0.0, "the bolt's box is round the bolt at rest");
        // The bolt pulled back 8 cm and turned up.
        let mut pose = animation.skeleton.rest();
        pose[1] = BonePose { rotation: Quat::from_rotation_z(1.4), ..rest(Vec3::new(0.0, 0.045, -0.04)) };
        world.insert(gun, Pose { bones: animation.skeleton.model_matrices(&pose).into() });
        let pulled = shapes_now(&world, gun, &[], &[]);
        assert_eq!(nearest(&pulled, Vec3::new(0.0, 0.045, -0.04)), 0.0, "it went back with the bolt");
        assert!(nearest(&pulled, Vec3::new(0.0, 0.045, 0.04)) > 0.005, "and left where it was");
        assert_eq!(shapes_now(&world, gun, &[], &[1]).len(), 1, "a bolt in hand is left out");
    }
}
