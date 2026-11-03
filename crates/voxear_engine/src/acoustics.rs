//! The level's walls as Steam Audio hears them: every collider that stays
//! put, a trigger's aside, made of its acoustic material. A collider with
//! a Rigidbody moves, and would have the walls built again every frame, so
//! it is left out.
//!
//! Boxes are their twelve triangles, spheres and capsules a few rings of
//! them, and a Mesh collider its own mesh. The walls are built again only
//! when one of the colliders changes, moves or goes.
//!
//! Steam Audio uses them twice: for what gets past them from each source
//! with occlusion on, and, with reverb on, for how long the listener's room
//! rings. Each starts the first time it is wanted.
//!
//! The editor bakes the paths sound takes round them into a `.paths` file
//! beside the level (see [`paths_file`]). While a level with one plays,
//! sources with pathing on are heard round the walls along them too.

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

pub use voxear_audio::steam_audio::{BakedPaths, bake_paths};
use voxear_audio::{
    Listener, ReverbTimes,
    steam_audio::{AcousticMaterial, Acoustics, Heard, PathProbes, RoomReverb, SourceAcoustics, Wall},
};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::World;
use voxear_physics::scene::{Acoustic, Collider, ColliderMesh, ColliderShape, Rigidbody};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// Rings from pole to pole, and points round each, of a sphere's or a
/// capsule's walls: coarse, as sound does not need more.
const RINGS: usize = 6;
const AROUND: usize = 8;

#[derive(Default)]
pub(crate) struct Walls {
    walls: Arc<[Wall]>,
    /// What the walls were last built from, and which of those builds the
    /// occlusion and the room have.
    built: Option<u64>,
    occlusion: Option<(Acoustics, Option<u64>)>,
    room: Option<(RoomReverb, Option<u64>)>,
    /// The level whose baked paths these are, and the `.paths` file's
    /// bytes, if it has one; the paths loaded from them, once wanted; and
    /// whether the occlusion has them.
    level: Option<String>,
    paths_file: Option<Arc<[u8]>>,
    paths: Option<Arc<PathProbes>>,
    has_paths: bool,
    /// Steam Audio is not here, which has been said.
    missing: bool,
}

impl Walls {
    /// The level whose paths these are, as [`set_paths`](Self::set_paths)
    /// was last told.
    pub fn level(&self) -> Option<&str> {
        self.level.as_deref()
    }

    /// The open level's baked paths, from its `.paths` file, if it has one.
    pub fn set_paths(&mut self, level: Option<&str>, file: Option<Vec<u8>>) {
        self.level = level.map(str::to_string);
        self.paths_file = file.map(Arc::from);
        self.paths = None;
        self.has_paths = false;
        if let Some((acoustics, _)) = &mut self.occlusion {
            acoustics.set_paths(None);
        }
    }

    /// How much of each voice, by its key, reaches the listener past the
    /// walls, and round them along the level's paths; none without Steam
    /// Audio.
    pub fn hear(
        &mut self,
        world: &World,
        listener: &Listener,
        voices: &[(u64, Vec3, SourceAcoustics)],
    ) -> Option<Vec<Heard>> {
        self.keep_up(world);
        if self.occlusion.is_none() {
            self.occlusion = self.started(Acoustics::new()).map(|acoustics| (acoustics, None));
        }
        if !self.has_paths && voices.iter().any(|(.., settings)| settings.pathing) {
            self.load_paths();
        }
        let (acoustics, has) = self.occlusion.as_mut()?;
        if *has != self.built {
            acoustics.set_geometry(&self.walls.iter().map(Wall::mesh).collect::<Vec<_>>());
            *has = self.built;
        }
        if !self.has_paths && self.paths.is_some() {
            acoustics.set_paths(self.paths.clone());
            self.has_paths = true;
        }
        Some(acoustics.hear(listener, voices))
    }

    /// Loads the paths from the level's `.paths` file the first time a
    /// voice would follow them, saying why not when they will not load.
    fn load_paths(&mut self) {
        if self.paths.is_some() || self.missing {
            return;
        }
        let Some(file) = self.paths_file.take() else { return };
        match PathProbes::load(&file) {
            Ok(paths) => {
                tracing::info!(target: "audio", "Sound follows {} probes' paths round the walls", paths.probes());
                self.paths = Some(paths);
            }
            Err(error) => {
                let level = self.level.as_deref().unwrap_or("the level");
                tracing::warn!(target: "audio", "{level}'s paths would not load, so sounds go without: {error}");
            }
        }
    }

    /// How long the listener's room rings, when that has been worked out
    /// again since the last time.
    pub fn room(&mut self, world: &World, listener: &Listener) -> Option<ReverbTimes> {
        self.keep_up(world);
        if self.room.is_none() {
            self.room = self.started(RoomReverb::start()).map(|room| (room, None));
        }
        let (room, has) = self.room.as_mut()?;
        if *has != self.built {
            room.set_walls(self.walls.clone());
            *has = self.built;
        }
        room.hear_from(*listener)
    }

    /// Builds the walls again when a collider changed since the last time.
    fn keep_up(&mut self, world: &World) {
        let colliders = still_colliders(world);
        let key = fingerprint(&colliders);
        if self.built == Some(key) {
            return;
        }
        self.walls = colliders.iter().map(wall).collect();
        self.built = Some(key);
    }

    /// What Steam Audio made, or nothing, saying why the first time.
    fn started<T>(&mut self, made: Result<T, String>) -> Option<T> {
        match made {
            Ok(made) => Some(made),
            Err(_) if self.missing => None,
            Err(error) => {
                tracing::warn!(target: "audio", "Sounds play without occlusion or reverb: {error}");
                self.missing = true;
                None
            }
        }
    }
}

/// The walls in `world`, as Steam Audio hears them, to bake paths round.
pub fn walls(world: &World) -> Vec<Wall> {
    still_colliders(world).iter().map(wall).collect()
}

/// Where a level's baked paths are kept: beside it, named as it is, as
/// `Levels/Main.level`'s in `Levels/Main.paths`.
pub fn paths_file(level: &str) -> String {
    let stem = level.strip_suffix(".level").unwrap_or(level);
    format!("{stem}.paths")
}

fn wall((collider, matrix, mesh): &(Collider, Mat4, Option<&ColliderMesh>)) -> Wall {
    let (vertices, triangles) = shape(collider, matrix, *mesh);
    Wall { vertices, triangles, material: material(collider.acoustic) }
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
    fn a_levels_paths_are_kept_beside_it() {
        assert_eq!(paths_file("Levels/Main.level"), "Levels/Main.paths");
        assert_eq!(paths_file("Start.level"), "Start.paths");
    }

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
