//! Voxel Prints made from the project's models (`.voxprint`, see
//! `voxear_render::VoxelPrint`): a model read as an import reads it, made
//! into voxels solid all through, coloured from its pictures or from the
//! pictures its settings give its parts, and kept in a file of its own
//! beside its settings, so it can be made again as the model changes.
//!
//! ```toml
//! # How a print is made, kept at the start of its file.
//! model = "Models/Sandbag Wall.fbx"
//! voxels_per_metre = 16
//! colours = 32
//! fill = "solid"            # or "sealed" for a model open underneath, "shell" for its skin alone
//! [[pictures]]
//! part = "material0"
//! picture = "Models/Sandbag Wall_BaseColor.png"
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use voxear_assets::{Access, AssetGuid, AssetKind};
use voxear_render::{VoxelPicture, VoxelPrint, VoxelPrintSettings};

use crate::content::Content;

/// Makes the print `settings` say from their model, read from `content`.
pub fn make(content: &mut Content, settings: &VoxelPrintSettings) -> Result<VoxelPrint> {
    let model = content
        .parse(&settings.model)
        .ok()
        .and_then(|reference| reference.guid())
        .and_then(|guid| content.db().get(guid).filter(|entry| entry.kind == AssetKind::Model))
        .map(|entry| entry.path.clone())
        .with_context(|| format!("{} is not a model in the project", settings.model))?;
    let mut pictures = HashMap::new();
    for given in &settings.pictures {
        let path = content
            .parse(&given.picture)
            .ok()
            .and_then(|reference| reference.guid())
            .and_then(|guid| content.db().get(guid).filter(|entry| entry.kind == AssetKind::Texture))
            .map(|entry| entry.path.clone())
            .with_context(|| format!("{} is not a picture in the project", given.picture))?;
        let bytes = content.read(&path)?;
        let picture = image::load_from_memory(&bytes).with_context(|| format!("reading {path}"))?.to_rgba8();
        pictures.insert(given.part.clone(), VoxelPicture::new(picture.width(), picture.height(), picture.as_raw()));
    }
    let mut read = |path: &str| content.read(path);
    voxear_render::voxel_print_model(&model, &mut read, settings, pictures)
        .with_context(|| format!("making a Voxel Print of {model}"))
}

/// Makes the print `settings` say and writes it to `path`, a `.voxprint`
/// in the project, which content reads again when next asked for. Gives
/// the print and its GUID, once the project knows the file.
pub fn make_file(
    content: &mut Content,
    path: &str,
    settings: &VoxelPrintSettings,
) -> Result<(Arc<VoxelPrint>, Option<AssetGuid>)> {
    let print = make(content, settings)?;
    write(content, path, &print)?;
    Ok((Arc::new(print), content.db().guid_of(path)))
}

/// Writes `print` to `path`, a `.voxprint` in the project.
pub fn write(content: &mut Content, path: &str, print: &VoxelPrint) -> Result<()> {
    if !path.to_ascii_lowercase().ends_with(".voxprint") {
        bail!("{path} is not a .voxprint");
    }
    let Some(mounts) = content.mounts() else { bail!("prints in built content cannot change") };
    let (mount, _) = mounts.locate(path);
    if mounts.access(&mount) == Access::ReadOnly {
        bail!("{path} is in a package from the package cache, which is not changed");
    }
    let file = mounts.file(path);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&file, print.to_bytes()?).with_context(|| format!("writing {}", file.display()))?;
    if content.db().guid_of(path).is_none() {
        content.rescan()?;
    }
    content.forget(path);
    Ok(())
}

/// The settings a `.voxprint` in the project was made with.
pub fn settings_of(content: &mut Content, reference: &str) -> Option<VoxelPrintSettings> {
    content.voxel_print(reference).map(|(_, print)| print.settings.clone())
}

/// A print's path for a model's: beside it, its name the model's.
pub fn path_for(model: &str) -> String {
    let stem = model.rsplit_once('.').map_or(model, |(stem, _)| stem);
    format!("{stem}.voxprint")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_print_is_kept_beside_its_model() {
        assert_eq!(path_for("Models/Sandbags.fbx"), "Models/Sandbags.voxprint");
        assert_eq!(path_for("Crate.glb"), "Crate.voxprint");
    }
}
