//! The project's blocks in the editor.
//!
//! The Blocks panel (see `block_database`): what it asks is done here. A
//! block picked shows in the Inspector, one double clicked is what the
//! brush builds with, and blocks are made, copied and deleted.
//!
//! Blocks in the Inspector (see `block_inspector`): saved as they are
//! changed, so every world of blocks holding one is drawn as it now says at
//! once; their fields naming assets filled from the object picker or by
//! what is dropped on them from the Project panel; and the item that places
//! a block, made for it.
//!
//! The brush's block is remembered between sessions, with those it built
//! with before it: a picker of blocks offers those first.
//!
//! A world written before blocks were assets keeps kinds of its own in its
//! file. Once such a world is in the level open, a block is made of each
//! (see `block_props::make_kinds_blocks`) and its file written, so it holds
//! the project's blocks as every other world does.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::Entity;
use voxear_engine::blocks::BlockWorldAsset;
use voxear_render::Renderer;
use voxear_terrain::blocks::block::{BlockDef, BlockDrop};
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    block_database::BlocksEvent,
    block_inspector::BlockRef,
    block_props,
    fields::{self, Pick},
    object_picker::{self, Choice, Chosen, PickFor},
    project_files::{self, block_text},
    project_view::ProjectEvent,
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
    /// The Blocks panel's frame: what it asked for is done, its pick
    /// follows what the Inspector shows, the brush's block is remembered,
    /// and older worlds' kinds are made blocks.
    pub(super) fn update_block_database(&mut self, renderer: &mut Renderer) {
        let showing = self.shown(self.nodes.blocks_panel);
        for event in self.block_database.update(&mut self.ui, self.game.content.db(), showing) {
            match event {
                BlocksEvent::Selected(path) => {
                    // The world being built in is selected again once the brush is given a block.
                    let world = &self.game.engine.world;
                    let building = self.selection.filter(|&entity| world.has::<BlockWorldAsset>(entity));
                    self.built_in = building.or(self.built_in);
                    self.inspect_path(&path);
                }
                BlocksEvent::BuildWith(path) => self.build_with_block(&path),
                BlocksEvent::New => self.new_block(renderer),
                BlocksEvent::Duplicate(path) => self.duplicate_block(&path, renderer),
                BlocksEvent::Delete(path) => self.delete_block(&path, renderer, false),
                BlocksEvent::DeleteHeld(path) => self.delete_block(&path, renderer, true),
            }
        }
        let db = self.game.content.db();
        let kind = |path: &str| db.guid_of(path).and_then(|guid| db.get(guid)).map(|entry| entry.kind);
        let is_block = |path: &str| kind(path) == Some(AssetKind::Block);
        let inspected = self.inspector.shown_asset().map(|item| item.path.clone()).filter(|path| is_block(path));
        if showing && inspected != self.block_database.selected() {
            self.block_database.select(&mut self.ui, inspected.as_deref());
        }
        self.keep_brush_block();
        if !self.is_playing() {
            self.make_older_kinds_blocks(renderer);
        }
    }

    /// Has the brush build with the block at `path`, and selects the Block
    /// World that was being built in before the Blocks panel's block took
    /// the Inspector.
    fn build_with_block(&mut self, path: &str) {
        let Some(guid) = self.game.content.db().guid_of(path) else { return };
        block_props::build_with(&mut self.game, guid);
        let world = &self.game.engine.world;
        let built_in = self.built_in.take();
        let back = built_in.filter(|&entity| world.is_alive(entity) && world.has::<BlockWorldAsset>(entity));
        if let Some(entity) = back.filter(|_| self.selection.is_none()) {
            self.select(Some(entity));
        }
        self.notify(&format!("Building with {}", block_name(path)));
    }

    /// Makes a new block of a colour of its own in the project's Blocks
    /// folder, and picks it.
    fn new_block(&mut self, renderer: &mut Renderer) {
        let blocks = self.game.content.db().entries().filter(|entry| entry.kind == AssetKind::Block).count();
        let block = BlockDef::colored(block_props::fresh_color(blocks + 1));
        let root = self.game.project.root.clone();
        match project_files::create_block(&root, block_props::BLOCKS_FOLDER, "Block", &block) {
            Ok(path) => self.block_made(path, renderer),
            Err(error) => self.notify(&format!("Could not make a block: {error:#}")),
        }
    }

    /// Copies the block at `path` beside it, and picks the copy.
    fn duplicate_block(&mut self, path: &str, renderer: &mut Renderer) {
        let copied = project_files::item_place(self.game.content.db().mounts(), path)
            .and_then(|at| project_files::duplicate(&at.root, &at.path).map(|copy| at.back(&copy)));
        match copied {
            Ok(copy) => self.block_made(copy, renderer),
            Err(error) => self.notify(&format!("Could not copy {}: {error:#}", block_name(path))),
        }
    }

    /// After a block was made at `path`: the project's files are looked at
    /// again, and it is picked and shown.
    fn block_made(&mut self, path: String, renderer: &mut Renderer) {
        self.project_event(ProjectEvent::Changed { select: Some(path.clone()), rename: false }, renderer);
        self.block_database.select(&mut self.ui, Some(&path));
    }

    /// Moves the block at `path` to the project's trash. While worlds hold
    /// it, the Blocks panel says which and asks again, and only `anyway`
    /// does it go: its blocks in them are then missing.
    fn delete_block(&mut self, path: &str, renderer: &mut Renderer, anyway: bool) {
        let Some(guid) = self.game.content.db().guid_of(path) else { return };
        let holding = block_props::worlds_holding(&self.game, guid);
        if !holding.is_empty() && !anyway {
            return self.block_database.ask_delete(&mut self.ui, path, &holding);
        }
        let root = self.game.project.root.clone();
        let trashed = project_files::item_place(self.game.content.db().mounts(), path)
            .and_then(|at| project_files::trash(&at.root, &at.path, &root));
        if let Err(error) = trashed {
            return self.notify(&format!("Could not delete {}: {error:#}", block_name(path)));
        }
        let name = block_name(path);
        if !holding.is_empty() {
            tracing::warn!("{name} was deleted while worlds hold it: its blocks are missing in {}", holding.join(", "));
        }
        self.game.content.forget_block(guid);
        self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        // The Inspector showed the block: it shows nothing now.
        if self.inspector.shown_asset().is_some_and(|item| item.path == path) {
            self.show_selection(None);
        }
        self.block_database.select(&mut self.ui, None);
        self.notify(&format!("Moved {name} to .voxear/Trash"));
    }

    /// Remembers the block the brush builds with, most recent first; a
    /// brush with none yet builds with the one it last did.
    fn keep_brush_block(&mut self) {
        match block_props::ui(&self.game.engine.world).block {
            Some(guid) => {
                let key = guid.to_string();
                if self.memory.recent_blocks.first() != Some(&key) {
                    self.memory.built_with(&key);
                }
            }
            None => {
                let db = self.game.content.db();
                let known = |key: &String| {
                    let guid = self.game.content.parse(key).ok()?.guid()?;
                    db.get(guid).filter(|entry| entry.kind == AssetKind::Block).map(|_| guid)
                };
                if let Some(guid) = self.memory.recent_blocks.iter().find_map(known) {
                    block_props::build_with(&mut self.game, guid);
                }
            }
        }
    }

    /// A picker's blocks with those the brush last built with first, most
    /// recent first, after none where none is offered.
    pub(super) fn recent_blocks_first(&self, choices: Vec<Choice>) -> Vec<Choice> {
        let db = self.game.content.db();
        let recent = &self.memory.recent_blocks;
        let rank = |choice: &Choice| match &choice.pick {
            Chosen::Value(Pick::Asset(None)) => Some(0),
            Chosen::Value(Pick::Asset(Some(path))) => {
                let key = db.guid_of(path)?.to_string();
                recent.iter().position(|known| *known == key).map(|at| at + 1)
            }
            _ => None,
        };
        let (mut first, rest): (Vec<Choice>, Vec<Choice>) = choices.into_iter().partition(|c| rank(c).is_some());
        first.sort_by_key(|choice| rank(choice));
        first.extend(rest);
        first
    }

    /// Makes blocks of the kinds the level's older worlds keep themselves,
    /// each world once, and writes each such world's file at once: what
    /// it is does not change, only where its blocks are kept.
    fn make_older_kinds_blocks(&mut self, renderer: &mut Renderer) {
        let world = &self.game.engine.world;
        let seen = &self.older_worlds_seen;
        let older: Vec<(Entity, AssetGuid)> = world
            .query_ref::<(Entity, &BlockWorldAsset)>()
            .into_iter()
            .filter(|(_, asset)| !seen.contains(&asset.guid) && block_props::kinds_to_make(&asset.file) > 0)
            .map(|(entity, asset)| (entity, asset.guid))
            .collect();
        for (entity, guid) in older {
            if !self.older_worlds_seen.insert(guid) {
                continue;
            }
            let unsaved = self.game.content.block_world_unsaved(guid);
            let Some((folder, made)) = block_props::make_kinds_blocks(&mut self.game, Some(renderer), entity) else {
                continue;
            };
            let Some(path) = self.game.content.db().get(guid).map(|entry| entry.path.clone()) else { continue };
            let Some(file) = self.game.engine.world.get::<BlockWorldAsset>(entity).map(|asset| asset.file.clone())
            else {
                continue;
            };
            let kinds = if made == 1 { "1 kind is a block".to_string() } else { format!("{made} kinds are blocks") };
            match std::fs::write(self.game.content.db().file(&path), file.to_text()) {
                Ok(()) => {
                    // Kept as its asset's, and written already unless it had other changes waiting to be saved.
                    voxear_engine::blocks::keep(&self.game.engine.world, &mut self.game.content);
                    if !unsaved {
                        self.game.content.block_world_written(guid);
                    }
                    tracing::info!("{path}: {kinds} now, in {folder}");
                }
                Err(error) => tracing::warn!("{path}: {kinds} now, in {folder}, but it could not be saved: {error}"),
            }
            self.files_seen();
        }
    }

    /// Writes a block, for the worlds of blocks listing it to take up (see
    /// `voxear_engine::blocks::refresh`), and when `rebuild`, shows it again.
    pub(super) fn save_block(&mut self, path: &str, block: &BlockDef, rebuild: bool) {
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(file, block_text(block)) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        self.blocks_saved.push(path.to_string());
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

/// What the block at `path` is called: its file's name.
fn block_name(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}
