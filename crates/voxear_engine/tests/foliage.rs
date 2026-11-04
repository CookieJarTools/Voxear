//! A Foliage in a scene: its plants placed and scattered as it loads, and
//! written back as it was written, its scatter rather than every plant.

use voxear_engine::{
    Engine, Game, Project,
    scene_file::{self, FoliageSource},
};
use voxear_render::{Foliage, Renderer, RendererSettings};

const MEADOW: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Meadow\"\n[obj.foliage]\nmesh = \"builtin:cube\"\n\
                      draw_distance = 40.0\nplants = [[1.0, 0.0, 2.0, 90.0, 1.5]]\n\
                      [obj.foliage.scatter]\nsize = [10.0, 10.0]\ndensity = 2.0\nseed = 4\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Meadow\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Meadow.scene\"\n"),
        ("Scenes/Meadow.scene", MEADOW),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn foliage_loads_its_plants_and_saves_as_written() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let mut renderer = Renderer::new_offscreen(64, 64, &RendererSettings::default()).ok();
    game.start(renderer.as_mut()).unwrap();
    let world = &game.engine.world;
    let meadow = world.query_ref::<(voxear_ecs::Entity, &FoliageSource)>().into_iter().next().expect("the meadow").0;
    if renderer.is_some() {
        let foliage = world.get::<Foliage>(meadow).expect("drawn where there is a renderer");
        assert!(
            (190..=211).contains(&foliage.plants.len()),
            "the placed plant and about 200 scattered: {}",
            foliage.plants.len()
        );
        assert_eq!(foliage.plants[0].scale, 1.5, "the placed plant first");
        assert!((foliage.plants[0].yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "turned in degrees");
        assert_eq!(foliage.draw_distance, 40.0);
    }
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[meadow]);
    let written = saved.objs[0].foliage.as_ref().expect("the foliage is saved");
    assert_eq!(written.plants.len(), 1, "the placed plant, not every scattered one");
    assert_eq!(written.scatter.map(|s| (s.density, s.seed)), Some((2.0, 4)), "but the scatter they came from");
}

#[test]
fn foliage_is_kept_to_save_without_a_renderer() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let meadow = world.query_ref::<(voxear_ecs::Entity, &FoliageSource)>().into_iter().next().expect("kept").0;
    assert!(world.get::<Foliage>(meadow).is_none(), "nothing to draw it with");
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[meadow]);
    assert!(saved.objs[0].foliage.is_some(), "but a save keeps it");
}
