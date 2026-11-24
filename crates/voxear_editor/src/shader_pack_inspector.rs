//! A shader pack's page in the Inspector: what it is, whether the Scene
//! view wears it and how its last compile went, its passes, and its
//! options, which change the pack worn at once.

use voxear_engine::{
    Game,
    shader_packs::{self, Standing},
};
use voxear_render::{
    SunShadows,
    shader_packs::{OptionKind, PackManifest, PackShadows, PassWrites, TargetScale},
};
use voxear_ui::{NodeId, Style, Ui};

use crate::{asset_inspector::part, inspector::prop};

/// One option's control.
enum OptionControl {
    /// A slider and the field beside it that says its value and takes one
    /// typed.
    Number(NodeId, NodeId),
    Toggle(NodeId),
    Choice(NodeId),
    /// Red, green and blue in sRGB from 0 to 1, and a swatch of the colour.
    Color([NodeId; 3], NodeId),
}

/// Shows a colour option's value in its fields and swatch.
fn show_color(ui: &mut Ui, (fields, swatch): ([NodeId; 3], NodeId), value: f32) {
    let srgb = voxear_render::shader_packs::value_color(value);
    for (field, channel) in fields.into_iter().zip(srgb) {
        ui.set_text(field, &crate::inspector::number(channel));
    }
    let [r, g, b] = srgb.map(|c| (c * 255.0).round() as u8);
    ui.set_inline_style(swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
}

/// What was changed on the page.
#[derive(Clone, Debug, PartialEq)]
pub enum PackChange {
    /// Wear the pack in the Scene view, or take it off.
    Wear(String, bool),
    /// An option of the pack worn, by its name, and its value.
    Option(String, f32),
}

/// The page's controls, and what they show now.
pub struct ShaderPackFields {
    /// What names the pack, as `shader_packs::wear` takes it.
    reference: String,
    worn: NodeId,
    standing: NodeId,
    shown: Option<(bool, Option<Standing>)>,
    options: Vec<(String, OptionControl)>,
    values: Vec<f32>,
}

/// The rows of the pack whose manifest is `file`, named by `reference`.
pub fn rows(ui: &mut Ui, root: NodeId, reference: &str, file: &std::path::Path) -> Option<ShaderPackFields> {
    let rows = part(ui, root, "Shader Pack", "layers");
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let manifest = match PackManifest::parse(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            let row = prop(ui, rows, "Not read");
            let value = ui.add_label(row, &format!("{error:#}"));
            ui.add_class(value, "value");
            ui.add_class(value, "shader-error");
            return None;
        }
    };
    let row = prop(ui, rows, "Description");
    let value = ui.add_label(row, if manifest.description.is_empty() { "None" } else { &manifest.description });
    ui.add_class(value, "value");

    let row = prop(ui, rows, "Worn");
    tip(
        ui,
        row,
        "Draws the Scene view and the game under the pack, and draws it again whenever one of its files is saved",
    );
    let worn = ui.toggle(row, "", false);
    ui.add_class(worn, "checkbox");
    let row = prop(ui, rows, "Compiled");
    tip(ui, row, "How the pack's programs compiled: Slang's message, with the file and line, where they did not");
    let standing = ui.add_label(row, "Not worn");
    ui.add_class(standing, "value");

    let passes = part(ui, root, "Passes", "layers");
    for pass in &manifest.passes {
        let row = prop(ui, passes, &pass.name);
        let writes = match pass.writes {
            PassWrites::Color => "the colour".to_string(),
            PassWrites::Target(target) => {
                let target = &manifest.targets[target];
                let size = match target.scale {
                    TargetScale::Full => "",
                    TargetScale::Half => ", half size",
                    TargetScale::Quarter => ", quarter size",
                };
                let kept = if target.kept { ", kept" } else { "" };
                format!("{}{size}{kept}", target.name)
            }
        };
        let reads = pass.reads.names(&manifest);
        let reads = if reads.is_empty() { "nothing".to_string() } else { reads.join(", ") };
        let text = format!("{} · {} → {writes}", pass.point.title(), reads);
        let value = ui.add_label(row, &text);
        ui.add_class(value, "value");
        tip(ui, row, &format!("{}, at {}: reads {reads}, writes {writes}", pass.program, pass.point.key()));
    }

    let named = manifest.surfaces.named();
    if !named.is_empty() {
        let rows = part(ui, root, "Surfaces", "blocks");
        for (key, program) in named {
            let (label, tip_text) = match key {
                "blocks" => {
                    ("Blocks", "Lights every block's faces, and leaves where the pack has no program of their own")
                }
                "leaves" => ("Leaves", "Lights faces of leaves, and sheets of leaves and of vines"),
                "water" => ("Water", "Lights the sea, Pools, Streams and worlds of blocks' water"),
                "models" => ("Models", "Lights characters, props, creatures and anything else drawn from a mesh"),
                _ => ("Sky", "Draws the sky behind everything, and the clouds and far weather in front of it"),
            };
            let row = prop(ui, rows, label);
            let value = ui.add_label(row, program);
            ui.add_class(value, "value");
            tip(ui, row, tip_text);
        }
    }

    let shadows = &manifest.shadows;
    if *shadows != PackShadows::default() {
        let rows = part(ui, root, "Shadows", "sun");
        let mut row_of = |label: &str, value: String, about: &str| {
            let row = prop(ui, rows, label);
            let text = ui.add_label(row, &value);
            ui.add_class(text, "value");
            tip(ui, row, about);
        };
        if let Some(filter) = &shadows.filter {
            let about = "Reads the sun's shadow at every point, in place of the engine's soft shadow";
            row_of("Filter", filter.clone(), about);
        }
        if let Some(softness) = shadows.softness {
            let about = "How wide the sun is: the wider, the softer shadows grow away from what casts them";
            row_of("Sun Width", format!("{softness}°"), about);
        }
        if let Some(distance) = shadows.distance {
            row_of("Distance", format!("{distance} m"), "How far from the camera the sun's cascades reach");
        }
        if let Some([r, g, b]) = shadows.tint {
            row_of("Tint", format!("{r}, {g}, {b}"), "The colour shade is lifted by, as the sky's light fills it");
        }
        if let Some(count) = shadows.cascades {
            let about = "How many cascades share the distance: fewer draw less, each less finely";
            row_of("Cascades", count.to_string(), about);
        }
        if let Some(split) = shadows.split {
            let about = "How the cascades share the distance, from 0, alike, to 1, the nearest small and sharp";
            row_of("Split", split.to_string(), about);
        }
        if shadows.leaves {
            let about = "Leaves let the sun through, tinted their colour, as much as the filter says";
            row_of("Leaves", "Let light through".to_string(), about);
        }
        if let Some(maps) = shadows.maps {
            let name = match maps {
                SunShadows::Virtual => "Virtual",
                SunShadows::Cascades => "Cascades",
            };
            row_of("Maps", name.to_string(), "Virtual shadow maps, or cascades drawn every frame");
        }
    }

    let mut options = Vec::new();
    if !manifest.options.is_empty() {
        let rows = part(ui, root, "Options", "settings");
        for option in &manifest.options {
            let row = prop(ui, rows, &option.label);
            if !option.about.is_empty() {
                tip(ui, row, &option.about);
            }
            let control = match &option.kind {
                OptionKind::Number { low, high, default } => {
                    let holder = ui.panel(row, Style::row());
                    ui.add_class(holder, "attr-range");
                    let slider = ui.slider(holder, *low, *high, *default);
                    ui.add_class(slider, "attr-slider");
                    let field = ui.text_field(holder, &crate::inspector::number(*default));
                    ui.add_class(field, "value-input");
                    ui.add_class(field, "number-input");
                    ui.add_class(field, "attr-range-input");
                    OptionControl::Number(slider, field)
                }
                OptionKind::Toggle { default } => {
                    let toggle = ui.toggle(row, "", *default);
                    ui.add_class(toggle, "checkbox");
                    OptionControl::Toggle(toggle)
                }
                OptionKind::Choice { choices, default } => {
                    let labels: Vec<&str> = choices.iter().map(String::as_str).collect();
                    let dropdown = ui.dropdown(row, &labels, *default);
                    ui.add_class(dropdown, "value-choice");
                    OptionControl::Choice(dropdown)
                }
                OptionKind::Color { .. } => {
                    let (fields, swatch) = crate::inspector::color_fields(ui, row);
                    show_color(ui, (fields, swatch), option.default_value());
                    OptionControl::Color(fields, swatch)
                }
            };
            options.push((option.key.clone(), control));
        }
    }
    let values = manifest.options.iter().map(|option| option.default_value()).collect();
    Some(ShaderPackFields { reference: reference.to_string(), worn, standing, shown: None, options, values })
}

