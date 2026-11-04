//! Lighting settings assets: a scene names one and is lit by it as it
//! loads, with its own settings over them, and writes the name back when it
//! is saved; the level can be lit with others while it runs.

use voxear_engine::{Engine, Game, Project, scene_file};
use voxear_render::{Look, LookName, PostProcess, Sky, SunLook};

const DUSK: &str = "format = 1\nlook = \"golden_hour\"\n\n[sky]\nhaze = 2.5\n\n[sun]\nstrength = 2.0\n\n\
                    [post.bloom]\nintensity = 0.3\n";

fn project(lighting: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let scene = format!(
        "format = 1\nlighting = \"{lighting}\"\n\n[post.bloom]\nscatter = 0.5\n\n[[obj]]\nid = 1\nname = \"Rock\"\n"
    );
    let files = [
        ("voxear.toml", "[project]\nname = \"Lit\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Valley.scene\"\n"),
        ("Scenes/Valley.scene", scene.as_str()),
        ("Lighting/Dusk.lighting", DUSK),
        ("Lighting/Night.lighting", "format = 1\n\n[sky]\nmode = \"atmosphere\"\nstars = 0.25\n"),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_scene_loads_with_its_lighting_its_own_settings_over_them_and_saves_the_name() {
    let dir = project("Lighting/Dusk.lighting");
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    assert_eq!(world.resource::<Sky>().map(|sky| sky.haze), Some(2.5), "the lighting's sky");
    assert_eq!(world.resource::<SunLook>().map(|sun| sun.strength), Some(2.0), "and its sun");
    let post = *world.resource::<PostProcess>().unwrap();
    assert_eq!((post.bloom.intensity, post.bloom.scatter), (0.3, 0.5), "its bloom, under the scene's own scatter");

    let entities: Vec<_> = world.query_ref::<voxear_ecs::Entity>().into_iter().collect();
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &entities);
    assert_eq!(saved.lighting.as_deref(), Some("Lighting/Dusk.lighting"), "the scene keeps naming it");
    assert!(saved.sky.is_none(), "and the lighting's sky stays in the lighting");
    assert_eq!(saved.post.unwrap()["bloom"]["scatter"].as_float(), Some(0.5), "the scene's own settings stay its own");
}

#[test]
fn built_in_lighting_is_a_look_and_the_running_level_takes_other_lighting() {
    let dir = project("builtin:storybook");
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let storybook = Look::of(LookName::Storybook);
    assert_eq!(game.engine.world.resource::<SunLook>().copied(), Some(storybook.sun));

    game.levels.apply_lighting(&mut game.engine, &mut game.content, "Lighting/Night.lighting").unwrap();
    let sky = *game.engine.world.resource::<Sky>().unwrap();
    assert_eq!(sky.stars, 0.25, "the night's sky");
    let sun = game.engine.world.resource::<SunLook>().copied();
    assert_eq!(sun, Some(voxear_engine::lighting::plain_sun()), "with no look, nothing changes the sun");
    assert!(game.levels.apply_lighting(&mut game.engine, &mut game.content, "Lighting/Missing.lighting").is_err());
    assert_eq!(game.engine.world.resource::<Sky>().unwrap().stars, 0.25, "and lighting that fails leaves it as it was");
}
