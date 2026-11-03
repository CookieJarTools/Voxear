//! Packages' assets as part of a project: mounted under `Packages/<name>/`,
//! named from its scenes and behaviour fields, loaded, and shipped by the
//! content build when they are used. Projects and packages are made in
//! temporary folders (headless: no GPU, no scripts).

use std::path::{Path, PathBuf};

use voxear_assets::{Access, AssetDb, Mount, Mounts, PackageMount, bundle::Catalog};
use voxear_engine::{Content, ContentSettings, Engine, Levels, content_build};
use voxear_scene::Name;

const LEVEL: &str = "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n";
const RIG: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Rig\"\n[obj.renderer]\nmesh = \"builtin:cube\"\nmaterial = \"Packages/orbit-tools/Materials/Ring.material\"\n";
const UNUSED: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Unused\"\n";
const MATERIAL: &str = "color = [0.5, 0.5, 0.5]\n";

fn write(folder: &Path, file: &str, text: &str) {
    let path = folder.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The orbit-tools package: a prefab the game uses with its material, one
/// it does not, and scripts and pages that are not assets.
fn orbit(folder: &Path) -> PathBuf {
    write(folder, "package.toml", "[package]\nname = \"orbit-tools\"\nversion = \"1.2.0\"\n");
    write(folder, "Prefabs/Rig.prefab", RIG);
    write(folder, "Prefabs/Unused.prefab", UNUSED);
    write(folder, "Materials/Ring.material", MATERIAL);
    write(folder, "Scripts/Orbit.cs", "namespace Orbit;\n");
    write(folder, "Docs/Orbit Cameras.md", "# Orbit Cameras\n");
    folder.to_path_buf()
}

/// A project whose one scene places the package's rig, and whose `extra`
/// lines add to the scene.
fn project(folder: &Path, extra: &str) {
    write(folder, "voxear.toml", "[project]\nname = \"Game\"\n");
    write(folder, "Levels/Main.level", LEVEL);
    let scene = format!("format = 1\n[[obj]]\nid = 1\nprefab = \"Packages/orbit-tools/Prefabs/Rig.prefab\"\n{extra}");
    write(folder, "Scenes/Main.scene", &scene);
}

fn unmerged() -> ContentSettings {
    ContentSettings { merge_below_kb: 0, ..ContentSettings::default() }
}

fn names(engine: &Engine) -> Vec<String> {
    let mut names: Vec<String> = engine.world.query_ref::<&Name>().into_iter().map(|n| n.0.clone()).collect();
    names.sort();
    names
}

#[test]
fn a_folder_package_asked_for_in_packages_toml_ships_what_the_game_uses() {
    let temp = tempfile::tempdir().unwrap();
    orbit(&temp.path().join("orbit"));
    let root = temp.path().join("Game");
    // A behaviour field names the package's material, as `Ref<Material>` does.
    let field = "[[obj]]\nid = 2\nname = \"Spawner\"\nbehaviours = [\"Game.Spawner\"]\nfields.\"Game.Spawner\" = { \
                 look = \"Packages/orbit-tools/Materials/Ring.material\" }\n";
    project(&root, field);
    write(&root, "packages.toml", "[packages]\norbit-tools = { path = \"../orbit\" }\n");

    let output = root.join("Built");
    let report = content_build::build(&root, &output, &unmerged(), &[]).unwrap();
    assert_eq!(report.excluded, ["Packages/orbit-tools/Prefabs/Unused.prefab"], "a package's prefab ships when used");
    let catalog = Catalog::read(&output).unwrap();
    let rig = catalog.assets.iter().find(|a| a.path == "Packages/orbit-tools/Prefabs/Rig.prefab").unwrap();
    assert_eq!(rig.package.as_deref(), Some("orbit-tools"));
    let ring = catalog.assets.iter().find(|a| a.path == "Packages/orbit-tools/Materials/Ring.material").unwrap();
    assert_eq!(ring.package.as_deref(), Some("orbit-tools"));
    assert!(temp.path().join("orbit/Prefabs/Rig.prefab.meta").is_file(), "a folder package gets .meta files");

    // The built game finds the package's assets as the project did.
    let content = Content::open_bundles(&root, &output).unwrap();
    let entry = content.db().get(content.db().guid_of("Packages/orbit-tools/Prefabs/Rig.prefab").unwrap()).unwrap();
    assert_eq!(entry.mount, Mount::Package("orbit-tools".into()));
}

#[test]
fn a_cached_package_loads_and_builds_and_its_references_survive_the_cache_moving() {
    let (temp, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let orbit = orbit(&cache.path().join("orbit-tools-1a2b"));
    let cached = |root: &Path| PackageMount {
        name: "orbit-tools".into(),
        title: "Orbit Tools".into(),
        root: root.to_path_buf(),
        access: Access::ReadOnly,
    };
    let root = temp.path().join("Game");
    project(&root, "");
    let mounts = Mounts::new(&root, vec![cached(&orbit)]);

    // The scene names the rig by its GUID, as the editor saves it.
    let (db, _) = AssetDb::scan_mounted(mounts.clone()).unwrap();
    let rig = db.guid_of("Packages/orbit-tools/Prefabs/Rig.prefab").unwrap();
    let scene = format!("format = 1\n[[obj]]\nid = 1\nprefab = \"{rig}\"\n");
    write(&root, "Scenes/Main.scene", &scene);

    let mut content = Content::open_mounted(mounts.clone()).unwrap();
    let (mut engine, mut levels) = (Engine::new(), Levels::default());
    levels.load(&mut engine, &mut content, None, "Levels/Main.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Rig"]);

    let output = root.join("Built");
    let report = content_build::build_mounted(&mounts, &output, &unmerged(), &[], &mut |_, _, _| Ok(())).unwrap();
    let shipped: Vec<&String> = report.bundles.iter().flat_map(|b| &b.assets).collect();
    assert!(shipped.contains(&&"Packages/orbit-tools/Prefabs/Rig.prefab".to_string()));
    assert!(shipped.contains(&&"Packages/orbit-tools/Materials/Ring.material".to_string()));
    let written: Vec<_> =
        walk(&orbit).into_iter().filter(|file| file.extension().is_some_and(|e| e == "meta")).collect();
    assert!(written.is_empty(), "nothing is written into the cache: {written:?}");

    let mut built = Content::open_bundles(&root, &output).unwrap();
    let (mut engine, mut levels) = (Engine::new(), Levels::default());
    levels.load(&mut engine, &mut built, None, "Levels/Main.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Rig"], "the built game has the package's rig");

    // On another machine the cache is somewhere else; the scene still finds the rig.
    let elsewhere = tempfile::tempdir().unwrap();
    let moved = elsewhere.path().join("orbit-tools-9f8e");
    std::fs::rename(&orbit, &moved).unwrap();
    let mut content = Content::open_mounted(Mounts::new(&root, vec![cached(&moved)])).unwrap();
    let (mut engine, mut levels) = (Engine::new(), Levels::default());
    levels.load(&mut engine, &mut content, None, "Levels/Main.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Rig"]);
}

