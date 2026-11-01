//! The editor's settings, and what it remembers of a project between runs:
//! the settings themselves and what the command palette ran recently. Both
//! are kept in the project's `.voxear/Editor.toml`, beside its builds, so
//! they stay out of version control. An editor opened without memory, as
//! tests and screenshots open it, starts from the defaults and writes
//! nothing.
//!
//! Each setting is described once in [`SETTINGS`], which the Settings panel
//! shows and the command palette searches.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use voxear_ui::{Color, Theme};

/// The most items the palette remembers having run.
const RECENT: usize = 8;
/// The snap distances the tool strip and Settings offer, in metres.
pub const SNAPS: [f32; 4] = [0.1, 0.25, 0.5, 1.0];

/// The editors a stack frame can open in, as the Code editor setting lists
/// them; the last opens the file as the desktop does, without its line.
pub const CODE_EDITORS: [&str; 4] = ["VS Code", "Rider", "Zed", "System default"];

/// The themes the Theme setting offers before Custom, which is whatever the
/// base, accent and contrast were last set to.
pub const THEMES: [(&str, Theme); 2] = [("Dark", Theme::DARK), ("Light", Theme::LIGHT)];

/// What a setting holds, and so how it is shown and changed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingKind {
    Toggle,
    /// One of these, by index.
    Choice(&'static [&'static str]),
    /// A number from `min` to `max`, with its unit.
    Number {
        min: f32,
        max: f32,
        unit: &'static str,
    },
    /// A colour, picked from its swatch.
    Color,
}

/// One setting: the key it is kept and found by, what it is called, the
/// group it shows in, and what it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingInfo {
    pub key: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub kind: SettingKind,
}

pub const SETTINGS: [SettingInfo; 23] = [
    SettingInfo {
        key: "snap",
        title: "Snap distance",
        group: "Scene view",
        kind: SettingKind::Choice(&["0.1 m", "0.25 m", "0.5 m", "1 m"]),
    },
    SettingInfo {
        key: "handle_position",
        title: "Handle position",
        group: "Scene view",
        kind: SettingKind::Choice(&["Pivot", "Center"]),
    },
    SettingInfo {
        key: "handle_space",
        title: "Handle space",
        group: "Scene view",
        kind: SettingKind::Choice(&["Global", "Local"]),
    },
    SettingInfo {
        key: "fly_speed",
        title: "Fly speed",
        group: "Scene view",
        kind: SettingKind::Number { min: 0.25, max: 250.0, unit: "m/s" },
    },
    SettingInfo { key: "show_stats", title: "Show stats", group: "Scene view", kind: SettingKind::Toggle },
    SettingInfo { key: "show_axis_gizmo", title: "Show axis gizmo", group: "Scene view", kind: SettingKind::Toggle },
    SettingInfo { key: "show_gizmos", title: "Show gizmos", group: "Scene view", kind: SettingKind::Toggle },
    SettingInfo {
        key: "scene_shading",
        title: "Shading",
        group: "Scene view",
        kind: SettingKind::Choice(&["Lit", "Unlit", "Wireframe"]),
    },
    SettingInfo { key: "start_paused", title: "Start paused", group: "Play mode", kind: SettingKind::Toggle },
    SettingInfo { key: "save_before_play", title: "Save before play", group: "Play mode", kind: SettingKind::Toggle },
    SettingInfo {
        key: "clear_console_on_play",
        title: "Clear Console on play",
        group: "Play mode",
        kind: SettingKind::Toggle,
    },
    SettingInfo { key: "error_pause", title: "Pause on error", group: "Play mode", kind: SettingKind::Toggle },
    SettingInfo {
        key: "reload_while_playing",
        title: "Reload changed scripts while playing",
        group: "Play mode",
        kind: SettingKind::Toggle,
    },
    SettingInfo {
        key: "console_collapse",
        title: "Collapse identical lines",
        group: "Console",
        kind: SettingKind::Toggle,
    },
    SettingInfo { key: "console_mine_only", title: "My code only", group: "Console", kind: SettingKind::Toggle },
    SettingInfo { key: "console_namespaces", title: "Show namespaces", group: "Console", kind: SettingKind::Toggle },
    SettingInfo { key: "console_parameters", title: "Show parameters", group: "Console", kind: SettingKind::Toggle },
    SettingInfo {
        key: "code_editor",
        title: "Code editor",
        group: "Console",
        kind: SettingKind::Choice(&CODE_EDITORS),
    },
    SettingInfo {
        key: "theme",
        title: "Theme",
        group: "Appearance",
        kind: SettingKind::Choice(&["Dark", "Light", "Custom"]),
    },
    SettingInfo { key: "theme_base", title: "Base colour", group: "Appearance", kind: SettingKind::Color },
    SettingInfo { key: "theme_accent", title: "Accent colour", group: "Appearance", kind: SettingKind::Color },
    SettingInfo {
        key: "theme_contrast",
        title: "Contrast",
        group: "Appearance",
        kind: SettingKind::Number { min: 0.0, max: 100.0, unit: "%" },
    },
    SettingInfo {
        key: "prefab_parts",
        title: "Open out prefabs to their Objs",
        group: "Project panel",
        kind: SettingKind::Toggle,
    },
];

