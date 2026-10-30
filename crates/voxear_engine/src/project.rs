use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub const PROJECT_FILE: &str = "voxear.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub project: ProjectInfo,
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
