//! Changing a level's scenes from the Inspector (see `level_files`). The
//! file is written at once. When a level is open, it is opened again, so a
//! change to it, or to a level it stands on, shows in the Hierarchy straight
//! away: scenes it keeps stay as they are, edits not saved and all, and
//! only scenes added load and scenes taken out unload. Taking out a scene
//! that is open while edits are not saved asks first, as opening another
//! level does, as they would go with it.

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_render::Renderer;

use super::Editor;
use crate::level_files::{self, LevelEdit};

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
        if let LevelEdit::Remove(at) = edit
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
        if let Some(item) = self.project.item(path) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        if let Err(error) = self.open_level_again(renderer) {
            self.notify(&format!("Could not open the level again: {error:#}"));
        }
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
