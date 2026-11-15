//! The Infinite World template, as `vox new` makes it: a project that
//! opens, builds its scripts, makes its endless world round the player and
//! plays, the player standing on the ground the world makes, and a close-up
//! from its camera that shows ground, sky and plants. Writes the picture to
//! `VOXEAR_TEST_OUTPUT` where that is set. Skips itself without .NET and
//! the managed build, or a GPU.

use voxear_engine::{
    Engine, Game, Project,
    new_project::{NewProject, create},
    scripts,
};
use voxear_render::{Renderer, RendererSettings};
use voxear_terrain::blocks::BlockWorld;

/// The Obj called `name`.
fn named(game: &Game, name: &str) -> voxear_ecs::Entity {
    let objs = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(entity, _)| entity).unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn a_new_infinite_world_opens_makes_its_world_round_the_player_and_plays() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Ok(mut renderer) = Renderer::new_offscreen(960, 540, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let parent = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = parent.path().join("Far Lands");
    let request = NewProject {
        folder: folder.clone(),
        name: "Far Lands".into(),
        template: "infinite-world".into(),
        setup: None,
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    create(&request, &mut |_| {}).unwrap();
    let project = Project::open(&folder).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("the template has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(Some(&mut renderer)).unwrap();

    // Play, drawn, until the world has made the ground under the player, then three seconds more for them to land
    // on it; on a busy machine its chunks take a while.
    let (world, player, camera) = (named(&game, "World"), named(&game, "Player"), named(&game, "Main Camera"));
    let column = |game: &Game| {
        let at = game.engine.world.get::<voxear_scene::Transform>(player).unwrap().position;
        let corner = game.engine.world.get::<voxear_scene::Transform>(world).unwrap().position;
        let blocks = game.engine.world.get::<BlockWorld>(world).unwrap();
        let local = (at - corner) / blocks.block_size;
        let (x, z) = (local.x.floor() as i32, local.z.floor() as i32);
        let top = (-80..600).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0);
        (local.y, top, blocks.floor)
    };
    let mut frames = 0;
    while column(&game).1.is_none() && frames < 3600 {
        game.tick(1.0 / 30.0, Some(&mut renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
        frames += 1;
    }
    for _ in 0..90 {
        game.tick(1.0 / 30.0, Some(&mut renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
    }
    let errors: Vec<String> = log
        .lock()
        .unwrap()
        .lines
        .iter()
        .filter(|line| line.level == tracing::Level::ERROR)
        // The graphics driver's own checks are the renderer's to answer, not the template's.
        .filter(|line| !line.message.contains("The Vulkan spec states"))
        .map(|line| line.message.clone())
        .collect();
    assert!(errors.is_empty(), "errors while playing: {errors:?}");

    // The player stands on the world's ground, not in it nor fallen through it.
    let (feet, top, floor) = column(&game);
    let top = top.unwrap_or_else(|| panic!("no ground under the player after {frames} frames"));
    let floor = floor.expect("a floor under the world");
    assert!(top > floor, "the player is over the floor alone");
    assert!((feet - (top + 1) as f32).abs() < 3.0, "the player's feet at {feet}, the ground's top at {}", top + 1);
    assert!(game.engine.world.get::<voxear_scene::Transform>(camera).is_some());

    // A close-up from the player's camera: ground, sky and plants, not one colour.
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let colours: std::collections::BTreeSet<[u8; 3]> =
        pixels.chunks(4).map(|p| [p[0] / 16, p[1] / 16, p[2] / 16]).collect();
    assert!(colours.len() > 40, "{} colours in the picture", colours.len());
    if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&out).join("infinite world.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}
