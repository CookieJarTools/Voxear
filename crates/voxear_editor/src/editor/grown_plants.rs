//! Plants a Block World grows by itself: a model picked on a Block World's
//! **Add plant** row, or dropped on it from the Project, becomes a Foliage
//! under the world that grows on its ground, as one step to undo. The row
//! offers the starter plants too (`voxear_render::StarterPlant`), each
//! written into the project's Plants folder the first time it is picked;
//! every one of them at once, each growing where it suits it; and the
//! starter trees of blocks (`starter_trees`), which the world's generator
//! builds.

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
        match path {
            objs::ALL_STARTER_PLANTS => return self.add_starter_plants(world, renderer),
            objs::STARTER_TREES => return self.add_starter_trees(world, renderer),
            _ => {}
        }
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

    /// Has the Block World on `world` grow every starter plant it does not
    /// grow yet, each where it suits it, and the pebbles along its shores
    /// besides: one step to undo.
    fn add_starter_plants(&mut self, world: Entity, renderer: &mut Renderer) {
        use voxear_engine::grown_plants::{SHORE_PEBBLES, starter_growth};
        use voxear_render::StarterPlant;
        let growing = objs::grown_names(&self.game, world);
        let mut wanted: Vec<(StarterPlant, String, voxear_engine::grown_plants::StarterGrowth)> =
            StarterPlant::ALL.into_iter().map(|plant| (plant, plant.name().to_string(), starter_growth(plant))).collect();
        wanted.push((StarterPlant::Pebbles, "Shore Pebbles".to_string(), SHORE_PEBBLES));
        let mut made = Vec::new();
        for (plant, name, growth) in wanted {
            if growing.contains(&name) {
                continue;
            }
            let path = match objs::make_starter_plant(&mut self.game, plant) {
                Ok(path) => path,
                Err(error) => return self.notify(&format!("Could not make {}: {error:#}", plant.name())),
            };
            match objs::create_grown(&mut self.game, renderer, world, &path, (&name, Some(growth))) {
                Ok(foliage) => made.push(foliage),
                Err(error) => return self.notify(&format!("Could not add {name}: {error:#}")),
            }
        }
        if made.is_empty() {
            return self.notify("The world grows every starter plant already");
        }
        if !self.is_playing() {
            let steps = made.iter().map(|&root| Edit::Exists { root, taken: None }).collect();
            self.history.record_step(Edit::Group(steps));
            self.unsaved = true;
        }
        self.notify(&format!("Added {} starter plants", made.len()));
    }

    /// Makes the starter trees of blocks and has the generator of the
    /// Block World on `world` build them.
    fn add_starter_trees(&mut self, world: Entity, renderer: &mut Renderer) {
        use voxear_engine::blocks::BlockWorldAsset;
        let generated = |editor: &Self| {
            let asset = editor.game.engine.world.get::<BlockWorldAsset>(world);
            asset.is_some_and(|asset| asset.file.generate.is_some())
        };
        if !generated(self) {
            return self.notify("Trees of blocks are built by a generator: set the world's Ground to Generated");
        }
        let before = crate::history::ObjState::of(&self.game.engine.world, world);
        match crate::starter_trees::add(&mut self.game, Some(renderer), world) {
            Ok(true) => {
                let after = crate::history::ObjState::of(&self.game.engine.world, world);
                if let (false, Some(before), Some(after)) = (self.is_playing(), before, after) {
                    self.history.record_step(Edit::Changed { entity: world, before, after });
                    self.unsaved = true;
                }
                self.notify("Added the starter trees: an oak, a birch and a pine");
            }
            Ok(false) => self.notify("The world builds the starter trees already"),
            Err(error) => self.notify(&format!("Could not make the starter trees: {error:#}")),
        }
    }
}
