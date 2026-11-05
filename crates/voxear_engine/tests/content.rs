//! The content build on a throwaway project, and loading from its bundles
//! (headless: no GPU, no scripts).

use voxear_assets::{AssetReader, bundle::Catalog};
use voxear_engine::{Content, ContentSettings, Engine, Levels, content_build};
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

/// A one-triangle FBX, a metre on a side, whose leaves' picture is looked
/// for beside it, by the name in the absolute path the artist's tool wrote.
const BUSH: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7400
}
Objects:  {
	Geometry: 1, "Geometry::Bush", "Mesh" {
		Vertices: *9 {
			a: 0,0,0,100,0,0,0,100,0
		}
		PolygonVertexIndex: *3 {
			a: 0,1,-3
		}
	}
	Model: 2, "Model::Bush", "Mesh" {
	}
	Material: 3, "Material::Leaves", "" {
	}
	Texture: 4, "Texture::Leaves", "" {
		FileName: "D:\Art\bush.png"
	}
}
Connections:  {
	C: "OO",2,0
	C: "OO",1,2
	C: "OO",3,2
	C: "OP",4,3, "DiffuseColor"
}
"#;

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

/// Bundles by usage set alone, without merging small ones, to check the grouping itself.
fn unmerged() -> ContentSettings {
    ContentSettings { merge_below_kb: 0, ..ContentSettings::default() }
}

/// Each bundle's roots and assets, in build order.
fn layout(report: &content_build::BuildReport) -> Vec<(Vec<String>, Vec<String>)> {
    report.bundles.iter().map(|b| (b.roots.clone(), b.assets.clone())).collect()
}

#[test]
fn assets_are_bundled_by_the_roots_that_use_them() {
    let dir = project();
    let output = dir.path().join("Built");
    let report = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
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
    assert!(voxear_render::CookedModel::from_bytes("Models/Tree.gltf", &cooked).is_ok());
    assert!(reader.read("Models/Tree Data.bin").is_err());
    assert_eq!(reader.read_text("Materials/Stone.material").unwrap(), MATERIAL);
}

#[test]
fn rebuilding_keeps_unchanged_bundles_and_drops_stale_ones() {
    let dir = project();
    let output = dir.path().join("Built");
    content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    let again = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    assert!(again.bundles.iter().all(|b| !b.written), "nothing changed, so nothing is written");

    std::fs::write(dir.path().join("Scenes/Cave.scene"), CAVE.replace("id = 1", "id = 1\nname = \"Big Torch\""))
        .unwrap();
    let changed = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
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
    content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
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
    let settings = ContentSettings {
        include: vec!["Materials/Unused.material".into()],
        exclude: vec!["Prefabs/Debug/**".into(), "Prefabs/Torch.prefab".into(), "Prefabs/Wolf.prefab".into()],
        ..ContentSettings::default()
    };
    let report = content_build::build(dir.path(), &dir.path().join("Built"), &settings, &[]).unwrap();
    let roots: Vec<String> = report.bundles.iter().flat_map(|b| b.roots.clone()).collect();
    let shipped: Vec<String> = report.bundles.iter().flat_map(|b| b.assets.clone()).collect();

    assert!(roots.contains(&"Materials/Unused.material".to_string()), "included");
    assert!(report.excluded.contains(&"Prefabs/Debug/Cheats.prefab".to_string()), "excluded, and nothing uses it");
    assert!(!roots.contains(&"Prefabs/Torch.prefab".to_string()), "no longer a root of its own");
    assert!(shipped.contains(&"Prefabs/Torch.prefab".to_string()), "but the cave still uses it, so it ships");
    assert!(roots.contains(&"Prefabs/Wolf.prefab".to_string()), "a behaviour field names it, so it stays a root");
}

