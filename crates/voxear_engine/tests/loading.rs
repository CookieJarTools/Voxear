//! Background loading on a throwaway project: headless, and on an offscreen
//! GPU for models.

use voxear_engine::{
    Engine, Game, LevelOptions, LoadState, Project,
    audio::{Audio, AudioSource, GameAudio},
};
use voxear_render::{MeshRenderer, Renderer, RendererSettings};
use voxear_scene::Name;

const TREE: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Tree.bin", "byteLength": 42 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
    { "buffer": 0, "byteOffset": 36, "byteLength": 6 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] },
    { "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR" }
  ],
  "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] }],
  "nodes": [{ "mesh": 0 }],
  "scenes": [{ "nodes": [0] }]
}"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Loading\"\nstart_level = \"Levels/Start.level\"\n"),
        ("Levels/Start.level", "format = 1\n[[scene]]\nscene = \"Scenes/Start.scene\"\n"),
        ("Levels/Forest.level", "format = 1\n[[scene]]\nscene = \"Scenes/Forest.scene\"\n"),
        ("Levels/Broken.level", "format = 1\n[[scene]]\nscene = \"Scenes/Broken.scene\"\n"),
        ("Scenes/Start.scene", "format = 1\n[[obj]]\nid = 1\nname = \"Start\"\n"),
        (
            "Scenes/Forest.scene",
            "format = 1\n[[obj]]\nid = 1\nname = \"Tree\"\n[obj.renderer]\nmesh = \"Models/Tree.gltf\"\n[[obj]]\nid = 2\nprefab = \"Prefabs/Rock.prefab\"\n",
        ),
        ("Scenes/Broken.scene", "format = 1\n[[obj]]\nid = 1\nname = \"Oops\"\nmystery = true\n"),
        ("Prefabs/Rock.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"Rock\"\n"),
        ("Models/Tree.gltf", TREE),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut buffer = Vec::new();
    for value in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    buffer.extend_from_slice(&[0, 0, 1, 0, 2, 0]);
    std::fs::write(dir.path().join("Models/Tree.bin"), buffer).unwrap();
    dir
}

fn game(dir: &tempfile::TempDir) -> Game {
    Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap()
}

fn names(game: &Game) -> Vec<String> {
    let mut names: Vec<String> = game.engine.world.query_ref::<&Name>().into_iter().map(|n| n.0.clone()).collect();
    names.sort();
    names
}

/// Ticks until the load leaves `state`, failing after a few seconds.
fn tick_while(game: &mut Game, mut renderer: Option<&mut Renderer>, id: voxear_engine::LoadId, state: LoadState) {
    let started = std::time::Instant::now();
    while game.load_status(id).unwrap().state == state {
        assert!(started.elapsed().as_secs() < 10, "the load is stuck at {state:?}");
        game.tick(1.0 / 60.0, renderer.as_deref_mut());
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn a_level_can_wait_at_ready_until_activation_is_allowed() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let id = game.load_level_async("Levels/Forest.level").unwrap();
    game.loads.set_allow_activation(id, false);

    tick_while(&mut game, None, id, LoadState::Loading);
    let status = game.load_status(id).unwrap();
    assert_eq!((status.state, status.progress), (LoadState::Ready, 0.9));
    for _ in 0..5 {
        game.tick(1.0 / 60.0, None);
    }
    assert_eq!(names(&game), ["Start"], "nothing changes while activation is held back");

    game.loads.set_allow_activation(id, true);
    tick_while(&mut game, None, id, LoadState::Ready);
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.load_status(id).unwrap().state, LoadState::Done);
    assert_eq!(names(&game), ["Rock", "Tree"]);
}

