use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::guid::AssetGuid;

pub const META_EXTENSION: &str = "meta";

/// Folders a scan never enters: engine caches, version control and C# build
/// output.
const SKIPPED_FOLDERS: &[&str] = &["bin", "obj", "target", "node_modules"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Model,
    Texture,
    Material,
    Scene,
    Level,
    Prefab,
}

impl AssetKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "gltf" | "glb" => Self::Model,
            "png" | "jpg" | "jpeg" => Self::Texture,
            "material" => Self::Material,
            "scene" => Self::Scene,
            "level" => Self::Level,
            "prefab" => Self::Prefab,
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

#[derive(Serialize, Deserialize)]
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
        collect(root, &mut files)?;
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

fn collect(folder: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !SKIPPED_FOLDERS.contains(&name) {
                collect(&path, files)?;
            }
            continue;
        }
        files.push(path);
    }
    Ok(())
}

fn meta_path(asset: &Path) -> PathBuf {
    let mut name = asset.as_os_str().to_owned();
    name.push(".");
    name.push(META_EXTENSION);
    PathBuf::from(name)
}

fn read_meta(path: &Path) -> Option<AssetGuid> {
    let text = std::fs::read_to_string(path).ok()?;
    let meta: MetaFile = toml::from_str(&text).ok()?;
    meta.guid.parse().ok()
}

fn write_meta(path: &Path, guid: AssetGuid) -> Result<AssetGuid> {
    let text = toml::to_string(&MetaFile { guid: guid.to_string() })?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
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

        let (db, report) = AssetDb::scan(dir.path()).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(report.created.len(), 2);
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

        let (db, report) = AssetDb::scan(dir.path()).unwrap();
        assert_eq!(report.reassigned, vec!["B.scene".to_string()]);
        assert_ne!(db.guid_of("A.scene"), db.guid_of("B.scene"));
    }
}