#[test]
fn ui_documents_ship_with_their_sheets() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("UI/Styles")).unwrap();
    let hud = r#"<vui><Style src="Styles/Hud.vss" /><Panel name="hud"><Label name="score">0</Label></Panel></vui>"#;
    std::fs::write(dir.path().join("UI/Hud.vui"), hud).unwrap();
    std::fs::write(dir.path().join("UI/Styles/Hud.vss"), ".hud { gap: 4px; }").unwrap();
    let output = dir.path().join("Built");
    let report = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    let roots: Vec<String> = report.bundles.iter().flat_map(|b| b.roots.clone()).collect();
    assert!(roots.contains(&"UI/Hud.vui".to_string()), "scripts load documents by path, so each is a root");

    let mut content = Content::open_bundles(dir.path(), &output).unwrap();
    assert_eq!(content.read_text("UI/Styles/Hud.vss").unwrap(), ".hud { gap: 4px; }", "its sheet ships beside it");
    let mut ui = voxear_engine::game_ui::GameUi::default();
    let document = ui.load(&mut content, "UI/Hud.vui").expect("the game loads it from the bundles");
    assert!(ui.find(document, "score").is_some());

    std::fs::write(dir.path().join("UI/Broken.vui"), r#"<vui><Style src="Missing.vss" /></vui>"#).unwrap();
    let error = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap_err();
    assert!(format!("{error:#}").contains("UI/Broken.vui names UI/Missing.vss"), "{error:#}");
}

#[test]
fn code_roots_ship_what_scripts_load_by_path() {
    let dir = project();
    std::fs::create_dir_all(dir.path().join("Prefabs/Bosses")).unwrap();
    std::fs::write(dir.path().join("Prefabs/Bosses/Golem.prefab"), WOLF).unwrap();
    std::fs::write(dir.path().join("Prefabs/Bosses/Wyrm.prefab"), WOLF).unwrap();
    let settings = ContentSettings {
        exclude: vec!["Prefabs/Bosses/**".into(), "Materials/**".into()],
        ..ContentSettings::default()
    };
    let unused = voxear_assets::AssetDb::scan(dir.path()).unwrap().0.guid_of("Materials/Unused.material").unwrap();
    let code_root = |pattern: &str| content_build::CodeRoot { pattern: pattern.into(), source: "Game.Boss".into() };
    let code_roots =
        [code_root("Prefabs/Bosses/*.prefab"), code_root(&unused.to_string()), code_root("Prefabs/Bossses/**")];
    let report = content_build::build(dir.path(), &dir.path().join("Built"), &settings, &code_roots).unwrap();
    let roots: Vec<String> = report.bundles.iter().flat_map(|b| b.roots.clone()).collect();

    assert!(roots.contains(&"Prefabs/Bosses/Golem.prefab".to_string()), "a pattern beats exclude");
    assert!(roots.contains(&"Prefabs/Bosses/Wyrm.prefab".to_string()));
    assert!(roots.contains(&"Materials/Unused.material".to_string()), "so does a GUID");
    assert_eq!(report.unmatched, [code_roots[2].clone()], "the typo is reported");
}

#[test]
fn unchanged_models_are_not_cooked_again() {
    let dir = project();
    let output = dir.path().join("Built");
    let first = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((first.cooked, first.cached), (1, 0));
    let again = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((again.cooked, again.cached), (0, 1));

    // Changing a file the model reads cooks it again, and the old cook goes.
    let mut buffer = vec![0u8; 36];
    buffer[0..4].copy_from_slice(&1.0f32.to_le_bytes());
    std::fs::write(dir.path().join("Models/Tree Data.bin"), buffer).unwrap();
    let changed = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((changed.cooked, changed.cached), (1, 0));
    let cooks = std::fs::read_dir(content_build::cache_dir(dir.path())).unwrap().count();
    assert_eq!(cooks, 1);
}

