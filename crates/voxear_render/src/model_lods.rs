//! A model's own levels of detail, as artists make them and Unity reads
//! them: parts named for one mesh with `_LOD0`, `_LOD1` and on after it, the
//! first in full and each later one simpler. They become one part, called
//! by the name before the suffix, whose mesh has those LODs, picked on the
//! GPU as made ones are, so a model placed in a level never shows its later
//! LODs as parts of their own. With `own_lods` off in its import settings,
//! the part draws with LODs made from its first instead, as any mesh does.
//!
//! Parts named as Unreal names collision meshes, `UCX_`, `UBX_`, `USP_` and
//! `UCP_` before the mesh's name, are left out too: models made for Unreal,
//! as many in asset stores are, carry them for its physics, and drawn they
//! would cover the model in boxes.
//!
//! A model's LODs are separate meshes, each with its own vertices, so the
//! first part's mesh takes theirs after its own, and each LOD's triangles
//! point into its share. Each is given the error a simplification of the
//! first to as many triangles would have, so the GPU picks it at the same
//! distance it would pick a made LOD of that size.

use std::collections::HashMap;

use voxear_assets::ModelImport;
use voxear_core::math::{Mat3, Mat4, Vec3};

use crate::{
    gltf_import::{PreparedModel, PreparedNode},
    lod::{self, Lod},
};

/// The name a part has without its LOD's suffix, and which LOD it is, for
/// names such as `Rock_LOD2` or `Rock_lod0`.
pub fn lod_of(name: &str) -> Option<(&str, u32)> {
    let at = name.to_ascii_lowercase().rfind("_lod")?;
    let level = name[at + 4..].parse().ok()?;
    Some((&name[..at], level))
}

/// Whether a part is one of Unreal's collision meshes, by its name.
pub fn is_collision(name: &str) -> bool {
    ["UCX_", "UBX_", "USP_", "UCP_"].iter().any(|prefix| name.starts_with(prefix))
}

/// Folds each mesh's own LODs into its first part, and leaves the later
/// LODs' parts, collision meshes, and the meshes only they used, out of the
/// model.
pub(crate) fn fold(model: &mut PreparedModel, settings: &ModelImport) {
    model.nodes.retain(|node| !is_collision(&node.name));
    let mut groups: HashMap<String, Vec<(u32, usize)>> = HashMap::new();
    for (index, node) in model.nodes.iter().enumerate() {
        if let Some((base, level)) = lod_of(&node.name) {
            groups.entry(base.to_string()).or_default().push((level, index));
        }
    }
    let mut later_parts = Vec::new();
    for (base, mut parts) in groups {
        parts.sort_unstable();
        let (firsts, later): (Vec<_>, Vec<_>) = parts.into_iter().partition(|&(level, _)| level == 0);
        if firsts.is_empty() || later.is_empty() {
            continue;
        }
        for &(_, first) in &firsts {
            if settings.own_lods {
                let pieces = matching_pieces(&model.nodes, first, &later);
                take_lods(model, first, &pieces);
            }
            model.nodes[first].name = base.clone();
        }
        later_parts.extend(later.into_iter().map(|(_, part)| part));
    }
    later_parts.sort_unstable();
    for part in later_parts.into_iter().rev() {
        model.nodes.remove(part);
    }
    drop_unused_meshes(model);
}

/// Each later LOD's piece that goes with the first's piece `first`: the one
/// with the same material, or the only one when its LOD has one alone.
fn matching_pieces(nodes: &[PreparedNode], first: usize, later: &[(u32, usize)]) -> Vec<usize> {
    let material = &nodes[first].material_part;
    let mut levels: Vec<u32> = later.iter().map(|&(level, _)| level).collect();
    levels.dedup();
    levels
        .into_iter()
        .filter_map(|level| {
            let pieces: Vec<usize> = later.iter().filter(|&&(at, _)| at == level).map(|&(_, part)| part).collect();
            let same = pieces.iter().copied().find(|&part| nodes[part].material_part == *material);
            same.or_else(|| (pieces.len() == 1).then(|| pieces[0]))
        })
        .collect()
}

/// Gives the first part's mesh the later LODs' pieces as its LODs, each
/// moved from where its part sits to where the first part does.
fn take_lods(model: &mut PreparedModel, first: usize, pieces: &[usize]) {
    let node = &model.nodes[first];
    let to_first = Mat4::from_scale_rotation_translation(node.scale, node.rotation, node.position).inverse();
    let (full, made) = &model.meshes[node.mesh];
    let (mut data, mut lods) = (full.clone(), vec![made[0].clone()]);
    for &piece in pieces {
        let other = &model.nodes[piece];
        let into = to_first * Mat4::from_scale_rotation_translation(other.scale, other.rotation, other.position);
        let turn = Mat3::from_mat4(into);
        let offset = data.vertices.len() as u32;
        let (mesh, _) = &model.meshes[other.mesh];
        data.vertices.extend(mesh.vertices.iter().map(|vertex| {
            let mut moved = *vertex;
            moved.position = into.transform_point3(Vec3::from(vertex.position)).into();
            moved.normal = (turn * Vec3::from(vertex.normal)).normalize_or_zero().into();
            let tangent = (turn * Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2])).normalize_or_zero();
            moved.tangent = [tangent.x, tangent.y, tangent.z, vertex.tangent[3]];
            moved
        }));
        // A skinned mesh's LODs bring their skin along, or are held to the first bone without one.
        if !data.skin.is_empty() {
            let held = (0..mesh.vertices.len()).map(|i| mesh.skin.get(i).copied().unwrap_or_default());
            data.skin.extend(held);
        }
        let indices: Vec<u32> = mesh.indices.iter().map(|&index| index + offset).collect();
        // Never smaller than the last, so a farther LOD is never picked before a nearer one.
        let error = lod::error_at(full, indices.len() / 3).max(lods.last().map_or(0.0, |lod: &Lod| lod.error * 1.01));
        lods.push(Lod::new(&indices, error, &data.vertices));
    }
    model.meshes[model.nodes[first].mesh] = (data, lods);
}

