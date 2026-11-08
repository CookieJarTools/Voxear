//! Hands on what they hold, with no clips. A hand is put by its palm: the
//! middle of the palm, on its skin, goes where it holds, the hand turned
//! as the socket says. Then its fingers close from straight, every joint
//! at once, and a link that would go into the gun stops there, with the
//! joints before it, while those after it close on: so fingers wrap a
//! grip of any size and never pass through it. A grip says how far each
//! finger may curl at most, thumb first.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};

use super::contact::{Shape, nearest, push_out};
use crate::animation::ik::{PoseEdit, aim_hand, aim_turn, hand_axes, reach_hand};
use crate::animation::rig::Arm;

/// How far a finger's joints bend at a full curl, from its root out, in
/// degrees, and the thumb's.
const FINGER_BEND: [f32; 3] = [75.0, 95.0, 60.0];
const THUMB_BEND: [f32; 3] = [25.0, 35.0, 40.0];

/// How many steps fingers close in, each joint its share of the way.
const STEPS: usize = 24;

/// How long a finger takes to bend half way to a new bend, in seconds.
const BENDING: f32 = 0.05;

/// A hand's grip: how far each finger curls at most, from 0 straight to 1
/// a fist, thumb first. Written as the name of one of its presets, or as
/// five curls.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "GripText", into = "GripText")]
pub struct Grip(pub [f32; 5]);

impl Grip {
    /// Round a pistol grip, the index finger along the trigger.
    pub const TRIGGER: Grip = Grip([0.55, 0.35, 0.85, 0.88, 0.9]);
    /// Round a handguard or fore-end from below.
    pub const SUPPORT: Grip = Grip([0.2, 0.7, 0.75, 0.78, 0.8]);
    /// Pinching something small, as a bolt's knob or a round.
    pub const PINCH: Grip = Grip([0.6, 0.6, 0.85, 0.9, 0.9]);
    /// Open, as when reaching.
    pub const OPEN: Grip = Grip([0.1, 0.1, 0.12, 0.14, 0.16]);

    /// `t` of the way to `other`.
    pub fn lerp(self, other: Grip, t: f32) -> Grip {
        Grip(std::array::from_fn(|i| self.0[i] + (other.0[i] - self.0[i]) * t))
    }

    /// The presets, as they are written.
    const NAMED: [(GripName, Grip); 4] = [
        (GripName::Trigger, Grip::TRIGGER),
        (GripName::Support, Grip::SUPPORT),
        (GripName::Pinch, Grip::PINCH),
        (GripName::Open, Grip::OPEN),
    ];
}

/// A grip as written: a preset's name, or five curls.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
enum GripText {
    Named(GripName),
    Curls { curl: [f32; 5] },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GripName {
    Trigger,
    Support,
    Pinch,
    Open,
}

impl From<GripText> for Grip {
    fn from(text: GripText) -> Self {
        match text {
            GripText::Named(name) => {
                Grip::NAMED.iter().find(|(named, _)| *named == name).map_or(Grip::PINCH, |(_, grip)| *grip)
            }
            GripText::Curls { curl } => Grip(curl),
        }
    }
}

impl From<Grip> for GripText {
    fn from(grip: Grip) -> Self {
        match Grip::NAMED.iter().find(|(_, named)| *named == grip) {
            Some((name, _)) => GripText::Named(*name),
            None => GripText::Curls { curl: grip.0 },
        }
    }
}

/// Where a hand touches what it holds, in the model's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palm {
    /// The middle of the palm, on its skin.
    pub point: Vec3,
    /// The way the palm faces.
    pub facing: Vec3,
    /// How far a finger's skin is from its bones.
    pub finger: f32,
}

/// A hand's palm as it is posed now, from its bones: a little past the
/// middle of the way from the wrist to the knuckles, out on the palm's
/// side by the hand's thickness, which goes with the width of its
/// knuckles.
pub fn palm_of(edit: &PoseEdit, arm: &Arm) -> Option<Palm> {
    let (along, facing) = hand_axes(edit, arm)?;
    let wrist = edit.position(arm.hand);
    let knuckle = |finger: usize| arm.fingers[finger].first().map(|&bone| edit.position(bone));
    let middle = knuckle(2).or_else(|| knuckle(1))?;
    let span = knuckle(1)?.distance(knuckle(4).or_else(|| knuckle(3))?);
    let reach = (middle - wrist).dot(along);
    Some(Palm { point: wrist + along * reach * 0.6 + facing * span * 0.22, facing, finger: span * 0.16 })
}

/// How far an arm reaches from its shoulder to the middle of its palm,
/// held out straight, from a pose of it at rest.
pub fn arm_reach(rest: &PoseEdit, arm: &Arm) -> Option<f32> {
    let (shoulder, elbow, wrist) = (rest.position(arm.upper), rest.position(arm.forearm), rest.position(arm.hand));
    let palm = palm_of(rest, arm)?.point;
    Some(shoulder.distance(elbow) + elbow.distance(wrist) + wrist.distance(palm))
}

