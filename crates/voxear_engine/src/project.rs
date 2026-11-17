use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::bundle::Compression;
use voxear_audio::ReverbMode;

pub const PROJECT_FILE: &str = "voxear.toml";
/// What an engine install is, at its root: its version, channel and platform.
pub const ENGINE_FILE: &str = "engine.toml";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub project: ProjectInfo,
    #[serde(default)]
    pub content: ContentSettings,
    #[serde(default)]
    pub layers: LayerSettings,
    #[serde(default)]
    pub physics: PhysicsSettings,
    #[serde(default)]
    pub localization: LocalizationSettings,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub network: crate::net::NetworkSettings,
    #[serde(default)]
    pub floating_origin: crate::origin::OriginSettings,
    #[serde(default)]
    pub quality: QualitySettings,
    /// What making worlds of blocks may spend (see `voxear_terrain::budget`).
    #[serde(default)]
    pub world: voxear_terrain::WorldBudget,
    /// What Portals may spend (see [`PortalSettings`]).
    #[serde(default)]
    pub portals: PortalSettings,
    /// How long the game's fixed steps and frames may be (see [`TimeSettings`]).
    #[serde(default)]
    pub time: TimeSettings,
    /// Which input actions the game plays with (see [`InputSettings`]).
    #[serde(default)]
    pub input: InputSettings,
    /// The modules the project has, or none for a project made before
    /// there were modules (see [`crate::modules`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules: Option<crate::modules::Modules>,
}

impl ProjectFile {
    /// A project named `name` with every setting its default, as a new
    /// `voxear.toml` with only `[project]` in it reads.
    pub fn defaults(name: &str) -> Self {
        Self {
            project: ProjectInfo { name: name.to_string(), scripts: None, start_level: None, engine: None },
            content: ContentSettings::default(),
            layers: LayerSettings::default(),
            physics: PhysicsSettings::default(),
            localization: LocalizationSettings::default(),
            audio: AudioSettings::default(),
            network: crate::net::NetworkSettings::default(),
            floating_origin: crate::origin::OriginSettings::default(),
            quality: QualitySettings::default(),
            world: voxear_terrain::WorldBudget::default(),
            portals: PortalSettings::default(),
            time: TimeSettings::default(),
            input: InputSettings::default(),
            modules: None,
        }
    }
}

/// How long the game's steps may be. Physics, FixedUpdate and everything
/// else that must not depend on the frame rate run in fixed steps of
/// `fixed_step` seconds, as many as the time passed needs; a networked game
/// steps at its tick rate instead. A frame longer than `max_step` counts as
/// that long, so a hitch or a breakpoint does not throw everything forward,
/// and no frame runs more than `max_fixed_steps` fixed steps, the rest
/// dropped, so a slow machine does not fall further and further behind.
///
/// ```toml
/// [time]
/// fixed_step = 0.02       # 50 a second
/// max_step = 0.25
/// max_fixed_steps = 5
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TimeSettings {
    pub fixed_step: f32,
    pub max_step: f32,
    pub max_fixed_steps: u32,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self { fixed_step: 1.0 / 50.0, max_step: voxear_core::time::MAX_DELTA, max_fixed_steps: 5 }
    }
}

impl TimeSettings {
    /// Puts the steps into the engine, from its next frame.
    pub fn put(&self, engine: &mut crate::Engine) {
        engine.set_fixed_delta(self.fixed_step);
        engine.set_frame_limits(self.max_step, self.max_fixed_steps);
    }
}

/// The input actions the game plays with: an `.inputactions` file, by GUID
/// or path, or none for the project's first by path, or Voxear's own where
/// it has none.
///
/// ```toml
/// [input]
/// actions = "Input/Player.inputactions"
/// ```
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<String>,
}

/// How much the project lets the renderer spend. `particles` is the most
/// simulated particles every Particle System may have room for between
/// them (see `voxear_render::ParticleBudget`): those there is no room for
/// are not drawn.
///
/// ```toml
/// [quality]
/// particles = 250000
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QualitySettings {
    pub particles: u32,
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self { particles: voxear_render::DEFAULT_PARTICLE_BUDGET }
    }
}

/// How much the project lets Portals spend (see `crate::portals` and
/// `voxear_render::portals::PortalQuality`).
///
/// ```toml
/// [portals]
/// views = 4               # far sides drawn in a view at most, the nearest first
/// depth = 1               # 2 draws the far side of a Portal seen through a Portal too
/// shadow_distance = 30.0  # the sun casts shadows on a far side within this of its opening
/// far_side_distance = 96.0  # how far round a far side a world without end makes its own blocks
/// water = true            # whether the water of a far side's worlds of blocks is drawn
/// draw_distance = 0.0     # how far ahead a far side is drawn; 0 for as far as the world is
/// view_distance = 96.0    # how far off a Portal without its own still shows its far side
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PortalSettings {
    pub views: u32,
    pub depth: u32,
    pub shadow_distance: f32,
    pub far_side_distance: f32,
    pub water: bool,
    pub draw_distance: f32,
    pub view_distance: f32,
}

