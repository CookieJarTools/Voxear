//! Painting a box model's sheet, and laying its boxes out on it again:
//! filling the pixels joined to one, the colours a sheet has, the part of
//! the sheet a face takes, and room found afresh for every unfolded box,
//! each taking its pixels with it, on a sheet that grows when it is full.

use std::collections::HashMap;

use super::{BoxModel, BoxSkin, Face, LARGEST_SHEET, ModelBox, Sheet};

impl Sheet {
    /// Colours every pixel joined to `at` through pixels of its own colour,
    /// no further than `within`, a part of the sheet by its left, top,
    /// width and height. Returns whether any pixel changed.
    pub fn flood(&mut self, within: [u32; 4], at: [u32; 2], color: [u8; 4]) -> bool {
        let [left, top, width, height] = within;
        let (right, bottom) = ((left + width).min(self.width), (top + height).min(self.height));
        let inside = |[x, y]: [u32; 2]| (left..right).contains(&x) && (top..bottom).contains(&y);
        let was = self.get(at[0], at[1]);
        if !inside(at) || was == color {
            return false;
        }
        let mut waiting = vec![at];
        while let Some([x, y]) = waiting.pop() {
            if !inside([x, y]) || self.get(x, y) != was {
                continue;
            }
            self.set(x, y, color);
            waiting.extend([[x + 1, y], [x, y + 1]]);
            waiting.extend(x.checked_sub(1).map(|x| [x, y]));
            waiting.extend(y.checked_sub(1).map(|y| [x, y]));
        }
        true
    }

    /// The colours its solid pixels have, the most used first, `most` of
    /// them at most.
    pub fn colors(&self, most: usize) -> Vec<[u8; 4]> {
        let mut counts: HashMap<[u8; 4], (usize, usize)> = HashMap::new();
        for (at, pixel) in self.rgba.chunks_exact(4).enumerate().filter(|(_, pixel)| pixel[3] >= 128) {
            // Each with where it is first met, so colours used as much keep their order.
            counts.entry([pixel[0], pixel[1], pixel[2], pixel[3]]).or_insert((0, at)).0 += 1;
        }
        let mut colors: Vec<([u8; 4], (usize, usize))> = counts.into_iter().collect();
        colors.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.1.1.cmp(&b.1.1)));
        colors.into_iter().take(most).map(|(color, _)| color).collect()
    }

    /// Copies a part of another sheet, by its left, top, width and height,
    /// to `to` on this one; what would fall off either sheet is left.
    pub fn copy_from(&mut self, from: &Sheet, [left, top, width, height]: [u32; 4], to: [u32; 2]) {
        for y in 0..height {
            for x in 0..width {
                if left + x < from.width && top + y < from.height {
                    self.set(to[0] + x, to[1] + y, from.get(left + x, top + y));
                }
            }
        }
    }
}

impl ModelBox {
    /// The part of the sheet a face's pixels take, by its left, top, width
    /// and height in whole pixels; `None` for a face that is not drawn.
    pub fn face_place(&self, face: Face) -> Option<[u32; 4]> {
        self.face_rect(face).map(whole_place)
    }

    /// The part of the sheet a face of its layer takes; `None` for a box
    /// with no layer, or a face its layer leaves out.
    pub fn layer_place(&self, face: Face) -> Option<[u32; 4]> {
        self.layer_rect(face).map(whole_place)
    }

    /// The pixel of its layer that lies over a pixel of its own faces, on
    /// `face`: the same way in from the face's corner. `None` for a box
    /// with no layer there, or a pixel that is not of that face.
    pub fn layer_pixel(&self, face: Face, [x, y]: [u32; 2]) -> Option<[u32; 2]> {
        let ([own_x, own_y, width, height], [left, top, ..]) = (self.face_place(face)?, self.layer_place(face)?);
        let (across, down) = (x.checked_sub(own_x)?, y.checked_sub(own_y)?);
        (across < width && down < height).then_some([left + across, top + down])
    }

    /// The two parts of the sheet its unfolded faces take: its top and
    /// bottom side by side, and the row of its sides under them.
    fn unfolded_places(&self, [u, v]: [u32; 2]) -> [[u32; 4]; 2] {
        let [w, h, d] = self.size;
        [[u + d, v, 2 * w, d], [u, v + d, 2 * (w + d), h]]
    }
}

