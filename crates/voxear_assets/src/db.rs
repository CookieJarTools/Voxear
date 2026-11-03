use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    bundle::content_hash,
    guid::AssetGuid,
    import::meta_path,
    mount::{Access, Mount, Mounts, PACKAGES_FOLDER, PackageMount},
};

pub const META_EXTENSION: &str = "meta";

/// Folders a scan never enters: engine caches, version control and C# build
/// output.
const SKIPPED_FOLDERS: &[&str] = &["bin", "obj", "target", "node_modules"];
/// The project's own documentation, VoxDocs' pages and their pictures, at
/// its top: not the game's content, so never given GUIDs or built.
const DOCS_FOLDER: &str = "Docs";
/// A package's folders at its top that are not its assets: its C#, its
/// VoxDocs pages, and the samples a project copies in to make its own.
const PACKAGE_FOLDERS: &[&str] = &["Scripts", "Docs", "Samples"];

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
    /// A `.strings` file: a table of the game's text in each of its languages.
    Strings,
    /// A `.paths` file: the paths sound takes round a level's walls, as
    /// baked beside its `.level`.
    Paths,
    /// A `.assettable` file: a table of the game's assets that differ by
    /// language, as a voice line recorded in each, a key's asset for each.
    AssetTable,
    /// An `.ies` file: how bright a real light fitting is in each
    /// direction, as its maker measured it, for a point or spot light.
    LightProfile,
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
            "strings" => Self::Strings,
            "paths" => Self::Paths,
            "assettable" => Self::AssetTable,
            "ies" => Self::LightProfile,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub guid: AssetGuid,
    /// The path the content knows it by, with forward slashes: from the
    /// project's folder for the project's own, and under `Packages/<name>/`
    /// for a package's (see [`Mount::content_path`]). Readers, bundles and
    /// hand-written references all name it so.
    pub path: String,
    pub kind: AssetKind,
    pub mount: Mount,
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
    mounts: Mounts,
    by_guid: HashMap<AssetGuid, AssetEntry>,
    by_path: HashMap<String, AssetGuid>,
}

impl AssetDb {
    /// Finds every asset under `root`, writing `.meta` files for new ones.
    pub fn scan(root: &Path) -> Result<(Self, ScanReport)> {
        Self::scan_mounted(Mounts::project(root))
    }