impl Default for PortalSettings {
    fn default() -> Self {
        let quality = voxear_render::portals::PortalQuality::default();
        Self {
            views: quality.views,
            depth: quality.depth,
            shadow_distance: quality.shadow_distance,
            far_side_distance: quality.far_side_distance,
            water: quality.water,
            draw_distance: quality.draw_distance,
            view_distance: quality.view_distance,
        }
    }
}

impl PortalSettings {
    /// Puts the project's settings where the renderer and the engine read them.
    pub fn put(&self, world: &mut voxear_ecs::World) {
        world.insert_resource(voxear_render::portals::PortalQuality {
            views: self.views.min(voxear_render::portals::MAX_PORTAL_QUADS as u32),
            depth: self.depth.min(voxear_render::portals::MAX_PORTAL_DEPTH),
            shadow_distance: self.shadow_distance.max(0.0),
            far_side_distance: self.far_side_distance.clamp(32.0, 4096.0),
            water: self.water,
            draw_distance: self.draw_distance.max(0.0),
            view_distance: self.view_distance.max(0.0),
        });
    }
}

impl QualitySettings {
    /// Puts the project's budgets where the renderer reads them.
    pub fn put(&self, world: &mut voxear_ecs::World) {
        world.insert_resource(voxear_render::ParticleBudget(self.particles));
    }
}

/// Sound for the whole project: whether 3D sounds start out heard as
/// through headphones, by Steam Audio's HRTF, and ringing as the
/// listener's room does, by how long it rings or by its echoes, and
/// whether player builds take Steam Audio with them. A game's options
/// usually let the player choose headphones, with `Audio.Headphones`.
///
/// ```toml
/// [audio]
/// headphones = true
/// reverb = true
/// reverb_mode = "convolution"
/// steam_audio = false
/// ray_traced = true
/// ray_quality = "high"
/// ray_budget = 0.3
/// ```
///
/// With `ray_traced`, sound is heard through the scene the GPU traces for
/// the GI, blocks and terrain too, within `ray_budget` milliseconds of GPU
/// time a frame (see `crate::sound_tracing`); a level may set its own.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSettings {
    pub headphones: bool,
    pub reverb: bool,
    #[serde(with = "ReverbModeName")]
    pub reverb_mode: ReverbMode,
    pub steam_audio: bool,
    pub ray_traced: bool,
    pub ray_quality: crate::sound_tracing::RayQualityName,
    pub ray_budget: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            headphones: false,
            reverb: false,
            reverb_mode: ReverbMode::Parametric,
            steam_audio: true,
            ray_traced: false,
            ray_quality: crate::sound_tracing::RayQualityName::Medium,
            ray_budget: crate::sound_tracing::DEFAULT_BUDGET,
        }
    }
}

impl AudioSettings {
    /// Its ray-traced audio, as the game takes it.
    pub fn sound_tracing(&self) -> crate::sound_tracing::SoundTracing {
        crate::sound_tracing::SoundTracing {
            on: self.ray_traced,
            quality: self.ray_quality.into(),
            budget: self.ray_budget.max(0.0),
        }
    }
}

/// A [`ReverbMode`] as `voxear.toml` and a player's settings write it.
#[derive(Deserialize, Serialize)]
#[serde(remote = "ReverbMode", rename_all = "lowercase")]
pub(crate) enum ReverbModeName {
    Parametric,
    Convolution,
}

/// The languages the game is in, by their codes, the one it is written in
/// first. Its text is in string tables (see `voxear_localization`). A
/// language may name fonts, by GUID or path, that its text falls back to
/// for the letters a style's own font has not got: its own, then those of
/// the languages it falls back to, then Voxear's faces, then the system's.
///
/// ```toml
/// [localization]
/// languages = ["en", "fr", "pt-BR", "ja"]
///
/// [localization.fonts]
/// ja = ["Fonts/NotoSansJP-Regular.otf"]
/// ```
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalizationSettings {
    pub languages: Vec<String>,
    pub fonts: std::collections::BTreeMap<String, Vec<String>>,
}

/// The project's layers' names, by number from layer 0, as Project
/// Settings' Tags and Layers name them; an empty name is a layer not used.
///
/// ```toml
/// [layers]
/// names = ["Default", "Ignore Raycast", "Player", "Enemy"]
/// ```
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerSettings {
    pub names: Vec<String>,
}

