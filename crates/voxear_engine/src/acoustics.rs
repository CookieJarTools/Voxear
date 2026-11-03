//! The level's walls as Steam Audio hears them: every collider that stays
//! put, a trigger's aside, made of its acoustic material. A collider with
//! a Rigidbody moves, and would have the walls built again every frame, so
//! it is left out; so is everything until a source with occlusion on plays.
//!
//! Boxes are their twelve triangles, spheres and capsules a few rings of
//! them, and a Mesh collider its own mesh. The walls are built again only
//! when one of the colliders changes, moves or goes.

use std::hash::{Hash, Hasher};

use voxear_audio::{
    Listener,
    steam_audio::{AcousticMaterial, AcousticMesh, Acoustics, Muffle, SourceAcoustics},
};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::World;
use voxear_physics::scene::{Acoustic, Collider, ColliderMesh, ColliderShape, Rigidbody};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// Rings from pole to pole, and points round each, of a sphere's or a
/// capsule's walls: coarse, as sound does not need more.
const RINGS: usize = 6;
const AROUND: usize = 8;

pub(crate) struct Walls {
    acoustics: Acoustics,
    /// What the walls were last built from.
    built: Option<u64>,
}

impl Walls {
    pub fn new() -> Result<Self, String> {
        Ok(Self { acoustics: Acoustics::new()?, built: None })
    }

    /// How much of each voice, by its key, reaches the listener past the
    /// level's walls, building them again first if they changed.
    pub fn hear(&mut self, world: &World, listener: &Listener, voices: &[(u64, Vec3, SourceAcoustics)]) -> Vec<Muffle> {
        let colliders = still_colliders(world);
        let key = fingerprint(&colliders);
        if self.built != Some(key) {
            let shapes: Vec<_> =
                colliders.iter().map(|(collider, matrix, mesh)| shape(collider, matrix, *mesh)).collect();
            let meshes: Vec<AcousticMesh> = shapes
                .iter()
                .zip(&colliders)
                .map(|((vertices, triangles), (collider, ..))| AcousticMesh {
                    vertices,
                    triangles,
                    material: material(collider.acoustic),
                })
                .collect();
            self.acoustics.set_geometry(&meshes);
            self.built = Some(key);
        }
        self.acoustics.hear(listener, voices)
    }
}

/// Steam Audio's preset for a collider's material.
pub fn material(acoustic: Acoustic) -> AcousticMaterial {
    match acoustic {
        Acoustic::Generic => AcousticMaterial::GENERIC,
        Acoustic::Brick => AcousticMaterial::BRICK,
        Acoustic::Concrete => AcousticMaterial::CONCRETE,
        Acoustic::Ceramic => AcousticMaterial::CERAMIC,
        Acoustic::Gravel => AcousticMaterial::GRAVEL,
        Acoustic::Carpet => AcousticMaterial::CARPET,
        Acoustic::Glass => AcousticMaterial::GLASS,
        Acoustic::Plaster => AcousticMaterial::PLASTER,
        Acoustic::Wood => AcousticMaterial::WOOD,
        Acoustic::Metal => AcousticMaterial::METAL,
        Acoustic::Rock => AcousticMaterial::ROCK,
    }
}

/// The colliders that are walls: neither triggers nor moved by physics,
/// each with where it is and its mesh, for a Mesh collider.
fn still_colliders(world: &World) -> Vec<(Collider, Mat4, Option<&ColliderMesh>)> {
    world
        .query_ref::<(&Collider, &Transform, Option<&WorldTransform>, Option<&ColliderMesh>)>()
        .without::<Rigidbody>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(collider, ..)| !collider.trigger)
        .map(|(collider, local, placed, mesh)| {
            (*collider, placed.map_or_else(|| local.matrix(), |placed| placed.0), mesh)
        })
        .collect()
}

