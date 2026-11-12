//! Plants a Block World grows by itself: a model picked on a Block World's
//! **Add plant** row, or dropped on it from the Project, becomes a Foliage
//! under the world that grows on its ground, as one step to undo. The row
//! offers the starter plants too (`voxear_render::StarterPlant`), each
//! written into the project's Plants folder the first time it is picked.

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_render::Renderer;

use super::Editor;
use crate::{
    history::Edit,
    objs,
    plants_props::{self, FoliageTool},
};

impl Editor {
    /// Whether the asset at `path` is a model.
    pub(super) fn is_model(&self, path: &str) -> bool {
        let db = self.game.content.db();
        db.guid_of(path).and_then(|guid| db.get(guid)).is_some_and(|entry| entry.kind == AssetKind::Model)
    }

    /// Makes the model at `path` a plant the Block World on `world` grows,
    /// and selects it with where it grows showing.
    pub(super) fn add_grown_plant(&mut self, world: Entity, path: &str, renderer: &mut Renderer) {
        // A starter plant the project has not yet is made first.
        if let Some(plant) = objs::starter_plant_at(path).filter(|_| !self.is_model(path)) {
            if let Err(error) = objs::make_starter_plant(&mut self.game, plant) {
                return self.notify(&format!("Could not make {}: {error:#}", plant.name()));
            }
        }
        if !self.is_model(path) {
            return self.notify("Only a model grows as a plant");
        }
        let made = match objs::create_grown_plant(&mut self.game, renderer, world, path) {
            Ok(made) => made,
            Err(error) => return self.notify(&format!("Could not add the plant: {error:#}")),
        };
        if !self.is_playing() {
            self.history.record_step(Edit::Exists { root: made, taken: None });
            self.unsaved = true;
        }
        plants_props::show_tool(&mut self.game.engine.world, FoliageTool::Grow);
        self.select(Some(made));
    }
}