/// Physics for the whole project: the pull of gravity a world starts with,
/// in 3D and in 2D, in metres per second per second, which scripts may
/// change as the game runs; and the pairs of layers whose Objs pass through
/// each other, every other pair colliding.
///
/// ```toml
/// [physics]
/// gravity = [0.0, -9.81, 0.0]
/// gravity_2d = [0.0, -9.81]
/// ignore = [["Player", "Player"], ["Enemy", "Pickup"]]
/// ```
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsSettings {
    pub gravity: [f32; 3],
    pub gravity_2d: [f32; 2],
    pub ignore: Vec<[String; 2]>,
}

impl Default for PhysicsSettings {
    fn default() -> Self {
        Self { gravity: [0.0, -9.81, 0.0], gravity_2d: [0.0, -9.81], ignore: Vec::new() }
    }
}

/// The gravity a physics scene starts with, from the project's
/// `[physics]`; a scene made without it has Earth's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartingGravity {
    pub three: voxear_core::math::Vec3,
    pub two: voxear_core::math::Vec2,
}

impl PhysicsSettings {
    /// Puts the gravity where the engine finds it as it makes its physics
    /// scenes.
    pub fn put(&self, world: &mut voxear_ecs::World) {
        let finite = |n: f32| if n.is_finite() { n } else { 0.0 };
        world.insert_resource(StartingGravity {
            three: voxear_core::math::Vec3::from_array(self.gravity.map(finite)),
            two: voxear_core::math::Vec2::from_array(self.gravity_2d.map(finite)),
        });
    }
}

/// What the content build treats as roots, beside every level and prefab.
///
/// ```toml
/// [content]
/// include = ["Textures/Icons/**"]   # loaded by paths the build cannot see
/// exclude = ["Prefabs/Debug/**"]    # not shipped unless something shipped uses them
/// compression = "lz4"               # or "zstd", smaller, or "none"
/// ```
///
/// Excluding only stops an asset being a root of its own: one that a shipped
/// scene uses, or names in a behaviour field, still ships.
///
/// Patterns are project paths where `*` matches within a folder, `**` any
/// number of folders, and `?` one character.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContentSettings {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Bundles smaller than this merge into one that every one of their
    /// roots opens anyway, saving a file at no cost. 0 never merges.
    pub merge_below_kb: u64,
    /// Bundles larger than this split, so patches and downloads stay small.
    pub split_above_mb: u64,
    /// How bundles are compressed: LZ4, quick to load, by default.
    pub compression: Compression,
    /// For a dedicated server: what only shows or sounds is left out, unless
    /// its `.meta` keeps it (see `BuildInclusion`), and models ship without
    /// their pictures. Set by a server build profile, not `voxear.toml`.
    #[serde(skip)]
    pub server: bool,
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            merge_below_kb: 16,
            split_above_mb: 64,
            compression: Compression::default(),
            server: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub name: String,
    /// The game's C# project, relative to the project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<PathBuf>,
    /// The level to load first: a `.level` path or GUID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_level: Option<String>,
    /// The engine version the project uses, as the launcher pins it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

pub struct Project {
    pub root: PathBuf,
    pub file: ProjectFile,
}

