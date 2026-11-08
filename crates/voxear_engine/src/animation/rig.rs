//! A humanoid's bones by what they are: the spine, the head, each arm with
//! its hand and fingers, and each leg with its foot, found by the names
//! most rigs give them, as `DEF-upper_arm.L`, `upperarm_l` or `LeftArm`.
//! What procedural posing needs to reach a hand somewhere or turn the upper
//! body, and what lets a clip play on a humanoid whose rig names its bones
//! otherwise (see `retarget`).

use voxear_render::Skeleton;

use super::motion_database::side_of;

/// The fingers in a hand's order, thumb first.
pub const FINGERS: [&str; 5] = ["thumb", "index", "middle", "ring", "pinky"];

/// One arm's bones.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    /// Whether it is the left arm.
    pub left: bool,
    /// The collarbone, when the rig has one.
    pub clavicle: Option<usize>,
    pub upper: usize,
    pub forearm: usize,
    pub hand: usize,
    /// Each finger's bones from the hand out, in [`FINGERS`]' order; empty
    /// for a finger the rig lacks.
    pub fingers: [Vec<usize>; 5],
}

/// One leg's bones.
#[derive(Clone, Debug, PartialEq)]
pub struct Leg {
    /// Whether it is the left leg.
    pub left: bool,
    pub thigh: usize,
    pub shin: usize,
    pub foot: usize,
    /// The toes, when the rig has them.
    pub toe: Option<usize>,
}

/// A humanoid's bones.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub hips: usize,
    /// The spine from above the hips up to the chest, in order.
    pub spine: Vec<usize>,
    pub neck: Option<usize>,
    pub head: usize,
    /// The left arm, then the right.
    pub arms: [Arm; 2],
    /// The left leg, then the right, for a rig with legs: an arms rig has
    /// none.
    pub legs: Option<[Leg; 2]>,
}

impl Rig {
    /// The rig of a skeleton, when its bones are named as a humanoid's.
    pub fn find(skeleton: &Skeleton) -> Option<Rig> {
        let names: Vec<String> = skeleton.bones.iter().map(|bone| bone.name.to_lowercase()).collect();
        let central = |part: &str| {
            names.iter().position(|name| name.contains(part) && side_of(name).is_none() && !name.ends_with("end"))
        };
        let hips = central("hips").or_else(|| central("pelvis"))?;
        let head = names.iter().position(|name| {
            name.contains("head") && side_of(name).is_none() && !name.contains("end") && !name.contains("top")
        })?;
        // The neck: the lowest of the bones named so that the head hangs from, one above another, as a rig with two
        // neck bones has; else the spine would reach up into the neck, and its top, the chest, nod with the head.
        let mut neck = None;
        let mut below = skeleton.bones[head].parent;
        while let Some(bone) = below.filter(|&bone| names[bone].contains("neck") && side_of(&names[bone]).is_none()) {
            neck = Some(bone);
            below = skeleton.bones[bone].parent;
        }
        // The spine is what lies between the hips and the neck, or the head.
        let mut spine = Vec::new();
        let mut at = skeleton.bones[neck.unwrap_or(head)].parent;
        while let Some(bone) = at.filter(|&bone| bone != hips) {
            spine.push(bone);
            at = skeleton.bones[bone].parent;
        }
        spine.reverse();
        let arms = [arm(skeleton, &names, true)?, arm(skeleton, &names, false)?];
        let legs = leg(skeleton, &names, true).zip(leg(skeleton, &names, false)).map(|(left, right)| [left, right]);
        Some(Rig { hips, spine, neck, head, arms, legs })
    }

    /// The chest: the top of the spine, where the arms hang from.
    pub fn chest(&self) -> usize {
        self.spine.last().copied().unwrap_or(self.hips)
    }
}

