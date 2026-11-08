//! Retargeting by name: a clip made for one model's skeleton plays on
//! another's with the same rig, bone by bone. Each bone the two share by
//! name turns as the clip turns it; it keeps the character's own length, but
//! for the root and hips, whose travel and height the clip gives, scaled
//! by how much taller or shorter the character stands, and for bones that
//! sit where the clip's do, which move as the clip moves them. Bones the
//! clip does not have stay at rest.
//!
//! Rigs that name or turn their bones differently need retargeting through
//! a humanoid's parts instead, which this is not.

use std::sync::{Arc, Weak};

use voxear_ecs::World;
use voxear_render::{BonePose, Clip, ModelAnimation, Skeleton};

use super::clips::ClipSource;

/// A clip made to fit a character's skeleton, the clip as it is when the
/// character is the model it was made for; kept per clip and character.
pub fn fitted(world: &mut World, source: &ClipSource, target: &Arc<ModelAnimation>) -> Arc<Clip> {
    if world.resource::<Fitted>().is_none() {
        world.insert_resource(Fitted::default());
    }
    let fitted = world.resource_mut::<Fitted>().expect("just made");
    fitted.made.retain(|made| made.source.strong_count() > 0 && made.target.strong_count() > 0);
    let found = fitted.made.iter().find(|made| {
        made.index == source.index
            && Weak::ptr_eq(&made.source, &Arc::downgrade(&source.animation))
            && Weak::ptr_eq(&made.target, &Arc::downgrade(target))
    });
    if let Some(made) = found {
        return made.clip.clone();
    }
    let clip = Arc::new(match Arc::ptr_eq(&source.animation, target) {
        true => source.clip().clone(),
        false => retarget(&source.animation.skeleton, source.clip(), &target.skeleton),
    });
    let made = Made {
        source: Arc::downgrade(&source.animation),
        index: source.index,
        target: Arc::downgrade(target),
        clip: clip.clone(),
    };
    fitted.made.push(made);
    clip
}

/// Clips made to fit skeletons other than their own.
#[derive(Default)]
struct Fitted {
    made: Vec<Made>,
}

struct Made {
    source: Weak<ModelAnimation>,
    index: usize,
    target: Weak<ModelAnimation>,
    clip: Arc<Clip>,
}

/// `clip`, made for `source`, made to play on `target`. A bone that sits
/// where it does in the clip's skeleton, to a millimetre, moves as the clip
/// moves it, as a gun's bolt or a carried gun's bone keyed in another file
/// of the same rig.
pub fn retarget(source: &Skeleton, clip: &Clip, target: &Skeleton) -> Clip {
    let rest = target.rest();
    let from: Vec<Option<usize>> = target.bones.iter().map(|bone| source.find(&bone.name)).collect();
    let source_rest = source.rest();
    let alike: Vec<bool> = from
        .iter()
        .zip(&rest)
        .map(|(from, at_rest)| {
            from.is_some_and(|from| source_rest[from].translation.distance(at_rest.translation) < 1e-3)
        })
        .collect();
    let (hips, root) =
        (named(target, &["hips", "pelvis"]), target.bones.iter().position(|b| b.name.eq_ignore_ascii_case("root")));
    let scale = height(target).zip(height(source)).filter(|(_, was)| *was > 1e-3).map_or(1.0, |(now, was)| now / was);
    let mut poses = Vec::with_capacity(clip.frames * rest.len());
    for frame in 0..clip.frames {
        let posed = clip.frame(frame);
        poses.extend(rest.iter().enumerate().map(|(bone, at_rest)| {
            let Some(pose) = from[bone].and_then(|source| posed.get(source)) else { return *at_rest };
            let travels = Some(bone) == hips || Some(bone) == root;
            let translation = match (travels, alike[bone]) {
                (true, _) => pose.translation * scale,
                (false, true) => pose.translation,
                (false, false) => at_rest.translation,
            };
            BonePose { translation, rotation: pose.rotation, scale: at_rest.scale }
        }));
    }
    Clip { name: clip.name.clone(), frames: clip.frames, poses }
}

