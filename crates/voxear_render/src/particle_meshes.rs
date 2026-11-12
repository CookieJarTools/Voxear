//! The meshes a Particle System's particles can be drawn as, in place of a
//! flat picture: one built in, made here once when the renderer starts, or
//! the first mesh of one of the project's models, which the engine loads and
//! lists in [`ParticleMeshes`]. Each particle's mesh is as wide as the
//! particle's size across its furthest corners, whatever size it was made,
//! tumbles as the system's Spin says, and is drawn with its own material's
//! colour, roughness and metal under the system's colour: a built-in one's
//! is plain white.

use std::{collections::HashMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;

use crate::{
    assets::{MaterialHandle, MeshHandle},
    mesh::{MeshData, NO_WIND_MASKS, Vertex},
};

/// What a built-in mesh's reference starts with, as a file writes it:
/// `builtin:chunk`.
const BUILTIN: &str = "builtin:";

/// A mesh every project has, with few enough triangles to draw thousands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinMesh {
    /// A box: bricks, crates' splinters, blocks.
    Cube,
    /// A ball: shot, stones worn round, beads.
    Ball,
    /// A lump with flat faces, no two sides alike: rubble, clods, shrapnel.
    Chunk,
    /// A short rod: shell cases, splinters, twigs.
    Rod,
}

impl BuiltinMesh {
    pub const ALL: [Self; 4] = [Self::Cube, Self::Ball, Self::Chunk, Self::Rod];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cube => "Cube",
            Self::Ball => "Ball",
            Self::Chunk => "Chunk",
            Self::Rod => "Rod",
        }
    }

    /// As a file names it: `builtin:chunk`.
    pub fn reference(self) -> String {
        format!("{BUILTIN}{}", self.name().to_lowercase())
    }

    pub fn from_reference(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mesh| mesh.reference() == text)
    }

    /// Its triangles.
    pub fn mesh(self) -> MeshData {
        match self {
            Self::Cube => MeshData::cube(),
            Self::Ball => MeshData::sphere(12, 6),
            Self::Chunk => chunk(),
            Self::Rod => rod(),
        }
    }
}

/// A triangle with a flat face, wound so its front is the side away from
/// the mesh's middle.
fn face(mesh: &mut MeshData, [a, b, c]: [Vec3; 3]) {
    let mut corners = [a, b, c];
    let mut normal = (b - a).cross(c - a);
    if normal.dot(a + b + c) < 0.0 {
        corners.swap(1, 2);
        normal = -normal;
    }
    let normal = normal.normalize_or_zero();
    let along = (corners[1] - corners[0]).normalize_or_zero();
    let base = mesh.vertices.len() as u32;
    for (corner, uv) in corners.into_iter().zip([[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]) {
        mesh.vertices.push(Vertex {
            position: corner.into(),
            normal: normal.into(),
            uv,
            tangent: [along.x, along.y, along.z, 1.0],
            wind: NO_WIND_MASKS,
        });
    }
    mesh.indices.extend([base, base + 1, base + 2]);
}

/// A lump: a twenty-sided ball with each corner pulled in or out by its own
/// amount, a little flatter than it is wide.
fn chunk() -> MeshData {
    let golden = (1.0 + 5.0f32.sqrt()) / 2.0;
    let corners = [
        [-1.0, golden, 0.0],
        [1.0, golden, 0.0],
        [-1.0, -golden, 0.0],
        [1.0, -golden, 0.0],
        [0.0, -1.0, golden],
        [0.0, 1.0, golden],
        [0.0, -1.0, -golden],
        [0.0, 1.0, -golden],
        [golden, 0.0, -1.0],
        [golden, 0.0, 1.0],
        [-golden, 0.0, -1.0],
        [-golden, 0.0, 1.0],
    ];
    let faces = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    // How far out each corner is, from a little over half way to a little past the ball: fixed, so the lump is
    // the same every time.
    let reach = [1.0, 0.72, 0.86, 1.08, 0.66, 0.94, 1.1, 0.78, 0.9, 0.7, 1.04, 0.82];
    let corners: Vec<Vec3> = corners
        .into_iter()
        .zip(reach)
        .map(|(corner, reach)| Vec3::from(corner).normalize() * (0.5 * reach) * Vec3::new(1.0, 0.72, 0.9))
        .collect();
    let mut mesh = MeshData::default();
    for [a, b, c] in faces {
        face(&mut mesh, [corners[a], corners[b], corners[c]]);
    }
    mesh
}

/// A rod along Y, three and a half times as long as it is wide, with eight
/// sides and flat ends.
fn rod() -> MeshData {
    const SIDES: u32 = 8;
    let (radius, half) = (0.5 / 3.5, 0.5);
    let mut mesh = MeshData::default();
    let round = |side: u32| {
        let turn = side as f32 / SIDES as f32 * std::f32::consts::TAU;
        Vec3::new(turn.cos(), 0.0, turn.sin())
    };
    for side in 0..SIDES {
        let (from, to) = (round(side), round(side + 1));
        // The side between them, rounded: each corner's normal points straight out from the middle.
        let base = mesh.vertices.len() as u32;
        for (out, y) in [(from, -half), (to, -half), (to, half), (from, half)] {
            let tangent = Vec3::Y.cross(out);
            mesh.vertices.push(Vertex {
                position: (out * radius + Vec3::Y * y).into(),
                normal: out.into(),
                uv: [out.x * 0.5 + 0.5, y + 0.5],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind: NO_WIND_MASKS,
            });
        }
        let corner = |i: u32| Vec3::from(mesh.vertices[(base + i) as usize].position);
        // Wound so the front is outward, as every mesh's is.
        let outward = (corner(1) - corner(0)).cross(corner(2) - corner(0)).dot(from + to) > 0.0;
        let quad = if outward { [0, 1, 2, 0, 2, 3] } else { [0, 2, 1, 0, 3, 2] };
        mesh.indices.extend(quad.map(|i| base + i));
        for end in [-half, half] {
            let middle = Vec3::Y * end;
            face(&mut mesh, [middle, from * radius + middle, to * radius + middle]);
        }
    }
    mesh
}

/// The mesh a Particle System's particles are drawn as: one built in, or the
/// first mesh of one of the project's models. A file writes it as
/// `builtin:chunk` or as the model's GUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ParticleMesh {
    Builtin(BuiltinMesh),
    Asset(AssetGuid),
}