#[test]
fn prefabs_load_in_the_background_and_failures_are_reported() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();

    let rock = game.load_prefab_async("Prefabs/Rock.prefab").unwrap();
    tick_while(&mut game, None, rock, LoadState::Loading);
    let status = game.load_status(rock).unwrap();
    assert_eq!(status.state, LoadState::Done);
    assert!(status.prefab.is_some());

    assert!(game.load_level_async("Levels/Missing.level").is_err(), "a missing level fails at once");
    let broken = game.load_level_async("Levels/Broken.level").unwrap();
    tick_while(&mut game, None, broken, LoadState::Loading);
    tick_while(&mut game, None, broken, LoadState::Ready);
    match game.load_status(broken).unwrap().state {
        LoadState::Failed(error) => assert!(error.contains("mystery"), "{error}"),
        other => panic!("expected a failure, got {other:?}"),
    }
    assert_eq!(names(&game), ["Start"], "a failed change leaves the level as it was");
}

#[test]
fn models_are_prepared_on_the_worker_and_uploaded_on_the_main_thread() {
    let dir = project();
    let mut renderer = Renderer::new_offscreen(64, 64, &RendererSettings::default()).unwrap();
    let mut game = game(&dir);
    game.start(Some(&mut renderer)).unwrap();

    let id = game.load_level_async("Levels/Forest.level").unwrap();
    tick_while(&mut game, Some(&mut renderer), id, LoadState::Loading);
    tick_while(&mut game, Some(&mut renderer), id, LoadState::Ready);
    assert_eq!(game.load_status(id).unwrap().state, LoadState::Done);
    let drawn = game.engine.world.query_ref::<(&Name, &MeshRenderer)>().into_iter().any(|(n, _)| n.0 == "Tree");
    assert!(drawn, "the tree's mesh came from the prepared model");
    renderer.render(&game.engine.world, 0.0).unwrap();
}

#[test]
fn a_level_change_frees_what_only_the_old_level_used() {
    let dir = project();
    let mut renderer = Renderer::new_offscreen(64, 64, &RendererSettings::default()).unwrap();
    let mut game = game(&dir);
    game.start(Some(&mut renderer)).unwrap();
    let before = renderer.asset_usage();

    game.load_level("Levels/Forest.level", Some(&mut renderer)).unwrap();
    let forest = renderer.asset_usage();
    assert_eq!(game.content.usage().models, 1);
    assert_eq!(forest.meshes, before.meshes + 1, "the tree's mesh");

    game.load_level("Levels/Start.level", Some(&mut renderer)).unwrap();
    assert_eq!(game.content.usage().models, 0, "nothing in Start uses the tree");
    // Freed space comes back once the frames that might still draw it are done.
    for _ in 0..4 {
        game.tick(1.0 / 60.0, Some(&mut renderer));
        renderer.render(&game.engine.world, 0.0).unwrap();
    }
    let after = renderer.asset_usage();
    assert_eq!((after.meshes, after.materials, after.retiring), (before.meshes, before.materials, 0));

    // And it loads again when needed.
    game.load_level("Levels/Forest.level", Some(&mut renderer)).unwrap();
    assert_eq!(renderer.asset_usage().meshes, forest.meshes);
}

#[test]
fn prefabs_go_unless_kept_and_come_back_when_used() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let rock = game.content.prefab(None, "Prefabs/Rock.prefab").unwrap();

    game.release_unused(None);
    assert!(game.content.template(rock).is_some(), "used since the last sweep, so it stays once");
    let released = game.release_unused(None);
    assert_eq!(released.prefabs, 1);
    assert!(game.content.template(rock).is_none());

    let again = game.content.loaded_template(None, rock).unwrap();
    assert_eq!(again.objs[0].name, "Rock", "the same id loads it again");

    game.content.keep_prefab(rock, true);
    for _ in 0..3 {
        game.release_unused(None);
    }
    assert!(game.content.template(rock).is_some(), "kept prefabs stay");
    game.content.keep_prefab(rock, false);
    game.release_unused(None);
    game.release_unused(None);
    assert!(game.content.template(rock).is_none());
}

