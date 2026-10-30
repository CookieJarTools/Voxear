//! The content build on a throwaway project, and loading from its bundles
//! (headless: no GPU, no scripts).

use voxear_assets::{AssetReader, bundle::Catalog};
use voxear_engine::{Content, Engine, Levels, content_build};
use voxear_scene::Name;

const SHARED: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.renderer]\nmesh = \"builtin:plane\"\nmaterial = \"Materials/Stone.material\"\n";
const FOREST: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Tree\"\n[obj.renderer]\nmesh = \"Models/Tree.gltf\"\nmaterial = \"Materials/Bark.material\"\n[[obj]]\nid = 2\nname = \"Spawner\"\nbehaviours = [\"Game.Spawner\"]\nfields.\"Game.Spawner\" = { enemy = \"Prefabs/Wolf.prefab\" }\n";
const CAVE: &str = "format = 1\n[[obj]]\nid = 1\nprefab = \"Prefabs/Torch.prefab\"\n";
const TORCH: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Torch\"\nlight = { type = \"point\" }\n";
const WOLF: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Wolf\"\n";
const MATERIAL: &str = "color = [0.5, 0.5, 0.5]\n";

/// A one-triangle glTF whose buffer lives in a file beside it.
const TREE: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Tree%20Data.bin", "byteLength": 36 }],
  "bufferViews": [{ "buffer": 0, "byteLength": 36 }],
  "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] }],
  "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
  "nodes": [{ "mesh": 0 }],
  "scenes": [{ "nodes": [0] }]
}"#;

fn level(scenes: &[&str]) -> String {
    let mut text = "format = 1\n".to_string();
    for scene in scenes {
        text += &format!("[[scene]]\nscene = \"{scene}\"\n");
    }
    text
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 11] = [
        ("Levels/Forest.level", level(&["Scenes/Shared.scene", "Scenes/Forest.scene"]).leak().as_bytes()),
        ("Levels/Cave.level", level(&["Scenes/Shared.scene", "Scenes/Cave.scene"]).leak().as_bytes()),
        ("Scenes/Shared.scene", SHARED.as_bytes()),
        ("Scenes/Forest.scene", FOREST.as_bytes()),
        ("Scenes/Cave.scene", CAVE.as_bytes()),
        ("Prefabs/Torch.prefab", TORCH.as_bytes()),
        ("Prefabs/Wolf.prefab", WOLF.as_bytes()),
        ("Materials/Stone.material", MATERIAL.as_bytes()),
        ("Materials/Bark.material", MATERIAL.as_bytes()),
        ("Materials/Unused.material", MATERIAL.as_bytes()),
        ("Models/Tree.gltf", TREE.as_bytes()),
    ];
    for (path, bytes) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::write(dir.path().join("Models/Tree Data.bin"), [0u8; 36]).unwrap();
    dir
}

/// Each bundle's roots and assets, in build order.
fn layout(report: &content_build::BuildReport) -> Vec<(Vec<String>, Vec<String>)> {
    report.bundles.iter().map(|b| (b.roots.clone(), b.assets.clone())).collect()
}

#[test]
fn assets_are_bundled_by_the_roots_that_use_them() {
    let dir = project();
    let output = dir.path().join("Built");
    let report = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        layout(&report),
        [
            (strings(&["Levels/Cave.level"]), strings(&["Levels/Cave.level", "Scenes/Cave.scene"])),
            (
                strings(&["Levels/Cave.level", "Levels/Forest.level"]),
                strings(&["Materials/Stone.material", "Scenes/Shared.scene"])
            ),
            (strings(&["Levels/Cave.level", "Prefabs/Torch.prefab"]), strings(&["Prefabs/Torch.prefab"])),
            (
                strings(&["Levels/Forest.level"]),
                strings(&["Levels/Forest.level", "Materials/Bark.material", "Models/Tree.gltf", "Scenes/Forest.scene"])
            ),
            // Named by a behaviour field, so it loads on its own rather than with the forest.
            (strings(&["Prefabs/Wolf.prefab"]), strings(&["Prefabs/Wolf.prefab"])),
        ]
    );
    assert_eq!(report.excluded, ["Materials/Unused.material"]);

    // The model ships cooked, with its buffer baked in, so the buffer file does not ship.
    let catalog = Catalog::read(&output).unwrap();
    let tree = catalog.assets.iter().find(|a| a.path == "Models/Tree.gltf").unwrap();
    let files: Vec<&str> = tree.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!((files, tree.cooked), (vec!["Models/Tree.gltf"], true));
    let mut reader = voxear_assets::bundle::BundleReader::new(&output, &catalog);
    let cooked = reader.read("Models/Tree.gltf").unwrap();
    assert!(voxear_render::PreparedModel::from_bytes("Models/Tree.gltf", &cooked).is_ok());
    assert!(reader.read("Models/Tree Data.bin").is_err());
    assert_eq!(reader.read_text("Materials/Stone.material").unwrap(), MATERIAL);
}

