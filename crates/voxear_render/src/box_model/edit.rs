//! Changing a box model, and finding what of it a ray meets: what an editor
//! does to one, kept with the model so it is done the same wherever it is
//! done from. Bones are taken out with all under them, moved under another
//! bone or copied across the body without moving where they stand, and a
//! new box is given room on the sheet no other box's faces use.

use voxear_core::math::{Mat4, Vec3};

use super::{BoxBone, BoxModel, BoxSkin, Face, FaceRect, ModelBox, degrees};

/// What a ray met of a model: the box, by its bone and its place among the
/// bone's boxes, the face it went in by, how far along the ray, in metres,
/// and where, in the model's space, and the pixel of the sheet there, for a
/// face that is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxHit {
    pub bone: usize,
    pub index: usize,
    pub face: Face,
    pub distance: f32,
    pub point: Vec3,
    pub pixel: Option<[u32; 2]>,
}

/// How thick a flat box is to a ray, in metres, so it can be met at all.
const PAPER: f32 = 1e-4;

impl BoxModel {
    /// The nearest box a ray from `origin` along `direction` meets, in the
    /// model's space in metres, with its bones at `bones` (see
    /// [`rest_matrices`](Self::rest_matrices)).
    pub fn pick(&self, bones: &[Mat4], origin: Vec3, direction: Vec3) -> Option<BoxHit> {
        let direction = direction.try_normalize()?;
        let mut nearest: Option<BoxHit> = None;
        for ((bone, in_model), at) in self.bones.iter().zip(bones).zip(0..) {
            for (index, model_box) in bone.boxes.iter().enumerate() {
                let (place, reach) = self.box_place(in_model, model_box);
                let inverse = place.inverse();
                let (from, along) = (inverse.transform_point3(origin), inverse.transform_vector3(direction));
                let Some((distance, face)) = enter(from, along, reach.max(Vec3::splat(PAPER))) else { continue };
                if nearest.is_some_and(|hit| hit.distance <= distance) {
                    continue;
                }
                let inside = from + along * distance;
                let pixel = model_box.face_rect(face).map(|rect| pixel_at(rect, face, inside, reach));
                let point = origin + direction * distance;
                nearest = Some(BoxHit { bone: at, index, face, distance, point, pixel });
            }
        }
        nearest
    }

