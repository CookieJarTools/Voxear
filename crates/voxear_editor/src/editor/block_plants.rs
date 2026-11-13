//! Plants of blocks (`.voxplant`) in the editor.
//!
//! A plant in the Inspector (see `plant_inspector`) is saved as it is
//! changed. The engine reads it again at once, so every world of blocks
//! growing it, by a structure that names it, grows it anew (see
//! `voxear_engine::Content::forget_structure`), and its saplings grow into
//! it as it is now. Its blocks are filled from the object picker, those
//! built with last first, or by a block dropped on them from the Project
//! panel or the Blocks panel.

use voxear_assets::AssetKind;
use voxear_terrain::blocks::plant::PlantDef;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{fields::Pick, object_picker::PickFor, plant_inspector::PlantRef};

impl Editor {
    /// Writes a plant, for the worlds growing it to grow it anew, and when
    /// `rebuild`, shows it again.
    pub(super) fn save_plant(&mut self, path: &str, plant: &PlantDef, rebuild: bool) {
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(file, plant.to_text()) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_structure(guid);
        }
        if let Some(item) = self.project.item(path).filter(|_| rebuild) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Opens the object picker under `anchor` for the block `field` of the
    /// plant at `plant`: the project's blocks, those built with last first.
    pub(super) fn browse_plant_ref(&mut self, plant: &str, field: PlantRef, anchor: NodeId) {
        let choices = self.recent_blocks_first(self.asset_choices(AssetKind::Block));
        let target = PickFor::PlantRef { plant: plant.to_string(), field };
        self.picker.open(&mut self.ui, (target, anchor), "Select Block", choices, "");
    }

    /// Puts the first block dragged onto a field in it.
    pub(super) fn drop_plant_ref(&mut self, plant: &str, field: PlantRef, source: &DragSource) {
        let Some(first) = self.dragged_assets(source).into_iter().next() else { return };
        let db = self.game.content.db();
        let is_block = db.guid_of(&first).and_then(|guid| db.get(guid)).is_some_and(|e| e.kind == AssetKind::Block);
        if !is_block {
            return self.notify("Only a block goes there");
        }
        self.pick_plant_ref(plant, field, &Pick::Asset(Some(first)));
    }

    /// Puts the block picked, or none, in a field of the plant at `plant`,
    /// and saves it.
    pub(super) fn pick_plant_ref(&mut self, plant: &str, field: PlantRef, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let reference = match choice {
            None => None,
            Some(path) => match self.game.content.parse(path) {
                Ok(reference) => Some(reference.to_string()),
                Err(_) => return,
            },
        };
        let text = std::fs::read_to_string(self.game.content.db().file(plant)).unwrap_or_default();
        let mut changed = PlantDef::parse(&text).unwrap_or_default();
        field.set(&mut changed, reference);
        self.save_plant(plant, &changed, true);
    }
}