#[test]
fn fbx_models_cook_and_cook_again_when_a_picture_they_look_for_comes() {
    let dir = project();
    std::fs::write(dir.path().join("Models/Bush.fbx"), BUSH).unwrap();
    let forest = FOREST.replace("Models/Tree.gltf", "Models/Bush.fbx#mesh0.0");
    std::fs::write(dir.path().join("Scenes/Forest.scene"), forest).unwrap();
    let output = dir.path().join("Built");
    let first = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((first.cooked, first.cached), (1, 0), "its picture is not there, which does not stop it");
    let catalog = Catalog::read(&output).unwrap();
    let mut reader = voxear_assets::bundle::BundleReader::new(&output, &catalog);
    let cooked = reader.read("Models/Bush.fbx").unwrap();
    assert!(voxear_render::CookedModel::from_bytes("Models/Bush.fbx", &cooked).is_ok(), "it ships cooked");

    let again = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((again.cooked, again.cached), (0, 1));
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([40, 160, 60, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(dir.path().join("Models/bush.png"), png.into_inner()).unwrap();
    let found = content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    assert_eq!((found.cooked, found.cached), (1, 0), "with its picture now");
}

#[test]
fn a_models_remapped_materials_and_their_pictures_ship_with_it() {
    let dir = project();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([60, 120, 40, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    std::fs::write(dir.path().join("Materials/Moss.png"), png.into_inner()).unwrap();
    let content = Content::open(dir.path()).unwrap();
    let picture = content.db().guid_of("Materials/Moss.png").unwrap();
    std::fs::write(dir.path().join("Materials/Moss.material"), format!("color_map = \"{picture}\"\n")).unwrap();
    let content = Content::open(dir.path()).unwrap();
    let moss = content.db().guid_of("Materials/Moss.material").unwrap();
    let remap = voxear_assets::MaterialRemap([("material0".to_string(), moss)].into());
    remap.write(&dir.path().join("Models/Tree.gltf")).unwrap();

    let output = dir.path().join("Built");
    let report = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    let shipped: Vec<&String> = report.bundles.iter().flat_map(|b| &b.assets).collect();
    for path in ["Materials/Moss.material", "Materials/Moss.png"] {
        assert!(shipped.iter().any(|p| *p == path), "{path} ships, as the tree draws with it: {shipped:?}");
    }
    let mut built = Content::open_bundles(dir.path(), &output).unwrap();
    let meta = built.read_text("Models/Tree.gltf.meta").expect("the tree's remapping ships beside it");
    assert_eq!(voxear_assets::MaterialRemap::from_meta(&meta), remap);
}

#[test]
fn small_bundles_merge_only_where_it_is_free() {
    let dir = project();
    let report = content_build::build(dir.path(), &dir.path().join("Built"), &ContentSettings::default(), &[]).unwrap();
    // Every bundle here is tiny. Each merged into one that all its roots open anyway: the cave's own
    // bundle went into the torch's ({Cave} is within {Cave, Torch}), and so on.
    for bundle in &report.bundles {
        for other in &report.bundles {
            let covered = bundle.roots.iter().all(|r| other.roots.contains(r));
            assert!(
                bundle.file == other.file || !covered,
                "{:?} could still merge into {:?}",
                bundle.roots,
                other.roots
            );
        }
    }
    assert!(
        report.bundles.len()
            < layout(&content_build::build(dir.path(), &dir.path().join("Plain"), &unmerged(), &[]).unwrap()).len()
    );

    // A level still opens what it needs from merged bundles.
    let mut content = Content::open_bundles(dir.path(), &dir.path().join("Built")).unwrap();
    let mut engine = Engine::new();
    Levels::default().load(&mut engine, &mut content, None, "Levels/Cave.level").unwrap();
    assert!(content.opened_bundles().iter().all(|b| b.roots.contains(&"Levels/Cave.level".to_string())));
}

#[test]
fn overlays_replace_assets_by_guid_and_add_new_ones() {
    let base = project();
    content_build::build(base.path(), &base.path().join("Built"), &ContentSettings::default(), &[]).unwrap();

    // A mod: the shared scene again, under the same GUID, and a prefab of its own.
    let overlay = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let path = overlay.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Scenes/Shared.scene", "format = 1\n[[obj]]\nid = 1\nname = \"Moss\"\n");
    let meta = std::fs::read_to_string(base.path().join("Scenes/Shared.scene.meta")).unwrap();
    write("Scenes/Shared.scene.meta", &meta);
    write("Prefabs/Hat.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"Hat\"\n");
    let settings = ContentSettings { include: vec!["Scenes/**".into()], ..ContentSettings::default() };
    content_build::build(overlay.path(), &overlay.path().join("Built"), &settings, &[]).unwrap();

    let mut content = Content::open_bundles(base.path(), &base.path().join("Built")).unwrap();
    content.mount(&overlay.path().join("Built")).unwrap();
    let mut engine = Engine::new();
    Levels::default().load(&mut engine, &mut content, None, "Levels/Cave.level").unwrap();
    let names: Vec<String> = engine.world.query_ref::<&Name>().into_iter().map(|n| n.0.clone()).collect();
    assert!(names.contains(&"Moss".to_string()) && !names.contains(&"Ground".to_string()), "{names:?}");
    assert!(content.prefab(None, "Prefabs/Hat.prefab").is_ok(), "the mod's own prefab");
    assert!(content.prefab(None, "Prefabs/Torch.prefab").is_ok(), "the game's prefabs are still there");

    let mut project_folder = Content::open(base.path()).unwrap();
    assert!(project_folder.mount(&overlay.path().join("Built")).is_err(), "layers need built content");
}

#[test]
fn asset_tables_ship_with_every_languages_asset() {
    let dir = project();
    let table = "format = 1\n\n[[entry]]\nkey = \"sign.welcome\"\nen = \"Materials/Unused.material\"\n";
    std::fs::create_dir_all(dir.path().join("Text")).unwrap();
    std::fs::write(dir.path().join("Text/Signs.assettable"), table).unwrap();
    let output = dir.path().join("Built");
    let report = content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    let roots: Vec<String> = report.bundles.iter().flat_map(|b| b.roots.clone()).collect();
    assert!(roots.contains(&"Text/Signs.assettable".to_string()), "the game reads every table as it starts");
    assert!(report.excluded.is_empty(), "the asset only the table names ships: {:?}", report.excluded);
}

#[test]
fn custom_shaders_ship_compiled_and_compile_again_only_when_changed() {
    const PAINT: &str = "struct Paint\n{\n    [Property, Color] float3 color = float3(1, 0.5, 0);\n}\n\n\
        void surface(Paint paint, SurfaceInput input, inout SurfaceOutput surface)\n\
        {\n    surface.color = paint.color;\n}\n";
    let dir = project();
    std::fs::write(dir.path().join("Materials/Paint.shader"), PAINT).unwrap();
    let output = dir.path().join("Built");
    content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    let shaders = output.join(content_build::SHADERS_FOLDER);
    let forward = shaders.join("forward.spv");
    assert!(forward.is_file() && shaders.join("glass.spv").is_file(), "the modules that draw materials are compiled");
    let content = Content::open_bundles(dir.path(), &output).unwrap();
    assert_eq!(content.shaders().shaders.len(), 1, "the shader ships, numbered as it was compiled");
    let modules = content.built_shaders().unwrap().unwrap();
    assert!(modules.iter().any(|(name, spirv)| name == "forward" && spirv.len() > 1000), "the game reads them back");

    let compiled = forward.metadata().unwrap().modified().unwrap();
    content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    assert_eq!(forward.metadata().unwrap().modified().unwrap(), compiled, "unchanged, so not compiled again");

    std::fs::remove_file(dir.path().join("Materials/Paint.shader")).unwrap();
    content_build::build(dir.path(), &output, &unmerged(), &[]).unwrap();
    assert!(!shaders.exists(), "a game without custom shaders has none compiled");
}
