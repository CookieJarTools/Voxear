//! What scripts do to a posed model in one call each (`Rig`, `RigArm` and
//! `Skeleton` in Voxear.Core): find its humanoid rig, bend a limb, reach or
//! turn a hand, close its fingers round boxes, turn the spine, and pose it
//! by a clip. Places and ways come and go in the world, and are turned into
//! the model's space here. What changes the pose writes it back, so what a
//! script does in PoseUpdate shows this frame.

use std::collections::BTreeSet;

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{BonePose, ModelRenderer, Pose};
use voxear_scene::world_matrix;

use super::boxes::Shape;
use super::clips::Clips;
use super::grip::{close_by, finger_bends, keep_out, knuckles, palm_of, seat_hand};
use super::ik::{PoseEdit, aim_hand, hand_axes, reach_hand, turn_spine, two_bone};
use super::rig::{Arm, Rig};

// What `run` does, as Voxear.Core's NativePose numbers them.
const RIG: i32 = 0;
const TWO_BONE: i32 = 1;
const REACH_HAND: i32 = 2;
const AIM_HAND: i32 = 3;
const HAND_AXES: i32 = 4;
const PALM: i32 = 5;
const SEAT_HAND: i32 = 6;
const CLOSE_FINGERS: i32 = 7;
const FINGER_BENDS: i32 = 8;
const KEEP_OUT: i32 = 9;
const KNUCKLES: i32 = 10;
const TURN_SPINE: i32 = 11;
const SAMPLE_CLIP: i32 = 12;
const CLIP_LENGTH: i32 = 13;
const BONE_BOXES: i32 = 14;

/// How many floats a box takes: its frame's sixteen, then its least and
/// most corners.
const BOX: usize = 22;

/// Clips scripts have asked to pose models by, for `give_clips` to find.
#[derive(Default)]
pub struct ScriptClips(pub BTreeSet<String>);

pub(crate) fn register() {
    voxear_scripting::set_pose_ops(run);
}

/// Does `op` on the model `entity` draws, writing what it gives back into
/// `out`: how many it wrote, or `None` when it cannot be done.
fn run(
    world: &mut World,
    entity: Entity,
    op: i32,
    ints: &[i32],
    floats: &[f32],
    text: &str,
    out: &mut [f32],
) -> Option<usize> {
    if !world.is_alive(entity) {
        return None;
    }
    let animation = world.get::<ModelRenderer>(entity)?.animation.clone()?;
    let skeleton = &animation.skeleton;
    let model = world_matrix(world, entity);
    let into = model.inverse();
    let turn_of_model = model.to_scale_rotation_translation().1;
    let matrices = super::bones::posed(world, entity, skeleton);
    let mut edit = PoseEdit::new(skeleton, &matrices);
    let at = |index: usize| into.transform_point3(vec3(floats, index));
    let way = |index: usize| into.transform_vector3(vec3(floats, index)).normalize_or_zero();
    let world_way = |v: Vec3| model.transform_vector3(v).normalize_or_zero();
    let arm = || -> Option<Arm> { Rig::find(skeleton)?.arms.get(usize::try_from(*ints.first()?).ok()?).cloned() };
    let written = match op {
        RIG => return put(out, &rig_layout(&Rig::find(skeleton)?)),
        HAND_AXES => {
            let (fingers, palm) = hand_axes(&edit, &arm()?)?;
            return put(out, &[world_way(fingers).to_array(), world_way(palm).to_array()].concat());
        }
        PALM => {
            let palm = palm_of(&edit, &arm()?)?;
            let point = model.transform_point3(palm.point);
            return put(
                out,
                &[point.to_array().as_slice(), &world_way(palm.facing).to_array(), &[palm.finger]].concat(),
            );
        }
        KNUCKLES => return put(out, &model.transform_point3(knuckles(&edit, &arm()?)?).to_array()),
        FINGER_BENDS => return put(out, &flat(finger_bends(&edit, &arm()?))),
        KEEP_OUT => {
            let moved = keep_out(&edit, &arm()?, &shapes(floats.get(3..)?, into), way(0));
            return put(out, &model.transform_vector3(moved).to_array());
        }
        CLIP_LENGTH => {
            let Some(source) = clip_source(world, text)? else { return Some(0) };
            return put(out, &[source.clip().duration()]);
        }
        BONE_BOXES => return put(out, &bone_boxes(world, entity)),
        TWO_BONE => {
            let bones: [usize; 3] = [bone(ints, 0)?, bone(ints, 1)?, bone(ints, 2)?];
            if bones.iter().any(|&bone| bone >= skeleton.bones.len()) {
                return None;
            }
            vec![two_bone(&mut edit, bones, at(0), at(3))]
        }
        REACH_HAND => vec![reach_hand(&mut edit, &arm()?, at(0), at(3), *floats.get(6)?)],
        AIM_HAND => {
            aim_hand(&mut edit, &arm()?, way(0), way(3));
            Vec::new()
        }
        SEAT_HAND => vec![seat_hand(&mut edit, &arm()?, at(0), (way(3), way(6)), at(9), *floats.get(12)?)],
        CLOSE_FINGERS => {
            let (delta, most, was) = (*floats.first()?, unflat(floats.get(1..16)?), unflat(floats.get(16..31)?));
            let mut shown = Some(was);
            close_by(&mut edit, &arm()?, most, &shapes(floats.get(31..)?, into), &mut shown, delta);
            flat(shown.unwrap_or(was)).to_vec()
        }
        TURN_SPINE => {
            let spine: Vec<usize> = ints.iter().filter_map(|&bone| usize::try_from(bone).ok()).collect();
            let turn = Quat::from_array([*floats.first()?, *floats.get(1)?, *floats.get(2)?, *floats.get(3)?]);
            turn_spine(&mut edit, &spine, (turn_of_model.inverse() * turn * turn_of_model).normalize());
            Vec::new()
        }
        SAMPLE_CLIP => {
            let Some(source) = clip_source(world, text)? else { return Some(0) };
            let fitted = super::retarget::fitted(world, &source, &animation);
            let (time, weight, looping) =
                (*floats.first()?, floats.get(1).copied().unwrap_or(1.0), floats.get(2) == Some(&1.0));
            let mut sampled = vec![BonePose::default(); skeleton.bones.len()];
            fitted.sample(time, looping, &mut sampled);
            let only: Vec<usize> = ints.iter().filter_map(|&bone| usize::try_from(bone).ok()).collect();
            for (index, (now, from_clip)) in edit.local.iter_mut().zip(&sampled).enumerate() {
                if only.is_empty() || only.contains(&index) {
                    *now = blend(*now, *from_clip, weight.clamp(0.0, 1.0));
                }
            }
            vec![1.0]
        }
        _ => return None,
    };
    world.insert(entity, Pose { bones: edit.matrices().into() });
    put(out, &written)
}