impl Project {
    /// Opens a project folder, or its `voxear.toml` directly.
    pub fn open(path: &Path) -> Result<Self> {
        let manifest = if path.is_dir() { path.join(PROJECT_FILE) } else { path.to_path_buf() };
        let text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("no Voxear project at {} (expected {PROJECT_FILE})", path.display()))?;
        let mut file: ProjectFile = toml::from_str(&text).with_context(|| format!("reading {}", manifest.display()))?;
        // A preset's numbers are its own, and a Custom budget's within bounds.
        file.world = file.world.checked();
        let root = manifest.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(pinned) = file.project.engine.as_deref().filter(|&pinned| pinned != engine_version()) {
            tracing::warn!(
                "{} is pinned to Voxear {pinned}, but this is Voxear {}; the launcher opens it with the version it pins",
                file.project.name,
                engine_version()
            );
        }
        Ok(Self { root, file })
    }

    pub fn name(&self) -> &str {
        &self.file.project.name
    }

    /// The project's layers, and which collide. A pair naming a layer the
    /// project does not have is left out, with a warning.
    pub fn layers(&self) -> voxear_scene::Layers {
        let mut layers = voxear_scene::Layers::named(&self.file.layers.names);
        for [a, b] in &self.file.physics.ignore {
            match (layers.find(a), layers.find(b)) {
                (Some(a), Some(b)) => layers.set_collide(a, b, false),
                _ => tracing::warn!("{PROJECT_FILE}: [physics] ignores {a} and {b}, but there is no such layer"),
            }
        }
        layers
    }

    /// Writes the project's layers into `voxear.toml`: each one's name, up
    /// to the last named, and the named pairs that do not collide, by name.
    /// The rest of the file is kept as it is, comments and all.
    pub fn set_layers(&mut self, layers: &voxear_scene::Layers) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let used = layers.names().iter().rposition(|name| !name.is_empty()).map_or(0, |last| last + 1);
        let names: Vec<String> = layers.names()[..used].to_vec();
        let mut ignore: Vec<[String; 2]> = Vec::new();
        for (a, first) in layers.used() {
            for (b, second) in layers.used().filter(|(b, _)| *b >= a) {
                if !layers.collide(a, b) {
                    ignore.push([first.to_string(), second.to_string()]);
                }
            }
        }
        let list =
            |items: &mut dyn Iterator<Item = toml_edit::Value>| toml_edit::value(toml_edit::Array::from_iter(items));
        let only_default = names.len() <= 1;
        set_key(&mut document, "layers", "names", (!only_default).then(|| list(&mut names.iter().map(|n| n.into()))));
        let pairs = (!ignore.is_empty()).then(|| {
            list(&mut ignore.iter().map(|[a, b]| toml_edit::Value::Array(toml_edit::Array::from_iter([a, b]))))
        });
        set_key(&mut document, "physics", "ignore", pairs);
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.layers.names = if only_default { Vec::new() } else { names };
        self.file.physics.ignore = ignore;
        Ok(())
    }

    /// Writes the fonts a language's text falls back to into `voxear.toml`,
    /// each by GUID or path in order, under `[localization.fonts]`; none
    /// takes the language's line out, and the table with its last. The rest
    /// of the file is kept as it is, comments and all.
    pub fn set_language_fonts(&mut self, language: &str, fonts: &[String]) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let localization = document.entry("localization").or_insert_with(toml_edit::table);
        let Some(localization) = localization.as_table_like_mut() else {
            anyhow::bail!("[localization] in {} is not a table", path.display());
        };
        if fonts.is_empty() {
            let emptied = localization.get_mut("fonts").and_then(|t| t.as_table_like_mut()).is_some_and(|table| {
                table.remove(language);
                table.is_empty()
            });
            if emptied {
                localization.remove("fonts");
            }
        } else {
            let table = localization.entry("fonts").or_insert_with(|| {
                let mut table = toml_edit::Table::new();
                table.set_implicit(false);
                toml_edit::Item::Table(table)
            });
            let Some(table) = table.as_table_like_mut() else {
                anyhow::bail!("[localization.fonts] in {} is not a table", path.display());
            };
            table.insert(language, toml_edit::value(toml_edit::Array::from_iter(fonts.iter())));
        }
        if localization.is_empty() {
            document.remove("localization");
        }
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        match fonts.is_empty() {
            true => drop(self.file.localization.fonts.remove(language)),
            false => drop(self.file.localization.fonts.insert(language.to_string(), fonts.to_vec())),
        }
        Ok(())
    }

    /// Writes the project's floating origin into `voxear.toml`: only what
    /// differs from the defaults, so a project that never switches it on
    /// has no `[floating_origin]` at all. The rest of the file is kept as it
    /// is, comments and all.
    pub fn set_floating_origin(&mut self, settings: &crate::origin::OriginSettings) -> Result<()> {
        use crate::origin::OriginSettings;
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let defaults = OriginSettings::default();
        let mode = toml::Value::try_from(settings.mode)?.as_str().unwrap_or_default().to_string();
        let mode = (settings.mode != defaults.mode).then(|| toml_edit::value(mode));
        let distance = (settings.distance != defaults.distance).then(|| toml_edit::value(f64::from(settings.distance)));
        let step = (settings.step != defaults.step).then(|| toml_edit::value(i64::from(settings.step)));
        set_key(&mut document, "floating_origin", "mode", mode);
        set_key(&mut document, "floating_origin", "distance", distance);
        set_key(&mut document, "floating_origin", "step", step);
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.floating_origin = settings.clone();
        Ok(())
    }

    /// Writes the project's quality settings into `voxear.toml`: only what
    /// differs from the defaults. The rest of the file is kept as it is.
    pub fn set_quality(&mut self, settings: &QualitySettings) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let particles = settings.particles.min(voxear_render::MAX_PARTICLE_BUDGET);
        let written = (particles != QualitySettings::default().particles).then(|| toml_edit::value(i64::from(particles)));
        set_key(&mut document, "quality", "particles", written);
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.quality = QualitySettings { particles };
        Ok(())
    }

    /// Writes the project's view distance for Portals without their own into
    /// `voxear.toml`'s `[portals]`, left out where it is the default. The
    /// rest of the file is kept as it is.
    pub fn set_portal_view_distance(&mut self, distance: f32) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let distance =
            if distance.is_finite() { distance.clamp(0.0, 100_000.0) } else { PortalSettings::default().view_distance };
        let written =
            (distance != PortalSettings::default().view_distance).then(|| toml_edit::value(f64::from(distance)));
        set_key(&mut document, "portals", "view_distance", written);
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.portals.view_distance = distance;
        Ok(())
    }

    /// Writes what making worlds of blocks may spend into `voxear.toml`'s
    /// `[world]`: its quality where it is not High, and every number only
    /// where it is Custom, as a preset's are its own. The rest of the file is
    /// kept as it is.
    pub fn set_world(&mut self, budget: &voxear_terrain::WorldBudget) -> Result<()> {
        use voxear_terrain::{WorldBudget, WorldQuality};
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let budget = budget.checked();
        let quality = toml::Value::try_from(budget.quality)?.as_str().unwrap_or_default().to_string();
        let quality = (budget.quality != WorldBudget::default().quality).then(|| toml_edit::value(quality));
        set_key(&mut document, "world", "quality", quality);
        let custom = budget.quality == WorldQuality::Custom;
        let whole = |n: u32| custom.then(|| toml_edit::value(i64::from(n)));
        let metres = |m: f32| custom.then(|| toml_edit::value(f64::from(m)));
        set_key(&mut document, "world", "threads", whole(budget.threads));
        set_key(&mut document, "world", "columns", whole(budget.columns));
        set_key(&mut document, "world", "upload", whole(budget.upload));
        set_key(&mut document, "world", "view_distance", metres(budget.view_distance));
        set_key(&mut document, "world", "far_distance", metres(budget.far_distance));
        set_key(&mut document, "world", "near_detail", metres(budget.near_detail));
        set_key(&mut document, "world", "far_detail", metres(budget.far_detail));
        set_key(&mut document, "world", "plant_detail", metres(budget.plant_detail));
        set_key(&mut document, "world", "far_block_pixels", metres(budget.far_block_pixels));
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.world = budget;
        Ok(())
    }

    /// The modules the project has.
    pub fn modules(&self) -> crate::modules::Modules {
        self.file.modules.unwrap_or_else(crate::modules::Modules::older_project)
    }

    /// Switches a module on or off in `voxear.toml`, writing out every
    /// module's choice so the file says plainly what the project has. The
    /// rest of the file is kept as it is, comments and all.
    pub fn set_module(&mut self, module: crate::modules::Module, on: bool) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let mut modules = self.modules();
        modules.set(module, on);
        for each in crate::modules::Module::ALL {
            set_key(&mut document, "modules", each.key(), Some(toml_edit::value(modules.has(each))));
        }
        std::fs::write(&path, document.to_string()).with_context(|| format!("writing {}", path.display()))?;
        self.file.modules = Some(modules);
        Ok(())
    }

    /// One setting as `voxear.toml` writes it, by its table and key
    /// (`audio.reverb_mode`), whether the file writes it or not: what the
    /// project plays with. None for a key no setting has.
    pub fn setting(&self, key: &str) -> Option<toml::Value> {
        let file = toml::Value::try_from(&self.file).ok()?;
        let (table, name) = key.split_once('.')?;
        file.get(table)?.get(name).cloned()
    }

    /// What a setting is where the file writes none.
    pub fn default_setting(key: &str) -> Option<toml::Value> {
        let file = toml::Value::try_from(ProjectFile::defaults("")).ok()?;
        let (table, name) = key.split_once('.')?;
        file.get(table)?.get(name).cloned()
    }

    /// Writes one setting into `voxear.toml` by its table and key: its
    /// default, or none, takes the key out, and its table with its last, so
    /// the file holds only what differs. The rest of the file is kept as it
    /// is, comments and all. Refused, with the file as it was, for a value
    /// the project could not be read with.
    pub fn set_setting(&mut self, key: &str, value: Option<toml::Value>) -> Result<()> {
        let (table, name) = key.split_once('.').with_context(|| format!("`{key}` is not a table and a key"))?;
        let default = Self::default_setting(key);
        let value = value.filter(|value| default.as_ref().is_none_or(|default| !same_setting(value, default)));
        let path = self.root.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut document: toml_edit::DocumentMut =
            text.parse().with_context(|| format!("reading {}", path.display()))?;
        let item = match value {
            Some(value) => {
                let written: toml_edit::Value =
                    value.to_string().parse().with_context(|| format!("{value} is not a value for {key}"))?;
                Some(toml_edit::value(written))
            }
            None => None,
        };
        set_key(&mut document, table, name, item);
        let written = document.to_string();
        let mut file: ProjectFile = toml::from_str(&written).with_context(|| format!("{key} cannot be that"))?;
        file.world = file.world.checked();
        std::fs::write(&path, written).with_context(|| format!("writing {}", path.display()))?;
        self.file = file;
        Ok(())
    }

    pub fn scripts_project(&self) -> Option<PathBuf> {
        self.file.project.scripts.as_ref().map(|p| self.root.join(p))
    }

    /// Where the scripts project builds to (set by its csproj).
    pub fn scripts_assembly(&self, configuration: &str) -> Option<PathBuf> {
        let csproj = self.scripts_project()?;
        let name = csproj.file_stem()?.to_string_lossy().into_owned();
        Some(self.root.join(".voxear").join("Scripts").join(configuration).join(format!("{name}.dll")))
    }

    /// The scripts' assembly as a build makes it, in `configuration`, in a
    /// folder of its own apart from the editor's.
    pub fn built_scripts_assembly(&self, configuration: &str) -> Option<PathBuf> {
        let csproj = self.scripts_project()?;
        let name = csproj.file_stem()?.to_string_lossy().into_owned();
        Some(self.root.join(".voxear").join("Scripts").join("Build").join(configuration).join(format!("{name}.dll")))
    }
}

