//! Meshlets: each LOD's triangles in small clusters, so the GPU can cull
//! the parts of a mesh it cannot see rather than only whole meshes.
//!
//! A LOD's index list is reordered so each meshlet's triangles sit together,
//! which makes a meshlet just a range of the LOD's indices: drawing the whole
//! LOD still works, and the culling pass draws the meshlets that survive as
//! ranges of the same index buffer. Each meshlet also keeps its own vertices
//! (at most [`MESHLET_VERTICES`] of the mesh's) and its triangles as three
//! bytes into those, which is what a mesh shader reads.
//!
//! Every meshlet has a bounding sphere, for the frustum and the depth
//! pyramid, and a cone around its triangles' normals: seen from inside the
//! cone's reverse, every triangle faces away, so the whole meshlet can go.

use meshopt::{VertexDataAdapter, build_meshlets, compute_meshlet_bounds};

use crate::{gpu::GpuMeshlet, mesh::Vertex};

/// At most this many vertices and triangles a meshlet: the size NVIDIA
/// recommends for mesh shaders, and small enough to cull finely.
pub const MESHLET_VERTICES: usize = 64;
pub const MESHLET_TRIANGLES: usize = 124;

/// How much meshlets are shaped for cone culling rather than for size.
const CONE_WEIGHT: f32 = 0.25;

/// One LOD's meshlets. Offsets into `vertices` and `triangles` count from
/// this LOD's own; [`MeshMeshlets::gather`] moves them to the mesh's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LodMeshlets {
    pub meshlets: Vec<GpuMeshlet>,
    /// Mesh vertex indices, each meshlet's run of them in turn.
    pub vertices: Vec<u32>,
    /// One triangle a word: three bytes into its meshlet's vertices.
    pub triangles: Vec<u32>,
}

/// Splits a LOD's triangles into meshlets, and returns its indices in
/// meshlet order with them. `indices` should already be ordered for the
/// vertex cache, which meshlets keep within themselves.
pub fn build(indices: &[u32], vertices: &[Vertex]) -> (Vec<u32>, LodMeshlets) {
    let Ok(adapter) = VertexDataAdapter::new(bytemuck::cast_slice(vertices), size_of::<Vertex>(), 0) else {
        return (indices.to_vec(), LodMeshlets::default());
    };
    if indices.is_empty() {
        return (Vec::new(), LodMeshlets::default());
    }
    let built = build_meshlets(indices, &adapter, MESHLET_VERTICES, MESHLET_TRIANGLES, CONE_WEIGHT);
    let mut ordered = Vec::with_capacity(indices.len());
    let mut lod = LodMeshlets::default();
    for meshlet in built.iter() {
        let bounds = compute_meshlet_bounds(meshlet, &adapter);
        let triangle_count = meshlet.triangles.len() / 3;
        lod.meshlets.push(GpuMeshlet {
            center: bounds.center,
            radius: bounds.radius,
            cone_axis: bounds.cone_axis,
            cone_cutoff: bounds.cone_cutoff,
            first_index: ordered.len() as u32,
            vertex_offset: lod.vertices.len() as u32,
            triangle_offset: lod.triangles.len() as u32,
            counts: meshlet.vertices.len() as u32 | (triangle_count as u32) << 8,
        });
        for corners in meshlet.triangles.as_chunks::<3>().0 {
            ordered.extend(corners.map(|corner| meshlet.vertices[usize::from(corner)]));
            let [a, b, c] = corners.map(u32::from);
            lod.triangles.push(a | b << 8 | c << 16);
        }
        lod.vertices.extend_from_slice(meshlet.vertices);
    }
    (ordered, lod)
}

/// Every LOD's meshlets of one mesh, one after another, as they go up to
/// the GPU: vertex and triangle offsets count from the mesh's own, and each
/// meshlet's first index still from its LOD's.
#[derive(Debug, Default, PartialEq)]
pub struct MeshMeshlets {
    pub meshlets: Vec<GpuMeshlet>,
    pub vertices: Vec<u32>,
    pub triangles: Vec<u32>,
    /// How many meshlets each LOD has, finest first.
    pub counts: Vec<u32>,
}

