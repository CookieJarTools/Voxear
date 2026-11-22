//! The editor's settings, and what it remembers of a project between runs:
//! the settings themselves and what the command palette ran recently. Both
//! are kept in the project's `.voxear/Editor.toml`, beside its builds, so
//! they stay out of version control. An editor opened without memory, as
//! tests and screenshots open it, starts from the defaults and writes
//! nothing.
//!
//! Each setting is described once in [`PREFERENCES`], which the Preferences
//! window shows, its search and the command palette find, and `vox editor
//! call` lists.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use voxear_ui::{Color, Theme};

pub use crate::code_editor::CodeEditor;
use crate::setting_info::{Category, Kind, SectionHelp, Setting, kept_word, shown_label};

/// The most items the palette remembers having run.
const RECENT: usize = 8;
/// How many blocks the brush built with are remembered.
const RECENT_BLOCKS: usize = 24;
/// The snap distances the tool strip and Preferences offer, in metres.
pub const SNAPS: [f32; 4] = [0.1, 0.25, 0.5, 1.0];

/// The themes the Theme setting offers before Custom, which is whatever the
/// base, accent and contrast were last set to.
pub const THEMES: [(&str, Theme); 2] = [("Dark", Theme::DARK), ("Light", Theme::LIGHT)];

/// The categories down the Preferences window's side, in order.
pub const CATEGORIES: &[Category] = &[
    Category { name: "General", about: "The Project panel", manual: "Getting Started/Preferences" },
    Category { name: "Appearance", about: "The editor's theme and colours", manual: "Getting Started/Preferences" },
    Category {
        name: "Console",
        about: "How the Console shows its lines and traces",
        manual: "Finding Problems/The Console",
    },
    Category {
        name: "External Tools",
        about: "The code editor, and folders the editor watches",
        manual: "Getting Started/Preferences",
    },
    Category { name: "Keys", about: "Every command's keys", manual: "Getting Started/Preferences" },
    Category { name: "Play Mode", about: "How Play starts and plays", manual: "Getting Started/Playing Your Game" },
    Category {
        name: "Scene View",
        about: "The Scene view's camera, handles, overlays and gizmos",
        manual: "Getting Started/A Tour of the Editor",
    },
    Category {
        name: "Tools",
        about: "What programs reaching the editor may do",
        manual: "The Command Line/Driving the Editor from a Terminal",
    },
];

/// Lines of help under sections' titles.
pub const HELP: &[SectionHelp] = &[
    SectionHelp {
        category: "Play Mode",
        section: "Networked Play",
        help: "For projects with the Networking module; the button beside Play sets these too.",
    },
    SectionHelp { category: "Keys", section: "Shortcuts", help: "Shortcuts cannot be changed yet." },
];

const SNAP_CHOICES: &[(&str, &str)] = &[("0.1 m", "0.1"), ("0.25 m", "0.25"), ("0.5 m", "0.5"), ("1 m", "1")];
const SHADING: &[(&str, &str)] = &[
    ("Lit", "lit"),
    ("Unlit", "unlit"),
    ("Wireframe", "wireframe"),
    ("Lit Wireframe", "lit-wireframe"),
    ("Heat", "heat"),
    ("Lightmap Density", "lightmap-density"),
];
const THEME_CHOICES: &[(&str, &str)] = &[("Dark", "dark"), ("Light", "light"), ("Custom", "custom")];
const ROW_HEIGHTS: &[(&str, &str)] = &[("Short", "18"), ("Cosy", "20"), ("Tall", "24")];
const CATEGORY_SHOWN: &[(&str, &str)] = &[("Off", "off"), ("Dot", "dot"), ("Name", "name")];
const KEPT_LINES: &[(&str, &str)] = &[("1,000", "1000"), ("5,000", "5000"), ("10,000", "10000"), ("50,000", "50000")];
const PLAY_AS: &[(&str, &str)] = &[("Offline", "offline"), ("Host", "host"), ("Server", "server")];
const NETWORKS: &[(&str, &str)] = &[("Perfect", "perfect"), ("Good", "good"), ("Average", "average"), ("Bad", "bad")];

