//! The floating origin in a game: when the world's middle moves, and what
//! moves with it.
//!
//! Far from the middle an f32 position loses its smallest steps (see
//! `voxear_scene::WorldOrigin`), so while a game plays, once the viewer
//! terrain is made round (see [`crate::terrain::viewer`]) is more than the
//! project's distance from the middle, the world moves back by whole steps,
//! between two frames ([`due`], then `Game` moving the world, its sound and
//! its drawing). Everything the game keeps in the world moves with it here
//! ([`shift`]): the Objs, physics, and what effects, animation and guns keep
//! of where things were. Ground made round the viewer counts its tiles and
//! chunks from its own Obj, so none is made again.
//!
//! ```toml
//! [floating_origin]
//! mode = "automatic"   # or "always", or "never"
//! distance = 2048      # metres from the middle before it moves
//! step = 1024          # metres it moves by, a whole number of them
//! ```

use serde::{Deserialize, Serialize};
use voxear_core::math::{IVec3, Vec3};
use voxear_ecs::World;
use voxear_physics::scene::PhysicsScene;
use voxear_scene::{Destroying, WorldOrigin};

use crate::terrain::{TerrainAsset, TerrainViewer};

/// When the world's middle moves with the viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OriginMode {
    /// In levels with ground without end, or a planet, only: a level of a
    /// set size keeps its middle where it is.
    #[default]
    Automatic,
    Always,
    Never,
}

/// The project's `[floating_origin]`: when the world's middle moves, how far
/// the viewer gets from it first, and how far it moves.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OriginSettings {
    pub mode: OriginMode,
    /// Metres from the middle, along any axis, before it moves.
    pub distance: f32,
    /// Metres it moves by, a whole number of them. The default is a whole
    /// number of terrain tiles and of block chunks, and of every grid the
    /// renderer keeps in the world.
    pub step: u32,
}

impl Default for OriginSettings {
    fn default() -> Self {
        Self { mode: OriginMode::Automatic, distance: 2048.0, step: 1024 }
    }
}

impl OriginSettings {
    /// How far everything moves to bring `viewer` back near the middle, in
    /// whole steps across the ground, x and z: none while it is within the
    /// distance along both. Heights stay as they are, as the sky and fog are
    /// measured from the level's ground.
    pub fn move_for(&self, viewer: Vec3) -> Option<IVec3> {
        let step = self.step.max(1) as f32;
        let across = Vec3::new(viewer.x, 0.0, viewer.z);
        let far = across.abs().max_element() > self.distance.max(step * 0.5) && across.is_finite();
        far.then(|| -((across / step).round() * step).as_ivec3())
    }
}

/// The move due now, if the world's middle should move: never in a
/// networked game, whose machines would each have their own middle.
pub fn due(world: &World, settings: &OriginSettings) -> Option<IVec3> {
    let wanted = match settings.mode {
        OriginMode::Never => false,
        OriginMode::Always => true,
        OriginMode::Automatic => without_end(world),
    };
    if !wanted || world.resource::<crate::net::NetSession>().is_some() {
        return None;
    }
    settings.move_for(crate::terrain::viewer(world)?).filter(|by| *by != IVec3::ZERO)
}

/// Whether the world has ground without end or a planet, which a game can
/// go as far into as it likes.
fn without_end(world: &World) -> bool {
    let endless_terrain = world
        .query_ref::<&TerrainAsset>()
        .without::<Destroying>()
        .into_iter()
        .any(|asset| asset.file.endless.is_some());
    let endless_blocks = || {
        world.query_ref::<&crate::blocks::BlockWorldAsset>().without::<Destroying>().into_iter().any(|asset| {
            asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some() || generate.planet.is_some())
        })
    };
    endless_terrain || endless_blocks()
}

/// Moves everything kept in `world` by `by` whole metres, and gives the new
/// origin: every Obj, the poses kept between ticks, physics, where terrain
/// is made round, and what smokes, mists, animation, first-person eyes, gun
/// holders and debug drawing keep of where things were. Sound, the
/// renderer and scripts are moved by their owners.
pub fn shift(world: &mut World, by: IVec3) -> WorldOrigin {
    let origin = voxear_scene::move_objs(world, by);
    let moved = by.as_vec3();
    crate::tick_smoothing::shift(world, moved);
    if let Some(viewer) = world.resource_mut::<TerrainViewer>() {
        viewer.0 += moved;
    }
    if let Some(mut physics) = world.remove_resource::<PhysicsScene>() {
        physics.shift(world, moved);
        world.insert_resource(physics);
    }
    crate::smoke::shift(world, moved);
    crate::mist::shift(world, moved);
    crate::animation::MotionMatcherState::shift(world, moved);
    crate::first_person::FirstPersonState::shift(world, moved);
    crate::guns::holder::GunHolderState::shift(world, moved);
    voxear_core::debug_draw::shift(moved);
    tracing::debug!(target: "origin", "the world moved by {by}: its middle is {} from the level's", origin.0);
    origin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_by_whole_steps_once_past_the_distance() {
        let settings = OriginSettings::default();
        assert_eq!(settings.move_for(Vec3::new(2000.0, 0.0, -1500.0)), None);
        // Past 2 km on x: back by two steps, z with it to the nearest step, and never up or down.
        assert_eq!(settings.move_for(Vec3::new(2100.0, 3000.0, -1600.0)), Some(IVec3::new(-2048, 0, 2048)));
        assert_eq!(settings.move_for(Vec3::new(0.0, -5000.0, 0.0)), None);
        assert_eq!(settings.move_for(Vec3::NAN), None);
    }

    #[test]
    fn reads_from_the_project_file() {
        let settings: OriginSettings = toml::from_str("mode = \"always\"\nstep = 512").unwrap();
        assert_eq!(settings, OriginSettings { mode: OriginMode::Always, distance: 2048.0, step: 512 });
    }
}