/// This engine's version: a release's, as its packaging names it, or the
/// workspace's for a build from source.
pub fn engine_version() -> &'static str {
    option_env!("VOXEAR_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The engine's own files: an install, or the source checkout a build from
/// source runs from. `VOXEAR_HOME` names the install, as the launcher sets
/// it; without it, a program in an install's `bin/` finds the install above
/// it by its `engine.toml`.
pub fn engine_root() -> PathBuf {
    if let Some(home) = std::env::var_os("VOXEAR_HOME") {
        return PathBuf::from(home);
    }
    let exe = std::env::current_exe().ok();
    let installed = exe.as_deref().and_then(Path::parent).and_then(Path::parent).filter(|home| is_install(home));
    match installed {
        Some(home) => home.to_path_buf(),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    }
}

/// Whether a folder is an engine install, rather than a source checkout.
pub fn is_install(root: &Path) -> bool {
    root.join(ENGINE_FILE).is_file()
}

/// The engine's manual, the Markdown pages VoxDocs shows after a project's
/// own: the `manual/` folder of an install, or of the checkout.
pub fn manual_folder() -> PathBuf {
    engine_root().join("manual")
}

/// `Voxear.Core.dll`: an install's, in its `managed/` folder, or a checkout's
/// build of it in this configuration.
pub fn core_assembly(configuration: &str) -> PathBuf {
    let root = engine_root();
    if is_install(&root) {
        return root.join("managed").join("Voxear.Core.dll");
    }
    root.join("target").join("managed").join(configuration).join("Voxear.Core.dll")
}

/// Whether two settings are the same, numbers as closely as the engine
/// keeps them: an `f32` read back is not quite the number written.
fn same_setting(a: &toml::Value, b: &toml::Value) -> bool {
    use toml::Value;
    let number = |value: &Value| match value {
        Value::Float(n) => Some(*n as f32),
        Value::Integer(n) => Some(*n as f32),
        _ => None,
    };
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_setting(a, b)),
        _ => match (number(a), number(b)) {
            (Some(a), Some(b)) => a == b,
            _ => a == b,
        },
    }
}

