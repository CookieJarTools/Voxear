//! Water for a terrain's rivers and lakes: Add Water on a Rivers layer
//! makes an Obj at the terrain's corner holding a Stream along each river
//! and a Pool over each lake, in the terrain's scene, as one step to undo.

use std::collections::HashSet;

use voxear_ecs::Entity;
use voxear_engine::scene_file;
use voxear_render::Renderer;

use super::Editor;
use crate::history::Edit;

impl Editor {
    /// Adds water for the picked Rivers layer of `terrain`'s rivers and
    /// lakes, and selects it.
    pub(super) fn add_river_water(&mut self, terrain: Entity, renderer: &mut Renderer) {
        let Some(text) = crate::terrain_props::river_water(&mut self.game, terrain) else {
            return self.notify("No rivers or lakes to fill: lower the layer's Source");
        };
        let made = scene_file::parse(&text, "Rivers and Lakes").and_then(|file| {
            scene_file::spawn(&mut self.game.engine, &mut self.game.content, Some(renderer), &file, &HashSet::new())
        });
        let spawned = match made {
            Ok(spawned) => spawned,
            Err(error) => return self.notify(&format!("Could not add the water: {error:#}")),
        };
        let scene = self.game.levels.scene_of(terrain);
        self.game.levels.track_new(scene, &spawned);
        let Some(&(_, root)) = spawned.first() else { return };
        if !self.is_playing() {
            self.history.record_step(Edit::Exists { root, taken: None });
            self.unsaved = true;
        }
        self.select(Some(root));
    }
}
