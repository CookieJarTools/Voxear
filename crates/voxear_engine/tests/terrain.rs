//! A Terrain in a scene: its heights read from its asset and the `.heights`
//! file beside it, with or without a renderer, and written back as written.

use voxear_engine::{
    Engine, Game, Project,
    scene_file::{self, TerrainSource},
};
use voxear_render::{Renderer, RendererSettings, TerrainLook};
use voxear_terrain::{Heightmap, Terrain, file::TerrainFile};

const FIELD: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Field\"\nposition = [-64.0, 2.0, -64.0]\n\
                     [obj.terrain]\ndata = \"Terrains/Field.terrain\"\ntexture_size = 6.0\ndetail_distance = 50.0\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let file = TerrainFile { size: 128.0, resolution: 257, ..TerrainFile::default() };
    let files = [
        ("voxear.toml", "[project]\nname = \"Field\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Terrains/Field.terrain", file.to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    // A slope rising 0.1 m a metre along x.
    let heights = Heightmap::from_fn(file.tiles(), file.tiles(), file.spacing(), |x, _| x * 0.1);
    std::fs::write(dir.path().join("Terrains/Field.heights"), voxear_terrain::file::write_heights(&heights)).unwrap();
    dir
}

#[test]
fn a_terrain_reads_its_heights_from_its_asset_and_saves_as_written() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let mut renderer = Renderer::new_offscreen(64, 64, &RendererSettings::default()).ok();
    game.start(renderer.as_mut()).unwrap();
    let world = &game.engine.world;
    let field = world.query_ref::<(voxear_ecs::Entity, &TerrainSource)>().into_iter().next().expect("the field").0;
    let terrain = world.get::<Terrain>(field).expect("its ground");
    assert_eq!(terrain.heights.size(), [128.0, 128.0]);
    assert_eq!(terrain.detail_distance, 50.0);
    // At (16, 0) in the world, 80 m along the slope from its corner 2 m up.
    let ground = voxear_terrain::ground_at(world, 16.0, 0.0).expect("ground there");
    assert!((ground.height - 10.0).abs() < 1e-3, "{}", ground.height);
    if renderer.is_some() {
        assert_eq!(world.get::<TerrainLook>(field).map(|look| look.texture_size), Some(6.0));
    }
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[field]);
    let written = saved.objs[0].terrain.as_ref().expect("the terrain is saved");
    assert_eq!(written.data, "Terrains/Field.terrain");
    assert_eq!((written.texture_size, written.detail_distance, written.cast_shadows), (6.0, 50.0, true));
}

#[test]
fn a_server_without_a_renderer_has_the_ground_to_stand_on() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let field = world.query_ref::<(voxear_ecs::Entity, &TerrainSource)>().into_iter().next().expect("kept").0;
    assert!(world.get::<TerrainLook>(field).is_none(), "nothing to draw it with");
    assert!(world.get::<Terrain>(field).is_some(), "but its heights, for physics");
    let ground = voxear_terrain::ground_at(world, -54.0, 0.0).expect("ground there");
    assert!((ground.height - 3.0).abs() < 1e-3, "{}", ground.height);
}

#[test]
fn a_terrain_whose_heights_are_missing_is_flat_and_as_large_as_its_file_says() {
    let dir = project();
    std::fs::remove_file(dir.path().join("Terrains/Field.heights")).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let terrain = world.query_ref::<&Terrain>().into_iter().next().expect("the field");
    assert_eq!(terrain.heights.size(), [128.0, 128.0]);
    assert_eq!(terrain.heights.range(), [0.0, 0.0]);
}
