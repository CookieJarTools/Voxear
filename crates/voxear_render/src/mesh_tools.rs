//! What a model's import settings do to each of its meshes once it is read
//! and in Voxear's space (see `voxear_assets::ModelImport`): its size, its
//! normals and tangents, imported or calculated, its wind masks, and its
//! LODs.

use std::collections::HashMap;

use voxear_assets::{ModelImport, Normals, Tangents, WindMasks};
use voxear_core::math::{Vec2, Vec3};

use crate::{
    lod::{Lod, build_lods, full_lod},
    mesh::{MeshData, NO_WIND_MASKS},
};

/// What a mesh's file gave it, so the settings know what to calculate.
#[derive(Clone, Copy)]
pub(crate) struct FromFile {
    pub normals: bool,
    pub tangents: bool,
    /// Vertex colours, which the importer put in its vertices' wind masks.
    pub colors: bool,
}

/// Finishes a mesh as its model's settings say. `size` is the metres one
/// unit of the mesh becomes. Normals are calculated when asked for or when
/// the file has none, and tangents then too, since they follow the normals.
pub(crate) fn finish(mut data: MeshData, settings: &ModelImport, size: f32, file: FromFile) -> (MeshData, Vec<Lod>) {
    if size != 1.0 {
        for vertex in &mut data.vertices {
            vertex.position = (Vec3::from(vertex.position) * size).into();
        }
    }
    let normals = settings.normals == Normals::Calculate || !file.normals;
    if normals {
        calculate_normals(&mut data, settings.smoothing_angle);
    }
    if normals || settings.tangents == Tangents::Calculate || !file.tangents {
        calculate_tangents(&mut data);
    }
    match settings.wind_masks {
        WindMasks::Colors if file.colors => {}
        WindMasks::Generate => generate_wind_masks(&mut data),
        _ => data.vertices.iter_mut().for_each(|vertex| vertex.wind = NO_WIND_MASKS),
    }
    let lods = if settings.lods { build_lods(&data) } else { full_lod(&data) };
    (data, lods)
}

/// Wind masks for a plant made without them, from its shape, with its pivot
/// at the foot of its trunk as wind.slang has it. Each separate piece of the
/// mesh small beside the whole plant is a leaf, or a card of leaves, as
/// SpeedTree and Quixel plants are made: it flutters, at a phase of its own,
/// and sways as far as the branch it hangs from, by how far out from the
/// trunk it is. The rest, trunk and branches, does not flutter, and sways
/// more the further out it reaches.
pub(crate) fn generate_wind_masks(data: &mut MeshData) {
    const LEAF_SIZE: f32 = 0.2;
    if data.vertices.is_empty() {
        return;
    }
    let pieces = pieces(data);
    let positions: Vec<Vec3> = data.vertices.iter().map(|vertex| Vec3::from(vertex.position)).collect();
    let (low, high) =
        positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let size = (high - low).max_element().max(1e-6);
    let reach = positions.iter().map(|p| p.with_y(0.0).length()).fold(1e-6_f32, f32::max);
    let mut bounds: HashMap<usize, (Vec3, Vec3, Vec3, usize)> = HashMap::new();
    for (i, &piece) in pieces.iter().enumerate() {
        let p = positions[i];
        let entry = bounds.entry(piece).or_insert((p, p, Vec3::ZERO, 0));
        *entry = (entry.0.min(p), entry.1.max(p), entry.2 + p, entry.3 + 1);
    }
    let extent = |piece: &usize| (bounds[piece].1 - bounds[piece].0).length();
    let largest = bounds.keys().copied().max_by(|a, b| extent(a).total_cmp(&extent(b)).then(b.cmp(a)));
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    // Out from the trunk as a share of the plant's reach, eased in, so what is near the trunk barely moves.
    let out = |p: Vec3| (p.with_y(0.0).length() / reach).clamp(0.0, 1.0).powi(2);
    for (i, &piece) in pieces.iter().enumerate() {
        let (lo, hi, sum, count) = bounds[&piece];
        let phase = byte((piece as u32).wrapping_mul(2_654_435_761) as f32 / u32::MAX as f32);
        let leaf = Some(piece) != largest && (hi - lo).max_element() < size * LEAF_SIZE;
        data.vertices[i].wind = match leaf {
            true => [255, phase, byte(out(sum / count as f32)), 0],
            false => [0, phase, byte(out(positions[i])), 0],
        };
    }
}

