//! Worlds of blocks in a game (the Terrain module).
//!
//! Each Block World loaded from a `.blockworld` asset carries a
//! [`BlockWorldAsset`]: which asset it is, and its file as the game has it
//! now. Its blocks are made from its file as it loads, the same on every
//! machine, so nothing but the file is kept. The editor changes a world's
//! file in the world; [`keep`] keeps it in the game's content as the
//! asset's, and saving the level writes each changed asset's file
//! ([`save`]).

use std::path::Path;

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_ecs::{Component, World};
use voxear_render::{BlockLook, Renderer};
use voxear_scene::Destroying;
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

use crate::{Content, MaterialData, MaterialSource, scene_file::default_material};

/// A Block World's `.blockworld` asset: its file as the game has it now.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockWorldAsset {
    pub guid: AssetGuid,
    pub file: BlockFile,
}

impl Component for BlockWorldAsset {}

/// The world of blocks `file` makes: its ground's blocks, each chunk's
/// faces worked out on every core.
pub fn make(file: &BlockFile) -> BlockWorld {
    let (_, faces) = file.faces();
    BlockWorld::new(file.make_ground(), &faces, file.block_size)
}

/// How a world of blocks looks: each way its faces are drawn, with its
/// material where it names one and else its colour, a picture a block.
pub fn look(content: &mut Content, renderer: &mut Renderer, file: &BlockFile) -> Result<BlockLook> {
    let fallback = content.material(renderer, &MaterialSource::Inline(default_material()))?;
    let (looks, _) = file.faces();
    let mut surfaces = Vec::with_capacity(looks.len());
    for face in looks {
        let named = face.material.as_ref().and_then(|reference| {
            let source = MaterialSource::Asset(content.parse(reference).ok()?);
            content.material(renderer, &source).map_err(|error| tracing::warn!("a block's material: {error:#}")).ok()
        });
        let material = match named {
            Some(material) => material,
            None => {
                let colored = MaterialData { color: face.color, ..default_material() };
                content.material(renderer, &MaterialSource::Inline(colored))?
            }
        };
        surfaces.push(voxear_render::SurfaceLook {
            material,
            size: file.block_size,
            sharpness: 0.0,
            triplanar: true,
            tracks: 0.0,
            fills_in: 0.0,
            liquid: Default::default(),
            glitter: false,
        });
    }
    Ok(BlockLook { material: fallback, surfaces: surfaces.into() })
}

/// Keeps every world of blocks' file in `content` as its asset's, as the
/// world has it now, marking those that changed to be written.
pub fn keep(world: &World, content: &mut Content) {
    for asset in world.query_ref::<&BlockWorldAsset>().without::<Destroying>() {
        content.keep_block_world(asset.guid, &asset.file);
    }
}

/// Writes each world of blocks' file changed since it was last written into
/// the project's folder; gives the paths written.
pub fn save(content: &mut Content, project: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for (path, file) in content.take_unsaved_block_worlds() {
        std::fs::write(project.join(&path), file.to_text()).with_context(|| format!("saving {path}"))?;
        written.push(path);
    }
    Ok(written)
}
