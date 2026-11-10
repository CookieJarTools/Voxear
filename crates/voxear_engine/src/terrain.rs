//! Terrains in a game (the Terrain module).
//!
//! Each Terrain loaded from a `.terrain` asset carries a [`TerrainAsset`]:
//! which asset it is, and its file as the game has it now. The editor
//! changes a terrain's heights and file in the world; [`keep`] keeps them in
//! the game's content as the asset's, as Unity keeps a terrain's data in
//! memory, so a level copied into play, or loaded again, has the ground as
//! it was left, and undoing an edit puts the asset back too. Saving the
//! level writes each changed asset's `.terrain` and `.heights` ([`save`]).
//!
//! A terrain without end is made round the camera as it goes ([`stream`]),
//! a tile at a time on the terrain's background threads, and keeps no
//! heights.

use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_render::Camera;
use voxear_scene::{Destroying, EditorOnly, Switch, Transform, WorldTransform};
use voxear_terrain::{
    Terrain,
    endless::{Making, Streamer},
    file::TerrainFile,
};

use crate::Content;

/// A Terrain's `.terrain` asset: its file and what its brushes painted, as
/// the game has them now. The rasters are shared, so an undo step keeps only
/// the tiles a stroke changed.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainAsset {
    pub guid: AssetGuid,
    pub file: TerrainFile,
    pub rasters: std::sync::Arc<voxear_terrain::raster::Rasters>,
}

impl Component for TerrainAsset {}

/// Keeps every terrain's heights and file in `content` as its asset's, as
/// the world has them now, marking those that changed to be written.
pub fn keep(world: &World, content: &mut Content) {
    let terrains = world.query_ref::<(&Terrain, &TerrainAsset)>().without::<Destroying>();
    for (terrain, asset) in terrains {
        content.keep_terrain(asset.guid, &asset.file, terrain, &asset.rasters);
    }
}

/// Writes each terrain asset changed since it was last written into the
/// project's folder: its `.terrain`, its `.heights` (with its holes and its
/// surfaces' shares), and its `.layers` when its brushes painted anything;
/// gives the paths written.
pub fn save(content: &mut Content, project: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for (path, loaded) in content.take_unsaved_terrains() {
        let file = project.join(&path);
        std::fs::write(&file, loaded.file.to_text()).with_context(|| format!("saving {path}"))?;
        let heights = voxear_terrain::file::heights_file(&path);
        // One without end keeps no heights: its ground is made as it is seen.
        if loaded.file.endless.is_some() {
            if project.join(&heights).is_file() {
                std::fs::remove_file(project.join(&heights)).with_context(|| format!("removing {heights}"))?;
            }
            written.push(path);
            continue;
        }
        // The shares are kept too, so a game need not work them out as it loads.
        let surfaces = voxear_terrain::surfaces::hash_surfaces(&loaded.file.surfaces);
        let weights = loaded.weights.as_deref().map(|weights| (weights, surfaces));
        let grown = voxear_terrain::grass::hash_grass(&loaded.file.grass, surfaces);
        let grass = loaded.grass.as_deref().map(|grass| (grass, grown));
        // Without the holes digging cut, which its layers cut again as it loads.
        let painted = |tx, tz| voxear_terrain::composite::hole_bits(&loaded.file, &loaded.rasters, tx, tz);
        let undug = loaded.dug.as_deref().map(|dug| voxear_terrain::dig::undug(&loaded.heights, dug, painted));
        let ground = undug.as_ref().unwrap_or(&loaded.heights);
        let bytes = voxear_terrain::file::write_ground(ground, loaded.file.shape_hash(), weights, grass);
        std::fs::write(project.join(&heights), bytes).with_context(|| format!("saving {heights}"))?;
        let layers = project.join(voxear_terrain::raster::layers_file(&path));
        if loaded.rasters.tiles().next().is_some() {
            std::fs::write(&layers, voxear_terrain::raster::write_rasters(&loaded.rasters))
                .with_context(|| format!("saving {}", layers.display()))?;
        } else if layers.is_file() {
            std::fs::remove_file(&layers).with_context(|| format!("removing {}", layers.display()))?;
        }
        written.push(path);
    }
    Ok(written)
}

/// Where terrains without end are made round, in the world, when not the
/// game's camera: where the editor's Scene view looks from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainViewer(pub Vec3);

/// Each terrain without end's streamer, by its Obj.
#[derive(Default)]
struct Streams(HashMap<Entity, Streamer>);

/// Where ground made round the camera is made round: the
/// [`TerrainViewer`] where there is one, or else the game's camera.
pub(crate) fn viewer(world: &World) -> Option<Vec3> {
    world.resource::<TerrainViewer>().map(|viewer| viewer.0).or_else(|| camera_position(world))
}

/// Where the game looks from: the first camera switched on, of the game's
/// own, not the editor's.
fn camera_position(world: &World) -> Option<Vec3> {
    world
        .query_ref::<(Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| voxear_scene::is_on(world, *entity, Switch::Camera))
        .min_by_key(|(entity, ..)| entity.index())
        .map(|(_, _, local, placed)| placed.map_or(local.position, |placed| placed.0.w_axis.truncate()))
}

/// Brings each terrain without end's ground round where it is seen from:
/// the [`TerrainViewer`] where there is one, or else the game's camera. The
/// tiles nearest are made at once, the rest on the terrain's background
/// threads, taken in as they are done. Without a camera, left as it is.
pub fn stream(world: &mut World, content: &mut Content) {
    let endless: Vec<(Entity, TerrainAsset, Vec3)> = world
        .query_ref::<(Entity, &Terrain, &TerrainAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, _, asset, ..)| asset.file.endless.is_some())
        .map(|(entity, _, asset, local, placed)| (entity, asset.clone(), voxear_terrain::corner(local, placed)))
        .collect();
    if endless.is_empty() {
        world.remove_resource::<Streams>();
        return;
    }
    let viewer = world.resource::<TerrainViewer>().map(|viewer| viewer.0).or_else(|| camera_position(world));
    let Some(viewer) = viewer else { return };
    let mut streams = world.remove_resource::<Streams>().unwrap_or_default();
    streams.0.retain(|entity, _| endless.iter().any(|(e, ..)| e == entity));
    for (entity, asset, corner) in endless {
        let making = Making {
            pictures: content.terrain_pictures(&asset.file),
            worlds: content.terrain_worlds(&asset.file),
            climate: content.terrain_climate(&asset.file),
            rasters: asset.rasters,
            file: asset.file,
        };
        let Some(terrain) = world.get_mut::<Terrain>(entity) else { continue };
        let local = [viewer.x - corner.x, viewer.z - corner.z];
        streams.0.entry(entity).or_default().update(terrain, making, local);
    }
    world.insert_resource(streams);
}

/// Whether every terrain without end has made all the ground round where
/// it is seen from, as its layers are now: for a test, or a game waiting
/// on its world before it starts.
pub fn streamed(world: &World) -> bool {
    world.resource::<Streams>().is_none_or(|streams| streams.0.values().all(Streamer::settled))
}
