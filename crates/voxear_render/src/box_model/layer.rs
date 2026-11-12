//! A box's layer built into the model's mesh: each solid pixel of the layer
//! a small block standing out from the box's face.
//!
//! The blocks of a face are built on the box grown by the layer's height,
//! so the layers of two faces meet at the box's edges as the faces
//! themselves do. A row of solid pixels shares one top, which the sheet
//! still colours pixel by pixel, and walls are built only where a solid
//! pixel has nothing solid beside it: nothing is built between two blocks,
//! and nothing under them, where the box's own face already is.
//!
//! From far off the pixels cannot be told from a flat picture, so the mesh
//! carries that as well: one sheet a face, where the tops would be, for a
//! coarser level of detail to draw in the blocks' place.

use voxear_core::math::{Mat4, Vec3};

use super::{BoxLayer, Face, INSET, ModelBox, Sheet};
use crate::{
    mesh::{MeshData, Vertex},
    skeleton::SkinVertex,
};

/// How far a layer of no height lies above its face, in pixels, so the two
/// are not drawn in the same place.
const LIFT: f32 = 0.02;
/// How far inside the grown box the walls at a face's own edges stand, as
/// a share of the layer's height, so they do not lie in the tops of the
/// next face's blocks.
const TUCK: f32 = 0.03;

/// What a layer is built onto: its box's place in the model and how far
/// its faces stand from its middle, in metres, the joint it moves with,
/// and how many metres a pixel is.
pub(super) struct Onto {
    pub place: Mat4,
    pub reach: Vec3,
    pub joint: u16,
    pub pixel: f32,
}