/// Which piece of the mesh each vertex belongs to: vertices joined by
/// triangles, or at the same place (as across a UV seam), are one piece.
/// Each piece is named by one of its vertices.
fn pieces(data: &MeshData) -> Vec<usize> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..data.vertices.len()).collect();
    let join = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let (a, b) = (root(parent, a), root(parent, b));
        parent[a.max(b)] = a.min(b);
    };
    for &[a, b, c] in data.indices.as_chunks::<3>().0 {
        join(&mut parent, a as usize, b as usize);
        join(&mut parent, a as usize, c as usize);
    }
    let mut first_at: HashMap<[u32; 3], usize> = HashMap::new();
    for (i, vertex) in data.vertices.iter().enumerate() {
        let first = *first_at.entry(vertex.position.map(f32::to_bits)).or_insert(i);
        join(&mut parent, first, i);
    }
    (0..data.vertices.len()).map(|i| root(&mut parent, i)).collect()
}

/// The metres one unit becomes, from the settings, or one for settings that
/// would turn a model inside out or make it vanish.
pub(crate) fn size(settings: &ModelImport) -> f32 {
    let size = settings.size();
    if size.is_finite() && size > 0.0 { size } else { 1.0 }
}

/// Makes normals from the faces. Each corner takes the faces around its
/// position whose own normals are within `smoothing_angle` degrees of its
/// face's, weighted by their area, so a gentle curve is smooth and a sharp
/// edge stays hard. Corners that share a vertex but now differ get a vertex
/// each. Faces around the same position count even across a UV seam.
pub(crate) fn calculate_normals(data: &mut MeshData, smoothing_angle: f32) {
    let triangles = data.indices.as_chunks::<3>().0;
    let position = |i: u32| Vec3::from(data.vertices[i as usize].position);
    let faces: Vec<Vec3> =
        triangles.iter().map(|&[a, b, c]| (position(b) - position(a)).cross(position(c) - position(a))).collect();
    let units: Vec<Vec3> = faces.iter().map(|face| face.normalize_or_zero()).collect();

    // The faces at each position, by its bits, so split vertices find each other.
    let key = |i: u32| data.vertices[i as usize].position.map(f32::to_bits);
    let mut around: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
    for (face, corners) in triangles.iter().enumerate() {
        for &corner in corners {
            around.entry(key(corner)).or_default().push(face);
        }
    }

    let smooth = smoothing_angle.clamp(0.0, 180.0).to_radians().cos() - 1e-5;
    let mut vertices = Vec::with_capacity(data.vertices.len());
    let mut made: HashMap<(u32, [u32; 3]), u32> = HashMap::new();
    let mut indices = Vec::with_capacity(data.indices.len());
    for (face, corners) in triangles.iter().enumerate() {
        for &corner in corners {
            let near = around[&key(corner)].iter().filter(|&&other| units[face].dot(units[other]) >= smooth);
            let normal = near.map(|&other| faces[other]).sum::<Vec3>().normalize_or(units[face]);
            let normal = normal.normalize_or(Vec3::Y).to_array();
            let index = *made.entry((corner, normal.map(f32::to_bits))).or_insert_with(|| {
                let mut vertex = data.vertices[corner as usize];
                vertex.normal = normal;
                vertices.push(vertex);
                vertices.len() as u32 - 1
            });
            indices.push(index);
        }
    }
    *data = MeshData { vertices, indices };
}

