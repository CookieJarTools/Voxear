//! Boxes sculpted from cubes: a box that has [`Voxels`] is not drawn as six
//! faces from the sheet but as the small cubes that fill it, one to each
//! pixel of its size, each one colour or left out. It is how rounded and
//! stepped shapes are made: hair, ears, hands, leaves, the crown of a tree.
//!
//! A cube's colour is a place in the model's palette, which every sculpted
//! box of the model shares, so a model's colours can be changed in one
//! place. The cubes are built into the model's one mesh with everything
//! else: only the faces that show, those side by side of one colour as one,
//! bound whole to the box's bone, and coloured from the palette's pixels,
//! which stand in the model's texture under its sheet.
//!
//! In the file a sculpted box has its cubes as text, a byte a cube,
//! compressed, and the model its palette as colours:
//!
//! ```toml
//! palette = ["e25454", "f0c8a0"]
//!
//! [[bones.boxes]]
//! from = [-5.0, 0.0, -5.0]
//! size = [10, 10, 10]
//! voxels = "…"
//! ```

use anyhow::{Context, Result, ensure};
use voxear_core::math::{Mat4, Vec3};

use super::{BoxHit, BoxModel, BoxSkin, Face, ModelBox, Sheet, turned};
use crate::{
    mesh::{MeshData, Vertex},
    skeleton::SkinVertex,
};

/// The most colours a model's palette has, and the most cubes a sculpted
/// box measures along a side.
pub const MOST_COLORS: usize = 255;
pub const LARGEST_GRID: u32 = 64;

/// The colour cubes get where the box they are made from has no pixels to
/// take one from.
const PLAIN: [u8; 4] = [200, 200, 200, 255];

/// A sculpted box's cubes: for each, 0 where there is none, or its colour's
/// place in the model's palette and one. They run along X, then Y, then Z,
/// from the box's least corner, as many each way as the box's size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voxels {
    pub cells: Vec<u8>,
}

fn count(size: [u32; 3]) -> usize {
    size.iter().map(|side| *side as usize).product()
}

impl Voxels {
    /// Every cube of a box of `size` there, in one colour.
    pub fn filled(size: [u32; 3], color: u8) -> Self {
        Self { cells: vec![color; count(size)] }
    }

    /// The cube at `cell` of a box of `size`: 0 for none, and for one
    /// outside the box.
    pub fn get(&self, size: [u32; 3], cell: [i32; 3]) -> u8 {
        if (0..3).any(|axis| cell[axis] < 0 || cell[axis] >= size[axis] as i32) {
            return 0;
        }
        let at = cell[0] as usize + size[0] as usize * (cell[1] as usize + size[1] as usize * cell[2] as usize);
        self.cells.get(at).copied().unwrap_or(0)
    }

    /// Colours the cube at `cell`, or with 0 takes it out; one outside the
    /// box is left.
    pub fn set(&mut self, size: [u32; 3], cell: [i32; 3], color: u8) {
        if (0..3).any(|axis| cell[axis] < 0 || cell[axis] >= size[axis] as i32) {
            return;
        }
        let at = cell[0] as usize + size[0] as usize * (cell[1] as usize + size[1] as usize * cell[2] as usize);
        if let Some(slot) = self.cells.get_mut(at) {
            *slot = color;
        }
    }

    /// How many cubes there are.
    pub fn solid(&self) -> usize {
        self.cells.iter().filter(|cell| **cell != 0).count()
    }

    /// The cubes of a box of `size` as those of one of `to`, each `shift`
    /// cubes further along; what falls outside is lost.
    pub fn resized(&self, size: [u32; 3], to: [u32; 3], shift: [i32; 3]) -> Self {
        let mut made = Self::filled(to, 0);
        for z in 0..to[2] as i32 {
            for y in 0..to[1] as i32 {
                for x in 0..to[0] as i32 {
                    let from = [x - shift[0], y - shift[1], z - shift[2]];
                    made.set(to, [x, y, z], self.get(size, from));
                }
            }
        }
        made
    }