/// Builds a box's layer into `data`, every solid pixel standing out, and
/// into `far` the same lying flat. Returns whether anything stands out, so
/// that near and far differ.
pub(super) fn raise(
    data: &mut MeshData,
    far: &mut Vec<u32>,
    sheet: &Sheet,
    model_box: &ModelBox,
    layer: BoxLayer,
    on: &Onto,
) -> bool {
    let height = layer.height.max(0.0);
    let stands = height > 0.0;
    let lift = Vec3::splat(height.max(LIFT) * on.pixel);
    let mut any = false;
    for face in Face::ALL {
        let Some([left, top, right, bottom]) = model_box.skin_rect(layer.skin, face) else { continue };
        let (normal, (across, down)) = (face.normal(), face.across_and_down());
        let (columns, rows) = ((right - left).abs().round() as u32, (bottom - top).abs().round() as u32);
        // A flat box's edges have no face, and so no layer.
        if columns == 0
            || rows == 0
            || (across * on.reach).length_squared() == 0.0
            || (down * on.reach).length_squared() == 0.0
        {
            continue;
        }
        let (step_u, step_v) = ((right - left).signum(), (bottom - top).signum());
        let pixel_at = |column: u32, row: u32| {
            let (x, y) = (left + step_u * (column as f32 + 0.5), top + step_v * (row as f32 + 0.5));
            (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32)
        };
        let solid = |column: i64, row: i64| {
            let inside = (0..i64::from(columns)).contains(&column) && (0..i64::from(rows)).contains(&row);
            inside && {
                let (x, y) = pixel_at(column as u32, row as u32);
                sheet.get(x, y)[3] >= 128
            }
        };
        // A point of the face's grid, lines counted from its upper left, `out` of the layer's height above the box's
        // face and `tuck` of it in from the grown box's edge.
        let grown = on.reach + lift;
        let point = |column: f32, row: f32, out: f32| {
            let (s, t) = (column / columns as f32 * 2.0 - 1.0, row / rows as f32 * 2.0 - 1.0);
            on.place.transform_point3(normal * (on.reach + lift * out) + across * grown * s + down * grown * t)
        };
        // The grid's own first and last lines, as walls stand on them: a little in from the grown box's faces. `extent`
        // is how far the grown face reaches from its middle that way.
        let tucked = |line: u32, lines: u32, extent: f32| {
            let in_by = TUCK * lift.x * lines as f32 / (2.0 * extent.max(1e-6));
            match line {
                0 => in_by,
                line if line == lines => line as f32 - in_by,
                line => line as f32,
            }
        };
        let (wide, high) = ((across * grown).length(), (down * grown).length());
        let uv = |x: f32, y: f32| [x / sheet.width as f32, y / sheet.height as f32];
        let towards = |from: f32, to: f32| from + (to - from).signum() * INSET;

        // From far off: the whole face as one sheet, where the tops are.
        let whole = [
            (point(0.0, 0.0, 1.0), uv(towards(left, right), towards(top, bottom))),
            (point(columns as f32, 0.0, 1.0), uv(towards(right, left), towards(top, bottom))),
            (point(columns as f32, rows as f32, 1.0), uv(towards(right, left), towards(bottom, top))),
            (point(0.0, rows as f32, 1.0), uv(towards(left, right), towards(bottom, top))),
        ];
        let facing = on.place.transform_vector3(normal).normalize_or_zero();
        let flat = quad(data, whole, facing, on.joint);
        far.extend(flat);
        if !stands {
            // Lying flat near as well: the sheet is all there is of it.
            data.indices.extend(flat);
            continue;
        }

        for row in 0..rows {
            let mut column = 0;
            while column < columns {
                if !solid(i64::from(column), i64::from(row)) {
                    column += 1;
                    continue;
                }
                // A run of solid pixels along the row: one top for them all.
                let first = column;
                while column < columns && solid(i64::from(column), i64::from(row)) {
                    column += 1;
                }
                any = true;
                let (from, to) = (first as f32, column as f32);
                let (u0, u1) = (left + step_u * from, left + step_u * to);
                let (v0, v1) = (top + step_v * row as f32, top + step_v * (row + 1) as f32);
                let (u0, u1, v0, v1) = (towards(u0, u1), towards(u1, u0), towards(v0, v1), towards(v1, v0));
                let (upper, lower) = (row as f32, (row + 1) as f32);
                let top_quad = [
                    (point(from, upper, 1.0), uv(u0, v0)),
                    (point(to, upper, 1.0), uv(u1, v0)),
                    (point(to, lower, 1.0), uv(u1, v1)),
                    (point(from, lower, 1.0), uv(u0, v1)),
                ];
                let indices = quad(data, top_quad, facing, on.joint);
                data.indices.extend(indices);

                // Walls: at each end of the run, and along it wherever the row above or below is not solid.
                let middle_v = (v0 + v1) * 0.5;
                let mut wall = |a: (f32, f32), b: (f32, f32), ua: f32, ub: f32, out: Vec3| {
                    let corners = [
                        (point(a.0, a.1, 1.0), uv(ua, middle_v)),
                        (point(b.0, b.1, 1.0), uv(ub, middle_v)),
                        (point(b.0, b.1, 0.0), uv(ub, middle_v)),
                        (point(a.0, a.1, 0.0), uv(ua, middle_v)),
                    ];
                    let facing = on.place.transform_vector3(out).normalize_or_zero();
                    let indices = quad(data, corners, facing, on.joint);
                    data.indices.extend(indices);
                };
                let (row_upper, row_lower) = (tucked(row, rows, high), tucked(row + 1, rows, high));
                let (end_first, end_last) = (tucked(first, columns, wide), tucked(column, columns, wide));
                let first_middle = (left + step_u * (from + 0.5), left + step_u * (to - 0.5));
                wall((end_first, upper), (end_first, lower), first_middle.0, first_middle.0, -across);
                wall((end_last, upper), (end_last, lower), first_middle.1, first_middle.1, across);
                for (beside, line, out) in
                    [(i64::from(row) - 1, row_upper, -down), (i64::from(row) + 1, row_lower, down)]
                {
                    // Each stretch of the run with nothing solid beside it in that row.
                    let mut at = first;
                    while at < column {
                        if solid(i64::from(at), beside) {
                            at += 1;
                            continue;
                        }
                        let open = at;
                        while at < column && !solid(i64::from(at), beside) {
                            at += 1;
                        }
                        let (ua, ub) = (left + step_u * open as f32, left + step_u * at as f32);
                        wall((open as f32, line), (at as f32, line), towards(ua, ub), towards(ub, ua), out);
                    }
                }
            }
        }
    }
    any
}

