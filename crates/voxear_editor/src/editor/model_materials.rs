//! A model's materials as `.material` assets, as Unity's material remapping:
//!
//! - The model's Inspector lists its materials by name, each with the
//!   `.material` it draws with in place of the file's, picked or dropped
//!   there and written to the model's `.meta` (see [`MaterialRemap`]).
//! - Extract Materials writes each of the file's materials out as a
//!   `.material` beside the model, in a folder named for it, with the
//!   pictures inside the file as `.png` files, and remaps the model to
//!   them, so editing one shows on every Obj drawing the model.
//! - A material's maps take a picture from the object picker, or dropped
//!   from the Project panel.

use anyhow::{Context, Result};
use voxear_assets::{AssetKind, MaterialRemap};
use voxear_engine::{MaterialData, MaterialMap};
use voxear_render::{FileMaterial, FilePicture, Renderer};
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    asset_inspector::AssetAction,
    fields::Pick,
    object_picker::{self, PickFor},
    project_files,
    project_view::ProjectEvent,
};

impl Editor {
    /// Lists the materials of the model the Inspector shows, loading the
    /// model first if it is not loaded.
    pub(super) fn fill_model_materials(&mut self, renderer: &mut Renderer) {
        let Some(view) = self.inspector.asset_view().filter(|view| view.wants_materials()) else { return };
        let path = view.item().path.clone();
        let Some(guid) = self.game.content.db().guid_of(&path) else { return };
        if let Err(error) = self.game.content.model(renderer, guid) {
            tracing::warn!("could not list the materials of {path}: {error:#}");
        }
        let materials = self.game.content.loaded_model_materials(guid).unwrap_or_default().to_vec();
        let remap = MaterialRemap::read_meta(&self.game.content.db().mounts().meta_file(&path));
        let Some(view) = self.inspector.asset_view() else { return };
        view.fill_materials(&mut self.ui, &materials, &remap, self.game.content.db());
    }

    /// Opens the object picker under `anchor` for a picture for a map of
    /// the material at `material`.
    pub(super) fn browse_material_map(&mut self, material: &str, map: MaterialMap, anchor: NodeId) {
        let choices = self.asset_choices(AssetKind::Texture);
        let target = PickFor::MaterialMap { material: material.to_string(), map };
        self.picker.open(&mut self.ui, (target, anchor), "Select Texture", choices, "");
    }

    /// Sets a map of the material the Inspector shows to a picture, or none.
    pub(super) fn pick_material_map(&mut self, material: &str, map: MaterialMap, pick: &Pick, renderer: &mut Renderer) {
        let picture = match pick {
            Pick::Asset(Some(path)) => match self.game.content.db().guid_of(path) {
                Some(guid) => Some((guid, path.clone())),
                None => return,
            },
            _ => None,
        };
        let shown = self.inspector.asset_view().filter(|view| view.item().path == material);
        if let Some(action) = shown.and_then(|view| view.set_map(&mut self.ui, map, picture.clone())) {
            return self.asset_action(action, renderer);
        }
        // Shown elsewhere, as on a VoxDocs page: changed in its file.
        let text = std::fs::read_to_string(self.game.project.root.join(material)).unwrap_or_default();
        let Ok(mut data) = toml::from_str::<MaterialData>(&text) else { return };
        data.set_map(map, picture.map(|(guid, _)| guid));
        self.asset_action(AssetAction::SaveMaterial(material.to_string(), data), renderer);
    }

    /// A picture dragged from the Project panel onto a material's map.
    pub(super) fn drop_material_map(
        &mut self,
        material: &str,
        map: MaterialMap,
        source: &DragSource,
        renderer: &mut Renderer,
    ) {
        let Some(path) = self.dragged_asset(source) else { return };
        if self.asset_kind(&path) != Some(AssetKind::Texture) {
            return self.notify("Only a picture goes in a material's map");
        }
        self.pick_material_map(material, map, &Pick::Asset(Some(path)), renderer);
    }

