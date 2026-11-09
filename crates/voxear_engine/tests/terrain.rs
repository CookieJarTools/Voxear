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

    // Shares written beside heights for these surfaces are read as written: here all snow.
    let marked = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(1))]);
    let bytes = voxear_terrain::file::write_ground(
        &terrain.heights,
        file.shape_hash(),
        Some((&marked, hash_surfaces(&file.surfaces))),
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
