//! The top of a Block World's Inspector section: its tools, **Build** and
//! **Settings**. Picking a tool is no step to undo.
//!
//! Under Settings, while the world's generator makes no overhangs and caves
//! of its own, **Move to Generator** moves the world's into it (see
//! `block_props::move_to_generator`).

use std::hash::{Hash, Hasher};

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_render::Renderer;
use voxear_ui::{NodeId, Ui};

use crate::block_props::{self, BlockTool};

/// The tools of the Block World the Inspector shows.
pub struct BlockPanel {
    tools: NodeId,
    /// Move to Generator, where the world's overhangs and caves can move.
    move_out: Option<NodeId>,
    signature: u64,
}

/// What the panel shows of an Obj's world: the tool, and whether its
/// overhangs and caves can move into its generator.
fn signature(world: &World, entity: Entity) -> u64 {
    let state = block_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.tool.hash(&mut hasher);
    block_props::generator_has_volume(world, entity).hash(&mut hasher);
    if let Some(asset) = world.get::<BlockWorldAsset>(entity) {
        asset.file.generate.as_ref().and_then(|generate| generate.world.as_ref()).hash(&mut hasher);
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
        let signature = signature(world, entity);
        Self { tools, move_out, signature }
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, world: &World, entity: Entity) -> bool {
        signature(world, entity) != self.signature
    }

    /// Does what was clicked: a tool picked, or the world's overhangs and
    /// caves moved into its generator. Whether anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = BlockTool::ALL[ui.segment(self.tools).min(BlockTool::ALL.len() - 1)];
        if tool != block_props::ui(world).tool {
            block_props::show_tool(world, tool);
            return true;
        }
        if self.move_out.is_some_and(|button| ui.clicked(button)) {
            return block_props::move_to_generator(game, renderer, entity);
        }
        false
    }
}
