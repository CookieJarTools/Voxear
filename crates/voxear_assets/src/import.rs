//! How an asset is imported, as Unity's import settings: kept in the asset's
//! `.meta` file beside its GUID, so they travel with the file, and in a
//! table named for the asset's kind:
//!
//! ```toml
//! guid = "…"
//!
//! [model]
//! units = "centimetres"
//! scale = 2.0
//! ```
//!
//! A `.meta` without the table, or without some of its settings, imports
//! with the defaults, and settings left at their defaults are not written.
//! Whatever else a `.meta` holds is kept when settings are written.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::db::META_EXTENSION;

/// How a model file (`.gltf`, `.glb` or `.fbx`) is imported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelImport {
    /// The length one unit in the file stands for.
    pub units: Units,
    /// Multiplies the model's size, after its units, as Unity's Scale Factor.
    pub scale: f32,
    pub normals: Normals,
    /// Calculated normals are smoothed across an edge whose faces meet at
    /// less than this many degrees; sharper edges stay hard.
    pub smoothing_angle: f32,
    pub tangents: Tangents,
    /// Whether simpler versions of each mesh are made, drawn when it is far
    /// away. Without them the mesh is always drawn in full.
    pub lods: bool,
    /// Whether the file's materials are used. Without them every part draws
    /// in the default material, and the parts' material references still
    /// work.
    pub materials: bool,
}

impl Default for ModelImport {
    fn default() -> Self {
        Self {
            units: Units::File,
            scale: 1.0,
            normals: Normals::Import,
            smoothing_angle: 60.0,
            tangents: Tangents::Import,
            lods: true,
            materials: true,
        }
    }
}

impl ModelImport {
    const TABLE: &str = "model";

    /// The settings in a `.meta` file's text, or the defaults.
    pub fn from_meta(text: &str) -> Self {
        section(text, Self::TABLE)
    }

    /// The settings of the model at `asset`, from its `.meta`, or the defaults.
    pub fn read(asset: &Path) -> Self {
        std::fs::read_to_string(meta_path(asset)).map(|text| Self::from_meta(&text)).unwrap_or_default()
    }

    /// Writes the settings into the `.meta` beside `asset`.
    pub fn write(&self, asset: &Path) -> Result<()> {
        write_section(asset, Self::TABLE, self, &Self::default())
    }

    /// How many metres one unit becomes, units and scale together, for a
    /// model read in metres when its units are the file's: glTF always is,
    /// and an FBX file is read in its own unit only when the settings name
    /// another.
    pub fn size(&self) -> f32 {
        self.units.metres().unwrap_or(1.0) * self.scale
    }
}

/// The length one unit in a model file stands for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    /// What the file says: glTF is always in metres, and an FBX file names
    /// its own unit.
    #[default]
    File,
    Millimetres,
    Centimetres,
    Metres,
    Inches,
    Feet,
}

impl Units {
    pub const ALL: [Self; 6] =
        [Self::File, Self::Millimetres, Self::Centimetres, Self::Metres, Self::Inches, Self::Feet];

    /// Metres in one unit, or `None` for the file's own.
    pub fn metres(self) -> Option<f32> {
        Some(match self {
            Self::File => return None,
            Self::Millimetres => 0.001,
            Self::Centimetres => 0.01,
            Self::Metres => 1.0,
            Self::Inches => 0.0254,
            Self::Feet => 0.3048,
        })
    }
}

/// Where a model's normals come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Normals {
    /// The file's, calculated for a mesh that has none.
    #[default]
    Import,
    /// Calculated from the faces, by the smoothing angle.
    Calculate,
}

/// Where a model's tangents come from. Normal maps need them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tangents {
    /// The file's, calculated for a mesh that has none.
    #[default]
    Import,
    /// Calculated from the normals and UVs.
    Calculate,
}

/// The `.meta` file of the asset at `asset`.
pub fn meta_path(asset: &Path) -> PathBuf {
    let mut name = asset.as_os_str().to_owned();
    name.push(".");
    name.push(META_EXTENSION);
    PathBuf::from(name)
}

/// A table of a `.meta` file's text, with the defaults for what it lacks.
fn section<T: DeserializeOwned + Default>(text: &str, name: &str) -> T {
    let Ok(mut meta) = text.parse::<toml::Table>() else { return T::default() };
    meta.remove(name).and_then(|table| table.try_into().ok()).unwrap_or_default()
}

/// Writes a table into the `.meta` beside `asset`, keeping the rest of it,
/// with only the settings that differ from `defaults`, and none at all
/// when nothing does.
fn write_section<T: Serialize>(asset: &Path, name: &str, value: &T, defaults: &T) -> Result<()> {
    let path = meta_path(asset);
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut meta: toml::Table = text.parse().with_context(|| format!("reading {}", path.display()))?;
    let toml::Value::Table(mut table) = toml::Value::try_from(value)? else { unreachable!("settings are a table") };
    let toml::Value::Table(defaults) = toml::Value::try_from(defaults)? else { unreachable!("settings are a table") };
    table.retain(|key, value| defaults.get(key) != Some(value));
    match table.is_empty() {
        true => meta.remove(name),
        false => meta.insert(name.to_string(), toml::Value::Table(table)),
    };
    std::fs::write(&path, toml::to_string(&meta)?).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_written_beside_the_guid_and_only_where_they_differ() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("Ship.glb");
        std::fs::write(&model, "").unwrap();
        std::fs::write(meta_path(&model), "guid = \"abc\"\n\n[later]\nkept = true\n").unwrap();
        assert_eq!(ModelImport::read(&model), ModelImport::default(), "a .meta without settings has the defaults");

        let settings = ModelImport { units: Units::Centimetres, scale: 2.0, ..ModelImport::default() };
        settings.write(&model).unwrap();
        let text = std::fs::read_to_string(meta_path(&model)).unwrap();
        assert!(text.contains("guid = \"abc\""), "{text}");
        assert!(text.contains("[later]"), "what else the .meta holds stays: {text}");
        assert!(text.contains("units = \"centimetres\"") && text.contains("scale = 2.0"), "{text}");
        assert!(!text.contains("lods"), "defaults are left out: {text}");
        assert_eq!(ModelImport::read(&model), settings);
        assert!((settings.size() - 0.02).abs() < 1e-6);

        ModelImport::default().write(&model).unwrap();
        let text = std::fs::read_to_string(meta_path(&model)).unwrap();
        assert!(!text.contains("[model]"), "back to the defaults, the table goes: {text}");
    }

    #[test]
    fn a_damaged_table_imports_with_the_defaults() {
        let text = "guid = \"abc\"\n[model]\nunits = \"furlongs\"\n";
        assert_eq!(ModelImport::from_meta(text), ModelImport::default());
        assert_eq!(ModelImport::from_meta("not toml ["), ModelImport::default());
    }
}