/// The editors scripts open in, as the Code editor setting lists them: the
/// names of [`CodeEditor::ALL`]. The last opens the file as the desktop
/// does, without its line.
pub const CODE_EDITORS: [(&str, &str); 5] = {
    let mut names = [("", ""); 5];
    let mut at = 0;
    while at < names.len() {
        names[at] = (CodeEditor::ALL[at].name(), CodeEditor::ALL[at].name());
        at += 1;
    }
    names
};

/// Every setting Preferences shows, in order (see `setting_info`). Each is
/// kept by its key in [`Settings`].
pub const PREFERENCES: &[Setting] = &[
    Setting::new(
        ("General", "Project Panel"),
        "prefab_parts",
        "Open out prefabs",
        Kind::Toggle,
        "Prefabs in the Project panel open out to the Objs they hold, as models open out to their parts.",
    ),
    Setting::new(
        ("Appearance", "Theme"),
        "theme",
        "Theme",
        Kind::Choice(THEME_CHOICES),
        "The editor's colours. Custom keeps the base, accent and contrast as they are set; changing any of them makes it Custom.",
    ),
    Setting::new(
        ("Appearance", "Theme"),
        "theme_base",
        "Base colour",
        Kind::Color,
        "The colour the editor's ground and panels are made from.",
    ),
    Setting::new(
        ("Appearance", "Theme"),
        "theme_accent",
        "Accent colour",
        Kind::Color,
        "The colour of the selection, of focus and of controls that are on.",
    ),
    Setting::new(
        ("Appearance", "Theme"),
        "theme_contrast",
        "Contrast",
        Kind::Number { min: 0.0, max: 100.0, unit: "%", whole: false, slider: true },
        "How far apart panels, fields and lines stand from the ground. Higher reads more clearly; lower is calmer.",
    ),
    Setting::new(
        ("Console", "Rows"),
        "console_row_height",
        "Row height",
        Kind::Choice(ROW_HEIGHTS),
        "How tall each line of the Console is. Taller is easier to read; shorter shows more lines.",
    ),
    Setting::new(
        ("Console", "Rows"),
        "console_tint",
        "Colour by kind",
        Kind::Toggle,
        "Warnings and errors take their colour across the whole line, not only in their mark.",
    ),
    Setting::new(
        ("Console", "Rows"),
        "console_stripes",
        "Alternating rows",
        Kind::Toggle,
        "Every other line is shaded, to follow long lines across.",
    ),
    Setting::new(
        ("Console", "Rows"),
        "console_category",
        "Show category",
        Kind::Choice(CATEGORY_SHOWN),
        "How a line shows its category: not at all, as a dot in its colour, or by its name.",
    ),
    Setting::new(
        ("Console", "Rows"),
        "console_collapse",
        "Collapse identical lines",
        Kind::Toggle,
        "Identical lines show once, with how many there were.",
    ),
    Setting::new(
        ("Console", "Stack Traces"),
        "console_namespaces",
        "Namespaces",
        Kind::Toggle,
        "A trace's frames show their namespaces, as Voxear.Physics.",
    ),
    Setting::new(
        ("Console", "Stack Traces"),
        "console_parameters",
        "Parameters",
        Kind::Toggle,
        "A trace's frames show their methods' parameters.",
    ),
    Setting::new(
        ("Console", "Stack Traces"),
        "console_mine_only",
        "My code only",
        Kind::Toggle,
        "A trace folds away the frames that are not the project's own.",
    ),
    Setting::new(
        ("Console", "Lines"),
        "console_kept",
        "Lines held",
        Kind::Choice(KEPT_LINES),
        "How many lines the Console holds before the oldest go. More keeps a longer history and uses more memory.",
    ),
    Setting::new(
        ("Console", "Lines"),
        "clear_console_on_play",
        "Clear on play",
        Kind::Toggle,
        "The Console is emptied each time Play starts.",
    ),
    Setting::new(
        ("Console", "Colours"),
        "console.settings",
        "Colours",
        Kind::Button("Open Console Settings"),
        "Opens the Console's own settings, with a live sample, where the colours of errors, warnings and traces are picked.",
    ),
    Setting::new(
        ("External Tools", "Code"),
        "code_editor",
        "Code editor",
        Kind::Choice(&CODE_EDITORS),
        "The editor scripts and the project open in, at the line asked for. System Default opens the file as the desktop does, without its line.",
    ),
    Setting::new(
        ("External Tools", "Downloads"),
        "fab_downloads",
        "Watch for downloads",
        Kind::Folder,
        "The folder the browser downloads into: Megascans downloaded from Fab into it are imported when the editor gets the focus back. None watches nothing.",
    ),
    Setting::new(
        ("Keys", "Shortcuts"),
        "keys",
        "Shortcuts",
        Kind::Part,
        "Every command that has keys, and its keys, as the command bar shows them.",
    ),
    Setting::new(
        ("Play Mode", "Starting"),
        "start_paused",
        "Start paused",
        Kind::Toggle,
        "Play starts paused, to step through the first frames.",
    ),
    Setting::new(
        ("Play Mode", "Starting"),
        "save_before_play",
        "Save before play",
        Kind::Toggle,
        "Unsaved changes are saved each time Play starts.",
    ),
    Setting::new(
        ("Play Mode", "Starting"),
        "reload_while_playing",
        "Reload scripts while playing",
        Kind::Toggle,
        "Scripts changed while playing reload into the running game; off waits for play to stop.",
    ),
    Setting::new(
        ("Play Mode", "Starting"),
        "error_pause",
        "Pause on error",
        Kind::Toggle,
        "Play pauses when an error is logged, to look at the moment it happened.",
    ),
    Setting::new(
        ("Play Mode", "Networked Play"),
        "multiplayer",
        "Play as",
        Kind::Choice(PLAY_AS),
        "How Play plays: offline; as a host, which runs the game and plays it; or as the first player of a dedicated server.",
    ),
    Setting::new(
        ("Play Mode", "Networked Play"),
        "test_players",
        "Test players",
        Kind::Number { min: 0.0, max: MOST_TEST_PLAYERS as f64, unit: "", whole: true, slider: false },
        "How many more players' windows open with networked Play, each a player of its own.",
    ),
    Setting::new(
        ("Play Mode", "Networked Play"),
        "test_network",
        "Test network",
        Kind::Choice(NETWORKS),
        "How bad the network networked Play goes through is made, with lag, jitter and loss, to see how the game holds up.",
    ),
    Setting::new(
        ("Scene View", "Camera"),
        "fly_speed",
        "Fly speed",
        Kind::Number { min: 0.25, max: 250.0, unit: "m/s", whole: false, slider: false },
        "How fast the Scene view's camera flies; the mouse wheel changes it while flying.",
    ),
    Setting::new(
        ("Scene View", "Camera"),
        "fly_speeds_up",
        "Speed up while flying",
        Kind::Toggle,
        "The camera flies faster the longer it moves without stopping, up to 32 times its speed after six seconds.",
    ),
    Setting::new(
        ("Scene View", "Handles"),
        "snap",
        "Snap",
        Kind::Choice(SNAP_CHOICES),
        "How far the handles move in a step while Ctrl is held.",
    ),
    Setting::new(
        ("Scene View", "Handles"),
        "handle_position",
        "Handle position",
        Kind::Choice(&[("Pivot", "pivot"), ("Center", "center")]),
        "Where the handles sit: at the selected Obj's pivot, or in the middle of its bounds.",
    ),
    Setting::new(
        ("Scene View", "Handles"),
        "handle_space",
        "Handle space",
        Kind::Choice(&[("Global", "global"), ("Local", "local")]),
        "Whether the handles follow the world's axes or the selected Obj's own.",
    ),
    Setting::new(
        ("Scene View", "Overlays"),
        "scene_shading",
        "Shading",
        Kind::Choice(SHADING),
        "How the Scene view draws: lit as the game is, unlit, as wireframe, by temperature, or by how finely lightmaps cover each surface.",
    ),
    Setting::new(
        ("Scene View", "Overlays"),
        "show_stats",
        "Show stats",
        Kind::Toggle,
        "The frame's time, draw calls and triangles over the Scene view.",
    ),
    Setting::new(
        ("Scene View", "Overlays"),
        "show_axis_gizmo",
        "Show axis gizmo",
        Kind::Toggle,
        "The axes in the Scene view's corner, which turn the view to look along one.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "show_gizmos",
        "Show gizmos",
        Kind::Toggle,
        "Lights, cameras and the other gizmos are drawn in the Scene view, with what the selected one reaches.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "light_gizmos",
        "Lights",
        Kind::Toggle,
        "Lights' gizmos: their icons, and the reach of the one selected.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "camera_gizmos",
        "Cameras",
        Kind::Toggle,
        "Cameras' gizmos: their icons, and what the one selected sees.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "audio_gizmos",
        "Audio sources",
        Kind::Toggle,
        "Audio Sources' gizmos: their icons, and how far the one selected is heard.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "vehicle_gizmos",
        "Vehicle debug",
        Kind::Toggle,
        "Every Vehicle's wheels drawn while playing: their suspension, contacts and slip.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "nav_gizmos",
        "Navigation",
        Kind::Toggle,
        "Where things can walk in each Block World near the camera, a square on each place's floor coloured by \
         what it is, and each Nav Agent's and Block Mob's way. Off by default.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "bare_ground",
        "Bare Ground",
        Kind::Toggle,
        "Worlds of blocks without end drawn without their world layers, as Ground Fits, to see the generator's own \
         ground under them. The scene is not changed. Off by default.",
    ),
    Setting::new(
        ("Scene View", "Gizmos"),
        "game_culling",
        "Game Camera's Culling",
        Kind::Toggle,
        "While playing, the Scene view draws only what the game's camera draws: what its frustum, its draw \
         distances, its LODs and the depth in the Game view leave out is left out here too, with its frustum drawn. \
         While editing, the same for a selected camera. Off, the Scene view culls from its own camera. On by default.",
    ),
    Setting::new(
        ("Tools", "Editor Link"),
        "tools_may_change",
        "Let tools change the project",
        Kind::Toggle,
        "Programs reaching the editor through vox editor call and vox mcp may change the project, not only read it. A tool may turn this off for itself; only you can turn it back on.",
    ),
];