    /// Finds every asset of the project and of the packages mounted beside
    /// it (see [`crate::mount`]). The project's and folder packages' new
    /// assets get `.meta` files; a read-only package's are never written.
    /// Its GUIDs cannot change, so they are taken first, and an asset of the
    /// project with one of them, as one copied from the package with its
    /// `.meta`, is the one given a new GUID.
    pub fn scan_mounted(mounts: Mounts) -> Result<(Self, ScanReport)> {
        let mut db = Self { mounts, by_guid: HashMap::new(), by_path: HashMap::new() };
        let mut report = ScanReport::default();
        let mut packages = db.mounts.packages().to_vec();
        packages.sort_by_key(|package| (package.access == Access::Editable, package.name.clone()));
        for package in &packages {
            let mut files = Vec::new();
            collect_package(&package.root, &mut files)
                .with_context(|| format!("reading the assets of the package {}", package.name))?;
            files.sort();
            for file in files {
                db.add_package(package, &file, &mut report)?;
            }
        }
        // Folder packages inside the project's folder are seen once, as packages.
        let inside: Vec<PathBuf> = packages.iter().filter_map(|package| package.root.canonicalize().ok()).collect();
        let mounted: Vec<String> = packages.iter().map(|package| package.name.clone()).collect();
        let root = db.mounts.root().to_path_buf();
        let mut files = Vec::new();
        collect(&root, &mut files, Some(&Skipped { inside: &inside, mounted: &mounted }))?;
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
        let mut db = Self { mounts: Mounts::project(root), by_guid: HashMap::new(), by_path: HashMap::new() };
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

    /// The project's folder.
    pub fn root(&self) -> &Path {
        self.mounts.root()
    }

    /// The project's folder and the packages mounted beside it.
    pub fn mounts(&self) -> &Mounts {
        &self.mounts
    }

    /// The file on disk a content path names, in the project or a package.
    pub fn file(&self, path: &str) -> PathBuf {
        self.mounts.file(path)
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
        self.get(guid).map(|entry| self.mounts.file(&entry.path))
    }

    /// Whether the asset's files may change: false for one in a package
    /// from the machine's cache.
    pub fn is_editable(&self, guid: AssetGuid) -> bool {
        self.get(guid).is_some_and(|entry| self.mounts.access(&entry.mount) == Access::Editable)
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
        let relative = normalize(file.strip_prefix(self.mounts.root()).unwrap_or(file));
        self.add_editable(file, relative, Mount::Project, report)
    }

    fn add_package(&mut self, package: &PackageMount, file: &Path, report: &mut ScanReport) -> Result<()> {
        let local = normalize(file.strip_prefix(&package.root).unwrap_or(file));
        let path = package.mount().content_path(&local);
        if package.access == Access::Editable {
            return self.add_editable(file, path, package.mount(), report);
        }
        let Some(kind) = AssetKind::from_path(file) else { return Ok(()) };
        let made = package_guid(&package.name, &local);
        let guid = match read_meta(&meta_path(file)) {
            Some(guid) if !self.by_guid.contains_key(&guid) => guid,
            Some(_) => {
                report.reassigned.push(path.clone());
                made
            }
            None => made,
        };
        self.by_path.insert(path.clone(), guid);
        self.by_guid.insert(guid, AssetEntry { guid, path, kind, mount: package.mount() });
        Ok(())
    }

    /// Adds an asset whose `.meta` may be written: the project's, or a
    /// folder package's.
    fn add_editable(&mut self, file: &Path, relative: String, mount: Mount, report: &mut ScanReport) -> Result<()> {
        let Some(kind) = AssetKind::from_path(file) else { return Ok(()) };
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
        self.by_guid.insert(guid, AssetEntry { guid, path: relative, kind, mount });
        Ok(())
    }
}

/// The GUID of a read-only package's asset that ships no `.meta`: made from
/// the package's name and the asset's path in it, never from where the
/// package is on disk, so it is the same on every machine.
fn package_guid(package: &str, local: &str) -> AssetGuid {
    let made = content_hash(&[b"voxear package asset\0", package.as_bytes(), b"\0", local.as_bytes()]);
    made.parse().expect("a content hash is 32 hex digits")
}

/// What the project's own scan leaves to its packages: folder packages kept
/// at the project's top or in its `Packages` folder, and `Packages/<name>`
/// for each package mounted.
struct Skipped<'a> {
    inside: &'a [PathBuf],
    mounted: &'a [String],
}

impl Skipped<'_> {
    fn is_package(&self, folder: &Path) -> bool {
        !self.inside.is_empty() && folder.canonicalize().is_ok_and(|folder| self.inside.contains(&folder))
    }
}

/// Every file under a project's folder that may be an asset. What is
/// `skipped` is looked for at the top only.
fn collect(folder: &Path, files: &mut Vec<PathBuf>, skipped: Option<&Skipped>) -> Result<()> {
    for entry in std::fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            files.push(path);
            continue;
        }
        if SKIPPED_FOLDERS.contains(&name) {
            continue;
        }
        match skipped {
            None => collect(&path, files, None)?,
            Some(_) if name == DOCS_FOLDER => {}
            Some(skipped) if name == PACKAGES_FOLDER => {
                let mounted = |name: &str| skipped.mounted.iter().any(|mounted| mounted == name);
                for entry in std::fs::read_dir(&path).with_context(|| format!("reading {}", path.display()))? {
                    let path = entry?.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    match path.is_dir() {
                        _ if name.starts_with('.') => {}
                        true if mounted(name) || skipped.is_package(&path) || SKIPPED_FOLDERS.contains(&name) => {}
                        true => collect(&path, files, None)?,
                        false => files.push(path),
                    }
                }
            }
            Some(skipped) if skipped.is_package(&path) => {}
            Some(_) => collect(&path, files, None)?,
        }
    }
    Ok(())
}

