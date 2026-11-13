//! A Plant (`.voxplant`) in the Inspector, laid out as a biome's is: a tree
//! or a bush of blocks a world grows from a few numbers and a seed. Its
//! **Trunk**: what kind of plant it is, how tall, how thick at its foot,
//! how much it thins to its top, how thick its branches are, and how far it
//! leans. Its **Crown**: how wide and how tall, how thick with branches,
//! how large and how full its clumps of leaves are, its roots, and how many
//! of its trees have vines. Its **Blocks**: its wood, its leaves, its vines
//! and its sapling, each one of the project's blocks, picked from them all,
//! and a cross takes one out again. Its **Saplings**: how many it scatters
//! as it is felled, how long one takes to grow into it and through how many
//! stages, and the least light one needs.
//!
//! Each change is saved as it is made, and every world growing the plant
//! grows it anew (see `editor::block_plants`).

use voxear_terrain::blocks::plant::{PlantDef, PlantKind};
use voxear_ui::{DragSource, NodeId, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    biome_inspector::{changed_number, changed_range, number_row, range_row},
    inspector::prop,
};

/// What was done to a plant's fields this frame.
pub enum PlantChange {
    /// A value typed: saved as it is shown.
    Typed,
    /// What it shows changed: saved, and shown again.
    Shape,
    /// The arrow of a block's field, to open the picker under.
    Browse(PlantRef, NodeId),
    /// Something dragged onto a block's field.
    Drop(PlantRef, DragSource),
    /// A click on a block's field: show the block in the Project panel.
    Ping(String),
}

/// One of the blocks a plant is grown of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlantRef {
    Wood,
    Leaves,
    Vine,
    Sapling,
}

impl PlantRef {
    const ALL: [(PlantRef, &'static str); 4] = [
        (PlantRef::Wood, "Wood"),
        (PlantRef::Leaves, "Leaves"),
        (PlantRef::Vine, "Vine"),
        (PlantRef::Sapling, "Sapling"),
    ];

    /// The reference the field holds in `plant`.
    pub fn of(self, plant: &PlantDef) -> Option<&str> {
        match self {
            PlantRef::Wood => plant.wood.as_deref(),
            PlantRef::Leaves => plant.leaves.as_deref(),
            PlantRef::Vine => plant.vine.as_deref(),
            PlantRef::Sapling => plant.sapling.as_deref(),
        }
    }

    /// Puts `reference` in the field of `plant`.
    pub fn set(self, plant: &mut PlantDef, reference: Option<String>) {
        match self {
            PlantRef::Wood => plant.wood = reference,
            PlantRef::Leaves => plant.leaves = reference,
            PlantRef::Vine => plant.vine = reference,
            PlantRef::Sapling => plant.sapling = reference,
        }
    }
}

/// A block's field of a plant: which, its slot, the path of the block it
/// names, and its cross.
struct BlockSlot {
    which: PlantRef,
    slot: Slot,
    path: Option<String>,
    clear: NodeId,
}

/// A plant's number fields, each with what it sets.
#[derive(Clone, Copy)]
enum Number {
    Trunk,
    Taper,
    Branch,
    Bend,
    Branches,
    LeafSize,
    LeafDensity,
    Roots,
    Vines,
    GrowTime,
    Stages,
    Light,
}

impl Number {
    fn get(self, plant: &PlantDef) -> f32 {
        match self {
            Number::Trunk => plant.trunk,
            Number::Taper => plant.taper,
            Number::Branch => plant.branch,
            Number::Bend => plant.bend,
            Number::Branches => plant.branches,
            Number::LeafSize => plant.leaf_size,
            Number::LeafDensity => plant.leaf_density,
            Number::Roots => plant.roots as f32,
            Number::Vines => plant.vines,
            Number::GrowTime => plant.grow_time / 60.0,
            Number::Stages => plant.stages as f32,
            Number::Light => plant.light,
        }
    }

