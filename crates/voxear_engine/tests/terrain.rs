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
    std::fs::write(dir.path().join("Terrains/Field.heights"), voxear_terrain::file::write_heights(&heights, 0))
        .unwrap();
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

/// A terrain with a 10 m hill stamped in its middle, as its file says.
const STAMPED: &str = "size = 128.0\nresolution = 257\n\n[[layers]]\nname = \"Hill\"\nkind = \"stamp\"\n\
                       position = [64.0, 64.0]\nsize = [40.0, 40.0]\nheight = 10.0\nfalloff = 0.0\n";

#[test]
fn a_terrain_without_heights_is_made_from_its_layers_and_heights_made_from_others_are_made_again() {
    let dir = project();
    std::fs::write(dir.path().join("Terrains/Field.terrain"), STAMPED).unwrap();
    // The slope's heights beside it, as made from other layers.
    let slope = Heightmap::from_fn(1, 1, 0.5, |x, _| x * 0.1);
    std::fs::write(dir.path().join("Terrains/Field.heights"), voxear_terrain::file::write_heights(&slope, 1)).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let terrain = game.engine.world.query_ref::<&Terrain>().into_iter().next().expect("the field").clone();
    assert!((terrain.heights.height_at(64.0, 64.0) - 10.0).abs() < 1e-3, "the hill, not the slope");
    assert_eq!(terrain.heights.height_at(5.0, 5.0), 0.0);

    // Heights written for these layers are read as they are, without making them again.
    let file = TerrainFile::parse(STAMPED).unwrap();
    let marked = Heightmap::flat(1, 1, 0.5, 7.0);
    std::fs::write(
        dir.path().join("Terrains/Field.heights"),
        voxear_terrain::file::write_heights(&marked, file.shape_hash()),
    )
    .unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let terrain = game.engine.world.query_ref::<&Terrain>().into_iter().next().expect("the field").clone();
    assert_eq!(terrain.heights.height_at(64.0, 64.0), 7.0, "read as written");
}