/// Every file of a package that may be an asset: anything outside its
/// `Scripts`, `Docs` and `Samples` folders.
fn collect_package(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            files.push(path);
        } else if !SKIPPED_FOLDERS.contains(&name) && !PACKAGE_FOLDERS.contains(&name) {
            collect(&path, files, None)?;
        }
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

    fn package(name: &str, root: &Path, access: Access) -> PackageMount {
        PackageMount { name: name.to_string(), title: name.to_string(), root: root.to_path_buf(), access }
    }

    #[test]
    fn a_cached_package_is_mounted_without_writing_to_it() {
        let (project, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let orbit = cache.path().join("orbit-tools-1a2b");
        for file in ["package.toml", "Models/Rig.glb", "Textures/Ring.png", "Scripts/Orbit.cs", "Docs/Orbit.md"] {
            touch(&orbit, file);
        }
        touch(&orbit, "Samples/First Orbit/Orbit.level");
        let shipped = AssetGuid::new_random();
        std::fs::write(orbit.join("Textures/Ring.png.meta"), format!("guid = \"{shipped}\"\n")).unwrap();
        touch(project.path(), "Scenes/Main.scene");

        let mounts = Mounts::new(project.path(), vec![package("orbit-tools", &orbit, Access::ReadOnly)]);
        let (db, report) = AssetDb::scan_mounted(mounts).unwrap();
        assert_eq!(db.len(), 3, "the package's scripts, docs and samples are not its assets");
        assert_eq!(report.created, vec!["Scenes/Main.scene".to_string()], "only the project's get .meta files");
        assert!(!orbit.join("Models/Rig.glb.meta").exists(), "nothing is written into the cache");
        let rig = db.guid_of("Packages/orbit-tools/Models/Rig.glb").unwrap();
        let entry = db.get(rig).unwrap();
        assert_eq!((entry.kind, &entry.mount), (AssetKind::Model, &Mount::Package("orbit-tools".into())));
        assert_eq!(db.path(rig).unwrap(), orbit.join("Models/Rig.glb"));
        assert!(!db.is_editable(rig));
        assert_eq!(db.guid_of("Packages/orbit-tools/Textures/Ring.png"), Some(shipped), "a shipped .meta's GUID");

        // Another machine keeps its cache elsewhere: the GUIDs, and so every reference, hold.
        let elsewhere = tempfile::tempdir().unwrap();
        let moved = elsewhere.path().join("cache/orbit-tools-9f8e");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&orbit, &moved).unwrap();
        let mounts = Mounts::new(project.path(), vec![package("orbit-tools", &moved, Access::ReadOnly)]);
        let (db, _) = AssetDb::scan_mounted(mounts).unwrap();
        assert_eq!(db.guid_of("Packages/orbit-tools/Models/Rig.glb"), Some(rig));
        assert_eq!(db.path(rig).unwrap(), moved.join("Models/Rig.glb"));
        let reference = crate::AssetRef::parse(&rig.to_string(), &db).unwrap();
        assert_eq!(reference.guid(), Some(rig));
        let by_path = crate::AssetRef::parse("Packages/orbit-tools/Models/Rig.glb#mesh0.0", &db).unwrap();
        assert_eq!(by_path, crate::AssetRef::asset(rig, Some("mesh0.0")), "hand-written paths name it too");
    }

    #[test]
    fn a_project_copy_of_a_package_asset_is_the_one_given_a_new_guid() {
        let (project, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let orbit = cache.path().join("orbit");
        touch(&orbit, "Rig.prefab");
        let shipped = AssetGuid::new_random();
        std::fs::write(orbit.join("Rig.prefab.meta"), format!("guid = \"{shipped}\"\n")).unwrap();
        touch(project.path(), "Prefabs/Rig.prefab");
        std::fs::copy(orbit.join("Rig.prefab.meta"), project.path().join("Prefabs/Rig.prefab.meta")).unwrap();

        let mounts = Mounts::new(project.path(), vec![package("orbit-tools", &orbit, Access::ReadOnly)]);
        let (db, report) = AssetDb::scan_mounted(mounts).unwrap();
        assert_eq!(db.guid_of("Packages/orbit-tools/Rig.prefab"), Some(shipped));
        assert_eq!(report.reassigned, vec!["Prefabs/Rig.prefab".to_string()]);
    }

    #[test]
    fn a_folder_package_in_the_project_is_seen_once_and_gets_meta_files() {
        let project = tempfile::tempdir().unwrap();
        let mine = project.path().join("Packages/my-tools");
        touch(&mine, "package.toml");
        touch(&mine, "Rocks/Rock.png");
        touch(project.path(), "Packages/Loose.png");
        let mounts = Mounts::new(project.path(), vec![package("my-tools", &mine, Access::Editable)]);
        let (db, report) = AssetDb::scan_mounted(mounts).unwrap();
        let mut paths: Vec<&str> = db.entries().map(|entry| entry.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["Packages/Loose.png", "Packages/my-tools/Rocks/Rock.png"]);
        assert!(report.created.contains(&"Packages/my-tools/Rocks/Rock.png".to_string()));
        assert!(mine.join("Rocks/Rock.png.meta").is_file(), "a folder package is the developer's own");
        let rock = db.guid_of("Packages/my-tools/Rocks/Rock.png").unwrap();
        assert!(db.is_editable(rock));
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