    /// The cubes turned over from left to right.
    pub fn mirrored(&self, size: [u32; 3]) -> Self {
        let mut made = Self::filled(size, 0);
        for z in 0..size[2] as i32 {
            for y in 0..size[1] as i32 {
                for x in 0..size[0] as i32 {
                    made.set(size, [size[0] as i32 - 1 - x, y, z], self.get(size, [x, y, z]));
                }
            }
        }
        made
    }

    /// The cubes as the file keeps them: compressed, as base64.
    pub fn to_text(&self) -> String {
        base64::encode(lz4_flex::compress_prepend_size(&self.cells))
    }

    /// The cubes of a box of `size` from [`to_text`](Self::to_text)'s text.
    pub fn from_text(size: [u32; 3], text: &str) -> Result<Self> {
        ensure!(
            size.iter().all(|side| (1..=LARGEST_GRID).contains(side)),
            "a sculpted box of {} × {} × {}: each side is from 1 to {LARGEST_GRID}",
            size[0],
            size[1],
            size[2]
        );
        let packed = base64::decode(text.trim()).context("the cubes are not base64")?;
        let cells = lz4_flex::decompress_size_prepended(&packed).context("the cubes could not be unpacked")?;
        ensure!(cells.len() == count(size), "{} cubes for a box of {}", cells.len(), count(size));
        Ok(Self { cells })
    }
}

/// A colour as the file has it: its red, green and blue as six letters,
/// and how solid as two more where it is not wholly so.
pub(super) fn color_text([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 { format!("{r:02x}{g:02x}{b:02x}") } else { format!("{r:02x}{g:02x}{b:02x}{a:02x}") }
}

pub(super) fn text_color(text: &str) -> Result<[u8; 4]> {
    let text = text.trim().trim_start_matches('#');
    ensure!(matches!(text.len(), 6 | 8) && text.is_ascii(), "{text} is not a colour as rrggbb");
    let part = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).with_context(|| format!("{text} is not a colour"));
    Ok([part(0)?, part(2)?, part(4)?, if text.len() == 8 { part(6)? } else { 255 }])
}

impl ModelBox {
    /// Makes a side of the box `length` pixels long: X, Y or Z by `axis`.
    /// With `low`, its low face is the one moved, and what it holds stays
    /// where it is by the other; the corner the box is measured from is
    /// left for whoever moves the face to move. A sculpted box keeps its
    /// cubes, but those cut off.
    pub fn set_side(&mut self, axis: usize, length: u32, low: bool) {
        let (was, axis) = (self.size, axis.min(2));
        self.size[axis] = length;
        if let Some(voxels) = &mut self.voxels {
            self.size[axis] = length.clamp(1, LARGEST_GRID);
            let mut shift = [0; 3];
            if low {
                shift[axis] = self.size[axis] as i32 - was[axis] as i32;
            }
            *voxels = voxels.resized(was, self.size, shift);
        }
    }
}

impl BoxModel {
    /// The colour of a cube, from the model's palette.
    pub fn voxel_color(&self, cell: u8) -> [u8; 4] {
        self.palette.get(usize::from(cell).wrapping_sub(1)).copied().unwrap_or(PLAIN)
    }

    /// What a cube of `color` is kept as: its place in the palette and one.
    /// A colour the palette has not is added to it; once it is full, the
    /// nearest it has stands for it.
    pub fn color_index(&mut self, color: [u8; 4]) -> u8 {
        let color = [color[0], color[1], color[2], 255];
        if let Some(at) = self.palette.iter().position(|other| *other == color) {
            return at as u8 + 1;
        }
        if self.palette.len() < MOST_COLORS {
            self.palette.push(color);
            return self.palette.len() as u8;
        }
        let away =
            |other: &[u8; 4]| (0..3).map(|part| (i32::from(other[part]) - i32::from(color[part])).pow(2)).sum::<i32>();
        let nearest = self.palette.iter().enumerate().min_by_key(|(_, other)| away(other));
        nearest.map_or(1, |(at, _)| at as u8 + 1)
    }

