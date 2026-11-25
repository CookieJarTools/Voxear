//! Voxel Prints (`.voxprint`) in the editor.
//!
//! A model's context menu in the Project panel makes a print of it beside
//! it (**Make Voxel Print**), at 16 voxels a metre. The print's Inspector
//! (see `voxel_print_inspector`) shows how it was made; its model and its
//! parts' pictures are picked from the object picker or dropped from the
//! Project panel, and **Make Again** makes it from the model as its fields
//! say, every Voxel Volume printing it showing it as it is now.

use voxear_assets::AssetKind;
use voxear_render::{Renderer, VoxelPrintSettings};
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{fields::Pick, object_picker::PickFor, project_view::ProjectEvent, voxel_print_inspector::PrintRef};

impl Editor {
    /// Makes a print of the model at `model` beside it, at 16 voxels a
    /// metre, and shows it.
    pub(super) fn make_print_of(&mut self, model: &str, renderer: &mut Renderer) {
        let Some(guid) = self.game.content.db().guid_of(model) else { return };
        let settings = VoxelPrintSettings { model: guid.to_string(), ..VoxelPrintSettings::default() };
        let path = voxear_engine::voxel_prints::path_for(model);
        self.make_voxel_print(&path, &settings, renderer);
    }

    /// Makes the print at `path` from its model as `settings` say, and
    /// shows it; every volume printing it prints it as it is now.
    pub(super) fn make_voxel_print(&mut self, path: &str, settings: &VoxelPrintSettings, renderer: &mut Renderer) {
        let started = std::time::Instant::now();
        match voxear_engine::voxel_prints::make_file(&mut self.game.content, path, settings) {
            Ok((print, guid)) => {
                let [x, y, z] = print.size;
                let took = started.elapsed().as_secs_f32();
                self.notify(&format!("Made {path}: {x} × {y} × {z} voxels in {took:.1} s"));
                if let Some(guid) = guid {
                    voxear_engine::voxel_volumes::reprint(&mut self.game.content, &mut self.game.engine.world, guid);
                }
            }
            Err(error) => return self.notify(&format!("Could not make {path}: {error:#}")),
        }
        self.files_seen();
        self.project_event(ProjectEvent::Changed { select: Some(path.to_string()), rename: false }, renderer);
        if let Some(item) = self.project.item(path) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Lists a picture field for each part of the model of the print the
    /// Inspector shows, loading the model first if it is not loaded.
    pub(super) fn fill_print_parts(&mut self, renderer: &mut Renderer) {
        let Some(fields) = self.inspector.asset_view().and_then(|view| view.voxel_print()) else { return };
        if !fields.wants_parts() {
            return;
        }
        let model = fields.settings.model.clone();
        let guid = self.game.content.parse(&model).ok().and_then(|reference| reference.guid());
        let parts = match guid {
            Some(guid) => {
                if let Err(error) = self.game.content.model(renderer, guid) {
                    tracing::warn!("could not list the parts of {model}: {error:#}");
                }
                self.game.content.loaded_model_materials(guid).unwrap_or_default().to_vec()
            }
            None => Vec::new(),
        };
        let content = &self.game.content;
        let path_of = |reference: &str| {
            let guid = content.parse(reference).ok()?.guid()?;
            Some(content.db().get(guid)?.path.clone())
        };
        let Some(fields) = self.inspector.asset_view().and_then(|view| view.voxel_print()) else { return };
        fields.fill_parts(&mut self.ui, &parts, &path_of);
    }

    /// Opens the object picker under `anchor` for a field of the print at
    /// `print`: the project's models, or its pictures.
    pub(super) fn browse_print_ref(&mut self, print: &str, field: PrintRef, anchor: NodeId) {
        let (kind, title) = match field {
            PrintRef::Model => (AssetKind::Model, "Select Model"),
            PrintRef::Picture(_) => (AssetKind::Texture, "Select Texture"),
        };
        let choices = self.asset_choices(kind);
        let target = PickFor::VoxelPrintRef { print: print.to_string(), field };
        self.picker.open(&mut self.ui, (target, anchor), title, choices, "");
    }

    /// Puts the first asset dragged onto a field in it, where it is of the
    /// kind the field takes.
    pub(super) fn drop_print_ref(&mut self, print: &str, field: PrintRef, source: &DragSource) {
        let Some(first) = self.dragged_assets(source).into_iter().next() else { return };
        let wanted = if field == PrintRef::Model { AssetKind::Model } else { AssetKind::Texture };
        let db = self.game.content.db();
        if db.guid_of(&first).and_then(|guid| db.get(guid)).is_none_or(|entry| entry.kind != wanted) {
            return self.notify(if wanted == AssetKind::Model {
                "Only a model goes there"
            } else {
                "Only a picture goes there"
            });
        }
        self.pick_print_ref(print, &field, &Pick::Asset(Some(first)));
    }

    /// Puts the asset picked, or none, in a field of the print the
    /// Inspector shows: kept until it is made again.
    pub(super) fn pick_print_ref(&mut self, print: &str, field: &PrintRef, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let reference = match choice {
            None => None,
            Some(path) => match self.game.content.db().guid_of(path) {
                Some(guid) => Some(guid.to_string()),
                None => return,
            },
        };
        let Some(view) = self.inspector.asset_view().filter(|view| view.item().path == print) else { return };
        let Some(fields) = view.voxel_print() else { return };
        fields.set(&mut self.ui, field, reference, choice.clone());
    }
}