/// The most test players' windows networked Play opens.
pub const MOST_TEST_PLAYERS: u32 = 7;

/// How the editor's Play plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayAs {
    #[default]
    Offline,
    /// The editor hosts: it runs the game and plays it.
    Host,
    /// A dedicated server runs the game, with the editor its first player.
    Server,
}

/// The network the editor's networked Play goes through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestNetwork {
    #[default]
    Perfect,
    Good,
    Average,
    Bad,
}

impl TestNetwork {
    pub const ALL: [TestNetwork; 4] = [TestNetwork::Perfect, TestNetwork::Good, TestNetwork::Average, TestNetwork::Bad];

    pub fn name(self) -> &'static str {
        match self {
            TestNetwork::Perfect => "Perfect",
            TestNetwork::Good => "Good",
            TestNetwork::Average => "Average",
            TestNetwork::Bad => "Bad",
        }
    }

    pub fn conditions(self) -> voxear_net::Conditions {
        match self {
            TestNetwork::Perfect => voxear_net::Conditions::PERFECT,
            TestNetwork::Good => voxear_net::Conditions::GOOD,
            TestNetwork::Average => voxear_net::Conditions::AVERAGE,
            TestNetwork::Bad => voxear_net::Conditions::BAD,
        }
    }
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
    /// The Scene view's camera flies faster the longer it flies without a
    /// stop.
    pub fly_speeds_up: bool,
    pub show_stats: bool,
    pub show_axis_gizmo: bool,
    /// Lights and cameras are drawn in the Scene view, and what the selected
    /// one reaches.
    pub show_gizmos: bool,
    /// Which gizmos the Gizmos menu leaves on: lights' and cameras', and
    /// the behaviour types whose gizmos it hides, by full name.
    pub light_gizmos: bool,
    pub camera_gizmos: bool,
    pub audio_gizmos: bool,
    /// Every Vehicle's wheels drawn while playing: their suspension,
    /// contacts and slip.
    pub vehicle_gizmos: bool,
    /// Where things can walk in each Block World near the camera, and each
    /// agent's way.
    pub nav_gizmos: bool,
    /// Whether world layers are left out of worlds' ground, to see it bare.
    pub bare_ground: bool,
    /// The Scene view culls and picks LODs from the game's camera while
    /// playing, and a selected camera's while editing.
    pub game_culling: bool,
    pub hidden_gizmos: Vec<String>,
    /// How the Scene view shades: lit, unlit, or as wireframe.
    pub scene_shading: usize,
    /// Prefabs in the Project panel open out to their Objs, as models open
    /// out to their parts.
    pub prefab_parts: bool,
    /// Play starts paused, to step through the first frames.
    pub start_paused: bool,
    /// How Play plays: offline, or networked as a host or a server's first
    /// player, with test players in windows of their own.
    pub multiplayer: PlayAs,
    /// How many test players' windows open with networked Play.
    pub test_players: u32,
    /// The network networked Play goes through, made as bad as asked.
    pub test_network: TestNetwork,
    pub save_before_play: bool,
    pub clear_console_on_play: bool,
    /// Play pauses when an error is logged.
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
    /// The code editor scripts and the project open in.
    pub code_editor: CodeEditor,
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
    /// Programs reaching the editor through its link, as `vox editor call`
    /// and `vox mcp` do, may change the project, not only read it.
    pub tools_may_change: bool,
    /// The folder the browser downloads into, watched for Megascans
    /// downloaded from Fab, which are imported when the editor gets the
    /// focus back (see `megascans`). None watches nothing.
    pub fab_downloads: Option<PathBuf>,
}