/// A setting's value, as the panel and palette show and change it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingValue {
    Toggle(bool),
    Choice(usize),
    Number(f32),
    /// `0xRRGGBB`.
    Color(u32),
}

/// The editor's settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Metres the handles snap to while Ctrl is held.
    pub snap: f32,
    /// Handles at the middle of the Obj's bounds rather than its pivot.
    pub centred: bool,
    /// Handles along the Obj's own axes rather than the world's.
    pub local: bool,
    /// How fast the Scene view's camera flies, in metres a second; the wheel
    /// changes it while flying.
    pub fly_speed: f32,
    pub show_stats: bool,
    pub show_axis_gizmo: bool,
    /// Lights and cameras are drawn in the Scene view, and what the selected
    /// one reaches.
    pub show_gizmos: bool,
    /// How the Scene view shades: lit, unlit, or as wireframe.
    pub scene_shading: usize,
    /// Prefabs in the Project panel open out to their Objs, as models open
    /// out to their parts.
    pub prefab_parts: bool,
    /// Play starts paused, to step through the first frames.
    pub start_paused: bool,
    pub save_before_play: bool,
    pub clear_console_on_play: bool,
    /// Play pauses when an error is logged, as Unity's Error Pause does.
    pub error_pause: bool,
    /// Scripts changed while playing reload into the running game, rather
    /// than waiting for play to stop.
    pub reload_while_playing: bool,
    /// The Console shows identical lines once, with how many there were.
    pub console_collapse: bool,
    /// A trace folds away frames that are not the project's own.
    pub console_mine_only: bool,
    pub console_namespaces: bool,
    pub console_parameters: bool,
    /// Which of [`CODE_EDITORS`] opens a frame's file.
    pub code_editor: usize,
    /// Frames from these assemblies (a frame's root namespace, such as
    /// `Voxear`, or an engine crate) fold away in the Console's traces.
    pub console_hidden_assemblies: Vec<String>,
    /// The Console's filters, saved by name.
    pub console_filters: Vec<SavedFilter>,
    /// How tall the Console's lines are, in pixels.
    pub console_row_height: f32,
    /// Warnings and errors take their colour across the line, not only in
    /// their mark.
    pub console_tint: bool,
    /// Every other line of the Console is shaded.
    pub console_stripes: bool,
    /// How a line shows its category: 0 not at all, 1 a dot in its colour,
    /// 2 its name.
    pub console_category: usize,
    /// How many lines the Console holds before the oldest go.
    pub console_kept: usize,
    /// Colours chosen in place of the Console's own: `error`, `warning`, and
    /// a frame's `namespace`, `class`, `method`, `parameters` and `file`.
    pub console_colours: std::collections::BTreeMap<String, u32>,
    /// Which of [`THEMES`] the editor wears, or past them, Custom.
    pub theme: usize,
    /// The theme's three choices (see [`Theme`]): its ground and accent as
    /// `0xRRGGBB`, and its contrast from 0 to 100.
    pub theme_base: u32,
    pub theme_accent: u32,
    pub theme_contrast: f32,
}

