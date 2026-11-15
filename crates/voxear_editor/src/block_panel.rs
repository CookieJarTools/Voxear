//! A Block World's own parts of its Inspector section. At its top, its
//! tools, **Build** and **Settings**; picking a tool is no step to undo.
//!
//! Under Settings, while the world's generator makes no overhangs and caves
//! of its own, **Move to Generator** moves the world's into it (see
//! `block_props::move_to_generator`). While the world lays land into its
//! generator as it makes ground (its levels, its biomes' land), **Write Land
//! into Generator** writes those nodes into the generator's graph, each part
//! in a comment, and the world lays none in from then on (see
//! `block_props::write_land`).
//!
//! Among the Settings of generated ground, under its generator's rows, the
//! **Biomes** the ground has: each by its colour and name, dragged into
//! another order by its grip and taken out by its cross. **Add Biome**
//! picks one of the project's, as a `.biome` dragged onto the list from the
//! Project panel does. **Add Starter Biomes** makes eight ready ones beside
//! the world and lists them, and **Add Showcase Biomes** three more with
//! their trees (see `starter_biomes`). Each change makes the ground again,
//! and is one step to undo.

use std::hash::{Hash, Hasher};

use voxear_ecs::Entity;
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_render::Renderer;
use voxear_ui::{DragSource, NodeId, Style, Ui};

use crate::{
    block_props::{self, BlockTool},
    collection::{self, Collection},
    inspector::prop,
};

/// The list of the world's biomes: its collection, each row's cross, Add
/// Starter Biomes and Add Showcase Biomes.
struct Biomes {
    list: Collection,
    removes: Vec<Option<NodeId>>,
    starters: Option<NodeId>,
    showcase: Option<NodeId>,
}

/// The tools of the Block World the Inspector shows, and its biomes.
pub struct BlockPanel {
    tools: NodeId,
    /// Move to Generator, where the world's overhangs and caves can move.
    move_out: Option<NodeId>,
    /// Write Land into Generator, where the world lays land into its generator.
    write_land: Option<NodeId>,
    /// Add Temple Dungeon, where the world's ground is generated.
    dungeon: Option<NodeId>,
    biomes: Option<Biomes>,
    signature: u64,
}

