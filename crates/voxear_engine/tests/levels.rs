//! Scenes and levels on a throwaway project, headless (no GPU, no scripts).

use voxear_engine::{Content, Engine, Levels, scene_file};
use voxear_render::{Camera, PostProcess};
use voxear_scene::{Name, Parent, ScenePersistence, Transform};

const ENVIRONMENT: &str = r#"
format = 1

[[obj]]
id = 1
name = "Main Camera"
position = [0.0, 2.0, -5.0]
rotation = [10.0, 0.0, 0.0]
[obj.camera]
fov = 55.0

[[obj]]
id = 2
name = "Floor"
"#;

const DAY: &str = r#"
format = 1

[sky]
zenith = [0.5, 0.7, 0.9]
horizon = [0.8, 0.85, 0.9]
ground = [0.5, 0.48, 0.46]
ambient = 1.0

[post]
preset = "cinematic"
[post.bloom]
intensity = 0.1

[[obj]]
id = 1
name = "Sun"
rotation = [45.0, -30.0, 0.0]
light = { type = "directional", color = [1.0, 0.95, 0.9], intensity = 3.0 }

[[obj]]
id = 2
name = "Manager"
persistent = true

[[obj]]
id = 3
name = "Manager child"
parent = 2

[[obj]]
id = 4
name = "Tree"
position = [3.0, 0.0, 1.0]
scale = [1.0, 2.0, 1.0]
"#;

const NIGHT: &str = r#"
format = 1

[[obj]]
id = 1
name = "Lamp"
position = [0.0, 1.0, 0.0]
light = { type = "point", color = [1.0, 0.5, 0.2], intensity = 5.0, range = 4.0 }
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Scenes/Environment.scene", ENVIRONMENT);
    write("Scenes/Day.scene", DAY);
    write("Scenes/Night.scene", NIGHT);
    write(
        "Levels/Day.level",
        "format = 1\n[[scene]]\nscene = \"Scenes/Environment.scene\"\n[[scene]]\nscene = \"Scenes/Day.scene\"\n",
    );
    write(
        "Levels/Night.level",
        "format = 1\n[[scene]]\nscene = \"Scenes/Environment.scene\"\n[[scene]]\nscene = \"Scenes/Night.scene\"\n",
    );
    dir
}

fn names(engine: &Engine) -> Vec<String> {
    let mut names: Vec<String> = engine.world.query_ref::<&Name>().into_iter().map(|n| n.0.clone()).collect();
    names.sort();
    names
}

#[test]
fn levels_keep_shared_scenes_and_persistent_objs() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let mut levels = Levels::default();

    let change = levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    assert_eq!(change.loaded, ["Scenes/Environment.scene", "Scenes/Day.scene"]);
    assert_eq!(engine.world.entity_count(), 6);
    let camera = engine.world.query_ref::<(&Name, &Camera)>().into_iter().next().unwrap();
    assert_eq!(camera.0.0, "Main Camera");
    assert_eq!(camera.1.fov_y_degrees, 55.0);

    let change = levels.load(&mut engine, &mut content, None, "Levels/Night.level").unwrap();
    assert_eq!(change.kept, ["Scenes/Environment.scene"]);
    assert_eq!(change.unloaded, ["Scenes/Day.scene"]);
    assert_eq!(change.loaded, ["Scenes/Night.scene"]);
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Floor", "Lamp", "Main Camera", "Manager", "Manager child"]);

    // Reloading Day must not make a second Manager.
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Floor", "Main Camera", "Manager", "Manager child", "Sun", "Tree"]);
    let managers = engine.world.query_ref::<&ScenePersistence>().count();
    assert_eq!(managers, 1);
    let child = engine.world.query_ref::<(&Name, &Parent)>().into_iter().find(|(n, _)| n.0 == "Manager child");
    assert!(child.is_some(), "the persistent Obj keeps its child");
}

#[test]
fn objs_created_at_runtime_unload_with_the_active_scene() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let mut levels = Levels::default();
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    engine.spawn_obj("Bullet", Transform::IDENTITY, &[]).unwrap();
    let keeper = engine.spawn_obj("Keeper", Transform::IDENTITY, &[]).unwrap();
    engine.world.insert(keeper, ScenePersistence);

    levels.load(&mut engine, &mut content, None, "Levels/Night.level").unwrap();
    engine.tick(1.0 / 60.0);
    let names = names(&engine);
    assert!(!names.contains(&"Bullet".to_string()), "Day was the active scene, and it unloaded");
    assert!(names.contains(&"Keeper".to_string()), "persistent runtime Objs survive");
}

#[test]
fn scenes_survive_a_save_and_load_round_trip() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let original = scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &original, &Default::default()).unwrap();
    let entities: Vec<_> = spawned.iter().map(|(_, e)| *e).collect();

    let captured = scene_file::capture(&engine.world, &mut content, &entities);
    let saved = dir.path().join("Scenes/Copy.scene");
    scene_file::write(&saved, &captured).unwrap();
    let reloaded = scene_file::read(&saved).unwrap();

    assert_eq!(toml::to_string(&reloaded).unwrap(), toml::to_string(&captured).unwrap());
    assert_eq!(reloaded.objs.len(), 4);
    let tree = reloaded.objs.iter().find(|o| o.name == "Tree").unwrap();
    assert_eq!(tree.position, Some([3.0, 0.0, 1.0]));
    assert_eq!(tree.scale, Some([1.0, 2.0, 1.0]));
    let sun = reloaded.objs.iter().find(|o| o.name == "Sun").unwrap();
    assert_eq!(sun.rotation, Some([45.0, -30.0, 0.0]), "Euler angles come back as written");
    assert!(sun.light.is_some());
    let child = reloaded.objs.iter().find(|o| o.name == "Manager child").unwrap();
    let manager = reloaded.objs.iter().find(|o| o.name == "Manager").unwrap();
    assert_eq!(child.parent, Some(manager.id));
    assert!(manager.persistent);
    assert_eq!(reloaded.sky, original.sky);
    assert_eq!(reloaded.post, original.post, "[post] keeps its preset and overrides as written");
}

#[test]
fn scenes_set_post_processing() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let scene = scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let post = engine.world.resource::<PostProcess>().unwrap();
    assert_eq!(post.bloom.intensity, 0.1);
    assert_eq!(post.tonemap.mapper, PostProcess::cinematic().tonemap.mapper);
    assert!(post.motion_blur.enabled, "the rest comes from the preset");
}