/// Makes tangents from the normals and UVs: each along the direction U
/// grows across the faces around the vertex, and a sign that makes
/// `cross(normal, tangent) * sign` point up the texture, toward smaller V,
/// as glTF's do. A vertex whose faces have no UVs to go by gets any tangent
/// at right angles to its normal.
pub(crate) fn calculate_tangents(data: &mut MeshData) {
    let mut along_u = vec![Vec3::ZERO; data.vertices.len()];
    let mut along_v = vec![Vec3::ZERO; data.vertices.len()];
    for &[a, b, c] in data.indices.as_chunks::<3>().0 {
        let [va, vb, vc] = [a, b, c].map(|i| data.vertices[i as usize]);
        let (edge1, edge2) =
            (Vec3::from(vb.position) - Vec3::from(va.position), Vec3::from(vc.position) - Vec3::from(va.position));
        let (uv1, uv2) = (Vec2::from(vb.uv) - Vec2::from(va.uv), Vec2::from(vc.uv) - Vec2::from(va.uv));
        let area = uv1.x * uv2.y - uv2.x * uv1.y;
        if area.abs() < 1e-12 {
            continue;
        }
        let u = (edge1 * uv2.y - edge2 * uv1.y) / area;
        let v = (edge2 * uv1.x - edge1 * uv2.x) / area;
        for corner in [a, b, c] {
            along_u[corner as usize] += u;
            along_v[corner as usize] += v;
        }
    }
    for (vertex, (u, v)) in data.vertices.iter_mut().zip(along_u.into_iter().zip(along_v)) {
        let normal = Vec3::from(vertex.normal).normalize_or(Vec3::Y);
        let tangent = (u - normal * normal.dot(u)).normalize_or(normal.any_orthonormal_vector());
        let sign = if normal.cross(tangent).dot(-v) < 0.0 { -1.0 } else { 1.0 };
        vertex.tangent = [tangent.x, tangent.y, tangent.z, sign];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Vertex;

    fn vertex(position: [f32; 3], uv: [f32; 2]) -> Vertex {
        Vertex { position, normal: [0.0, 1.0, 0.0], uv, tangent: [1.0, 0.0, 0.0, 1.0], ..Vertex::default() }
    }

    /// Two faces of a box meeting at a right angle along the Z axis: the top,
    /// facing +Y, and the side, facing -X, sharing the edge's two vertices.
    fn corner() -> MeshData {
        let vertices = vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([0.0, 0.0, 1.0], [0.0, 1.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            vertex([0.0, -1.0, 0.0], [1.0, 1.0]),
        ];
        // Wound so cross(b - a, c - a) points out, as mesh.rs says.
        MeshData { vertices, indices: vec![0, 1, 2, 0, 3, 1] }
    }

    fn normals_at_edge(data: &MeshData) -> Vec<[f32; 3]> {
        let mut normals: Vec<[f32; 3]> =
            data.vertices.iter().filter(|v| v.position[0] == 0.0 && v.position[1] == 0.0).map(|v| v.normal).collect();
        normals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        normals.dedup();
        normals
    }

    #[test]
    fn an_edge_sharper_than_the_smoothing_angle_stays_hard() {
        let mut hard = corner();
        calculate_normals(&mut hard, 60.0);
        assert_eq!(normals_at_edge(&hard), vec![[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], "each face keeps its own");
        assert_eq!(hard.vertices.len(), 6, "the edge's two vertices split in two");

        let mut smooth = corner();
        calculate_normals(&mut smooth, 100.0);
        let normals = normals_at_edge(&smooth);
        assert_eq!(normals.len(), 1, "one normal, between the faces: {normals:?}");
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!(Vec3::from(normals[0]).abs_diff_eq(Vec3::new(-half, half, 0.0), 1e-5), "{normals:?}");
        assert_eq!(smooth.vertices.len(), 4);
    }

    #[test]
    fn tangents_run_along_u_with_the_bitangent_up_the_texture() {
        // A quad facing -Z, toward a camera at the origin looking down +Z in
        // Voxear's left-handed space: +X is right on screen, +Y up. Its
        // texture is upright, so U grows to the right and V down.
        let quad = |position: [f32; 3], uv: [f32; 2]| Vertex { normal: [0.0, 0.0, -1.0], ..vertex(position, uv) };
        let vertices = vec![
            quad([-1.0, 1.0, 5.0], [0.0, 0.0]),
            quad([1.0, 1.0, 5.0], [1.0, 0.0]),
            quad([1.0, -1.0, 5.0], [1.0, 1.0]),
            quad([-1.0, -1.0, 5.0], [0.0, 1.0]),
        ];
        let mut data = MeshData { vertices, indices: vec![0, 1, 2, 0, 2, 3] };
        calculate_tangents(&mut data);
        for vertex in &data.vertices {
            let [x, y, z, sign] = vertex.tangent;
            assert_eq!([x, y, z], [1.0, 0.0, 0.0], "along U, to the right: {vertex:?}");
            let up = Vec3::from(vertex.normal).cross(Vec3::new(x, y, z)) * sign;
            assert!(up.abs_diff_eq(Vec3::Y, 1e-6), "the bitangent points up the texture: {up:?}");
        }

        // The same texture mirrored left to right flips the sign, not the up.
        for vertex in &mut data.vertices {
            vertex.uv[0] = 1.0 - vertex.uv[0];
        }
        calculate_tangents(&mut data);
        for vertex in &data.vertices {
            let [x, y, z, sign] = vertex.tangent;
            assert_eq!([x, y, z], [-1.0, 0.0, 0.0]);
            let up = Vec3::from(vertex.normal).cross(Vec3::new(x, y, z)) * sign;
            assert!(up.abs_diff_eq(Vec3::Y, 1e-6), "{up:?}");
        }
    }

    #[test]
    fn settings_size_the_mesh_and_can_leave_out_its_lods() {
        let settings = ModelImport { scale: 2.0, lods: false, ..ModelImport::default() };
        let file = FromFile { normals: true, tangents: true, colors: false };
        let (data, lods) = finish(corner(), &settings, size(&settings), file);
        assert_eq!(data.vertices[2].position, [2.0, 0.0, 0.0]);
        assert_eq!(lods.len(), 1, "the mesh in full, only");
        assert_eq!(data.vertices.len(), 4, "the file's normals are kept");
        assert_eq!(size(&ModelImport { scale: -1.0, ..ModelImport::default() }), 1.0);
    }

    /// A small square of `half` its width across, standing at `centre`, facing -Z.
    fn card(data: &mut MeshData, centre: [f32; 3], half: f32) {
        let at = data.vertices.len() as u32;
        let [x, y, z] = centre;
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            data.vertices.push(vertex([x + dx * half, y + dy * half, z], [0.0, 0.0]));
        }
        data.indices.extend([at, at + 2, at + 1, at, at + 3, at + 2]);
    }

    /// A tree: a trunk two metres tall, from its pivot up, a branch reaching
    /// out from it in a piece of its own but as large, and two leaves.
    fn tree() -> MeshData {
        let trunk = [[-0.1, 0.0, 0.0], [0.1, 0.0, 0.0], [0.1, 2.0, 0.0], [-0.1, 2.0, 0.0]];
        let vertices = trunk.iter().map(|&p| vertex(p, [0.0, 0.0])).collect();
        let mut data = MeshData { vertices, indices: vec![0, 2, 1, 0, 3, 2] };
        // A long branch out to +X, in a piece of its own.
        let at = data.vertices.len() as u32;
        for p in [[0.1, 1.5, 0.0], [2.0, 1.9, 0.0], [2.0, 2.0, 0.0], [0.1, 1.6, 0.0]] {
            data.vertices.push(vertex(p, [0.0, 0.0]));
        }
        data.indices.extend([at, at + 2, at + 1, at, at + 3, at + 2]);
        card(&mut data, [1.9, 2.15, 0.05], 0.1);
        card(&mut data, [-0.4, 1.8, 0.2], 0.1);
        data
    }

    #[test]
    fn generated_wind_masks_flutter_leaves_and_sway_what_reaches_out() {
        let mut data = tree();
        generate_wind_masks(&mut data);
        let wind = |i: usize| data.vertices[i].wind;
        let [flutter, _, sway, _] = wind(0);
        assert!(flutter == 0 && sway < 3, "the trunk's foot neither flutters nor sways: {:?}", wind(0));
        assert_eq!(wind(5)[0], 0, "a branch as large as the tree does not flutter");
        assert!(wind(5)[2] > 200, "but its tip sways: {:?}", wind(5));
        assert!(wind(4)[2] < 10, "and not where it meets the trunk: {:?}", wind(4));
        let (far, near) = (wind(8), wind(12));
        assert_eq!((far[0], near[0]), (255, 255), "leaves flutter");
        assert!(far[2] > near[2] + 100, "a leaf far out sways further than one near the trunk: {far:?} {near:?}");
        assert!((8..12).all(|i| wind(i) == far), "a leaf moves as one");
        assert_ne!(far[1], near[1], "each leaf at its own phase");
    }

    #[test]
    fn vertex_colours_are_wind_masks_only_when_the_settings_say() {
        let mut painted = tree();
        painted.vertices.iter_mut().for_each(|vertex| vertex.wind = [10, 20, 30, 255]);
        let file = FromFile { normals: true, tangents: true, colors: true };
        let read = |masks: WindMasks| {
            let settings = ModelImport { wind_masks: masks, lods: false, ..ModelImport::default() };
            finish(painted.clone(), &settings, 1.0, file).0.vertices[0].wind
        };
        assert_eq!(read(WindMasks::None), NO_WIND_MASKS, "colours are not masks unless asked");
        assert_eq!(read(WindMasks::Colors), [10, 20, 30, 255]);
        assert_eq!(read(WindMasks::Generate)[0], 0, "generated from the shape, colours or not");
    }
}
