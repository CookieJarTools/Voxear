//! A model's bones for scripts (`Skeleton` and `Bone` in Voxear.Core): each
//! bone's name and parent, its place at rest, and where it is now, as
//! animation and scripts have posed it this frame. A bone set from a
//! script's PoseUpdate bends the model after animation; what is set stays
//! until animation poses the model again.

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{ModelRenderer, Pose, Skeleton};
use voxear_scene::world_matrix;
use voxear_scripting::ComponentKind;

use super::ik::PoseEdit;

// What `values` reads or writes, in the low byte of its number, as `Bone` in
// Voxear.Core numbers them; the bone is the number's higher bytes.
const COUNT: i32 = 0;
const PARENT: i32 = 1;
const LOCAL_POSITION: i32 = 2;
const LOCAL_ROTATION: i32 = 3;
const POSITION: i32 = 4;
const ROTATION: i32 = 5;
const REST_POSITION: i32 = 6;
const REST_ROTATION: i32 = 7;

// What `texts` answers.
const NAME: i32 = 0;
const FIND: i32 = 1;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Skeleton, values);
    voxear_scripting::set_kind_texts(ComponentKind::Skeleton, texts);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let animation = world.get::<ModelRenderer>(entity)?.animation.clone()?;
    let skeleton = &animation.skeleton;
    let (bone, value) = ((what >> 8) as usize, what & 0xff);
    if value == COUNT {
        return Some([skeleton.bones.len() as f32, 0.0, 0.0, 0.0]);
    }
    let rest = skeleton.bones.get(bone)?.rest;
    let matrices = posed(world, entity, skeleton);
    let Some(written) = written else {
        return read(world, entity, skeleton, &matrices, bone, value, (rest.translation, rest.rotation));
    };
    let mut edit = PoseEdit::new(skeleton, &matrices);
    let model = world_matrix(world, entity);
    let into_parent = |edit: &PoseEdit| {
        let parent = skeleton.bones[bone].parent.map_or(Mat4::IDENTITY, |parent| edit.matrices()[parent]);
        parent.inverse() * model.inverse()
    };
    let point = Vec3::new(written[0], written[1], written[2]);
    match value {
        LOCAL_POSITION => edit.local[bone].translation = point,
        LOCAL_ROTATION => edit.local[bone].rotation = Quat::from_array(written).normalize(),
        POSITION => edit.local[bone].translation = into_parent(&edit).transform_point3(point),
        ROTATION => {
            let turn = model.to_scale_rotation_translation().1.inverse() * Quat::from_array(written);
            edit.set_rotation(bone, turn.normalize());
        }
        _ => return None,
    }
    world.insert(entity, Pose { bones: edit.matrices().into() });
    Some(written)
}

/// A value of a bone as it is posed now, or at rest.
fn read(
    world: &World,
    entity: Entity,
    skeleton: &Skeleton,
    matrices: &[Mat4],
    bone: usize,
    value: i32,
    (rest_at, rest_turn): (Vec3, Quat),
) -> Option<[f32; 4]> {
    let point = |p: Vec3| [p.x, p.y, p.z, 0.0];
    let in_world = || world_matrix(world, entity) * matrices[bone];
    Some(match value {
        PARENT => [skeleton.bones[bone].parent.map_or(-1.0, |parent| parent as f32), 0.0, 0.0, 0.0],
        LOCAL_POSITION => point(PoseEdit::new(skeleton, matrices).local[bone].translation),
        LOCAL_ROTATION => PoseEdit::new(skeleton, matrices).local[bone].rotation.to_array(),
        POSITION => point(in_world().w_axis.truncate()),
        ROTATION => in_world().to_scale_rotation_translation().1.to_array(),
        REST_POSITION => point(rest_at),
        REST_ROTATION => rest_turn.to_array(),
        _ => return None,
    })
}

fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    let animation = world.get::<ModelRenderer>(entity)?.animation.clone()?;
    let bones = &animation.skeleton.bones;
    match what & 0xff {
        NAME => bones.get((what >> 8) as usize).map(|bone| bone.name.clone()),
        FIND => {
            let exact = bones.iter().position(|bone| bone.name == text);
            let found = exact.or_else(|| bones.iter().position(|bone| bone.name.eq_ignore_ascii_case(text)));
            Some(found.map_or(-1, |index| index as i64).to_string())
        }
        _ => None,
    }
}

/// A model's bones in its space as posed now: by its Pose, or at rest
/// without one.
pub(super) fn posed(world: &World, entity: Entity, skeleton: &Skeleton) -> Vec<Mat4> {
    match world.get::<Pose>(entity).filter(|pose| pose.bones.len() == skeleton.bones.len()) {
        Some(pose) => pose.bones.to_vec(),
        None => skeleton.model_matrices(&skeleton.rest()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_render::{Bone, BonePose, ModelAnimation};
    use voxear_scene::Transform;

    use super::*;

    /// An arm: a shoulder at the Obj's middle, an elbow 30 cm out along X.
    fn arm() -> (World, Entity) {
        let rest = |translation| BonePose { translation, ..BonePose::default() };
        let skeleton = Skeleton {
            bones: vec![
                Bone { name: "Shoulder".into(), parent: None, rest: rest(Vec3::ZERO) },
                Bone { name: "Elbow".into(), parent: Some(0), rest: rest(Vec3::new(0.3, 0.0, 0.0)) },
            ],
        };
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
        let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
        let mut world = World::new();
        let at = Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let entity = world.spawn((at, model));
        (world, entity)
    }

    fn at(world: &mut World, entity: Entity, bone: i32, value: i32) -> [f32; 4] {
        values(world, entity, bone << 8 | value, None).unwrap()
    }

    #[test]
    fn a_scripts_bones_read_where_they_are_and_turning_one_carries_those_under_it() {
        let (mut world, arm) = arm();
        assert_eq!(values(&mut world, arm, COUNT, None), Some([2.0, 0.0, 0.0, 0.0]));
        assert_eq!(texts(&mut world, arm, 1 << 8 | NAME, ""), Some("Elbow".into()));
        assert_eq!(texts(&mut world, arm, FIND, "elbow"), Some("1".into()), "found whatever its case");
        assert_eq!(texts(&mut world, arm, FIND, "Wrist"), Some("-1".into()));
        assert_eq!(at(&mut world, arm, 1, PARENT)[0], 0.0);
        let elbow = at(&mut world, arm, 1, POSITION);
        assert!((Vec3::new(elbow[0], elbow[1], elbow[2]) - Vec3::new(0.3, 1.0, 0.0)).length() < 1e-5, "{elbow:?}");

        // The shoulder turned a quarter about Y, in the world: the elbow swings round with it.
        let quarter = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        values(&mut world, arm, ROTATION, Some(quarter.to_array()));
        let elbow = at(&mut world, arm, 1, POSITION);
        assert!((Vec3::new(elbow[0], elbow[1], elbow[2]) - Vec3::new(0.0, 1.0, -0.3)).length() < 1e-5, "{elbow:?}");
        let rest = at(&mut world, arm, 1, REST_POSITION);
        assert_eq!(rest[0], 0.3, "its rest stays as it was");

        // The elbow put somewhere in the world: its local place follows, in the turned shoulder's space.
        values(&mut world, arm, 1 << 8 | POSITION, Some([0.0, 1.0, -0.5, 0.0]));
        let local = at(&mut world, arm, 1, LOCAL_POSITION);
        assert!((Vec3::new(local[0], local[1], local[2]) - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-5, "{local:?}");
        assert!(values(&mut world, arm, 7 << 8 | POSITION, None).is_none(), "no such bone");
    }
}
