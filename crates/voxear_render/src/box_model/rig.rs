//! Bones made for a box model from its shape, and a limb split for a
//! joint. [`BoxModel::make_bones`] looks at where the boxes stand: the
//! largest is the body, what stands under it on the ground are legs, what
//! hangs at its sides arms or wings, what sits on it or before it the
//! head, and what trails behind it a tail. Each gets a bone, pivoted where
//! it meets the body, with a [`BoneRole`] saying which it is, and the rest
//! go to whichever of those they touch. [`BoxModel::add_joint`] cuts a box
//! across its longest side and hangs the far half from a bone at the cut:
//! a knee or an elbow.

use voxear_core::math::{Mat4, Vec3};

use super::{BoneRole, BoxBone, BoxModel, BoxSkin, Face, FaceRect, ModelBox, degrees};

/// How near two boxes must be to touch, and how near the ground a foot
/// must be to stand on it, in pixels.
const NEAR: f32 = 0.6;

/// A box as it stands in the model, in pixels: where it came from, the box
/// round it, and its place and turn.
struct Standing {
    from: (usize, usize),
    least: Vec3,
    most: Vec3,
    place: Mat4,
}

impl Standing {
    fn middle(&self) -> Vec3 {
        (self.least + self.most) * 0.5
    }

    fn size(&self) -> Vec3 {
        self.most - self.least
    }

    fn volume(&self) -> f32 {
        let size = self.size().max(Vec3::splat(0.5));
        size.x * size.y * size.z
    }

    /// How far apart two boxes are: 0 where they touch or cross.
    fn gap(&self, other: &Standing) -> f32 {
        (self.least - other.most).max(other.least - self.most).max(Vec3::ZERO).length()
    }
}