/// Sets a key in a table of the file, making the table when it has none,
/// or takes it out, and the table with it once it is empty.
fn set_key(document: &mut toml_edit::DocumentMut, table: &str, key: &str, value: Option<toml_edit::Item>) {
    match value {
        Some(value) => {
            let section = document.entry(table).or_insert_with(toml_edit::table);
            if let Some(section) = section.as_table_like_mut() {
                section.insert(key, value);
            }
        }
        None => {
            let emptied = document.get_mut(table).and_then(|t| t.as_table_like_mut()).is_some_and(|section| {
                section.remove(key);
                section.is_empty()
            });
            if emptied {
                document.remove(table);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_languages_fonts_are_written_under_localization_and_taken_out_again() {
        let dir = tempfile::tempdir().unwrap();
        let text = "# My game\n[project]\nname = \"Fonts\"\n\n[localization]\nlanguages = [\"en\", \"ja\"]\n";
        std::fs::write(dir.path().join(PROJECT_FILE), text).unwrap();
        let mut project = Project::open(dir.path()).unwrap();
        let fonts = vec!["Fonts/NotoSansJP.otf".to_string(), "0123456789abcdef0123456789abcdef".to_string()];
        project.set_language_fonts("ja", &fonts).unwrap();
        let written = std::fs::read_to_string(dir.path().join(PROJECT_FILE)).unwrap();
        assert!(written.starts_with("# My game") && written.contains("languages = [\"en\", \"ja\"]"), "{written}");
        assert!(written.contains("[localization.fonts]\nja = [\"Fonts/NotoSansJP.otf\""), "{written}");
        assert_eq!(Project::open(dir.path()).unwrap().file.localization.fonts["ja"], fonts, "read back");
        assert_eq!(project.file.localization.fonts["ja"], fonts, "and kept");
        project.set_language_fonts("ja", &[]).unwrap();
        let written = std::fs::read_to_string(dir.path().join(PROJECT_FILE)).unwrap();
        assert!(!written.contains("fonts") && written.contains("languages"), "{written}");
        assert!(project.file.localization.fonts.is_empty());
    }

    #[test]
    fn switching_a_module_writes_every_modules_choice_and_keeps_the_rest_of_the_file() {
        use crate::modules::Module;
        let folder = tempfile::tempdir().unwrap();
        let text = "# Orbit\n[project]\nname = \"Orbit\"\n";
        std::fs::write(folder.path().join(PROJECT_FILE), text).unwrap();
        let mut project = Project::open(folder.path()).unwrap();
        assert!(project.modules().has(Module::Water) && !project.modules().has(Module::Guns), "made before modules");
        project.set_module(Module::Guns, true).unwrap();
        project.set_module(Module::Water, false).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(written.starts_with("# Orbit\n"), "{written}");
        let reopened = Project::open(folder.path()).unwrap();
        assert_eq!(reopened.modules(), project.modules());
        assert!(reopened.modules().has(Module::Guns) && !reopened.modules().has(Module::Water));
        assert_eq!(written.matches(" = ").count(), 1 + Module::ALL.len(), "the name and every module: {written}");
    }

    #[test]
    fn the_floating_origin_is_written_only_where_it_differs_from_the_defaults() {
        use crate::origin::{OriginMode, OriginSettings};
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(PROJECT_FILE), "# Orbit\n[project]\nname = \"Orbit\"\n").unwrap();
        let mut project = Project::open(folder.path()).unwrap();
        assert_eq!(project.file.floating_origin.mode, OriginMode::Off, "off until switched on");
        let on = OriginSettings { mode: OriginMode::Endless, step: 512, ..OriginSettings::default() };
        project.set_floating_origin(&on).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(written.starts_with("# Orbit\n") && written.contains("mode = \"endless\""), "{written}");
        assert!(!written.contains("distance"), "the default distance is left out: {written}");
        assert_eq!(Project::open(folder.path()).unwrap().file.floating_origin, on);
        project.set_floating_origin(&OriginSettings::default()).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(!written.contains("floating_origin"), "switched off again, the section goes: {written}");
    }

    #[test]
    fn the_world_budget_is_written_as_its_preset_or_every_number_where_custom() {
        use voxear_terrain::{WorldBudget, WorldQuality};
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(PROJECT_FILE), "# Orbit\n[project]\nname = \"Orbit\"\n").unwrap();
        let mut project = Project::open(folder.path()).unwrap();
        assert_eq!(project.file.world, WorldBudget::default(), "High until changed");
        let low = WorldQuality::Low.budget().unwrap();
        project.set_world(&low).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(written.contains("[world]\nquality = \"low\"\n") && !written.contains("threads"), "{written}");
        assert_eq!(Project::open(folder.path()).unwrap().file.world, low);
        let custom = WorldBudget { quality: WorldQuality::Custom, threads: 3, plant_detail: 1.5, ..low };
        project.set_world(&custom).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(written.contains("quality = \"custom\"") && written.contains("threads = 3"), "{written}");
        assert!(written.contains("plant_detail = 1.5") && written.contains("near_detail = 0.5"), "{written}");
        assert_eq!(Project::open(folder.path()).unwrap().file.world, custom);
        project.set_world(&WorldBudget::default()).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(
            written.starts_with("# Orbit\n") && !written.contains("[world]"),
            "High again, the section goes: {written}"
        );
    }

    #[test]
    fn a_setting_is_written_by_its_key_only_where_it_differs_and_refused_where_it_cannot_be() {
        use toml::Value;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(PROJECT_FILE), "# Orbit\n[project]\nname = \"Orbit\"\n").unwrap();
        let mut project = Project::open(folder.path()).unwrap();
        let read = || std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert_eq!(
            project.setting("time.max_fixed_steps"),
            Some(Value::Integer(5)),
            "the default where it is not written"
        );
        assert_eq!(Project::default_setting("network.port"), Some(Value::Integer(7777)));

        project.set_setting("time.fixed_step", Some(Value::Float(0.01))).unwrap();
        project
            .set_setting("physics.gravity", Some(Value::Array(vec![0.0.into(), (-1.62).into(), 0.0.into()])))
            .unwrap();
        assert!(read().starts_with("# Orbit\n") && read().contains("[time]\nfixed_step = 0.01"), "{}", read());
        assert_eq!(project.file.time.fixed_step, 0.01);
        assert_eq!(Project::open(folder.path()).unwrap().file.physics.gravity, [0.0, -1.62, 0.0], "read back");

        // A value the project could not be read with is refused, the file as it was.
        let before = read();
        assert!(project.set_setting("audio.reverb_mode", Some(Value::String("echo".into()))).is_err());
        assert!(project.set_setting("time.max_fixed_steps", Some(Value::Float(2.5))).is_err());
        assert_eq!(read(), before);
        assert!(project.set_setting("nonsense", None).is_err());

        // Back to the default, the line goes, and the table with its last.
        project.set_setting("time.fixed_step", Some(Value::Float(0.02))).unwrap();
        assert!(!read().contains("[time]"), "{}", read());
        project.set_setting("physics.gravity", None).unwrap();
        assert!(!read().contains("[physics]") && project.file.physics.gravity[1] == -9.81, "{}", read());
    }

    #[test]
    fn a_project_pinned_by_the_launcher_opens_and_an_install_is_told_by_its_engine_file() {
        let folder = tempfile::tempdir().unwrap();
        let pinned = "[project]\nname = \"Orbit\"\nengine = \"0.1.0-nightly.20261031\"\n";
        std::fs::write(folder.path().join(PROJECT_FILE), pinned).unwrap();
        let project = Project::open(folder.path()).unwrap();
        assert_eq!(project.file.project.engine.as_deref(), Some("0.1.0-nightly.20261031"));

        assert!(!is_install(folder.path()), "a folder without engine.toml is not an install");
        std::fs::write(folder.path().join(ENGINE_FILE), "format = 1\n").unwrap();
        assert!(is_install(folder.path()));
        assert!(!is_install(&engine_root()), "tests run from the source checkout");
    }

    #[test]
    fn a_project_names_its_layers_and_the_pairs_that_pass_through_each_other() {
        let folder = tempfile::tempdir().unwrap();
        let text = "[project]\nname = \"Orbit\"\n\n[layers]\nnames = [\"\", \"Player\", \"Ghost\"]\n\n\
                    [physics]\nignore = [[\"Ghost\", \"Player\"], [\"Ghost\", \"Nobody\"]]\n";
        std::fs::write(folder.path().join(PROJECT_FILE), text).unwrap();
        let layers = Project::open(folder.path()).unwrap().layers();
        assert_eq!((layers.name(0), layers.find("Ghost")), ("Default", Some(2)));
        assert!(!layers.collide(1, 2) && layers.collide(2, 2) && layers.collide(0, 1));
    }

    #[test]
    fn layers_are_written_into_the_project_file_keeping_the_rest() {
        let folder = tempfile::tempdir().unwrap();
        let text = "# Orbit's settings\n[project]\nname = \"Orbit\"\n\n[physics]\nignore = [[\"Ghost\", \"Ghost\"]]\n";
        std::fs::write(folder.path().join(PROJECT_FILE), text).unwrap();
        let mut project = Project::open(folder.path()).unwrap();
        let mut layers =
            voxear_scene::Layers::named(&["Default".into(), String::new(), "Player".into(), "Ghost".into()]);
        layers.set_collide(2, 3, false);
        layers.set_collide(3, 3, false);
        project.set_layers(&layers).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(written.starts_with("# Orbit's settings\n[project]"), "{written}");
        assert!(written.contains("names = [\"Default\", \"\", \"Player\", \"Ghost\"]"), "{written}");
        assert!(written.contains("ignore = [[\"Player\", \"Ghost\"], [\"Ghost\", \"Ghost\"]]"), "{written}");
        assert_eq!(Project::open(folder.path()).unwrap().layers(), layers, "read back as written");
        assert_eq!(project.layers(), layers, "and as the project now has them");

        // With only Default, and every pair colliding, the tables go.
        project.set_layers(&voxear_scene::Layers::default()).unwrap();
        let written = std::fs::read_to_string(folder.path().join(PROJECT_FILE)).unwrap();
        assert!(!written.contains("[layers]") && !written.contains("[physics]"), "{written}");
    }
}
