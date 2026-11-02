//! An asset's import settings in the Inspector, as Unity's importers show
//! them: a model's units and scale, where its normals and tangents come
//! from, whether it gets LODs and uses the file's materials; a picture's
//! largest size and whether a build compresses it; a sound's volume, and
//! whether it loads whole or streams. They change nothing until Apply,
//! which asks for them to be written to the asset's `.meta` and the asset
//! imported again; Revert puts back the ones applied.

use std::path::Path;

use voxear_assets::{ModelImport, Normals, SoundImport, SoundLoad, Tangents, TextureImport, Units};
use voxear_ui::{NodeId, Style, Ui};

use crate::inspector::{number, prop};

/// Import settings applied, to write and import with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportSettings {
    Model(ModelImport),
    Texture(TextureImport),
    Sound(SoundImport),
}

/// The import settings of a model, a picture or a sound, as they are being
/// changed.
pub enum ImportFields {
    Model(ModelFields),
    Texture(TextureFields),
    Sound(SoundFields),
}

impl ImportFields {
    /// The settings in the `.meta` beside the model at `file`, in `rows`.
    pub fn model(ui: &mut Ui, rows: NodeId, file: &Path) -> Self {
        Self::Model(ModelFields::build(ui, rows, ModelImport::read(file)))
    }

    /// The settings in the `.meta` beside the picture at `file`, in `rows`.
    pub fn texture(ui: &mut Ui, rows: NodeId, file: &Path) -> Self {
        Self::Texture(TextureFields::build(ui, rows, TextureImport::read(file)))
    }

    /// The settings in the `.meta` beside the sound at `file`, in `rows`.
    pub fn sound(ui: &mut Ui, rows: NodeId, file: &Path) -> Self {
        Self::Sound(SoundFields::build(ui, rows, SoundImport::read(file)))
    }

    /// The settings to apply, when Apply was clicked.
    pub fn update(&mut self, ui: &mut Ui) -> Option<ImportSettings> {
        match self {
            Self::Model(fields) => fields.update(ui).map(ImportSettings::Model),
            Self::Texture(fields) => fields.update(ui).map(ImportSettings::Texture),
            Self::Sound(fields) => fields.update(ui).map(ImportSettings::Sound),
        }
    }
}

/// A model's import settings, and the ones last applied.
pub struct ModelFields {
    applied: ModelImport,
    units: NodeId,
    scale: NodeId,
    normals: NodeId,
    smoothing_angle: NodeId,
    tangents: NodeId,
    lods: NodeId,
    materials: NodeId,
    buttons: Buttons,
}

impl ModelFields {
    const UNITS: [&str; 6] = ["As the file says", "Millimetres", "Centimetres", "Metres", "Inches", "Feet"];
    const SOURCES: [&str; 2] = ["From the file", "Calculate"];

    fn build(ui: &mut Ui, rows: NodeId, applied: ModelImport) -> Self {
        let fields = Self {
            applied,
            units: choice(ui, rows, "Units", &Self::UNITS),
            scale: number_field(ui, rows, "Scale"),
            normals: choice(ui, rows, "Normals", &Self::SOURCES),
            smoothing_angle: number_field(ui, rows, "Smoothing angle"),
            tangents: choice(ui, rows, "Tangents", &Self::SOURCES),
            lods: toggle(ui, rows, "Make LODs"),
            materials: toggle(ui, rows, "Import materials"),
            buttons: Buttons::new(ui, rows),
        };
        fields.show(ui, &applied);
        fields
    }

    /// Shows `settings` in the fields.
    fn show(&self, ui: &mut Ui, settings: &ModelImport) {
        let units = Units::ALL.iter().position(|&units| units == settings.units).unwrap_or(0);
        ui.set_selected_option(self.units, units);
        ui.set_text(self.scale, &number(settings.scale));
        ui.set_selected_option(self.normals, usize::from(settings.normals == Normals::Calculate));
        ui.set_text(self.smoothing_angle, &number(settings.smoothing_angle));
        ui.set_selected_option(self.tangents, usize::from(settings.tangents == Tangents::Calculate));
        ui.set_checked(self.lods, settings.lods);
        ui.set_checked(self.materials, settings.materials);
        self.enable(ui, settings);
    }