/// A clip's source by its reference: `Some(None)` while it is being found,
/// asked for here so `give_clips` finds it, and `None` when it cannot be.
fn clip_source(world: &mut World, reference: &str) -> Option<Option<super::clips::ClipSource>> {
    let clips = world.resource::<Clips>();
    if let Some(source) = clips.and_then(|clips| clips.get(reference)) {
        return Some(Some(source.clone()));
    }
    if clips.is_some_and(|clips| clips.tried(reference)) {
        return None;
    }
    if world.resource::<ScriptClips>().is_none() {
        world.insert_resource(ScriptClips::default());
    }
    world.resource_mut::<ScriptClips>()?.0.insert(reference.to_string());
    Some(None)
}

fn blend(now: BonePose, to: BonePose, weight: f32) -> BonePose {
    BonePose {
        translation: now.translation.lerp(to.translation, weight),
        rotation: now.rotation.slerp(to.rotation, weight).normalize(),
        scale: now.scale.lerp(to.scale, weight),
    }
}

/// A humanoid rig as numbers: the hips, neck and head; how many spine bones
/// and each; each arm, left first, as its collarbone, upper arm, forearm and
/// hand, then each finger, thumb first, as how many bones and each; then
/// whether it has legs, and each leg's thigh, shin, foot and toes. A bone
/// it lacks is -1.
fn rig_layout(rig: &Rig) -> Vec<f32> {
    let bone = |bone: usize| bone as f32;
    let maybe = |bone: Option<usize>| bone.map_or(-1.0, |bone| bone as f32);
    let mut out = vec![bone(rig.hips), maybe(rig.neck), bone(rig.head), rig.spine.len() as f32];
    out.extend(rig.spine.iter().map(|&b| bone(b)));
    for arm in &rig.arms {
        out.extend([maybe(arm.clavicle), bone(arm.upper), bone(arm.forearm), bone(arm.hand)]);
        for finger in &arm.fingers {
            out.push(finger.len() as f32);
            out.extend(finger.iter().map(|&b| bone(b)));
        }
    }
    out.push(if rig.legs.is_some() { 1.0 } else { 0.0 });
    for leg in rig.legs.iter().flatten() {
        out.extend([bone(leg.thigh), bone(leg.shin), bone(leg.foot), maybe(leg.toe)]);
    }
    out
}

