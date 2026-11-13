//! A Biome in the Inspector, laid out as a block's is. Its **Color**, as
//! maps and the lists of biomes show it. Its **Climate**: how warm and how
//! wet the places it takes are, from and to, with a **Preset** that sets
//! both to one of the eight climates at once (and says Custom once they
//! are any others), and the heights over the sea it keeps to, where it
//! keeps to any. Its **Ground**: the block on top, the block under it and
//! how deep that lies, the block deep down, the block of steep ground with
//! the slope it starts at, and the block along the shore with how high
//! over and under the sea that reaches. Each block is one of the
//! project's, picked from them all, and a cross takes it out again: a
//! block left out is the world's own there.
//!
//! Each change is saved as it is made, and every world listing the biome
//! has its ground made again at once (see `editor::biomes`).

use voxear_terrain::{
    generate::{BiomeDef, BiomeGround},
    surfaces::Biome,
};
use voxear_ui::{DragSource, NodeId, PickedColor, Style, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    inspector::{color_row, number, prop},
};

/// What was done to a biome's fields this frame.
pub enum BiomeChange {
    /// A value typed: saved as it is shown.
    Typed,
    /// What it shows changed: saved, and shown again.
    Shape,
    /// The arrow of a block's field, to open the picker under.
    Browse(BiomeRef, NodeId),
    /// Something dragged onto a block's field.
    Drop(BiomeRef, DragSource),
    /// A click on a block's field: show the block in the Project panel.
    Ping(String),
}

/// One of the blocks a biome's ground is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiomeRef {
    Top,
    Under,
    Deep,
    Steep,
    Shore,
}

impl BiomeRef {
    /// The reference the field holds in `ground`.
    fn of(self, ground: &BiomeGround) -> Option<&str> {
        match self {
            BiomeRef::Top => ground.top.as_deref(),
            BiomeRef::Under => ground.under.as_deref(),
            BiomeRef::Deep => ground.deep.as_deref(),
            BiomeRef::Steep => ground.steep.as_deref(),
            BiomeRef::Shore => ground.shore.as_deref(),
        }
    }

    /// Puts `reference` in the field of `biome`.
    pub fn set(self, biome: &mut BiomeDef, reference: Option<String>) {
        let ground = &mut biome.ground;
        match self {
            BiomeRef::Top => ground.top = reference,
            BiomeRef::Under => ground.under = reference,
            BiomeRef::Deep => ground.deep = reference,
            BiomeRef::Steep => ground.steep = reference,
            BiomeRef::Shore => ground.shore = reference,
        }
    }
}

/// The widest a biome's temperatures and humidities are: any at all.
const ANY_TEMPERATURE: [f32; 2] = [-60.0, 60.0];
const ANY_HUMIDITY: [f32; 2] = [0.0, 1.0];

/// The heights a biome keeps to when it first keeps to any, in metres.
const FIRST_HEIGHTS: [f32; 2] = [200.0, 10_000.0];

/// The temperatures and humidities of one of the eight climates.
fn ranges(climate: Biome) -> ([f32; 2], [f32; 2]) {
    let (temperature, humidity) = climate.ranges();
    (temperature.unwrap_or(ANY_TEMPERATURE), humidity.unwrap_or(ANY_HUMIDITY))
}

/// Which of the eight climates `biome`'s temperatures and humidities are,
/// where they are one's.
fn preset_of(biome: &BiomeDef) -> Option<usize> {
    Biome::ALL.iter().position(|&climate| ranges(climate) == (biome.temperature, biome.humidity))
}

/// A block's field of a biome: which, its slot, the path of the block it
/// names, and its cross.
struct BlockSlot {
    which: BiomeRef,
    slot: Slot,
    path: Option<String>,
    clear: NodeId,
}

/// A biome's fields, and the biome as they show it.
pub struct BiomeFields {
    pub biome: BiomeDef,
    color: [NodeId; 3],
    swatch: NodeId,
    preset: NodeId,
    temperature: [NodeId; 2],
    humidity: [NodeId; 2],
    height: NodeId,
    heights: Option<[NodeId; 2]>,
    under_depth: NodeId,
    steep_slope: NodeId,
    shore_height: NodeId,
    slots: Vec<BlockSlot>,
}

