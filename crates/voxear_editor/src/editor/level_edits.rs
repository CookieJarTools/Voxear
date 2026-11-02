//! Changing a level's scenes from the Inspector (see `level_files`). The
//! file is written at once. When a level is open, it is opened again, so a
//! change to it, or to a level it stands on, shows in the Hierarchy straight
//! away: scenes it keeps stay as they are, edits not saved and all, and
//! only scenes added load and scenes taken out unload. Taking out a scene
//! that is open while edits are not saved asks first, as opening another
//! level does, as they would go with it.
//!
//! Each scene is a reference, as an Obj's are: the object picker offers the
//! project's other scenes for it, or for Add Scene, and a scene dragged from
//! the Project panel onto either goes in. The level a level stands on is
//! one too, offering the project's other levels, but none that stands on
//! this one, which would go round in a circle.

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_render::Renderer;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    fields::Pick,
    level_files::{self, LevelEdit},
    object_picker::{self, PickFor},
};

impl Editor {
    pub(super) fn edit_level(&mut self, path: &str, edit: &LevelEdit, renderer: &mut Renderer) {
        if self.is_playing() {
            return self.notify("Stop playing to change a level's scenes");
        }
        let file = self.game.project.root.join(path);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => return self.notify(&format!("Could not read {path}: {error}")),
        };
        if let LevelEdit::Remove(at) | LevelEdit::Replace { at, .. } = edit
            && self.unsaved
            && self.takes_out_open_scene(&text, *at)
        {
            let key = format!("{path}: take out scene {at}");
            if self.discard_armed.as_deref() != Some(key.as_str()) {
                self.notify("Unsaved changes: Ctrl+S keeps them, or click again to take the scene out without them");
                self.discard_armed = Some(key);
                return;
            }
        }
        let written = level_files::edit(&text, edit)
            .and_then(|text| std::fs::write(&file, text).with_context(|| format!("writing {path}")));
        if let Err(error) = written {
            return self.notify(&format!("Could not change {path}: {error:#}"));
        }
        self.discard_armed = None;
        self.game.content.forget(path);
        self.levels.refresh(&mut self.ui, &mut self.game.content);
        // The Inspector shows the change, when it shows the level, and a scene moved by its grip
        // eases into place in the list built again, as it would have in the old one.
        if self.inspected_asset().is_some_and(|shown| shown.path == path)
            && let Some(item) = self.project.item(path)
        {
            let items = |editor: &mut Self| editor.inspector.asset_view().and_then(|view| view.level_items());
            let slides = items(self).map(|old| self.ui.slides_of(old)).unwrap_or_default();
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
            if let Some(new) = items(self) {
                self.ui.start_slides(new, &slides);
            }
        }
        if let Err(error) = self.open_level_again(renderer) {
            self.notify(&format!("Could not open the level again: {error:#}"));
        }
    }

    /// Opens the object picker under `anchor` for a level's scene: the one
    /// at `at`, to put another in its place, or a new one. It offers the
    /// project's scenes the level does not have yet.
    pub(super) fn browse_level_scene(&mut self, level: &str, at: Option<usize>, anchor: NodeId) {
        let listed = self.level_scene_paths(level);
        let now = at.and_then(|at| listed.get(at)).cloned().unwrap_or_default();
        let db = self.game.content.db();
        let mut paths: Vec<String> = (db.entries())
            .filter(|entry| entry.kind == AssetKind::Scene && (entry.path == now || !listed.contains(&entry.path)))
            .map(|entry| entry.path.clone())
            .collect();
        paths.sort();
        let choices = paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))).collect();
        let choices = object_picker::decorate(&self.game, choices);
        let target = PickFor::LevelScene { level: level.to_string(), at };
        let now = now.rsplit('/').next().unwrap_or_default().to_string();
        self.picker.open(&mut self.ui, (target, anchor), "Select Scene", choices, &now);
    }

    /// Puts a scene picked for a level in: in place of the one at `at`, or
    /// at the end.
    pub(super) fn pick_level_scene(&mut self, level: &str, at: Option<usize>, pick: &Pick, renderer: &mut Renderer) {
        let Pick::Asset(Some(path)) = pick else { return };
        if at.is_some_and(|at| self.level_scene_paths(level).get(at) == Some(path)) {
            return;
        }
        let Some(guid) = self.game.content.db().guid_of(path) else { return };
        let (guid, path) = (guid.to_string(), path.clone());
        let edit = match at {
            Some(at) => LevelEdit::Replace { at, guid, path },
            None => LevelEdit::Add { guid, path },
        };
        self.edit_level(level, &edit, renderer);
    }

    /// Something dragged from the Project panel onto a level's scene, or
    /// onto Add Scene: a scene the level does not have yet goes in, and
    /// anything else is turned away, saying why.
    pub(super) fn drop_level_scene(
        &mut self,
        level: &str,
        at: Option<usize>,
        source: &DragSource,
        renderer: &mut Renderer,
    ) {
        let Some(path) = self.dragged_asset(source) else { return };
        let db = self.game.content.db();
        if db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind) != Some(AssetKind::Scene) {
            return self.notify("Only a scene goes in a level's scenes");
        }
        let listed = self.level_scene_paths(level);
        if listed.iter().position(|listed| *listed == path).is_some_and(|already| Some(already) != at) {
            let name = path.rsplit('/').next().unwrap_or(&path);
            return self.notify(&format!("{name} is in this level already"));
        }
        self.pick_level_scene(level, at, &Pick::Asset(Some(path)), renderer);
    }

    /// Opens the object picker under `anchor` for the level that `level`
    /// stands on: the project's other levels, or none.
    pub(super) fn browse_stands_on(&mut self, level: &str, anchor: NodeId) {
        let db = self.game.content.db();
        let mut paths: Vec<String> = (db.entries())
            .filter(|entry| entry.kind == AssetKind::Level && entry.path != level)
            .map(|entry| entry.path.clone())
            .collect();
        paths.sort();
        let mut choices = vec![("None".to_string(), Pick::Asset(None))];
        choices.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
        let choices = object_picker::decorate(&self.game, choices);
        let target = PickFor::StandsOn { level: level.to_string() };
        self.picker.open(&mut self.ui, (target, anchor), "Select Level", choices, "");
    }

    /// Makes `level` stand on the level picked, or on none, unless the one
    /// picked stands on `level`, through the levels it stands on.
    pub(super) fn pick_stands_on(&mut self, level: &str, pick: &Pick, renderer: &mut Renderer) {
        let base = match pick {
            Pick::Asset(Some(path)) => Some(path.clone()),
            _ => None,
        };
        let Some(base) = base else { return self.edit_level(level, &LevelEdit::StandsOn(None), renderer) };
        if base == level || self.stands_on_chain(&base).iter().any(|below| below == level) {
            let name = base.rsplit('/').next().unwrap_or(&base);
            return self.notify(&format!("{name} stands on this level already"));
        }
        let Some(guid) = self.game.content.db().guid_of(&base) else { return };
        self.edit_level(level, &LevelEdit::StandsOn(Some((guid.to_string(), base))), renderer);
    }

    /// A level dragged from the Project panel onto what a level stands on.
    pub(super) fn drop_stands_on(&mut self, level: &str, source: &DragSource, renderer: &mut Renderer) {
        let Some(path) = self.dragged_asset(source) else { return };
        let db = self.game.content.db();
        if db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind) != Some(AssetKind::Level) {
            return self.notify("A level stands only on another level");
        }
        self.pick_stands_on(level, &Pick::Asset(Some(path)), renderer);
    }

    /// The levels `level` stands on, through each, by their paths.
    fn stands_on_chain(&self, level: &str) -> Vec<String> {
        let db = self.game.content.db();
        let mut chain = Vec::new();
        let mut at = level.to_string();
        while chain.len() < 64 {
            let text = std::fs::read_to_string(self.game.project.root.join(&at)).unwrap_or_default();
            let Some(base) = level_files::stands_on(&text).ok().flatten() else { break };
            let path = base.parse().ok().and_then(|guid| db.get(guid)).map_or(base, |entry| entry.path.clone());
            if chain.contains(&path) {
                break;
            }
            chain.push(path.clone());
            at = path;
        }
        chain
    }

    /// A level's own scenes, by their paths, in its file's order. A scene
    /// the project does not have keeps the reference written.
    fn level_scene_paths(&self, level: &str) -> Vec<String> {
        let text = std::fs::read_to_string(self.game.project.root.join(level)).unwrap_or_default();
        let db = self.game.content.db();
        let path = |scene: &str| scene.parse().ok().and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
        let entries = level_files::entries(&text).unwrap_or_default();
        entries.iter().map(|entry| path(&entry.scene).unwrap_or_else(|| entry.scene.clone())).collect()
    }

    /// Whether the scene at `at` in a level's text is open in the editor, so
    /// taking it out unloads it.
    fn takes_out_open_scene(&self, text: &str, at: usize) -> bool {
        let entries = level_files::entries(text).unwrap_or_default();
        let Some(entry) = entries.get(at) else { return false };
        let guid = self.game.content.parse(&entry.scene).ok().and_then(|reference| reference.guid());
        self.prefab.is_none() && self.game.levels.scene_order().iter().any(|(open, ..)| Some(*open) == guid)
    }

    /// Opens the level open now again, as its file or one it stands on has
    /// changed. Nothing happens for a scene or prefab open on its own.
    fn open_level_again(&mut self, renderer: &mut Renderer) -> Result<()> {
        let Some(level) = self.game.levels.current().map(str::to_string) else { return Ok(()) };
        let db = self.game.content.db();
        let kind = db.guid_of(&level).and_then(|guid| db.get(guid)).map(|entry| entry.kind);
        if kind != Some(AssetKind::Level) || self.prefab.is_some() {
            return Ok(());
        }
        let change = self.game.load_level_with(&level, &Default::default(), Some(renderer))?;
        // Steps that name Objs of a scene taken out cannot be undone any more.
        if !change.unloaded.is_empty() {
            self.history.clear();
        }
        Ok(())
    }
}