    /// Takes the colours no cube has out of the palette.
    pub fn prune_palette(&mut self) {
        let mut used = [false; 256];
        for bone in &self.bones {
            for voxels in bone.boxes.iter().filter_map(|model_box| model_box.voxels.as_ref()) {
                voxels.cells.iter().for_each(|cell| used[usize::from(*cell)] = true);
            }
        }
        if (1..=self.palette.len()).all(|cell| used[cell]) {
            return;
        }
        let mut now = [0u8; 256];
        let mut kept = Vec::new();
        for (at, color) in self.palette.iter().enumerate().filter(|(at, _)| used[at + 1]) {
            kept.push(*color);
            now[at + 1] = kept.len() as u8;
        }
        self.palette = kept;
        for bone in &mut self.bones {
            for voxels in bone.boxes.iter_mut().filter_map(|model_box| model_box.voxels.as_mut()) {
                voxels.cells.iter_mut().for_each(|cell| *cell = now[usize::from(*cell)]);
            }
        }
    }

    /// The model's one texture: its sheet, and under it, where the model
    /// has a palette, the palette's colours a pixel each, row by row.
    pub fn texture(&self) -> Sheet {
        let mut texture = self.sheet.clone();
        if self.palette.is_empty() {
            return texture;
        }
        texture.height += self.palette_rows();
        texture.rgba.resize((texture.width * texture.height * 4) as usize, 0);
        for (at, color) in self.palette.iter().enumerate() {
            let [x, y] = self.palette_pixel(at as u8 + 1);
            texture.set(x, y, *color);
        }
        texture
    }

    fn palette_rows(&self) -> u32 {
        (self.palette.len() as u32).div_ceil(self.sheet.width.max(1))
    }

    /// The pixel of the texture a cube's colour is at.
    fn palette_pixel(&self, cell: u8) -> [u32; 2] {
        let at = u32::from(cell).saturating_sub(1);
        let width = self.sheet.width.max(1);
        [at % width, self.sheet.height + at / width]
    }

    /// How tall the model's texture is, in pixels.
    pub(super) fn texture_height(&self) -> u32 {
        self.sheet.height + if self.palette.is_empty() { 0 } else { self.palette_rows() }
    }