#[test]
fn cooked_models_load_from_bundles_now_and_in_the_background() {
    let dir = project();
    let output = voxear_engine::content_build::default_output(dir.path());
    voxear_engine::content_build::build(dir.path(), &output, &Default::default(), &[]).unwrap();
    let mut renderer = Renderer::new_offscreen(64, 64, &RendererSettings::default()).unwrap();
    let content = voxear_engine::Content::open_bundles(dir.path(), &output).unwrap();
    let mut game = Game::with_content(Project::open(dir.path()).unwrap(), Engine::new(), content);
    game.start(Some(&mut renderer)).unwrap();

    let tree_drawn =
        |game: &Game| game.engine.world.query_ref::<(&Name, &MeshRenderer)>().into_iter().any(|(n, _)| n.0 == "Tree");
    game.load_level("Levels/Forest.level", Some(&mut renderer)).unwrap();
    assert!(tree_drawn(&game), "loaded at once from the cooked model");

    game.load_level("Levels/Start.level", Some(&mut renderer)).unwrap();
    let id = game.load_level_async("Levels/Forest.level").unwrap();
    tick_while(&mut game, Some(&mut renderer), id, LoadState::Loading);
    tick_while(&mut game, Some(&mut renderer), id, LoadState::Ready);
    assert_eq!(game.load_status(id).unwrap().state, LoadState::Done);
    assert!(tree_drawn(&game), "and in the background");
    renderer.render(&game.engine.world, 0.0).unwrap();
}

#[test]
fn a_held_change_waits_its_least_time_and_says_what_came_in() {
    let dir = project();
    std::fs::write(
        dir.path().join("Levels/Both.level"),
        "format = 1\n[[scene]]\nscene = \"Scenes/Start.scene\"\n[[scene]]\nscene = \"Scenes/Forest.scene\"\nweight = 4\nactive = true\n",
    )
    .unwrap();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let options = voxear_engine::LevelOptions { hold: true, least_time: 0.4, ..Default::default() };
    let started = std::time::Instant::now();
    let id = game.load_level_async_with("Levels/Both.level", options).unwrap();
    tick_while(&mut game, None, id, LoadState::Loading);
    for _ in 0..5 {
        game.tick(1.0 / 60.0, None);
    }
    assert_eq!(game.load_status(id).unwrap().state, LoadState::Ready, "held until let in");
    assert_eq!(names(&game), ["Start"]);

    game.loads.set_allow_activation(id, true);
    tick_while(&mut game, None, id, LoadState::Ready);
    assert!(started.elapsed().as_secs_f32() >= 0.4, "and not before its least time");
    let status = game.load_status(id).unwrap();
    assert_eq!(status.state, LoadState::Done);
    assert_eq!(status.scenes, ["Scenes/Forest.scene"], "Start was kept, so only Forest came in");
    assert_eq!(game.levels.active_scene(), Some("Scenes/Forest.scene"));
}

#[test]
fn a_scene_loads_alongside_the_level_and_goes_again_on_its_own() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let options = LevelOptions { set_active: true, ..LevelOptions::default() };
    let id = game.load_scene_async("Scenes/Forest.scene", options).unwrap();
    tick_while(&mut game, None, id, LoadState::Loading);
    tick_while(&mut game, None, id, LoadState::Ready);
    let status = game.load_status(id).unwrap();
    assert_eq!((status.state, status.scenes), (LoadState::Done, vec!["Scenes/Forest.scene".to_string()]));
    assert_eq!(names(&game), ["Rock", "Start", "Tree"], "the level's scene stays: nothing else unloads");
    assert_eq!(game.levels.active_scene(), Some("Scenes/Forest.scene"), "asked to be the active scene");
    assert_eq!(game.levels.current(), Some("Levels/Start.level"), "the level is the same level");

    assert!(game.set_active_scene("Scenes/Start.scene").unwrap());
    assert_eq!(game.levels.active_scene(), Some("Scenes/Start.scene"));
    assert!(game.unload_scene("Scenes/Forest.scene").unwrap());
    game.tick(1.0 / 60.0, None);
    assert_eq!(names(&game), ["Start"], "its Objs go at the end of the frame");
    assert!(!game.unload_scene("Scenes/Forest.scene").unwrap(), "it is not loaded any more");
}