    /// The settings as the fields show them. A number that does not read,
    /// or is out of its range, keeps the value applied.
    fn shown(&self, ui: &Ui) -> ModelImport {
        let read = |field: NodeId, was: f32| {
            ui.text(field).trim().parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(was)
        };
        let calculate = |dropdown: NodeId| ui.selected_option(dropdown) == 1;
        let scale = read(self.scale, self.applied.scale);
        ModelImport {
            units: Units::ALL.get(ui.selected_option(self.units)).copied().unwrap_or_default(),
            scale: if scale > 0.0 { scale } else { self.applied.scale },
            normals: if calculate(self.normals) { Normals::Calculate } else { Normals::Import },
            smoothing_angle: read(self.smoothing_angle, self.applied.smoothing_angle).clamp(0.0, 180.0),
            tangents: if calculate(self.tangents) { Tangents::Calculate } else { Tangents::Import },
            lods: ui.is_checked(self.lods),
            materials: ui.is_checked(self.materials),
        }
    }

    /// Revert and Apply while the fields differ from what is applied; the
    /// smoothing angle while normals are calculated.
    fn enable(&self, ui: &mut Ui, shown: &ModelImport) {
        self.buttons.enable(ui, *shown != self.applied);
        ui.set_enabled(self.smoothing_angle, shown.normals == Normals::Calculate);
    }

    fn update(&mut self, ui: &mut Ui) -> Option<ModelImport> {
        let shown = self.shown(ui);
        match self.buttons.clicked(ui) {
            Some(Click::Revert) => self.show(ui, &self.applied.clone()),
            Some(Click::Apply) if shown != self.applied => {
                self.applied = shown;
                self.show(ui, &shown);
                return Some(shown);
            }
            _ => self.enable(ui, &shown),
        }
        None
    }
}

/// A picture's import settings, and the ones last applied.
pub struct TextureFields {
    applied: TextureImport,
    max_size: NodeId,
    compress: NodeId,
    buttons: Buttons,
}

impl TextureFields {
    /// The largest sizes offered, after "Its own size".
    const SIZES: [u32; 9] = [8192, 4096, 2048, 1024, 512, 256, 128, 64, 32];

    fn build(ui: &mut Ui, rows: NodeId, applied: TextureImport) -> Self {
        let sizes: Vec<String> = Self::SIZES.iter().map(u32::to_string).collect();
        let options: Vec<&str> = std::iter::once("Its own size").chain(sizes.iter().map(String::as_str)).collect();
        let fields = Self {
            applied,
            max_size: choice(ui, rows, "Max size", &options),
            compress: toggle(ui, rows, "Compress in builds"),
            buttons: Buttons::new(ui, rows),
        };
        fields.show(ui, &applied);
        fields
    }

    /// Where a largest size shows in the menu: a size it does not offer, as
    /// written by hand in the `.meta`, shows as the next one up.
    fn option(max_size: Option<u32>) -> usize {
        let Some(max) = max_size else { return 0 };
        Self::SIZES.iter().rposition(|&size| size >= max).map_or(1, |at| at + 1)
    }

    fn show(&self, ui: &mut Ui, settings: &TextureImport) {
        ui.set_selected_option(self.max_size, Self::option(settings.max_size));
        ui.set_checked(self.compress, settings.compress);
        self.buttons.enable(ui, *settings != self.applied);
    }

    /// The settings as the fields show them. The menu still on the size
    /// applied keeps it, though the menu does not offer it.
    fn shown(&self, ui: &Ui) -> TextureImport {
        let picked = ui.selected_option(self.max_size);
        let max_size = match picked == Self::option(self.applied.max_size) {
            true => self.applied.max_size,
            false => picked.checked_sub(1).and_then(|at| Self::SIZES.get(at).copied()),
        };
        TextureImport { max_size, compress: ui.is_checked(self.compress) }
    }