/// A Console filter saved by name, as Sifter keeps them: a search, and the
/// levels and categories it hides.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedFilter {
    pub name: String,
    pub search: String,
    /// Log, Warning and Error.
    pub hidden_levels: [bool; 3],
    pub hidden_categories: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            snap: 0.25,
            centred: false,
            local: false,
            fly_speed: 6.0,
            show_stats: true,
            show_axis_gizmo: true,
            show_gizmos: true,
            scene_shading: 0,
            prefab_parts: false,
            start_paused: false,
            save_before_play: false,
            clear_console_on_play: false,
            error_pause: false,
            reload_while_playing: true,
            console_collapse: false,
            console_mine_only: true,
            console_namespaces: true,
            console_parameters: true,
            code_editor: 0,
            console_hidden_assemblies: Vec::new(),
            console_filters: Vec::new(),
            console_row_height: 20.0,
            console_tint: true,
            console_stripes: false,
            console_category: 2,
            console_kept: 10_000,
            console_colours: std::collections::BTreeMap::new(),
            theme: 0,
            theme_base: 0x0c0e10,
            theme_accent: 0x7988ff,
            theme_contrast: 50.0,
        }
    }
}

impl Settings {
    pub fn get(&self, key: &str) -> Option<SettingValue> {
        Some(match key {
            "snap" => SettingValue::Choice(SNAPS.iter().position(|s| (s - self.snap).abs() < 1e-4).unwrap_or(1)),
            "handle_position" => SettingValue::Choice(usize::from(self.centred)),
            "handle_space" => SettingValue::Choice(usize::from(self.local)),
            "fly_speed" => SettingValue::Number(self.fly_speed),
            "show_stats" => SettingValue::Toggle(self.show_stats),
            "show_axis_gizmo" => SettingValue::Toggle(self.show_axis_gizmo),
            "show_gizmos" => SettingValue::Toggle(self.show_gizmos),
            "scene_shading" => SettingValue::Choice(self.scene_shading),
            "prefab_parts" => SettingValue::Toggle(self.prefab_parts),
            "start_paused" => SettingValue::Toggle(self.start_paused),
            "save_before_play" => SettingValue::Toggle(self.save_before_play),
            "clear_console_on_play" => SettingValue::Toggle(self.clear_console_on_play),
            "error_pause" => SettingValue::Toggle(self.error_pause),
            "reload_while_playing" => SettingValue::Toggle(self.reload_while_playing),
            "console_collapse" => SettingValue::Toggle(self.console_collapse),
            "console_mine_only" => SettingValue::Toggle(self.console_mine_only),
            "console_namespaces" => SettingValue::Toggle(self.console_namespaces),
            "console_parameters" => SettingValue::Toggle(self.console_parameters),
            "code_editor" => SettingValue::Choice(self.code_editor),
            "theme" => SettingValue::Choice(self.theme),
            "theme_base" => SettingValue::Color(self.theme_base),
            "theme_accent" => SettingValue::Color(self.theme_accent),
            "theme_contrast" => SettingValue::Number(self.theme_contrast),
            _ => return None,
        })
    }