/// A Console filter saved by name: a search, and the
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
            fly_speeds_up: true,
            show_stats: true,
            show_axis_gizmo: true,
            show_gizmos: true,
            light_gizmos: true,
            camera_gizmos: true,
            audio_gizmos: true,
            vehicle_gizmos: false,
            nav_gizmos: false,
            bare_ground: false,
            game_culling: true,
            hidden_gizmos: Vec::new(),
            scene_shading: 0,
            prefab_parts: false,
            start_paused: false,
            multiplayer: PlayAs::Offline,
            test_players: 1,
            test_network: TestNetwork::Perfect,
            save_before_play: false,
            clear_console_on_play: false,
            error_pause: false,
            reload_while_playing: true,
            console_collapse: false,
            console_mine_only: true,
            console_namespaces: true,
            console_parameters: true,
            code_editor: CodeEditor::Whisk,
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
            tools_may_change: true,
            fab_downloads: None,
        }
    }
}

impl Settings {
    /// A setting's value, by its key in [`PREFERENCES`], as the window and
    /// `vox editor call` show it (see `setting_info`).
    pub fn value(&self, key: &str) -> Option<Json> {
        let choice = |options: &[(&'static str, &'static str)], word: &str| json!(shown_label(options, word));
        let colour = |hex: u32| json!(format!("#{hex:06x}"));
        Some(match key {
            "snap" => json!(SNAP_CHOICES[SNAPS.iter().position(|s| (s - self.snap).abs() < 1e-4).unwrap_or(1)].0),
            "handle_position" => json!(if self.centred { "Center" } else { "Pivot" }),
            "handle_space" => json!(if self.local { "Local" } else { "Global" }),
            "fly_speed" => json!(self.fly_speed),
            "fly_speeds_up" => json!(self.fly_speeds_up),
            "show_stats" => json!(self.show_stats),
            "show_axis_gizmo" => json!(self.show_axis_gizmo),
            "show_gizmos" => json!(self.show_gizmos),
            "light_gizmos" => json!(self.light_gizmos),
            "camera_gizmos" => json!(self.camera_gizmos),
            "audio_gizmos" => json!(self.audio_gizmos),
            "vehicle_gizmos" => json!(self.vehicle_gizmos),
            "nav_gizmos" => json!(self.nav_gizmos),
            "bare_ground" => json!(self.bare_ground),
            "game_culling" => json!(self.game_culling),
            "scene_shading" => json!(SHADING.get(self.scene_shading).map_or("Lit", |(label, _)| *label)),
            "prefab_parts" => json!(self.prefab_parts),
            "start_paused" => json!(self.start_paused),
            "save_before_play" => json!(self.save_before_play),
            "clear_console_on_play" => json!(self.clear_console_on_play),
            "error_pause" => json!(self.error_pause),
            "reload_while_playing" => json!(self.reload_while_playing),
            "multiplayer" => json!(PLAY_AS[self.multiplayer as usize].0),
            "test_players" => json!(self.test_players),
            "test_network" => json!(self.test_network.name()),
            "console_collapse" => json!(self.console_collapse),
            "console_mine_only" => json!(self.console_mine_only),
            "console_namespaces" => json!(self.console_namespaces),
            "console_parameters" => json!(self.console_parameters),
            "console_row_height" => choice(ROW_HEIGHTS, &format!("{}", self.console_row_height.round())),
            "console_tint" => json!(self.console_tint),
            "console_stripes" => json!(self.console_stripes),
            "console_category" => json!(CATEGORY_SHOWN.get(self.console_category).map_or("Name", |(label, _)| *label)),
            "console_kept" => choice(KEPT_LINES, &self.console_kept.to_string()),
            "code_editor" => json!(self.code_editor.name()),
            "theme" => json!(THEME_CHOICES.get(self.theme).map_or("Custom", |(label, _)| *label)),
            "theme_base" => colour(self.theme_base),
            "theme_accent" => colour(self.theme_accent),
            "theme_contrast" => json!(self.theme_contrast),
            "tools_may_change" => json!(self.tools_may_change),
            "fab_downloads" => json!(self.fab_downloads.as_ref().map(|folder| folder.display().to_string())),
            _ => return None,
        })
    }