    fn update(&mut self, ui: &mut Ui) -> Option<TextureImport> {
        let shown = self.shown(ui);
        match self.buttons.clicked(ui) {
            Some(Click::Revert) => self.show(ui, &self.applied.clone()),
            Some(Click::Apply) if shown != self.applied => {
                self.applied = shown;
                self.show(ui, &shown);
                return Some(shown);
            }
            _ => self.buttons.enable(ui, shown != self.applied),
        }
        None
    }
}

/// A sound's import settings, and the ones last applied.
pub struct SoundFields {
    applied: SoundImport,
    load: NodeId,
    volume: NodeId,
    buttons: Buttons,
}

impl SoundFields {
    /// By length decodes a sound whole unless it is long, when it streams.
    const LOADS: [&str; 3] = ["By length", "Whole", "Streamed"];

    fn build(ui: &mut Ui, rows: NodeId, applied: SoundImport) -> Self {
        let fields = Self {
            applied,
            load: choice(ui, rows, "Load", &Self::LOADS),
            volume: number_field(ui, rows, "Volume"),
            buttons: Buttons::new(ui, rows),
        };
        fields.show(ui, &applied);
        fields
    }

    fn show(&self, ui: &mut Ui, settings: &SoundImport) {
        let load = SoundLoad::ALL.iter().position(|&load| load == settings.load).unwrap_or(0);
        ui.set_selected_option(self.load, load);
        ui.set_text(self.volume, &number(settings.volume));
        self.buttons.enable(ui, *settings != self.applied);
    }

    /// The settings as the fields show them. A volume that does not read,
    /// or is below nothing, keeps the one applied.
    fn shown(&self, ui: &Ui) -> SoundImport {
        let volume = ui.text(self.volume).trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0);
        SoundImport {
            load: SoundLoad::ALL.get(ui.selected_option(self.load)).copied().unwrap_or_default(),
            volume: volume.unwrap_or(self.applied.volume),
        }
    }

    fn update(&mut self, ui: &mut Ui) -> Option<SoundImport> {
        let shown = self.shown(ui);
        match self.buttons.clicked(ui) {
            Some(Click::Revert) => self.show(ui, &self.applied.clone()),
            Some(Click::Apply) if shown != self.applied => {
                self.applied = shown;
                self.show(ui, &shown);
                return Some(shown);
            }
            _ => self.buttons.enable(ui, shown != self.applied),
        }
        None
    }
}

/// Revert and Apply, at the end of the settings.
struct Buttons {
    revert: NodeId,
    apply: NodeId,
}

impl Buttons {
    fn new(ui: &mut Ui, rows: NodeId) -> Self {
        let buttons = ui.panel(rows, Style::row());
        ui.add_class(buttons, "import-buttons");
        let revert = ui.button(buttons, "Revert");
        let apply = ui.button(buttons, "Apply");
        for (button, name) in [(revert, "import-revert"), (apply, "import-apply")] {
            ui.add_class(button, "import-button");
            ui.set_name(button, name);
        }
        Self { revert, apply }
    }

    /// Both buttons, while the settings shown differ from those applied.
    fn enable(&self, ui: &mut Ui, changed: bool) {
        ui.set_enabled(self.revert, changed);
        ui.set_enabled(self.apply, changed);
    }

    fn clicked(&self, ui: &Ui) -> Option<Click> {
        match (ui.clicked(self.revert), ui.clicked(self.apply)) {
            (true, _) => Some(Click::Revert),
            (_, true) => Some(Click::Apply),
            _ => None,
        }
    }
}

enum Click {
    Revert,
    Apply,
}

fn choice(ui: &mut Ui, rows: NodeId, label: &str, options: &[&str]) -> NodeId {
    let row = prop(ui, rows, label);
    let dropdown = ui.dropdown(row, options, 0);
    ui.add_class(dropdown, "value-choice");
    dropdown
}

fn number_field(ui: &mut Ui, rows: NodeId, label: &str) -> NodeId {
    let row = prop(ui, rows, label);
    let field = ui.text_field(row, "");
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    field
}

fn toggle(ui: &mut Ui, rows: NodeId, label: &str) -> NodeId {
    let row = prop(ui, rows, label);
    ui.toggle(row, "", false)
}
