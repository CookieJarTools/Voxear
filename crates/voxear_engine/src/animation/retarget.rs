//! Retargeting: a clip made for one model's skeleton plays on another's.
//!
//! On the same rig, it goes by name, bone by bone. Each bone the two share
//! by name turns as the clip turns it; it keeps the character's own length,
//! but for the root and hips, whose travel and height the clip gives,
//! scaled by how much taller or shorter the character stands, and for bones
//! that sit where the clip's do, which move as the clip moves them. Bones
//! the clip does not have stay at rest.
//!
//! Between humanoids whose rigs name their bones otherwise, as
//! `DEF-upper_arm.L` and `upperarm_l`, it goes by their parts (see `Rig`):
//! hips, spine, neck and head, collarbones, arms, hands and fingers, legs,
//! feet and toes. Each bone turns in the model's space as its part does in
//! the clip, from how it stands at rest, so rigs whose bones point their
//! own axes otherwise move alike; and it is first turned to point at rest
//! as the clip's does, so a rig standing with its arms down plays a clip
//! made with them out. The root and hips travel as by name.

use std::sync::{Arc, Weak};

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{BonePose, Clip, ModelAnimation, Skeleton};

use super::clips::ClipSource;
use super::rig::Rig;

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

/// `clip`, made for `source`, made to play on `target`: by their humanoids'
/// parts when the two rigs name them otherwise, else by name. By name, a
/// bone that sits where it does in the clip's skeleton, to a millimetre,
/// moves as the clip moves it, as a gun's bolt or a carried gun's bone
/// keyed in another file of the same rig.
pub fn retarget(source: &Skeleton, clip: &Clip, target: &Skeleton) -> Clip {
    if let Some(pairs) = part_pairs(source, target) {
        return by_parts(source, clip, target, &pairs);
    }
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

/// A bone of the target played by one of the clip's, and the next bone
/// along each, which says which way it points.
struct Pair {
    target: usize,
    source: usize,
    toward: Option<(usize, usize)>,
}

/// The bones of `target` each bone of `source` plays, by their parts, for
/// two humanoids whose rigs name their hips, head and hands otherwise; none
/// for rigs that share those names, which go by name, or for a skeleton
/// that is not a humanoid's. Bones beyond the rig's that share a name, as
/// a root, pair by it.
fn part_pairs(source: &Skeleton, target: &Skeleton) -> Option<Vec<Pair>> {
    let (from, to) = (Rig::find(source)?, Rig::find(target)?);
    let key = [to.hips, to.head, to.arms[0].hand, to.arms[1].hand];
    if key.iter().all(|&bone| source.find(&target.bones[bone].name).is_some()) {
        return None;
    }
    // Chains of bones, (target, source), each pointing at the next.
    let mut chains: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut body = vec![(to.hips, from.hips)];
    let spine = |at: usize| {
        let (n, m) = (to.spine.len(), from.spine.len());
        let along = if n < 2 { m.saturating_sub(1) } else { (at * m.saturating_sub(1) + (n - 1) / 2) / (n - 1) };
        from.spine.get(along).copied()
    };
    body.extend(to.spine.iter().enumerate().filter_map(|(at, &bone)| Some((bone, spine(at)?))));
    if let (Some(neck), Some(theirs)) = (to.neck, from.neck) {
        body.push((neck, theirs));
    }
    body.push((to.head, from.head));
    chains.push(body);
    for (mine, theirs) in to.arms.iter().zip(&from.arms) {
        let mut arm = Vec::new();
        if let (Some(clavicle), Some(theirs)) = (mine.clavicle, theirs.clavicle) {
            arm.push((clavicle, theirs));
        }
        arm.extend([(mine.upper, theirs.upper), (mine.forearm, theirs.forearm), (mine.hand, theirs.hand)]);
        // The hand points at its middle knuckle, or else its index.
        let knuckle = |arm: &super::rig::Arm| arm.fingers[2].first().or_else(|| arm.fingers[1].first()).copied();
        if let (Some(knuckle), Some(theirs)) = (knuckle(mine), knuckle(theirs)) {
            arm.push((knuckle, theirs));
        }
        chains.push(arm);
        for (finger, theirs) in mine.fingers.iter().zip(&theirs.fingers) {
            chains.push(finger.iter().zip(theirs).take(3).map(|(&bone, &theirs)| (bone, theirs)).collect());
        }
    }
    if let (Some(mine), Some(theirs)) = (&to.legs, &from.legs) {
        for (mine, theirs) in mine.iter().zip(theirs) {
            let mut leg = vec![(mine.thigh, theirs.thigh), (mine.shin, theirs.shin), (mine.foot, theirs.foot)];
            if let (Some(toe), Some(theirs)) = (mine.toe, theirs.toe) {
                leg.push((toe, theirs));
            }
            chains.push(leg);
        }
    }
    let mut pairs: Vec<Pair> = Vec::new();
    for chain in &chains {
        for (at, &(target, source)) in chain.iter().enumerate() {
            let toward = chain.get(at + 1).copied();
            match pairs.iter_mut().find(|pair| pair.target == target) {
                Some(pair) => pair.toward = pair.toward.or(toward),
                None => pairs.push(Pair { target, source, toward }),
            }
        }
    }
    for (bone, named) in target.bones.iter().enumerate() {
        let theirs = source.find(&named.name).filter(|&theirs| pairs.iter().all(|pair| pair.source != theirs));
        if let Some(source) = theirs.filter(|_| pairs.iter().all(|pair| pair.target != bone)) {
            pairs.push(Pair { target: bone, source, toward: None });
        }
    }
    Some(pairs)
}

/// `clip` played on `target` by its parts: each paired bone turned in the
/// model's space as its pair turns from rest in the clip, after a turn that
/// points it at rest as its pair points; each bone's own length, but the
/// root's and hips' travel, scaled as by name; and a bone with no pair as
/// it stands at rest under its parent.
fn by_parts(source: &Skeleton, clip: &Clip, target: &Skeleton, pairs: &[Pair]) -> Clip {
    let (source_rest, target_rest) = (source.model_matrices(&source.rest()), target.model_matrices(&target.rest()));
    let turn = |m: &Mat4| m.to_scale_rotation_translation().1;
    let at = |m: &Mat4| m.w_axis.truncate();
    let mut from: Vec<Option<(usize, Quat)>> = vec![None; target.bones.len()];
    for pair in pairs {
        let lined = pair.toward.map_or(Quat::IDENTITY, |(next, theirs)| {
            let mine = (at(&target_rest[next]) - at(&target_rest[pair.target])).normalize_or_zero();
            let clips = (at(&source_rest[theirs]) - at(&source_rest[pair.source])).normalize_or_zero();
            if mine == Vec3::ZERO || clips == Vec3::ZERO {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_arc(mine, clips)
            }
        });
        from[pair.target] = Some((pair.source, lined * turn(&target_rest[pair.target])));
    }
    let hips = Rig::find(target).map(|rig| rig.hips);
    let root = target.bones.iter().position(|bone| bone.name.eq_ignore_ascii_case("root"));
    let scale = height(target).zip(height(source)).filter(|(_, was)| *was > 1e-3).map_or(1.0, |(now, was)| now / was);
    let rest = target.rest();
    let mut poses = Vec::with_capacity(clip.frames * rest.len());
    for frame in 0..clip.frames {
        let posed = source.model_matrices(clip.frame(frame));
        let mut placed: Vec<Mat4> = Vec::with_capacity(rest.len());
        for (bone, named) in target.bones.iter().enumerate() {
            let parent = named.parent.map_or(Mat4::IDENTITY, |parent| placed[parent]);
            let mut local = rest[bone];
            if let Some((theirs, at_rest)) = from[bone] {
                let wanted = turn(&posed[theirs]) * turn(&source_rest[theirs]).inverse() * at_rest;
                local.rotation = (turn(&parent).inverse() * wanted).normalize();
                if Some(bone) == hips || Some(bone) == root {
                    local.translation = parent.inverse().transform_point3(at(&posed[theirs]) * scale);
                }
            }
            placed.push(parent * local.matrix());
            poses.push(local);
        }
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

    /// The same body as a source rig names it, and as another rig does with
    /// every bone's own axes turned some other way, though each stands
    /// where it did.
    fn renamed_and_turned() -> (Skeleton, Skeleton) {
        use crate::animation::rig::tests::body;
        let source = body(|part, side| match side {
            Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
            None => format!("DEF-{part}"),
        });
        let mut target = body(|part, side| match side {
            Some(left) => format!("{}_{}", part.replace('_', ""), if left { "l" } else { "r" }),
            None => part.to_string(),
        });
        let placed = target.model_matrices(&target.rest());
        let axes: Vec<Quat> = (0..target.bones.len())
            .map(|bone| Quat::from_euler(voxear_core::math::EulerRot::XYZ, bone as f32 * 0.7, bone as f32 * 1.3, -0.4))
            .collect();
        for bone in 0..target.bones.len() {
            let at = placed[bone].w_axis.truncate();
            let (parent_at, parent_axes) = target.bones[bone]
                .parent
                .map_or((Vec3::ZERO, Quat::IDENTITY), |parent| (placed[parent].w_axis.truncate(), axes[parent]));
            target.bones[bone].rest.rotation = parent_axes.inverse() * axes[bone];
            target.bones[bone].rest.translation = parent_axes.inverse() * (at - parent_at);
        }
        (source, target)
    }

    /// A humanoid's clip plays on another rig of the same build, named
    /// otherwise and with its bones' axes turned otherwise: its hands, feet
    /// and head go where the clip's do.
    #[test]
    fn a_clip_plays_by_parts_on_a_rig_named_and_turned_otherwise() {
        use crate::animation::rig::Rig;
        let (source, target) = renamed_and_turned();
        let rig = Rig::find(&source).unwrap();
        let mut frame = source.rest();
        frame[rig.arms[1].upper].rotation = Quat::from_rotation_z(-1.2);
        frame[rig.arms[0].forearm].rotation = Quat::from_rotation_y(0.9);
        let legs = rig.legs.clone().unwrap();
        frame[legs[0].thigh].rotation = Quat::from_rotation_x(-0.7);
        frame[legs[1].shin].rotation = Quat::from_rotation_x(0.8);
        frame[rig.spine[0]].rotation = Quat::from_rotation_x(0.3);
        frame[rig.hips].translation += Vec3::new(0.0, -0.1, 0.4);
        let clip = Clip { name: "Reach".into(), frames: 1, poses: frame.clone() };
        let played = retarget(&source, &clip, &target);
        let (theirs, mine) = (source.model_matrices(&frame), target.model_matrices(played.frame(0)));
        let to = Rig::find(&target).unwrap();
        let to_legs = to.legs.clone().unwrap();
        let ends = [
            (rig.arms[0].hand, to.arms[0].hand),
            (rig.arms[1].hand, to.arms[1].hand),
            (legs[0].foot, to_legs[0].foot),
            (legs[1].toe.unwrap(), to_legs[1].toe.unwrap()),
            (rig.head, to.head),
            (rig.hips, to.hips),
        ];
        for (source_bone, target_bone) in ends {
            let off = theirs[source_bone].w_axis.truncate().distance(mine[target_bone].w_axis.truncate());
            assert!(
                off < 1e-4,
                "{} is {off} m from the clip's {}",
                target.bones[target_bone].name,
                source.bones[source_bone].name
            );
        }
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
