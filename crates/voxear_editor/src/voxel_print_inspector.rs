//! A Voxel Print (`.voxprint`) in the Inspector: what it is, its size in
//! voxels and metres, how many of them are solid, its colours and its
//! file's size; and how it is made, which **Make Again** makes it from: its
//! **Model**, how many voxels a metre, how large, how many colours, what it
//! leaves inside its skin, and a picture for each of the model's parts
//! whose file names none, as a model exported without its pictures.
//!
//! Its settings change nothing until it is made again, as making it reads
//! the whole model (see `editor::voxel_prints`).

use voxear_render::{VoxelPrint, VoxelPrintFill, VoxelPrintPicture, VoxelPrintSettings};
use voxear_ui::{DragSource, NodeId, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    biome_inspector::{changed_number, number_row},
    inspector::prop,
};

/// What was done to a print's fields this frame.
pub enum PrintChange {
    /// The arrow of a field, to open the picker under.
    Browse(PrintRef, NodeId),
    /// Something dragged onto a field.
    Drop(PrintRef, DragSource),
    /// A click on a field: show its asset in the Project panel.
    Ping(String),
    /// Make Again: the print made from its model as the fields say.
    Make(VoxelPrintSettings),
}

/// A field of a print that names an asset: its model, or the picture of one
/// of the model's parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrintRef {
    Model,
    Picture(String),
}

/// A field naming an asset: which, its slot, the path it shows, and its
/// cross where it can be emptied.
struct RefSlot {
    which: PrintRef,
    slot: Slot,
    path: Option<String>,
    clear: Option<NodeId>,
}

/// A print's fields, and its settings as they show them.
pub struct VoxelPrintFields {
    pub settings: VoxelPrintSettings,
    per_metre: NodeId,
    scale: NodeId,
    colours: NodeId,
    fill: NodeId,
    /// The rows of the parts' pictures, filled once the model's parts are
    /// known.
    pictures: NodeId,
    filled: bool,
    slots: Vec<RefSlot>,
    make: NodeId,
}