#[test]
fn rebuilding_keeps_unchanged_bundles_and_drops_stale_ones() {
    let dir = project();
    let output = dir.path().join("Built");
    content_build::build(dir.path(), &output, &Default::default()).unwrap();
    let again = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    assert!(again.bundles.iter().all(|b| !b.written), "nothing changed, so nothing is written");

    std::fs::write(dir.path().join("Scenes/Cave.scene"), CAVE.replace("id = 1", "id = 1\nname = \"Big Torch\""))
        .unwrap();
    let changed = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    let written: Vec<&Vec<String>> = changed.bundles.iter().filter(|b| b.written).map(|b| &b.roots).collect();
    assert_eq!(written, [&vec!["Levels/Cave.level".to_string()]], "only the cave's own bundle changes");
    let bundles = std::fs::read_dir(&output)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "bundle"))
        .count();
    assert_eq!(bundles, changed.bundles.len(), "the old cave bundle is removed");
}

#[test]
fn a_level_opens_only_the_bundles_it_needs() {
    let dir = project();
    // Headless runs have no scripts, so the forest loses its spawner here.
    let forest = FOREST.split("[[obj]]\nid = 2").next().unwrap();
    std::fs::write(dir.path().join("Scenes/Forest.scene"), forest).unwrap();
    let output = dir.path().join("Built");
    content_build::build(dir.path(), &output, &Default::default()).unwrap();
    let mut content = Content::open_bundles(dir.path(), &output).unwrap();
    let mut engine = Engine::new();
    let mut levels = Levels::default();

    levels.load(&mut engine, &mut content, None, "Levels/Cave.level").unwrap();
    let opened =
        |content: &Content| -> Vec<Vec<String>> { content.opened_bundles().iter().map(|b| b.roots.clone()).collect() };
    assert!(opened(&content).iter().all(|roots| roots.contains(&"Levels/Cave.level".to_string())));
    assert_eq!(opened(&content).len(), 3, "the cave's own, the shared and the torch's");

    levels.load(&mut engine, &mut content, None, "Levels/Forest.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(opened(&content).len(), 4, "the forest adds only its own; the shared bundle is already open");
    let mut names: Vec<String> = engine.world.query_ref::<&Name>().into_iter().map(|n| n.0.clone()).collect();
    names.sort();
    assert_eq!(names, ["Ground", "Tree"]);
}

#[test]
fn content_settings_add_and_remove_roots() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("Prefabs/Debug")).unwrap();
    std::fs::write(dir.path().join("Prefabs/Debug/Cheats.prefab"), WOLF).unwrap();
    let settings = voxear_engine::ContentSettings {
        include: vec!["Materials/Unused.material".into()],
        exclude: vec!["Prefabs/Debug/**".into(), "Prefabs/Torch.prefab".into(), "Prefabs/Wolf.prefab".into()],
    };
    let report = content_build::build(dir.path(), &dir.path().join("Built"), &settings).unwrap();
    let roots: Vec<String> = report.bundles.iter().flat_map(|b| b.roots.clone()).collect();
    let shipped: Vec<String> = report.bundles.iter().flat_map(|b| b.assets.clone()).collect();

    assert!(roots.contains(&"Materials/Unused.material".to_string()), "included");
    assert!(report.excluded.contains(&"Prefabs/Debug/Cheats.prefab".to_string()), "excluded, and nothing uses it");
    assert!(!roots.contains(&"Prefabs/Torch.prefab".to_string()), "no longer a root of its own");
    assert!(shipped.contains(&"Prefabs/Torch.prefab".to_string()), "but the cave still uses it, so it ships");
    assert!(roots.contains(&"Prefabs/Wolf.prefab".to_string()), "a behaviour field names it, so it stays a root");
}

#[test]
fn unchanged_models_are_not_cooked_again() {
    let dir = project();
    let output = dir.path().join("Built");
    let first = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    assert_eq!((first.cooked, first.cached), (1, 0));
    let again = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    assert_eq!((again.cooked, again.cached), (0, 1));

    // Changing a file the model reads cooks it again, and the old cook goes.
    let mut buffer = vec![0u8; 36];
    buffer[0..4].copy_from_slice(&1.0f32.to_le_bytes());
    std::fs::write(dir.path().join("Models/Tree Data.bin"), buffer).unwrap();
    let changed = content_build::build(dir.path(), &output, &Default::default()).unwrap();
    assert_eq!((changed.cooked, changed.cached), (1, 0));
    let cooks = std::fs::read_dir(content_build::cache_dir(dir.path())).unwrap().count();
    assert_eq!(cooks, 1);
}