/// What the panel shows of an Obj's world: the tool, whether its overhangs
/// and caves can move into its generator, and its biomes as its list shows
/// them.
fn signature(game: &Game, entity: Entity) -> u64 {
    let world = &game.engine.world;
    let state = block_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.tool.hash(&mut hasher);
    block_props::generator_has_volume(world, entity).hash(&mut hasher);
    block_props::can_write_land(game, entity).hash(&mut hasher);
    if let Some(asset) = world.get::<BlockWorldAsset>(entity) {
        asset.file.generate.as_ref().and_then(|generate| generate.world.as_ref()).hash(&mut hasher);
    }
    if state.tool == BlockTool::Settings {
        for biome in block_props::biomes(game, entity) {
            (&biome.name, &biome.path).hash(&mut hasher);
            biome.color.map(|color| color.map(f32::to_bits)).hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl BlockPanel {
    pub fn new(ui: &mut Ui, rows: NodeId, game: &Game, entity: Entity) -> Self {
        let world = &game.engine.world;
        let state = block_props::ui(world);
        let tools = ui.segmented(rows);
        ui.add_class(tools, "block-tools");
        for tool in BlockTool::ALL {
            ui.add_segment(tools, tool.name(), None);
        }
        let at = BlockTool::ALL.iter().position(|&tool| tool == state.tool).unwrap_or_default();
        ui.set_segment(tools, at);
        let mut move_out = None;
        if state.tool == BlockTool::Settings && block_props::can_move_to_generator(game, entity) {
            let button = ui.button(rows, "Move to Generator");
            ui.add_class(button, "block-move-to-generator");
            ui.set_tooltip(button, "Moves Overhangs and Caves into the generator, as nodes of its graph", None);
            move_out = Some(button);
        }
        let mut write_land = None;
        if state.tool == BlockTool::Settings && block_props::can_write_land(game, entity) {
            let button = ui.button(rows, "Write Land into Generator");
            ui.add_class(button, "block-move-to-generator");
            ui.add_class(button, "block-write-land");
            let tip = "Writes the levels and biome land laid in at load into the generator, as nodes in comments";
            ui.set_tooltip(button, tip, None);
            write_land = Some(button);
        }
        let mut dungeon = None;
        let generated = world.get::<BlockWorldAsset>(entity).is_some_and(|asset| asset.file.generate.is_some());
        if state.tool == BlockTool::Settings && generated {
            let button = ui.button(rows, "Add Temple Dungeon");
            ui.add_class(button, "block-move-to-generator");
            let tip = "Makes a temple dungeon's blocks and structure, and has the generator build it under the land";
            ui.set_tooltip(button, tip, None);
            dungeon = Some(button);
        }
        let signature = signature(game, entity);
        Self { tools, move_out, write_land, dungeon, biomes: None, signature }
    }

    /// The list of the world's biomes, under the rows made so far.
    pub fn add_biomes(&mut self, ui: &mut Ui, rows: NodeId, game: &Game, entity: Entity) {
        let listed = block_props::biomes(game, entity);
        let heading = prop(ui, rows, "Biomes");
        let count = ui.add_label(heading, &listed.len().to_string());
        ui.add_class(count, "value");
        let list = collection::collection(ui, rows, listed.len(), true, Some("Add Biome"));
        ui.accept_drops(list.frame, true);
        ui.add_class(list.frame, "terrain-layers");
        ui.add_class(list.frame, "block-biomes");
        ui.set_text(list.empty, "No biomes: its own blocks everywhere");
        let mut removes = Vec::new();
        for (place, biome) in listed.iter().enumerate() {
            let item = collection::item(ui, list.items, place, true);
            let line = ui.panel(item.body, Style::row());
            ui.add_class(line, "terrain-layer");
            let swatch = ui.panel(line, Style::default());
            ui.add_class(swatch, "biome-swatch");
            if let Some([r, g, b]) = biome.color.map(|color| color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)) {
                ui.set_inline_style(swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            }
            let name = ui.add_label(line, &biome.name);
            ui.add_class(name, "terrain-layer-name");
            if let Some(path) = &biome.path {
                ui.set_tooltip(line, path, None);
            }
            removes.push(item.remove);
        }
        let starters = list.footer.map(|footer| {
            let button = collection::footer_button(ui, footer, "Add Starter Biomes");
            ui.set_tooltip(button, "Makes eight biomes and their blocks beside the world, and lists them", None);
            button
        });
        let showcase = list.footer.map(|footer| {
            let button = collection::footer_button(ui, footer, "Add Showcase Biomes");
            let tip = "Makes Autumn Woods, Blossom Grove and Volcanic Fields, their blocks and trees, and lists them";
            ui.set_tooltip(button, tip, None);
            button
        });
        self.biomes = Some(Biomes { list, removes, starters, showcase });
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, game: &Game, entity: Entity) -> bool {
        signature(game, entity) != self.signature
    }

    /// Add Biome, where it was clicked: the button the picker of biomes
    /// opens under.
    pub fn add_biome_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.biomes.as_ref()?.list.add.filter(|&add| ui.clicked(add))
    }

    /// What was dropped onto the list of biomes, as a biome dragged from
    /// the Project panel is.
    pub fn dropped(&self, ui: &Ui) -> Option<DragSource> {
        ui.dropped_onto(self.biomes.as_ref()?.list.frame)
    }

    /// Does what was clicked or dragged: a tool picked, the world's
    /// overhangs and caves moved into its generator, a biome moved or taken
    /// out, or the starter or showcase biomes made. Whether anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, mut renderer: Option<&mut Renderer>, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = BlockTool::ALL[ui.segment(self.tools).min(BlockTool::ALL.len() - 1)];
        if tool != block_props::ui(world).tool {
            block_props::show_tool(world, tool);
            return true;
        }
        if self.move_out.is_some_and(|button| ui.clicked(button)) {
            return block_props::move_to_generator(game, renderer, entity);
        }
        if self.write_land.is_some_and(|button| ui.clicked(button)) {
            return match block_props::write_land(game, renderer, entity) {
                Ok(_) => true,
                Err(error) => {
                    tracing::error!("Could not write the land into the generator: {error}");
                    false
                }
            };
        }
        if self.dungeon.is_some_and(|button| ui.clicked(button)) {
            return match crate::starter_dungeon::add(game, renderer, entity) {
                Ok((changed, _)) => changed,
                Err(error) => {
                    tracing::error!("Could not make the temple dungeon: {error:#}");
                    false
                }
            };
        }
        let Some(biomes) = &self.biomes else { return false };
        if biomes.starters.is_some_and(|button| ui.clicked(button)) {
            return match crate::starter_biomes::add(game, renderer.as_deref_mut(), entity) {
                Ok(listed) => listed,
                Err(error) => {
                    tracing::error!("Could not make the starter biomes: {error:#}");
                    false
                }
            };
        }
        if biomes.showcase.is_some_and(|button| ui.clicked(button)) {
            return match crate::starter_biomes::add_showcase(game, renderer.as_deref_mut(), entity) {
                Ok(listed) => listed,
                Err(error) => {
                    tracing::error!("Could not make the showcase biomes: {error:#}");
                    false
                }
            };
        }
        if let Some((from, to)) = collection::moved(ui, biomes.list.items) {
            return block_props::move_biome(game, renderer, entity, from, to);
        }
        let clicked = |remove: &Option<NodeId>| remove.is_some_and(|remove| ui.clicked(remove));
        if let Some(at) = biomes.removes.iter().position(clicked) {
            return block_props::remove_biome(game, renderer, entity, at);
        }
        false
    }
}
