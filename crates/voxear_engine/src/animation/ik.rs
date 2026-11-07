//! Inverse kinematics on a posed model: changes made after animation, in
//! the model's space, as a pose's matrices are. A hand reaches a point and
//! turns as it is told, its elbow bending toward a pole, its collarbone
//! taking a share of a long reach; and the upper body turns as a whole.

use voxear_core::math::{Mat3, Mat4, Quat, Vec3};
use voxear_render::{BonePose, Skeleton};

use super::rig::Arm;

/// A posed model's bones as turns in their parents' spaces, to change and
/// put back as the model's matrices.
pub struct PoseEdit<'a> {
    skeleton: &'a Skeleton,
    pub local: Vec<BonePose>,
}

impl<'a> PoseEdit<'a> {
    /// Reads a pose's matrices, in the model's space.
    pub fn new(skeleton: &'a Skeleton, matrices: &[Mat4]) -> Self {
        let local = skeleton
            .bones
            .iter()
            .enumerate()
            .map(|(index, bone)| {
                let own = matrices.get(index).copied().unwrap_or(Mat4::IDENTITY);
                let parent = bone.parent.and_then(|p| matrices.get(p)).copied().unwrap_or(Mat4::IDENTITY);
                let (scale, rotation, translation) = (parent.inverse() * own).to_scale_rotation_translation();
                BonePose { translation, rotation, scale }
            })
            .collect();
        Self { skeleton, local }
    }

    /// The bones' matrices in the model's space.
    pub fn matrices(&self) -> Vec<Mat4> {
        self.skeleton.model_matrices(&self.local)
    }

    /// Where a bone is in the model's space.
    pub fn position(&self, bone: usize) -> Vec3 {
        self.matrices()[bone].w_axis.truncate()
    }

    /// A bone's turn in the model's space.
    pub fn rotation(&self, bone: usize) -> Quat {
        self.matrices()[bone].to_scale_rotation_translation().1
    }

    /// Turns a bone by `turn` in the model's space, about where it is,
    /// carrying everything under it.
    pub fn turn(&mut self, bone: usize, turn: Quat) {
        let parent = self.parent_rotation(bone);
        let local = &mut self.local[bone].rotation;
        *local = (parent.inverse() * turn * parent * *local).normalize();
    }

    /// Sets a bone's turn in the model's space.
    pub fn set_rotation(&mut self, bone: usize, rotation: Quat) {
        let parent = self.parent_rotation(bone);
        self.local[bone].rotation = (parent.inverse() * rotation).normalize();
    }

    fn parent_rotation(&self, bone: usize) -> Quat {
        let matrices = self.matrices();
        self.skeleton.bones[bone].parent.map_or(Quat::IDENTITY, |p| matrices[p].to_scale_rotation_translation().1)
    }
}

/// Bends a limb, from `top` through `middle` to `end`, so `end` reaches
/// `target` in the model's space with the middle joint bending toward
/// `pole`. A target out of reach is reached toward; gives back how far
/// short the limb fell, 0 when it reached.
pub fn two_bone(edit: &mut PoseEdit, [top, middle, end]: [usize; 3], target: Vec3, pole: Vec3) -> f32 {
    let (a, b, c) = (edit.position(top), edit.position(middle), edit.position(end));
    let (upper, lower) = ((b - a).length(), (c - b).length());
    if upper < 1e-5 || lower < 1e-5 {
        return 0.0;
    }
    let wanted = (target - a).length();
    let span = wanted.clamp((upper - lower).abs() + 1e-4, upper + lower - 1e-4);
    let angle = |x: Vec3, y: Vec3| x.normalize_or_zero().dot(y.normalize_or_zero()).clamp(-1.0, 1.0).acos();
    let law = |near: f32, far: f32, opposite: f32| {
        ((opposite * opposite - near * near - far * far) / (-2.0 * near * far)).clamp(-1.0, 1.0).acos()
    };
    // Open or close the two joints in the plane they bend in, so the limb spans as far as the target is.
    let bend = (c - a).cross(b - a).try_normalize().unwrap_or_else(|| (b - a).any_orthonormal_vector());
    let (top_now, middle_now) = (angle(c - a, b - a), angle(a - b, c - b));
    let (top_then, middle_then) = (law(upper, span, lower), law(upper, lower, span));
    edit.turn(top, Quat::from_axis_angle(bend, top_then - top_now));
    edit.turn(middle, Quat::from_axis_angle(bend, middle_then - middle_now));
    // Swing the whole limb onto the target.
    let reach = edit.position(end) - a;
    edit.turn(top, Quat::from_rotation_arc(reach.normalize_or_zero(), (target - a).normalize_or_zero()));
    // Roll it about the line to the target, so the middle joint points at the pole.
    let line = (target - a).normalize_or_zero();
    let across = |p: Vec3| {
        let off = p - a;
        (off - line * off.dot(line)).normalize_or_zero()
    };
    let (elbow, wish) = (across(edit.position(middle)), across(pole));
    if elbow != Vec3::ZERO && wish != Vec3::ZERO {
        let roll = elbow.cross(wish).dot(line).atan2(elbow.dot(wish));
        edit.turn(top, Quat::from_axis_angle(line, roll));
    }
    (wanted - span).max(0.0)
}