/// The boxes round what each of the model's skinned bones moves, in the
/// bone's own space: how many, then each as its bone and its least and most
/// corners.
fn bone_boxes(world: &World, entity: Entity) -> Vec<f32> {
    let Some(model) = world.get::<ModelRenderer>(entity) else { return vec![0.0] };
    let Some(animation) = model.animation.as_ref() else { return vec![0.0] };
    let skins: BTreeSet<usize> = model.parts.iter().filter_map(|part| part.skin.map(|skin| skin as usize)).collect();
    let mut out = vec![0.0];
    for skin in skins.into_iter().filter_map(|skin| animation.skins.get(skin)) {
        for (&bone, &(least, most)) in skin.joints.iter().zip(&skin.bounds).filter(|(_, (l, m))| l.cmple(*m).all()) {
            out.push(bone as f32);
            out.extend(least.to_array());
            out.extend(most.to_array());
            out[0] += 1.0;
        }
    }
    out
}

/// Boxes given as floats, each its frame in the world then its corners,
/// moved into the model's space by `into`.
fn shapes(floats: &[f32], into: Mat4) -> Vec<Shape> {
    floats
        .chunks_exact(BOX)
        .map(|b| Shape {
            frame: into * Mat4::from_cols_array(b[..16].try_into().expect("sixteen")),
            least: Vec3::new(b[16], b[17], b[18]),
            most: Vec3::new(b[19], b[20], b[21]),
        })
        .collect()
}

fn vec3(floats: &[f32], at: usize) -> Vec3 {
    let get = |i: usize| floats.get(at + i).copied().unwrap_or(0.0);
    Vec3::new(get(0), get(1), get(2))
}

fn bone(ints: &[i32], at: usize) -> Option<usize> {
    usize::try_from(*ints.get(at)?).ok()
}

fn flat(bends: [[f32; 3]; 5]) -> [f32; 15] {
    std::array::from_fn(|i| bends[i / 3][i % 3])
}

fn unflat(floats: &[f32]) -> [[f32; 3]; 5] {
    std::array::from_fn(|finger| std::array::from_fn(|joint| floats.get(finger * 3 + joint).copied().unwrap_or(0.0)))
}