/// A number that changes when any wall does.
fn fingerprint(colliders: &[(Collider, Mat4, Option<&ColliderMesh>)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (collider, matrix, mesh) in colliders {
        let numbers = collider.center.iter().chain(&collider.size).chain([&collider.radius, &collider.height]);
        numbers.chain(&matrix.to_cols_array()).for_each(|n| n.to_bits().hash(&mut hasher));
        (collider.shape as u8, collider.acoustic, mesh.map(|mesh| mesh.key)).hash(&mut hasher);
    }
    hasher.finish()
}

/// A collider's triangles in the world.
fn shape(collider: &Collider, matrix: &Mat4, mesh: Option<&ColliderMesh>) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let center = Vec3::from(collider.center);
    let (points, triangles) = match collider.shape {
        ColliderShape::Box => cube(center, Vec3::from(collider.size) * 0.5),
        ColliderShape::Sphere => rings(center, collider.radius, 0.0),
        ColliderShape::Capsule => rings(center, collider.radius, (collider.height * 0.5 - collider.radius).max(0.0)),
        ColliderShape::Mesh => match mesh {
            Some(mesh) => (mesh.points.to_vec(), mesh.indices.as_chunks::<3>().0.to_vec()),
            None => (Vec::new(), Vec::new()),
        },
    };
    (points.into_iter().map(|point| matrix.transform_point3(point)).collect(), triangles)
}

fn cube(center: Vec3, half: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let corner = |i: u32| center + half * Vec3::new(sign(i & 1), sign(i & 2), sign(i & 4));
    let points = (0..8).map(corner).collect();
    let faces = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let triangles = faces.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect();
    (points, triangles)
}

fn sign(bit: u32) -> f32 {
    if bit == 0 { -1.0 } else { 1.0 }
}

/// A sphere of `radius`, or with `stretch` a capsule: its top half that far
/// up and its bottom half that far down, the two joined by a cylinder.
fn rings(center: Vec3, radius: f32, stretch: f32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    // From the top pole down, the middle ring twice, once for each half.
    let latitudes = (0..=RINGS / 2).map(|ring| (ring, stretch)).chain((RINGS / 2..=RINGS).map(|ring| (ring, -stretch)));
    let mut points = Vec::new();
    for (ring, lift) in latitudes {
        let down = ring as f32 / RINGS as f32 * std::f32::consts::PI;
        for step in 0..AROUND {
            let round = step as f32 / AROUND as f32 * std::f32::consts::TAU;
            let at = Vec3::new(down.sin() * round.cos(), down.cos(), down.sin() * round.sin()) * radius;
            points.push(center + at + Vec3::Y * lift);
        }
    }
    let ring_count = (points.len() / AROUND) as u32;
    let around = AROUND as u32;
    let mut triangles = Vec::new();
    for ring in 0..ring_count - 1 {
        for step in 0..around {
            let [a, b] = [ring * around + step, ring * around + (step + 1) % around];
            let [c, d] = [a + around, b + around];
            triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    (points, triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_is_twelve_triangles_round_its_corners() {
        let collider = Collider { size: [2.0, 4.0, 6.0], center: [1.0, 0.0, 0.0], ..Collider::default() };
        let (points, triangles) = shape(&collider, &Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)), None);
        assert_eq!((points.len(), triangles.len()), (8, 12));
        assert!(points.contains(&Vec3::new(2.0, 12.0, 3.0)) && points.contains(&Vec3::new(0.0, 8.0, -3.0)));
    }

    #[test]
    fn a_capsule_reaches_its_height_and_a_sphere_its_radius() {
        let capsule = Collider { shape: ColliderShape::Capsule, radius: 0.5, height: 3.0, ..Collider::default() };
        let (points, _) = shape(&capsule, &Mat4::IDENTITY, None);
        let top = points.iter().fold(f32::MIN, |most, p| most.max(p.y));
        let bottom = points.iter().fold(f32::MAX, |least, p| least.min(p.y));
        assert!((top - 1.5).abs() < 1e-4 && (bottom + 1.5).abs() < 1e-4, "{bottom} to {top}");

        let sphere = Collider { shape: ColliderShape::Sphere, radius: 2.0, ..Collider::default() };
        let (points, triangles) = shape(&sphere, &Mat4::IDENTITY, None);
        assert!(points.iter().all(|p| (p.length() - 2.0).abs() < 1e-4));
        assert!(triangles.iter().flatten().all(|&i| (i as usize) < points.len()));
    }
}
