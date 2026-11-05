//! A material's custom shader in its Inspector: a Shader field, the
//! Standard shader or one of the project's `.shader` assets, picked from
//! the object picker or dropped from the Project panel, and under the
//! material's own settings a section of the shader's properties, each
//! edited as its kind is: a number, a vector, a colour with its swatch, a
//! toggle, or a picture. A value set to the shader's default is left out
//! of the material's file, so changing the default changes it.

use voxear_assets::AssetDb;
use voxear_engine::{
    MaterialData, PropertyValue,
    shaders::{PropertyKind, ShaderInfo, ShaderProperty, ShaderRegistry},
};
use voxear_ui::{DragSource, NodeId, PickedColor, Style, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    inspector::{color_row, number, prop},
};

/// What the shader's fields ask of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum ShaderEdit {
    /// Open the object picker under the Shader field.
    BrowseShader(NodeId),
    /// Something was dragged onto the Shader field.
    DropShader(DragSource),
    /// Show the shader, or a property's picture, in the Project panel.
    Ping(String),
    /// Open the object picker under a picture property's field.
    BrowsePicture { property: String, anchor: NodeId },
    /// Something was dragged onto a picture property's field.
    DropPicture { property: String, source: DragSource },
    /// A property's value changed, in the material's data, to save.
    Changed,
}

/// A property's field, as its kind edits it.
enum Control {
    Number(NodeId),
    Vector(Vec<NodeId>),
    Color { fields: Vec<NodeId>, swatch: NodeId },
    Toggle(NodeId),
    Picture { slot: Slot, path: Option<String> },
}

struct PropertyRow {
    property: ShaderProperty,
    control: Control,
}

pub struct ShaderFields {
    shader: Slot,
    /// The shader picked, by its path, to ping.
    path: Option<String>,
    rows: Vec<PropertyRow>,
}

impl ShaderFields {
    /// The Shader field into `rows`, and the shader's properties, as `data`
    /// sets them, in a section of their own in `root`.
    pub fn build(
        ui: &mut Ui,
        (rows, root): (NodeId, NodeId),
        data: &MaterialData,
        registry: &ShaderRegistry,
        db: &AssetDb,
    ) -> Self {
        let shader = slot(ui, rows, "Shader", "shader");
        let picked = data.shader.and_then(|guid| registry.shaders.iter().find(|shader| shader.guid == guid));
        let path = picked.map(|shader| shader.path.clone()).or_else(|| {
            let guid = data.shader?;
            db.get(guid).map(|entry| entry.path.clone())
        });
        match &path {
            Some(path) => show_slot(ui, shader, Some(path), "Shader"),
            None => {
                ui.set_text(shader.label, "Standard");
                ui.set_visible(shader.icon, true);
            }
        }
        let mut fields = Self { shader, path, rows: Vec::new() };
        let Some(info) = picked.and_then(|shader| shader.info.as_ref().ok()) else {
            if let Some(Err(error)) = picked.map(|shader| &shader.info) {
                let rows = part(ui, root, "Shader", "shader");
                let row = prop(ui, rows, "Error");
                let text = ui.add_label(row, error);
                ui.add_class(text, "value");
            }
            return fields;
        };
        fields.build_properties(ui, root, info, data, db);
        fields
    }