impl VoxelPrintFields {
    /// The fields of `print` under `root`, or of a file that could not be
    /// read; `path_of` gives the project path of the asset a reference names.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        print: Option<&VoxelPrint>,
        bytes: usize,
        path_of: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let rows = part(ui, root, "Voxel Print", "box");
        match print {
            Some(print) => {
                let [x, y, z] = print.size;
                let m = print.metres();
                text(ui, rows, "Size", &format!("{x} × {y} × {z}"));
                text(ui, rows, "Metres", &format!("{:.2} × {:.2} × {:.2}", m.x, m.y, m.z));
                text(ui, rows, "Solid", &thousands(print.solid));
                text(ui, rows, "Colours", &print.palette.len().to_string());
                text(ui, rows, "File", &size_of(bytes));
            }
            None => text(ui, rows, "File", "Could not be read"),
        }
        let settings = print.map(|print| print.settings.clone()).unwrap_or_default();
        let rows = part(ui, root, "Made From", "box");
        let model = slot(ui, rows, "Model", "box");
        let model_path = path_of(&settings.model);
        show_slot(ui, model, model_path.as_deref(), "Model");
        let mut slots = vec![RefSlot { which: PrintRef::Model, slot: model, path: model_path, clear: None }];
        let per_metre = number_row(ui, rows, "Voxels a Metre", "", settings.voxels_per_metre as f32);
        tip(ui, per_metre, "As many as the Voxel Volumes it is printed into, for voxels of the same size as theirs");
        let scale = number_row(ui, rows, "Scale", "", settings.scale);
        tip(ui, scale, "Multiplies the model's size");
        let colours = number_row(ui, rows, "Colours", "", settings.colours as f32);
        tip(ui, colours, "The most colours its voxels are in, each the nearest of them, up to 64");
        let row = prop(ui, rows, "Fill");
        ui.set_tooltip(
            row,
            "Solid: solid where the outside cannot reach. Sealed: solid too where its skin is round a place on five sides, for a model open underneath. Shell: its skin alone",
            None,
        );
        let picked = VoxelPrintFill::ALL.iter().position(|fill| *fill == settings.fill).unwrap_or_default();
        let fill = ui.dropdown(row, &VoxelPrintFill::ALL.map(VoxelPrintFill::name), picked);
        ui.add_class(fill, "value-choice");
        let pictures = part(ui, root, "Pictures", "texture");
        // The pictures it was made with, until the model's parts are known.
        for given in &settings.pictures {
            slots.push(picture_slot(ui, pictures, &given.part, &given.part, path_of(&given.picture)));
        }
        let make = ui.button(root, "Make Again");
        ui.add_class(make, "add-component");
        ui.set_tooltip(make, "Make it from its model as these settings say", None);
        Self { settings, per_metre, scale, colours, fill, pictures, filled: false, slots, make }
    }

    /// Whether the model's parts are wanted, to list a picture for each.
    pub fn wants_parts(&self) -> bool {
        !self.filled && !self.settings.model.is_empty()
    }

    /// Lists a picture field for each of the model's parts, by part and
    /// name.
    pub fn fill_parts(&mut self, ui: &mut Ui, parts: &[(String, String)], path_of: &dyn Fn(&str) -> Option<String>) {
        self.filled = true;
        for child in ui.children(self.pictures) {
            ui.remove(child);
        }
        self.slots.retain(|slot| slot.which == PrintRef::Model);
        for (number, (part, name)) in parts.iter().enumerate() {
            let label = if name.is_empty() { format!("Material {number}") } else { name.clone() };
            let given = self.settings.pictures.iter().find(|given| given.part == *part);
            let path = given.and_then(|given| path_of(&given.picture));
            self.slots.push(picture_slot(ui, self.pictures, &label, part, path));
        }
        if parts.is_empty() {
            text(ui, self.pictures, "Parts", "None");
        }
    }

    /// Puts `reference` in a field, the asset at `path` shown in it.
    pub fn set(&mut self, ui: &mut Ui, which: &PrintRef, reference: Option<String>, path: Option<String>) {
        match which {
            PrintRef::Model => {
                self.settings.model = reference.unwrap_or_default();
                self.filled = false;
            }
            PrintRef::Picture(part) => {
                self.settings.pictures.retain(|given| given.part != *part);
                if let Some(picture) = reference {
                    self.settings.pictures.push(VoxelPrintPicture { part: part.clone(), picture });
                }
            }
        }
        if let Some(field) = self.slots.iter_mut().find(|field| field.which == *which) {
            let kind = if *which == PrintRef::Model { "Model" } else { "Texture" };
            show_slot(ui, field.slot, path.as_deref(), kind);
            if let Some(clear) = field.clear {
                ui.set_visible(clear, path.is_some());
            }
            field.path = path;
        }
    }

    /// What was done this frame, the settings changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<PrintChange> {
        let mut emptied = None;
        for field in &self.slots {
            if field.clear.is_some_and(|clear| ui.clicked(clear)) {
                emptied = Some(field.which.clone());
            }
            if let Some(source) = ui.dropped_onto(field.slot.field) {
                return Some(PrintChange::Drop(field.which.clone(), source));
            }
            if ui.clicked(field.slot.arrow) {
                return Some(PrintChange::Browse(field.which.clone(), field.slot.field));
            }
            if let Some(path) = field.path.clone().filter(|_| ui.clicked(field.slot.field)) {
                return Some(PrintChange::Ping(path));
            }
        }
        if let Some(which) = emptied {
            self.set(ui, &which, None, None);
        }
        let settings = &mut self.settings;
        if let Some(value) = changed_number(ui, self.per_metre) {
            settings.voxels_per_metre = (value.round() as u32).clamp(1, voxear_render::MOST_VOXELS_A_METRE);
        }
        if let Some(value) = changed_number(ui, self.scale) {
            settings.scale = value.clamp(0.01, 100.0);
        }
        if let Some(value) = changed_number(ui, self.colours) {
            settings.colours = (value.round() as u32).clamp(1, voxear_render::MOST_PRINT_COLOURS);
        }
        if ui.changed(self.fill)
            && let Some(&fill) = VoxelPrintFill::ALL.get(ui.selected_option(self.fill))
        {
            settings.fill = fill;
        }
        ui.clicked(self.make).then(|| PrintChange::Make(self.settings.clone()))
    }
}

/// A picture field for a part, with its cross.
fn picture_slot(ui: &mut Ui, rows: NodeId, label: &str, part: &str, path: Option<String>) -> RefSlot {
    let shown = slot(ui, rows, label, "texture");
    show_slot(ui, shown, path.as_deref(), "Texture");
    ui.set_tooltip(shown.field, "The picture the part is coloured from, in place of its own", None);
    let row = ui.parent(shown.field).expect("a slot is in its row");
    let clear = ui.icon_button(row, "close");
    ui.add_class(clear, "slot-clear");
    ui.set_tooltip(clear, "None", None);
    ui.set_visible(clear, path.is_some());
    RefSlot { which: PrintRef::Picture(part.to_string()), slot: shown, path, clear: Some(clear) }
}

fn text(ui: &mut Ui, rows: NodeId, label: &str, value: &str) {
    let row = prop(ui, rows, label);
    let value = ui.add_label(row, value);
    ui.add_class(value, "value");
}

fn tip(ui: &mut Ui, field: NodeId, tip: &str) {
    if let Some(row) = ui.parent(field).and_then(|within| ui.parent(within)) {
        ui.set_tooltip(row, tip, None);
    }
}

/// A count with its thousands apart: 1,234,567.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A file's size as file managers write it.
fn size_of(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f32 / 1024.0),
        b => format!("{:.1} MB", b as f32 / (1024.0 * 1024.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_sizes_read_as_people_write_them() {
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
        assert_eq!(size_of(2048), "2.0 KB");
        assert_eq!(size_of(512), "512 B");
    }
}