    /// Opens the object picker under `anchor` for the `.material` a
    /// model's material `part` draws with.
    pub(super) fn browse_remap(&mut self, model: &str, part: &str, anchor: NodeId) {
        let choices = self.asset_choices(AssetKind::Material);
        let target = PickFor::Remap { model: model.to_string(), part: part.to_string() };
        self.picker.open(&mut self.ui, (target, anchor), "Select Material", choices, "");
    }

    /// Draws a model's material `part` with the `.material` picked, or with
    /// the file's again for none.
    pub(super) fn pick_remap(&mut self, model: &str, part: &str, pick: &Pick, renderer: &mut Renderer) {
        let material = match pick {
            Pick::Asset(Some(path)) => match self.game.content.db().guid_of(path) {
                Some(guid) => Some(guid),
                None => return,
            },
            _ => None,
        };
        let mut remap = MaterialRemap::read_meta(&self.game.content.db().mounts().meta_file(model));
        match material {
            Some(material) => remap.0.insert(part.to_string(), material),
            None => remap.0.remove(part),
        };
        if let Err(error) = self.write_remap(model, &remap, renderer) {
            self.notify(&format!("Could not change the materials of {model}: {error:#}"));
        }
    }

    /// A `.material` dragged from the Project panel onto a model's material.
    pub(super) fn drop_remap(&mut self, model: &str, part: &str, source: &DragSource, renderer: &mut Renderer) {
        let Some(path) = self.dragged_asset(source) else { return };
        if self.asset_kind(&path) != Some(AssetKind::Material) {
            return self.notify("Only a material goes in a model's materials");
        }
        self.pick_remap(model, part, &Pick::Asset(Some(path)), renderer);
    }

    /// Writes each of a model's materials out as a `.material` asset in a
    /// folder beside it, with the pictures inside the file as `.png` files,
    /// and draws the model with them.
    pub(super) fn extract_materials(&mut self, model: &str, renderer: &mut Renderer) {
        match self.write_materials(model) {
            Ok((folder, remap)) => {
                let count = remap.0.len();
                if let Err(error) = self.write_remap(model, &remap, renderer) {
                    return self.notify(&format!("Could not draw {model} with its materials: {error:#}"));
                }
                self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
                let name = model.rsplit('/').next().unwrap_or(model);
                self.notify(&format!("Extracted {count} materials of {name} to {folder}"));
            }
            Err(error) => self.notify(&format!("Could not extract the materials of {model}: {error:#}")),
        }
    }

    /// Writes the model's materials and pictures, and returns the folder
    /// they went in and the remapping to them.
    fn write_materials(&mut self, model: &str) -> Result<(String, MaterialRemap)> {
        let mounts = self.game.content.db().mounts().clone();
        let prepared = voxear_render::prepare_model(model, &mut |file| Ok(std::fs::read(mounts.file(file))?))?;
        let materials = prepared.file_materials();
        anyhow::ensure!(!materials.is_empty(), "it has no materials of its own");
        let (parent, file) = model.rsplit_once('/').unwrap_or(("", model));
        // Beside the model: in the project, or in a folder package's own folder.
        let at = project_files::place(&mounts, parent)?;
        let root = at.root.clone();
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let folder = project_files::free_path(&root, &at.path, &format!("{stem} Materials"), "");
        std::fs::create_dir_all(root.join(&folder)).with_context(|| format!("making {folder}"))?;

        // Pictures first, so the materials can name them once they have GUIDs.
        let mut pictures = Vec::new();
        for (number, material) in materials.iter().enumerate() {
            let name = file_name(&material.name).unwrap_or_else(|| format!("Material {number}"));
            let maps = material.maps.clone().into_iter().zip(MAP_NAMES);
            let written = maps.map(|(map, kind)| write_picture(&at, &folder, map?, &format!("{name} {kind}")));
            pictures.push(<[Option<String>; 4]>::try_from(written.collect::<Vec<_>>()).expect("four maps"));
        }
        self.game.content.rescan()?;
        let mut written = Vec::new();
        for (number, (material, maps)) in materials.iter().zip(pictures).enumerate() {
            let data = material_data(material, maps.map(|path| path.and_then(|path| self.game.content.db().guid_of(&path))));
            let name = file_name(&material.name).unwrap_or_else(|| format!("Material {number}"));
            let path = project_files::free_path(&root, &folder, &name, ".material");
            std::fs::write(root.join(&path), toml::to_string(&data)?).with_context(|| format!("writing {path}"))?;
            written.push((material.part.clone(), at.back(&path)));
        }
        self.game.content.rescan()?;
        let db = self.game.content.db();
        let parts = written.into_iter().filter_map(|(part, path)| Some((part, db.guid_of(&path)?)));
        Ok((at.back(&folder), MaterialRemap(parts.collect())))
    }

