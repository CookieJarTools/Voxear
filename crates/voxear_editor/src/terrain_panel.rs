//! The top of a Terrain's Inspector section: its tools, **Layers**,
//! **Sculpt** and **Settings**, as Unity's Terrain has them, and under
//! Layers the list of its layers, the top one first, as image editors list
//! theirs. A click picks a layer, whose settings show below; its box
//! switches it on and off; a drag by its grip moves it; its cross takes it
//! out; Add Layer offers a new one of each kind. Each change to the layers
//! is a step to undo; picking a tool or a layer is not.

use std::hash::{Hash, Hasher};

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, terrain::TerrainAsset};
use voxear_terrain::layers::LayerKind;
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    collection::{self, Collection},
    terrain_props::{self, TerrainTool},
};

/// One layer's row: the layer's place in the stack, the row, the part a
/// click picks it by, its box, and its cross.
struct Row {
    layer: usize,
    body: NodeId,
    toggle: NodeId,
    remove: Option<NodeId>,
}

/// The tools and the layer list of the Terrain the Inspector shows.
pub struct TerrainPanel {
    tools: NodeId,
    list: Option<Collection>,
    rows: Vec<Row>,
    /// What the panel was built from, to build it again when that changes.
    signature: u64,
}

/// The icon a layer of its kind shows.
fn icon(kind: &LayerKind) -> &'static str {
    match kind {
        LayerKind::Import { .. } => "texture",
        LayerKind::Stamp { .. } => "mountain",
        LayerKind::Noise { .. } => "grid",
        LayerKind::Sculpt { .. } => "edit",
        LayerKind::Spline { .. } => "branch",
        LayerKind::Terrace { .. } => "layers",
        LayerKind::Smooth { .. } => "globe",
    }
}

/// What the panel shows of an Obj's terrain: the tool, the picked layer,
/// and each layer's name, kind and switch.
fn signature(world: &World, entity: Entity) -> u64 {
    let ui = terrain_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (ui.tool, ui.layer).hash(&mut hasher);
    if let Some(asset) = world.get::<TerrainAsset>(entity) {
        for layer in &asset.file.layers {
            (&layer.name, layer.kind.title(), layer.enabled).hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl TerrainPanel {
    pub fn new(ui: &mut Ui, rows: NodeId, world: &World, entity: Entity) -> Self {
        let state = terrain_props::ui(world);
        let tools = ui.segmented(rows);
        ui.add_class(tools, "terrain-tools");
        for tool in TerrainTool::ALL {
            ui.add_segment(tools, tool.name(), None);
        }
        let at = TerrainTool::ALL.iter().position(|&tool| tool == state.tool).unwrap_or_default();
        ui.set_segment(tools, at);
        let layers = world.get::<TerrainAsset>(entity).map(|asset| asset.file.layers.clone());
        let (list, rows_shown) = match (state.tool, layers) {
            (TerrainTool::Layers, Some(layers)) => {
                let list = collection::collection(ui, rows, layers.len(), true, Some("Add Layer"));
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No layers: flat ground at its base height");
                let mut shown = Vec::with_capacity(layers.len());
                // The top layer first.
                for (place, layer_at) in (0..layers.len()).rev().enumerate() {
                    let layer = &layers[layer_at];
                    let item = collection::item(ui, list.items, place, true);
                    ui.set_class(item.node, "selected", layer_at == state.layer);
                    ui.set_class(item.node, "off", !layer.enabled);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let mark = ui.icon(line, icon(&layer.kind));
                    ui.add_class(mark, "terrain-layer-icon");
                    let name = ui.add_label(line, &layer.name);
                    ui.add_class(name, "terrain-layer-name");
                    let kind = ui.add_label(line, layer.kind.title());
                    ui.add_class(kind, "terrain-layer-kind");
                    let toggle = ui.toggle(line, "", layer.enabled);
                    ui.add_class(toggle, "terrain-layer-on");
                    ui.set_tooltip(toggle, "On", None);
                    shown.push(Row { layer: layer_at, body: line, toggle, remove: item.remove });
                }
                (Some(list), shown)
            }
            _ => (None, Vec::new()),
        };
        Self { tools, list, rows: rows_shown, signature: signature(world, entity) }
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, world: &World, entity: Entity) -> bool {
        signature(world, entity) != self.signature
    }

    /// Does what was clicked or dragged: a tool or a layer picked, a layer
    /// switched, moved or taken out. Whether anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = TerrainTool::ALL[ui.segment(self.tools).min(TerrainTool::ALL.len() - 1)];
        if tool != terrain_props::ui(world).tool {
            terrain_props::show_tool(world, tool);
            return true;
        }
        if let Some((from, to)) = self.list.as_ref().and_then(|list| collection::moved(ui, list.items)) {
            // Places count from the top; layers from the bottom.
            let last = self.rows.len().saturating_sub(1);
            return terrain_props::move_layer(game, entity, last - from.min(last), last - to.min(last));
        }
        for row in &self.rows {
            if ui.clicked(row.toggle) {
                return terrain_props::toggle_layer(game, entity, row.layer);
            }
            if row.remove.is_some_and(|remove| ui.clicked(remove)) {
                return terrain_props::remove_layer(game, entity, row.layer);
            }
            if ui.clicked(row.body) && terrain_props::ui(&game.engine.world).layer != row.layer {
                terrain_props::pick_layer(&mut game.engine.world, row.layer);
                return true;
            }
        }
        false
    }

    /// The Add Layer button, when it was clicked, for its menu to open under.
    pub fn add_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.list.as_ref().and_then(|list| list.add).filter(|&add| ui.clicked(add))
    }
}