/// A face's place on the sheet as whole pixels: its left, top, width and
/// height, whichever way round it is drawn.
fn whole_place([left, top, right, bottom]: super::FaceRect) -> [u32; 4] {
    let (x, y) = (left.min(right).floor().max(0.0), top.min(bottom).floor().max(0.0));
    [x as u32, y as u32, (left - right).abs().ceil() as u32, (top - bottom).abs().ceil() as u32]
}

/// Room for `width` × `height` pixels on a sheet of `sheet` pixels that
/// none of `used` takes, each its left, top, width and height: the nearest
/// the top, then the left.
pub(super) fn first_free(used: &[[u32; 4]], [width, height]: [u32; 2], sheet: [u32; 2]) -> Option<[u32; 2]> {
    if width == 0 || height == 0 {
        return Some([0, 0]);
    }
    for y in 0..=sheet[1].checked_sub(height)? {
        let mut x = 0;
        while x + width <= sheet[0] {
            let over =
                |[left, top, w, h]: &&[u32; 4]| x < left + w && *left < x + width && y < top + h && *top < y + height;
            match used.iter().find(over) {
                // Past what is in the way.
                Some([left, _, w, _]) => x = left + w,
                None => return Some([x, y]),
            }
        }
    }
    None
}

impl BoxModel {
    /// Lays every unfolded box out on the sheet again, the tallest first,
    /// each taking its pixels with it, so none lies over another: for
    /// after boxes have been sized or added where the sheet had no room.
    /// Boxes that shared their pixels still do. Faces placed one by one
    /// stay where they are. The sheet doubles, its shorter side first,
    /// while it is too small, as far as it may. Returns whether anything
    /// moved; `false`, with nothing changed, where no sheet is large
    /// enough.
    pub fn lay_out(&mut self) -> bool {
        // Boxes' own faces and their layers' by where they are unfolded and their size: those sharing both share
        // their pixels, own faces with own faces and layers with layers. Each is a box's own faces, or its layer's.
        let mut shared: Vec<([u32; 2], [u32; 3], Vec<(usize, usize, bool)>)> = Vec::new();
        let mut placed: Vec<[u32; 4]> = Vec::new();
        for (bone, index, model_box) in self.boxes() {
            let skins = [(model_box.skin, false)].into_iter().chain(model_box.layer.map(|layer| (layer.skin, true)));
            for (skin, layer) in skins {
                match skin {
                    BoxSkin::Unfolded { at, .. } => {
                        let same = |(other, size, members): &&mut ([u32; 2], [u32; 3], Vec<(usize, usize, bool)>)| {
                            *other == at && *size == model_box.size && members[0].2 == layer
                        };
                        match shared.iter_mut().find(same) {
                            Some((_, _, members)) => members.push((bone, index, layer)),
                            None => shared.push((at, model_box.size, vec![(bone, index, layer)])),
                        }
                    }
                    BoxSkin::Faces(_) => {
                        let rect = |face: &Face| model_box.skin_rect(skin, *face);
                        placed.extend(Face::ALL.iter().filter_map(rect).map(whole_place));
                    }
                }
            }
        }
        let unfolded = |size: [u32; 3]| [2 * (size[0] + size[2]), size[1] + size[2]];
        shared.sort_by_key(|(_, size, _)| std::cmp::Reverse((unfolded(*size)[1], unfolded(*size)[0])));

        let mut sheet = [self.sheet.width, self.sheet.height];
        let places = loop {
            let mut used = placed.clone();
            let found: Option<Vec<[u32; 2]>> = shared
                .iter()
                .map(|(_, size, _)| {
                    let [width, height] = unfolded(*size);
                    let at = first_free(&used, [width, height], sheet)?;
                    used.push([at[0], at[1], width, height]);
                    Some(at)
                })
                .collect();
            if let Some(found) = found {
                break found;
            }
            let shorter = if sheet[0] <= sheet[1] { 0 } else { 1 };
            let grown = if sheet[shorter] * 2 <= LARGEST_SHEET { shorter } else { 1 - shorter };
            if sheet[grown] * 2 > LARGEST_SHEET {
                return false;
            }
            sheet[grown] *= 2;
        };

        let moved = sheet != [self.sheet.width, self.sheet.height]
            || shared.iter().zip(&places).any(|((at, _, _), place)| at != place);
        if !moved {
            return false;
        }
        let mut laid = Sheet::filled(sheet[0], sheet[1], [0; 4]);
        for place in &placed {
            laid.copy_from(&self.sheet, *place, [place[0], place[1]]);
        }
        for ((at, size, members), place) in shared.iter().zip(&places) {
            let sized = ModelBox::new(Default::default(), *size, *at);
            for (from, to) in sized.unfolded_places(*at).into_iter().zip(sized.unfolded_places(*place)) {
                laid.copy_from(&self.sheet, from, [to[0], to[1]]);
            }
            for &(bone, index, layer) in members {
                let model_box = &mut self.bones[bone].boxes[index];
                let skin = match (layer, model_box.layer.as_mut()) {
                    (true, Some(layer)) => &mut layer.skin,
                    _ => &mut model_box.skin,
                };
                if let BoxSkin::Unfolded { mirror, .. } = *skin {
                    *skin = BoxSkin::Unfolded { at: *place, mirror };
                }
            }
        }
        self.sheet = laid;
        true
    }

