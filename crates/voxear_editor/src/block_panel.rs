//! The top of a Block World's Inspector section: its tools, **Build** and
//! **Settings**, and under Build the list of its kinds of block, each by
//! its name and colour. A click picks the kind the brush builds with, whose
//! name, colour and materials show below; a drag by its grip moves it,
//! every block of every kind renumbered with it; its cross takes it out,
//! its blocks with it; Add Block adds one of a colour of its own. Each
//! change to the list is a step to undo; picking a tool or a kind is not.

use std::hash::{Hash, Hasher};

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_render::Renderer;
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    block_props::{self, BlockTool},
    collection::{self, Collection},
};

/// One row of the list: its kind's place, the part a click picks it by,
/// and its cross.
struct Row {
    place: usize,
    body: NodeId,
    remove: Option<NodeId>,
}

/// The tools and the list of kinds of block of the Block World the
/// Inspector shows.
pub struct BlockPanel {
    tools: NodeId,
    list: Option<Collection>,
    rows: Vec<Row>,
    signature: u64,
}

/// What the panel shows of an Obj's world: the tool, the picked kind, and
/// each kind's name and colour.
fn signature(world: &World, entity: Entity) -> u64 {
    let state = block_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (state.tool, state.block).hash(&mut hasher);
    if let Some(asset) = world.get::<BlockWorldAsset>(entity) {
        for kind in &asset.file.blocks {
            kind.name.hash(&mut hasher);
            kind.color.map(f32::to_bits).hash(&mut hasher);
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
        let mut shown = Vec::new();
        let file = world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file);
        let list = match (state.tool, file) {
            (BlockTool::Build, Some(file)) => {
                let list = collection::collection(ui, rows, file.blocks.len(), true, Some("Add Block"));
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No kinds of block");
                for (place, kind) in file.blocks.iter().enumerate() {
                    let item = collection::item(ui, list.items, place, true);
                    ui.set_class(item.node, "selected", place == state.block);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let swatch = ui.icon(line, "blocks");
                    ui.add_class(swatch, "terrain-layer-icon");
                    let [r, g, b] = kind.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                    ui.set_inline_style(swatch, &format!("color: #{r:02x}{g:02x}{b:02x}"));
                    let name = ui.add_label(line, &kind.name);
                    ui.add_class(name, "terrain-layer-name");
                    let number = ui.add_label(line, &(place + 1).to_string());
                    ui.add_class(number, "terrain-layer-kind");
                    shown.push(Row { place, body: line, remove: item.remove });
                }
                Some(list)
            }
            _ => None,
        };
        Self { tools, list, rows: shown, signature: signature(world, entity) }
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, world: &World, entity: Entity) -> bool {
        signature(world, entity) != self.signature
    }

    /// Does what was clicked or dragged: a tool or a kind picked, a kind
    /// added, moved or taken out. Whether anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = BlockTool::ALL[ui.segment(self.tools).min(BlockTool::ALL.len() - 1)];
        if tool != block_props::ui(world).tool {
            block_props::show_tool(world, tool);
            return true;
        }
        let Some(list) = &self.list else { return false };
        if list.add.is_some_and(|add| ui.clicked(add)) {
            return block_props::add_block(game, renderer, entity);
        }
        if let Some((from, to)) = collection::moved(ui, list.items) {
            return block_props::move_block(game, renderer, entity, from, to);
        }
        for row in &self.rows {
            if row.remove.is_some_and(|remove| ui.clicked(remove)) {
                return block_props::remove_block(game, renderer, entity, row.place);
            }
            if ui.clicked(row.body) && block_props::ui(&game.engine.world).block != row.place {
                block_props::pick_block(&mut game.engine.world, row.place);
                return true;
            }
        }
        false
    }
}