#[test]
fn a_level_change_can_load_its_shared_scenes_again() {
    let dir = project();
    let mut game = game(&dir);
    game.start(None).unwrap();
    let start =
        |game: &Game| game.engine.world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().next().unwrap().0;
    let before = start(&game);

    let kept = game.load_level_with("Levels/Start.level", &LevelOptions::default(), None).unwrap();
    assert_eq!(kept.kept, ["Scenes/Start.scene"], "a scene both levels have stays as it is");
    assert_eq!(start(&game), before);

    let again = LevelOptions { reload_shared: true, ..LevelOptions::default() };
    let reloaded = game.load_level_with("Levels/Start.level", &again, None).unwrap();
    assert_eq!(
        (reloaded.unloaded, reloaded.loaded),
        (vec!["Scenes/Start.scene".to_string()], vec!["Scenes/Start.scene".to_string()])
    );
    game.tick(1.0 / 60.0, None);
    assert_ne!(start(&game), before, "loaded again, its Objs are new");
    assert_eq!(names(&game), ["Start"], "and the old ones went at the end of the frame");
}

/// A 16-bit mono WAV at 48 kHz holding one level for a tenth of a second.
fn wav(level: i16) -> Vec<u8> {
    let data = 4800u32 * 2;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for value in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
        file.extend_from_slice(&value);
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&data.to_le_bytes());
    (0..4800).for_each(|_| file.extend_from_slice(&level.to_le_bytes()));
    file
}

#[test]
fn a_levels_sounds_are_read_and_decoded_on_the_loading_thread() {
    let dir = project();
    let files = [
        ("Levels/River.level", "format = 1\n[[scene]]\nscene = \"Scenes/River.scene\"\n"),
        (
            "Scenes/River.scene",
            "format = 1\n[[obj]]\nid = 1\nname = \"River\"\n[obj.audio]\nsound = \"Sounds/River.wav\"\nloop = true\n[[obj]]\nid = 2\nprefab = \"Prefabs/Bird.prefab\"\n",
        ),
        (
            "Prefabs/Bird.prefab",
            "format = 1\n[[obj]]\nid = 1\nname = \"Bird\"\n[obj.audio]\nsound = \"Sounds/Song.wav\"\n",
        ),
    ];
    for (path, text) in files {
        std::fs::create_dir_all(dir.path().join(path).parent().unwrap()).unwrap();
        std::fs::write(dir.path().join(path), text).unwrap();
    }
    std::fs::create_dir_all(dir.path().join("Sounds")).unwrap();
    std::fs::write(dir.path().join("Sounds/River.wav"), wav(8000)).unwrap();
    std::fs::write(dir.path().join("Sounds/Song.wav"), wav(4000)).unwrap();
    let mut game = game(&dir);
    game.audio = GameAudio::new(Audio::quiet());
    // The river is imported quieter, which the loading thread reads from its .meta.
    let meta = dir.path().join("Sounds/River.wav.meta");
    let text = std::fs::read_to_string(&meta).unwrap();
    std::fs::write(&meta, format!("{text}\n[sound]\nvolume = 0.5\n")).unwrap();
    game.start(None).unwrap();

    let id = game.load_level_async("Levels/River.level").unwrap();
    game.loads.set_allow_activation(id, false);
    tick_while(&mut game, None, id, LoadState::Loading);
    assert_eq!(game.content.loaded_sounds(), 0, "they come in with the level");

    // With the files gone, the scene comes in only from what the loading thread read.
    std::fs::remove_file(dir.path().join("Sounds/River.wav")).unwrap();
    std::fs::remove_file(dir.path().join("Sounds/Song.wav")).unwrap();
    game.loads.set_allow_activation(id, true);
    tick_while(&mut game, None, id, LoadState::Ready);
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.load_status(id).unwrap().state, LoadState::Done);
    assert_eq!(names(&game), ["Bird", "River"]);
    assert_eq!(game.content.loaded_sounds(), 2, "the scene's sound and the prefab's");
    let world = &game.engine.world;
    let river = world.query_ref::<(&Name, &AudioSource)>().into_iter().find(|(name, _)| name.0 == "River");
    let sound = river.and_then(|(_, source)| source.sound).expect("the river plays its sound");
    assert_eq!(game.content.loaded_sound(sound).unwrap().volume, 0.5);
}
