//! The Lighting page, in the rail's drawer, as Unity's Lighting window: the
//! lighting settings the active scene names, picked from none, the
//! built-in ones and the project's `.lighting` files, and below, a tab for
//! each group of their settings, each changed in place.
//!
//! Each setting shows what it is in effect, the look's value where the file
//! writes none, and a change writes only that setting into the file,
//! keeping its comments, so the open scene re-lights at once. Emptying a
//! number's field takes the setting out of the file, back to the look's.
//! Built-in lighting cannot be changed: a copy of it can.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use voxear_assets::{AssetKind, AssetRef};
use voxear_engine::{
    Game,
    lighting::LightingFile,
    scene_file::{PostTable, SkyFromLook, SkyModeData, sky_data},
};
use voxear_render::{LookName, PostProcess, Sky};
use voxear_ui::{Color, NodeId, Style, Ui};

/// What a setting is edited with.
#[derive(Clone, Copy)]
enum Kind {
    Toggle,
    /// One of these, each as the menu shows it and as the file writes it;
    /// an empty value takes the setting out of the file.
    Choice(&'static [(&'static str, &'static str)]),
    /// None, or one of the built-in looks (`LookName::ALL`).
    Look,
    Number {
        min: f32,
        max: f32,
        unit: &'static str,
    },
    /// An sRGB colour, picked from its swatch.
    Color,
}

/// One setting: where it is in a `.lighting` file, its name, its group, and
/// when it shows.
struct Info {
    path: &'static [&'static str],
    title: &'static str,
    group: &'static str,
    kind: Kind,
    /// What must hold for the setting to do anything, each a setting and
    /// its value as the file writes it (`true` for a toggle that is on).
    /// While any does not, the setting is hidden, so the page shows only
    /// what changes the picture.
    when: When,
    /// What it does, in plain words, shown when the pointer rests on its
    /// name.
    help: &'static str,
}

type When = &'static [(&'static [&'static str], &'static str)];

const ALWAYS: When = &[];
const GRADIENT_SKY: When = &[(&["sky", "mode"], "gradient")];
const ATMOSPHERE: When = &[(&["sky", "mode"], "atmosphere")];
const AUTO_EXPOSURE: When = &[(&["post", "exposure", "auto"], "true")];
const AGX: When = &[(&["post", "tonemap", "mapper"], "agx")];
const BLOOM: When = &[(&["post", "bloom", "enabled"], "true")];
const FOG: When = &[(&["post", "fog", "enabled"], "true")];
const SHAFTS: When = &[(&["post", "fog", "enabled"], "true"), (&["post", "fog", "volumetric"], "true")];
const BOUNCE: When = &[(&["post", "global_illumination", "enabled"], "true")];
const OCCLUSION: When = &[(&["post", "ambient_occlusion", "enabled"], "true")];
const FOCUS: When = &[(&["post", "depth_of_field", "enabled"], "true")];
const BLUR: When = &[(&["post", "motion_blur", "enabled"], "true")];

const fn number(min: f32, max: f32, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit }
}

const SKY_MODES: &[(&str, &str)] = &[("Gradient", "gradient"), ("Atmosphere", "atmosphere")];
const TONEMAPPERS: &[(&str, &str)] =
    &[("AgX", "agx"), ("ACES", "aces"), ("Neutral", "neutral"), ("Reinhard", "reinhard"), ("None", "none")];

