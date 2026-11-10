//! A Block in the Inspector: its **Look**, laid out as a material's is.
//! Its colour, the material every face is drawn with and those its top and
//! bottom have of their own, and which liquid it is. A liquid is drawn in
//! its colour alone, so its materials go. Each change is saved as it is
//! made, and every world of blocks listing the block takes it up at once
//! (see `editor::blocks`).

use voxear_terrain::blocks::block::{BlockDef, Liquid};
use voxear_ui::{DragSource, NodeId, PickedColor, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    inspector::{color_row, number, prop},
};

/// What was done to a block's fields this frame.
pub enum BlockChange {
    /// A value typed: saved as it is shown.
    Typed,
    /// What it shows changed: saved, and shown again.
    Shape,
    /// The arrow of a field naming an asset, to open the picker under.
    Browse(BlockRef, NodeId),
    /// Something dragged onto such a field.
    Drop(BlockRef, DragSource),
    /// A click on a field naming an asset: show it in the Project panel.
    Ping(String),
}

/// One of a block's fields naming an asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockRef {
    Material,
    Top,
    Bottom,
}

impl BlockRef {
    const FACES: [(BlockRef, &'static str); 3] =
        [(BlockRef::Material, "Material"), (BlockRef::Top, "Top"), (BlockRef::Bottom, "Bottom")];

    /// The reference the field holds in `block`.
    fn of(self, block: &BlockDef) -> Option<&str> {
        match self {
            BlockRef::Material => block.material.as_deref(),
            BlockRef::Top => block.top.as_deref(),
            BlockRef::Bottom => block.bottom.as_deref(),
        }
    }

    /// Puts `reference` in the field of `block`.
    pub fn set(self, block: &mut BlockDef, reference: Option<String>) {
        match self {
            BlockRef::Material => block.material = reference,
            BlockRef::Top => block.top = reference,
            BlockRef::Bottom => block.bottom = reference,
        }
    }
}

const LIQUIDS: [(Liquid, &str); 2] = [(Liquid::None, "None"), (Liquid::Water, "Water")];

/// A block's fields, and the block as they show it.
pub struct BlockFields {
    pub block: BlockDef,
    color: [NodeId; 3],
    swatch: NodeId,
    liquid: NodeId,
    /// Its materials' fields, each with the path of what it names: none for
    /// a liquid.
    faces: Vec<(BlockRef, Slot, Option<String>)>,
}

impl BlockFields {
    /// The fields of `block` under `root`; `path_of` gives the project path
    /// of the asset a reference names.
    pub fn new(ui: &mut Ui, root: NodeId, block: BlockDef, path_of: &dyn Fn(&str) -> Option<String>) -> Self {
        let rows = part(ui, root, "Look", "block");
        let (color, swatch) = color_row(ui, rows, "Color");
        for (field, value) in color.iter().zip(block.color) {
            ui.set_text(*field, &number(value));
        }
        let mut faces = Vec::new();
        if block.liquid.is_none() {
            for (which, label) in BlockRef::FACES {
                let field = slot(ui, rows, label, "material");
                let path = which.of(&block).and_then(path_of);
                show_slot(ui, field, path.as_deref(), "Material");
                faces.push((which, field, path));
            }
        }
        let row = prop(ui, rows, "Liquid");
        let picked = LIQUIDS.iter().position(|(liquid, _)| *liquid == block.liquid).unwrap_or_default();
        let liquid = ui.dropdown(row, &LIQUIDS.map(|(_, name)| name), picked);
        ui.add_class(liquid, "value-choice");
        let fields = Self { block, color, swatch, liquid, faces };
        fields.show_swatch(ui);
        fields
    }

    /// What was done this frame, the block changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<BlockChange> {
        for (which, field, path) in &self.faces {
            if let Some(source) = ui.dropped_onto(field.field) {
                return Some(BlockChange::Drop(*which, source));
            }
            if ui.clicked(field.arrow) {
                return Some(BlockChange::Browse(*which, field.field));
            }
            if let Some(path) = path.clone().filter(|_| ui.clicked(field.field)) {
                return Some(BlockChange::Ping(path));
            }
        }
        if ui.changed(self.liquid) {
            let liquid = LIQUIDS[ui.selected_option(self.liquid).min(LIQUIDS.len() - 1)].0;
            if liquid != self.block.liquid {
                self.block.liquid = liquid;
                return Some(BlockChange::Shape);
            }
        }
        if !self.color.iter().any(|&field| ui.changed(field)) {
            return None;
        }
        for (at, field) in self.color.iter().enumerate() {
            let typed = ui.text(*field).trim().parse::<f32>().unwrap_or(self.block.color[at]);
            self.block.color[at] = typed.clamp(0.0, 1.0);
        }
        self.show_swatch(ui);
        Some(BlockChange::Typed)
    }

    /// Its colour's swatch where it was clicked, with the colour as the
    /// picker shows it.
    pub fn color_clicked(&self, ui: &Ui) -> Option<(NodeId, PickedColor)> {
        ui.clicked(self.swatch).then(|| (self.swatch, PickedColor::srgb(self.block.color)))
    }

    /// Sets its colour from the colour picker, in its fields and swatch.
    pub fn set_color(&mut self, ui: &mut Ui, color: PickedColor) {
        self.block.color = color.srgb;
        for (field, value) in self.color.into_iter().zip(color.srgb) {
            ui.set_text(field, &number(value));
        }
        self.show_swatch(ui);
    }

    fn show_swatch(&self, ui: &mut Ui) {
        let hex = PickedColor::srgb(self.block.color).hex();
        ui.set_inline_style(self.swatch, &format!("background-color: {hex}"));
    }
}