    /// Sets a setting to a value `setting_info::check` has passed, keeping a
    /// number inside its range. Returns whether it changed.
    pub fn set_value(&mut self, key: &str, value: &Json) -> bool {
        let before = self.clone();
        let on = value.as_bool().unwrap_or_default();
        let number = value.as_f64().unwrap_or_default() as f32;
        let word = value.as_str().unwrap_or_default();
        let at = |options: &[(&'static str, &'static str)]| options.iter().position(|(label, _)| *label == word);
        let hex = || word.strip_prefix('#').and_then(|hex| u32::from_str_radix(hex, 16).ok());
        match key {
            "snap" => self.snap = SNAPS[at(SNAP_CHOICES).unwrap_or(1)],
            "handle_position" => self.centred = word == "Center",
            "handle_space" => self.local = word == "Local",
            "fly_speed" => self.fly_speed = number.clamp(0.25, 250.0),
            "fly_speeds_up" => self.fly_speeds_up = on,
            "show_stats" => self.show_stats = on,
            "show_axis_gizmo" => self.show_axis_gizmo = on,
            "show_gizmos" => self.show_gizmos = on,
            "light_gizmos" => self.light_gizmos = on,
            "camera_gizmos" => self.camera_gizmos = on,
            "audio_gizmos" => self.audio_gizmos = on,
            "vehicle_gizmos" => self.vehicle_gizmos = on,
            "nav_gizmos" => self.nav_gizmos = on,
            "bare_ground" => self.bare_ground = on,
            "game_culling" => self.game_culling = on,
            "scene_shading" => self.scene_shading = at(SHADING).unwrap_or(0),
            "prefab_parts" => self.prefab_parts = on,
            "tools_may_change" => self.tools_may_change = on,
            "start_paused" => self.start_paused = on,
            "save_before_play" => self.save_before_play = on,
            "clear_console_on_play" => self.clear_console_on_play = on,
            "error_pause" => self.error_pause = on,
            "reload_while_playing" => self.reload_while_playing = on,
            "multiplayer" => {
                self.multiplayer = [PlayAs::Offline, PlayAs::Host, PlayAs::Server][at(PLAY_AS).unwrap_or(0)]
            }
            "test_players" => self.test_players = (number.round() as u32).min(MOST_TEST_PLAYERS),
            "test_network" => self.test_network = TestNetwork::ALL[at(NETWORKS).unwrap_or(0)],
            "console_collapse" => self.console_collapse = on,
            "console_mine_only" => self.console_mine_only = on,
            "console_namespaces" => self.console_namespaces = on,
            "console_parameters" => self.console_parameters = on,
            "console_row_height" => {
                self.console_row_height = kept_word(ROW_HEIGHTS, word).and_then(|n| n.parse().ok()).unwrap_or(20.0)
            }
            "console_tint" => self.console_tint = on,
            "console_stripes" => self.console_stripes = on,
            "console_category" => self.console_category = at(CATEGORY_SHOWN).unwrap_or(2),
            "console_kept" => {
                self.console_kept = kept_word(KEPT_LINES, word).and_then(|n| n.parse().ok()).unwrap_or(10_000)
            }
            "code_editor" => {
                let index = CodeEditor::ALL.iter().position(|editor| editor.name() == word).unwrap_or(0);
                self.code_editor = CodeEditor::at(index);
            }
            "theme" => self.set_theme(at(THEME_CHOICES).unwrap_or(0)),
            // Changing one of the three makes the theme Custom.
            "theme_base" => {
                if let Some(c) = hex().filter(|c| *c != self.theme_base) {
                    (self.theme_base, self.theme) = (c, 2);
                }
            }
            "theme_accent" => {
                if let Some(c) = hex().filter(|c| *c != self.theme_accent) {
                    (self.theme_accent, self.theme) = (c, 2);
                }
            }
            "theme_contrast" if number.clamp(0.0, 100.0) != self.theme_contrast => {
                (self.theme_contrast, self.theme) = (number.clamp(0.0, 100.0), 2)
            }
            "fab_downloads" => self.fab_downloads = value.as_str().map(PathBuf::from),
            _ => {}
        }
        *self != before
    }

