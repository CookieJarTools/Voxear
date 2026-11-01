//! What a model's import settings do to each of its meshes once it is read
//! and in Voxear's space (see `voxear_assets::ModelImport`): its size, its
//! normals and tangents, imported or calculated, and its LODs.

use std::collections::HashMap;

use voxear_assets::{ModelImport, Normals, Tangents};
use voxear_core::math::{Vec2, Vec3};

use crate::{
    lod::{Lod, build_lods, full_lod},
    mesh::MeshData,
};

/// What a mesh's file gave it, so the settings know what to calculate.
#[derive(Clone, Copy)]
pub(crate) struct FromFile {
    pub normals: bool,
    pub tangents: bool,
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
    let lods = if settings.lods { build_lods(&data) } else { full_lod(&data) };
    (data, lods)
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
        Vertex { position, normal: [0.0, 1.0, 0.0], uv, tangent: [1.0, 0.0, 0.0, 1.0] }
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
        let file = FromFile { normals: true, tangents: true };
        let (data, lods) = finish(corner(), &settings, size(&settings), file);
        assert_eq!(data.vertices[2].position, [2.0, 0.0, 0.0]);
        assert_eq!(lods.len(), 1, "the mesh in full, only");
        assert_eq!(data.vertices.len(), 4, "the file's normals are kept");
        assert_eq!(size(&ModelImport { scale: -1.0, ..ModelImport::default() }), 1.0);
    }
}