impl BoxModel {
    /// Makes the model's bones afresh from its shape, in place of those it
    /// has: a Body, and under it a Head, legs, arms, wings and a tail where
    /// its boxes make them, every box standing where it stood. Returns
    /// whether it found a body to hang them from; `false`, with nothing
    /// changed, for a model with no boxes.
    pub fn make_bones(&mut self) -> bool {
        let scale = self.pixels_per_block;
        let bones = self.rest_matrices();
        let mut standing: Vec<Standing> = Vec::new();
        for (bone, index, model_box) in self.boxes() {
            let corners = self.box_corners(&bones[bone], model_box).map(|corner| corner * scale);
            let least = corners.iter().fold(Vec3::INFINITY, |least, corner| least.min(*corner));
            let most = corners.iter().fold(Vec3::NEG_INFINITY, |most, corner| most.max(*corner));
            let (place, _) = self.box_place(&bones[bone], model_box);
            standing.push(Standing { from: (bone, index), least, most, place });
        }
        // The body: what the most other boxes touch, as limbs do it, and of those touched by as many the largest. A
        // head is often the larger, but little hangs from it.
        let touched = |at: usize| {
            (0..standing.len()).filter(|&other| other != at && standing[at].gap(&standing[other]) <= NEAR).count()
        };
        let Some(body) = (0..standing.len())
            .max_by(|&a, &b| touched(a).cmp(&touched(b)).then(standing[a].volume().total_cmp(&standing[b].volume())))
        else {
            return false;
        };
        let ground = standing.iter().map(|s| s.least.y).fold(f32::INFINITY, f32::min);
        let (trunk_least, trunk_most, trunk_middle) =
            (standing[body].least, standing[body].most, standing[body].middle());
        let on_ground = |s: &Standing| s.least.y <= ground + NEAR;

        // What each other box is, where it is one of these by itself.
        let role = |s: &Standing| -> BoneRole {
            let (size, middle) = (s.size(), s.middle());
            let beside = s.least.x >= trunk_most.x - NEAR || s.most.x <= trunk_least.x + NEAR;
            let under = s.most.y <= trunk_least.y + size.y * 0.5 + NEAR;
            if on_ground(s) && !on_ground(&standing[body]) && (under || beside) && size.y >= size.x.max(size.z) * 0.75 {
                return BoneRole::Leg;
            }
            if beside && size.y >= size.x.max(size.z) && s.most.y > trunk_middle.y {
                return BoneRole::Arm;
            }
            if beside && size.y.min(size.z) <= size.x.max(1.0) * 0.5 {
                return BoneRole::Wing;
            }
            if s.most.z <= trunk_least.z + NEAR && middle.y >= trunk_least.y - NEAR {
                return BoneRole::Tail;
            }
            let above = s.least.y >= trunk_most.y - NEAR;
            let before = s.least.z >= trunk_most.z - NEAR;
            if above || before { BoneRole::Head } else { BoneRole::None }
        };
        let mut roles: Vec<BoneRole> = standing.iter().map(role).collect();
        roles[body] = BoneRole::Body;
        // One head, the largest that could be: the rest of them are what it wears.
        let head = (0..standing.len())
            .filter(|&at| roles[at] == BoneRole::Head)
            .max_by(|&a, &b| standing[a].volume().total_cmp(&standing[b].volume()));
        for at in 0..standing.len() {
            if roles[at] == BoneRole::Head && Some(at) != head {
                roles[at] = BoneRole::None;
            }
        }
        // One tail: the nearest the body of what trails it; the rest go on the end of it.
        let tail = (0..standing.len())
            .filter(|&at| roles[at] == BoneRole::Tail)
            .max_by(|&a, &b| standing[a].most.z.total_cmp(&standing[b].most.z));
        for at in 0..standing.len() {
            if roles[at] == BoneRole::Tail && Some(at) != tail {
                roles[at] = BoneRole::None;
            }
        }

        // A bone for each part, in the order they will be named, and which bone each box goes to.
        let limbs: Vec<usize> = (0..standing.len()).filter(|&at| !matches!(roles[at], BoneRole::None)).collect();
        let mut owner: Vec<Option<usize>> = (0..standing.len()).map(|at| limbs.contains(&at).then_some(at)).collect();
        // The rest go to the part they are nearest, as each is placed, so a hat on a hat on a head goes to the head.
        loop {
            let loose = (0..standing.len()).filter(|&at| owner[at].is_none());
            let nearest = loose
                .flat_map(|at| (0..standing.len()).filter(|&to| owner[to].is_some()).map(move |to| (at, to)))
                .min_by(|a, b| standing[a.0].gap(&standing[a.1]).total_cmp(&standing[b.0].gap(&standing[b.1])));
            let Some((at, to)) = nearest else { break };
            owner[at] = owner[to];
        }

        // Where each part turns: where it meets the body.
        let pivot = |at: usize| -> Vec3 {
            let (s, middle) = (&standing[at], standing[at].middle());
            match roles[at] {
                BoneRole::Leg => Vec3::new(middle.x, s.most.y, middle.z),
                BoneRole::Arm => Vec3::new(middle.x, s.most.y - (s.size().y / 6.0).min(2.0).round(), middle.z),
                BoneRole::Wing => {
                    let inner = if middle.x >= trunk_middle.x { s.least.x } else { s.most.x };
                    Vec3::new(inner, middle.y, middle.z)
                }
                BoneRole::Tail => Vec3::new(middle.x, middle.y, s.most.z),
                // On top of the body it turns about its foot; before it, about its back.
                BoneRole::Head if s.least.y >= trunk_most.y - NEAR => Vec3::new(middle.x, s.least.y, middle.z),
                BoneRole::Head => Vec3::new(middle.x, middle.y, s.least.z),
                _ => Vec3::new(middle.x, s.least.y, middle.z),
            }
        };
        let names = names(&limbs, &roles, &standing, trunk_middle);
        let body_pivot = pivot(body);
        let mut made: Vec<BoxBone> = Vec::new();
        let mut bone_of: Vec<Option<usize>> = vec![None; standing.len()];
        // As they are named: the body first, so it is every other bone's parent.
        for (at, name) in &names {
            let (parent, from) = if *at == body { (None, pivot(*at)) } else { (Some(0), pivot(*at) - body_pivot) };
            bone_of[*at] = Some(made.len());
            made.push(BoxBone { role: roles[*at], ..BoxBone::new(name, parent, from) });
        }
        for (at, s) in standing.iter().enumerate() {
            let Some(bone) = owner[at].and_then(|limb| bone_of[limb]) else { continue };
            let mut model_box = self.bones[s.from.0].boxes[s.from.1].clone();
            // Standing as it stood, measured from its new bone, which stands straight.
            let (_, rotation, middle) = s.place.to_scale_rotation_translation();
            let half = Vec3::from(model_box.size.map(|side| side as f32)) * 0.5;
            let at_pivot = pivot(owner[at].expect("checked"));
            model_box.from = round(middle * scale - half - at_pivot);
            model_box.rotation = degrees(rotation);
            made[bone].boxes.push(model_box);
        }
        self.bones = made;
        true
    }

