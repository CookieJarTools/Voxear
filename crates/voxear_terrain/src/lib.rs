//! Terrain: the ground of a level as heights on a grid.
//!
//! A **Terrain** component on an Obj is a stretch of ground from the Obj's
//! position along +x and +z, as wide and long as its heights say. Its
//! heights are kept in tiles (see [`heights`]); the renderer draws them in
//! levels of detail ([`lod`]), physics stands things on them, and rays find
//! them ([`ray`]). Only the Obj's position places a terrain: it is not
//! turned or scaled, as ground is level with the world.
//!
//! This crate holds what a terrain is, with no GPU: the renderer, physics
//! and the engine each keep what they need of it, and see what changed by
//! the tiles' pointers.

pub mod brush;
pub mod composite;
pub mod file;
pub mod grass;
pub mod heights;
pub mod import;
pub mod layers;
pub mod lod;
pub mod noise;
pub mod raster;
pub mod ray;
pub mod surfaces;

use std::sync::Arc;

pub use heights::{HeightTile, Heightmap, TILE_QUADS, TILE_SAMPLES};
pub use ray::GroundHit;
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// A stretch of ground: its heights, its surfaces' shares, and how it is
/// drawn.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub heights: Arc<Heightmap>,
    /// Each sample's surfaces and their shares (see `surfaces`); none for
    /// ground drawn with one material.
    pub weights: Option<Arc<surfaces::Weights>>,
    /// How thickly each kind of grass grows at each sample (see `grass`);
    /// none for a terrain without grass.
    pub grass: Option<Arc<grass::GrassMap>>,
    /// Out to how far from the camera the ground is drawn at its finest, in
    /// metres; each coarser level is drawn out twice as far again.
    pub detail_distance: f32,
    /// Whether it casts shadows from the sun.
    pub cast_shadows: bool,
}

impl Component for Terrain {}

impl PartialEq for Terrain {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.heights, &other.heights) || self.heights == other.heights)
            && match (&self.weights, &other.weights) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
                (a, b) => a.is_none() && b.is_none(),
            }
            && match (&self.grass, &other.grass) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
                (a, b) => a.is_none() && b.is_none(),
            }
            && self.detail_distance == other.detail_distance
            && self.cast_shadows == other.cast_shadows
    }
}

impl Terrain {
    pub const DEFAULT_DETAIL_DISTANCE: f32 = 80.0;

    pub fn new(heights: Heightmap) -> Self {
        Self {
            heights: Arc::new(heights),
            weights: None,
            grass: None,
            detail_distance: Self::DEFAULT_DETAIL_DISTANCE,
            cast_shadows: true,
        }
    }
}

/// Where an Obj's terrain starts: its corner, at the Obj's position in the
/// world.
pub fn corner(transform: &Transform, placed: Option<&WorldTransform>) -> Vec3 {
    placed.map_or(transform.position, |world| world.0.w_axis.truncate())
}

/// The ground under a point: how high it is, which way it faces, the
/// terrain it is on, and which of its surfaces is most of it there (0 for
/// a terrain of one material).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub height: f32,
    pub normal: Vec3,
    pub terrain: Entity,
    pub surface: usize,
}

/// Every terrain in `world`, with its corner.
pub fn terrains(world: &World) -> Vec<(Entity, &Terrain, Vec3)> {
    world
        .query_ref::<(Entity, &Terrain, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, terrain, transform, placed)| (entity, terrain, corner(transform, placed)))
        .collect()
}

/// The ground at `(x, z)` in the world, on the first terrain that reaches
/// there and has no hole there.
pub fn ground_at(world: &World, x: f32, z: f32) -> Option<Ground> {
    terrains(world).into_iter().find_map(|(entity, terrain, corner)| {
        let (lx, lz) = (x - corner.x, z - corner.z);
        (terrain.heights.contains(lx, lz) && !terrain.heights.cut_at(lx, lz)).then(|| Ground {
            height: corner.y + terrain.heights.height_at(lx, lz),
            normal: terrain.heights.normal_at(lx, lz),
            terrain: entity,
            surface: terrain.weights.as_ref().map_or(0, |weights| weights.surface_at(terrain.heights.spacing(), lx, lz)),
        })
    })
}

/// The nearest place a ray in the world meets any terrain within
/// `max_distance`, in the world, and the terrain.
pub fn raycast(world: &World, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<(Entity, GroundHit)> {
    terrains(world)
        .into_iter()
        .filter_map(|(entity, terrain, corner)| {
            let hit = ray::raycast(&terrain.heights, origin - corner, direction, max_distance)?;
            Some((entity, GroundHit { point: hit.point + corner, ..hit }))
        })
        .min_by(|a, b| a.1.distance.total_cmp(&b.1.distance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_is_found_under_a_point_and_along_a_ray_where_the_obj_put_it() {
        let mut world = World::new();
        let heights = Heightmap::from_fn(1, 1, 1.0, |x, _| x * 0.1);
        let at = Vec3::new(100.0, 5.0, -50.0);
        let entity = world.spawn((Terrain::new(heights), Transform::new(at, Default::default(), Vec3::ONE)));
        let ground = ground_at(&world, 110.0, -40.0).expect("ground");
        assert_eq!(ground.terrain, entity);
        assert!((ground.height - 6.0).abs() < 1e-4);
        assert!(ground_at(&world, 90.0, 0.0).is_none());
        let (hit_entity, hit) = raycast(&world, Vec3::new(110.0, 50.0, -40.0), Vec3::NEG_Y, 100.0).expect("a hit");
        assert_eq!(hit_entity, entity);
        assert!((hit.point.y - 6.0).abs() < 1e-3 && (hit.point.x - 110.0).abs() < 1e-3);
    }
}
