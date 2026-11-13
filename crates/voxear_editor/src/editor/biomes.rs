//! Biomes in the editor.
//!
//! A biome in the Inspector (see `biome_inspector`) is saved as it is
//! changed. The engine reads it again at once, so every world of blocks
//! listing it has its ground made again with it in the next frame, in the
//! level being edited and in a game being played alike (see
//! `voxear_engine::blocks::refresh`). Its blocks are filled from the object
//! picker, those built with last first, or by a block dropped on them from
//! the Project panel or the Blocks panel.
//!
//! The command palette's **New Biome** makes one in the project's Biomes
//! folder and shows it.

use voxear_assets::AssetKind;
use voxear_render::Renderer;
use voxear_terrain::generate::BiomeDef;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    biome_inspector::BiomeRef,
    fields::Pick,
    object_picker::PickFor,
    project_files::{self, NewFile, biome_text},
    project_view::ProjectEvent,
};

/// Where the palette's New Biome puts a biome, from the project's folder.
const BIOMES_FOLDER: &str = "Biomes";

impl Editor {
    /// Writes a biome, for the worlds of blocks listing it to make their
    /// ground again with, and when `rebuild`, shows it again.
    pub(super) fn save_biome(&mut self, path: &str, biome: &BiomeDef, rebuild: bool) {
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(file, biome_text(biome)) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_biome(guid);
        }
        if let Some(item) = self.project.item(path).filter(|_| rebuild) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Opens the object picker under `anchor` for the block `field` of the
    /// biome at `biome`: the project's blocks, those built with last first.
    pub(super) fn browse_biome_ref(&mut self, biome: &str, field: BiomeRef, anchor: NodeId) {
        let choices = self.recent_blocks_first(self.asset_choices(AssetKind::Block));
        let target = PickFor::BiomeRef { biome: biome.to_string(), field };
        self.picker.open(&mut self.ui, (target, anchor), "Select Block", choices, "");
    }

    /// Puts the first block dragged onto a field in it.
    pub(super) fn drop_biome_ref(&mut self, biome: &str, field: BiomeRef, source: &DragSource) {
        let Some(first) = self.dragged_assets(source).into_iter().next() else { return };
        let db = self.game.content.db();
        let is_block = db.guid_of(&first).and_then(|guid| db.get(guid)).is_some_and(|e| e.kind == AssetKind::Block);
        if !is_block {
            return self.notify("Only a block goes there");
        }
        self.pick_biome_ref(biome, field, &Pick::Asset(Some(first)));
    }

    /// Puts the block picked, or none, in a field of the biome at `biome`,
    /// and saves it.
    pub(super) fn pick_biome_ref(&mut self, biome: &str, field: BiomeRef, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let reference = match choice {
            None => None,
            Some(path) => match self.game.content.parse(path) {
                Ok(reference) => Some(reference.to_string()),
                Err(_) => return,
            },
        };
        let mut changed = self.read_biome(biome);
        field.set(&mut changed, reference);
        self.save_biome(biome, &changed, true);
    }

    /// The biome at `biome` as its file has it.
    fn read_biome(&self, biome: &str) -> BiomeDef {
        let text = std::fs::read_to_string(self.game.content.db().file(biome)).unwrap_or_default();
        BiomeDef::parse(&text).unwrap_or_default()
    }

    /// Makes a new biome in the project's Biomes folder, and shows it in
    /// the Project panel, its name ready to type over.
    pub(super) fn new_biome(&mut self, renderer: &mut Renderer) {
        let root = self.game.project.root.clone();
        let made = std::fs::create_dir_all(root.join(BIOMES_FOLDER))
            .map_err(anyhow::Error::from)
            .and_then(|()| project_files::create(&root, BIOMES_FOLDER, NewFile::Biome, ""));
        match made {
            Ok(path) => {
                self.open_panel("project");
                self.project_event(ProjectEvent::Changed { select: Some(path), rename: true }, renderer);
            }
            Err(error) => self.notify(&format!("Could not make a biome: {error:#}")),
        }
    }
}
