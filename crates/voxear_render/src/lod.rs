//! Automatic levels of detail. Every LOD shares the mesh's vertices and only
//! has its own index list, so LODs cost little memory and the GPU can switch
//! between them per instance with no extra state.

use meshopt::{SimplifyOptions, VertexDataAdapter, optimize_vertex_cache, simplify_with_attributes_and_locks};

use crate::mesh::{MeshData, Vertex};

pub const MAX_LODS: usize = 6;

/// Below this, a mesh is cheap enough that LODs are not worth having.
const MIN_TRIANGLES: usize = 256;

/// Each LOD aims for this fraction of the previous one's triangles.
const REDUCTION: f32 = 0.5;

/// Stop when a step removes less than this fraction of triangles: the mesh has
/// stopped simplifying without visible damage.
const MIN_PROGRESS: f32 = 0.15;

/// How strongly normals hold vertices in place, relative to position error.
const NORMAL_WEIGHT: f32 = 0.5;

#[derive(Clone)]
pub struct Lod {
    pub indices: Vec<u32>,
    /// Largest distance, in mesh units, between this LOD and the original
    /// surface. The GPU projects it to pixels to pick a LOD.
    pub error: f32,
}

/// One of a mesh's LODs, as the editor shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodInfo {
    pub triangles: u32,
    /// As [`Lod::error`].
    pub error: f32,
}

impl LodInfo {
    /// How far from a mesh's bounding sphere a camera first draws this LOD,
    /// in metres: where its error, at `scale`, shrinks to `threshold` pixels
    /// with `pixels_per_metre` pixels a metre away, as the GPU picks.
    pub fn from_distance(&self, scale: f32, pixels_per_metre: f32, threshold: f32) -> f32 {
        self.error * scale * pixels_per_metre / threshold.max(1e-6)
    }
}

/// The LOD the GPU draws `distance` metres from a mesh's bounding sphere:
/// the coarsest whose error stays within `threshold` pixels.
pub fn lod_at(lods: &[LodInfo], distance: f32, scale: f32, pixels_per_metre: f32, threshold: f32) -> usize {
    let distance = distance.max(1e-4);
    let fits = |lod: &LodInfo| lod.error * scale * pixels_per_metre / distance <= threshold;
    lods.iter().skip(1).take_while(|lod| fits(lod)).count()
}

/// LOD 0 is the original mesh with its triangles reordered for the vertex
/// cache; each further LOD has about half the triangles of the one before.
pub fn build_lods(mesh: &MeshData) -> Vec<Lod> {
    let vertex_count = mesh.vertices.len();
    let mut lods = full_lod(mesh);
    if mesh.indices.len() / 3 < MIN_TRIANGLES {
        return lods;
    }

    let Ok(adapter) = VertexDataAdapter::new(bytemuck::cast_slice(&mesh.vertices), size_of::<Vertex>(), 0) else {
        return lods;
    };
    let normals: Vec<f32> = mesh.vertices.iter().flat_map(|v| v.normal).collect();
    let weights = [NORMAL_WEIGHT; 3];
    // The wrapper hands this pointer straight to C, so it must cover every vertex.
    let locks = vec![false; vertex_count];

    let mut target = mesh.indices.len();
    while lods.len() < MAX_LODS {
        target = ((target as f32 * REDUCTION) as usize / 3) * 3;
        let previous = lods.last().expect("LOD 0 exists").indices.len();
        let mut error = 0.0;
        // Always simplify from the original, so each error is measured against it.
        let indices = simplify_with_attributes_and_locks(
            &mesh.indices,
            &adapter,
            &normals,
            &weights,
            3 * size_of::<f32>(),
            &locks,
            target,
            f32::MAX,
            SimplifyOptions::ErrorAbsolute,
            Some(&mut error),
        );
        if indices.is_empty() || indices.len() as f32 > previous as f32 * (1.0 - MIN_PROGRESS) {
            break;
        }
        lods.push(Lod { indices: optimize_vertex_cache(&indices, vertex_count), error });
    }
    lods
}

/// How far from a mesh's surface a simplification of it to `triangles`
/// triangles strays, in mesh units, as meshoptimizer measures it: the error
/// a model's own LOD of that many triangles is given (see `model_lods`).
pub fn error_at(mesh: &MeshData, triangles: usize) -> f32 {
    if triangles * 3 >= mesh.indices.len() {
        return 0.0;
    }
    let Ok(adapter) = VertexDataAdapter::new(bytemuck::cast_slice(&mesh.vertices), size_of::<Vertex>(), 0) else {
        return 0.0;
    };
    let normals: Vec<f32> = mesh.vertices.iter().flat_map(|v| v.normal).collect();
    let locks = vec![false; mesh.vertices.len()];
    let mut error = 0.0;
    simplify_with_attributes_and_locks(
        &mesh.indices,
        &adapter,
        &normals,
        &[NORMAL_WEIGHT; 3],
        3 * size_of::<f32>(),
        &locks,
        triangles * 3,
        f32::MAX,
        SimplifyOptions::ErrorAbsolute,
        Some(&mut error),
    );
    error
}

/// LOD 0 alone, the mesh in full with its triangles reordered for the
/// vertex cache, for a mesh imported without LODs.
pub fn full_lod(mesh: &MeshData) -> Vec<Lod> {
    vec![Lod { indices: optimize_vertex_cache(&mesh.indices, mesh.vertices.len()), error: 0.0 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_meshes_get_one_lod() {
        assert_eq!(build_lods(&MeshData::cube()).len(), 1);
    }

    #[test]
    fn dense_meshes_get_a_halving_chain_with_growing_error() {
        let sphere = MeshData::sphere(64, 32);
        let lods = build_lods(&sphere);
        assert!(lods.len() >= 4, "only {} LODs", lods.len());
        assert_eq!(lods[0].indices.len(), sphere.indices.len());
        for pair in lods.windows(2) {
            assert!(pair[1].indices.len() < pair[0].indices.len());
            assert!(pair[1].error >= pair[0].error);
        }
        assert!(lods.iter().all(|lod| lod.indices.iter().all(|&i| (i as usize) < sphere.vertices.len())));
    }
}