#[rustfmt::skip]
const SETTINGS: &[Info] = &[
    Info { path: &["look"], title: "Look", group: "Sky", kind: Kind::Look, when: ALWAYS,
        help: "A finished lighting style to start from. Every other setting begins at the look's value until you change it." },
    Info { path: &["sky", "mode"], title: "Mode", group: "Sky", kind: Kind::Choice(SKY_MODES), when: ALWAYS,
        help: "Gradient paints the sky with three colours you pick. Atmosphere works the sky out from the sun, as real air does: blue by day, orange at sunset." },
    Info { path: &["sky", "zenith"], title: "Zenith", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The sky's colour straight overhead." },
    Info { path: &["sky", "horizon"], title: "Horizon", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The sky's colour at the horizon, where it meets the ground." },
    Info { path: &["sky", "ground"], title: "Ground", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The colour below the horizon, which also lights things from beneath." },
    Info { path: &["sky", "ambient"], title: "Ambient", group: "Sky", kind: number(0.0, 8.0, ""), when: ALWAYS,
        help: "How brightly the sky lights everything, shade included. Lower it for a darker, moodier scene." },
    Info { path: &["sky", "haze"], title: "Haze", group: "Sky", kind: number(0.0, 10.0, ""), when: ATMOSPHERE,
        help: "How dusty or misty the air is. More haze makes the distance fade and the sky glow round the sun." },
    Info { path: &["sky", "haze_tint"], title: "Haze tint", group: "Sky", kind: Kind::Color, when: ATMOSPHERE,
        help: "The colour of the haze: warm for dust, white for mist." },
    Info { path: &["sky", "haze_glow"], title: "Haze glow", group: "Sky", kind: number(0.0, 0.95, ""), when: ATMOSPHERE,
        help: "How tightly the haze glows round the sun. Higher makes a small bright halo; lower spreads it over the sky." },
    Info { path: &["sky", "distance_haze"], title: "Distance haze", group: "Sky", kind: number(0.0, 100.0, ""), when: ATMOSPHERE,
        help: "How quickly far-away things fade into the haze. Higher hides the distance sooner." },
    Info { path: &["sky", "sun_size"], title: "Sun size", group: "Sky", kind: number(0.05, 20.0, "°"), when: ALWAYS,
        help: "How big the sun's disc looks in the sky, in degrees." },
    Info { path: &["sky", "stars"], title: "Stars", group: "Sky", kind: number(0.0, 4.0, ""), when: ALWAYS,
        help: "How bright the stars are at night. 0 hides them." },
    Info { path: &["sun", "temperature"], title: "Temperature", group: "Sun", kind: number(1000.0, 20000.0, "K"), when: ALWAYS,
        help: "The sun's colour as a temperature, in kelvin: about 2000 K is a candle-orange sunset, 5500 K is white noon light, above 7000 K is cool and blue." },
    Info { path: &["sun", "tint"], title: "Tint", group: "Sun", kind: Kind::Color, when: ALWAYS,
        help: "A colour the sunlight is filtered through, on top of its temperature. White leaves it as it is." },
    Info { path: &["sun", "strength"], title: "Strength", group: "Sun", kind: number(0.0, 10.0, "×"), when: ALWAYS,
        help: "How bright the sun is, times its light's own intensity. 1 leaves it as set." },
    Info { path: &["sun", "softness"], title: "Softness", group: "Sun", kind: number(0.0, 20.0, "°"), when: ALWAYS,
        help: "How soft the sun's shadows are, as its width in degrees. 0 gives hard, crisp shadows; higher blurs their edges, as through thin cloud." },
    Info { path: &["sun", "shadow_tint"], title: "Shadow tint", group: "Sun", kind: Kind::Color, when: ALWAYS,
        help: "The colour shadows lean towards. Black leaves them neutral; a warm brown or cool blue gives shade a mood." },
    Info { path: &["post", "exposure", "auto"], title: "Auto exposure", group: "Exposure", kind: Kind::Toggle, when: ALWAYS,
        help: "Lets the camera adjust its brightness to the scene, like an eye: brighter in dark places, darker in bright ones. Off keeps it fixed." },
    Info { path: &["post", "exposure", "compensation"], title: "Compensation", group: "Exposure", kind: number(-8.0, 8.0, "EV"), when: ALWAYS,
        help: "Makes the whole picture brighter (positive) or darker (negative), in stops. Each stop doubles or halves the brightness." },
    Info { path: &["post", "exposure", "min_ev"], title: "Min exposure", group: "Exposure", kind: number(-16.0, 16.0, "EV"), when: AUTO_EXPOSURE,
        help: "The most auto exposure may darken the picture, in stops, for very bright scenes." },
    Info { path: &["post", "exposure", "max_ev"], title: "Max exposure", group: "Exposure", kind: number(-16.0, 16.0, "EV"), when: AUTO_EXPOSURE,
        help: "The most auto exposure may brighten the picture, in stops, for dark scenes. Raise it for rooms lit only by their windows." },
    Info { path: &["post", "exposure", "speed_up"], title: "Adapt to light", group: "Exposure", kind: number(0.0, 20.0, "EV/s"), when: AUTO_EXPOSURE,
        help: "How fast the eye adjusts when you walk into somewhere brighter, in stops a second." },
    Info { path: &["post", "exposure", "speed_down"], title: "Adapt to dark", group: "Exposure", kind: number(0.0, 20.0, "EV/s"), when: AUTO_EXPOSURE,
        help: "How fast the eye adjusts when you walk into somewhere darker, in stops a second. Eyes adjust to the dark slowly." },
    Info { path: &["post", "tonemap", "mapper"], title: "Tonemapper", group: "Colour", kind: Kind::Choice(TONEMAPPERS), when: ALWAYS,
        help: "How bright light is squeezed into what a screen can show. AgX keeps colours natural in bright light and is the best default; ACES is punchier, like film." },
    Info { path: &["post", "tonemap", "contrast"], title: "Contrast", group: "Colour", kind: number(0.0, 3.0, ""), when: AGX,
        help: "The difference between light and dark. Higher is punchier, lower is flatter. 1 is AgX's own." },
    Info { path: &["post", "tonemap", "saturation"], title: "Saturation", group: "Colour", kind: number(0.0, 3.0, ""), when: AGX,
        help: "How strong colours are. Higher is more vivid, lower towards grey. 1 is AgX's own." },
    Info { path: &["post", "grading", "temperature"], title: "White balance", group: "Colour", kind: number(-1.0, 1.0, ""), when: ALWAYS,
        help: "Shifts the whole picture warmer (orange) or cooler (blue), as a camera's white balance does." },
    Info { path: &["post", "grading", "tint"], title: "Green-magenta", group: "Colour", kind: number(-1.0, 1.0, ""), when: ALWAYS,
        help: "Shifts the whole picture towards green (negative) or magenta (positive)." },
    Info { path: &["post", "grading", "contrast"], title: "Grade contrast", group: "Colour", kind: number(0.0, 3.0, ""), when: ALWAYS,
        help: "Contrast applied in the final colour grade, after the tonemapper." },
    Info { path: &["post", "grading", "saturation"], title: "Grade saturation", group: "Colour", kind: number(0.0, 3.0, ""), when: ALWAYS,
        help: "Colour strength applied in the final colour grade, after the tonemapper." },
    Info { path: &["post", "vignette", "intensity"], title: "Vignette", group: "Colour", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "Darkens the corners of the picture, drawing the eye to the middle, as old lenses do." },
    Info { path: &["post", "chromatic_aberration"], title: "Colour fringes", group: "Colour", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "Splits colours slightly at the picture's edges, as a cheap lens does. Small amounts look filmic." },
    Info { path: &["post", "bloom", "enabled"], title: "Bloom", group: "Bloom", kind: Kind::Toggle, when: ALWAYS,
        help: "Makes bright things glow softly into what is around them: lamps, the sun, shiny highlights." },
    Info { path: &["post", "bloom", "intensity"], title: "Intensity", group: "Bloom", kind: number(0.0, 1.0, ""), when: BLOOM,
        help: "How much bright things glow." },
    Info { path: &["post", "bloom", "scatter"], title: "Scatter", group: "Bloom", kind: number(0.0, 1.0, ""), when: BLOOM,
        help: "How far the glow spreads. Low keeps it tight round the light; high spreads a wide haze." },
    Info { path: &["post", "bloom", "tint"], title: "Tint", group: "Bloom", kind: Kind::Color, when: BLOOM,
        help: "The colour of the glow. White keeps each light's own colour." },
    Info { path: &["post", "fog", "enabled"], title: "Fog", group: "Fog", kind: Kind::Toggle, when: ALWAYS,
        help: "Fills the air with fog that thickens with distance and towards the ground." },
    Info { path: &["post", "fog", "density"], title: "Density", group: "Fog", kind: number(0.0, 1.0, ""), when: FOG,
        help: "How thick the fog is. Small numbers go a long way: 0.01 is a light mist." },
    Info { path: &["post", "fog", "height_falloff"], title: "Height falloff", group: "Fog", kind: number(0.0, 2.0, ""), when: FOG,
        help: "How quickly the fog thins as it rises. Higher keeps it low to the ground." },
    Info { path: &["post", "fog", "base_height"], title: "Base height", group: "Fog", kind: number(-1000.0, 1000.0, "m"), when: FOG,
        help: "The height, in metres, where the fog is thickest." },
    Info { path: &["post", "fog", "sun_glow"], title: "Sun glow", group: "Fog", kind: number(0.0, 1.0, ""), when: FOG,
        help: "How brightly the fog glows when you look towards the sun." },
    Info { path: &["post", "fog", "volumetric"], title: "Light shafts", group: "Fog", kind: Kind::Toggle, when: FOG,
        help: "Lights the fog near the camera, so sunbeams and lamp glows show in the air, cut by shadows. It costs a little more." },
    Info { path: &["post", "fog", "volumetric_distance"], title: "Shaft distance", group: "Fog", kind: number(1.0, 512.0, "m"), when: SHAFTS,
        help: "How far from the camera light shafts reach, in metres. Beyond it the fog is plain." },
    Info { path: &["post", "fog", "anisotropy"], title: "Forward glow", group: "Fog", kind: number(-0.95, 0.95, ""), when: SHAFTS,
        help: "How much the fog glows looking towards a light. Higher gives brighter shafts towards the sun; 0 glows evenly every way." },
    Info { path: &["post", "global_illumination", "enabled"], title: "Bounced light", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Light bounces off surfaces onto others, as in real life: a sunlit floor lights the ceiling, a red wall tints what is beside it. It costs more, and looks much more natural." },
    Info { path: &["post", "global_illumination", "strength"], title: "Bounce strength", group: "Shading", kind: number(0.0, 4.0, ""), when: BOUNCE,
        help: "How bright the bounced light is. 1 is as physics has it." },
    Info { path: &["post", "global_illumination", "probe_spacing"], title: "Probe spacing", group: "Shading", kind: number(0.1, 16.0, "m"), when: BOUNCE,
        help: "How far apart, in metres, the points that measure bounced light are. Smaller catches more detail and costs more." },
    Info { path: &["post", "ambient_occlusion", "enabled"], title: "Ambient occlusion", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Darkens corners, cracks and where things meet, where less light reaches. It makes objects look grounded." },
    Info { path: &["post", "ambient_occlusion", "radius"], title: "Occlusion radius", group: "Shading", kind: number(0.0, 8.0, "m"), when: OCCLUSION,
        help: "How far, in metres, each point looks for nearby things that shade it. Larger darkens wider areas." },
    Info { path: &["post", "ambient_occlusion", "strength"], title: "Occlusion strength", group: "Shading", kind: number(0.0, 4.0, ""), when: OCCLUSION,
        help: "How dark the corners get." },
    Info { path: &["post", "contact_shadows", "enabled"], title: "Contact shadows", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Adds small, sharp shadows where things touch the ground or each other, that the sun's shadows are too coarse to show." },
    Info { path: &["post", "reflections", "enabled"], title: "Reflections", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Shiny surfaces reflect what is round them: polished floors, water, metal. With ray tracing they reflect everything, even what is off screen." },
    Info { path: &["post", "depth_of_field", "enabled"], title: "Depth of field", group: "Lens", kind: Kind::Toggle, when: ALWAYS,
        help: "Blurs what is nearer or farther than the focus distance, as a camera lens does." },
    Info { path: &["post", "depth_of_field", "focus_distance"], title: "Focus distance", group: "Lens", kind: number(0.0, 10000.0, "m"), when: FOCUS,
        help: "How far from the camera, in metres, things are in sharp focus." },
    Info { path: &["post", "depth_of_field", "aperture"], title: "Aperture", group: "Lens", kind: number(0.5, 64.0, "f"), when: FOCUS,
        help: "How much blur there is outside the focus: smaller f-numbers blur more, as on a real lens." },
    Info { path: &["post", "motion_blur", "enabled"], title: "Motion blur", group: "Lens", kind: Kind::Toggle, when: ALWAYS,
        help: "Blurs things as they move fast, or as the camera turns, as a film camera does." },
    Info { path: &["post", "motion_blur", "intensity"], title: "Blur intensity", group: "Lens", kind: number(0.0, 1.0, ""), when: BLUR,
        help: "How much moving things blur." },
];

#[derive(Clone, Copy)]
enum Control {
    Toggle(NodeId),
    Choice(NodeId),
    Number(NodeId),
    Color(NodeId),
}

struct Row {
    info: &'static Info,
    row: NodeId,
    control: Control,
}

/// What a change on the page asks of the editor.
#[derive(Clone, Debug)]
pub enum LightingAction {
    /// The active scene names other lighting settings, or none.
    SetScene(Option<String>),
    /// A setting of the lighting shown is written into its file, or taken
    /// out of it with `None`.
    Set { file: PathBuf, path: &'static [&'static str], value: Option<toml_edit::Value> },
    /// New lighting settings are made, a copy of those shown, and named in
    /// the active scene.
    Copy,
}

/// Two changes are the same when they write the same text: the file's values
/// keep their own formatting, so they compare as written.
impl PartialEq for LightingAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SetScene(a), Self::SetScene(b)) => a == b,
            (Self::Set { file, path, value }, Self::Set { file: f, path: p, value: v }) => {
                let text = |value: &Option<toml_edit::Value>| value.as_ref().map(|v| v.to_string());
                file == f && path == p && text(value) == text(v)
            }
            (Self::Copy, Self::Copy) => true,
            _ => false,
        }
    }
}

pub struct LightingPanel {
    /// The scene's lighting menu, what each of its entries names, a note
    /// under it, and the button that makes a copy.
    scene: NodeId,
    choices: Vec<Option<String>>,
    note: NodeId,
    copy: NodeId,
    fields: LightingFields,
    /// The lighting shown, as the scene names it.
    shown: Option<String>,
}

impl LightingPanel {
    /// Builds the scene's menu into `top`, a tab for each group into
    /// `tab_bar` and every setting into `view`.
    pub fn new(ui: &mut Ui, top: NodeId, tab_bar: NodeId, view: NodeId) -> Self {
        let row = ui.panel(top, Style::default());
        ui.add_class(row, "prop");
        let label = ui.add_label(row, "Lighting");
        ui.add_class(label, "prop-label");
        let scene = ui.dropdown(row, &["None"], 0);
        ui.add_class(scene, "value-choice");
        let note = ui.add_label(top, "");
        ui.add_class(note, "lighting-note");
        let copy = ui.button(top, "Make a Copy");
        ui.add_class(copy, "lighting-copy");
        let fields = LightingFields::new(ui, tab_bar, view);
        Self { scene, choices: vec![None], note, copy, fields, shown: None }
    }

    /// Shows the active scene's lighting as it now is: the menu's choices,
    /// and every setting's value, which can be changed only in a file of
    /// the project's.
    pub fn refresh(&mut self, ui: &mut Ui, game: &mut Game) {
        let current = game.levels.scene_lighting().map(str::to_string);
        let mut labels = vec!["None".to_string()];
        let mut choices = vec![None];
        for (_, key, title) in LookName::ALL {
            labels.push(title.to_string());
            choices.push(Some(format!("builtin:{key}")));
        }
        for (label, pick) in crate::fields::assets_of(game, Some(AssetKind::Lighting)) {
            if let crate::fields::Pick::Asset(Some(path)) = pick {
                labels.push(label);
                choices.push(Some(path));
            }
        }
        let same = |choice: &Option<String>| match (choice, &current) {
            (Some(choice), Some(current)) => {
                choice == current || game.content.parse(choice).ok() == game.content.parse(current).ok()
            }
            (None, None) => true,
            _ => false,
        };
        let index = match choices.iter().position(same) {
            Some(index) => index,
            None => {
                labels.push(current.clone().unwrap_or_default());
                choices.push(current.clone());
                choices.len() - 1
            }
        };
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        ui.set_options(self.scene, &labels);
        ui.set_selected_option(self.scene, index);
        self.choices = choices;

        self.shown = current.clone();
        let file = current.as_deref().and_then(|reference| project_file(game, reference));
        let read = current.as_deref().map_or_else(|| Ok(LightingFile::default()), |reference| read(game, reference));
        let note = match (&current, &file, &read) {
            (_, _, Err(error)) => format!("{error:#}"),
            (None, ..) => {
                "The scene sets its own sky and post-processing. Make lighting settings to share them.".into()
            }
            (Some(_), None, _) => "Built-in lighting. Make a copy to change it.".into(),
            (Some(_), Some(_), _) => String::new(),
        };
        ui.set_visible(self.note, !note.is_empty());
        ui.set_text(self.note, &note);
        ui.set_visible(self.copy, file.is_none());
        ui.set_text(self.copy, if current.is_none() { "New Lighting Settings" } else { "Make a Copy" });
        let values = read.and_then(|file| effective(&file)).unwrap_or_default();
        self.fields.show(ui, file, values);
    }

    /// What was changed on the page this frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<LightingAction> {
        if ui.changed(self.scene) {
            let chosen = self.choices.get(ui.selected_option(self.scene)).cloned().flatten();
            return Some(LightingAction::SetScene(chosen));
        }
        if ui.clicked(self.copy) {
            return Some(LightingAction::Copy);
        }
        self.fields.update(ui)
    }

    /// The colour setting whose swatch was clicked this frame.
    pub fn color_clicked(&self, ui: &Ui) -> Option<ColorClicked> {
        self.fields.color_clicked(ui)
    }

    /// The setting a page shows, by its title, for tests and the palette.
    pub fn row_of(&self, title: &str) -> Option<NodeId> {
        self.fields.row_of(title)
    }

    /// The lighting settings shown, as the scene names them.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }
}

/// A colour setting's swatch clicked: the file and setting it is, the
/// swatch, and its colour.
pub struct ColorClicked {
    pub file: PathBuf,
    pub path: &'static [&'static str],
    pub swatch: NodeId,
    pub srgb: [f32; 3],
}