    /// Cuts a box in two across its longest side, at its middle, and hangs
    /// the half further from its bone's pivot from a new bone at the cut,
    /// under the box's bone and with its role: a joint, such as a knee.
    /// Each half shows the pixels it showed. Returns the new bone; `None`,
    /// with nothing changed, for a box too short to cut or turned on its
    /// own.
    pub fn add_joint(&mut self, bone: usize, index: usize) -> Option<usize> {
        let whole = self.bones.get(bone)?.boxes.get(index)?.clone();
        let axis = (0..3).max_by_key(|&axis| whole.size[axis])?;
        if whole.size[axis] < 2 || whole.rotation != Vec3::ZERO {
            return None;
        }
        let near = whole.size[axis] / 2;
        let far = whole.size[axis] - near;
        // The half nearer the pivot stays: a leg's top, an arm's shoulder, a tail's root.
        let low_stays = (whole.from[axis] + whole.size[axis] as f32 * 0.5).abs() <= whole.size[axis] as f32 * 0.5
            && whole.from[axis].abs() <= (whole.from[axis] + whole.size[axis] as f32).abs();
        let (low_size, high_size) = if low_stays { (near, far) } else { (far, near) };
        let cut = whole.from[axis] + low_size as f32;
        let (mut low, mut high) = (whole.clone(), whole.clone());
        (low.size[axis], high.size[axis]) = (low_size, high_size);
        high.from[axis] = cut;
        let share = low_size as f32 / whole.size[axis] as f32;
        (low.skin, high.skin) =
            (halved(&whole, whole.skin, axis, share, false), halved(&whole, whole.skin, axis, share, true));
        // Its cubes go with the half they are in.
        if let Some(cubes) = &whole.voxels {
            let mut shift = [0; 3];
            shift[axis] = -(low_size as i32);
            low.voxels = Some(cubes.resized(whole.size, low.size, [0; 3]));
            high.voxels = Some(cubes.resized(whole.size, high.size, shift));
        }
        // Its layer is cut with it.
        if let Some(layer) = whole.layer {
            let half =
                |high: bool| Some(super::BoxLayer { skin: halved(&whole, layer.skin, axis, share, high), ..layer });
            (low.layer, high.layer) = (half(false), half(true));
        }

        let mut at_cut = whole.from + Vec3::from(whole.size.map(|side| side as f32)) * 0.5;
        at_cut[axis] = cut;
        let (mut kept, mut hung) = if low_stays { (low, high) } else { (high, low) };
        hung.from -= at_cut;
        kept.from = round(kept.from);
        let name = self.unique_name(&format!("{}Lower", self.bones[bone].name));
        let role = self.bones[bone].role;
        self.bones[bone].boxes[index] = kept;
        self.bones.push(BoxBone { role, boxes: vec![hung], ..BoxBone::new(&name, Some(bone), at_cut) });
        // Under its parent's own, so every parent is before its children.
        let made = self.bones.len() - 1;
        self.set_parent(made, Some(bone)).or(Some(made))
    }
}

/// Pixels to the nearest hundredth, so a place worked out from matrices
/// reads as the number it was.
fn round(pixels: Vec3) -> Vec3 {
    (pixels * 100.0).round() / 100.0 + Vec3::ZERO
}

