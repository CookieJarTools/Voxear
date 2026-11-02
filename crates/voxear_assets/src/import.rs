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

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{AssetGuid, db::META_EXTENSION};

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
    /// Whether a model that has LODs of its own, as parts named `Rock_LOD0`,
    /// `Rock_LOD1` and on, as Unity names them, draws with those, rather
    /// than with LODs made from its first. Either way its later LODs are not
    /// parts of their own.
    pub own_lods: bool,
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
            own_lods: true,
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

/// Which of a model's materials draw with a `.material` asset in place of
/// the one in its file, as Unity's material remapping: each by its part
/// (`material3`), with the material asset's GUID. In the model's `.meta`:
///
/// ```toml
/// [materials]
/// material3 = "…"
/// ```
///
/// A part left out draws with the file's material.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaterialRemap(pub BTreeMap<String, AssetGuid>);

impl MaterialRemap {
    const TABLE: &str = "materials";

    /// The remapping in a `.meta` file's text, or none.
    pub fn from_meta(text: &str) -> Self {
        section(text, Self::TABLE)
    }

    /// The remapping of the model at `asset`, from its `.meta`, or none.
    pub fn read(asset: &Path) -> Self {
        std::fs::read_to_string(meta_path(asset)).map(|text| Self::from_meta(&text)).unwrap_or_default()
    }

    /// Writes the remapping into the `.meta` beside `asset`, or takes the
    /// table out when nothing is remapped.
    pub fn write(&self, asset: &Path) -> Result<()> {
        write_section(asset, Self::TABLE, self, &Self::default())
    }

    /// The material asset a part draws with, when it is remapped.
    pub fn get(&self, part: &str) -> Option<AssetGuid> {
        self.0.get(part).copied()
    }
}

/// How a picture file (`.png` or `.jpg`) is imported, where it is used: as
/// a texture a model reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureImport {
    /// The most pixels its longer side keeps: a larger picture is shrunk,
    /// keeping its shape. `None` keeps it at its own size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
    /// Whether a build ships it compressed, a quarter of the memory. Off,
    /// it ships as it is, as sharp edges and pixel art need.
    pub compress: bool,
}

impl Default for TextureImport {
    fn default() -> Self {
        Self { max_size: None, compress: true }
    }
}

impl TextureImport {
    const TABLE: &str = "texture";

    /// The settings in a `.meta` file's text, or the defaults.
    pub fn from_meta(text: &str) -> Self {
        section(text, Self::TABLE)
    }

    /// The settings of the picture at `asset`, from its `.meta`, or the defaults.
    pub fn read(asset: &Path) -> Self {
        std::fs::read_to_string(meta_path(asset)).map(|text| Self::from_meta(&text)).unwrap_or_default()
    }

    /// Writes the settings into the `.meta` beside `asset`.
    pub fn write(&self, asset: &Path) -> Result<()> {
        write_section(asset, Self::TABLE, self, &Self::default())
    }

    /// The size a picture of `width` × `height` pixels is imported at.
    pub fn size(&self, width: u32, height: u32) -> (u32, u32) {
        let Some(max) = self.max_size.filter(|&max| max > 0 && width.max(height) > max) else {
            return (width, height);
        };
        let shrink = |side: u32| ((u64::from(side) * u64::from(max)) / u64::from(width.max(height))).max(1) as u32;
        (shrink(width), shrink(height))
    }
}

/// How a sound file (`.wav`, `.ogg`, `.flac` or `.mp3`) is imported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundImport {
    pub load: SoundLoad,
    /// Multiplies the sound wherever it plays, as a quick fix for a file
    /// recorded too loud or too quiet.
    pub volume: f32,
}

impl Default for SoundImport {
    fn default() -> Self {
        Self { load: SoundLoad::Auto, volume: 1.0 }
    }
}

impl SoundImport {
    const TABLE: &str = "sound";

    /// The settings in a `.meta` file's text, or the defaults.
    pub fn from_meta(text: &str) -> Self {
        section(text, Self::TABLE)
    }

    /// The settings of the sound at `asset`, from its `.meta`, or the defaults.
    pub fn read(asset: &Path) -> Self {
        std::fs::read_to_string(meta_path(asset)).map(|text| Self::from_meta(&text)).unwrap_or_default()
    }

    /// Writes the settings into the `.meta` beside `asset`.
    pub fn write(&self, asset: &Path) -> Result<()> {
        write_section(asset, Self::TABLE, self, &Self::default())
    }