/// Adds four corners, each with its place on the sheet, as a face looking
/// along `facing`, bound whole to a joint, and gives its two triangles.
fn quad(data: &mut MeshData, corners: [(Vec3, [f32; 2]); 4], facing: Vec3, joint: u16) -> [u32; 6] {
    let first = data.vertices.len() as u32;
    for (position, uv) in corners {
        data.vertices.push(Vertex { position: position.into(), normal: facing.into(), uv, ..Vertex::default() });
        data.skin.push(SkinVertex { joints: [joint, 0, 0, 0], weights: [u16::MAX, 0, 0, 0] });
    }
    // Wound to face out, whichever way round its corners came.
    let [a, b, c] = [corners[0].0, corners[1].0, corners[2].0];
    if (b - a).cross(c - a).dot(facing) >= 0.0 {
        [first, first + 1, first + 2, first, first + 2, first + 3]
    } else {
        [first, first + 2, first + 1, first, first + 3, first + 2]
    }
}

#[cfg(test)]
mod tests {
    use super::super::{BoxModel, BoxSkin};
    use super::*;

    /// A block 4 pixels a side, its own faces at the sheet's corner and a layer beside them.
    fn block(height: f32) -> BoxModel {
        let mut model = BoxModel { sheet: Sheet::filled(32, 16, [0; 4]), ..BoxModel::default() };
        let bone = model.add_bone("Body", None, Vec3::ZERO);
        let added = model.add_box(bone, Vec3::new(-2.0, 0.0, -2.0), [4, 4, 4], [0, 0]);
        added.layer = Some(BoxLayer { skin: BoxSkin::Unfolded { at: [16, 0], mirror: false }, height });
        model
    }

    /// The front face's layer is at 20, 4 on the sheet, 4 × 4.
    const FRONT: [u32; 2] = [20, 4];

    fn triangles(data: &MeshData, indices: &[u32]) -> Vec<[Vec3; 3]> {
        indices.chunks_exact(3).map(|t| [0, 1, 2].map(|i| Vec3::from(data.vertices[t[i] as usize].position))).collect()
    }

    #[test]
    fn a_layer_with_nothing_solid_adds_nothing_to_see_near() {
        let model = block(1.0);
        let (data, far) = model.build(&model.rest_matrices());
        assert_eq!(data.indices.len(), 6 * 6, "the box's own six faces");
        assert!(far.is_none(), "nothing stands out, so far is as near");
    }