    /// Makes a box one sculpted from cubes: as many as it has pixels each
    /// way, each the colour of the pixel of the box's nearest face to it,
    /// so it looks as it did. Its faces leave the sheet, and its layer
    /// goes. False for a box already sculpted, or one there is not.
    pub fn to_voxels(&mut self, bone: usize, index: usize) -> bool {
        let Some(model_box) = self.bones.get(bone).and_then(|bone| bone.boxes.get(index)) else { return false };
        if model_box.voxels.is_some() {
            return false;
        }
        let was = model_box.clone();
        let size = was.size.map(|side| side.clamp(1, LARGEST_GRID));
        let mut voxels = Voxels::filled(size, 0);
        let reach = Vec3::from(was.size.map(|side| (side as f32 * 0.5).max(1e-3)));
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let cell = [x, y, z];
                    // Its middle, from -1 to 1 of the box's reach each way.
                    let middle = Vec3::from(std::array::from_fn(|axis| (cell[axis] as f32 + 0.5) / size[axis] as f32))
                        * 2.0
                        - Vec3::ONE;
                    // The faces, the nearest first, and the pixel of each the cube lies under.
                    let mut faces = Face::ALL;
                    faces.sort_by(|a, b| (1.0 - a.normal().dot(middle)).total_cmp(&(1.0 - b.normal().dot(middle))));
                    let seen = faces.iter().find_map(|face| {
                        let rect = was.face_rect(*face)?;
                        let normal = face.normal();
                        let on_face = (middle - normal * normal.dot(middle) + normal) * reach;
                        let [px, py] = super::edit::pixel_at(rect, *face, on_face, reach);
                        Some(self.sheet.get(px, py)).filter(|color| color[3] >= 128)
                    });
                    let color = self.color_index(seen.unwrap_or(PLAIN));
                    voxels.set(size, [x as i32, y as i32, z as i32], color);
                }
            }
        }
        let model_box = &mut self.bones[bone].boxes[index];
        model_box.size = size;
        model_box.skin = BoxSkin::Faces([None; 6]);
        (model_box.layer, model_box.voxels) = (None, Some(voxels));
        true
    }

    /// Makes a sculpted box a plain one again, of its size, its faces
    /// unfolded where the sheet has room. Its cubes are lost.
    pub fn from_voxels(&mut self, bone: usize, index: usize) -> bool {
        let Some(model_box) = self.bones.get(bone).and_then(|bone| bone.boxes.get(index)) else { return false };
        if model_box.voxels.is_none() {
            return false;
        }
        let at = self.free_place(model_box.unfolded_size()).unwrap_or([0, 0]);
        let model_box = &mut self.bones[bone].boxes[index];
        (model_box.voxels, model_box.skin) = (None, BoxSkin::Unfolded { at, mirror: false });
        self.prune_palette();
        true
    }

    /// Colours one cube of a sculpted box, or with 0 takes it out. A cube
    /// put outside the box makes the box as much larger as holds it, what
    /// it has staying where it is; how far the box's cubes moved along in
    /// it for that is returned, each of them now that much further from
    /// its least corner. Nothing is done past the largest a box gets.
    pub fn set_voxel(&mut self, bone: usize, index: usize, cell: [i32; 3], color: u8) -> [i32; 3] {
        let Some(model_box) = self.bones.get_mut(bone).and_then(|bone| bone.boxes.get_mut(index)) else {
            return [0; 3];
        };
        let Some(voxels) = &mut model_box.voxels else { return [0; 3] };
        let size = model_box.size;
        let below = cell.map(|at| (-at).max(0));
        let above: [i32; 3] = std::array::from_fn(|axis| (cell[axis] + 1 - size[axis] as i32).max(0));
        if below == [0; 3] && above == [0; 3] {
            voxels.set(size, cell, color);
            return [0; 3];
        }
        let grown: [u32; 3] = std::array::from_fn(|axis| size[axis] + (below[axis] + above[axis]) as u32);
        if color == 0 || grown.iter().any(|side| *side > LARGEST_GRID) {
            return [0; 3];
        }
        *voxels = voxels.resized(size, grown, below);
        voxels.set(grown, std::array::from_fn(|axis| cell[axis] + below[axis]), color);
        // Its middle moves as it grows: the corner it is measured from goes where that leaves every cube in place.
        let more = Vec3::from(grown.map(|side| side as f32)) - Vec3::from(size.map(|side| side as f32));
        let shift = Vec3::from(below.map(|by| by as f32));
        model_box.from += turned(model_box.rotation) * (more * 0.5 - shift) - more * 0.5;
        model_box.size = grown;
        below
    }

    /// The four corners of the face of the cube a ray met, in the model's
    /// space in metres, round it as its face runs. `None` for a hit that
    /// is not on a sculpted box.
    pub fn cube_corners(&self, bones: &[Mat4], hit: &BoxHit) -> Option<[Vec3; 4]> {
        let model_box = self.bones.get(hit.bone)?.boxes.get(hit.index)?;
        let cell = hit.cell?;
        let (place, reach) = self.box_place(bones.get(hit.bone)?, model_box);
        let each = reach * 2.0 / Vec3::from(model_box.size.map(|side| side.max(1) as f32));
        let middle = (Vec3::from(cell.map(|at| at as f32)) + Vec3::splat(0.5)) * each - reach;
        let (normal, (across, down)) = (hit.face.normal(), hit.face.across_and_down());
        let (out, along, fall) = (normal * each * 0.5, across * each * 0.5, down * each * 0.5);
        let corner = |a: f32, d: f32| place.transform_point3(middle + out + along * a + fall * d);
        Some([corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)])
    }
}

/// Where a sculpted box's cubes are built: the box's middle and turn in
/// the model's space, how far its faces stand from its middle, its joint,
/// and how wide and tall the model's texture is.
pub(super) struct Onto {
    pub place: Mat4,
    pub reach: Vec3,
    pub joint: u16,
    pub texture: [f32; 2],
}

