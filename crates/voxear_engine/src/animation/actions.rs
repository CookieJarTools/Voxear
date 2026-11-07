//! Actions: a Motion Set's clips played once over a state's, as swinging a
//! wand, jumping or dying. Each fades in, plays, and near its end fades out,
//! unless it loops or holds its last frame; on the part of the body it is
//! for, its clip takes the place of the state's, bone by bone, by how far
//! it has faded in. While it plays it may stand the character still, carry
//! it as its clip travels, or lunge it ahead, and the body may stop turning.
//! The marks it passes are told to scripts.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_render::{BonePose, Clip, Skeleton};

use super::{
    motion_database::{Ground, side_of},
    motion_set::{ActionMovement, BodyPart, MotionAction},
};

/// An action of a set's playing: which, its clip on the character's
/// skeleton, how much of each bone it takes, how far in and how far faded
/// in it is, and whether it is fading out.
#[derive(Clone)]
pub struct ActionPlay {
    pub index: usize,
    pub clip: Arc<Clip>,
    pub mask: Arc<[f32]>,
    pub time: f32,
    pub weight: f32,
    pub ending: bool,
    pub looping: bool,
}

/// What a game asks of a Motion Matcher's actions.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionCommand {
    /// Play the set's action of this name, in place of any playing.
    Play(String),
    /// Fade out the action playing.
    Stop,
}

/// The marks actions passed this frame: whose, and which, as
/// `<set>#mark:<name>`; for scripts to be told of.
#[derive(Default)]
pub struct MotionMarks(pub Vec<(Entity, String)>);

impl ActionPlay {
    /// Moves the action on by `delta`: fades it in, or out once it ends or
    /// is stopped, and says which of its marks it passed. False once it has
    /// faded out.
    pub fn advance(&mut self, action: &MotionAction, wanting_to_move: bool, delta: f32) -> (bool, Vec<String>) {
        let duration = self.clip.duration();
        let was = self.time;
        self.time += delta;
        if action.looping && duration > 0.0 {
            self.time = self.time.rem_euclid(duration);
        } else {
            self.time = self.time.min(duration);
        }
        let ends = !action.looping && !action.hold_end && self.time >= duration - action.blend_out;
        let cancelled = action.moving_cancels && wanting_to_move && self.time >= action.cancel_from;
        self.ending |= ends || cancelled;
        let rate = |seconds: f32| if seconds > 1e-4 { delta / seconds } else { 1.0 };
        self.weight = match self.ending {
            true => self.weight - rate(action.blend_out),
            false => (self.weight + rate(action.blend_in)).min(1.0),
        };
        let wrapped = action.looping && self.time < was;
        let passed = action
            .marks
            .iter()
            .filter(|mark| match wrapped {
                true => mark.time > was || mark.time <= self.time,
                false => (mark.time > was || (was == 0.0 && mark.time == 0.0)) && mark.time <= self.time,
            })
            .map(|mark| mark.name.clone())
            .collect();
        (self.weight > 0.0, passed)
    }

    /// Lays the action's pose over `pose`, each bone by its share of the
    /// mask and how far the action has faded in; its root kept where the
    /// state's is, as the action's travel moves the Obj instead.
    pub fn lay_over(&self, pose: &mut [BonePose], rest: &[BonePose], root: usize) {
        let mut over = rest.to_vec();
        self.clip.sample(self.time, self.looping, &mut over);
        over[root] = pose[root];
        let weight = smooth(self.weight);
        for ((bone, over), share) in pose.iter_mut().zip(&over).zip(self.mask.iter()) {
            let take = share * weight;
            if take > 1e-4 {
                *bone = bone.lerp(over, take);
            }
        }
    }

    /// How far the action's legs take over from the state's, as its mask
    /// has them and it has faded in: feet are held down only by the state's.
    pub fn legs_taken(&self, ankles: [usize; 2]) -> f32 {
        ankles.map(|ankle| self.mask.get(ankle).copied().unwrap_or(0.0)).into_iter().fold(0.0, f32::max) * self.weight
    }

    /// How the action moves the character from `was` seconds in to now, in
    /// metres a second along the ground, as seen from the body: its clip's
    /// root travel with root motion, its lunge with a lunge.
    pub fn carried(&self, action: &MotionAction, skeleton: &Skeleton, root: usize, rest_root: Quat, was: f32) -> Vec3 {
        let delta = self.time - was;
        if delta <= 1e-5 {
            return Vec3::ZERO;
        }
        match action.movement {
            ActionMovement::Root => {
                let ground = |time: f32| {
                    let mut pose = skeleton.rest();
                    self.clip.sample(time, action.looping, &mut pose);
                    Ground::of(skeleton.model_matrices(&pose)[root], rest_root)
                };
                ground(was).to(ground(self.time)).position / delta
            }
            ActionMovement::Lunge if action.lunge_length > 1e-4 => {
                let along = |time: f32| smooth((time - action.lunge_start) / action.lunge_length);
                Vec3::Z * action.lunge_distance * (along(self.time) - along(was)) / delta
            }
            _ => Vec3::ZERO,
        }
    }
}