    /// A folder setting's folder, if one is chosen.
    pub fn folder(&self, key: &str) -> Option<&Path> {
        match key {
            "fab_downloads" => self.fab_downloads.as_deref(),
            _ => None,
        }
    }

    /// Sets a folder setting, or clears it. Returns whether it changed.
    pub fn set_folder(&mut self, key: &str, folder: Option<PathBuf>) -> bool {
        let kept = match key {
            "fab_downloads" => &mut self.fab_downloads,
            _ => return false,
        };
        let changed = *kept != folder;
        *kept = folder;
        changed
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
    pub fn describe(&self, setting: &Setting) -> String {
        let value = self.value(setting.key).unwrap_or(Json::Null);
        crate::setting_info::describe(setting, &value)
    }
}

/// The setting of Preferences whose key this is.
pub fn info(key: &str) -> Option<&'static Setting> {
    PREFERENCES.iter().find(|s| s.key == key)
}

/// A layout saved with Save Layout As: its name, and the arrangement it
/// starts with and goes back to on Reset Layout.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedLayout {
    pub name: String,
    pub start: String,
}

/// A window written in the game's C# that a layout names: its panel, by id
/// (`window-1`), and the window's type, by its full name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeptScriptWindow {
    pub panel: String,
    pub window: String,
}