#[test]
fn import_settings_for_a_cached_package_are_kept_in_the_project() {
    let (temp, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let orbit = orbit(&cache.path().join("orbit"));
    write(&orbit, "Sounds/Whoosh.wav", "");
    let root = temp.path().join("Game");
    project(&root, "");
    let package = PackageMount {
        name: "orbit-tools".into(),
        title: "Orbit Tools".into(),
        root: orbit.clone(),
        access: Access::ReadOnly,
    };
    let mounts = Mounts::new(&root, vec![package]);

    let whoosh = "Packages/orbit-tools/Sounds/Whoosh.wav";
    let settings = voxear_assets::SoundImport { volume: 0.5, ..Default::default() };
    settings.write_meta(&mounts.writable_meta(whoosh).unwrap()).unwrap();
    assert!(root.join("Settings/Packages/orbit-tools/Sounds/Whoosh.wav.meta").is_file());
    assert!(!orbit.join("Sounds/Whoosh.wav.meta").exists());
    assert_eq!(voxear_assets::SoundImport::read_meta(&mounts.meta_file(whoosh)).volume, 0.5);

    // The content reads the project's settings in place of the package's.
    let mut content = Content::open_mounted(mounts).unwrap();
    let text = content.read_text(&format!("{whoosh}.meta")).unwrap();
    assert_eq!(voxear_assets::SoundImport::from_meta(&text).volume, 0.5);
}

fn walk(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
