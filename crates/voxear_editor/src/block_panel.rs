//! The top of a Block World's Inspector section: its tools, **Build** and
//! **Settings**, and under Build the list of the blocks it is made of, each
//! by its name and colour. A click picks the block the brush builds with,
//! whose settings show below. Its cross takes it out, its blocks with it,
//! and leaves its place empty, so every other block keeps its number: the
//! list has no order to change. Add Block lists a `.block` of the project,
//! as one dragged onto the list from the Project panel is; New Block makes
//! one and lists it; Add Color adds a kind of the world's own, a colour
//! alone. While the world keeps kinds of its own that are more than a
//! model's colours, Make Block Assets makes blocks of them. Each change to
//! the list is a step to undo; picking a tool or a block is not.

use std::hash::{Hash, Hasher};

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_render::Renderer;
use voxear_ui::{DragSource, NodeId, Style, Ui};

use crate::{
    block_props::{self, BlockTool},
    collection::{self, Collection},
};

/// One row of the list: its block's place in the world's list, the part a
/// click picks it by, and its cross.
struct Row {
    place: usize,
    body: NodeId,
    remove: Option<NodeId>,
}

/// The tools and the list of blocks of the Block World the Inspector
/// shows.
pub struct BlockPanel {
    tools: NodeId,
    list: Option<Collection>,
    rows: Vec<Row>,
    /// New Block, Add Color, and Make Block Assets while there are kinds to
    /// make blocks of.
    new_block: Option<NodeId>,
    add_color: Option<NodeId>,
    make_assets: Option<NodeId>,
    signature: u64,
}

/// What the panel shows of an Obj's world: the tool, the picked block, and
/// each block's name and colour, as its file is drawn.
fn signature(world: &World, entity: Entity) -> u64 {
    let state = block_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (state.tool, state.block).hash(&mut hasher);
    if let Some(asset) = world.get::<BlockWorldAsset>(entity) {
        for kind in &asset.drawn.blocks {
            (&kind.name, &kind.block, kind.removed).hash(&mut hasher);
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
        let (mut new_block, mut add_color, mut make_assets) = (None, None, None);
        let file = world.get::<BlockWorldAsset>(entity).map(|asset| &**asset.drawn);
        let list = match (state.tool, file) {
            (BlockTool::Build, Some(file)) => {
                let listed = file.blocks.iter().filter(|kind| kind.is_live()).count();
                let list = collection::collection(ui, rows, listed, true, Some("Add Block"));
                // A block's place is its number, which nothing moves.
                ui.set_reorderable(list.items, false);
                ui.accept_drops(list.frame, true);
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No blocks");
                let picked = block_props::picked(world, file);
                for (place, kind) in file.blocks.iter().enumerate().filter(|(_, kind)| kind.is_live()) {
                    let item = collection::fixed_item(ui, list.items, shown.len());
                    ui.set_class(item.node, "selected", Some(place) == picked);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let swatch = ui.icon(line, "block");
                    ui.add_class(swatch, "terrain-layer-icon");
                    let [r, g, b] = kind.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                    ui.set_inline_style(swatch, &format!("color: #{r:02x}{g:02x}{b:02x}"));
                    let name = ui.add_label(line, &kind.name);
                    ui.add_class(name, "terrain-layer-name");
                    let number = ui.add_label(line, &(place + 1).to_string());
                    ui.add_class(number, "terrain-layer-kind");
                    shown.push(Row { place, body: line, remove: item.remove });
                }
                if let Some(footer) = list.footer {
                    new_block = Some(collection::footer_button(ui, footer, "New Block"));
                    add_color = Some(collection::footer_button(ui, footer, "Add Color"));
                    if block_props::kinds_to_make(file) > 0 {
                        make_assets = Some(collection::footer_button(ui, footer, "Make Block Assets"));
                    }
                }
                Some(list)
            }
            _ => None,
        };
        Self { tools, list, rows: shown, new_block, add_color, make_assets, signature: signature(world, entity) }
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, world: &World, entity: Entity) -> bool {
        signature(world, entity) != self.signature
    }

    /// Add Block, where it was clicked: the button the picker of blocks
    /// opens under.
    pub fn add_block_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.list.as_ref()?.add.filter(|&add| ui.clicked(add))
    }

    /// What was dropped onto the list, as a block dragged from the Project
    /// panel is.
    pub fn dropped(&self, ui: &Ui) -> Option<DragSource> {
        ui.dropped_onto(self.list.as_ref()?.frame)
    }

    /// Does what was clicked: a tool or a block picked, a block made, a
    /// colour added, blocks made of the world's own kinds, or a block taken
    /// out. Whether anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = BlockTool::ALL[ui.segment(self.tools).min(BlockTool::ALL.len() - 1)];
        if tool != block_props::ui(world).tool {
            block_props::show_tool(world, tool);
            return true;
        }
        if self.list.is_none() {
            return false;
        }
        let clicked = |button: Option<NodeId>| button.is_some_and(|button| ui.clicked(button));
        if clicked(self.new_block) {
            return block_props::new_block(game, renderer, entity);
        }
        if clicked(self.add_color) {
            return block_props::add_block(game, renderer, entity);
        }
        if clicked(self.make_assets) {
            return block_props::make_block_assets(game, renderer, entity);
        }
        for row in &self.rows {
            if clicked(row.remove) {
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
