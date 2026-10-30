use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub const PROJECT_FILE: &str = "voxear.toml";

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

/// The engine checkout, which holds the managed `Voxear.Core` build. Set
/// `VOXEAR_HOME` to point a shipped `vox` at an install.
pub fn engine_root() -> PathBuf {
    std::env::var_os("VOXEAR_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

pub fn core_assembly(configuration: &str) -> PathBuf {
    engine_root().join("target").join("managed").join(configuration).join("Voxear.Core.dll")
}
