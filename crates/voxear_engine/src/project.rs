use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::bundle::Compression;
use voxear_audio::ReverbMode;

pub const PROJECT_FILE: &str = "voxear.toml";
/// What an engine install is, at its root: its version, channel and platform.
pub const ENGINE_FILE: &str = "engine.toml";

#[derive(Debug, Deserialize)]
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
    /// The modules the project has, or none for a project made before
    /// there were modules (see [`crate::modules`]).
    pub modules: Option<crate::modules::Modules>,
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
/// ```
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSettings {
    pub headphones: bool,
    pub reverb: bool,
    #[serde(with = "ReverbModeName")]
    pub reverb_mode: ReverbMode,
    pub steam_audio: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { headphones: false, reverb: false, reverb_mode: ReverbMode::Parametric, steam_audio: true }
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
/// first. Its text is in string tables (see `voxear_localization`).
///
/// ```toml
/// [localization]
/// languages = ["en", "fr", "pt-BR"]
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalizationSettings {
    pub languages: Vec<String>,
}

/// The project's layers' names, by number from layer 0, as Unity's Tags
/// and Layers name them; an empty name is a layer not used.
///
/// ```toml
/// [layers]
/// names = ["Default", "Ignore Raycast", "Player", "Enemy"]
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerSettings {
    pub names: Vec<String>,
}

/// Physics for the whole project: the pairs of layers whose Objs pass
/// through each other, every other pair colliding.
///
/// ```toml
/// [physics]
/// ignore = [["Player", "Player"], ["Enemy", "Pickup"]]
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsSettings {
    pub ignore: Vec<[String; 2]>,
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
#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub name: String,
    /// The game's C# project, relative to the project root.
    pub scripts: Option<PathBuf>,
    /// The level to load first: a `.level` path or GUID.
    pub start_level: Option<String>,
    /// The engine version the project uses, as the launcher pins it.
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
        let file: ProjectFile = toml::from_str(&text).with_context(|| format!("reading {}", manifest.display()))?;
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