    fn build_properties(&mut self, ui: &mut Ui, root: NodeId, info: &ShaderInfo, data: &MaterialData, db: &AssetDb) {
        if info.properties.is_empty() {
            return;
        }
        let rows = part(ui, root, &info.name, "shader");
        for property in &info.properties {
            let label = title(&property.name);
            let value = values(data, property);
            let control = match property.kind {
                PropertyKind::Float => {
                    let row = prop(ui, rows, &label);
                    Control::Number(number_field(ui, row, value[0]))
                }
                PropertyKind::Float2 | PropertyKind::Float3 | PropertyKind::Float4 => {
                    let row = prop(ui, rows, &label);
                    let vector = ui.panel(row, Style::row());
                    ui.add_class(vector, "vector");
                    let fields = (0..property.kind.values()).map(|i| number_field(ui, vector, value[i])).collect();
                    Control::Vector(fields)
                }
                PropertyKind::Color { alpha } => {
                    let (rgb, swatch) = color_row(ui, rows, &label);
                    let mut fields = rgb.to_vec();
                    if alpha {
                        let row = prop(ui, rows, &format!("{label} alpha"));
                        fields.push(number_field(ui, row, value[3]));
                    }
                    for (field, value) in fields.iter().zip(value) {
                        ui.set_text(*field, &number(value));
                    }
                    Control::Color { fields, swatch }
                }
                PropertyKind::Toggle => {
                    let row = prop(ui, rows, &label);
                    let toggle = ui.toggle(row, "", value[0] > 0.5);
                    ui.add_class(toggle, "checkbox");
                    Control::Toggle(toggle)
                }
                PropertyKind::Picture => {
                    let slot = slot(ui, rows, &label, "texture");
                    let guid = match data.properties.get(&property.name) {
                        Some(PropertyValue::Picture(guid)) => Some(*guid),
                        _ => None,
                    };
                    let path = guid.and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
                    show_slot(ui, slot, path.as_deref(), "Texture");
                    Control::Picture { slot, path }
                }
            };
            if let Some(tooltip) = &property.tooltip {
                let field = match &control {
                    Control::Number(field) | Control::Toggle(field) => *field,
                    Control::Vector(fields) | Control::Color { fields, .. } => fields[0],
                    Control::Picture { slot, .. } => slot.field,
                };
                ui.set_tooltip(field, tooltip, None);
            }
            self.rows.push(PropertyRow { property: property.clone(), control });
        }
        self.show_swatches(ui, data);
    }

    /// What was asked this frame; a property typed or toggled is set in
    /// `data` first.
    pub fn update(&self, ui: &mut Ui, data: &mut MaterialData) -> Option<ShaderEdit> {
        if let Some(source) = ui.dropped_onto(self.shader.field) {
            return Some(ShaderEdit::DropShader(source));
        }
        // The arrow first, as a click on it is a click on the field too.
        if ui.clicked(self.shader.arrow) {
            return Some(ShaderEdit::BrowseShader(self.shader.field));
        }
        if let Some(path) = self.path.clone().filter(|_| ui.clicked(self.shader.field)) {
            return Some(ShaderEdit::Ping(path));
        }
        for row in &self.rows {
            let name = row.property.name.clone();
            match &row.control {
                Control::Picture { slot, path } => {
                    if let Some(source) = ui.dropped_onto(slot.field) {
                        return Some(ShaderEdit::DropPicture { property: name, source });
                    }
                    if ui.clicked(slot.arrow) {
                        return Some(ShaderEdit::BrowsePicture { property: name, anchor: slot.field });
                    }
                    if let Some(path) = path.clone().filter(|_| ui.clicked(slot.field)) {
                        return Some(ShaderEdit::Ping(path));
                    }
                }
                Control::Toggle(toggle) if ui.changed(*toggle) => {
                    set(data, &row.property, [f32::from(u8::from(ui.is_checked(*toggle))), 0.0, 0.0, 0.0]);
                    return Some(ShaderEdit::Changed);
                }
                Control::Number(field) if ui.changed(*field) => {
                    let mut value = values(data, &row.property);
                    value[0] = read(ui, *field, value[0], row.property.range);
                    set(data, &row.property, value);
                    return Some(ShaderEdit::Changed);
                }
                Control::Vector(fields) | Control::Color { fields, .. } if fields.iter().any(|&f| ui.changed(f)) => {
                    let mut value = values(data, &row.property);
                    for (i, field) in fields.iter().enumerate() {
                        value[i] = read(ui, *field, value[i], row.property.range);
                    }
                    set(data, &row.property, value);
                    self.show_swatches(ui, data);
                    return Some(ShaderEdit::Changed);
                }
                _ => {}
            }
        }
        None
    }