impl MeshMeshlets {
    pub fn gather<'a>(lods: impl IntoIterator<Item = &'a LodMeshlets>) -> Self {
        let mut mesh = Self::default();
        for lod in lods {
            let (vertices, triangles) = (mesh.vertices.len() as u32, mesh.triangles.len() as u32);
            mesh.meshlets.extend(lod.meshlets.iter().map(|meshlet| GpuMeshlet {
                vertex_offset: meshlet.vertex_offset + vertices,
                triangle_offset: meshlet.triangle_offset + triangles,
                ..*meshlet
            }));
            mesh.vertices.extend_from_slice(&lod.vertices);
            mesh.triangles.extend_from_slice(&lod.triangles);
            mesh.counts.push(lod.meshlets.len() as u32);
        }
        mesh
    }

    /// The most meshlets any one LOD has: the most one draw of the mesh
    /// can turn into.
    pub fn most(&self) -> u32 {
        self.counts.iter().copied().max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use voxear_core::math::Vec3;

    use super::*;
    use crate::{lod::build_lods, mesh::MeshData};

    /// Each triangle as its corners' vertex indices, turned so the lowest
    /// comes first, which keeps its winding.
    fn triangle_counts(indices: &[u32]) -> HashMap<[u32; 3], usize> {
        let mut counts = HashMap::new();
        for &[a, b, c] in indices.as_chunks::<3>().0 {
            let turned = match a.min(b).min(c) {
                low if low == a => [a, b, c],
                low if low == b => [b, c, a],
                _ => [c, a, b],
            };
            *counts.entry(turned).or_default() += 1;
        }
        counts
    }

    #[test]
    fn meshlets_keep_every_triangle_and_its_winding() {
        let sphere = MeshData::sphere(64, 32);
        let (ordered, lod) = build(&sphere.indices, &sphere.vertices);
        assert_eq!(triangle_counts(&ordered), triangle_counts(&sphere.indices));
        assert!(lod.meshlets.len() > 1, "{} meshlets", lod.meshlets.len());

        let mut next = 0;
        for meshlet in &lod.meshlets {
            let (vertex_count, triangle_count) = (meshlet.counts & 0xFF, meshlet.counts >> 8);
            assert!(vertex_count as usize <= MESHLET_VERTICES && triangle_count as usize <= MESHLET_TRIANGLES);
            assert_eq!(meshlet.first_index, next, "meshlets follow one another in the index list");
            next += triangle_count * 3;
            // Its triangles, read through its own vertices, are its range of the indices.
            let own = &lod.vertices[meshlet.vertex_offset as usize..][..vertex_count as usize];
            let triangles = &lod.triangles[meshlet.triangle_offset as usize..][..triangle_count as usize];
            let read: Vec<u32> =
                triangles.iter().flat_map(|&t| [t & 0xFF, t >> 8 & 0xFF, t >> 16].map(|c| own[c as usize])).collect();
            assert_eq!(read, ordered[meshlet.first_index as usize..next as usize]);
        }
        assert_eq!(next as usize, ordered.len());
    }

    #[test]
    fn bounds_hold_each_meshlet_and_cones_face_outward() {
        let sphere = MeshData::sphere(64, 32);
        let (ordered, lod) = build(&sphere.indices, &sphere.vertices);
        let mut culls = 0;
        for meshlet in &lod.meshlets {
            let center = Vec3::from(meshlet.center);
            let range = meshlet.first_index as usize..(meshlet.first_index + (meshlet.counts >> 8) * 3) as usize;
            for &index in &ordered[range.clone()] {
                let position = Vec3::from(sphere.vertices[index as usize].position);
                assert!(position.distance(center) <= meshlet.radius * 1.001 + 1e-5, "a vertex outside its sphere");
            }
            // The sphere's faces point away from its middle, so each cone does too.
            let axis = Vec3::from(meshlet.cone_axis);
            assert!((axis.length() - 1.0).abs() < 1e-3);
            assert!(axis.dot(center) > 0.0, "a cone pointing inward");
            if meshlet.cone_cutoff < 1.0 {
                culls += 1;
                // Seen from far behind it, through the sphere, the meshlet is all back faces.
                let eye = center - axis * 100.0;
                let to = center - eye;
                assert!(to.dot(axis) >= meshlet.cone_cutoff * to.length() + meshlet.radius);
                // And seen from in front, it is not culled.
                let eye = center + axis * 100.0;
                let to = center - eye;
                assert!(to.dot(axis) < meshlet.cone_cutoff * to.length() + meshlet.radius);
            }
        }
        assert!(culls * 2 > lod.meshlets.len(), "most of a sphere's meshlets can be cone culled");
    }

    #[test]
    fn a_meshs_lods_gather_with_offsets_from_its_own_start() {
        let sphere = MeshData::sphere(64, 32);
        let lods = build_lods(&sphere);
        let mesh = MeshMeshlets::gather(lods.iter().map(|lod| &lod.meshlets));
        assert_eq!(mesh.counts.len(), lods.len());
        assert_eq!(mesh.counts.iter().sum::<u32>() as usize, mesh.meshlets.len());
        assert_eq!(mesh.most(), mesh.counts[0], "the finest LOD has the most");
        let second = &mesh.meshlets[mesh.counts[0] as usize];
        assert_eq!(second.first_index, 0, "first indices count from each LOD's own");
        assert_eq!(second.vertex_offset as usize, lods[0].meshlets.vertices.len());
        assert_eq!(second.triangle_offset as usize, lods[0].meshlets.triangles.len());
        for (lod, &count) in lods.iter().zip(&mesh.counts) {
            let triangles: u32 = lod.meshlets.meshlets.iter().map(|m| m.counts >> 8).sum();
            assert_eq!(triangles as usize * 3, lod.indices.len());
            assert_eq!(count as usize, lod.meshlets.meshlets.len());
        }
    }

    #[test]
    fn small_and_empty_meshes() {
        let cube = MeshData::cube();
        let (ordered, lod) = build(&cube.indices, &cube.vertices);
        assert_eq!(lod.meshlets.len(), 1);
        assert_eq!(triangle_counts(&ordered), triangle_counts(&cube.indices));
        let (ordered, lod) = build(&[], &cube.vertices);
        assert!(ordered.is_empty() && lod.meshlets.is_empty());
    }
}