    /// Every box, with its bone and its place among the bone's boxes.
    pub fn boxes(&self) -> impl Iterator<Item = (usize, usize, &ModelBox)> {
        self.bones
            .iter()
            .enumerate()
            .flat_map(|(bone, of)| of.boxes.iter().enumerate().map(move |(index, model_box)| (bone, index, model_box)))
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Vec3;

    use super::*;

    #[test]
    fn a_fill_stops_at_other_colours_and_at_the_edge_it_is_given() {
        let mut sheet = Sheet::filled(8, 8, [10, 10, 10, 255]);
        // A wall down the middle, with a gap outside the part filled.
        (0..6).for_each(|y| sheet.set(4, y, [200, 0, 0, 255]));
        assert!(sheet.flood([0, 0, 8, 6], [1, 1], [0, 0, 255, 255]));
        assert_eq!(sheet.get(3, 5), [0, 0, 255, 255]);
        assert_eq!(sheet.get(4, 2), [200, 0, 0, 255], "the wall");
        assert_eq!(sheet.get(5, 2), [10, 10, 10, 255], "past the wall");
        assert_eq!(sheet.get(1, 6), [10, 10, 10, 255], "under the part filled");
        assert!(!sheet.flood([0, 0, 8, 6], [1, 1], [0, 0, 255, 255]), "its own colour already");
        assert!(!sheet.flood([0, 0, 4, 4], [6, 6], [9, 9, 9, 255]), "from outside the part");
    }

    #[test]
    fn a_sheets_colours_come_the_most_used_first_without_its_holes() {
        let mut sheet = Sheet::filled(4, 4, [0; 4]);
        sheet.fill([0, 0], [4, 2], [1, 2, 3, 255]);
        sheet.fill([0, 2], [2, 1], [9, 9, 9, 255]);
        sheet.set(3, 3, [5, 5, 5, 255]);
        assert_eq!(sheet.colors(8), [[1, 2, 3, 255], [9, 9, 9, 255], [5, 5, 5, 255]]);
        assert_eq!(sheet.colors(1), [[1, 2, 3, 255]]);
    }

    #[test]
    fn a_face_takes_a_part_of_the_sheet_in_whole_pixels() {
        let body = ModelBox::new(Vec3::ZERO, [8, 12, 4], [16, 16]);
        assert_eq!(body.face_place(Face::Front), Some([20, 20, 8, 12]));
        let mirrored = ModelBox { skin: BoxSkin::Unfolded { at: [16, 16], mirror: true }, ..body };
        assert_eq!(mirrored.face_place(Face::Front), Some([20, 20, 8, 12]), "turned over, in the same place");
    }

    /// The pixels each face of each box shows, row by row.
    fn shown(model: &BoxModel) -> Vec<Vec<[u8; 4]>> {
        let mut faces = Vec::new();
        for (_, _, model_box) in model.boxes() {
            for face in Face::ALL {
                let Some([left, top, width, height]) = model_box.face_place(face) else { continue };
                let pixels = (0..height).flat_map(|y| (0..width).map(move |x| (left + x, top + y)));
                faces.push(pixels.map(|(x, y)| model.sheet.get(x, y)).collect());
            }
        }
        faces
    }

    #[test]
    fn boxes_are_laid_out_again_with_their_pixels_and_the_sheet_grows_to_hold_them() {
        // Three boxes unfolded over one another on a small sheet, two of them sharing their pixels.
        let mut model = BoxModel { sheet: Sheet::filled(16, 16, [0; 4]), ..BoxModel::default() };
        let body = model.add_bone("Body", None, Vec3::ZERO);
        model.add_box(body, Vec3::ZERO, [4, 4, 4], [0, 0]);
        model.add_box(body, Vec3::ZERO, [2, 6, 2], [8, 8]);
        model.add_box(body, Vec3::ZERO, [2, 6, 2], [8, 8]).skin = BoxSkin::Unfolded { at: [8, 8], mirror: true };
        // Each pixel its own colour, so a pixel moved wrongly shows.
        for at in 0..16 * 16u32 {
            model.sheet.set(at % 16, at / 16, [(at % 16) as u8 * 16, (at / 16) as u8 * 16, 77, 255]);
        }
        let mut big = model.clone();
        big.bones[body].boxes[0].size = [8, 8, 8];
        let before = shown(&big);
        assert!(big.lay_out());
        assert_eq!((big.sheet.width, big.sheet.height), (32, 32), "grown to hold a box of 32 × 16 and one of 8 × 8");
        let places = big.used_places();
        for (at, a) in places.iter().enumerate() {
            for b in &places[at + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart || a == b, "{a:?} lies over {b:?}");
            }
        }
        // Each face shows what it showed, where the old sheet had pixels for it.
        for (was, now) in before.iter().zip(shown(&big)) {
            let kept = was.iter().zip(&now).filter(|(was, _)| was[3] != 0).all(|(was, now)| was == now);
            assert!(kept);
        }
        let arms = &big.bones[body].boxes;
        assert_eq!(arms[1].face_place(Face::Front), arms[2].face_place(Face::Front), "the two still share");
        assert_ne!(arms[1].skin, arms[2].skin, "one of them mirrored");

        // Laid out already, nothing moves; and with room for everything where it is, nothing does either.
        assert!(!big.clone().lay_out());
        let mut apart = BoxModel::default();
        let bone = apart.add_bone("Body", None, Vec3::ZERO);
        apart.add_box(bone, Vec3::ZERO, [8, 8, 8], [0, 0]);
        assert!(!apart.lay_out());
    }

