//! Scenes opened alongside the level, additively, as Unity's Open Scene
//! Additive does: from a scene's menu in the Project, or by dragging a scene
//! onto the Hierarchy or the Scene view (see `placing`). The scene loads with
//! the others, its Objs are edited and saved with theirs, and it stays while
//! the level opens again after a change, until another level opens. A scene
//! header's menu in the Hierarchy adds such a scene to the level's file, or
//! closes any scene while another is open; closing one with edits not saved
//! asks first, as opening another level does.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_render::Renderer;

use super::Editor;
use crate::level_files::LevelEdit;

impl Editor {
    /// Opens the scene at `path` (from the project's folder) alongside those
    /// open.
    pub(super) fn open_scene_alongside(&mut self, path: &str, renderer: &mut Renderer) {
        if self.is_playing() {
            return self.notify("Stop playing to open a scene alongside the level");
        }
        if self.prefab.is_some() {
            return self.notify("Go back from the prefab to open scenes alongside the level");
        }
        let db = self.game.content.db();
        let Some(scene) = db.guid_of(path).filter(|&guid| db.get(guid).is_some_and(|e| e.kind == AssetKind::Scene))
        else {
            return self.notify(&format!("{path} is not a scene"));
        };
        let game = &mut self.game;
        match game.levels.open_scene_alongside(&mut game.engine, &mut game.content, Some(renderer), scene) {
            Ok(true) => self.notify(&format!("Opened {path} alongside the level")),
            Ok(false) => self.notify(&format!("{path} is open already")),
            Err(error) => self.notify(&format!("Could not open {path}: {error:#}")),
        }
    }

    /// Closes an open scene, while another is open.
    pub(super) fn close_scene(&mut self, scene: AssetGuid) {
        if self.is_playing() {
            return self.notify("Stop playing to close a scene");
        }
        let key = format!("close {scene}");
        if self.unsaved && self.discard_armed.as_deref() != Some(key.as_str()) {
            self.notify("Unsaved changes: Ctrl+S keeps them, or close the scene again without them");
            self.discard_armed = Some(key);
            return;
        }
        self.discard_armed = None;
        let game = &mut self.game;
        if let Err(error) = game.levels.close_scene(&mut game.engine, scene) {
            return self.notify(&format!("Could not close the scene: {error:#}"));
        }
        // Steps that name the scene's Objs cannot be undone any more.
        self.history.clear();
        let selected = self.selected().filter(|&entity| self.game.levels.scene_of(entity).is_some());
        self.select(selected);
    }

    /// Adds a scene opened alongside the open level to the level's file.
    pub(super) fn add_scene_to_level(&mut self, scene: AssetGuid, renderer: &mut Renderer) {
        let Some(level) = self.game.levels.current().map(str::to_string) else { return };
        let Some(path) = self.game.content.db().get(scene).map(|entry| entry.path.clone()) else { return };
        self.edit_level(&level, &LevelEdit::Add { guid: scene.to_string(), path }, renderer);
    }
}