#[test]
fn surfaces_and_holes_are_made_from_the_file_and_shares_kept_beside_the_heights_are_read_as_written() {
    use std::sync::Arc;
    use voxear_terrain::{
        layers::Mask,
        raster::{Rasters, write_rasters},
        surfaces::{Cover, Surface, WeightTile, Weights, hash_surfaces},
    };
    let dir = project();
    // The stamped hill, grass everywhere, snow from 5 m up, and a hole raster cut round (20, 20).
    let mut file = TerrainFile::parse(STAMPED).unwrap();
    let mut snow = Surface::new(None);
    snow.cover = Cover::Rules;
    snow.mask = Mask { height: Some([5.0, 100.0]), slope: None, feather: 0.5 };
    file.surfaces = vec![Surface { cover: Cover::Everywhere, ..Surface::new(None) }, snow];
    file.holes = 3;
    // Grass on the snow alone.
    let mut grass = voxear_terrain::grass::Grass::new(Some("builtin:cube".into()));
    grass.cover = voxear_terrain::grass::GrassCover::Surfaces;
    grass.surfaces = vec![1];
    file.grass = vec![grass];
    let mut rasters = Rasters::default();
    let hole = rasters.tile_mut(3, 0, 0);
    for z in 36..=44 {
        for x in 36..=44 {
            hole[z * voxear_terrain::TILE_SAMPLES as usize + x] = 1.0;
        }
    }
    std::fs::write(dir.path().join("Terrains/Field.terrain"), file.to_text()).unwrap();
    std::fs::write(dir.path().join("Terrains/Field.layers"), write_rasters(&rasters)).unwrap();
    std::fs::remove_file(dir.path().join("Terrains/Field.heights")).unwrap();
    let start = |dir: &tempfile::TempDir| {
        let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
        game.start(None).unwrap();
        game
    };
    // The field's corner is at (-64, 2, -64): its (20, 20) is (-44, -44) in the world.
    let game = start(&dir);
    let world = &game.engine.world;
    assert!(voxear_terrain::ground_at(world, -44.0, -44.0).is_none(), "a hole there");
    assert!(voxear_terrain::ground_at(world, -40.0, -44.0).is_some(), "ground beside it");
    assert_eq!(voxear_terrain::ground_at(world, 0.0, 0.0).unwrap().surface, 1, "snow on the hill");
    assert_eq!(voxear_terrain::ground_at(world, -54.0, 40.0).unwrap().surface, 0, "grass below");
    let terrain = world.query_ref::<&Terrain>().into_iter().next().unwrap().clone();
    let grown = terrain.grass.as_ref().expect("its grass grown on load");
    // The field's corner is at (-64, 2, -64).
    let at = |x: f32, z: f32| (((x + 64.0) / file.spacing()).round() as i64, ((z + 64.0) / file.spacing()).round() as i64);
    let (hill, field) = (at(0.0, 0.0), at(-54.0, 40.0));
    assert_eq!(grown.density_at(0, hill.0, hill.1), 255, "on the snow");
    assert_eq!(grown.density_at(0, field.0, field.1), 0, "not on the grass below");

    // Shares written beside heights for these surfaces are read as written: here all snow.
    let marked = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(1))]);
    let bytes = voxear_terrain::file::write_ground(
        &terrain.heights,
        file.shape_hash(),
        Some((&marked, hash_surfaces(&file.surfaces))),
        None,
    );
    std::fs::write(dir.path().join("Terrains/Field.heights"), &bytes).unwrap();
    let game = start(&dir);
    let world = &game.engine.world;
    assert_eq!(voxear_terrain::ground_at(world, -54.0, 40.0).unwrap().surface, 1, "read as written");
    assert!(voxear_terrain::ground_at(world, -44.0, -44.0).is_none(), "the hole kept in the heights");

    // Surfaces that cover elsewhere since are worked out again: snow from 50 m up covers nothing here.
    file.surfaces[1].mask.height = Some([50.0, 100.0]);
    std::fs::write(dir.path().join("Terrains/Field.terrain"), file.to_text()).unwrap();
    let game = start(&dir);
    assert_eq!(voxear_terrain::ground_at(&game.engine.world, 0.0, 0.0).unwrap().surface, 0);
}

#[test]
fn a_trench_in_a_terrains_layers_is_dug_as_it_loads() {
    use voxear_terrain::{
        dig::Trench,
        layers::{Layer, LayerKind},
    };
    let dir = project();
    // A trench across the field along z at x = 64 from its corner, its floor 2 m down.
    let trench = LayerKind::Spline {
        points: vec![[64.0, 0.0, 20.0], [64.0, 0.0, 100.0]],
        width: 1.0,
        falloff: 1.0,
        dig: Some(Trench { step_width: 0.0, parapet: 0.0, ..Trench::default() }),
    };
    let file = TerrainFile { size: 128.0, resolution: 257, layers: vec![Layer::new(trench)], ..TerrainFile::default() };
    std::fs::write(dir.path().join("Terrains/Field.terrain"), file.to_text()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let field = world.query_ref::<(voxear_ecs::Entity, &TerrainSource)>().into_iter().next().expect("the field").0;
    let terrain = world.get::<Terrain>(field).expect("its ground");
    assert!(terrain.dug.is_some(), "dug as it loads");
    // Its corner at (-64, 2, -64) and the ground rising 0.1 m a metre: the trench at x = 0 in the world, where
    // the ground is 8.4 m up, its floor 2 m under that.
    let floor = voxear_terrain::ground_at(world, 0.0, 0.0).expect("its floor").height;
    assert!((floor - 6.4).abs() < 0.2, "a trench 2 m deep: its floor at {floor}");
    let beside = voxear_terrain::ground_at(world, 10.0, 0.0).expect("ground beside it").height;
    assert!((beside - 9.4).abs() < 0.01, "and the ground beside it as it was: {beside}");

}

/// A terrain without end made by a generator, 4 m between samples, and a camera far out over it.
const ENDLESS: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"World\"\n[obj.terrain]\n\
                       data = \"Terrains/World.terrain\"\n\n[[obj]]\nid = 2\nname = \"Eye\"\n\
                       position = [5000.0, 300.0, -3000.0]\n[obj.camera]\nfov = 60.0\n";

fn endless_project() -> tempfile::TempDir {
    use voxear_terrain::{
        file::Endless,
        generate::Preset,
        layers::{Layer, LayerKind},
    };
    let dir = project();
    let generate = LayerKind::Generate { world: Some("Worlds/Hills.worldgen".into()), position: [100.0, 0.0] };
    let file = TerrainFile {
        layers: vec![Layer::new(generate)],
        endless: Some(Endless { distance: 900.0, spacing: 4.0 }),
        ..TerrainFile::default()
    };
    std::fs::write(dir.path().join("Scenes/Field.scene"), ENDLESS).unwrap();
    std::fs::write(dir.path().join("Terrains/World.terrain"), file.to_text()).unwrap();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), Preset::RollingHills.world().to_text()).unwrap();
    dir
}