/// Takes out the meshes no part uses now, and points the parts at where
/// theirs moved to.
fn drop_unused_meshes(model: &mut PreparedModel) {
    let mut used = vec![false; model.meshes.len()];
    for node in &model.nodes {
        used[node.mesh] = true;
    }
    let mut moved_to = vec![0; model.meshes.len()];
    let mut kept = 0;
    for (index, &is_used) in used.iter().enumerate() {
        moved_to[index] = kept;
        kept += usize::from(is_used);
    }
    let mut index = 0;
    model.meshes.retain(|_| {
        index += 1;
        used[index - 1]
    });
    for node in &mut model.nodes {
        node.mesh = moved_to[node.mesh];
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;
    use crate::{lod::build_lods, mesh::MeshData};

    fn part(name: &str, mesh: usize, position: Vec3) -> PreparedNode {
        PreparedNode {
            name: name.into(),
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            mesh,
            material: Some(0),
            mesh_part: format!("mesh{mesh}"),
            material_part: Some("material0".into()),
            skin: None,
        }
    }

    /// A rock in three LODs, its second sitting a metre along X, and a crate.
    fn rock_and_crate() -> PreparedModel {
        let meshes = [MeshData::sphere(64, 32), MeshData::sphere(32, 16), MeshData::sphere(12, 6), MeshData::cube()];
        PreparedModel {
            path: "Models/Rock.fbx".into(),
            meshes: meshes.into_iter().map(|mesh| (mesh.clone(), build_lods(&mesh))).collect(),
            textures: Vec::new(),
            materials: Vec::new(),
            nodes: vec![
                part("Rock_LOD0", 0, Vec3::ZERO),
                part("Rock_LOD1", 1, Vec3::X),
                part("Rock_lod2", 2, Vec3::ZERO),
                part("Crate", 3, Vec3::Z),
                part("UCX_Crate_LOD0_01", 3, Vec3::Z),
            ],
            animation: None,
            roles: Vec::new(),
        }
    }

    #[test]
    fn names_say_which_lod_a_part_is() {
        assert_eq!(lod_of("Rock_LOD2"), Some(("Rock", 2)));
        assert_eq!(lod_of("Old_Rock_lod0"), Some(("Old_Rock", 0)));
        assert_eq!(lod_of("Lodge"), None);
        assert_eq!(lod_of("Rock_LODs"), None);
    }

    #[test]
    fn a_models_own_lods_become_its_meshes_lods() {
        let mut model = rock_and_crate();
        fold(&mut model, &ModelImport::default());
        let names: Vec<&str> = model.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Rock", "Crate"], "one part for the rock, called by its own name, and no collision mesh");
        assert_eq!(model.meshes.len(), 2, "the later LODs' meshes are gone");
        assert_eq!(model.nodes[1].mesh, 1, "and the crate points at where its mesh moved");
        assert_eq!(model.nodes[0].mesh_part, "mesh0", "the rock keeps its first part's name, for what refers to it");

        let (mesh, lods) = &model.meshes[0];
        let triangles = |segments, rings| MeshData::sphere(segments, rings).indices.len();
        let counts: Vec<usize> = lods.iter().map(|lod| lod.indices.len()).collect();
        assert_eq!(counts, [triangles(64, 32), triangles(32, 16), triangles(12, 6)], "the model's own, in order");
        assert!(lods.iter().all(|lod| lod.indices.iter().all(|&i| (i as usize) < mesh.vertices.len())));
        assert_eq!(lods[0].error, 0.0);
        assert!(lods[1].error > 0.0 && lods[2].error > lods[1].error, "farther LODs stray further");

        // The second LOD sat a metre along: its vertices are moved to where the first sits.
        let first_count = MeshData::sphere(64, 32).vertices.len();
        let second = &mesh.vertices[first_count..first_count + MeshData::sphere(32, 16).vertices.len()];
        let points = second.iter().map(|v| Vec3::from(v.position));
        let (low, high) = points.fold((Vec3::MAX, Vec3::MIN), |(low, high), p| (low.min(p), high.max(p)));
        let middle = (low + high) / 2.0;
        assert!(middle.abs_diff_eq(Vec3::X, 1e-3), "{middle}");
    }

    #[test]
    fn with_its_own_lods_off_a_model_makes_them_from_its_first() {
        let mut model = rock_and_crate();
        fold(&mut model, &ModelImport { own_lods: false, ..ModelImport::default() });
        let names: Vec<&str> = model.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Rock", "Crate"], "its later LODs are still not parts of their own");
        let (mesh, lods) = &model.meshes[0];
        assert_eq!(mesh.vertices.len(), MeshData::sphere(64, 32).vertices.len(), "only the first's vertices");
        assert_eq!(lods.len(), build_lods(&MeshData::sphere(64, 32)).len(), "and LODs made from it");
    }
}