/// Puts a hand on what it holds: turned so its fingers point along the
/// first of `axes` and its palm faces the second, with the middle of its
/// palm at `contact`, its elbow bending toward `pole` and its collarbone
/// taking `clavicle` of a long reach. Gives back how far short it fell.
pub fn seat_hand(edit: &mut PoseEdit, arm: &Arm, contact: Vec3, axes: (Vec3, Vec3), pole: Vec3, clavicle: f32) -> f32 {
    // Where the palm is from the wrist once the hand is turned so.
    let offset = match (aim_turn(edit, arm, axes.0, axes.1), palm_of(edit, arm)) {
        (Some(turn), Some(palm)) => turn * (palm.point - edit.position(arm.hand)),
        _ => Vec3::ZERO,
    };
    let short = reach_hand(edit, arm, contact - offset, pole, clavicle);
    aim_hand(edit, arm, axes.0, axes.1);
    short
}

/// How far a hand must move for it to be clear of `shapes`, in the
/// model's space, staying on the gun's `side` it is on: its body, from the
/// wrist to the knuckles and across them, as thick as it is, and its
/// forearm just behind the wrist. Its fingers close round what they touch
/// instead.
pub fn keep_out(edit: &PoseEdit, arm: &Arm, shapes: &[Shape], side: Vec3) -> Vec3 {
    let (Some(palm), Some((along, facing))) = (palm_of(edit, arm), hand_axes(edit, arm)) else { return Vec3::ZERO };
    let wrist = edit.position(arm.hand);
    let Some(&middle) = arm.fingers[2].first().or_else(|| arm.fingers[1].first()) else { return Vec3::ZERO };
    let (length, span) = ((edit.position(middle) - wrist).dot(along), palm.finger / 0.16);
    let (across, thick) = (along.cross(facing), (palm.point - wrist).dot(facing));
    let body = [0.15, 0.5, 0.85]
        .into_iter()
        .flat_map(|a| [-0.35, 0.0, 0.35].map(|c| (wrist + along * length * a + across * span * c, thick)));
    let back = (wrist - edit.position(arm.forearm)).normalize_or_zero();
    let forearm = [0.05, 0.1].map(|d| (wrist - back * d, span * 0.3));
    body.chain(forearm)
        .map(|(point, room)| push_out(shapes, point, room, side))
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap_or(Vec3::ZERO)
}

/// Closes an arm's fingers by a grip, from straight, round `shapes` in
/// the model's space; with none they curl all the way the grip says. They
/// bend from how they were, `was`, toward that over `delta` seconds, but
/// never further in than a joint that touches.
pub fn close(
    edit: &mut PoseEdit,
    arm: &Arm,
    grip: Grip,
    shapes: &[Shape],
    was: &mut Option<[[f32; 3]; 5]>,
    delta: f32,
) {
    close_by(edit, arm, bends_of(grip), shapes, was, delta);
}

/// How far a grip lets each finger's joints bend from rest, in radians,
/// thumb first.
pub fn bends_of(grip: Grip) -> [[f32; 3]; 5] {
    std::array::from_fn(|finger| {
        let bends = if finger == 0 { THUMB_BEND } else { FINGER_BEND };
        bends.map(|bend| (bend * grip.0[finger].clamp(0.0, 1.0)).to_radians())
    })
}

/// Closes an arm's fingers as [`close`] does, each joint bending at most
/// `most` from how it stands at rest, in radians, thumb first: as a clip's
/// fingers are bent (see [`bends`]).
pub fn close_by(
    edit: &mut PoseEdit,
    arm: &Arm,
    most: [[f32; 3]; 5],
    shapes: &[Shape],
    was: &mut Option<[[f32; 3]; 5]>,
    delta: f32,
) {
    for bones in &arm.fingers {
        for &bone in bones.iter().take(3) {
            edit.rest(bone);
        }
    }
    let Some(palm) = palm_of(edit, arm) else { return };
    for (finger, bones) in arm.fingers.iter().enumerate() {
        let thickness = if finger == 0 { palm.finger * 1.15 } else { palm.finger };
        let Some(shape) = Finger::of(edit, bones, palm.facing) else { continue };
        let (wanted, touching) = shape.close(most[finger], shapes, thickness);
        let shown = was.get_or_insert([[0.0; 3]; 5]);
        let fade = 0.5f32.powf(delta / BENDING);
        for joint in 0..3 {
            let eased = wanted[joint] + (shown[finger][joint] - wanted[joint]) * fade;
            shown[finger][joint] = if touching[joint] { eased.min(wanted[joint]) } else { eased };
        }
        shape.bend(edit, bones, shown[finger]);
    }
}

