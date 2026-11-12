//! The top of a Foliage's Inspector section: its tools, **Settings**,
//! **Paint** and **Grow** (see `plants_props`). Picking a tool is not a step to undo.

use voxear_ecs::World;
use voxear_engine::Game;
use voxear_ui::{NodeId, Ui};

use crate::plants_props::{self, FoliageTool};

/// The tools of the Foliage the Inspector shows.
pub struct PlantsPanel {
    tools: NodeId,
    tool: FoliageTool,
}

impl PlantsPanel {
    pub fn new(ui: &mut Ui, rows: NodeId, world: &World) -> Self {
        let tool = plants_props::ui(world).tool;
        let tools = ui.segmented(rows);
        ui.add_class(tools, "terrain-tools");
        ui.add_class(tools, "foliage-tools");
        for each in FoliageTool::ALL {
            ui.add_segment(tools, each.name(), None);
        }
        ui.set_segment(tools, FoliageTool::ALL.iter().position(|&each| each == tool).unwrap_or_default());
        Self { tools, tool }
    }

    /// Whether the tool shown changed since it was built.
    pub fn stale(&self, world: &World) -> bool {
        plants_props::ui(world).tool != self.tool
    }

    /// Shows the tool picked; whether it changed.
    pub fn update(&self, ui: &Ui, game: &mut Game) -> bool {
        let picked = FoliageTool::ALL[ui.segment(self.tools).min(FoliageTool::ALL.len() - 1)];
        if picked == plants_props::ui(&game.engine.world).tool {
            return false;
        }
        plants_props::show_tool(&mut game.engine.world, picked);
        true
    }
}
