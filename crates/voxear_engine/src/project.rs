use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub const PROJECT_FILE: &str = "voxear.toml";
/// What an engine install is, at its root: its version, channel and platform.
pub const ENGINE_FILE: &str = "engine.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub project: ProjectInfo,
    #[serde(default)]
    pub content: ContentSettings,
}

/// What the content build treats as roots, beside every level and prefab.
///
/// ```toml
/// [content]
/// include = ["Textures/Icons/**"]   # loaded by paths the build cannot see
/// exclude = ["Prefabs/Debug/**"]    # not shipped unless something shipped uses them
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
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self { include: Vec::new(), exclude: Vec::new(), merge_below_kb: 16, split_above_mb: 64 }
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

    pub fn scripts_project(&self) -> Option<PathBuf> {
        self.file.project.scripts.as_ref().map(|p| self.root.join(p))
    }

    /// Where the scripts project builds to (set by its csproj).
    pub fn scripts_assembly(&self, configuration: &str) -> Option<PathBuf> {
        let csproj = self.scripts_project()?;
        let name = csproj.file_stem()?.to_string_lossy().into_owned();
        Some(self.root.join(".voxear").join("Scripts").join(configuration).join(format!("{name}.dll")))
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

/// `Voxear.Core.dll`: an install's, in its `managed/` folder, or a checkout's
/// build of it in this configuration.
pub fn core_assembly(configuration: &str) -> PathBuf {
    let root = engine_root();
    if is_install(&root) {
        return root.join("managed").join("Voxear.Core.dll");
    }
    root.join("target").join("managed").join(configuration).join("Voxear.Core.dll")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
