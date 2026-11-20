//! A look pack's look shader on its Inspector page: a menu of the project's
//! look shaders, and a row for each property of the one the pack names, its
//! value the pack's or the shader's own. A change is written into the
//! pack's `[shader]` as a step to undo (see `Editor::set_pack_setting`).

use std::path::Path;

use voxear_engine::{
    Game,
    look_packs::LookPackFile,
    shaders::{PropertyKind, ShaderProperty},
};
use voxear_ui::{NodeId, Style, Ui};

use crate::{asset_inspector::part, inspector::prop};

/// A property's row: what to read its value from.
enum Control {
    Toggle(NodeId),
    /// A field for each number it holds.
    Numbers(Vec<NodeId>),
}

/// The look shader part of a look pack's page.
pub struct LookShaderFields {
    menu: NodeId,
    /// Each choice of the menu after None: a look shader's project path.
    shaders: Vec<String>,
    properties: Vec<(String, Control)>,
}

/// A setting of the pack's `[shader]` to write: its path, and its value or
/// none to take it out.
pub type ShaderChange = (Vec<String>, Option<toml_edit::Value>);

/// The part, for the pack in `file`.
pub fn rows(ui: &mut Ui, root: NodeId, game: &Game, file: &Path) -> LookShaderFields {
    let rows = part(ui, root, "Look Shader", "material");
    let pack = std::fs::read_to_string(file).ok().and_then(|text| LookPackFile::parse(&text).ok()).unwrap_or_default();
    let looks: Vec<(String, Vec<ShaderProperty>)> = game
        .content
        .shaders()
        .shaders
        .iter()
        .filter_map(|shader| {
            let info = shader.info.as_ref().ok().filter(|info| info.look)?;
            Some((shader.path.clone(), info.properties.clone()))
        })
        .collect();
    let named = pack.shader.as_ref();
    let row = prop(ui, rows, "Shader");
    let mut labels = vec!["None"];
    labels.extend(looks.iter().map(|(path, _)| path.as_str()));
    let chosen =
        named.and_then(|named| looks.iter().position(|(path, _)| *path == named.shader)).map_or(0, |at| at + 1);
    let menu = ui.dropdown(row, &labels, chosen);
    ui.add_class(menu, "value-choice");
    if let Some(&label) = ui.children(row).first() {
        let tip = "A shader of the game's with a look function, which has the last word on the finished picture";
        ui.set_tooltip(label, tip, None);
    }
    let mut properties = Vec::new();
    let shown = named.and_then(|named| looks.iter().find(|(path, _)| *path == named.shader));
    for property in shown.map(|(_, properties)| properties.as_slice()).unwrap_or_default() {
        let given = named.and_then(|named| named.values.get(&property.name));
        let Some(control) = property_row(ui, rows, property, given) else { continue };
        properties.push((property.name.clone(), control));
    }
    LookShaderFields { menu, shaders: looks.into_iter().map(|(path, _)| path).collect(), properties }
}

/// A row for one of the shader's properties: its value as the pack gives
/// it, or the shader's own. A picture has none, as a look shader reads
/// none.
fn property_row(ui: &mut Ui, rows: NodeId, property: &ShaderProperty, given: Option<&toml::Value>) -> Option<Control> {
    let row = prop(ui, rows, &property.name);
    if let (Some(tooltip), Some(&label)) = (&property.tooltip, ui.children(row).first()) {
        ui.set_tooltip(label, tooltip, None);
    }
    let number = |value: &toml::Value| value.as_float().or_else(|| value.as_integer().map(|n| n as f64));
    let count = match property.kind {
        PropertyKind::Toggle => {
            let on = given.and_then(toml::Value::as_bool).unwrap_or(property.default[0] != 0.0);
            let toggle = ui.toggle(row, "", on);
            ui.add_class(toggle, "checkbox");
            return Some(Control::Toggle(toggle));
        }
        PropertyKind::Picture => return None,
        PropertyKind::Float => 1,
        PropertyKind::Float2 => 2,
        PropertyKind::Float3 | PropertyKind::Color { alpha: false } => 3,
        PropertyKind::Float4 | PropertyKind::Color { alpha: true } => 4,
    };
    let values: Vec<f32> = match given {
        Some(toml::Value::Array(parts)) => parts.iter().filter_map(number).map(|n| n as f32).collect(),
        Some(one) => number(one).map(|n| vec![n as f32; count]).unwrap_or_default(),
        None => property.default[..count].to_vec(),
    };
    let holder = ui.panel(row, Style::row());
    let fields = (0..count)
        .map(|at| {
            let field = ui.text_field(holder, &crate::inspector::number(values.get(at).copied().unwrap_or_default()));
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            field
        })
        .collect();
    Some(Control::Numbers(fields))
}

impl LookShaderFields {
    /// What was changed on the part since the last frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<ShaderChange> {
        if ui.changed(self.menu) {
            let chosen = ui.selected_option(self.menu).checked_sub(1).and_then(|at| self.shaders.get(at));
            let value = chosen.map(|path| toml_edit::Value::from(path.as_str()));
            return Some((vec!["shader".into(), "shader".into()], value));
        }
        for (name, control) in &self.properties {
            let path = vec!["shader".to_string(), name.clone()];
            match control {
                Control::Toggle(toggle) if ui.changed(*toggle) => {
                    return Some((path, Some(toml_edit::Value::from(ui.is_checked(*toggle)))));
                }
                Control::Numbers(fields) if fields.iter().any(|field| ui.changed(*field)) => {
                    let read: Option<Vec<f64>> =
                        fields.iter().map(|field| ui.text(*field).trim().parse().ok()).collect();
                    let numbers = read?;
                    let value = match numbers.as_slice() {
                        [one] => toml_edit::Value::from(*one),
                        many => toml_edit::Value::Array(many.iter().copied().collect()),
                    };
                    return Some((path, Some(value)));
                }
                _ => {}
            }
        }
        None
    }
}