    /// Sets a setting, keeping a number inside its range. Returns whether it
    /// changed.
    pub fn set(&mut self, key: &str, value: SettingValue) -> bool {
        let before = self.clone();
        match (key, value) {
            ("snap", SettingValue::Choice(i)) => self.snap = SNAPS[i.min(SNAPS.len() - 1)],
            ("handle_position", SettingValue::Choice(i)) => self.centred = i == 1,
            ("handle_space", SettingValue::Choice(i)) => self.local = i == 1,
            ("fly_speed", SettingValue::Number(n)) => self.fly_speed = n.clamp(0.25, 250.0),
            ("show_stats", SettingValue::Toggle(on)) => self.show_stats = on,
            ("show_axis_gizmo", SettingValue::Toggle(on)) => self.show_axis_gizmo = on,
            ("show_gizmos", SettingValue::Toggle(on)) => self.show_gizmos = on,
            ("scene_shading", SettingValue::Choice(i)) => self.scene_shading = i.min(2),
            ("prefab_parts", SettingValue::Toggle(on)) => self.prefab_parts = on,
            ("start_paused", SettingValue::Toggle(on)) => self.start_paused = on,
            ("save_before_play", SettingValue::Toggle(on)) => self.save_before_play = on,
            ("clear_console_on_play", SettingValue::Toggle(on)) => self.clear_console_on_play = on,
            ("error_pause", SettingValue::Toggle(on)) => self.error_pause = on,
            ("reload_while_playing", SettingValue::Toggle(on)) => self.reload_while_playing = on,
            ("console_collapse", SettingValue::Toggle(on)) => self.console_collapse = on,
            ("console_mine_only", SettingValue::Toggle(on)) => self.console_mine_only = on,
            ("console_namespaces", SettingValue::Toggle(on)) => self.console_namespaces = on,
            ("console_parameters", SettingValue::Toggle(on)) => self.console_parameters = on,
            ("code_editor", SettingValue::Choice(i)) => self.code_editor = i.min(CODE_EDITORS.len() - 1),
            ("theme", SettingValue::Choice(i)) => self.set_theme(i),
            // Changing one of the three makes the theme Custom.
            ("theme_base", SettingValue::Color(c)) if c != self.theme_base => (self.theme_base, self.theme) = (c, 2),
            ("theme_accent", SettingValue::Color(c)) if c != self.theme_accent => {
                (self.theme_accent, self.theme) = (c, 2)
            }
            ("theme_contrast", SettingValue::Number(n)) if n.clamp(0.0, 100.0) != self.theme_contrast => {
                (self.theme_contrast, self.theme) = (n.clamp(0.0, 100.0), 2)
            }
            _ => {}
        }
        *self != before
    }

    /// Wears one of [`THEMES`], its three choices with it, or Custom, which
    /// keeps the choices as they are.
    fn set_theme(&mut self, index: usize) {
        self.theme = index.min(THEMES.len());
        if let Some((_, theme)) = THEMES.get(index) {
            let hex = |c: Color| u32::from_be_bytes([0, c.r, c.g, c.b]);
            (self.theme_base, self.theme_accent) = (hex(theme.base), hex(theme.accent));
            self.theme_contrast = theme.contrast * 100.0;
        }
    }

    /// The theme the three choices make.
    pub fn theme(&self) -> Theme {
        let (base, accent) = (Color::hex(self.theme_base), Color::hex(self.theme_accent));
        Theme { base, accent, contrast: self.theme_contrast / 100.0 }
    }

    /// A setting as words: "On", "0.25 m", "6 m/s".
    pub fn describe(&self, info: &SettingInfo) -> String {
        match (self.get(info.key), info.kind) {
            (Some(SettingValue::Toggle(on)), _) => if on { "On" } else { "Off" }.into(),
            (Some(SettingValue::Choice(i)), SettingKind::Choice(options)) => {
                options.get(i).copied().unwrap_or_default().into()
            }
            (Some(SettingValue::Number(n)), SettingKind::Number { unit, .. }) => {
                format!("{} {unit}", (n * 100.0).round() / 100.0)
            }
            (Some(SettingValue::Color(c)), _) => Color::hex(c).css(),
            _ => String::new(),
        }
    }
}

