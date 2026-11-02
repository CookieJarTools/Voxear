use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{guid::AssetGuid, import::meta_path};

pub const META_EXTENSION: &str = "meta";

/// Folders a scan never enters: engine caches, version control and C# build
/// output.
const SKIPPED_FOLDERS: &[&str] = &["bin", "obj", "target", "node_modules"];
/// The project's own documentation, VoxDocs' pages and their pictures, at
/// its top: not the game's content, so never given GUIDs or built.
const DOCS_FOLDER: &str = "Docs";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Model,
    Texture,
    Material,
    Scene,
    Level,
    Prefab,
    /// A `.vui` document, which ships with the `.vss` sheets it names.
    Ui,
    /// A sound file: WAV, OGG Vorbis, FLAC or MP3.
    Sound,
    /// A `.inputactions` file: the game's input actions, in maps.
    InputActions,
}

impl AssetKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "gltf" | "glb" | "fbx" => Self::Model,
            "png" | "jpg" | "jpeg" => Self::Texture,
            "material" => Self::Material,
            "scene" => Self::Scene,
            "level" => Self::Level,
            "prefab" => Self::Prefab,
            "vui" => Self::Ui,
            "wav" | "ogg" | "flac" | "mp3" => Self::Sound,
            "inputactions" => Self::InputActions,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub guid: AssetGuid,
    /// Relative to the project root, with forward slashes.
    pub path: String,
    pub kind: AssetKind,
}

/// What a scan changed, for logging.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Assets that had no `.meta` and were given a new GUID.
    pub created: Vec<String>,
    /// Assets whose GUID was already taken (usually a copied file and
    /// `.meta`), which were given a new one.
    pub reassigned: Vec<String>,
}

#[derive(Deserialize)]
struct MetaFile {
    guid: String,
}

#[derive(Clone)]
pub struct AssetDb {
    root: PathBuf,
    by_guid: HashMap<AssetGuid, AssetEntry>,
    by_path: HashMap<String, AssetGuid>,
}

impl AssetDb {
    /// Finds every asset under `root`, writing `.meta` files for new ones.
    pub fn scan(root: &Path) -> Result<(Self, ScanReport)> {
        let mut db = Self { root: root.to_path_buf(), by_guid: HashMap::new(), by_path: HashMap::new() };
        let mut report = ScanReport::default();
        let mut files = Vec::new();
        collect(root, &mut files, true)?;
        // Sorted, so which of two duplicates keeps its GUID does not depend on the file system.
        files.sort();
        for file in files {
            db.add(&file, &mut report)?;
        }
        Ok((db, report))
    }

    /// A database of known entries, such as those a content catalog lists,
    /// with nothing read from disk.
    pub fn from_entries(root: &Path, entries: impl IntoIterator<Item = AssetEntry>) -> Self {
        let mut db = Self { root: root.to_path_buf(), by_guid: HashMap::new(), by_path: HashMap::new() };
        for entry in entries {
            db.by_path.insert(entry.path.clone(), entry.guid);
            db.by_guid.insert(entry.guid, entry);
        }
        db
    }

    /// Adds an entry, replacing any with the same GUID or the same path, as
    /// an overlay catalog does.
    pub fn insert(&mut self, entry: AssetEntry) {
        if let Some(old) = self.by_guid.get(&entry.guid) {
            self.by_path.remove(&old.path);
        }
        self.by_path.insert(entry.path.clone(), entry.guid);
        self.by_guid.insert(entry.guid, entry);
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, guid: AssetGuid) -> Option<&AssetEntry> {
        self.by_guid.get(&guid)
    }

    /// The asset at a project-relative path.
    pub fn guid_of(&self, path: &str) -> Option<AssetGuid> {
        self.by_path.get(&normalize(Path::new(path))).copied()
    }

    /// The asset's file on disk.
    pub fn path(&self, guid: AssetGuid) -> Option<PathBuf> {
        self.get(guid).map(|entry| self.root.join(&entry.path))
    }

    pub fn entries(&self) -> impl Iterator<Item = &AssetEntry> {
        self.by_guid.values()
    }

    pub fn len(&self) -> usize {
        self.by_guid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_guid.is_empty()
    }