    /// A colour property whose swatch was clicked, by its place, with the
    /// swatch and the colour as `data` has it.
    pub fn color_clicked(&self, ui: &Ui, data: &MaterialData) -> Option<(usize, NodeId, PickedColor)> {
        self.rows.iter().enumerate().find_map(|(at, row)| match &row.control {
            Control::Color { swatch, .. } if ui.clicked(*swatch) => {
                let [r, g, b, _] = values(data, &row.property);
                Some((at, *swatch, PickedColor::srgb([r, g, b])))
            }
            _ => None,
        })
    }

    /// Sets colour property `at` from the colour picker, in `data` and its
    /// fields.
    pub fn set_color(&self, ui: &mut Ui, at: usize, color: PickedColor, data: &mut MaterialData) {
        let Some(row) = self.rows.get(at) else { return };
        let Control::Color { fields, .. } = &row.control else { return };
        let mut value = values(data, &row.property);
        value[..3].copy_from_slice(&color.srgb);
        set(data, &row.property, value);
        for (field, value) in fields.iter().zip(value) {
            ui.set_text(*field, &number(value));
        }
        self.show_swatches(ui, data);
    }

    pub fn show_swatches(&self, ui: &mut Ui, data: &MaterialData) {
        for row in &self.rows {
            if let Control::Color { swatch, .. } = &row.control {
                let [r, g, b, _] = values(data, &row.property);
                ui.set_inline_style(*swatch, &format!("background-color: {}", PickedColor::srgb([r, g, b]).hex()));
            }
        }
    }
}

/// A property's name as a label: `waveSpeed` as "Wave speed".
fn title(name: &str) -> String {
    let mut words = String::new();
    for (at, c) in name.chars().enumerate() {
        match at {
            0 => words.extend(c.to_uppercase()),
            _ if c.is_uppercase() => {
                words.push(' ');
                words.extend(c.to_lowercase());
            }
            _ if c == '_' => words.push(' '),
            _ => words.push(c),
        }
    }
    words
}

/// A property's value as `data` sets it, or else its default, as numbers.
fn values(data: &MaterialData, property: &ShaderProperty) -> [f32; 4] {
    let mut value = property.default;
    match data.properties.get(&property.name) {
        Some(PropertyValue::Number(number)) => value[0] = *number,
        Some(PropertyValue::Toggle(on)) => value[0] = f32::from(u8::from(*on)),
        Some(PropertyValue::Vector(numbers)) => {
            for (slot, number) in value.iter_mut().zip(numbers) {
                *slot = *number;
            }
        }
        _ => {}
    }
    value
}

/// Sets a property in `data`, or takes it out where it is the default.
fn set(data: &mut MaterialData, property: &ShaderProperty, value: [f32; 4]) {
    let used = &value[..property.kind.values()];
    if used == &property.default[..property.kind.values()] {
        data.properties.remove(&property.name);
        return;
    }
    let written = match property.kind {
        PropertyKind::Toggle => PropertyValue::Toggle(value[0] > 0.5),
        PropertyKind::Float => PropertyValue::Number(value[0]),
        PropertyKind::Picture => return,
        _ => PropertyValue::Vector(used.to_vec()),
    };
    data.properties.insert(property.name.clone(), written);
}

fn number_field(ui: &mut Ui, parent: NodeId, value: f32) -> NodeId {
    let field = ui.text_field(parent, &number(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    field
}

/// A number typed, kept in its range; what it was when it does not read.
fn read(ui: &Ui, field: NodeId, was: f32, range: Option<(f32, f32)>) -> f32 {
    let typed = ui.text(field).trim().parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(was);
    range.map_or(typed, |(low, high)| typed.clamp(low, high))
}