impl fmt::Display for ParticleMesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(mesh) => f.write_str(&mesh.reference()),
            Self::Asset(guid) => write!(f, "{guid}"),
        }
    }
}

impl FromStr for ParticleMesh {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if text.starts_with(BUILTIN) {
            let names: Vec<String> = BuiltinMesh::ALL.iter().map(|mesh| mesh.reference()).collect();
            let unknown = || format!("{text} is not a built-in particle mesh; they are {}", names.join(", "));
            return BuiltinMesh::from_reference(text).map(Self::Builtin).ok_or_else(unknown);
        }
        text.parse().map(Self::Asset).map_err(|_| format!("{text} is not a model's GUID"))
    }
}

impl TryFrom<String> for ParticleMesh {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

impl From<ParticleMesh> for String {
    fn from(mesh: ParticleMesh) -> Self {
        mesh.to_string()
    }
}

/// The project's models that Particle Systems draw their particles as, each
/// by its first mesh and that mesh's material as the renderer has them: a
/// resource of the world, kept by whoever loads models. A system whose
/// model is not listed yet draws nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticleMeshes(pub HashMap<AssetGuid, (MeshHandle, MaterialHandle)>);

/// A mesh as a draw of particles needs it: its place in the mesh table,
/// its material's among the materials, and its most detailed LOD's first
/// index and how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeshDraw {
    pub mesh: u32,
    pub material: u32,
    pub first_index: u32,
    pub index_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_mesh_is_closed_faces_outward_and_has_few_triangles() {
        for builtin in BuiltinMesh::ALL {
            let mesh = builtin.mesh();
            let triangles = mesh.indices.len() / 3;
            assert!((12..=200).contains(&triangles), "{}: {triangles} triangles", builtin.name());
            let (centre, radius) = mesh.bounds();
            assert!(centre.length() < 0.1 && (0.3..0.9).contains(&radius), "{}: {centre} {radius}", builtin.name());
            // Its faces enclose a volume, all wound the same way: outward, so it comes to more than nothing.
            let mut volume = 0.0;
            for triangle in mesh.indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[triangle[i] as usize].position));
                let normal = (b - a).cross(c - a);
                volume += a.dot(normal) / 6.0;
                let said: Vec3 = triangle.iter().map(|&i| Vec3::from(mesh.vertices[i as usize].normal)).sum();
                // A ball's faces come to a point at its poles, where they have no area to face any way.
                assert!(normal.dot(said) >= 0.0, "{}: a face wound against its normals", builtin.name());
            }
            assert!(volume > 0.01, "{}: {volume}", builtin.name());
        }
    }

    #[test]
    fn a_mesh_is_written_by_its_name_or_its_guid_and_read_back() {
        for builtin in BuiltinMesh::ALL {
            let mesh = ParticleMesh::Builtin(builtin);
            assert_eq!(mesh.to_string().parse::<ParticleMesh>(), Ok(mesh));
        }
        assert_eq!(ParticleMesh::Builtin(BuiltinMesh::Chunk).to_string(), "builtin:chunk");
        let guid: AssetGuid = "0123456789abcdef0123456789abcdef".parse().expect("a GUID");
        assert_eq!(guid.to_string().parse::<ParticleMesh>(), Ok(ParticleMesh::Asset(guid)));
        let unknown = "builtin:teapot".parse::<ParticleMesh>().expect_err("not one of them");
        assert!(unknown.contains("builtin:chunk"), "{unknown}");
    }
}
