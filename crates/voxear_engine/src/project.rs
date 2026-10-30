use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub const PROJECT_FILE: &str = "voxear.toml";

#[derive(Debug, Deserialize)]
pub struct ProjectFile {
    pub project: ProjectInfo,
    /// Objs created at startup. A stand-in until scene assets exist.
    #[serde(default)]
    pub objs: Vec<ObjSpec>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    /// The game's C# project, relative to the project root.
    pub scripts: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjSpec {
    pub name: String,
    #[serde(default)]
    pub position: [f32; 3],
    /// Euler angles in degrees, applied Z, then X, then Y, as in Unity.
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default = "one")]
    pub scale: [f32; 3],
    /// Full C# type names, e.g. `Hello.Spinner`.
    #[serde(default)]
    pub behaviours: Vec<String>,
    /// `cube`, `sphere`, `plane`, or a `.gltf`/`.glb` path relative to the project.
    pub mesh: Option<String>,
    #[serde(default)]
    pub material: MaterialSpec,
    pub camera: Option<CameraSpec>,
    pub light: Option<LightSpec>,
}

fn one() -> [f32; 3] {
    [1.0; 3]
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialSpec {
    pub color: Option<[f32; 3]>,
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    pub emissive: Option<[f32; 3]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraSpec {
    #[serde(default = "default_fov")]
    pub fov: f32,
    #[serde(default = "default_exposure")]
    pub exposure: f32,
}

fn default_fov() -> f32 {
    60.0
}

fn default_exposure() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum LightSpec {
    Directional {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_sun")]
        intensity: f32,
    },
    Point {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
    },
    Spot {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default = "default_inner")]
        inner_angle: f32,
        #[serde(default = "default_outer")]
        outer_angle: f32,
    },
}

fn white() -> [f32; 3] {
    [1.0; 3]
}

fn default_sun() -> f32 {
    3.0
}

fn default_lamp() -> f32 {
    10.0
}

fn default_range() -> f32 {
    10.0
}

fn default_inner() -> f32 {
    30.0
}

fn default_outer() -> f32 {
    45.0
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
