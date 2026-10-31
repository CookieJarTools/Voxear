//! Background loading on a throwaway project: headless, and on an offscreen
//! GPU for models.

use voxear_engine::{Engine, Game, LoadState, Project};
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