/// Puts a hand's wrist at `at` in the model's space, its elbow bending
/// toward `pole`: the collarbone first takes `clavicle` of the turn toward
/// it, up to 25 degrees, then the arm bends. Gives back how far short the
/// hand fell.
pub fn reach_hand(edit: &mut PoseEdit, arm: &Arm, at: Vec3, pole: Vec3, clavicle: f32) -> f32 {
    if let Some(collar) = arm.clavicle.filter(|_| clavicle > 0.0) {
        let from = edit.position(collar);
        let (now, wish) = ((edit.position(arm.upper) - from).normalize_or_zero(), (at - from).normalize_or_zero());
        let full = Quat::from_rotation_arc(now, wish);
        let (axis, angle) = full.to_axis_angle();
        let share = (angle * clavicle.clamp(0.0, 1.0)).min(25f32.to_radians());
        if axis.is_finite() && share > 1e-4 {
            edit.turn(collar, Quat::from_axis_angle(axis, share));
        }
    }
    two_bone(edit, [arm.upper, arm.forearm, arm.hand], at, pole)
}

/// The way a hand's fingers point and its palm faces, in the model's
/// space, from its bones: toward its middle finger, and square to that and
/// to the line of its knuckles, on the side a hand of its side's palm is.
pub fn hand_axes(edit: &PoseEdit, arm: &Arm) -> Option<(Vec3, Vec3)> {
    let hand = edit.position(arm.hand);
    let knuckle = |finger: usize| arm.fingers[finger].first().map(|&bone| edit.position(bone));
    let middle = knuckle(2).or_else(|| knuckle(1))?;
    let along = (middle - hand).try_normalize()?;
    let across = match (knuckle(1), knuckle(4).or_else(|| knuckle(3))) {
        (Some(index), Some(little)) => index - little,
        _ => return None,
    };
    let across = (across - along * across.dot(along)).try_normalize()?;
    // The model's space is the engine's, left-handed: a right hand palm down, fingers forward, has its index finger
    // to the left, so its palm is along × across; a left hand's is the other way round.
    let palm = if arm.left { across.cross(along) } else { along.cross(across) };
    Some((along, palm))
}

/// Turns a hand so its fingers point along `fingers` and its palm faces
/// `palm`, in the model's space.
pub fn aim_hand(edit: &mut PoseEdit, arm: &Arm, fingers: Vec3, palm: Vec3) {
    let Some((along, facing)) = hand_axes(edit, arm) else { return };
    let frame = |a: Vec3, p: Vec3| {
        let a = a.normalize_or_zero();
        let p = (p - a * p.dot(a)).normalize_or_zero();
        Mat3::from_cols(a, p, a.cross(p))
    };
    let (now, wanted) = (frame(along, facing), frame(fingers, palm));
    if now.determinant().abs() < 0.5 || wanted.determinant().abs() < 0.5 {
        return;
    }
    let turn = Quat::from_mat3(&(wanted * now.transpose())).normalize();
    let rotation = edit.rotation(arm.hand);
    edit.set_rotation(arm.hand, turn * rotation);
}