/// How much of each bone an action on `body` takes: 1 for the bones of the
/// part, 0 for the rest, found by the bones' names.
pub fn mask(skeleton: &Skeleton, spine: &[usize], body: BodyPart, bone: &str) -> Vec<f32> {
    let count = skeleton.bones.len();
    let starts: Vec<usize> = match body {
        BodyPart::Full => return vec![1.0; count],
        BodyPart::Upper => spine.first().copied().into_iter().collect(),
        BodyPart::Arms => [arm(skeleton, true), arm(skeleton, false)].into_iter().flatten().collect(),
        BodyPart::LeftArm => arm(skeleton, true).into_iter().collect(),
        BodyPart::RightArm => arm(skeleton, false).into_iter().collect(),
        BodyPart::Bone => skeleton.find(bone).into_iter().collect(),
    };
    // Parents come before their children, so one pass finds every bone under the starts.
    let mut taken = vec![0.0; count];
    for index in 0..count {
        let under = skeleton.bones[index].parent.is_some_and(|parent| taken[parent] > 0.0);
        if starts.contains(&index) || under {
            taken[index] = 1.0;
        }
    }
    taken
}

/// Where an arm starts: its shoulder or collarbone, or else its upper arm.
fn arm(skeleton: &Skeleton, left: bool) -> Option<usize> {
    let names: Vec<String> = skeleton.bones.iter().map(|bone| bone.name.to_lowercase()).collect();
    let named = |part: &str| names.iter().position(|name| name.contains(part) && side_of(name) == Some(left));
    named("shoulder").or_else(|| named("clavicle")).or_else(|| named("upper_arm")).or_else(|| named("upperarm"))
}

/// Eases 0 to 1 in and out.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::motion_set::ActionMark;
    use voxear_render::{Bone, CLIP_RATE};

    /// How many frames a clip `seconds` long has.
    fn frames(seconds: f32) -> usize {
        (seconds * CLIP_RATE).round() as usize + 1
    }

    /// Hips, a spine with a shoulder and arm on it, and a leg.
    fn body() -> Skeleton {
        let rest = BonePose::default();
        let bone = |name: &str, parent| Bone { name: name.into(), parent, rest };
        Skeleton {
            bones: vec![
                bone("root", None),
                bone("hips", Some(0)),
                bone("spine", Some(1)),
                bone("shoulder.L", Some(2)),
                bone("arm.L", Some(3)),
                bone("thigh.L", Some(1)),
            ],
        }
    }

    #[test]
    fn a_part_of_the_body_is_its_bones_and_those_under_them() {
        let skeleton = body();
        assert_eq!(mask(&skeleton, &[2], BodyPart::Upper, ""), [0.0, 0.0, 1.0, 1.0, 1.0, 0.0]);
        assert_eq!(mask(&skeleton, &[2], BodyPart::LeftArm, ""), [0.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
        assert_eq!(mask(&skeleton, &[2], BodyPart::RightArm, ""), [0.0; 6], "it has no right arm");
        assert_eq!(mask(&skeleton, &[2], BodyPart::Bone, "thigh.L"), [0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn an_action_fades_in_passes_its_marks_once_and_fades_out_at_its_end() {
        let clip = Clip { name: "Cast".into(), frames: frames(1.0), poses: vec![BonePose::default(); frames(1.0)] };
        let action = MotionAction {
            blend_in: 0.2,
            blend_out: 0.2,
            marks: vec![ActionMark { name: "Release".into(), time: 0.5 }],
            ..MotionAction::default()
        };
        let mut play = ActionPlay {
            index: 0,
            clip: Arc::new(clip),
            mask: vec![1.0].into(),
            time: 0.0,
            weight: 0.0,
            ending: false,
            looping: false,
        };
        let mut passed = Vec::new();
        let mut alive = true;
        let mut frames_on = 0;
        while alive && frames_on < 200 {
            let (still, marks) = play.advance(&action, false, 1.0 / 60.0);
            alive = still;
            passed.extend(marks);
            frames_on += 1;
            if frames_on == 12 {
                assert!((play.weight - 1.0).abs() < 1e-5, "faded in after its blend: {}", play.weight);
            }
        }
        assert_eq!(passed, ["Release"], "each mark once");
        assert!(!alive && frames_on <= 61, "faded out by its end: {frames_on}");

        let held = MotionAction { hold_end: true, ..action.clone() };
        let mut play = ActionPlay { time: 0.0, weight: 0.0, ending: false, ..play };
        for _ in 0..200 {
            assert!(play.advance(&held, false, 1.0 / 60.0).0, "held at its end");
        }
        assert_eq!(play.time, 1.0);
    }
}