    /// Whether a sound of `seconds` streams as it plays rather than being
    /// decoded whole when it loads.
    pub fn streams(&self, seconds: f64) -> bool {
        match self.load {
            SoundLoad::Auto => seconds > SoundLoad::AUTO_STREAM_SECONDS,
            SoundLoad::Whole => false,
            SoundLoad::Stream => true,
        }
    }
}

/// How a sound is held while it is loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundLoad {
    /// Whole for a short sound, streamed for one longer than
    /// [`AUTO_STREAM_SECONDS`](Self::AUTO_STREAM_SECONDS).
    #[default]
    Auto,
    /// Decoded into memory when it loads, so it starts at once and costs
    /// nothing while it plays: for effects.
    Whole,
    /// Kept as its file and decoded a little ahead as it plays: a fraction
    /// of the memory, for music and long ambience.
    Stream,
}

impl SoundLoad {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Whole, Self::Stream];
    /// How long a sound can be before Auto streams it.
    pub const AUTO_STREAM_SECONDS: f64 = 10.0;
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
    fn a_models_remapped_materials_are_written_by_part_and_taken_out_when_none_are() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("Gate.fbx");
        std::fs::write(&model, "").unwrap();
        std::fs::write(meta_path(&model), "guid = \"abc\"\n\n[model]\nscale = 2.0\n").unwrap();
        let stone = AssetGuid::new_random();
        let remap = MaterialRemap([("material3".to_string(), stone)].into());
        remap.write(&model).unwrap();
        let text = std::fs::read_to_string(meta_path(&model)).unwrap();
        assert!(text.contains(&format!("[materials]\nmaterial3 = \"{stone}\"")), "{text}");
        assert_eq!(MaterialRemap::read(&model).get("material3"), Some(stone));
        assert_eq!(ModelImport::read(&model).scale, 2.0, "the import settings stay");

        MaterialRemap::default().write(&model).unwrap();
        let text = std::fs::read_to_string(meta_path(&model)).unwrap();
        assert!(!text.contains("[materials]"), "{text}");
    }

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
    fn a_textures_settings_shrink_it_keeping_its_shape() {
        let dir = tempfile::tempdir().unwrap();
        let picture = dir.path().join("Bricks.png");
        std::fs::write(meta_path(&picture), "guid = \"abc\"\n").unwrap();
        let settings = TextureImport { max_size: Some(256), compress: false };
        settings.write(&picture).unwrap();
        let text = std::fs::read_to_string(meta_path(&picture)).unwrap();
        assert!(text.contains("[texture]") && text.contains("max_size = 256") && text.contains("compress = false"));
        assert_eq!(TextureImport::read(&picture), settings);

        assert_eq!(settings.size(1024, 512), (256, 128));
        assert_eq!(settings.size(100, 3000), (8, 256));
        assert_eq!(settings.size(200, 100), (200, 100), "a smaller picture keeps its size");
        assert_eq!(TextureImport::default().size(8192, 8192), (8192, 8192));
    }

    #[test]
    fn a_sounds_settings_say_how_it_loads_and_how_loud_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let sound = dir.path().join("Rain.ogg");
        std::fs::write(meta_path(&sound), "guid = \"abc\"\n").unwrap();
        let auto = SoundImport::read(&sound);
        assert_eq!(auto, SoundImport::default());
        assert!(!auto.streams(2.0) && auto.streams(120.0), "Auto streams long sounds only");

        let settings = SoundImport { load: SoundLoad::Stream, volume: 0.5 };
        settings.write(&sound).unwrap();
        let text = std::fs::read_to_string(meta_path(&sound)).unwrap();
        assert!(text.contains("[sound]") && text.contains("load = \"stream\"") && text.contains("volume = 0.5"));
        assert_eq!(SoundImport::read(&sound), settings);
        assert!(settings.streams(0.1));
        assert!(!SoundImport { load: SoundLoad::Whole, volume: 1.0 }.streams(600.0));
    }

    #[test]
    fn a_damaged_table_imports_with_the_defaults() {
        let text = "guid = \"abc\"\n[model]\nunits = \"furlongs\"\n";
        assert_eq!(ModelImport::from_meta(text), ModelImport::default());
        assert_eq!(ModelImport::from_meta("not toml ["), ModelImport::default());
    }
}