    fn add(&mut self, file: &Path, report: &mut ScanReport) -> Result<()> {
        let Some(kind) = AssetKind::from_path(file) else { return Ok(()) };
        let relative = normalize(file.strip_prefix(&self.root).unwrap_or(file));
        let meta_path = meta_path(file);
        let existing = read_meta(&meta_path);

        let guid = match existing {
            Some(guid) if !self.by_guid.contains_key(&guid) => guid,
            Some(_) => {
                report.reassigned.push(relative.clone());
                write_meta(&meta_path, AssetGuid::new_random())?
            }
            None => {
                report.created.push(relative.clone());
                write_meta(&meta_path, AssetGuid::new_random())?
            }
        };
        self.by_path.insert(relative.clone(), guid);
        self.by_guid.insert(guid, AssetEntry { guid, path: relative, kind });
        Ok(())
    }
}

fn collect(folder: &Path, files: &mut Vec<PathBuf>, top: bool) -> Result<()> {
    for entry in std::fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !SKIPPED_FOLDERS.contains(&name) && !(top && name == DOCS_FOLDER) {
                collect(&path, files, false)?;
            }
            continue;
        }
        files.push(path);
    }
    Ok(())
}

fn read_meta(path: &Path) -> Option<AssetGuid> {
    let text = std::fs::read_to_string(path).ok()?;
    let meta: MetaFile = toml::from_str(&text).ok()?;
    meta.guid.parse().ok()
}

/// Gives a `.meta` file a GUID, keeping whatever else it holds, such as a
/// copied model's import settings.
fn write_meta(path: &Path, guid: AssetGuid) -> Result<AssetGuid> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut meta: toml::Table = text.parse().unwrap_or_default();
    meta.insert("guid".to_string(), toml::Value::String(guid.to_string()));
    std::fs::write(path, toml::to_string(&meta)?).with_context(|| format!("writing {}", path.display()))?;
    Ok(guid)
}

fn normalize(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, path: &str) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn new_assets_get_meta_files_and_keep_their_guids() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "Models/Ship.glb");
        touch(dir.path(), "Scenes/Main.scene");
        touch(dir.path(), "notes.txt");
        touch(dir.path(), ".voxear/Cache/Ignored.scene");
        touch(dir.path(), "Docs/Guide/images/Shot.png");
        touch(dir.path(), "Textures/Docs/Plate.png");

        let (db, report) = AssetDb::scan(dir.path()).unwrap();
        assert_eq!(db.len(), 3, "the project's Docs are its documentation, not its content");
        assert!(db.guid_of("Textures/Docs/Plate.png").is_some(), "a Docs folder further in is content");
        assert_eq!(report.created.len(), 3);
        assert!(dir.path().join("Models/Ship.glb.meta").is_file());
        let ship = db.guid_of("Models/Ship.glb").unwrap();
        assert_eq!(db.get(ship).unwrap().kind, AssetKind::Model);

        let (again, report) = AssetDb::scan(dir.path()).unwrap();
        assert!(report.created.is_empty());
        assert_eq!(again.guid_of("Models/Ship.glb"), Some(ship));
    }

    #[test]
    fn moving_a_file_with_its_meta_keeps_the_guid() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "Ship.glb");
        let (db, _) = AssetDb::scan(dir.path()).unwrap();
        let guid = db.guid_of("Ship.glb").unwrap();

        std::fs::create_dir(dir.path().join("Models")).unwrap();
        std::fs::rename(dir.path().join("Ship.glb"), dir.path().join("Models/Ship.glb")).unwrap();
        std::fs::rename(dir.path().join("Ship.glb.meta"), dir.path().join("Models/Ship.glb.meta")).unwrap();
        let (db, _) = AssetDb::scan(dir.path()).unwrap();
        assert_eq!(db.guid_of("Models/Ship.glb"), Some(guid));
        assert_eq!(db.get(guid).unwrap().path, "Models/Ship.glb");
    }

    #[test]
    fn copied_assets_get_a_fresh_guid() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "A.scene");
        AssetDb::scan(dir.path()).unwrap();
        std::fs::copy(dir.path().join("A.scene"), dir.path().join("B.scene")).unwrap();
        std::fs::copy(dir.path().join("A.scene.meta"), dir.path().join("B.scene.meta")).unwrap();

        let meta = dir.path().join("B.scene.meta");
        let copied = std::fs::read_to_string(&meta).unwrap();
        std::fs::write(&meta, format!("{copied}\n[later]\nkept = true\n")).unwrap();

        let (db, report) = AssetDb::scan(dir.path()).unwrap();
        assert_eq!(report.reassigned, vec!["B.scene".to_string()]);
        assert_ne!(db.guid_of("A.scene"), db.guid_of("B.scene"));
        let text = std::fs::read_to_string(&meta).unwrap();
        assert!(text.contains("[later]"), "the copy keeps its settings: {text}");
    }
}
