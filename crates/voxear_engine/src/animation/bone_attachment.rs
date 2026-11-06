//! Bone attachments: an Obj kept on a bone of its parent's model as the
//! model is posed, as eyes on a character's head or a gun in its hand. Its
//! place, and with `turn` its turn, follow the bone each frame, after the
//! bones are posed and before `LateUpdate`, so scripts there see it where
//! the bone is this frame. With `hide`, the bone and every bone under it
//! shrink to nothing, as a first-person camera inside the head wants.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{ModelRenderer, Pose, Skeleton};
use voxear_scene::{Destroying, Parent, Transform};

/// Keeps its Obj on a bone of its parent's model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoneAttachment {
    /// The bone it follows, by its name in the parent's model.
    pub bone: String,
    /// Where it sits from the bone, in metres: in the parent's space, or
    /// in the bone's when it turns with it.
    pub offset: [f32; 3],
    /// Turning as the bone turns, rather than keeping its own turn.
    pub turn: bool,
    /// Shrinking the bone, and every bone under it, to nothing.
    pub hide: bool,
}

impl Component for BoneAttachment {}

impl Default for BoneAttachment {
    fn default() -> Self {
        Self { bone: String::new(), offset: [0.0; 3], turn: true, hide: false }
    }
}

/// Sets what answers `BoneAttachment` in scripts.
pub fn register() {
    voxear_scripting::set_bone_attachment_values(values);
}

// What `values` reads or writes, as `NativeBoneAttachment` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const OFFSET: i32 = 3;
const TURN: i32 = 4;
const HIDE: i32 = 5;

/// Reads value `what` of `entity`'s Bone Attachment, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let toggle = |on: bool| [f32::from(u8::from(on)), 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(toggle(world.has::<BoneAttachment>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<BoneAttachment>(entity) {
                world.insert(entity, BoneAttachment::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<BoneAttachment>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let attachment = world.get_mut::<BoneAttachment>(entity)?;
    match written {
        None => Some(match what {
            OFFSET => [attachment.offset[0], attachment.offset[1], attachment.offset[2], 0.0],
            TURN => toggle(attachment.turn),
            HIDE => toggle(attachment.hide),
            _ => return None,
        }),
        Some([x, y, z, w]) => {
            match what {
                OFFSET => attachment.offset = [x, y, z],
                TURN => attachment.turn = x != 0.0,
                HIDE => attachment.hide = x != 0.0,
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

/// Moves every Obj with a Bone Attachment onto its bone, and hides the
/// bones those that hide them are on.
pub fn attach_to_bones(world: &mut World) {
    let attached: Vec<(Entity, BoneAttachment, Entity)> = world
        .query_ref::<(Entity, &BoneAttachment, &Parent)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, attachment, parent)| (entity, attachment.clone(), parent.0))
        .collect();
    for (entity, attachment, parent) in attached {
        let Some(animation) = world.get::<ModelRenderer>(parent).and_then(|model| model.animation.clone()) else {
            continue;
        };
        let Some(bone) = animation.skeleton.find(&attachment.bone) else { continue };
        let Some(pose) = world.get::<Pose>(parent).cloned() else { continue };
        let Some(&on) = pose.bones.get(bone) else { continue };
        // The parent's space is the model's, so the bone's matrix places the Obj there.
        let offset = Vec3::from(attachment.offset);
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            match attachment.turn {
                true => {
                    let (_, rotation, position) = (on * Mat4::from_translation(offset)).to_scale_rotation_translation();
                    (transform.position, transform.rotation) = (position, rotation);
                }
                false => transform.position = on.w_axis.truncate() + offset,
            }
        }
        if attachment.hide {
            let bones = hidden(&animation.skeleton, bone, &pose.bones);
            world.insert(parent, Pose { bones: bones.into() });
        }
    }
}

/// The bones of a pose with `bone`, and every bone under it, shrunk to
/// nothing where `bone` is.
fn hidden(skeleton: &Skeleton, bone: usize, bones: &[Mat4]) -> Vec<Mat4> {
    let at = bones[bone].w_axis.truncate();
    let shrink = Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(1e-4)) * Mat4::from_translation(-at);
    let mut under = vec![false; bones.len()];
    let mut out = bones.to_vec();
    // Parents come before their children, so one pass finds them all.
    for (index, matrix) in out.iter_mut().enumerate() {
        under[index] = index == bone || skeleton.bones[index].parent.is_some_and(|parent| under[parent]);
        if under[index] {
            *matrix = shrink * *matrix;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use voxear_core::math::Quat;
    use voxear_render::{Bone, BonePose, ModelAnimation};

    /// A model Obj of a neck with a head on it, posed with the head a metre
    /// up and turned a quarter about Y, and an Obj under it on the head.
    fn posed() -> (World, Entity, Entity) {
        let mut world = World::new();
        let rest = BonePose::default();
        let skeleton = Skeleton {
            bones: vec![
                Bone { name: "neck".into(), parent: None, rest },
                Bone { name: "head".into(), parent: Some(0), rest },
                Bone { name: "jaw".into(), parent: Some(1), rest },
            ],
        };
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
        let model = world.spawn((
            Transform::default(),
            ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) },
        ));
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let head = Mat4::from_rotation_translation(turned, Vec3::Y);
        let jaw = head * Mat4::from_translation(Vec3::new(0.0, -0.1, 0.1));
        world.insert(model, Pose { bones: vec![Mat4::IDENTITY, head, jaw].into() });
        let eyes = world.spawn((Transform::default(), Parent(model)));
        (world, model, eyes)
    }

    #[test]
    fn an_obj_follows_its_bone_and_may_turn_with_it() {
        let (mut world, _, eyes) = posed();
        world.insert(eyes, BoneAttachment { bone: "head".into(), offset: [0.0, 0.0, 0.2], turn: true, hide: false });
        attach_to_bones(&mut world);
        let placed = *world.get::<Transform>(eyes).unwrap();
        assert!(
            placed.position.distance(Vec3::new(0.2, 1.0, 0.0)) < 1e-5,
            "ahead of the turned head: {}",
            placed.position
        );
        world.insert(eyes, BoneAttachment { bone: "head".into(), offset: [0.0, 0.0, 0.2], turn: false, hide: false });
        world.insert(eyes, Transform::default());
        attach_to_bones(&mut world);
        let placed = *world.get::<Transform>(eyes).unwrap();
        assert!(
            placed.position.distance(Vec3::new(0.0, 1.0, 0.2)) < 1e-5,
            "ahead in the parent's space: {}",
            placed.position
        );
        assert_eq!(placed.rotation, Quat::IDENTITY, "keeping its own turn");
    }

    #[test]
    fn hiding_shrinks_the_bone_and_those_under_it() {
        let (mut world, model, eyes) = posed();
        world.insert(eyes, BoneAttachment { bone: "head".into(), hide: true, ..BoneAttachment::default() });
        attach_to_bones(&mut world);
        let bones = world.get::<Pose>(model).unwrap().bones.clone();
        assert_eq!(bones[0], Mat4::IDENTITY, "the neck as it was");
        for hidden in [1, 2] {
            let size = bones[hidden].x_axis.truncate().length();
            assert!(size < 1e-3, "bone {hidden} shrunk: {size}");
            assert!(bones[hidden].w_axis.truncate().distance(Vec3::Y) < 1e-3, "into the head");
        }
    }
}
