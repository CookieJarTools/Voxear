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