/// Writes `values` into `out` when they fit, saying how many.
fn put(out: &mut [f32], values: &[f32]) -> Option<usize> {
    out.get_mut(..values.len())?.copy_from_slice(values);
    Some(values.len())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_render::{Clip, ModelAnimation};
    use voxear_scene::Transform;

    use super::*;
    use crate::animation::clips::ClipSource;

    /// A body standing a metre to the right of the origin, its bones named as
    /// most rigs name them.
    fn body() -> (World, Entity, Arc<ModelAnimation>) {
        let skeleton = crate::animation::rig::tests::body(|part, side| match side {
            Some(left) => format!("{part}_{}", if left { "l" } else { "r" }),
            None => part.to_string(),
        });
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
        let model = ModelRenderer { animation: Some(animation.clone()), ..ModelRenderer::new(Vec::new()) };
        let mut world = World::new();
        let at = Transform::new(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let entity = world.spawn((at, model));
        (world, entity, animation)
    }

    fn call(world: &mut World, entity: Entity, op: i32, ints: &[i32], floats: &[f32], text: &str) -> Option<Vec<f32>> {
        let mut out = vec![0.0; 256];
        let written = run(world, entity, op, ints, floats, text, &mut out)?;
        out.truncate(written);
        Some(out)
    }

    #[test]
    fn a_rig_is_given_bone_by_bone_and_a_hand_reaches_where_it_is_told_in_the_world() {
        let (mut world, body, animation) = body();
        let rig = Rig::find(&animation.skeleton).unwrap();
        let layout = call(&mut world, body, RIG, &[], &[], "").unwrap();
        assert_eq!(layout[0] as usize, rig.hips);
        assert_eq!(layout[2] as usize, rig.head);
        assert_eq!(
            *layout.last().unwrap() as usize,
            rig.legs.as_ref().unwrap()[1].toe.unwrap(),
            "it ends with the right toes"
        );

        // The right hand, out at the side at rest, reaches 30 cm before the chest, in the world.
        let target = Vec3::new(1.15, 1.3, 0.3);
        let short = call(&mut world, body, REACH_HAND, &[1], &[target.x, target.y, target.z, 1.0, 1.0, -1.0, 0.0], "");
        assert!(short.unwrap()[0] < 1e-3, "it reaches");
        let wrist =
            PoseEdit::new(&animation.skeleton, &world.get::<Pose>(body).unwrap().bones).position(rig.arms[1].hand);
        assert!((wrist + Vec3::X - target).length() < 2e-3, "its wrist is there, in the world: {wrist}");

        // Its fingers aimed forward with the palm down, as the hand's own axes then read in the world.
        call(&mut world, body, AIM_HAND, &[1], &[0.0, 0.0, 1.0, 0.0, -1.0, 0.0], "").unwrap();
        let axes = call(&mut world, body, HAND_AXES, &[1], &[], "").unwrap();
        assert!(Vec3::new(axes[0], axes[1], axes[2]).dot(Vec3::Z) > 0.99, "{axes:?}");
        assert!(Vec3::new(axes[3], axes[4], axes[5]).dot(Vec3::NEG_Y) > 0.99, "{axes:?}");
        assert!(call(&mut world, body, REACH_HAND, &[2], &[0.0; 7], "").is_none(), "there is no third arm");
    }

    #[test]
    fn fingers_close_round_a_box_and_stop_at_it() {
        let (mut world, body, _) = body();
        call(&mut world, body, AIM_HAND, &[1], &[0.0, 0.0, 1.0, 0.0, -1.0, 0.0], "").unwrap();
        let palm = call(&mut world, body, PALM, &[1], &[], "").unwrap();
        // A bar 3 cm thick across, just under the palm.
        let under = Vec3::new(palm[0], palm[1] - 0.03, palm[2]);
        let frame = Mat4::from_scale_rotation_translation(Vec3::new(0.2, 0.03, 0.03), Quat::IDENTITY, under);
        let mut bar = frame.to_cols_array().to_vec();
        bar.extend([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5]);
        let fist = [1.4f32; 15];
        let closed = |boxes: &[f32], world: &mut World| {
            let mut floats = vec![1.0];
            floats.extend(fist);
            floats.extend([0.0; 15]);
            floats.extend(boxes);
            call(world, body, CLOSE_FINGERS, &[1], &floats, "").unwrap()
        };
        let free = closed(&[], &mut world);
        let held = closed(&bar, &mut world);
        assert!(free.iter().sum::<f32>() > held.iter().sum::<f32>() + 0.2, "the bar stops them: {free:?} {held:?}");
        let bends = call(&mut world, body, FINGER_BENDS, &[1], &[], "").unwrap();
        assert_eq!(bends.len(), 15);
    }

    #[test]
    fn a_clip_poses_the_model_once_found_and_is_asked_for_till_then() {
        let (mut world, body, animation) = body();
        assert_eq!(
            call(&mut world, body, SAMPLE_CLIP, &[], &[0.0, 1.0, 0.0], "Models/Wave.glb#clip:Wave"),
            Some(vec![])
        );
        assert!(world.resource::<ScriptClips>().unwrap().0.contains("Models/Wave.glb#clip:Wave"), "asked for");

        // A clip of the same skeleton that lifts the hips 10 cm.
        let mut poses = animation.skeleton.rest();
        poses[0].translation.y += 0.1;
        let bones = poses.len();
        let clip = Clip { name: "Wave".into(), frames: 1, poses };
        assert_eq!(bones, animation.skeleton.bones.len());
        let source = Arc::new(ModelAnimation { clips: vec![clip], ..(*animation).clone() });
        let mut clips = Clips::default();
        clips
            .put("Models/Wave.glb#clip:Wave", ClipSource { animation: source, index: 0, settings: Default::default() });
        world.insert_resource(clips);
        let length = call(&mut world, body, CLIP_LENGTH, &[], &[], "Models/Wave.glb#clip:Wave").unwrap();
        assert_eq!(length, [0.0], "one frame lasts no time");
        assert_eq!(
            call(&mut world, body, SAMPLE_CLIP, &[], &[0.0, 0.5, 0.0], "Models/Wave.glb#clip:Wave"),
            Some(vec![1.0])
        );
        let hips = world.get::<Pose>(body).unwrap().bones[0].w_axis.y;
        assert!((hips - 1.05).abs() < 1e-4, "half way up: {hips}");
    }
}