/// Builds a sculpted box's cubes into `data`: each face of a cube that has
/// no cube against it, those of one colour side by side as one face.
pub(super) fn build(data: &mut MeshData, model: &BoxModel, model_box: &ModelBox, voxels: &Voxels, on: &Onto) {
    let size = model_box.size;
    let each = on.reach * 2.0 / Vec3::from(size.map(|side| side.max(1) as f32));
    for face in Face::ALL {
        let normal = face.normal();
        let (across, down) = face.across_and_down();
        // The axis the face looks along, and the two it lies across, by number.
        let axis_of = |way: Vec3| way.abs().max_position();
        let (n, u, v) = (axis_of(normal), axis_of(across), axis_of(down));
        let out = normal[n] as i32;
        let world_normal = on.place.transform_vector3(normal).normalize_or_zero();
        for layer in 0..size[n] as i32 {
            // The colour of each cube of this layer whose face this way shows.
            let (wide, tall) = (size[u] as usize, size[v] as usize);
            let mut shown = vec![0u8; wide * tall];
            for b in 0..tall {
                for a in 0..wide {
                    let mut cell = [0i32; 3];
                    (cell[n], cell[u], cell[v]) = (layer, a as i32, b as i32);
                    let color = voxels.get(size, cell);
                    cell[n] += out;
                    if color != 0 && voxels.get(size, cell) == 0 {
                        shown[a + wide * b] = color;
                    }
                }
            }
            // Each as wide a run of one colour as there is, then as tall as that run goes on.
            for b in 0..tall {
                let mut a = 0;
                while a < wide {
                    let color = shown[a + wide * b];
                    if color == 0 {
                        a += 1;
                        continue;
                    }
                    let run = (a..wide).take_while(|at| shown[at + wide * b] == color).count();
                    let rows =
                        (b..tall).take_while(|row| (a..a + run).all(|at| shown[at + wide * row] == color)).count();
                    for row in b..b + rows {
                        shown[a + wide * row..a + run + wide * row].fill(0);
                    }
                    // Its middle, and half its width and height, in the box's own space.
                    let mut middle = Vec3::ZERO;
                    middle[n] = (layer as f32 + 0.5 + 0.5 * out as f32) * each[n] - on.reach[n];
                    middle[u] = (a as f32 + run as f32 * 0.5) * each[u] - on.reach[u];
                    middle[v] = (b as f32 + rows as f32 * 0.5) * each[v] - on.reach[v];
                    let along = across * (run as f32 * 0.5 * each[u]);
                    let fall = down * (rows as f32 * 0.5 * each[v]);
                    let [x, y] = model.palette_pixel(color);
                    let uv = [(x as f32 + 0.5) / on.texture[0], (y as f32 + 0.5) / on.texture[1]];
                    let first = data.vertices.len() as u32;
                    for corner in [-along - fall, along - fall, along + fall, fall - along] {
                        data.vertices.push(Vertex {
                            position: on.place.transform_point3(middle + corner).into(),
                            normal: world_normal.into(),
                            uv,
                            ..Vertex::default()
                        });
                        data.skin.push(SkinVertex { joints: [on.joint, 0, 0, 0], weights: [u16::MAX, 0, 0, 0] });
                    }
                    data.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
                    a += run;
                }
            }
        }
    }
}