/// How far each of a posed hand's fingers' first three joints is bent
/// toward the palm, in radians, thumb first: each joint's turn from the
/// line before it, the first's from the way the fingers point. Taken from
/// a clip's hand, less a hand's own at rest, it is what [`close_by`] bends
/// that hand's fingers by, so a hand of another build, or that stands
/// otherwise at rest, curls as the clip's does.
pub fn finger_bends(edit: &PoseEdit, arm: &Arm) -> [[f32; 3]; 5] {
    let Some((along, palm)) = hand_axes(edit, arm) else { return [[0.0; 3]; 5] };
    std::array::from_fn(|finger| {
        let Some(shape) = Finger::of(edit, &arm.fingers[finger], palm) else { return [0.0; 3] };
        let mut before = along;
        std::array::from_fn(|joint| {
            if joint >= shape.joints {
                return 0.0;
            }
            let link = shape.points[joint + 1] - shape.points[joint];
            let axis = before.cross(palm).normalize_or_zero();
            let angle = before.cross(link).dot(axis).atan2(before.dot(link));
            before = link;
            angle
        })
    })
}

/// A finger as it stands straight: its joints, from the knuckle out, and
/// its tip, and the axis each joint bends about toward the palm.
struct Finger {
    joints: usize,
    points: [Vec3; 4],
    axes: [Vec3; 3],
}

impl Finger {
    fn of(edit: &PoseEdit, bones: &[usize], palm: Vec3) -> Option<Finger> {
        let joints = bones.len().min(3);
        if joints < 2 {
            return None;
        }
        let mut points = [Vec3::ZERO; 4];
        for (point, &bone) in points.iter_mut().zip(bones) {
            *point = edit.position(bone);
        }
        // The tip: the bone past the last joint, or as far on again as most of the link before.
        points[joints] = match bones.get(joints) {
            Some(&tip) => edit.position(tip),
            None => points[joints - 1] + (points[joints - 1] - points[joints - 2]) * 0.8,
        };
        let mut axes = [Vec3::X; 3];
        for (joint, axis) in axes.iter_mut().enumerate().take(joints) {
            *axis = (points[joint + 1] - points[joint]).cross(palm).try_normalize()?;
        }
        Some(Finger { joints, points, axes })
    }

    /// Where the joints and tip are with the joints bent by `angles`.
    fn bent(&self, angles: [f32; 3]) -> [Vec3; 4] {
        let mut out = self.points;
        let mut turn = Quat::IDENTITY;
        for joint in 0..self.joints {
            turn *= Quat::from_axis_angle(self.axes[joint], angles[joint]);
            out[joint + 1] = out[joint] + turn * (self.points[joint + 1] - self.points[joint]);
        }
        out
    }

    /// The first link that would be in a shape with the joints bent so.
    fn touching(&self, angles: [f32; 3], shapes: &[Shape], thickness: f32) -> Option<usize> {
        let points = self.bent(angles);
        (0..self.joints).find(|&link| {
            let (from, to) = (points[link], points[link + 1]);
            [0.4, 0.7, 1.0].iter().any(|&t| nearest(shapes, from.lerp(to, t)) < thickness)
        })
    }

    /// How far each joint bends, closing on `shapes` no further than
    /// `most`, and which were stopped by touching one.
    fn close(&self, most: [f32; 3], shapes: &[Shape], thickness: f32) -> ([f32; 3], [bool; 3]) {
        let mut angles = [0.0; 3];
        let mut stopped = [false; 3];
        for step in 1..=STEPS {
            let share = step as f32 / STEPS as f32;
            let mut trial = angles;
            for joint in (0..self.joints).filter(|&joint| !stopped[joint]) {
                trial[joint] = most[joint] * share;
            }
            // A link that would go in stops, with the joints before it, where they were; the rest close on.
            while let Some(link) = self.touching(trial, shapes, thickness) {
                let moved = (0..=link).any(|joint| trial[joint] != angles[joint]);
                for joint in 0..=link {
                    (trial[joint], stopped[joint]) = (angles[joint], true);
                }
                if !moved {
                    break;
                }
            }
            angles = trial;
        }
        (angles, stopped)
    }

    /// Bends the finger's bones by `angles`, each about its axis as the
    /// joints before it carried it.
    fn bend(&self, edit: &mut PoseEdit, bones: &[usize], angles: [f32; 3]) {
        let mut turn = Quat::IDENTITY;
        for joint in 0..self.joints {
            edit.turn(bones[joint], Quat::from_axis_angle(turn * self.axes[joint], angles[joint]));
            turn *= Quat::from_axis_angle(self.axes[joint], angles[joint]);
        }
    }
}