/// A tooltip on a row's label.
fn tip(ui: &mut Ui, row: NodeId, text: &str) {
    if let Some(&label) = ui.children(row).first() {
        ui.set_tooltip(label, text, None);
    }
}

impl ShaderPackFields {
    /// What was changed on the page since the last frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<PackChange> {
        if ui.changed(self.worn) {
            return Some(PackChange::Wear(self.reference.clone(), ui.is_checked(self.worn)));
        }
        for (at, (key, control)) in self.options.iter().enumerate() {
            let value = match control {
                OptionControl::Number(slider, _) if ui.changed(*slider) => ui.slider_value(*slider),
                OptionControl::Number(_, field) if ui.changed(*field) => match ui.text(*field).trim().parse() {
                    Ok(value) => value,
                    Err(_) => continue,
                },
                OptionControl::Toggle(toggle) if ui.changed(*toggle) => f32::from(u8::from(ui.is_checked(*toggle))),
                OptionControl::Choice(dropdown) if ui.changed(*dropdown) => ui.selected_option(*dropdown) as f32,
                OptionControl::Color(fields, _) if fields.iter().any(|field| ui.changed(*field)) => {
                    let mut srgb = voxear_render::shader_packs::value_color(self.values[at]);
                    for (channel, field) in srgb.iter_mut().zip(fields) {
                        if let Ok(typed) = ui.text(*field).trim().parse::<f32>() {
                            *channel = typed.clamp(0.0, 1.0);
                        }
                    }
                    voxear_render::shader_packs::color_value(srgb)
                }
                _ => continue,
            };
            self.values[at] = value;
            return Some(PackChange::Option(key.clone(), value));
        }
        None
    }

    /// Shows whether the pack is worn, how its compile went, and its
    /// options' values as worn.
    pub fn show(&mut self, ui: &mut Ui, game: &Game) {
        let world = &game.engine.world;
        let worn = shader_packs::worn(world) == Some(self.reference.as_str());
        let standing = worn.then(|| shader_packs::standing(world)).flatten();
        let now = (worn, standing);
        if self.shown.as_ref() == Some(&now) && !worn {
            return;
        }
        if self.shown.as_ref() != Some(&now) {
            ui.set_checked(self.worn, worn);
            let text = match &now.1 {
                None => "Not worn".to_string(),
                Some(Standing::Compiling) => "Compiling…".to_string(),
                Some(Standing::Drawn) => "Compiled".to_string(),
                Some(Standing::Failed(error)) => {
                    let lines: Vec<&str> = error.lines().filter(|line| !line.trim().is_empty()).take(8).collect();
                    lines.join("\n")
                }
            };
            ui.set_text(self.standing, &text);
            ui.set_class(self.standing, "shader-error", matches!(now.1, Some(Standing::Failed(_))));
            self.shown = Some(now);
        }
        if !worn {
            return;
        }
        // The options as the pack worn has them, as a call may have set them.
        for (option, value) in shader_packs::options(world) {
            let Some(at) = self.options.iter().position(|(key, _)| *key == option.key) else { continue };
            if (self.values[at] - value).abs() <= f32::EPSILON {
                continue;
            }
            self.values[at] = value;
            match self.options[at].1 {
                OptionControl::Number(slider, field) => {
                    ui.set_slider_value(slider, value);
                    ui.set_text(field, &crate::inspector::number(value));
                }
                OptionControl::Toggle(toggle) => ui.set_checked(toggle, value >= 0.5),
                OptionControl::Choice(dropdown) => ui.set_selected_option(dropdown, value as usize),
                OptionControl::Color(fields, swatch) => show_color(ui, (fields, swatch), value),
            }
        }
    }
}