/// An arm, from its hand: the forearm and upper arm above it, passing over
/// any twist bones, and the collarbone above them when it is named so.
fn arm(skeleton: &Skeleton, names: &[String], left: bool) -> Option<Arm> {
    let sided = |name: &String| side_of(name) == Some(left);
    let hand = names
        .iter()
        .position(|name| sided(name) && name.contains("hand") && !FINGERS.iter().any(|finger| name.contains(finger)))?;
    let above = |bone: usize| {
        let mut at = skeleton.bones[bone].parent?;
        while names[at].contains("twist") {
            at = skeleton.bones[at].parent?;
        }
        Some(at)
    };
    let forearm = above(hand)?;
    let upper = above(forearm)?;
    let clavicle = above(upper).filter(|&bone| names[bone].contains("shoulder") || names[bone].contains("clavicle"));
    // A finger starts at its knuckle: the bones in the palm before it, where a rig has them, are not its own.
    let fingers = FINGERS.map(|finger| {
        let mut chain: Vec<usize> = (0..names.len())
            .filter(|&bone| sided(&names[bone]) && finger_named(&names[bone], finger) && under(skeleton, bone, hand))
            .filter(|&bone| !names[bone].contains("metacarpal"))
            .collect();
        chain.sort_by_key(|&bone| depth(skeleton, bone));
        chain
    });
    Some(Arm { left, clavicle, upper, forearm, hand, fingers })
}

/// A leg, from its foot: the shin and thigh above it, passing over any
/// twist bones, and the toes under it. A bone for an IK target, or the
/// end of a chain, is not the foot.
fn leg(skeleton: &Skeleton, names: &[String], left: bool) -> Option<Leg> {
    let sided = |name: &String| side_of(name) == Some(left);
    let not_a_bone = |name: &String| ["ik", "toe", "ball", "leaf", "end"].iter().any(|word| name.contains(word));
    let foot = names.iter().position(|name| sided(name) && name.contains("foot") && !not_a_bone(name))?;
    let above = |bone: usize| {
        let mut at = skeleton.bones[bone].parent?;
        while names[at].contains("twist") {
            at = skeleton.bones[at].parent?;
        }
        Some(at)
    };
    let shin = above(foot)?;
    let thigh = above(shin)?;
    let toe = (0..names.len()).find(|&bone| {
        let name = &names[bone];
        skeleton.bones[bone].parent == Some(foot)
            && (name.contains("toe") || name.contains("ball"))
            && !name.contains("leaf")
    });
    Some(Leg { left, thigh, shin, foot, toe })
}

/// Whether a bone's name makes it this finger: the little finger goes by
/// pinky or little.
fn finger_named(name: &str, finger: &str) -> bool {
    name.contains(finger) || (finger == "pinky" && name.contains("little"))
}

/// Whether `bone` hangs somewhere under `above`.
fn under(skeleton: &Skeleton, bone: usize, above: usize) -> bool {
    let mut at = skeleton.bones[bone].parent;
    while let Some(parent) = at {
        if parent == above {
            return true;
        }
        at = skeleton.bones[parent].parent;
    }
    false
}