/// Every setting of lighting settings, a tab for each group, as the Lighting
/// page and a `.lighting` asset's Inspector show them; changed in place
/// when they are a file of the project's.
pub struct LightingFields {
    tabs: Vec<(&'static str, NodeId)>,
    headings: Vec<(&'static str, NodeId)>,
    group: &'static str,
    rows: Vec<Row>,
    /// The file shown, when it is the project's to change.
    file: Option<PathBuf>,
    /// Every setting as it is in effect.
    values: toml::Table,
}

impl LightingFields {
    /// A tab for each group into `tab_bar` and every setting into `view`.
    pub fn new(ui: &mut Ui, tab_bar: NodeId, view: NodeId) -> Self {
        let mut tabs: Vec<(&'static str, NodeId)> = Vec::new();
        let body = ui.panel(view, Style::column());
        ui.add_class(body, "settings-body");
        let mut headings: Vec<(&'static str, NodeId)> = Vec::new();
        let mut rows = Vec::new();
        for info in SETTINGS {
            if !tabs.iter().any(|(group, _)| *group == info.group) {
                let tab = ui.button(tab_bar, info.group);
                ui.add_class(tab, "settings-tab");
                ui.set_tooltip(tab, group_help(info.group), None);
                tabs.push((info.group, tab));
                let heading = ui.add_label(body, info.group);
                ui.add_class(heading, "settings-group");
                headings.push((info.group, heading));
            }
            let row = ui.panel(body, Style::default());
            ui.add_class(row, "prop");
            ui.add_class(row, "setting");
            let label = ui.add_label(row, info.title);
            ui.add_class(label, "prop-label");
            ui.add_class(label, "scrub");
            ui.set_tooltip(label, info.help, None);
            let control = control(ui, row, info.kind);
            rows.push(Row { info, row, control });
        }
        let group = SETTINGS[0].group;
        let mut fields = Self { tabs, headings, group, rows, file: None, values: toml::Table::new() };
        fields.show_group(ui);
        fields
    }

    /// Shows lighting settings' values, in effect, changeable when `file` is
    /// the project's file of them.
    pub fn show(&mut self, ui: &mut Ui, file: Option<PathBuf>, values: toml::Table) {
        self.file = file;
        self.values = values;
        let editable = self.file.is_some();
        for row in &self.rows {
            let node = match row.control {
                Control::Toggle(node) | Control::Choice(node) | Control::Number(node) | Control::Color(node) => node,
            };
            ui.set_enabled(node, editable);
            show(ui, row, find(&self.values, row.info.path));
        }
        self.show_group(ui);
    }

    /// Shows the project's `.lighting` file at `file`.
    pub fn show_file(&mut self, ui: &mut Ui, file: &Path) {
        let read = std::fs::read_to_string(file)
            .with_context(|| format!("reading {}", file.display()))
            .and_then(|text| toml::from_str::<LightingFile>(&text).with_context(|| format!("reading {}", file.display())));
        match read.and_then(|lighting| effective(&lighting)) {
            Ok(values) => self.show(ui, Some(file.to_path_buf()), values),
            Err(error) => {
                tracing::error!("{error:#}");
                self.show(ui, None, toml::Table::new());
            }
        }
    }

    /// A setting changed this frame, as a change to make to the file.
    pub fn update(&mut self, ui: &mut Ui) -> Option<LightingAction> {
        if let Some(&(group, _)) = self.tabs.iter().find(|(_, tab)| ui.clicked(*tab)) {
            self.group = group;
            self.show_group(ui);
        }
        let file = self.file.clone()?;
        self.rows.iter().find_map(|row| {
            let value = read_control(ui, row)?;
            Some(LightingAction::Set { file: file.clone(), path: row.info.path, value })
        })
    }

    /// The colour setting whose swatch was clicked this frame.
    pub fn color_clicked(&self, ui: &Ui) -> Option<ColorClicked> {
        let file = self.file.clone()?;
        self.rows.iter().find_map(|row| match row.control {
            Control::Color(swatch) if ui.clicked(swatch) => {
                let srgb = find(&self.values, row.info.path).and_then(rgb).unwrap_or([1.0; 3]);
                Some(ColorClicked { file: file.clone(), path: row.info.path, swatch, srgb })
            }
            _ => None,
        })
    }

    pub fn row_of(&self, title: &str) -> Option<NodeId> {
        self.rows.iter().find(|row| row.info.title == title).map(|row| row.row)
    }

    /// The tab's group of settings, each only while it does anything.
    fn show_group(&mut self, ui: &mut Ui) {
        for (group, tab) in &self.tabs {
            ui.set_checked(*tab, *group == self.group);
        }
        for (group, heading) in &self.headings {
            ui.set_visible(*heading, *group == self.group);
        }
        for row in &self.rows {
            ui.set_visible(row.row, row.info.group == self.group && applies(&self.values, row.info.when));
        }
    }
}

/// What a group of settings is for, shown when the pointer rests on its tab.
fn group_help(group: &str) -> &'static str {
    match group {
        "Sky" => "The sky, and the light it gives everything from all round.",
        "Sun" => "How the sun looks: its colour, brightness and shadows. Where it shines from is the sun Obj's rotation.",
        "Exposure" => "How bright the picture is, and how the camera adjusts to light and dark, as an eye does.",
        "Colour" => "The colour grade: contrast, saturation, warmth and lens effects over the finished picture.",
        "Bloom" => "The soft glow round bright lights and highlights.",
        "Fog" => "Mist and dust in the air, and the light shafts that show in it.",
        "Shading" => "How light settles in the scene: bounced light, shade in corners, small shadows and reflections.",
        "Lens" => "Camera effects: focus blur and motion blur.",
        _ => "",
    }
}

/// Whether every condition holds in `values`, as a file writes them.
fn applies(values: &toml::Table, when: When) -> bool {
    when.iter().all(|(path, wanted)| match find(values, path) {
        Some(toml::Value::Boolean(on)) => on.to_string() == *wanted,
        Some(toml::Value::String(text)) => text == wanted,
        _ => false,
    })
}

/// The change to make for a colour picked for a setting.
pub fn set_color(file: PathBuf, path: &'static [&'static str], srgb: [f32; 3]) -> LightingAction {
    LightingAction::Set { file, path, value: Some(color_value(srgb)) }
}

/// The project's file of lighting settings a scene names, when it is one.
fn project_file(game: &Game, reference: &str) -> Option<PathBuf> {
    let AssetRef::Asset { guid, .. } = game.content.parse(reference).ok()? else { return None };
    let entry = game.content.db().get(guid)?;
    Some(game.project.root.join(&entry.path))
}

/// Lighting settings as written: a built-in's look, or a project file's text.
fn read(game: &Game, reference: &str) -> Result<LightingFile> {
    match game.content.parse(reference)? {
        AssetRef::Builtin(name) => LightingFile::builtin(&name),
        AssetRef::Asset { .. } => {
            let file = project_file(game, reference).with_context(|| format!("no lighting settings {reference}"))?;
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            toml::from_str(&text).with_context(|| format!("reading {reference}"))
        }
    }
}

/// Every setting of `file` as it is in effect, laid out as a `.lighting`
/// file writes them: the look's where the file writes none.
fn effective(file: &LightingFile) -> Result<toml::Table> {
    let lighting = file.resolve()?;
    let mut table = toml::Table::new();
    table.insert("look".into(), toml::Value::String(file.look.clone().unwrap_or_default()));
    let sky = sky_data(&lighting.sky);
    let color = |c: [f32; 3]| toml::Value::Array(c.iter().map(|&v| toml::Value::Float(f64::from(v))).collect());
    let mut written: toml::Table = toml::Table::try_from(sky)?;
    // A sky file leaves out settings at their defaults; the page shows every one.
    for (key, value) in [
        ("stars", toml::Value::Float(f64::from(sky.stars))),
        ("haze", toml::Value::Float(f64::from(sky.haze))),
        ("haze_tint", color(sky.haze_tint)),
        ("haze_glow", toml::Value::Float(f64::from(sky.haze_glow))),
        ("distance_haze", toml::Value::Float(f64::from(sky.distance_haze))),
        ("sun_size", toml::Value::Float(f64::from(sky.sun_size))),
        (
            "mode",
            toml::Value::String(if sky.mode == SkyModeData::Atmosphere { "atmosphere" } else { "gradient" }.into()),
        ),
    ] {
        written.insert(key.into(), value);
    }
    table.insert("sky".into(), toml::Value::Table(written));
    let own = file.sun.unwrap_or_default();
    let mut sun = toml::Table::new();
    if let Some(kelvin) = own.temperature {
        sun.insert("temperature".into(), toml::Value::Float(f64::from(kelvin)));
    }
    sun.insert("tint".into(), color(own.tint.unwrap_or([1.0; 3])));
    sun.insert("strength".into(), toml::Value::Float(f64::from(lighting.sun.strength)));
    sun.insert("softness".into(), toml::Value::Float(f64::from(lighting.sun.softness)));
    sun.insert("shadow_tint".into(), color(voxear_engine::srgb(lighting.sun.shadow_tint.to_array())));
    table.insert("sun".into(), toml::Value::Table(sun));
    let post = PostProcess::from_toml(&lighting.post)?;
    table.insert("post".into(), toml::Value::Table(toml::Table::try_from(post)?));
    Ok(table)
}

/// Rounds every number in `table` to four decimals, so a file written from
/// settings held in single precision reads `0.506`, not `0.5059999823570251`.
fn tidy(table: &mut toml::Table) {
    fn value(v: &mut toml::Value) {
        match v {
            toml::Value::Float(f) => *f = (*f * 10_000.0).round() / 10_000.0,
            toml::Value::Array(items) => items.iter_mut().for_each(value),
            toml::Value::Table(inner) => tidy(inner),
            _ => {}
        }
    }
    table.iter_mut().for_each(|(_, v)| value(v));
}

/// The value at `path` in `table`.
fn find<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (last, tables) = path.split_last()?;
    let mut at = table;
    for key in tables {
        at = at.get(*key)?.as_table()?;
    }
    at.get(*last)
}

fn rgb(value: &toml::Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    let channel = |i: usize| array.get(i).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)));
    Some([channel(0)? as f32, channel(1)? as f32, channel(2)? as f32])
}