    #[test]
    fn a_layer_is_given_room_of_its_own_and_its_pixels_lie_over_the_boxs_own() {
        let mut model = BoxModel { sheet: Sheet::filled(32, 16, [0; 4]), ..BoxModel::default() };
        let bone = model.add_bone("Head", None, Vec3::ZERO);
        // A box and its layer unfolded in the same place, as a layer just added without room would be.
        let added = model.add_box(bone, Vec3::ZERO, [4, 4, 4], [0, 0]);
        added.layer =
            Some(super::super::BoxLayer { skin: BoxSkin::Unfolded { at: [0, 0], mirror: false }, height: 1.0 });
        model.sheet.set(5, 5, [200, 100, 50, 255]);
        assert!(model.lay_out());
        let laid = &model.bones[bone].boxes[0];
        let layer = laid.layer.unwrap();
        assert_ne!(BoxSkin::Unfolded { at: [0, 0], mirror: false }, layer.skin, "one of them moved");
        assert_eq!(model.free_place([16, 8]), Some([0, 8]), "side by side along the top, with room under them");
        // The front's second pixel in and down: the layer's is the same way in from its own front's corner.
        let [x, y, ..] = laid.face_place(Face::Front).unwrap();
        let [left, top, ..] = laid.layer_place(Face::Front).unwrap();
        assert_eq!(laid.layer_pixel(Face::Front, [x + 1, y + 1]), Some([left + 1, top + 1]));
        assert_eq!(laid.layer_pixel(Face::Front, [x + 4, y]), None, "not a pixel of that face");
        assert_eq!(ModelBox::new(Vec3::ZERO, [4, 4, 4], [0, 0]).layer_pixel(Face::Front, [5, 5]), None);
    }

    #[test]
    fn faces_placed_one_by_one_stay_where_they_are_when_the_rest_is_laid_out() {
        let mut model = BoxModel { sheet: Sheet::filled(32, 16, [0; 4]), ..BoxModel::default() };
        let bone = model.add_bone("Body", None, Vec3::ZERO);
        let mut faces = [None; 6];
        faces[Face::Front.index()] = Some([0.0, 0.0, 8.0, 8.0]);
        model.add_box(bone, Vec3::ZERO, [8, 8, 0], [0, 0]).skin = BoxSkin::Faces(faces);
        model.add_box(bone, Vec3::ZERO, [2, 2, 2], [0, 0]);
        model.sheet.set(3, 3, [200, 100, 50, 255]);
        assert!(model.lay_out());
        assert_eq!(model.bones[bone].boxes[0].skin, BoxSkin::Faces(faces));
        assert_eq!(model.sheet.get(3, 3), [200, 100, 50, 255], "its pixels stay");
        assert_eq!(model.bones[bone].boxes[1].skin, BoxSkin::Unfolded { at: [8, 0], mirror: false }, "beside it");
    }
}