    /// Sets it in `plant` as typed, kept within what it can be.
    fn set(self, plant: &mut PlantDef, value: f32) {
        match self {
            Number::Trunk => plant.trunk = value.clamp(0.25, 8.0),
            Number::Taper => plant.taper = value.clamp(0.0, 1.0),
            Number::Branch => plant.branch = value.clamp(0.25, 1.0),
            Number::Bend => plant.bend = value.clamp(0.0, 1.0),
            Number::Branches => plant.branches = value.clamp(0.0, 1.0),
            Number::LeafSize => plant.leaf_size = value.clamp(0.5, 12.0),
            Number::LeafDensity => plant.leaf_density = value.clamp(0.0, 1.0),
            Number::Roots => plant.roots = value.round().clamp(0.0, 8.0) as u32,
            Number::Vines => plant.vines = value.clamp(0.0, 1.0),
            Number::GrowTime => plant.grow_time = (value * 60.0).max(1.0),
            Number::Stages => plant.stages = value.round().clamp(1.0, 8.0) as u32,
            Number::Light => plant.light = value.clamp(0.0, 1.0),
        }
    }
}

/// A plant's fields, and the plant as they show it.
pub struct PlantFields {
    pub plant: PlantDef,
    kind: NodeId,
    height: [NodeId; 2],
    crown_width: [NodeId; 2],
    crown_height: [NodeId; 2],
    saplings: [NodeId; 2],
    numbers: Vec<(Number, NodeId)>,
    slots: Vec<BlockSlot>,
}

impl PlantFields {
    /// The fields of `plant` under `root`; `path_of` gives the project path
    /// of the block a reference names.
    pub fn new(ui: &mut Ui, root: NodeId, plant: PlantDef, path_of: &dyn Fn(&str) -> Option<String>) -> Self {
        let mut numbers = Vec::new();
        let mut number = |ui: &mut Ui, rows: NodeId, which: Number, label: &str, unit: &str, tip: &str| {
            let field = number_row(ui, rows, label, unit, which.get(&plant));
            if let Some(row) = ui.parent(field) {
                ui.set_tooltip(row, tip, None);
            }
            numbers.push((which, field));
        };

        let rows = part(ui, root, "Trunk", "sprout");
        let row = prop(ui, rows, "Kind");
        ui.set_tooltip(
            row,
            "Broadleaf: a rounded crown of forking branches. Conifer: rings of branches. Bush: stems",
            None,
        );
        let picked = PlantKind::ALL.iter().position(|kind| *kind == plant.kind).unwrap_or_default();
        let kind = ui.dropdown(row, &PlantKind::ALL.map(PlantKind::name), picked);
        ui.add_class(kind, "value-choice");
        let height = range_row(ui, rows, "Height", "blocks", plant.height);
        number(ui, rows, Number::Trunk, "Thickness", "blocks", "How thick its trunk is at its foot");
        number(
            ui,
            rows,
            Number::Taper,
            "Taper",
            "",
            "How much thinner its trunk is at its top: 0 for none, 1 to a point",
        );
        number(ui, rows, Number::Branch, "Branch", "blocks", "How thick its branches are: logs as thin as this");
        number(ui, rows, Number::Bend, "Bend", "", "How far its trunk leans and wanders: 0 for straight");

        let rows = part(ui, root, "Crown", "sprout");
        let crown_width = range_row(ui, rows, "Width", "blocks", plant.crown_width);
        let crown_height = range_row(ui, rows, "Height", "blocks", plant.crown_height);
        number(ui, rows, Number::Branches, "Branches", "", "How thick with branches its crown is, 0 to 1");
        number(ui, rows, Number::LeafSize, "Leaf size", "blocks", "How large a clump of leaves is");
        number(ui, rows, Number::LeafDensity, "Leaf density", "", "How full a clump of leaves is, 0 to 1");
        number(ui, rows, Number::Roots, "Roots", "", "How many roots run out from its foot");
        number(ui, rows, Number::Vines, "Vines", "", "How many of its trees have vines, 0 to 1");

        let rows = part(ui, root, "Blocks", "block");
        let mut slots = Vec::new();
        for (which, label) in PlantRef::ALL {
            let shown = slot(ui, rows, label, "block");
            let path = which.of(&plant).and_then(path_of);
            show_slot(ui, shown, path.as_deref(), "Block");
            let row = ui.parent(shown.field).expect("a slot is in its row");
            let clear = ui.icon_button(row, "close");
            ui.add_class(clear, "slot-clear");
            ui.set_tooltip(clear, "None", None);
            ui.set_visible(clear, which.of(&plant).is_some());
            slots.push(BlockSlot { which, slot: shown, path, clear });
        }

        let rows = part(ui, root, "Saplings", "sprout");
        let scattered = plant.saplings.map(|count| count as f32);
        let saplings = range_row(ui, rows, "Scattered", "", scattered);
        if let Some(row) = ui.parent(saplings[0]) {
            ui.set_tooltip(row, "How many saplings it scatters where its crown lands as it is felled", None);
        }
        number(ui, rows, Number::GrowTime, "Grow time", "min", "How long a sapling takes to grow into it");
        number(ui, rows, Number::Stages, "Stages", "", "How many sizes it grows through, the last the whole plant");
        number(ui, rows, Number::Light, "Light", "", "The least light a sapling needs: 0 for none, 1 for open sky");

        Self { plant, kind, height, crown_width, crown_height, saplings, numbers, slots }
    }

