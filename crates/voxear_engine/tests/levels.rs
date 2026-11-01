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
    // Day and Night stand on World, as LayerCake's sets stand on a base: its Environment stays loaded.
    write("Levels/World.level", "format = 1\n[[scene]]\nscene = \"Scenes/Environment.scene\"\n");
    write(
        "Levels/Day.level",
        "format = 1\nstands_on = \"Levels/World.level\"\n[[scene]]\nscene = \"Scenes/Day.scene\"\n",
    );
    write(
        "Levels/Night.level",
        "format = 1\nstands_on = \"Levels/World.level\"\n[[scene]]\nscene = \"Scenes/Night.scene\"\n",
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
fn scenes_open_alongside_a_level_and_stay_while_it_opens_again() {
    let dir = project();
    let dusk = "format = 1\n[post]\npreset = \"cinematic\"\n[post.bloom]\nintensity = 0.5\n";
    std::fs::write(dir.path().join("Scenes/Dusk.scene"), dusk).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    let mut levels = Levels::default();
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    let night = content.db().guid_of("Scenes/Night.scene").unwrap();
    let day = content.db().guid_of("Scenes/Day.scene").unwrap();

    assert!(levels.open_scene_alongside(&mut engine, &mut content, None, night).unwrap());
    assert!(!levels.open_scene_alongside(&mut engine, &mut content, None, night).unwrap(), "open already");
    let open: Vec<&str> = levels.loaded_scenes().collect();
    assert_eq!(open, ["Scenes/Environment.scene", "Scenes/Day.scene", "Scenes/Night.scene"]);
    assert_eq!(levels.active_scene(), Some("Scenes/Day.scene"), "the active scene stays");
    let dusk = content.db().guid_of("Scenes/Dusk.scene").unwrap();
    levels.open_scene_alongside(&mut engine, &mut content, None, dusk).unwrap();
    let post = engine.world.resource::<PostProcess>().unwrap();
    assert_eq!(post.bloom.intensity, 0.1, "and so do its sky and post-processing, not Dusk's");
    assert!(levels.close_scene(&mut engine, dusk).unwrap());
    assert!(levels.opened_alongside(night) && !levels.opened_alongside(day));
    let level = content.db().guid_of("Levels/Day.level").unwrap();
    assert!(levels.open_scene_alongside(&mut engine, &mut content, None, level).is_err(), "only a scene");

    // Opening the level again keeps it; another level closes it.
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    assert!(levels.loaded_scenes().any(|s| s == "Scenes/Night.scene"));
    assert!(levels.opened_alongside(night));
    levels.load(&mut engine, &mut content, None, "Levels/Night.level").unwrap();
    assert!(!levels.opened_alongside(night), "one of Night's own now");
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    assert!(!levels.loaded_scenes().any(|s| s == "Scenes/Night.scene"));

    // Closing a scene unloads its Objs, persistent ones too in edit mode, and moves the active scene on.
    assert!(levels.close_scene(&mut engine, day).unwrap());
    engine.tick(1.0 / 60.0);
    assert_eq!(names(&engine), ["Floor", "Main Camera"]);
    assert_eq!(levels.active_scene(), Some("Scenes/Environment.scene"));
    assert!(!levels.close_scene(&mut engine, day).unwrap(), "closed already");
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

#[test]
fn edit_mode_loads_behaviours_as_data_without_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = "format = 1\n[[obj]]\nid = 1\nname = \"Spawner\"\nbehaviours = [\"Game.Spawner\"]\nfields.\"Game.Spawner\" = { count = 3 }\n";
    std::fs::create_dir_all(dir.path().join("Scenes")).unwrap();
    std::fs::create_dir_all(dir.path().join("Levels")).unwrap();
    std::fs::write(dir.path().join("Scenes/Main.scene"), scene).unwrap();
    std::fs::write(dir.path().join("Levels/Main.level"), "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n")
        .unwrap();
    let mut engine = voxear_engine::Engine::new();
    engine.set_edit_mode(true);
    let mut content = voxear_engine::Content::open(dir.path()).unwrap();
    let mut levels = voxear_engine::Levels::default();
    levels.load(&mut engine, &mut content, None, "Levels/Main.level").unwrap();
    engine.tick(1.0 / 60.0);
    let (_, kept) = engine
        .world
        .query_ref::<(&voxear_scene::Name, &voxear_engine::EditBehaviours)>()
        .into_iter()
        .next()
        .expect("the spawner keeps its behaviour as data");
    assert_eq!(kept.0[0].type_name, "Game.Spawner");
    assert_eq!(kept.0[0].fields.get("count").and_then(|v| v.as_integer()), Some(3));
}

#[test]
fn play_copies_the_level_as_edited_and_leaves_it_alone() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut edit = Engine::new();
    edit.set_edit_mode(true);
    let mut levels = Levels::default();
    levels.load(&mut edit, &mut content, None, "Levels/Day.level").unwrap();
    let find = |engine: &Engine, name: &str| {
        engine.world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let sun = find(&edit, "Sun");
    edit.world.get_mut::<Transform>(sun).unwrap().position.y = 7.5;

    let mut play = Engine::new();
    let mut played = levels.play_copy(&edit, &mut play, &mut content, None).unwrap();
    play.tick(1.0 / 60.0);
    assert_eq!(names(&play), names(&edit), "every Obj comes across");
    let copy = levels.counterpart(sun, &played).expect("the Sun has a counterpart");
    assert_eq!(play.world.get::<Name>(copy).unwrap().0, "Sun");
    assert_eq!(play.world.get::<Transform>(copy).unwrap().position.y, 7.5, "with the edit made to it");
    assert_eq!(played.counterpart(copy, &levels), Some(sun), "and back again");

    // The copy keeps the scenes' paths, so the persistent Manager is still known on the way round.
    played.load(&mut play, &mut content, None, "Levels/Night.level").unwrap();
    played.load(&mut play, &mut content, None, "Levels/Day.level").unwrap();
    play.tick(1.0 / 60.0);
    assert_eq!(play.world.query_ref::<&ScenePersistence>().count(), 1, "one Manager, not two");

    if let Some(transform) = play.world.get_mut::<Transform>(copy) {
        transform.position.y = -1.0;
    }
    play.shutdown();
    assert_eq!(play.world.entity_count(), 0, "stopping clears the play world");
    assert_eq!(edit.world.get::<Transform>(sun).unwrap().position.y, 7.5, "the edited level is as it was");
}

#[test]
fn levels_mark_an_active_scene_pass_over_optional_ones_and_keep_what_they_are_told() {
    let dir = project();
    let write = |path: &str, text: &str| std::fs::write(dir.path().join(path), text).unwrap();
    write("Scenes/Broken.scene", "this is not a scene");
    write(
        "Levels/Both.level",
        "format = 1\nstands_on = \"Levels/World.level\"\n\
         [[scene]]\nscene = \"Scenes/Day.scene\"\n\
         [[scene]]\nscene = \"Scenes/Night.scene\"\nactive = true\n\
         [[scene]]\nscene = \"Scenes/Broken.scene\"\nrequired = false\n\
         [[scene]]\nscene = \"Scenes/Gone.scene\"\nrequired = false\n",
    );
    write("Levels/Loop.level", "format = 1\nstands_on = \"Levels/Loop.level\"\n");
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let mut levels = Levels::default();

    let change = levels.load(&mut engine, &mut content, None, "Levels/Both.level").unwrap();
    assert_eq!(change.loaded, ["Scenes/Environment.scene", "Scenes/Day.scene", "Scenes/Night.scene"], "the base first");
    assert_eq!(change.skipped, ["Scenes/Broken.scene"], "a scene that is not required is passed over");
    assert_eq!(levels.active_scene(), Some("Scenes/Night.scene"), "the scene the level marks");
    assert!(engine.world.resource::<voxear_render::Sky>().is_some(), "the last sky, as the active scene has none");
    engine.spawn_obj("Firefly", Transform::IDENTITY, &[]).unwrap();

    let change = levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    assert_eq!(change.kept, ["Scenes/Environment.scene", "Scenes/Day.scene"]);
    assert_eq!(change.unloaded, ["Scenes/Night.scene"]);
    engine.tick(1.0 / 60.0);
    assert!(!names(&engine).contains(&"Firefly".to_string()), "runtime Objs went with the active scene");
    assert_eq!(levels.active_scene(), Some("Scenes/Day.scene"), "a level's own first scene, not its base's");

    let keep = voxear_engine::LevelOptions { unload_rest: false, set_active: false, ..Default::default() };
    let change = levels.load_with(&mut engine, &mut content, None, "Levels/Night.level", &keep).unwrap();
    assert!(change.unloaded.is_empty() && change.kept.contains(&"Scenes/Day.scene".to_string()), "{change:?}");
    assert_eq!(levels.active_scene(), Some("Scenes/Day.scene"), "the scene active before stays so");

    let error = levels.load(&mut engine, &mut content, None, "Levels/Loop.level").unwrap_err();
    assert!(format!("{error:#}").contains("stands on itself"), "{error:#}");
}

#[test]
fn saving_writes_only_the_scenes_that_changed() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    let mut levels = Levels::default();
    levels.load(&mut engine, &mut content, None, "Levels/Day.level").unwrap();
    let environment = std::fs::read_to_string(dir.path().join("Scenes/Environment.scene")).unwrap();
    assert!(levels.save(&engine, &mut content, dir.path()).unwrap().is_empty(), "nothing changed, nothing written");

    let tree =
        engine.world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Tree").unwrap().0;
    engine.world.get_mut::<Transform>(tree).unwrap().position.x = 9.5;
    engine.spawn_obj("Bench", Transform::IDENTITY, &[]).unwrap();
    let written = levels.save(&engine, &mut content, dir.path()).unwrap();
    assert_eq!(written, ["Scenes/Day.scene"], "the new Obj goes into the active scene, with the edit");
    assert_eq!(std::fs::read_to_string(dir.path().join("Scenes/Environment.scene")).unwrap(), environment);
    assert!(levels.save(&engine, &mut content, dir.path()).unwrap().is_empty(), "and saving again writes nothing");

    let mut fresh = Engine::new();
    let mut reloaded = Levels::default();
    reloaded.load(&mut fresh, &mut content, None, "Levels/Day.level").unwrap();
    let saved = scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    assert!(saved.sky.is_some(), "Day keeps its own sky");
    let tree = saved.objs.iter().find(|o| o.name == "Tree").unwrap();
    assert_eq!(tree.position.map(|p| p[0]), Some(9.5));
    assert!(saved.objs.iter().any(|o| o.name == "Bench"));
    assert!(names(&fresh).contains(&"Bench".to_string()));
}

/// A copy of the Hello sample's assets, without its build output.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

/// Saving needs the renderer the editor has: without one no meshes load,
/// so there would be no renderers to write back.
#[test]
fn hellos_levels_save_back_unchanged() {
    let mut renderer = match voxear_render::Renderer::new_offscreen(64, 64, &voxear_render::RendererSettings::default())
    {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let mut content = Content::open(dir.path()).unwrap();
    for level in ["Levels/Day.level", "Levels/Night.level"] {
        let mut engine = Engine::new();
        engine.set_edit_mode(true);
        let mut levels = Levels::default();
        levels.load(&mut engine, &mut content, Some(&mut renderer), level).unwrap();
        let written = levels.save(&engine, &mut content, dir.path()).unwrap();
        assert!(written.is_empty(), "{level}: an unedited level writes nothing, but wrote {written:?}");
    }
}
