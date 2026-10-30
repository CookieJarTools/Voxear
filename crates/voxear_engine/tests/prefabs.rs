//! Prefabs on a throwaway project, headless (no GPU, no scripts).

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Content, Engine, ObjPath,
    prefab::{self, FromPrefab, PrefabInstance},
    scene_file,
};
use voxear_render::Light;
use voxear_scene::{Name, Parent, Placement, Transform};

const LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Lamp"
position = [0.0, 3.0, 0.0]

[[obj]]
id = 2
name = "Glow"
parent = 1
position = [0.0, 0.5, 0.0]
light = { type = "point", color = [1.0, 1.0, 1.0], intensity = 5.0 }
"#;

// A variant: its root is a Lamp, tinted red, with a child of its own.
const RED_LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Red Lamp"
prefab = "Prefabs/Lamp.prefab"
[obj.overrides.2]
light = { type = "point", color = [1.0, 0.0, 0.0], intensity = 5.0 }

[[obj]]
id = 2
name = "Spark"
parent = 1
"#;

// Nesting: a post with a lamp on top.
const STREET_LIGHT: &str = r#"
format = 1

[[obj]]
id = 1
name = "Street Light"

[[obj]]
id = 2
prefab = "Prefabs/Lamp.prefab"
parent = 1
position = [0.0, 4.0, 0.0]
"#;

const SCENE: &str = r#"
format = 1

[[obj]]
id = 1
name = "Street"

[[obj]]
id = 2
prefab = "Prefabs/StreetLight.prefab"
name = "Corner Light"
parent = 1
position = [10.0, 0.0, 0.0]
[obj.overrides."2/2"]
position = [0.0, 1.0, 0.0]

[[obj]]
id = 3
prefab = "Prefabs/RedLamp.prefab"
position = [-5.0, 0.0, 0.0]
"#;

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn street() -> tempfile::TempDir {
    project(&[
        ("Prefabs/Lamp.prefab", LAMP),
        ("Prefabs/RedLamp.prefab", RED_LAMP),
        ("Prefabs/StreetLight.prefab", STREET_LIGHT),
        ("Scenes/Street.scene", SCENE),
    ])
}

fn find(engine: &Engine, name: &str) -> Entity {
    let found = engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name);
    found.unwrap_or_else(|| panic!("no Obj named {name}")).0
}

fn parent_name(engine: &Engine, entity: Entity) -> String {
    let parent = engine.world.get::<Parent>(entity).expect("has a parent").0;
    engine.world.get::<Name>(parent).unwrap().0.clone()
}

#[test]
fn nested_prefabs_and_variants_expand_with_their_overrides() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Scenes/Street.scene")).unwrap();
    let objs = prefab::expand(&mut content, &scene.objs).unwrap();
    let paths: Vec<String> = objs.iter().map(|o| o.path.to_string()).collect();
    assert_eq!(paths, ["1", "2", "2/2", "2/2/2", "3", "3/1/2", "3/2"]);

    let mut engine = Engine::new();
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let transform = |name| *engine.world.get::<Transform>(find(&engine, name)).unwrap();

    // The instance names and places the root; the nested lamp keeps its name.
    assert_eq!(parent_name(&engine, find(&engine, "Corner Light")), "Street");
    assert_eq!(transform("Corner Light").position, Vec3::new(10.0, 0.0, 0.0));
    let lamps: Vec<Entity> = engine
        .world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .filter(|(_, n)| n.0 == "Lamp")
        .map(|(e, _)| e)
        .collect();
    assert_eq!(lamps.len(), 1);
    assert_eq!(parent_name(&engine, lamps[0]), "Corner Light");
    assert_eq!(engine.world.get::<Transform>(lamps[0]).unwrap().position, Vec3::new(0.0, 4.0, 0.0));

    // The scene overrides a Glow two prefabs deep; the variant tints its own.
    let glows: Vec<(Entity, Light, Transform)> = engine
        .world
        .query_ref::<(Entity, &Name, &Light, &Transform)>()
        .into_iter()
        .filter(|(_, n, ..)| n.0 == "Glow")
        .map(|(e, _, l, t)| (e, *l, *t))
        .collect();
    assert_eq!(glows.len(), 2);
    let corner = glows.iter().find(|(e, ..)| parent_name(&engine, *e) == "Lamp").unwrap();
    assert_eq!(corner.2.position, Vec3::new(0.0, 1.0, 0.0));
    let red = glows.iter().find(|(e, ..)| parent_name(&engine, *e) == "Red Lamp").unwrap();
    assert_eq!(red.1.color, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(parent_name(&engine, find(&engine, "Spark")), "Red Lamp");

    // Every Obj from an instance points at the outermost instance's root.
    let corner_light = find(&engine, "Corner Light");
    assert!(engine.world.has::<PrefabInstance>(corner_light));
    let link = engine.world.get::<FromPrefab>(corner.0).unwrap();
    assert_eq!((link.instance, link.path.to_string()), (corner_light, "2/2".to_string()));
}