    #[test]
    fn a_solid_pixel_stands_out_as_a_block_with_a_top_and_four_walls() {
        let mut model = block(1.0);
        model.sheet.set(FRONT[0] + 1, FRONT[1] + 1, [200, 50, 50, 255]);
        let (data, far) = model.build(&model.rest_matrices());
        let raised = &data.indices[6 * 6..];
        assert_eq!(raised.len(), 5 * 6, "a top and four walls, and nothing under it");
        let points: Vec<Vec3> = triangles(&data, raised).into_iter().flatten().collect();
        let (least, most) =
            points.iter().fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(l, m), p| (l.min(*p), m.max(*p)));
        // The box is 4 px across: from -2 to 2, its front at z = 2. Grown by a pixel, each of the layer's pixels is
        // 1.5 px across, the second from the model's right from x = 1.5 down to 0, and a pixel deep.
        let px = 1.0 / 16.0;
        // Up and down the same: from 5, the grown box's top, the second row runs from 3.5 down to 2.
        assert!((most - Vec3::new(1.5, 3.5, 3.0) * px).length() < 1e-4, "{}", most / px);
        assert!((least - Vec3::new(0.0, 2.0, 2.0) * px).length() < 1e-4, "{}", least / px);
        // Every triangle faces out of the block.
        let middle = (least + most) * 0.5;
        for (at, [a, b, c]) in triangles(&data, raised).into_iter().enumerate() {
            let out = (a + b + c) / 3.0 - middle;
            assert!((b - a).cross(c - a).dot(out) > 0.0, "triangle {at} faces in");
        }
        // Every corner of it shows the one pixel.
        for index in raised {
            let [u, v] = data.vertices[*index as usize].uv;
            let (x, y) = ((u * 32.0).floor() as u32, (v * 16.0).floor() as u32);
            assert_eq!((x, y), (FRONT[0] + 1, FRONT[1] + 1));
        }
        // From far off: the box, and each face's layer as one sheet.
        let far = far.expect("something stands out");
        assert_eq!(far.len(), 6 * 6 + 6 * 6);
    }

    #[test]
    fn a_row_of_pixels_shares_a_top_and_has_no_walls_between_them() {
        let mut model = block(1.0);
        for x in 0..4 {
            model.sheet.set(FRONT[0] + x, FRONT[1] + 2, [50, 200, 50, 255]);
        }
        let (data, _) = model.build(&model.rest_matrices());
        // One top, a wall at each end, and one along each side of the row.
        assert_eq!(data.indices[6 * 6..].len(), 5 * 6);
        // With the row under it solid too but for one pixel, that side's wall is only where the gap is.
        for x in [0, 1, 3] {
            model.sheet.set(FRONT[0] + x, FRONT[1] + 3, [50, 200, 50, 255]);
        }
        let (data, _) = model.build(&model.rest_matrices());
        // Row 2: top, 2 ends, the wall above, and under it only over the gap. Row 3: two runs, each a top and two
        // ends, a wall under each, and above them nothing, the row above being solid.
        let quads = (1 + 2 + 1 + 1) + (1 + 2 + 1) + (1 + 2 + 1);
        assert_eq!(data.indices[6 * 6..].len(), quads * 6);
    }

    #[test]
    fn a_layer_of_no_height_lies_flat_on_its_faces_near_and_far() {
        let mut model = block(0.0);
        model.sheet.set(FRONT[0], FRONT[1], [9, 9, 9, 255]);
        let (data, far) = model.build(&model.rest_matrices());
        assert_eq!(data.indices.len(), 6 * 6 + 6 * 6, "a sheet over each face");
        assert!(far.is_none());
        let most = data.vertices.iter().fold(Vec3::NEG_INFINITY, |most, vertex| most.max(vertex.position.into()));
        assert!(most.z > 2.0 / 16.0 && most.z < 2.1 / 16.0, "just clear of the face: {}", most.z * 16.0);
    }

    #[test]
    fn a_layer_is_kept_in_the_file_and_cooked_with_a_flat_level_for_far_off() {
        let mut model = block(1.5);
        model.sheet.set(FRONT[0] + 1, FRONT[1] + 1, [200, 50, 50, 255]);
        let text = model.to_text();
        assert!(text.contains("[bones.boxes.layer]") && text.contains("height = 1.5"), "{text}");
        assert_eq!(BoxModel::parse(&text).unwrap(), model);
        let prepared = model.prepared("Mobs/Block.boxmodel");
        let lods = &prepared.meshes[0].1;
        assert_eq!(lods.len(), 2);
        assert!((lods[1].error - 1.5 / 16.0).abs() < 1e-6, "as far out as its pixels stand");
        assert_eq!(lods[1].indices.len(), 12 * 6);
        // A box with no layer has the one level.
        assert_eq!(BoxModel::starter().prepared("Block.boxmodel").meshes[0].1.len(), 1);
    }
}