/// Where a ray first meets a cube of a sculpted box: how far along the
/// ray, the face of the cube it goes in by, and the cube. The ray is in the
/// box's own space, from `from` along `along`; `reach` is how far the
/// box's faces stand from its middle.
pub(super) fn enter(
    voxels: &Voxels,
    size: [u32; 3],
    from: Vec3,
    along: Vec3,
    reach: Vec3,
) -> Option<(f32, Face, [u32; 3])> {
    // In cubes from the box's least corner.
    let sides = Vec3::from(size.map(|side| side.max(1) as f32));
    let scale = sides / (reach * 2.0);
    let (origin, way) = ((from + reach) * scale, along * scale);
    let (mut near, mut far, mut by) = (0.0f32, f32::INFINITY, None);
    for axis in 0..3 {
        if way[axis].abs() < 1e-9 {
            if origin[axis] < 0.0 || origin[axis] > sides[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = (-origin[axis] / way[axis], (sides[axis] - origin[axis]) / way[axis]);
        if a.min(b) > near {
            (near, by) = (a.min(b), Some(axis));
        }
        far = far.min(a.max(b));
    }
    if near > far {
        return None;
    }
    let start = origin + way * (near + 1e-4);
    let mut cell: [i32; 3] = std::array::from_fn(|axis| (start[axis].floor() as i32).clamp(0, size[axis] as i32 - 1));
    let step: [i32; 3] = std::array::from_fn(|axis| if way[axis] >= 0.0 { 1 } else { -1 });
    // How far along the ray the next wall across each axis is, and from one wall to the next.
    let mut next: [f32; 3] = std::array::from_fn(|axis| {
        if way[axis].abs() < 1e-9 {
            return f32::INFINITY;
        }
        let wall = cell[axis] as f32 + if step[axis] > 0 { 1.0 } else { 0.0 };
        (wall - origin[axis]) / way[axis]
    });
    let each: [f32; 3] =
        std::array::from_fn(|axis| if way[axis].abs() < 1e-9 { f32::INFINITY } else { 1.0 / way[axis].abs() });
    let mut distance = near;
    loop {
        if voxels.get(size, cell) != 0 {
            let face = match (by, by.map(|axis| step[axis])) {
                (Some(0), Some(1)) => Face::Left,
                (Some(0), _) => Face::Right,
                (Some(1), Some(1)) => Face::Bottom,
                (Some(1), _) => Face::Top,
                (Some(2), Some(1)) => Face::Back,
                _ => Face::Front,
            };
            return Some((distance, face, cell.map(|at| at as u32)));
        }
        let axis = (0..3).min_by(|a, b| next[*a].total_cmp(&next[*b])).unwrap_or(0);
        distance = next[axis];
        cell[axis] += step[axis];
        if cell[axis] < 0 || cell[axis] >= size[axis] as i32 || !distance.is_finite() {
            return None;
        }
        next[axis] += each[axis];
        by = Some(axis);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [226, 84, 84, 255];
    const BLUE: [u8; 4] = [60, 90, 200, 255];

    /// A model of one bone at the ground with one box of `size` pixels, its least corner at the bone.
    fn boxed(size: [u32; 3]) -> BoxModel {
        let mut model = BoxModel { sheet: Sheet::filled(32, 16, [0; 4]), ..BoxModel::default() };
        let body = model.add_bone("Body", None, Vec3::ZERO);
        model.add_box(body, Vec3::ZERO, size, [0, 0]);
        model
    }

    #[test]
    fn a_box_made_of_cubes_takes_its_colours_from_the_pixels_of_its_faces() {
        let mut model = boxed([4, 4, 2]);
        let plain = model.bones[0].boxes[0].clone();
        // Its front red, its top blue, the rest unpainted.
        for (face, color) in [(Face::Front, RED), (Face::Top, BLUE)] {
            let [left, top, right, bottom] = plain.face_rect(face).unwrap();
            model.sheet.fill([left as u32, top as u32], [(right - left) as u32, (bottom - top) as u32], color);
        }
        assert!(model.to_voxels(0, 0));
        assert!(!model.to_voxels(0, 0), "already sculpted");
        let made = &model.bones[0].boxes[0];
        let voxels = made.voxels.as_ref().unwrap();
        assert_eq!((voxels.solid(), made.size), (32, [4, 4, 2]), "every cube there");
        assert_eq!(made.skin, BoxSkin::Faces([None; 6]), "its faces have left the sheet");
        let color = |cell: [i32; 3]| model.voxel_color(voxels.get(made.size, cell));
        assert_eq!(color([1, 1, 1]), RED, "at the front");
        assert_eq!(color([1, 3, 0]), BLUE, "at the top, nearer it than the back");
        assert_eq!(color([1, 1, 0]), BLUE, "at the back, which has no pixels: from the nearest face that has");
        assert_eq!(model.palette.len(), 2);

        // Plain again, it has faces on the sheet, and the colours no cube has are gone.
        assert!(model.from_voxels(0, 0));
        assert!(model.bones[0].boxes[0].voxels.is_none() && model.palette.is_empty());
        assert!(matches!(model.bones[0].boxes[0].skin, BoxSkin::Unfolded { .. }));
    }

    #[test]
    fn only_the_faces_that_show_are_built_and_those_of_one_colour_side_by_side_are_one() {
        let mut model = boxed([4, 4, 4]);
        model.to_voxels(0, 0);
        let faces = |model: &BoxModel| model.mesh().indices.len() / 6;
        assert_eq!(faces(&model), 6, "a whole block of one colour is six faces");
        let bounds = model.bounds().unwrap();
        assert_eq!(bounds, (Vec3::ZERO, Vec3::splat(0.25)), "as large as the box it was");

        // One cube of another colour in the middle of the front: that face is cut round it.
        let blue = model.color_index(BLUE);
        model.set_voxel(0, 0, [1, 1, 3], blue);
        assert!(faces(&model) > 6 && faces(&model) <= 11, "{}", faces(&model));
        let mesh = model.mesh();
        let texture = model.texture();
        assert_eq!((texture.width, texture.height), (32, 17), "the palette under the sheet");
        assert_eq!(texture.get(1, 16), BLUE);
        let blue_uv = [1.5 / 32.0, 16.5 / 17.0];
        let painted: Vec<&Vertex> = mesh.vertices.iter().filter(|vertex| vertex.uv == blue_uv).collect();
        assert_eq!(painted.len(), 4, "one face of the blue cube shows");
        assert!(painted.iter().all(|vertex| vertex.normal == [0.0, 0.0, 1.0] && vertex.position[2] == 0.25));
        assert_eq!(mesh.skin.len(), mesh.vertices.len(), "every corner bound to the bone");

        // A cube taken out of a corner shows the three faces behind it.
        let whole = faces(&model);
        model.set_voxel(0, 0, [0, 3, 3], 0);
        assert!(faces(&model) > whole);
        // Faces look out: each triangle wound the way its normal points.
        let mesh = model.mesh();
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|at| Vec3::from(mesh.vertices[triangle[at] as usize].position));
            let normal = Vec3::from(mesh.vertices[triangle[0] as usize].normal);
            assert!((b - a).cross(c - a).dot(normal) > 0.0, "{a} {b} {c} against {normal}");
        }
    }

    #[test]
    fn a_cube_put_outside_a_box_makes_the_box_larger_and_leaves_the_rest_where_it_was() {
        let mut model = boxed([2, 2, 2]);
        model.to_voxels(0, 0);
        let red = model.color_index(RED);
        // Past its top: taller by one, nothing moved.
        assert_eq!(model.set_voxel(0, 0, [0, 2, 0], red), [0; 3]);
        assert_eq!((model.bones[0].boxes[0].size, model.bones[0].boxes[0].from), ([2, 3, 2], Vec3::ZERO));
        // Before its least corner: wider by one, its corner a pixel further out, its cubes one along.
        assert_eq!(model.set_voxel(0, 0, [-1, 0, 0], red), [1, 0, 0]);
        let grown = &model.bones[0].boxes[0];
        assert_eq!((grown.size, grown.from), ([3, 3, 2], Vec3::new(-1.0, 0.0, 0.0)));
        let voxels = grown.voxels.as_ref().unwrap();
        assert_eq!(voxels.get(grown.size, [0, 0, 0]), red);
        assert_eq!(voxels.get(grown.size, [1, 2, 0]), red, "the one put on top, now one along");
        assert_eq!(voxels.solid(), 10);
        assert_eq!(model.bounds().unwrap().0, Vec3::new(-1.0 / 16.0, 0.0, 0.0));
        // Taking a cube out where there is none, outside the box, changes nothing.
        let before = model.clone();
        assert_eq!(model.set_voxel(0, 0, [9, 9, 9], 0), [0; 3]);
        assert_eq!(model, before);

        // A side made shorter cuts cubes off; made longer from its low face, they stay by the other.
        let mut model_box = model.bones[0].boxes[0].clone();
        model_box.set_side(1, 2, false);
        assert_eq!(model_box.voxels.as_ref().unwrap().solid(), 9, "the one on top is cut off");
        model_box.set_side(0, 4, true);
        assert_eq!(model_box.voxels.as_ref().unwrap().get(model_box.size, [1, 0, 0]), red);
    }

    #[test]
    fn a_ray_meets_the_first_cube_in_its_way_and_goes_through_where_there_is_none() {
        let mut model = boxed([4, 4, 4]);
        model.to_voxels(0, 0);
        let bones = model.rest_matrices();
        let pixel = 1.0 / 16.0;
        // From the front, at the cube second along and third up.
        let at = Vec3::new(1.5, 2.5, 0.0) * pixel;
        let hit = model.pick(&bones, at + Vec3::Z, Vec3::NEG_Z).unwrap();
        assert_eq!((hit.cell, hit.face), (Some([1, 2, 3]), Face::Front));
        assert!((hit.distance - (1.0 - 4.0 * pixel)).abs() < 1e-4);
        let corners = model.cube_corners(&bones, &hit).unwrap();
        assert!((corners[0] - Vec3::new(2.0, 3.0, 4.0) * pixel).length() < 1e-5, "{corners:?}");
        assert!((corners[2] - Vec3::new(1.0, 2.0, 4.0) * pixel).length() < 1e-5, "{corners:?}");
        // With that cube and the one behind it gone, the ray meets the third.
        model.set_voxel(0, 0, [1, 2, 3], 0);
        model.set_voxel(0, 0, [1, 2, 2], 0);
        let hit = model.pick(&bones, at + Vec3::Z, Vec3::NEG_Z).unwrap();
        assert_eq!((hit.cell, hit.face), (Some([1, 2, 1]), Face::Front));
        // With the whole row gone it goes through, though it crosses the box.
        model.set_voxel(0, 0, [1, 2, 1], 0);
        model.set_voxel(0, 0, [1, 2, 0], 0);
        assert_eq!(model.pick(&bones, at + Vec3::Z, Vec3::NEG_Z), None);
        // From above, down into the hole: the side of the hole is not met, the floor of it is.
        let hit = model.pick(&bones, Vec3::new(1.5, 20.0, 1.5) * pixel, Vec3::NEG_Y).unwrap();
        assert_eq!((hit.cell, hit.face), (Some([1, 3, 1]), Face::Top));
    }

    #[test]
    fn cubes_and_the_palette_are_kept_in_the_file() {
        let mut model = boxed([3, 2, 2]);
        model.to_voxels(0, 0);
        let red = model.color_index(RED);
        model.set_voxel(0, 0, [2, 1, 1], red);
        model.set_voxel(0, 0, [0, 0, 0], 0);
        let text = model.to_text();
        assert!(text.contains("palette = [\"c8c8c8\", \"e25454\"]"), "{text}");
        assert!(text.contains("voxels = \""), "{text}");
        assert_eq!(BoxModel::parse(&text).unwrap(), model);

        // Mirrored, its cubes are turned over.
        let mirrored = BoxModel::mirrored_box(&model.bones[0].boxes[0]);
        let voxels = mirrored.voxels.as_ref().unwrap();
        assert_eq!((voxels.get([3, 2, 2], [0, 1, 1]), voxels.get([3, 2, 2], [2, 0, 0])), (red, 0));

        // A colour no cube has is not kept once the palette is pruned, and the cubes keep their colours.
        model.set_voxel(0, 0, [2, 1, 1], 1);
        model.prune_palette();
        assert_eq!(model.palette, [PLAIN]);
        let bad = text.replace("palette = [\"c8c8c8\", \"e25454\"]", "palette = [\"c8c8c8\"]");
        let error = format!("{:#}", BoxModel::parse(&bad).unwrap_err());
        assert!(error.contains("a colour the palette has not"), "{error}");
    }
}