/// Turns a run of the spine, from the bottom up, by `turn` in the model's
/// space between them, each bone its share.
pub fn turn_spine(edit: &mut PoseEdit, spine: &[usize], turn: Quat) {
    if spine.is_empty() {
        return;
    }
    let (axis, angle) = turn.to_axis_angle();
    if !axis.is_finite() || angle.abs() < 1e-5 {
        return;
    }
    let share = Quat::from_axis_angle(axis, angle / spine.len() as f32);
    for &bone in spine {
        edit.turn(bone, share);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::rig::{Rig, tests::body};

    fn rig_and_skeleton() -> (Rig, Skeleton) {
        let skeleton = body(|part, side| match side {
            Some(left) => format!("{part}.{}", if left { "L" } else { "R" }),
            None => part.to_string(),
        });
        (Rig::find(&skeleton).unwrap(), skeleton)
    }

    #[test]
    fn a_hand_reaches_a_point_with_its_elbow_toward_the_pole_and_turns_as_told() {
        let (rig, skeleton) = rig_and_skeleton();
        let rest = skeleton.model_matrices(&skeleton.rest());
        let mut edit = PoseEdit::new(&skeleton, &rest);
        let arm = &rig.arms[1];
        let target = Vec3::new(0.15, 1.35, 0.35);
        let pole = Vec3::new(0.6, 0.6, -0.2);
        let short = reach_hand(&mut edit, arm, target, pole, 0.3);
        assert!(short < 1e-4, "within reach: {short}");
        assert!(edit.position(arm.hand).distance(target) < 1e-3, "{}", edit.position(arm.hand));
        let (fingers, palm) = (Vec3::new(0.3, -0.2, 1.0).normalize(), Vec3::X);
        aim_hand(&mut edit, arm, fingers, palm);
        let (along, facing) = hand_axes(&edit, arm).unwrap();
        assert!(along.distance(fingers) < 1e-3, "{along} {fingers}");
        assert!(facing.dot(palm) > 0.9, "the palm faces as told: {facing}");
        // The elbow sits on the pole's side of the line from shoulder to hand.
        let (shoulder, elbow) = (edit.position(arm.upper), edit.position(arm.forearm));
        let line = (target - shoulder).normalize();
        let side = |p: Vec3| (p - shoulder) - line * (p - shoulder).dot(line);
        assert!(side(elbow).dot(side(pole)) > 0.0, "the elbow bends toward the pole");
        // The bones keep their lengths.
        let lengths = |edit: &PoseEdit| {
            (
                edit.position(arm.forearm).distance(edit.position(arm.upper)),
                edit.position(arm.hand).distance(edit.position(arm.forearm)),
            )
        };
        let before = lengths(&PoseEdit::new(&skeleton, &rest));
        let after = lengths(&edit);
        assert!((before.0 - after.0).abs() < 1e-4 && (before.1 - after.1).abs() < 1e-4);
    }

    #[test]
    fn a_target_out_of_reach_is_reached_toward_and_says_how_far_short() {
        let (rig, skeleton) = rig_and_skeleton();
        let mut edit = PoseEdit::new(&skeleton, &skeleton.model_matrices(&skeleton.rest()));
        let arm = &rig.arms[0];
        let target = Vec3::new(2.0, 1.4, 1.0);
        let short = reach_hand(&mut edit, arm, target, Vec3::new(1.0, 0.0, -1.0), 0.0);
        assert!(short > 1.0, "{short}");
        let toward = (target - edit.position(arm.upper)).normalize();
        let pointing = (edit.position(arm.hand) - edit.position(arm.upper)).normalize();
        assert!(toward.dot(pointing) > 0.999, "straight at it");
    }

    #[test]
    fn the_spine_turns_as_a_whole_carrying_the_head() {
        let (rig, skeleton) = rig_and_skeleton();
        let mut edit = PoseEdit::new(&skeleton, &skeleton.model_matrices(&skeleton.rest()));
        let before = edit.position(rig.head);
        turn_spine(&mut edit, &rig.spine, Quat::from_rotation_x(0.4));
        let after = edit.position(rig.head);
        let chest = edit.position(rig.chest());
        let rest = PoseEdit::new(&skeleton, &skeleton.model_matrices(&skeleton.rest()));
        let neck_length = rest.position(rig.head).distance(rest.position(rig.chest()));
        assert!((after.distance(chest) - neck_length).abs() < 1e-4, "the neck keeps its length");
        assert!(after.z > before.z + 0.05, "leaning forward: {before} {after}");
        assert!(edit.rotation(rig.head).angle_between(Quat::from_rotation_x(0.4)) < 1e-3);
    }
}

#[cfg(test)]
mod real {
    use super::*;
    use crate::animation::rig::Rig;

    /// Reaches the model at `VOXEAR_TEST_MODEL`'s right hand to a point
    /// before its chest with the elbow toward a pole below, and reports
    /// where the elbow went: for checking a real rig by hand.
    #[test]
    fn a_real_rig_bends_its_elbow_toward_the_pole() {
        let Ok(path) = std::env::var("VOXEAR_TEST_MODEL") else { return };
        let model = voxear_render::prepare_model(&path, &mut |file| Ok(std::fs::read(file)?)).unwrap();
        let animation = model.animation().cloned().expect("an animated model");
        let skeleton = &animation.skeleton;
        let rig = Rig::find(skeleton).unwrap();
        let rest = skeleton.model_matrices(&skeleton.rest());
        let mut edit = PoseEdit::new(skeleton, &rest);
        let arm = &rig.arms[1];
        let shoulder = edit.position(arm.upper);
        eprintln!("shoulder {shoulder} hand {} chest {}", edit.position(arm.hand), edit.position(rig.chest()));
        let target = shoulder + Vec3::new(-0.15, -0.15, 0.3);
        let pole = shoulder + Vec3::new(0.3, -0.6, -0.1);
        let short = reach_hand(&mut edit, arm, target, pole, 0.35);
        let elbow = edit.position(arm.forearm);
        eprintln!(
            "short {short} hand {} elbow {elbow} (shoulder {})",
            edit.position(arm.hand),
            edit.position(arm.upper)
        );
        assert!(elbow.y < edit.position(arm.upper).y - 0.1, "the elbow hangs below the shoulder: {elbow}");
    }
}