pub fn info(key: &str) -> Option<&'static SettingInfo> {
    SETTINGS.iter().find(|s| s.key == key)
}

/// A layout saved with Save Layout As: its name, and the arrangement it
/// starts with and goes back to on Reset Layout.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedLayout {
    pub name: String,
    pub start: String,
}

/// What the editor remembers of a project: its settings, the keys of what
/// the palette ran, most recent first, and how its panels were laid out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    pub settings: Settings,
    pub recent: Vec<String>,
    /// The layout showing, by its place, and each layout's arrangement as
    /// it was left, as the dock writes it; empty for one never shown. The
    /// built-in layouts come first, then those saved, in that order.
    pub workspace: usize,
    pub layouts: Vec<String>,
    pub saved_layouts: Vec<SavedLayout>,
    /// Where it is kept, or `None` for an editor that remembers nothing.
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Memory {
    /// What the editor remembers of the project at `root`, or the defaults
    /// when it has nothing, or its file cannot be read.
    pub fn open(root: &Path) -> Self {
        let path = root.join(".voxear").join("Editor.toml");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let mut memory: Memory = toml::from_str(&text).unwrap_or_else(|error| {
            tracing::warn!("{} could not be read, so the editor starts from its defaults: {error}", path.display());
            Memory::default()
        });
        memory.path = Some(path);
        memory
    }

    /// Memory that starts from the defaults and is never written.
    pub fn forgetful() -> Self {
        Self::default()
    }

    /// Notes that the palette ran what `key` names, most recent first.
    pub fn ran(&mut self, key: &str) {
        self.recent.retain(|k| k != key);
        self.recent.insert(0, key.to_string());
        self.recent.truncate(RECENT);
        self.save();
    }

    /// Writes the memory, unless it is forgetful. A failure is logged and
    /// otherwise ignored: the editor works on without it.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let written = toml::to_string_pretty(self).map_err(anyhow::Error::from).and_then(|text| {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder)?;
            }
            Ok(std::fs::write(path, text)?)
        });
        if let Err(error) = written {
            tracing::warn!("could not keep the editor's settings in {}: {error:#}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_recent_items_are_kept_per_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut memory = Memory::open(dir.path());
        assert_eq!(memory.settings, Settings::default(), "nothing kept yet");
        assert!(memory.settings.set("snap", SettingValue::Choice(3)));
        assert!(!memory.settings.set("snap", SettingValue::Choice(3)), "the same again is no change");
        assert!(memory.settings.set("fly_speed", SettingValue::Number(1000.0)));
        assert_eq!(memory.settings.fly_speed, 250.0, "kept inside its range");
        memory.save();
        for key in ["command:Save", "obj:Lamp", "command:Save"] {
            memory.ran(key);
        }

        let again = Memory::open(dir.path());
        assert_eq!(again.settings.snap, 1.0);
        assert_eq!(again.recent, ["command:Save", "obj:Lamp"], "most recent first, once each");
        assert_eq!(again.settings.describe(info("snap").unwrap()), "1 m");
        assert_eq!(again.settings.describe(info("show_stats").unwrap()), "On");

        let mut themed = Settings::default();
        assert_eq!(themed.theme(), Theme::DARK, "the default theme's own choices");
        assert!(themed.set("theme", SettingValue::Choice(1)));
        assert_eq!(themed.theme(), Theme::LIGHT, "a theme brings its choices");
        assert!(themed.set("theme_accent", SettingValue::Color(0xe2a04a)));
        assert_eq!(themed.describe(info("theme").unwrap()), "Custom", "changing one makes it Custom");
        assert_eq!(themed.theme().base, Theme::LIGHT.base, "keeping the rest");
        assert!(!themed.set("theme_contrast", SettingValue::Number(50.0)), "the same contrast is no change");

        let forgetful = Memory::forgetful();
        forgetful.save();
        assert_eq!(forgetful.settings, Settings::default());
    }
}
