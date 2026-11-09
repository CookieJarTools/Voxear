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

/// A Terrain's `.terrain` asset, and its file as the game has it now.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainAsset {
    pub guid: AssetGuid,
    pub file: TerrainFile,
}

impl Component for TerrainAsset {}

/// Keeps every terrain's heights and file in `content` as its asset's, as
/// the world has them now, marking those that changed to be written.
pub fn keep(world: &World, content: &mut Content) {
    let terrains = world.query_ref::<(&Terrain, &TerrainAsset)>().without::<Destroying>();
    for (terrain, asset) in terrains {
        content.keep_terrain(asset.guid, &asset.file, &terrain.heights);
    }
}

/// Writes each terrain asset changed since it was last written, its
/// `.terrain` and its `.heights`, into the project's folder; gives the
/// paths written.
pub fn save(content: &mut Content, project: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for (path, loaded) in content.take_unsaved_terrains() {
        let file = project.join(&path);
        std::fs::write(&file, loaded.file.to_text()).with_context(|| format!("saving {path}"))?;
        let heights = voxear_terrain::file::heights_file(&path);
        let bytes = voxear_terrain::file::write_heights(&loaded.heights);
        std::fs::write(project.join(&heights), bytes).with_context(|| format!("saving {heights}"))?;
        written.push(path);
    }
    Ok(written)
}