fn color_value(srgb: [f32; 3]) -> toml_edit::Value {
    let rounded = srgb.map(|c| f64::from((c.clamp(0.0, 1.0) * 1000.0).round() / 1000.0));
    toml_edit::Value::Array(rounded.into_iter().collect())
}

fn control(ui: &mut Ui, row: NodeId, kind: Kind) -> Control {
    match kind {
        Kind::Toggle => {
            let toggle = ui.toggle(row, "", false);
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        Kind::Choice(options) => {
            let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
            let dropdown = ui.dropdown(row, &labels, 0);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        Kind::Look => {
            let labels: Vec<&str> = std::iter::once("None").chain(LookName::ALL.iter().map(|(.., title)| *title)).collect();
            let dropdown = ui.dropdown(row, &labels, 0);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        Kind::Number { unit, .. } => {
            let holder = ui.panel(row, Style::row());
            ui.add_class(holder, "prop-value");
            let field = ui.text_field(holder, "");
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            let unit = ui.add_label(holder, unit);
            ui.add_class(unit, "setting-unit");
            Control::Number(field)
        }
        Kind::Color => {
            let swatch = ui.panel(row, Style::default());
            ui.add_class(swatch, "swatch");
            Control::Color(swatch)
        }
    }
}

/// A change made in a row's control this frame: the value to write, or
/// `None` inside to take the setting out of the file.
fn read_control(ui: &Ui, row: &Row) -> Option<Option<toml_edit::Value>> {
    match (row.control, row.info.kind) {
        (Control::Toggle(toggle), _) if ui.changed(toggle) => Some(Some(ui.is_checked(toggle).into())),
        (Control::Choice(dropdown), Kind::Choice(options)) if ui.changed(dropdown) => {
            let (_, value) = options.get(ui.selected_option(dropdown))?;
            Some((!value.is_empty()).then(|| (*value).into()))
        }
        // None, the first, takes the look out of the file.
        (Control::Choice(dropdown), Kind::Look) if ui.changed(dropdown) => match ui.selected_option(dropdown) {
            0 => Some(None),
            index => LookName::ALL.get(index - 1).map(|(_, key, _)| Some((*key).into())),
        },
        (Control::Number(field), Kind::Number { min, max, .. }) if ui.changed(field) => {
            let text = ui.text(field);
            let text = text.trim();
            if text.is_empty() {
                return Some(None);
            }
            let number = text.parse::<f64>().ok()?.clamp(f64::from(min), f64::from(max));
            Some(Some(((number * 10_000.0).round() / 10_000.0).into()))
        }
        _ => None,
    }
}

/// Shows a setting's value in its control, except in a field being typed in.
fn show(ui: &mut Ui, row: &Row, value: Option<&toml::Value>) {
    match (row.control, row.info.kind) {
        (Control::Toggle(toggle), _) => {
            let on = value.and_then(toml::Value::as_bool).unwrap_or(false);
            if ui.is_checked(toggle) != on {
                ui.set_checked(toggle, on);
            }
        }
        (Control::Choice(dropdown), Kind::Choice(options)) => {
            let written = value.and_then(toml::Value::as_str).unwrap_or("");
            let index = options.iter().position(|(_, v)| *v == written).unwrap_or(0);
            if ui.selected_option(dropdown) != index {
                ui.set_selected_option(dropdown, index);
            }
        }
        (Control::Choice(dropdown), Kind::Look) => {
            let written = value.and_then(toml::Value::as_str).unwrap_or("");
            let index = LookName::ALL.iter().position(|(_, key, _)| *key == written).map_or(0, |at| at + 1);
            if ui.selected_option(dropdown) != index {
                ui.set_selected_option(dropdown, index);
            }
        }
        (Control::Number(field), _) => {
            let number = value.and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)));
            let text = number.map_or_else(String::new, format_number);
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
        (Control::Color(swatch), _) => {
            let [r, g, b] = value.and_then(rgb).unwrap_or([0.5; 3]).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
            let color = Color::hex(r << 16 | g << 8 | b);
            if ui.style(swatch).and_then(|s| s.background) != Some(color) {
                ui.set_inline_style(swatch, &format!("background-color: {}", color.css()));
            }
        }
        _ => {}
    }
}

/// A number as short as it can be written: up to four decimals.
fn format_number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Writes one setting into a `.lighting` file, or takes it out, keeping
/// everything else in the file as written, comments and all.
pub fn write_setting(file: &Path, path: &[&str], value: Option<toml_edit::Value>) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let mut document: toml_edit::DocumentMut = text.parse().with_context(|| format!("reading {}", file.display()))?;
    let (last, tables) = path.split_last().context("no setting")?;
    let mut table = document.as_table_mut();
    for key in tables {
        let entry = table.entry(key).or_insert_with(|| {
            let mut fresh = toml_edit::Table::new();
            fresh.set_implicit(true);
            toml_edit::Item::Table(fresh)
        });
        table = entry.as_table_mut().with_context(|| format!("`{key}` in {} is not a table", file.display()))?;
    }
    match value {
        Some(value) => drop(table.insert(last, toml_edit::Item::Value(value))),
        None => drop(table.remove(last)),
    }
    std::fs::write(file, document.to_string()).with_context(|| format!("writing {}", file.display()))
}