impl BiomeFields {
    /// The fields of `biome` under `root`; `path_of` gives the project path
    /// of the block a reference names.
    pub fn new(ui: &mut Ui, root: NodeId, biome: BiomeDef, path_of: &dyn Fn(&str) -> Option<String>) -> Self {
        let rows = part(ui, root, "Biome", "sprout");
        let (color, swatch) = color_row(ui, rows, "Color");
        for (shown, value) in color.iter().zip(biome.color) {
            ui.set_text(*shown, &number(value));
        }

        let rows = part(ui, root, "Climate", "light");
        let row = prop(ui, rows, "Preset");
        let mut names: Vec<&str> = Biome::ALL.map(Biome::name).to_vec();
        let picked = preset_of(&biome).unwrap_or_else(|| {
            names.push("Custom");
            names.len() - 1
        });
        let preset = ui.dropdown(row, &names, picked);
        ui.add_class(preset, "value-choice");
        let temperature = range_row(ui, rows, "Temperature", "°C", biome.temperature);
        let humidity = range_row(ui, rows, "Humidity", "", biome.humidity);
        let row = prop(ui, rows, "Height");
        let height = ui.toggle(row, "", biome.height.is_some());
        ui.add_class(height, "checkbox");
        let heights = biome.height.map(|range| range_row(ui, rows, "Heights", "m", range));

        let rows = part(ui, root, "Ground", "block");
        let mut slots = Vec::new();
        let mut block = |ui: &mut Ui, which: BiomeRef, label: &str| {
            let shown = slot(ui, rows, label, "block");
            let path = which.of(&biome.ground).and_then(path_of);
            show_slot(ui, shown, path.as_deref(), "Block");
            // A cross beside it takes the block out again.
            let row = ui.parent(shown.field).expect("a slot is in its row");
            let clear = ui.icon_button(row, "close");
            ui.add_class(clear, "slot-clear");
            ui.set_tooltip(clear, "None", None);
            ui.set_visible(clear, which.of(&biome.ground).is_some());
            slots.push(BlockSlot { which, slot: shown, path, clear });
        };
        block(ui, BiomeRef::Top, "Top");
        block(ui, BiomeRef::Under, "Under");
        let under_depth = number_row(ui, rows, "Under depth", "", biome.ground.under_depth as f32);
        block(ui, BiomeRef::Deep, "Deep");
        block(ui, BiomeRef::Steep, "Steep");
        let steep_slope = number_row(ui, rows, "Steep slope", "°", biome.ground.steep_slope);
        block(ui, BiomeRef::Shore, "Shore");
        let shore_height = number_row(ui, rows, "Shore height", "m", biome.ground.shore_height);

        let fields = Self {
            biome,
            color,
            swatch,
            preset,
            temperature,
            humidity,
            height,
            heights,
            under_depth,
            steep_slope,
            shore_height,
            slots,
        };
        fields.show_swatch(ui);
        fields
    }

    /// What was done this frame, the biome changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<BiomeChange> {
        for field in &self.slots {
            if ui.clicked(field.clear) {
                field.which.set(&mut self.biome, None);
                return Some(BiomeChange::Shape);
            }
            if let Some(source) = ui.dropped_onto(field.slot.field) {
                return Some(BiomeChange::Drop(field.which, source));
            }
            if ui.clicked(field.slot.arrow) {
                return Some(BiomeChange::Browse(field.which, field.slot.field));
            }
            if let Some(path) = field.path.clone().filter(|_| ui.clicked(field.slot.field)) {
                return Some(BiomeChange::Ping(path));
            }
        }
        let biome = &mut self.biome;
        // A preset sets how warm and how wet at once; Custom is what any others are called, and sets nothing.
        if ui.changed(self.preset)
            && let Some(&climate) = Biome::ALL.get(ui.selected_option(self.preset))
            && ranges(climate) != (biome.temperature, biome.humidity)
        {
            (biome.temperature, biome.humidity) = ranges(climate);
            return Some(BiomeChange::Shape);
        }
        if ui.changed(self.height) {
            let keeps = ui.is_checked(self.height);
            if keeps != biome.height.is_some() {
                biome.height = keeps.then_some(FIRST_HEIGHTS);
                return Some(BiomeChange::Shape);
            }
        }
        let was_preset = preset_of(biome);
        let mut typed = false;
        if self.color.iter().any(|&field| ui.changed(field)) {
            for (at, field) in self.color.iter().enumerate() {
                biome.color[at] = ui.text(*field).trim().parse::<f32>().unwrap_or(biome.color[at]).clamp(0.0, 1.0);
            }
            typed = true;
        }
        if let Some(range) = changed_range(ui, self.temperature, biome.temperature) {
            (biome.temperature, typed) = (range.map(|degrees| degrees.clamp(-100.0, 100.0)), true);
        }
        if let Some(range) = changed_range(ui, self.humidity, biome.humidity) {
            (biome.humidity, typed) = (range.map(|wet| wet.clamp(0.0, 1.0)), true);
        }
        if let (Some(fields), Some(now)) = (self.heights, biome.height)
            && let Some(range) = changed_range(ui, fields, now)
        {
            (biome.height, typed) = (Some(range), true);
        }
        if let Some(depth) = changed_number(ui, self.under_depth) {
            (biome.ground.under_depth, typed) = (depth.round().clamp(0.0, 64.0) as u32, true);
        }
        if let Some(degrees) = changed_number(ui, self.steep_slope) {
            (biome.ground.steep_slope, typed) = (degrees.clamp(0.0, 85.0), true);
        }
        if let Some(metres) = changed_number(ui, self.shore_height) {
            (biome.ground.shore_height, typed) = (metres.max(0.0), true);
        }
        if !typed {
            return None;
        }
        self.show_swatch(ui);
        // Ranges typed into or out of a preset's are shown again, the Preset saying which they are now.
        Some(if preset_of(&self.biome) == was_preset { BiomeChange::Typed } else { BiomeChange::Shape })
    }

    /// Its colour's swatch where it was clicked, with the colour as the
    /// picker shows it.
    pub fn color_clicked(&self, ui: &Ui) -> Option<(NodeId, PickedColor)> {
        ui.clicked(self.swatch).then(|| (self.swatch, PickedColor::srgb(self.biome.color)))
    }

    /// Sets its colour from the colour picker, in its fields and swatch.
    pub fn set_color(&mut self, ui: &mut Ui, color: PickedColor) {
        self.biome.color = color.srgb;
        for (field, value) in self.color.into_iter().zip(color.srgb) {
            ui.set_text(field, &number(value));
        }
        self.show_swatch(ui);
    }

    fn show_swatch(&self, ui: &mut Ui) {
        let hex = PickedColor::srgb(self.biome.color).hex();
        ui.set_inline_style(self.swatch, &format!("background-color: {hex}"));
    }
}