    /// The box round the whole model as it stands at rest, in metres, least
    /// and most; `None` for one with no boxes.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let bones = self.rest_matrices();
        let mut found: Option<(Vec3, Vec3)> = None;
        for (bone, in_model) in self.bones.iter().zip(&bones) {
            for model_box in &bone.boxes {
                for corner in self.box_corners(in_model, model_box) {
                    let (least, most) = found.unwrap_or((corner, corner));
                    found = Some((least.min(corner), most.max(corner)));
                }
            }
        }
        found
    }

    /// A box's eight corners in the model's space, in metres, with its bone
    /// at `bone`: the four of its back, round from its lower right, then
    /// the four of its front the same way.
    pub fn box_corners(&self, bone: &Mat4, model_box: &ModelBox) -> [Vec3; 8] {
        let (place, reach) = self.box_place(bone, model_box);
        let signs = [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)];
        std::array::from_fn(|corner| {
            let (x, y) = signs[corner % 4];
            let z = if corner < 4 { -1.0 } else { 1.0 };
            place.transform_point3(Vec3::new(x, y, z) * reach)
        })
    }

    /// A bone and every bone under it, in the order the model has them.
    pub fn with_children(&self, bone: usize) -> Vec<usize> {
        let mut found = vec![bone];
        for (at, other) in self.bones.iter().enumerate().skip(bone + 1) {
            if other.parent.is_some_and(|parent| found.contains(&parent)) {
                found.push(at);
            }
        }
        found
    }

    /// `wanted`, or with a number after it where a bone has that name.
    pub fn unique_name(&self, wanted: &str) -> String {
        let taken = |name: &str| self.bones.iter().any(|bone| bone.name == name);
        if !taken(wanted) {
            return wanted.to_string();
        }
        let stem = wanted.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
        (2..).map(|number| format!("{stem} {number}")).find(|name| !taken(name)).expect("numbers do not run out")
    }

    /// Takes a bone out, with its boxes and every bone under it.
    pub fn remove_bone(&mut self, bone: usize) {
        let gone = self.with_children(bone);
        let now = |old: usize| old - gone.iter().filter(|&&other| other < old).count();
        let kept = std::mem::take(&mut self.bones).into_iter().enumerate().filter(|(at, _)| !gone.contains(at));
        self.bones = kept
            .map(|(_, mut bone)| {
                bone.parent = bone.parent.map(now);
                bone
            })
            .collect();
    }

    /// Hangs a bone from another, or from the model itself, leaving it and
    /// all it carries standing where they were. Returns where the bone now
    /// is among the bones, or `None`, with nothing changed, for a parent
    /// that is the bone itself or one under it.
    pub fn set_parent(&mut self, bone: usize, parent: Option<usize>) -> Option<usize> {
        let moved = self.with_children(bone);
        if parent.is_some_and(|parent| moved.contains(&parent) || parent >= self.bones.len()) {
            return None;
        }
        let placed = self.rest_matrices();
        let under = parent.map_or(Mat4::IDENTITY, |parent| placed[parent]);
        let (_, rotation, translation) = (under.inverse() * placed[bone]).to_scale_rotation_translation();
        self.bones[bone].parent = parent;
        self.bones[bone].pivot = whole(translation * self.pixels_per_block);
        self.bones[bone].rotation = degrees(rotation);

        // Every parent before its children again: the bone and those under it go right after its new parent's own.
        let after = parent.map_or(self.bones.len(), |parent| {
            let family = self.with_children(parent);
            family.iter().filter(|other| !moved.contains(other)).max().map_or(parent, |last| *last) + 1
        });
        let mut order: Vec<usize> = (0..self.bones.len()).filter(|at| !moved.contains(at)).collect();
        let insert = order.iter().position(|&at| at >= after).unwrap_or(order.len());
        order.splice(insert..insert, moved.iter().copied());
        let now = |old: usize| order.iter().position(|&at| at == old).expect("every bone is in the order");
        let mut bones: Vec<Option<BoxBone>> = std::mem::take(&mut self.bones).into_iter().map(Some).collect();
        self.bones = order
            .iter()
            .map(|&old| {
                let mut bone = bones[old].take().expect("each bone once");
                bone.parent = bone.parent.map(now);
                bone
            })
            .collect();
        Some(now(bone))
    }

    /// Moves a box to another bone, leaving it standing where it was.
    /// Returns its place among that bone's boxes.
    pub fn move_box(&mut self, bone: usize, index: usize, to: usize) -> Option<usize> {
        if bone == to || to >= self.bones.len() || index >= self.bones.get(bone)?.boxes.len() {
            return None;
        }
        let placed = self.rest_matrices();
        let mut moved = self.bones[bone].boxes.remove(index);
        let (place, _) = self.box_place(&placed[bone], &moved);
        let (_, rotation, middle) = (placed[to].inverse() * place).to_scale_rotation_translation();
        let half = Vec3::from(moved.size.map(|side| side as f32)) * 0.5;
        moved.from = whole(middle * self.pixels_per_block - half);
        moved.rotation = degrees(rotation);
        self.bones[to].boxes.push(moved);
        Some(self.bones[to].boxes.len() - 1)
    }

    /// Copies a bone, with its boxes and every bone under it, beside it
    /// under the same parent, each copy named for what it copies. Returns
    /// the copy of the bone itself.
    pub fn duplicate_bone(&mut self, bone: usize) -> usize {
        self.copy_bones(bone, |model, name| model.unique_name(name), |_| {})
    }

    /// Copies a bone, with its boxes and every bone under it, across the
    /// body: where the mirror at X = 0 of its parent shows it, its boxes'
    /// pixels turned over, and Left in its name made Right or Right Left.
    /// Returns the copy of the bone itself.
    pub fn mirror_bone(&mut self, bone: usize) -> usize {
        let named = |model: &BoxModel, name: &str| model.unique_name(&other_side(name));
        self.copy_bones(bone, named, |bone| {
            bone.pivot.x = -bone.pivot.x;
            bone.rotation = Vec3::new(bone.rotation.x, -bone.rotation.y, -bone.rotation.z);
            bone.boxes.iter_mut().for_each(mirror_box);
        })
    }

    /// A box as the mirror at its bone's X = 0 shows it, its pixels turned
    /// over.
    pub fn mirrored_box(model_box: &ModelBox) -> ModelBox {
        let mut copy = model_box.clone();
        mirror_box(&mut copy);
        copy
    }

    fn copy_bones(
        &mut self,
        bone: usize,
        name: impl Fn(&BoxModel, &str) -> String,
        change: impl Fn(&mut BoxBone),
    ) -> usize {
        let copied = self.with_children(bone);
        let first = copied.last().map_or(bone, |last| *last) + 1;
        // Those after the copies move along by as many as are put in.
        for later in self.bones.iter_mut().skip(first) {
            later.parent = later.parent.map(|parent| if parent >= first { parent + copied.len() } else { parent });
        }
        for (made, &old) in copied.iter().enumerate() {
            let mut copy = self.bones[old].clone();
            copy.name = name(self, &copy.name);
            // The first hangs where the bone copied does; the rest from their parents' copies.
            if made > 0 {
                copy.parent = copy.parent.map(|parent| first + copied.iter().position(|&at| at == parent).unwrap_or(0));
            }
            change(&mut copy);
            self.bones.insert(first + made, copy);
        }
        first
    }

    /// The parts of the sheet boxes' faces use, each its left, top, width
    /// and height in pixels.
    pub fn used_places(&self) -> Vec<[u32; 4]> {
        let mut used = Vec::new();
        for model_box in self.bones.iter().flat_map(|bone| &bone.boxes) {
            match model_box.skin {
                // The row of the top and bottom, and the row of the sides under it.
                BoxSkin::Unfolded { at: [u, v], .. } => {
                    let [w, h, d] = model_box.size;
                    used.push([u + d, v, 2 * w, d]);
                    used.push([u, v + d, 2 * (w + d), h]);
                }
                BoxSkin::Faces(faces) => used.extend(faces.iter().flatten().map(|&[left, top, right, bottom]| {
                    let (x, y) = (left.min(right).floor(), top.min(bottom).floor());
                    [x as u32, y as u32, ((left - right).abs().ceil()) as u32, ((top - bottom).abs().ceil()) as u32]
                })),
            }
        }
        used.retain(|place| place[2] > 0 && place[3] > 0);
        used
    }

    /// Room on the sheet for `width` × `height` pixels no box's faces use,
    /// the nearest the top, then the left; `None` where the sheet has none.
    pub fn free_place(&self, [width, height]: [u32; 2]) -> Option<[u32; 2]> {
        let (sheet_width, sheet_height) = (self.sheet.width, self.sheet.height);
        if width == 0 || height == 0 {
            return Some([0, 0]);
        }
        let used = self.used_places();
        for y in 0..=sheet_height.checked_sub(height)? {
            let mut x = 0;
            while x + width <= sheet_width {
                let over = used
                    .iter()
                    .find(|[left, top, w, h]| x < left + w && *left < x + width && y < top + h && *top < y + height);
                match over {
                    // Past what is in the way.
                    Some([left, _, w, _]) => x = left + w,
                    None => return Some([x, y]),
                }
            }
        }
        None
    }

    /// Adds a box of `size` pixels to a bone, standing on its pivot with
    /// its middle over it, unfolded where the sheet has room, or at its
    /// corner where it has none. Returns its place among the bone's boxes.
    pub fn add_box_with_room(&mut self, bone: usize, size: [u32; 3]) -> usize {
        let from = Vec3::new(-(size[0] as f32) * 0.5, 0.0, -(size[2] as f32) * 0.5);
        let model_box = ModelBox::new(from.floor(), size, [0, 0]);
        let at = self.free_place(model_box.unfolded_size()).unwrap_or([0, 0]);
        self.add_box(bone, from.floor(), size, at);
        self.bones[bone].boxes.len() - 1
    }
}