/// The names of a model's parts: Body, Head and Tail, and Left or Right
/// before a leg, arm or wing, with Front, Middle and Back before legs that
/// stand one behind another, and a number where that is not enough.
fn names(limbs: &[usize], roles: &[BoneRole], standing: &[Standing], middle: Vec3) -> Vec<(usize, String)> {
    let side = |at: usize| match standing[at].middle().x - middle.x {
        x if x > NEAR => "Right",
        x if x < -NEAR => "Left",
        _ => "",
    };
    let mut named: Vec<(usize, String)> = Vec::new();
    for role in [BoneRole::Body, BoneRole::Head, BoneRole::Tail, BoneRole::Arm, BoneRole::Wing, BoneRole::Leg] {
        for which in ["Right", "Left", ""] {
            // Those of a side, from the front back.
            let mut row: Vec<usize> =
                limbs.iter().copied().filter(|&at| roles[at] == role && side(at) == which).collect();
            row.sort_by(|&a, &b| standing[b].middle().z.total_cmp(&standing[a].middle().z));
            for (place, &at) in row.iter().enumerate() {
                let along = match (role, row.len(), place) {
                    (BoneRole::Leg, 2, 0) | (BoneRole::Leg, 3, 0) => "Front",
                    (BoneRole::Leg, 3, 1) => "Middle",
                    (BoneRole::Leg, 2, 1) | (BoneRole::Leg, 3, 2) => "Back",
                    _ => "",
                };
                let mut name = format!("{along}{which}{}", role.label());
                // More of a kind than there are words for: numbered from the front.
                if row.len() > 3 || (row.len() > 1 && role != BoneRole::Leg) {
                    name = format!("{name}{}", place + 1);
                }
                while named.iter().any(|(_, other)| *other == name) {
                    name.push('2');
                }
                named.push((at, name));
            }
        }
    }
    named
}

