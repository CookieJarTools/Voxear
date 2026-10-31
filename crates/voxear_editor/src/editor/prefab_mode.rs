//! A prefab opened on its own to edit, as Unity's prefab mode does: its Objs
//! load as a scene's would, in place of the level, a bar over the Scene view
//! names it, and Back opens again what was open before. Save writes the
//! prefab, and what was read of it is then forgotten, so the level's
//! instances show the change when it opens again. Only here can a component
//! be locked for designers, and the values they still see chosen.

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::prefab::{FromPrefab, PrefabInstance};
use voxear_render::Renderer;

use super::Editor;
use crate::levels_panel::LevelAction;

/// The prefab open on its own, and what Back opens again: nothing, when
/// nothing was open before it.
pub(super) struct OpenPrefab {
    path: String,
    back: Option<LevelAction>,
}

impl Editor {
    pub(super) fn update_prefab_mode(&mut self, renderer: &mut Renderer) {
        if self.ui.clicked(self.nodes.prefab_back) {
            self.close_prefab(renderer);
        }
    }

    /// The prefab open on its own, if one is.
    pub fn open_prefab_path(&self) -> Option<&str> {
        self.prefab.as_ref().map(|open| open.path.as_str())
    }

    /// Opens a prefab on its own to edit, in place of the level. Opened from
    /// another prefab, Back still goes to the level both were opened from.
    pub fn open_prefab(&mut self, path: &str, renderer: &mut Renderer) {
        if self.is_playing() {
            return self.notify("Stop playing to edit a prefab");
        }
        if !self.may_discard(path) {
            return;
        }
        let back = match &self.prefab {
            Some(open) => open.back.clone(),
            None => self.reopening(),
        };
        let game = &mut self.game;
        if let Err(error) = game.levels.open_scene(&mut game.engine, &mut game.content, Some(&mut *renderer), path) {
            return self.notify(&format!("Could not open {path}: {error:#}"));
        }
        self.opened(renderer);
        self.prefab = Some(OpenPrefab { path: path.to_string(), back });
        self.inspector.set_prefab_mode(true);
        let name = path.rsplit('/').next().unwrap_or(path);
        self.ui.set_text(self.nodes.prefab_name, name);
        self.ui.set_visible(self.nodes.prefab_bar, true);
        let root = self.game.levels.scene_order().first().and_then(|(_, _, objs)| objs.first().copied());
        self.select(root);
    }

    /// Opens the prefab an Obj in the Hierarchy is an instance of, or part
    /// of one.
    pub(super) fn open_prefab_of(&mut self, entity: Entity, renderer: &mut Renderer) {
        let world = &self.game.engine.world;
        let root = world.get::<FromPrefab>(entity).map_or(entity, |from| from.instance);
        let Some(&PrefabInstance(guid)) = world.get::<PrefabInstance>(root) else { return };
        let Some(path) = self.game.content.db().get(guid).map(|entry| entry.path.clone()) else { return };
        self.open_prefab(&path, renderer);
    }

    /// Leaves the prefab for what was open before it, as Back does. With
    /// nothing to go back to, the prefab stays open as a scene.
    fn close_prefab(&mut self, renderer: &mut Renderer) {
        let Some(open) = &self.prefab else { return };
        let back = open.back.clone();
        if !self.may_discard("back from the prefab") {
            return;
        }
        match back {
            Some(LevelAction::Open(level)) => self.open_level(&level, renderer),
            Some(LevelAction::OpenScene(scene)) => self.open_scene(&scene, renderer),
            _ => {
                self.leave_prefab_mode();
                let selected = self.selected();
                self.inspector.show(&mut self.ui, &self.game, selected, false);
            }
        }
    }

    /// Out of prefab mode, as after something else opened.
    pub(super) fn leave_prefab_mode(&mut self) {
        if self.prefab.take().is_none() {
            return;
        }
        self.inspector.set_prefab_mode(false);
        self.ui.set_visible(self.nodes.prefab_bar, false);
    }

    /// After a save: the prefab open is read from its file again when next
    /// used, so instances made or loaded from now on show what was saved.
    pub(super) fn prefab_saved(&mut self) {
        if let Some(open) = &self.prefab {
            self.game.content.forget(&open.path);
        }
    }

    /// What is open now, as what would open it again.
    fn reopening(&self) -> Option<LevelAction> {
        let current = self.game.levels.current()?.to_string();
        let db = self.game.content.db();
        let level = db.guid_of(&current).and_then(|guid| db.get(guid)).is_some_and(|e| e.kind == AssetKind::Level);
        Some(if level { LevelAction::Open(current) } else { LevelAction::OpenScene(current) })
    }
}