/// A Scene view opened from a window's menu, as it was left: its panel, by
/// id (`scene-2`), where its camera was and which way it looked, and how it
/// drew.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeptSceneView {
    pub panel: String,
    pub position: [f32; 3],
    /// As a quaternion's x, y, z and w.
    pub rotation: [f32; 4],
    /// Lit, Unlit, Wireframe, Lit Wireframe, Heat or Lightmap Density, by index.
    pub shading: usize,
    pub show_gizmos: bool,
}

impl Default for KeptSceneView {
    fn default() -> Self {
        Self { panel: String::new(), position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], shading: 0, show_gizmos: true }
    }
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
    /// The Scene views opened from a window's menu that a layout names, so
    /// they open again as they were left with the layout.
    pub scene_views: Vec<KeptSceneView>,
    /// The game's own windows a layout names, so they open again with it.
    pub script_windows: Vec<KeptScriptWindow>,
    /// How big the Project panel's tiles are, as its slider was left, from
    /// 0 for rows to 1; `None` for the starting size.
    pub tile_size: Option<f32>,
    /// The blocks the brush last built with, by GUID, most recent first:
    /// the order a picker of blocks offers them in.
    pub recent_blocks: Vec<String>,
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

    /// Whether it is kept: else the editor writes nothing of its own, as
    /// in tests and pictures.
    pub fn remembers(&self) -> bool {
        self.path.is_some()
    }

    /// Notes that the palette ran what `key` names, most recent first.
    pub fn ran(&mut self, key: &str) {
        self.recent.retain(|k| k != key);
        self.recent.insert(0, key.to_string());
        self.recent.truncate(RECENT);
        self.save();
    }

    /// Notes that the brush built with the block `guid` names, most recent
    /// first.
    pub fn built_with(&mut self, guid: &str) {
        self.recent_blocks.retain(|known| known != guid);
        self.recent_blocks.insert(0, guid.to_string());
        self.recent_blocks.truncate(RECENT_BLOCKS);
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
        assert!(memory.settings.set_value("snap", &json!("1 m")));
        assert!(!memory.settings.set_value("snap", &json!("1 m")), "the same again is no change");
        assert!(memory.settings.set_value("fly_speed", &json!(1000.0)));
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
        assert!(themed.set_value("theme", &json!("Light")));
        assert_eq!(themed.theme(), Theme::LIGHT, "a theme brings its choices");
        assert!(themed.set_value("theme_accent", &json!("#e2a04a")));
        assert_eq!(themed.describe(info("theme").unwrap()), "Custom", "changing one makes it Custom");
        assert_eq!(themed.theme().base, Theme::LIGHT.base, "keeping the rest");
        assert!(!themed.set_value("theme_contrast", &json!(50.0)), "the same contrast is no change");

        let forgetful = Memory::forgetful();
        forgetful.save();
        assert_eq!(forgetful.settings, Settings::default());
    }

    #[test]
    fn a_folder_setting_is_kept_as_its_path_and_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let mut memory = Memory::open(dir.path());
        let watched = info("fab_downloads").unwrap();
        assert_eq!((watched.kind, memory.settings.describe(watched).as_str()), (Kind::Folder, "None"));
        let downloads = dir.path().join("Downloads");
        assert!(memory.settings.set_folder("fab_downloads", Some(downloads.clone())));
        assert!(!memory.settings.set_folder("fab_downloads", Some(downloads.clone())), "the same again is no change");
        memory.save();

        let mut again = Memory::open(dir.path());
        assert_eq!(again.settings.folder("fab_downloads"), Some(downloads.as_path()));
        assert_eq!(again.settings.describe(watched), downloads.display().to_string());
        assert!(again.settings.set_folder("fab_downloads", None));
        assert_eq!(again.settings.fab_downloads, None);
    }

    #[test]
    fn every_preference_reads_back_what_it_is_set_to_and_has_a_tooltip() {
        let defaults = Settings::default();
        for setting in PREFERENCES {
            assert!(CATEGORIES.iter().any(|c| c.name == setting.category), "{} has a category", setting.key);
            assert!(setting.tip.len() > 20 && setting.tip.ends_with('.'), "{} has a tooltip", setting.key);
            if !setting.settable() {
                continue;
            }
            let value = defaults.value(setting.key).unwrap_or_else(|| panic!("{} reads", setting.key));
            let checked = crate::setting_info::check(setting, &value);
            assert!(checked.is_ok() || value.is_null(), "{}'s default {value} is a value for it", setting.key);
            let mut changed = defaults.clone();
            assert!(!changed.set_value(setting.key, &value), "{} set to what it is is no change", setting.key);
        }
        let mut changed = defaults.clone();
        assert!(changed.set_value("console_row_height", &json!("Tall")) && changed.console_row_height == 24.0);
        assert!(changed.set_value("test_network", &json!("Bad")) && changed.test_network == TestNetwork::Bad);
        assert!(changed.set_value("multiplayer", &json!("Host")) && changed.multiplayer == PlayAs::Host);
        assert_eq!(changed.value("console_kept"), Some(json!("10,000")));
    }
}