fn depth(skeleton: &Skeleton, bone: usize) -> usize {
    let mut depth = 0;
    let mut at = skeleton.bones[bone].parent;
    while let Some(parent) = at {
        depth += 1;
        at = skeleton.bones[parent].parent;
    }
    depth
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use voxear_core::math::{Quat, Vec3};
    use voxear_render::{Bone, BonePose};

    /// A small humanoid standing at the origin facing +Z, its arms out to
    /// the sides, named as `name` names each bone.
    pub fn body(name: impl Fn(&str, Option<bool>) -> String) -> Skeleton {
        let mut bones: Vec<Bone> = Vec::new();
        let mut add = |part: &str, side: Option<bool>, parent: Option<usize>, at: Vec3| {
            let rest = BonePose { translation: at, rotation: Quat::IDENTITY, scale: Vec3::ONE };
            bones.push(Bone { name: name(part, side), parent, rest });
            bones.len() - 1
        };
        let hips = add("hips", None, None, Vec3::new(0.0, 1.0, 0.0));
        let spine = add("spine", None, Some(hips), Vec3::new(0.0, 0.15, 0.0));
        let chest = add("chest", None, Some(spine), Vec3::new(0.0, 0.2, 0.0));
        let neck = add("neck", None, Some(chest), Vec3::new(0.0, 0.2, 0.0));
        add("head", None, Some(neck), Vec3::new(0.0, 0.1, 0.0));
        for left in [true, false] {
            // The engine is left-handed: facing +Z, the left hand is toward -X.
            let out = if left { -1.0 } else { 1.0 };
            let clavicle = add("shoulder", Some(left), Some(chest), Vec3::new(0.05 * out, 0.15, 0.0));
            let upper = add("upper_arm", Some(left), Some(clavicle), Vec3::new(0.15 * out, 0.0, 0.0));
            let twist = add("upper_arm_twist", Some(left), Some(upper), Vec3::new(0.14 * out, 0.0, 0.0));
            let forearm = add("forearm", Some(left), Some(twist), Vec3::new(0.14 * out, 0.0, 0.0));
            let hand = add("hand", Some(left), Some(forearm), Vec3::new(0.26 * out, 0.0, 0.0));
            let index = add("index_01", Some(left), Some(hand), Vec3::new(0.08 * out, 0.0, 0.02));
            add("index_02", Some(left), Some(index), Vec3::new(0.03 * out, 0.0, 0.0));
            add("thumb_01", Some(left), Some(hand), Vec3::new(0.03 * out, -0.02, 0.04));
            add("middle_01", Some(left), Some(hand), Vec3::new(0.085 * out, 0.0, 0.0));
            add("pinky_01", Some(left), Some(hand), Vec3::new(0.075 * out, 0.0, -0.035));
        }
        for left in [true, false] {
            let out = if left { -1.0 } else { 1.0 };
            let thigh = add("thigh", Some(left), Some(hips), Vec3::new(0.1 * out, -0.05, 0.0));
            let shin = add("shin", Some(left), Some(thigh), Vec3::new(0.0, -0.45, 0.0));
            let foot = add("foot", Some(left), Some(shin), Vec3::new(0.0, -0.42, 0.0));
            add("toe", Some(left), Some(foot), Vec3::new(0.0, -0.06, 0.12));
        }
        Skeleton { bones }
    }

    #[test]
    fn a_rig_with_two_neck_bones_ends_its_spine_below_them() {
        let mut skeleton = body(|part, side| match side {
            Some(left) => format!("{part}_{}", if left { "l" } else { "r" }),
            None => part.to_string(),
        });
        let find = |skeleton: &Skeleton, name: &str| skeleton.bones.iter().position(|bone| bone.name == name).unwrap();
        let (neck, head) = (find(&skeleton, "neck"), find(&skeleton, "head"));
        skeleton.bones[neck].name = "neck_01".into();
        let rest = BonePose { translation: Vec3::new(0.0, 0.05, 0.0), ..BonePose::default() };
        skeleton.bones.push(Bone { name: "neck_02".into(), parent: Some(neck), rest });
        skeleton.bones[head].parent = Some(skeleton.bones.len() - 1);
        let rig = Rig::find(&skeleton).expect("a humanoid");
        assert_eq!(rig.neck, Some(neck), "the lower neck bone");
        assert_eq!(skeleton.bones[rig.chest()].name, "chest", "the spine stops below the neck");
        assert_eq!(rig.spine.len(), 2);
    }

    #[test]
    fn a_rig_is_found_by_any_of_the_usual_names() {
        let namings: [&dyn Fn(&str, Option<bool>) -> String; 2] = [
            &|part, side| match side {
                Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
                None => format!("DEF-{part}"),
            },
            &|part, side| match side {
                Some(left) => format!("{}_{}", part.replace('_', ""), if left { "l" } else { "r" }),
                None => part.to_string(),
            },
        ];
        for naming in namings {
            let skeleton = body(naming);
            let rig = Rig::find(&skeleton).expect("a humanoid");
            let name = |bone: usize| skeleton.bones[bone].name.to_lowercase();
            assert!(name(rig.head).contains("head") && name(rig.chest()).contains("chest"));
            assert_eq!(rig.spine.len(), 2);
            let left = &rig.arms[0];
            assert!(name(left.hand).contains("hand") && side_of(&name(left.hand)) == Some(true));
            assert!(name(left.forearm).contains("forearm"), "{}", name(left.forearm));
            assert!(!name(left.upper).contains("twist"), "twist bones are passed over: {}", name(left.upper));
            assert!(left.clavicle.is_some_and(|bone| name(bone).contains("shoulder")));
            assert_eq!(left.fingers[1].len(), 2, "the index finger's two bones");
            assert_eq!(left.fingers[0].len(), 1);
            assert_eq!(left.fingers[2].len(), 1);
            assert!(left.fingers[3].is_empty(), "no ring finger");
            let [leg, right_leg] = rig.legs.as_ref().expect("legs");
            assert!(name(leg.foot).contains("foot") && side_of(&name(leg.foot)) == Some(true));
            assert!(name(leg.shin).contains("shin") && name(leg.thigh).contains("thigh"));
            assert!(leg.toe.is_some_and(|toe| name(toe).contains("toe")) && !right_leg.left);
        }
    }
}