/// The first bone whose name holds any of `parts`, ignoring case.
fn named(skeleton: &Skeleton, parts: &[&str]) -> Option<usize> {
    skeleton.bones.iter().position(|bone| parts.iter().any(|part| bone.name.to_lowercase().contains(part)))
}

/// How high a skeleton's hips stand at rest, in the model's space.
fn height(skeleton: &Skeleton) -> Option<f32> {
    let hips = named(skeleton, &["hips", "pelvis"])?;
    Some(skeleton.model_matrices(&skeleton.rest())[hips].w_axis.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::{Quat, Vec3};
    use voxear_render::Bone;

    /// A root, hips at `height`, and a leg of two bones under the hips,
    /// each `length` long.
    fn legged(height: f32, length: f32) -> Skeleton {
        let at = |translation| BonePose { translation, ..BonePose::default() };
        let bone = |name: &str, parent, rest| Bone { name: name.into(), parent, rest };
        Skeleton {
            bones: vec![
                bone("root", None, at(Vec3::ZERO)),
                bone("hips", Some(0), at(Vec3::Y * height)),
                bone("thigh", Some(1), at(Vec3::ZERO)),
                bone("shin", Some(2), at(Vec3::NEG_Y * length)),
            ],
        }
    }

    #[test]
    fn a_clip_plays_on_another_body_of_the_same_rig() {
        let source = legged(1.0, 0.5);
        let target = legged(0.8, 0.4);
        let mut frame = source.rest();
        frame[0].translation = Vec3::new(0.0, 0.0, 2.0);
        frame[1].translation = Vec3::new(0.0, 0.9, 0.0);
        frame[2].rotation = Quat::from_rotation_x(0.5);
        let clip = Clip { name: "Step".into(), frames: 1, poses: frame };
        let fitted = retarget(&source, &clip, &target);
        let pose = fitted.frame(0);
        assert!(
            (pose[0].translation.z - 1.6).abs() < 1e-5,
            "travel scaled to the shorter body: {}",
            pose[0].translation
        );
        assert!((pose[1].translation.y - 0.72).abs() < 1e-5, "hips scaled too: {}", pose[1].translation);
        assert_eq!(pose[2].rotation, Quat::from_rotation_x(0.5), "the thigh turns as the clip turns it");
        assert_eq!(pose[3].translation, Vec3::NEG_Y * 0.4, "the shin keeps the body's own length");
    }

    #[test]
    fn bones_the_clip_lacks_stay_at_rest() {
        let source = legged(1.0, 0.5);
        let mut target = legged(1.0, 0.5);
        let rest = BonePose { rotation: Quat::from_rotation_z(0.3), ..BonePose::default() };
        target.bones.push(Bone { name: "tail".into(), parent: Some(1), rest });
        let clip = Clip { name: "Still".into(), frames: 1, poses: source.rest() };
        assert_eq!(retarget(&source, &clip, &target).frame(0)[4], rest);
    }

    /// A gun's bolt keyed in a clip file of the gun's own rig slides as the
    /// clip slides it, though the clip is not from the gun's own file.
    #[test]
    fn a_bone_where_the_clips_is_moves_as_the_clip_moves_it() {
        let gun = |bolt: Vec3| Skeleton {
            bones: vec![
                Bone { name: "Root".into(), parent: None, rest: BonePose::default() },
                Bone {
                    name: "bolt".into(),
                    parent: Some(0),
                    rest: BonePose { translation: bolt, ..BonePose::default() },
                },
            ],
        };
        let source = gun(Vec3::new(0.0, 0.045, 0.04));
        let mut frame = source.rest();
        frame[1].translation.z -= 0.08;
        let clip = Clip { name: "Cycle".into(), frames: 1, poses: frame };
        let pose = retarget(&source, &clip, &gun(Vec3::new(0.0, 0.0453, 0.04))).frame(0).to_vec();
        assert!((pose[1].translation.z + 0.04).abs() < 1e-6, "the bolt slides back: {}", pose[1].translation);
        let pose = retarget(&source, &clip, &gun(Vec3::new(0.0, 0.06, 0.04))).frame(0).to_vec();
        assert_eq!(pose[1].translation, Vec3::new(0.0, 0.06, 0.04), "one built otherwise keeps its own place");
    }
}