/// Where a ray from `from` along `along`, in a box's own space, goes into
/// the box reaching `reach` each way from its middle: how far along, and by
/// which face. `None` for a ray that misses, or starts inside or past it.
fn enter(from: Vec3, along: Vec3, reach: Vec3) -> Option<(f32, Face)> {
    let (mut nearest, mut farthest, mut face) = (f32::NEG_INFINITY, f32::INFINITY, Face::Front);
    let faces = [(Face::Left, Face::Right), (Face::Bottom, Face::Top), (Face::Back, Face::Front)];
    for axis in 0..3 {
        let (low, high) = faces[axis];
        if along[axis].abs() < 1e-9 {
            if from[axis].abs() > reach[axis] {
                return None;
            }
            continue;
        }
        let (to_low, to_high) = ((-reach[axis] - from[axis]) / along[axis], (reach[axis] - from[axis]) / along[axis]);
        // Going up the axis it goes in by the low face, and down it by the high one.
        let (first, last, by) = if to_low < to_high { (to_low, to_high, low) } else { (to_high, to_low, high) };
        if first > nearest {
            (nearest, face) = (first, by);
        }
        farthest = farthest.min(last);
    }
    (nearest <= farthest && nearest >= 0.0).then_some((nearest, face))
}

/// The sheet's pixel under a point of a face, the point in the box's own
/// space.
fn pixel_at([left, top, right, bottom]: FaceRect, face: Face, point: Vec3, reach: Vec3) -> [u32; 2] {
    let (across, down) = face.across_and_down();
    let share = |along: Vec3| {
        let most = (along * reach).length().max(f32::EPSILON);
        ((point.dot(along) / most + 1.0) * 0.5).clamp(0.0, 0.999_99)
    };
    let (u, v) = (left + (right - left) * share(across), top + (bottom - top) * share(down));
    [u.max(0.0).floor() as u32, v.max(0.0).floor() as u32]
}

