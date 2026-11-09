//! Terrains in a game (the Terrain module).
//!
//! Each Terrain loaded from a `.terrain` asset carries a [`TerrainAsset`]:
//! which asset it is, and its file as the game has it now. The editor
//! changes a terrain's heights and file in the world; [`keep`] keeps them in
//! the game's content as the asset's, as Unity keeps a terrain's data in
//! memory, so a level copied into play, or loaded again, has the ground as
//! it was left, and undoing an edit puts the asset back too. Saving the
//! level writes each changed asset's `.terrain` and `.heights` ([`save`]).

use std::path::Path;

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_ecs::{Component, World};
use voxear_scene::Destroying;
use voxear_terrain::{Terrain, file::TerrainFile};

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
        // The shares are kept too, so a game need not work them out as it loads.
        let surfaces = voxear_terrain::surfaces::hash_surfaces(&loaded.file.surfaces);
        let weights = loaded.weights.as_deref().map(|weights| (weights, surfaces));
        let grown = voxear_terrain::grass::hash_grass(&loaded.file.grass, surfaces);
        let grass = loaded.grass.as_deref().map(|grass| (grass, grown));
        let bytes = voxear_terrain::file::write_ground(&loaded.heights, loaded.file.shape_hash(), weights, grass);
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