#[test]
fn saving_keeps_instances_and_writes_changes_as_overrides() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let scene = scene_file::read(&dir.path().join("Scenes/Street.scene")).unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();

    // Move the red lamp's glow, as an editor would.
    let red_glow = engine
        .world
        .query_ref::<(Entity, &Name, &Parent)>()
        .into_iter()
        .find(|(_, n, p)| n.0 == "Glow" && engine.world.get::<Name>(p.0).unwrap().0 == "Red Lamp")
        .unwrap()
        .0;
    engine.world.get_mut::<Transform>(red_glow).unwrap().position = Vec3::new(0.0, 2.0, 0.0);

    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    assert_eq!(saved.objs.len(), 3, "Objs inside instances are not written on their own");
    let corner = saved.objs.iter().find(|o| o.name == "Corner Light").unwrap();
    assert!(corner.prefab.is_some());
    assert_eq!(corner.position, Some([10.0, 0.0, 0.0]));
    assert_eq!(corner.overrides.keys().collect::<Vec<_>>(), ["2/2"]);
    let red = saved.objs.iter().find(|o| o.prefab.is_some() && o.name.is_empty()).expect("keeps the variant's name");
    assert_eq!(red.overrides.keys().collect::<Vec<_>>(), ["1/2"]);
    assert_eq!(red.overrides["1/2"].get("position").unwrap().to_string(), "[0.0, 2.0, 0.0]");

    // And it loads back the same.
    let path = dir.path().join("Scenes/Copy.scene");
    scene_file::write(&path, &saved).unwrap();
    let mut reloaded = Engine::new();
    let copy = scene_file::read(&path).unwrap();
    scene_file::spawn(&mut reloaded, &mut content, None, &copy, &Default::default()).unwrap();
    assert_eq!(reloaded.world.entity_count(), engine.world.entity_count());
    let moved = reloaded
        .world
        .query_ref::<(&Name, &Transform)>()
        .into_iter()
        .any(|(n, t)| n.0 == "Glow" && t.position == Vec3::new(0.0, 2.0, 0.0));
    assert!(moved);
}

#[test]
fn instantiate_places_a_copy_under_a_parent() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let id = content.prefab(None, "Prefabs/RedLamp.prefab").unwrap();
    assert_eq!(content.prefab(None, "Prefabs/RedLamp.prefab").unwrap(), id, "loading twice returns the same prefab");
    let template = content.template(id).unwrap().clone();

    let holder = engine.world.spawn((Name("Holder".into()), Transform::IDENTITY));
    let placement = Placement::at(Vec3::new(1.0, 2.0, 3.0));
    let first = engine.instantiate(&template, Some(holder), Some(placement)).unwrap();
    let second = engine.instantiate(&template, None, None).unwrap();

    assert_eq!(engine.world.get::<Name>(first).unwrap().0, "Red Lamp");
    assert_eq!(engine.world.get::<Parent>(first).unwrap().0, holder);
    assert_eq!(engine.world.get::<Transform>(first).unwrap().position, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(engine.world.get::<Transform>(second).unwrap().position, Vec3::new(0.0, 3.0, 0.0), "the prefab's own");
    assert!(engine.world.get::<Parent>(second).is_none());
    let link = engine.world.get::<FromPrefab>(first).unwrap();
    assert_eq!((link.instance, link.path.clone()), (first, ObjPath::default()));
    // Holder, then two copies of the root, glow and spark.
    assert_eq!(engine.world.entity_count(), 7);
}

#[test]
fn mistakes_are_reported_clearly() {
    let looped = project(&[
        ("A.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"A\"\n[[obj]]\nid = 2\nparent = 1\nprefab = \"B.prefab\"\n"),
        ("B.prefab", "format = 1\n[[obj]]\nid = 1\nprefab = \"A.prefab\"\n"),
        ("Two.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"A\"\n[[obj]]\nid = 2\nname = \"B\"\n"),
        ("Typo.prefab", "format = 1\n[[obj]]\nid = 1\nprefab = \"Two.prefab\"\n"),
    ]);
    let mut content = Content::open(looped.path()).unwrap();
    let error = |reference: &str, content: &mut Content| format!("{:#}", content.prefab(None, reference).unwrap_err());
    assert!(error("A.prefab", &mut content).contains("contains an instance of itself"));
    assert!(error("Two.prefab", &mut content).contains("more than one root"));

    let lamp = project(&[("Lamp.prefab", LAMP)]);
    let mut content = Content::open(lamp.path()).unwrap();
    let scene = |overrides: &str| -> scene_file::SceneFile {
        toml::from_str(&format!("format = 1\n[[obj]]\nid = 1\nprefab = \"Lamp.prefab\"\n{overrides}")).unwrap()
    };
    let expand =
        |text: &str, content: &mut Content| format!("{:#}", prefab::expand(content, &scene(text).objs).unwrap_err());
    assert!(expand("[obj.overrides.9]\nname = \"X\"\n", &mut content).contains("no Obj at that path"));
    assert!(expand("[obj.overrides.2]\nparent = 1\n", &mut content).contains("`parent` cannot be overridden"));
    assert!(expand("[obj.overrides.2]\nglow = 1\n", &mut content).contains("glow"), "typos are caught");
}
