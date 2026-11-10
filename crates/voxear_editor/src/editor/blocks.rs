//! Blocks in the Inspector (see `block_inspector`): saved as they are
//! changed, so every world of blocks listing one is drawn as it now says at
//! once; their fields naming assets filled from the object picker or by
//! what is dropped on them from the Project panel.

use voxear_assets::AssetKind;
use voxear_terrain::blocks::block::BlockDef;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{block_inspector::BlockRef, fields::Pick, object_picker::PickFor, project_files::block_text};

impl Editor {
    /// Writes a block, for the worlds of blocks listing it to take up (see
    /// `voxear_engine::blocks::refresh`), and when `rebuild`, shows it again.
    pub(super) fn save_block(&mut self, path: &str, block: &BlockDef, rebuild: bool) {
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(file, block_text(block)) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_block(guid);
        }
        if let Some(item) = self.project.item(path).filter(|_| rebuild) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// What a field of a block takes: the kind of asset, and what the
    /// picker calls choosing one.
    fn block_ref_kind(field: BlockRef) -> (AssetKind, &'static str) {
        match field {
            BlockRef::Material | BlockRef::Top | BlockRef::Bottom => (AssetKind::Material, "Select Material"),
        }
    }

    /// Opens the object picker under `anchor` for the field `field` of the
    /// block at `block`.
    pub(super) fn browse_block_ref(&mut self, block: &str, field: BlockRef, anchor: NodeId) {
        let (kind, title) = Self::block_ref_kind(field);
        let choices = self.asset_choices(kind);
        let target = PickFor::BlockRef { block: block.to_string(), field };
        self.picker.open(&mut self.ui, (target, anchor), title, choices, "");
    }

    /// Puts the first asset dragged onto a field in it, where it is of the
    /// field's kind.
    pub(super) fn drop_block_ref(&mut self, block: &str, field: BlockRef, source: &DragSource) {
        let Some(first) = self.dragged_assets(source).into_iter().next() else { return };
        let db = self.game.content.db();
        let (kind, _) = Self::block_ref_kind(field);
        if db.guid_of(&first).and_then(|guid| db.get(guid)).map(|entry| entry.kind) != Some(kind) {
            return self.notify("That is not what goes there");
        }
        self.pick_block_ref(block, field, &Pick::Asset(Some(first)));
    }

    /// Puts the asset picked, or none, in a field of the block at `block`,
    /// and saves it.
    pub(super) fn pick_block_ref(&mut self, block: &str, field: BlockRef, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let reference = match choice {
            None => None,
            Some(path) => match self.game.content.parse(path) {
                Ok(reference) => Some(reference.to_string()),
                Err(_) => return,
            },
        };
        let text = std::fs::read_to_string(self.game.content.db().file(block)).unwrap_or_default();
        let mut changed = BlockDef::parse(&text).unwrap_or_default();
        field.set(&mut changed, reference);
        self.save_block(block, &changed, true);
    }
}