/// The faces of one half of a box cut across `axis`, `share` of the way
/// along it: the low half's, or the high half's, each showing the pixels
/// that part of the whole box showed, with the face at the cut showing
/// those of the end it looks towards.
fn halved(whole: &ModelBox, skin: BoxSkin, axis: usize, share: f32, high: bool) -> BoxSkin {
    let mut faces: [Option<FaceRect>; 6] = [None; 6];
    for face in Face::ALL {
        let Some([left, top, right, bottom]) = whole.skin_rect(skin, face) else { continue };
        let (across, down) = face.across_and_down();
        // Which way the cut's axis runs on this face: along its pixels across, down them, or out of it.
        let cut = if across[axis] != 0.0 {
            let (from, to) = if across[axis] > 0.0 { (left, right) } else { (right, left) };
            let at = from + (to - from) * share;
            // From the low end to the cut, or from the cut to the high end, the way round the face has them.
            match (high, across[axis] > 0.0) {
                (false, true) => [left, top, at, bottom],
                (false, false) => [at, top, right, bottom],
                (true, true) => [at, top, right, bottom],
                (true, false) => [left, top, at, bottom],
            }
        } else if down[axis] != 0.0 {
            let (from, to) = if down[axis] > 0.0 { (top, bottom) } else { (bottom, top) };
            let at = from + (to - from) * share;
            match (high, down[axis] > 0.0) {
                (false, true) => [left, top, right, at],
                (false, false) => [left, at, right, bottom],
                (true, true) => [left, at, right, bottom],
                (true, false) => [left, top, right, at],
            }
        } else {
            // An end of the box: both halves show it, one of them at the cut.
            [left, top, right, bottom]
        };
        faces[face.index()] = Some(cut);
    }
    BoxSkin::Faces(faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where every box's corners stand, in the order the boxes are met.
    fn standing(model: &BoxModel) -> Vec<[Vec3; 8]> {
        let bones = model.rest_matrices();
        let mut found: Vec<[Vec3; 8]> =
            model.boxes().map(|(bone, _, model_box)| model.box_corners(&bones[bone], model_box)).collect();
        found.sort_by(|a, b| {
            let (a, b) = (a.iter().fold(Vec3::ZERO, |sum, c| sum + *c), b.iter().fold(Vec3::ZERO, |sum, c| sum + *c));
            a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.z.total_cmp(&b.z))
        });
        found
    }

    fn same(a: &BoxModel, b: &BoxModel) -> bool {
        let (a, b) = (standing(a), standing(b));
        a.len() == b.len() && a.iter().zip(&b).all(|(a, b)| a.iter().zip(b).all(|(a, b)| (*a - *b).length() < 1e-3))
    }

    fn bone<'a>(model: &'a BoxModel, name: &str) -> &'a BoxBone {
        model.bones.iter().find(|bone| bone.name == name).unwrap_or_else(|| {
            let names: Vec<&str> = model.bones.iter().map(|bone| bone.name.as_str()).collect();
            panic!("no {name} among {names:?}")
        })
    }

    /// Boxes on one bone, each by its least corner and size in pixels.
    fn heap(boxes: &[([f32; 3], [u32; 3])]) -> BoxModel {
        let mut model = BoxModel::default();
        let all = model.add_bone("All", None, Vec3::ZERO);
        for (from, size) in boxes {
            model.add_box(all, Vec3::from(*from), *size, [0, 0]);
        }
        model
    }

    #[test]
    fn a_figure_on_two_legs_gets_a_body_a_head_two_arms_and_two_legs() {
        let before = heap(&[
            ([-4.0, 12.0, -2.0], [8, 12, 4]), // body
            ([-4.0, 24.0, -4.0], [8, 8, 8]),  // head
            ([4.0, 12.0, -2.0], [4, 12, 4]),  // its right arm
            ([-8.0, 12.0, -2.0], [4, 12, 4]), // its left arm
            ([0.0, 0.0, -2.0], [4, 12, 4]),   // its right leg
            ([-4.0, 0.0, -2.0], [4, 12, 4]),  // its left leg
            ([-1.0, 32.0, -1.0], [2, 2, 2]),  // a knob on its head
        ]);
        let mut model = before.clone();
        assert!(model.make_bones());
        let names: Vec<&str> = model.bones.iter().map(|bone| bone.name.as_str()).collect();
        assert_eq!(names, ["Body", "Head", "RightArm", "LeftArm", "RightLeg", "LeftLeg"]);
        assert!(same(&model, &before), "nothing moved");
        assert!(model.bones.iter().skip(1).all(|bone| bone.parent == Some(0)));
        assert_eq!(bone(&model, "Body").pivot, Vec3::new(0.0, 12.0, 0.0), "at its foot");
        assert_eq!(
            bone(&model, "Head").pivot,
            Vec3::new(0.0, 12.0, 0.0),
            "where it meets the body, from the body's pivot"
        );
        assert_eq!(bone(&model, "Head").boxes.len(), 2, "the knob goes with the head");
        assert_eq!(bone(&model, "RightLeg").pivot, Vec3::new(2.0, 0.0, 0.0), "at its hip");
        assert_eq!(bone(&model, "RightLeg").boxes[0].from, Vec3::new(-2.0, -12.0, -2.0));
        assert_eq!(bone(&model, "LeftArm").pivot, Vec3::new(-6.0, 10.0, 0.0), "at its shoulder");
        let roles: Vec<BoneRole> = model.bones.iter().map(|bone| bone.role).collect();
        use BoneRole::{Arm, Body, Head, Leg};
        assert_eq!(roles, [Body, Head, Arm, Arm, Leg, Leg]);
        assert_eq!(BoxModel::parse(&model.to_text()).unwrap(), model, "roles are kept in the file");
    }

    #[test]
    fn a_beast_on_four_legs_gets_its_head_in_front_and_its_tail_behind() {
        let before = heap(&[
            ([-5.0, 8.0, -8.0], [10, 8, 16]), // body, along Z
            ([-4.0, 10.0, 8.0], [8, 8, 8]),   // head, before it
            ([-2.0, 11.0, 16.0], [4, 3, 1]),  // snout
            ([-1.0, 12.0, -14.0], [2, 2, 6]), // tail
            ([3.0, 0.0, 5.0], [4, 8, 4]),
            ([-7.0, 0.0, 5.0], [4, 8, 4]),
            ([3.0, 0.0, -9.0], [4, 8, 4]),
            ([-7.0, 0.0, -9.0], [4, 8, 4]),
        ]);
        let mut model = before.clone();
        assert!(model.make_bones());
        let names: Vec<&str> = model.bones.iter().map(|bone| bone.name.as_str()).collect();
        assert_eq!(names, ["Body", "Head", "Tail", "FrontRightLeg", "BackRightLeg", "FrontLeftLeg", "BackLeftLeg"]);
        assert!(same(&model, &before));
        assert_eq!(bone(&model, "Head").boxes.len(), 2, "its snout with it");
        // The head turns about the middle of its back, the tail about its root.
        assert_eq!(bone(&model, "Head").pivot + bone(&model, "Body").pivot, Vec3::new(0.0, 14.0, 8.0));
        assert_eq!(bone(&model, "Tail").pivot + bone(&model, "Body").pivot, Vec3::new(0.0, 13.0, -8.0));
        assert_eq!(bone(&model, "BackLeftLeg").role, BoneRole::Leg);
    }

    #[test]
    fn a_blob_on_the_ground_is_all_body_and_wings_are_flat_at_the_sides() {
        let mut blob = heap(&[([-4.0, 0.0, -4.0], [8, 8, 8]), ([-2.0, 2.0, 4.0], [4, 2, 1])]);
        assert!(blob.make_bones());
        assert_eq!(blob.bones.len(), 2, "a body, and its face before it as a head");
        let mut bird = heap(&[
            ([-3.0, 6.0, -4.0], [6, 6, 8]),
            ([3.0, 9.0, -3.0], [6, 1, 6]),
            ([-9.0, 9.0, -3.0], [6, 1, 6]),
            ([-1.0, 0.0, -1.0], [2, 6, 2]),
        ]);
        assert!(bird.make_bones());
        let names: Vec<&str> = bird.bones.iter().map(|bone| bone.name.as_str()).collect();
        assert_eq!(names, ["Body", "RightWing", "LeftWing", "Leg"]);
        assert_eq!(bone(&bird, "RightWing").pivot, Vec3::new(3.0, 3.5, 0.0), "at its inner edge, from the body's foot");
        assert!(!BoxModel::default().make_bones());
    }

    #[test]
    fn a_leg_cut_for_a_knee_hangs_its_lower_half_from_a_bone_at_the_cut() {
        let mut before = BoxModel::default();
        let body = before.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
        before.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
        let leg = before.add_bone("LeftLeg", Some(body), Vec3::new(-2.0, 0.0, 0.0));
        before.bones[leg].role = BoneRole::Leg;
        before.add_box(leg, Vec3::new(-2.0, -12.0, -2.0), [4, 12, 4], [0, 16]);
        // Each pixel its own colour, to see which each half shows.
        for at in 0..64 * 64u32 {
            before.sheet.set(at % 64, at / 64, [(at % 64) as u8 * 4, (at / 64) as u8 * 4, 9, 255]);
        }
        let mut model = before.clone();
        let knee = model.add_joint(leg, 0).unwrap();
        assert_eq!((model.bones[knee].name.as_str(), model.bones[knee].parent), ("LeftLegLower", Some(leg)));
        assert_eq!(model.bones[knee].role, BoneRole::Leg);
        assert_eq!(model.bones[knee].pivot, Vec3::new(0.0, -6.0, 0.0), "the middle of the cut, from the hip");
        let (upper, lower) = (&model.bones[leg].boxes[0], &model.bones[knee].boxes[0]);
        assert_eq!((upper.from, upper.size), (Vec3::new(-2.0, -6.0, -2.0), [4, 6, 4]), "the half at the hip stays");
        assert_eq!((lower.from, lower.size), (Vec3::new(-2.0, -6.0, -2.0), [4, 6, 4]), "the other from the knee");
        assert_eq!(model.bounds(), before.bounds(), "the leg stands where it stood");
        assert_eq!(standing(&model).len(), 3);
        // The front of the whole leg was 4 × 12 at 4, 20: its upper half's is the upper six rows, the lower's the rest.
        assert_eq!(upper.face_rect(Face::Front), Some([4.0, 20.0, 8.0, 26.0]));
        assert_eq!(lower.face_rect(Face::Front), Some([4.0, 26.0, 8.0, 32.0]));
        assert_eq!(upper.face_rect(Face::Top), before.bones[leg].boxes[0].face_rect(Face::Top), "its top is the leg's");
        assert_eq!(lower.face_rect(Face::Bottom), before.bones[leg].boxes[0].face_rect(Face::Bottom));
        assert!(BoxModel::parse(&model.to_text()).is_ok());

        // An arm reaching out along X: the half at the shoulder stays, whichever side it is on.
        let mut arm = BoxModel::default();
        let shoulder = arm.add_bone("LeftArm", None, Vec3::ZERO);
        arm.add_box(shoulder, Vec3::new(-10.0, -1.0, -1.0), [10, 2, 2], [0, 0]);
        let elbow = arm.add_joint(shoulder, 0).unwrap();
        assert_eq!(arm.bones[shoulder].boxes[0].from.x, -5.0, "from the shoulder out to the cut");
        assert_eq!(arm.bones[elbow].pivot, Vec3::new(-5.0, 0.0, 0.0));
        assert_eq!(arm.bones[elbow].boxes[0].from.x, -5.0, "and from the cut on out");

        let mut small = BoxModel::default();
        let bone = small.add_bone("Body", None, Vec3::ZERO);
        small.add_box(bone, Vec3::ZERO, [1, 1, 1], [0, 0]);
        assert_eq!(small.add_joint(bone, 0), None, "too short to cut");
    }
}