/// A row of a number, with its unit after it where it has one.
fn number_row(ui: &mut Ui, rows: NodeId, label: &str, unit: &str, value: f32) -> NodeId {
    let row = prop(ui, rows, label);
    let within = with_unit(ui, row, unit);
    let field = ui.text_field(within, &number(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    show_unit(ui, within, unit);
    field
}

/// A row of two numbers, from and to, with their unit after them where
/// they have one.
fn range_row(ui: &mut Ui, rows: NodeId, label: &str, unit: &str, range: [f32; 2]) -> [NodeId; 2] {
    let row = prop(ui, rows, label);
    let within = with_unit(ui, row, unit);
    let pair = ui.panel(within, Style::row());
    ui.add_class(pair, "vector");
    ui.add_class(pair, "pair");
    let fields = range.map(|value| {
        let field = ui.text_field(pair, &number(value));
        ui.add_class(field, "value-input");
        ui.add_class(field, "number-input");
        field
    });
    show_unit(ui, within, unit);
    fields
}

/// Where a row's fields go: beside their unit, where they have one.
fn with_unit(ui: &mut Ui, row: NodeId, unit: &str) -> NodeId {
    if unit.is_empty() {
        return row;
    }
    let within = ui.panel(row, Style::row());
    ui.add_class(within, "value-with-unit");
    within
}

fn show_unit(ui: &mut Ui, within: NodeId, unit: &str) {
    if !unit.is_empty() {
        let unit = ui.add_label(within, unit);
        ui.add_class(unit, "value-unit");
    }
}

fn changed_number(ui: &Ui, field: NodeId) -> Option<f32> {
    ui.changed(field).then(|| ui.text(field).trim().parse().ok()).flatten()
}

/// A range typed into: from the lesser to the greater of what its two
/// fields say, one that is no number kept as `now` has it.
fn changed_range(ui: &Ui, fields: [NodeId; 2], now: [f32; 2]) -> Option<[f32; 2]> {
    if !fields.iter().any(|&field| ui.changed(field)) {
        return None;
    }
    let [low, high] = [0, 1].map(|at| ui.text(fields[at]).trim().parse::<f32>().unwrap_or(now[at]));
    Some([low.min(high), low.max(high)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_biomes_ranges_are_a_preset_while_they_are_one_of_the_eight_climates() {
        let mut biome = BiomeDef::default();
        assert_eq!(preset_of(&biome), None, "a new biome's ranges are its own");
        (biome.temperature, biome.humidity) = ranges(Biome::Desert);
        assert_eq!(preset_of(&biome).map(|at| Biome::ALL[at]), Some(Biome::Desert));
        // Snow takes any humidity: its preset is the whole of it.
        (biome.temperature, biome.humidity) = ranges(Biome::Snow);
        assert_eq!((biome.humidity, preset_of(&biome)), (ANY_HUMIDITY, Some(0)));
        biome.temperature[1] += 1.0;
        assert_eq!(preset_of(&biome), None, "changed, it is Custom");
        // A block set and taken out again.
        BiomeRef::Steep.set(&mut biome, Some("Blocks/Stone.block".into()));
        assert_eq!(BiomeRef::Steep.of(&biome.ground), Some("Blocks/Stone.block"));
        BiomeRef::Steep.set(&mut biome, None);
        assert_eq!(biome.ground, BiomeGround::default());
    }
}