/// Text for a copy of lighting settings: a built-in's look, or the file's
/// own text as it is; or with none, the scene's own sky and
/// post-processing, so they carry over into the new lighting.
pub fn copy_text(game: &Game, reference: Option<&str>) -> Result<String> {
    let header = "# Voxear lighting settings. A scene names them, lighting = \"<this file>\",\n\
                  # and scripts apply them with Lighting.Apply.\n";
    let Some(reference) = reference else {
        let world = &game.engine.world;
        let mut file = LightingFile { format: voxear_engine::lighting::LIGHTING_FORMAT, ..LightingFile::default() };
        let mut post = world.resource::<PostTable>().map(|table| table.0.clone()).unwrap_or_default();
        // A look picked in [post] is the lighting's look, sky and sun and all.
        file.look = post.remove("look").and_then(|look| look.as_str().map(str::to_string));
        file.post = (!post.is_empty()).then_some(post);
        if world.resource::<SkyFromLook>().is_none() {
            let sky = world.resource::<Sky>().copied().unwrap_or_default();
            file.sky = Some(toml::Table::try_from(sky_data(&sky))?);
        }
        let mut table = toml::Table::try_from(&file)?;
        tidy(&mut table);
        return Ok(format!("{header}{}", toml::to_string(&table)?));
    };
    match game.content.parse(reference)? {
        AssetRef::Builtin(name) => Ok(format!("{header}format = 1\nlook = \"{name}\"\n")),
        AssetRef::Asset { .. } => {
            let file = project_file(game, reference).with_context(|| format!("no lighting settings {reference}"))?;
            Ok(std::fs::read_to_string(&file)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_written_into_its_table_keeping_the_rest_and_taken_out_again() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Dusk.lighting");
        std::fs::write(&file, "# Dusk\nformat = 1\nlook = \"golden_hour\"\n\n[sky]\nhaze = 2.0\n").unwrap();
        write_setting(&file, &["post", "bloom", "intensity"], Some(0.25.into())).unwrap();
        write_setting(&file, &["sky", "haze_glow"], Some(0.9.into())).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# Dusk\n"), "comments stay: {text}");
        let read: LightingFile = toml::from_str(&text).unwrap();
        assert_eq!(read.post.as_ref().unwrap()["bloom"]["intensity"].as_float(), Some(0.25));
        assert_eq!(read.sky.as_ref().unwrap()["haze"].as_float(), Some(2.0));
        assert_eq!(read.sky.as_ref().unwrap()["haze_glow"].as_float(), Some(0.9));
        write_setting(&file, &["sky", "haze"], None).unwrap();
        let read: LightingFile = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(read.sky.unwrap().get("haze").is_none(), "back to the look's");
    }

    #[test]
    fn every_setting_and_group_says_what_it_does() {
        for info in SETTINGS {
            assert!(info.help.len() > 20, "{} explains itself", info.title);
            assert!(!group_help(info.group).is_empty(), "the {} tab explains itself", info.group);
        }
    }

    #[test]
    fn every_setting_shows_a_value_in_effect() {
        let file = LightingFile::builtin("golden_hour").unwrap();
        let values = effective(&file).unwrap();
        for info in SETTINGS {
            let optional = info.path == ["sun", "temperature"];
            assert!(optional || find(&values, info.path).is_some(), "{:?} has a value", info.path);
        }
        assert_eq!(find(&values, &["sky", "mode"]).and_then(|v| v.as_str()), Some("atmosphere"));
    }
}