    /// Writes a model's remapping to its `.meta`, imports the model again
    /// with it, and shows its Inspector again. A model in a package from the
    /// package cache has it written into the project's settings for the
    /// package.
    fn write_remap(&mut self, model: &str, remap: &MaterialRemap, renderer: &mut Renderer) -> Result<()> {
        remap.write_meta(&self.game.content.db().mounts().writable_meta(model)?)?;
        self.wrote_meta(model);
        self.reimport_model(model, renderer);
        let shown = self.inspector.shown_asset().filter(|item| item.path == model).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        Ok(())
    }

    /// The project's assets of `kind` for the object picker, after None.
    fn asset_choices(&self, kind: AssetKind) -> Vec<object_picker::Choice> {
        let db = self.game.content.db();
        let mut paths: Vec<String> = db.entries().filter(|e| e.kind == kind).map(|e| e.path.clone()).collect();
        paths.sort();
        let mut choices = vec![("None".to_string(), Pick::Asset(None))];
        choices.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
        object_picker::decorate(&self.game, choices)
    }

    fn asset_kind(&self, path: &str) -> Option<AssetKind> {
        let db = self.game.content.db();
        db.guid_of(path).and_then(|guid| db.get(guid)).map(|entry| entry.kind)
    }
}

/// A material of a model's file as a `.material` asset's values, its maps
/// reading the pictures with these GUIDs.
fn material_data(material: &FileMaterial, maps: [Option<voxear_assets::AssetGuid>; 4]) -> MaterialData {
    let [r, g, b, _] = material.desc.base_color;
    let [color_map, metallic_map, normal_map, emissive_map] = maps;
    MaterialData {
        color: voxear_engine::srgb([r, g, b]).map(|c| c.clamp(0.0, 1.0)),
        metallic: material.desc.metallic,
        roughness: material.desc.roughness,
        emissive: material.desc.emissive,
        color_map,
        metallic_map,
        normal_map,
        emissive_map,
        foliage: material.desc.foliage,
        glass: material.desc.glass,
        ior: material.desc.ior,
    }
}

/// What a picture inside a model's file is named for, after its material,
/// when it has no name of its own.
const MAP_NAMES: [&str; 4] = ["Colour", "Metallic", "Normal", "Emission"];

/// The project path of a picture a material reads: its own file's, or a
/// `.png` written into `folder`, within `at`, for one inside the model's
/// file, named `unnamed` when it has no name.
fn write_picture(at: &project_files::Place, folder: &str, picture: FilePicture, unnamed: &str) -> Option<String> {
    let (name, width, height, rgba) = match picture {
        FilePicture::File(path) => return Some(path),
        FilePicture::Inside { name, width, height, rgba } => (name, width, height, rgba),
    };
    let stem = file_name(name.rsplit(['/', '\\']).next().unwrap_or(&name)).unwrap_or_else(|| unnamed.to_string());
    let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(stem, _)| stem).to_string();
    let path = project_files::free_path(&at.root, folder, &stem, ".png");
    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    match image.save(at.root.join(&path)) {
        Ok(()) => Some(at.back(&path)),
        Err(error) => {
            tracing::warn!("could not write {path}: {error}");
            None
        }
    }
}

/// A name made safe for a file's, or none when nothing of it is left.
fn file_name(name: &str) -> Option<String> {
    let safe: String =
        name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
    let safe = safe.trim().trim_start_matches('.').to_string();
    (!safe.is_empty()).then_some(safe)
}
