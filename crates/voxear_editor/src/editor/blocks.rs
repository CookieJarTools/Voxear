//! Blocks in the Inspector (see `block_inspector`): saved as they are
//! changed, so every world of blocks listing one is drawn as it now says at
//! once; their fields naming assets filled from the object picker or by
//! what is dropped on them from the Project panel; and the item that places
//! a block, made for it.

use voxear_assets::AssetKind;
use voxear_terrain::blocks::block::{BlockDef, BlockDrop};
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    block_inspector::BlockRef,
    fields::{self, Pick},
    object_picker::{self, PickFor},
    project_files::{self, block_text},
};

/// Where the items made for blocks go, from the project's folder.
const ITEMS_FOLDER: &str = "Items";

/// The Definitions a block's fields hold data assets of.
const TOOL_KIND: &str = "Voxear.ToolKind";
const ITEM: &str = "Voxear.Item";

/// What a field of a block takes.
enum Takes {
    /// Assets of a kind.
    Kind(AssetKind),
    /// Data assets holding a Definition, or one deriving from it.
    Definition(&'static str),
}

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

    /// What a field of a block takes, and what the picker calls choosing
    /// one.
    fn block_ref_takes(field: BlockRef) -> (Takes, &'static str) {
        match field {
            BlockRef::Material | BlockRef::Top | BlockRef::Bottom => {
                (Takes::Kind(AssetKind::Material), "Select Material")
            }
            BlockRef::Tool => (Takes::Definition(TOOL_KIND), "Select Tool Kind"),
            BlockRef::Drop(_) => (Takes::Definition(ITEM), "Select Item"),
            BlockRef::BreakSound | BlockRef::PlaceSound | BlockRef::StepSound => {
                (Takes::Kind(AssetKind::Sound), "Select Sound")
            }
            BlockRef::Data => (Takes::Kind(AssetKind::Data), "Select Data Asset"),
        }
    }

    /// Opens the object picker under `anchor` for the field `field` of the
    /// block at `block`.
    pub(super) fn browse_block_ref(&mut self, block: &str, field: BlockRef, anchor: NodeId) {
        let (takes, title) = Self::block_ref_takes(field);
        let choices = match takes {
            Takes::Kind(kind) => self.asset_choices(kind),
            Takes::Definition(definition) => {
                let found = fields::data_assets(&self.game, self.inspector.types(), definition);
                object_picker::decorate(&self.game, found)
            }
        };
        let target = PickFor::BlockRef { block: block.to_string(), field };
        self.picker.open(&mut self.ui, (target, anchor), title, choices, "");
    }

    /// Puts the first asset dragged onto a field in it, where it is what
    /// the field takes.
    pub(super) fn drop_block_ref(&mut self, block: &str, field: BlockRef, source: &DragSource) {
        let Some(first) = self.dragged_assets(source).into_iter().next() else { return };
        let db = self.game.content.db();
        let dropped = db.guid_of(&first).and_then(|guid| db.get(guid));
        let fits = dropped.is_some_and(|entry| match Self::block_ref_takes(field).0 {
            Takes::Kind(kind) => entry.kind == kind,
            Takes::Definition(definition) => {
                entry.kind == AssetKind::Data
                    && fields::holds(&self.game, self.inspector.types(), entry.guid, definition)
            }
        });
        if !fits {
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
        let mut changed = self.read_block(block);
        field.set(&mut changed, reference);
        self.save_block(block, &changed, true);
    }

    /// The block at `block` as its file has it.
    fn read_block(&self, block: &str) -> BlockDef {
        let text = std::fs::read_to_string(self.game.content.db().file(block)).unwrap_or_default();
        BlockDef::parse(&text).unwrap_or_default()
    }

    /// Makes the item the block at `block` is carried as: a data asset in
    /// the project's Items folder, named as the block is, that places it.
    /// A block that drops nothing yet drops it.
    pub(super) fn make_block_item(&mut self, block: &str) {
        let Some(guid) = self.game.content.db().guid_of(block) else { return };
        let name = std::path::Path::new(block).file_stem().map_or("Item".into(), |stem| stem.to_string_lossy());
        let places = toml::Table::from_iter([("places".to_string(), toml::Value::String(guid.to_string()))]);
        let root = self.game.project.root.clone();
        let made = crate::data_assets::text(ITEM, &places).and_then(|text| {
            std::fs::create_dir_all(root.join(ITEMS_FOLDER))?;
            let file = project_files::unique(&root.join(ITEMS_FOLDER), &name, ".asset");
            std::fs::write(root.join(ITEMS_FOLDER).join(&file), text)?;
            Ok(format!("{ITEMS_FOLDER}/{file}"))
        });
        let path = match made.and_then(|path| self.game.content.rescan().map(|_| path)) {
            Ok(path) => path,
            Err(error) => return self.notify(&format!("Could not make an item: {error:#}")),
        };
        let mut changed = self.read_block(block);
        if changed.drops.is_empty() {
            let item = self.game.content.db().guid_of(&path).map(|item| item.to_string());
            changed.drops.push(BlockDrop { item: item.or(Some(path)), ..BlockDrop::default() });
        }
        self.save_block(block, &changed, true);
    }
}