#[test]
fn a_terrain_without_end_is_made_round_the_camera_and_follows_it() {
    use voxear_terrain::generate::{Preset, WorldGen};
    let dir = endless_project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let hills = WorldGen::parse(&Preset::RollingHills.world().to_text()).unwrap();
    let (world_entity, eye) = {
        let world = &game.engine.world;
        let named = |name: &str| {
            world
                .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
                .into_iter()
                .find(|(_, n)| n.0 == name)
                .unwrap()
                .0
        };
        (named("World"), named("Eye"))
    };
    // Before a frame, holes round its Obj: nothing to stand on yet.
    assert!(voxear_terrain::ground_at(&game.engine.world, 5000.0, -3000.0).is_none());
    // A frame: the ground under the camera made at once, as its generator makes it.
    game.tick(1.0 / 60.0, None);
    let ground = voxear_terrain::ground_at(&game.engine.world, 5000.0, -3000.0).expect("ground under the camera");
    assert_eq!(ground.terrain, world_entity);
    assert!((ground.height - hills.height_at(5100.0, -3000.0)).abs() < 0.05, "{} as made", ground.height);
    let terrain = game.engine.world.get::<Terrain>(world_entity).unwrap();
    assert_eq!(terrain.heights.tiles_x(), 3, "900 m each way of 1024 m tiles: the camera's and one each side");
    assert_eq!(terrain.origin, Some([3, -4]));
    // Physics stands things on it.
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(voxear_engine::terrain::streamed(&game.engine.world));
    // The camera gone 3 km on: the ground under it there, from the frame after it is placed there.
    game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap().position.x = 8000.0;
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    let ground = voxear_terrain::ground_at(&game.engine.world, 8000.0, -3000.0).expect("ground there now");
    assert!((ground.height - hills.height_at(8100.0, -3000.0)).abs() < 0.05);
    assert_eq!(game.engine.world.get::<Terrain>(world_entity).unwrap().origin, Some([6, -4]));
    assert!(voxear_terrain::ground_at(&game.engine.world, 5000.0, -3000.0).is_none(), "left behind");

    // Saved, it keeps no heights: its file alone.
    voxear_engine::terrain::keep(&game.engine.world, &mut game.content);
    let kept =
        game.content.terrain(game.engine.world.get::<voxear_engine::terrain::TerrainAsset>(world_entity).unwrap().guid);
    assert!(kept.is_some_and(|kept| kept.file.endless.is_some()));
    let mut changed = game.engine.world.get::<voxear_engine::terrain::TerrainAsset>(world_entity).unwrap().clone();
    changed.file.base = 1.0;
    game.engine.world.insert(world_entity, changed);
    voxear_engine::terrain::keep(&game.engine.world, &mut game.content);
    let written = voxear_engine::terrain::save(&mut game.content, dir.path()).unwrap();
    assert_eq!(written, vec!["Terrains/World.terrain".to_string()]);
    assert!(!dir.path().join("Terrains/World.heights").exists());
    let text = std::fs::read_to_string(dir.path().join("Terrains/World.terrain")).unwrap();
    assert!(TerrainFile::parse(&text).unwrap().endless.is_some());
}