/// Pixels to the nearest thousandth, so a place worked out from matrices
/// reads as the whole number it was.
fn whole(pixels: Vec3) -> Vec3 {
    (pixels * 1000.0).round() / 1000.0 + Vec3::ZERO
}

fn mirror_box(model_box: &mut ModelBox) {
    model_box.from.x = -(model_box.from.x + model_box.size[0] as f32);
    model_box.rotation = Vec3::new(model_box.rotation.x, -model_box.rotation.y, -model_box.rotation.z);
    model_box.skin = match model_box.skin {
        BoxSkin::Unfolded { at, mirror } => BoxSkin::Unfolded { at, mirror: !mirror },
        // The sides change places, and every face runs the other way.
        BoxSkin::Faces(faces) => {
            let turned = |rect: Option<FaceRect>| rect.map(|[left, top, right, bottom]| [right, top, left, bottom]);
            let of = |face: Face| turned(faces[face.index()]);
            let mut mirrored = [None; 6];
            for face in Face::ALL {
                mirrored[face.index()] = match face {
                    Face::Right => of(Face::Left),
                    Face::Left => of(Face::Right),
                    face => of(face),
                };
            }
            BoxSkin::Faces(mirrored)
        }
    };
}

/// A bone's name for the other side of the body: Left for Right and Right
/// for Left, however it is written; itself for a name with neither.
fn other_side(name: &str) -> String {
    for (one, other) in [("Left", "Right"), ("left", "right"), ("LEFT", "RIGHT")] {
        if name.contains(one) {
            return name.replacen(one, other, 1);
        }
        if name.contains(other) {
            return name.replacen(other, one, 1);
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body with a head, and a right arm with a hand.
    fn figure() -> BoxModel {
        let mut model = BoxModel::default();
        let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
        model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
        let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
        model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
        let arm = model.add_bone("RightArm", Some(body), Vec3::new(6.0, 10.0, 0.0));
        model.add_box(arm, Vec3::new(-2.0, -10.0, -2.0), [4, 12, 4], [40, 16]);
        let hand = model.add_bone("RightHand", Some(arm), Vec3::new(0.0, -10.0, 0.0));
        model.add_box(hand, Vec3::new(-1.0, -2.0, -1.0), [2, 2, 2], [56, 16]);
        model
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    /// Where every box's corners stand, by its bone's name.
    fn standing(model: &BoxModel) -> Vec<(String, [Vec3; 8])> {
        let bones = model.rest_matrices();
        let mut found = Vec::new();
        for (bone, in_model) in model.bones.iter().zip(&bones) {
            found.extend(bone.boxes.iter().map(|b| (bone.name.clone(), model.box_corners(in_model, b))));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    fn same_standing(a: &BoxModel, b: &BoxModel) -> bool {
        let (a, b) = (standing(a), standing(b));
        a.len() == b.len()
            && a.iter().zip(&b).all(|(a, b)| a.0 == b.0 && a.1.iter().zip(&b.1).all(|(a, b)| close(*a, *b)))
    }

    #[test]
    fn a_ray_meets_the_nearest_box_by_the_face_and_pixel_it_goes_in_at() {
        let model = figure();
        let bones = model.rest_matrices();
        // From in front, at the head's upper corner on the model's right: the first pixel of its front.
        let hit = model.pick(&bones, Vec3::new(0.22, 1.97, 5.0), Vec3::NEG_Z).unwrap();
        assert_eq!((hit.bone, hit.index, hit.face), (1, 0, Face::Front));
        assert!((hit.distance - 4.75).abs() < 1e-4 && close(hit.point, Vec3::new(0.22, 1.97, 0.25)));
        assert_eq!(hit.pixel, Some([8, 8]), "the front's upper left, on an unfolded head at the sheet's corner");
        // The other corner of the same face.
        let hit = model.pick(&bones, Vec3::new(-0.22, 1.53, 5.0), Vec3::NEG_Z).unwrap();
        assert_eq!(hit.pixel, Some([15, 15]));
        // From above, the head's top: its row nearest the front is its lowest on the sheet.
        let hit = model.pick(&bones, Vec3::new(0.22, 5.0, 0.22), Vec3::NEG_Y).unwrap();
        assert_eq!((hit.bone, hit.face, hit.pixel), (1, Face::Top, Some([8, 7])));
        // Through the arm to the body behind it, from the side: the arm is nearer.
        let hit = model.pick(&bones, Vec3::new(5.0, 1.0, 0.0), Vec3::NEG_X).unwrap();
        assert_eq!((hit.bone, hit.face), (2, Face::Right));
        assert!(model.pick(&bones, Vec3::new(3.0, 1.0, 5.0), Vec3::NEG_Z).is_none(), "past everything");
        assert!(model.pick(&bones, Vec3::new(0.0, 1.0, -5.0), Vec3::NEG_Z).is_none(), "looking away");

        // A turned bone turns what a ray meets: the head a quarter round shows its right side to the front.
        let mut turned = model.clone();
        turned.bones[1].rotation = Vec3::new(0.0, -90.0, 0.0);
        let hit = turned.pick(&turned.rest_matrices(), Vec3::new(0.0, 1.75, 5.0), Vec3::NEG_Z).unwrap();
        assert_eq!((hit.bone, hit.face), (1, Face::Right));

        // A flat box is met from either side.
        let mut cloth = BoxModel::default();
        let bone = cloth.add_bone("Cloth", None, Vec3::ZERO);
        cloth.add_box(bone, Vec3::new(-4.0, 0.0, 0.0), [8, 8, 0], [0, 0]);
        let bones = cloth.rest_matrices();
        assert_eq!(cloth.pick(&bones, Vec3::new(0.0, 0.25, 5.0), Vec3::NEG_Z).unwrap().face, Face::Front);
        assert_eq!(cloth.pick(&bones, Vec3::new(0.0, 0.25, -5.0), Vec3::Z).unwrap().face, Face::Back);
    }

    #[test]
    fn the_whole_model_is_bounded_in_metres() {
        let (least, most) = figure().bounds().unwrap();
        assert!(close(least, Vec3::new(-0.25, 0.625, -0.25)) && close(most, Vec3::new(0.5, 2.0, 0.25)), "{least} {most}");
        assert!(BoxModel::default().bounds().is_none());
    }

    #[test]
    fn a_bone_taken_out_takes_those_under_it_and_the_rest_keep_their_parents() {
        let mut model = figure();
        model.add_bone("Tail", Some(0), Vec3::ZERO);
        assert_eq!(model.with_children(2), [2, 3]);
        model.remove_bone(2);
        let names: Vec<(&str, Option<usize>)> = model.bones.iter().map(|b| (b.name.as_str(), b.parent)).collect();
        assert_eq!(names, [("Body", None), ("Head", Some(0)), ("Tail", Some(0))]);
        model.remove_bone(0);
        assert!(model.bones.is_empty());
    }

    #[test]
    fn a_bone_hung_from_another_stays_where_it_stood_with_all_it_carries() {
        let before = figure();
        // The hand from the head, which is turned: its pivot and turn change, and nothing moves.
        let mut model = before.clone();
        model.bones[1].rotation = Vec3::new(0.0, 30.0, 0.0);
        let turned = model.clone();
        let hand = model.set_parent(3, Some(1)).unwrap();
        assert_eq!((model.bones[hand].name.as_str(), model.bones[hand].parent), ("RightHand", Some(1)));
        assert!(same_standing(&model, &turned));
        assert!(model.bones.iter().enumerate().all(|(at, bone)| bone.parent.is_none_or(|parent| parent < at)));

        // The body from nothing is as it was; from its own arm it cannot hang.
        let mut model = before.clone();
        assert_eq!(model.set_parent(0, Some(2)), None);
        assert_eq!(model, before);
        // The arm, with its hand, to the top: both stay, and their parents are right.
        let arm = model.set_parent(2, None).unwrap();
        assert_eq!(model.bones[arm].parent, None);
        assert_eq!(model.bones[arm].pivot, Vec3::new(6.0, 22.0, 0.0), "from the model's feet now");
        let hand = model.find_bone("RightHand").unwrap();
        assert_eq!(model.bones[hand].parent, Some(arm));
        assert!(same_standing(&model, &before));
        // And back under the head, after it.
        let arm = model.set_parent(arm, model.find_bone("Head")).unwrap();
        assert!(arm > model.find_bone("Head").unwrap() && same_standing(&model, &before));
        assert!(BoxModel::parse(&model.to_text()).is_ok(), "every parent before its children");
    }

    #[test]
    fn a_box_moved_to_another_bone_stays_where_it_stood() {
        let before = figure();
        let mut model = before.clone();
        let at = model.move_box(1, 0, 2).unwrap();
        assert_eq!((model.bones[1].boxes.len(), model.bones[2].boxes.len(), at), (0, 2, 1));
        assert_eq!(model.bones[2].boxes[1].from, Vec3::new(-10.0, 2.0, -4.0), "the head's box, from the arm's pivot");
        let corners = |m: &BoxModel, bone: usize, index: usize| {
            m.box_corners(&m.rest_matrices()[bone], &m.bones[bone].boxes[index])
        };
        assert!(corners(&model, 2, 1).iter().zip(&corners(&before, 1, 0)).all(|(a, b)| close(*a, *b)));
        assert_eq!(model.move_box(1, 0, 2), None, "no such box now");
    }

    #[test]
    fn a_bone_is_copied_with_those_under_it_and_mirrored_across_the_body() {
        let mut model = figure();
        model.add_bone("Tail", Some(0), Vec3::ZERO);
        let copy = model.duplicate_bone(2);
        let names: Vec<(&str, Option<usize>)> = model.bones.iter().map(|b| (b.name.as_str(), b.parent)).collect();
        assert_eq!(
            names,
            [
                ("Body", None),
                ("Head", Some(0)),
                ("RightArm", Some(0)),
                ("RightHand", Some(2)),
                ("RightArm 2", Some(0)),
                ("RightHand 2", Some(4)),
                ("Tail", Some(0))
            ]
        );
        assert_eq!(copy, 4);
        model.remove_bone(copy);

        // The arm leans out and forward; its mirror leans out the other way and forward the same.
        model.bones[2].rotation = Vec3::new(-20.0, 10.0, -15.0);
        let left = model.mirror_bone(2);
        assert_eq!((model.bones[left].name.as_str(), model.bones[left + 1].name.as_str()), ("LeftArm", "LeftHand"));
        assert_eq!(model.bones[left + 1].parent, Some(left));
        assert_eq!(model.bones[left].pivot, Vec3::new(-6.0, 10.0, 0.0));
        assert_eq!(model.bones[left].boxes[0].skin, BoxSkin::Unfolded { at: [40, 16], mirror: true });
        let bones = model.rest_matrices();
        let right = model.box_corners(&bones[2], &model.bones[2].boxes[0]);
        let mirrored = model.box_corners(&bones[left], &model.bones[left].boxes[0]);
        for corner in right {
            let across = Vec3::new(-corner.x, corner.y, corner.z);
            assert!(mirrored.iter().any(|other| close(*other, across)), "{corner} has no mirror");
        }
        assert_eq!(model.mirror_bone(left), left + 2, "and back: a second right arm, numbered");
        assert_eq!(model.bones[left + 2].name, "RightArm 2");

        // A box with a place for each face: its sides change places and every face is turned over.
        let mut faces = [None; 6];
        faces[Face::Right.index()] = Some([0.0, 0.0, 4.0, 8.0]);
        faces[Face::Front.index()] = Some([4.0, 0.0, 12.0, 8.0]);
        let sided =
            ModelBox { skin: BoxSkin::Faces(faces), ..ModelBox::new(Vec3::new(1.0, 0.0, 0.0), [8, 8, 4], [0, 0]) };
        let mirrored = BoxModel::mirrored_box(&sided);
        assert_eq!(mirrored.from, Vec3::new(-9.0, 0.0, 0.0));
        assert_eq!(mirrored.face_rect(Face::Left), Some([4.0, 0.0, 0.0, 8.0]));
        assert_eq!(mirrored.face_rect(Face::Front), Some([12.0, 0.0, 4.0, 8.0]));
        assert_eq!(mirrored.face_rect(Face::Right), None);
    }

    #[test]
    fn a_new_box_is_given_room_on_the_sheet_no_face_uses() {
        let mut model = figure();
        // The head's unfolded faces take 32 × 16 at the corner, but for the two corners of its top row.
        assert_eq!(model.free_place([8, 8]), Some([0, 0]), "the corner beside the head's top is free");
        assert_eq!(model.free_place([24, 8]), Some([24, 0]), "past the head's top");
        assert_eq!(model.free_place([64, 20]), Some([0, 32]), "under everything");
        assert_eq!(model.free_place([65, 1]), None);
        let added = model.add_box_with_room(0, [4, 4, 4]);
        let made = model.bones[0].boxes[added].clone();
        assert_eq!(made.from, Vec3::new(-2.0, 0.0, -2.0), "standing on the bone's pivot");
        let BoxSkin::Unfolded { at, .. } = made.skin else { panic!("unfolded") };
        assert_eq!(at, [24, 0]);
        // Until the sheet is full: then the corner.
        let mut small = BoxModel { sheet: super::super::Sheet::filled(16, 8, [0; 4]), ..BoxModel::default() };
        let bone = small.add_bone("Body", None, Vec3::ZERO);
        small.add_box_with_room(bone, [4, 4, 4]);
        let second = small.add_box_with_room(bone, [4, 4, 4]);
        assert_eq!(small.bones[bone].boxes[second].skin, BoxSkin::Unfolded { at: [0, 0], mirror: false });
    }
}