    /// What was done this frame, the plant changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<PlantChange> {
        for field in &self.slots {
            if ui.clicked(field.clear) {
                field.which.set(&mut self.plant, None);
                return Some(PlantChange::Shape);
            }
            if let Some(source) = ui.dropped_onto(field.slot.field) {
                return Some(PlantChange::Drop(field.which, source));
            }
            if ui.clicked(field.slot.arrow) {
                return Some(PlantChange::Browse(field.which, field.slot.field));
            }
            if let Some(path) = field.path.clone().filter(|_| ui.clicked(field.slot.field)) {
                return Some(PlantChange::Ping(path));
            }
        }
        let plant = &mut self.plant;
        if ui.changed(self.kind)
            && let Some(&kind) = PlantKind::ALL.get(ui.selected_option(self.kind))
            && kind != plant.kind
        {
            plant.kind = kind;
            return Some(PlantChange::Shape);
        }
        let mut typed = false;
        let sizes = [(self.height, &mut plant.height), (self.crown_width, &mut plant.crown_width)];
        for (fields, range) in sizes {
            if let Some(typed_range) = changed_range(ui, fields, *range) {
                (*range, typed) = (typed_range.map(|blocks| blocks.clamp(1.0, 128.0)), true);
            }
        }
        if let Some(range) = changed_range(ui, self.crown_height, plant.crown_height) {
            (plant.crown_height, typed) = (range.map(|blocks| blocks.clamp(1.0, 128.0)), true);
        }
        let scattered = plant.saplings.map(|count| count as f32);
        if let Some(range) = changed_range(ui, self.saplings, scattered) {
            (plant.saplings, typed) = (range.map(|count| count.round().clamp(0.0, 16.0) as u32), true);
        }
        for &(which, field) in &self.numbers {
            if let Some(value) = changed_number(ui, field) {
                which.set(plant, value);
                typed = true;
            }
        }
        typed.then_some(PlantChange::Typed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plants_blocks_are_set_and_taken_out_and_its_numbers_kept_within_what_they_can_be() {
        let mut plant = PlantDef::default();
        PlantRef::Sapling.set(&mut plant, Some("Blocks/Oak Sapling.block".into()));
        assert_eq!(PlantRef::Sapling.of(&plant), Some("Blocks/Oak Sapling.block"));
        assert!(plant.to_text().contains("sapling = \"Blocks/Oak Sapling.block\""));
        PlantRef::Sapling.set(&mut plant, None);
        assert!(!plant.to_text().contains("sapling"));
        Number::Stages.set(&mut plant, 40.0);
        Number::GrowTime.set(&mut plant, 5.0);
        Number::Branch.set(&mut plant, 0.1);
        assert_eq!((plant.stages, plant.grow_time, plant.branch), (8, 300.0, 0.25));
        assert_eq!(Number::GrowTime.get(&plant), 5.0, "shown in minutes");
    }
}
