//! Storms over a sea without end (see `voxear_engine::storms`): a game with
//! a generated world of blocks that is all sea, made round the camera, and
//! storms that cross it. No GPU and no scripts.

use voxear_core::math::{DVec2, Vec2, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    storms::{self, Falling, SeaNow, StormEvent, StormKind, StormRules, Storms},
    water,
    weather::Weather,
};
use voxear_render::{
    sea::{RoughSea, SeaLevel},
    weather::WeatherPlaces,
};
use voxear_scene::{Name, Transform, WorldOrigin};
use voxear_terrain::{
    blocks::{
        chunk::BlockId,
        file::{BlockFile, BlockKind},
        generate::{BlockEndless, BlockGenerate},
    },
    generate::{Landmass, Preset, WorldGen},
};

fn find(world: &voxear_ecs::World, name: &str) -> Entity {
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

/// How high the sea stands, in metres.
const SEA: f32 = 40.0;

/// Land that is sea floor everywhere, thirty metres under the sea.
fn sea_floor() -> Landmass {
    let flat = voxear_core::curve::Curve::flat;
    Landmass {
        coast: flat(-0.5),
        relief: flat(0.0),
        bumps: 0.0,
        height: 60.0,
        sea_level: SEA,
        ..Preset::RollingHills.landmass()
    }
}

/// A project whose level is a world of blocks without end, all of it sea
/// [`SEA`] metres up, with a camera just over the water, and the level's
/// storm rules as `rules` writes them.
fn sea_without_end(rules: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.03, 0.3, 0.42]) });
    let water = file.blocks.len() as BlockId;
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Sea.worldgen".into()),
        bottom: -4,
        caves: 0.0,
        sea: water,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let scene = format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
         [obj.block_world]\ndata = \"Blocks/Sea.blockworld\"\n\n[[obj]]\nid = 2\nname = \"Eye\"\n\
         position = [8.0, {}, 8.0]\n[obj.camera]\nfov = 60.0\n\n[[obj]]\nid = 3\nname = \"Weather\"\n{rules}",
        SEA + 3.0
    );
    let project = "[project]\nname = \"Sea\"\nstart_level = \"Levels/Main.level\"\n\n\
                   [floating_origin]\nmode = \"always\"\ndistance = 64\nstep = 64\n";
    let files = [
        ("voxear.toml", project.to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Sea.scene\"\n".into()),
        ("Scenes/Sea.scene", scene),
        ("Blocks/Sea.blockworld", file.to_text()),
        ("Worlds/Sea.worldgen", WorldGen::from(sea_floor()).to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn settle(game: &mut Game) {
    for _ in 0..3000 {
        game.tick(1.0 / 60.0, None);
        if voxear_engine::blocks::streamed(&game.engine.world) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the blocks round the camera were never all made");
}

#[test]
fn a_storm_crosses_a_sea_without_end_roughening_it_and_raining_on_whoever_is_under_it() {
    let dir = sea_without_end("[obj.storms]\nnatural = false\n");
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game);
    let (blocks, eye) = (find(&game.engine.world, "Blocks"), find(&game.engine.world, "Eye"));
    let world = &game.engine.world;
    // The world knows how high its sea is, and the sea is flat at that height.
    let level = world.get::<SeaLevel>(blocks).expect("a sea level").0;
    assert!((level - SEA).abs() <= 0.5, "the sea's top {level} m over the world's corner");
    let top = world.get::<Transform>(blocks).unwrap().position.y + level;
    // A point in the water, just under its top.
    let at = Vec3::new(8.0, top - 0.4, 8.0);
    let calm = water::sample(world, at).expect("sea under the camera");
    assert_eq!((calm.height, calm.normal, calm.flow), (top, Vec3::Y, Vec3::ZERO));
    assert!(world.resource::<Weather>().is_none_or(|weather| weather.now().is_none()), "clear before it comes");

    // A storm 600 m west, 250 m across, coming east at 25 m/s.
    let world = &mut game.engine.world;
    let id = storms::start(world, Vec3::new(-600.0, 0.0, 8.0), 250.0, 1.0, StormKind::ByTemperature);
    {
        let storm = world.resource_mut::<Storms>().unwrap().made_mut(id).unwrap();
        (storm.velocity, storm.grows) = ([25.0, 0.0], 0.0);
    }
    game.tick(1.0 / 60.0, None);
    let world = &game.engine.world;
    let places = &world.resource::<WeatherPlaces>().unwrap().0;
    assert_eq!(places.len(), 1, "seen from afar");
    assert!((places[0].middle - Vec2::new(-600.0, 8.0)).length() < 1.0 && places[0].amounts.cloud == 1.0);
    assert_eq!(water::sample(world, at).unwrap().height, top, "the sea here still flat");
    let under_it = water::sample(world, Vec3::new(-600.0, top, 8.0));
    assert!(under_it.is_none(), "no blocks made that far off to be sea");

    // Twenty-four seconds on its middle is over the camera: its weather is the viewer's, and the sea is rough.
    let mut lowest = top;
    let mut steepest = 1.0f32;
    let mut fastest = 0.0f32;
    for frame in 0..(24 * 60) {
        game.tick(1.0 / 60.0, None);
        if frame > 20 * 60 {
            let here = water::sample(&game.engine.world, at).unwrap();
            assert!(here.height <= top + 1e-3, "never over a crest's height: {}", here.height);
            lowest = lowest.min(here.height);
            steepest = steepest.min(here.normal.y);
            fastest = fastest.max(here.flow.length());
        }
    }
    let world = &mut game.engine.world;
    assert!(top - lowest > 1.0, "troughs over a metre down: {}", top - lowest);
    assert!(steepest < 0.99 && fastest > 0.3, "sloping and moving: {steepest}, {fastest} m/s");
    let now = world.resource::<Weather>().unwrap().now();
    let here = storms::weather_at(world, at);
    assert_eq!((here.storm, here.strength), (id, 1.0));
    assert!(matches!(here.falling, Falling::Thunderstorm | Falling::Blizzard | Falling::Sandstorm), "{here:?}");
    // What falls is what the open air's temperature makes it; far up over the same place, where it freezes, snow.
    match here.celsius {
        c if c > 2.0 && !(c >= 28.0 && here.humidity <= 0.25) => assert!(now.rain == 1.0 && now.snow == 0.0, "{now:?}"),
        c if c < 0.0 => assert!(now.snow == 1.0 && now.rain == 0.0, "{now:?}"),
        _ => {}
    }
    let high = storms::weather_at(world, at + Vec3::Y * 6000.0);
    assert!(high.celsius < here.celsius - 30.0 && high.falling == Falling::Blizzard, "{high:?}");
    assert_eq!(world.resource_mut::<Storms>().unwrap().take_events(), vec![StormEvent::Arrived(id)]);
    let clock = world.resource::<Storms>().unwrap().clock;
    assert_eq!(world.resource::<RoughSea>().unwrap().time, clock, "the sea's clock is the storms'");

    // It blows over, and the sea lies flat again under a clear sky.
    for _ in 0..(20 * 60) {
        game.tick(1.0 / 60.0, None);
    }
    let world = &mut game.engine.world;
    assert_eq!(water::sample(world, at).unwrap().height, top);
    assert!(world.resource::<Weather>().unwrap().now().is_none());
    let sea = &world.resource::<SeaNow>().expect("the storm's sea, further on").0;
    assert!(sea.rough_at(at.x, at.z) == 0.0 && sea.rough_at(at.x + 500.0, at.z) == 1.0);
    assert_eq!(world.resource_mut::<Storms>().unwrap().take_events(), vec![StormEvent::Left(id)]);

    // The camera flies after it, the world's middle moving as it goes: the storm keeps its place in the level.
    let before = WorldOrigin::of(world);
    for _ in 0..40 {
        game.engine.world.get_mut::<Transform>(eye).unwrap().position.x += 16.0;
        game.tick(1.0 / 60.0, None);
    }
    let world = &game.engine.world;
    let origin = WorldOrigin::of(world);
    assert_ne!(origin, before, "the middle moved");
    let storm = world.resource::<Storms>().unwrap().get(id).unwrap().clone();
    let place = world.resource::<WeatherPlaces>().unwrap().0[0];
    let in_level = origin.to_level(Vec3::new(place.middle.x, 0.0, place.middle.y));
    assert!(DVec2::new(in_level.x, in_level.z).distance(DVec2::from(storm.middle)) < 0.01);
    assert!((storm.middle[0] - (-600.0 + 25.0 * 44.7)).abs() < 30.0, "where 25 m/s took it: {}", storm.middle[0]);
}

#[test]
fn a_generated_sea_has_storms_of_its_own_the_same_every_time_and_great_whirlpools_where_it_is_deep() {
    let run = || {
        let dir = sea_without_end("[obj.storms]\nstanding = false\nfrequency = 1.0\n");
        let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
        game.start(None).unwrap();
        settle(&mut game);
        let world = &mut game.engine.world;
        assert_eq!(find(world, "Weather"), world.query_ref::<(Entity, &StormRules)>().into_iter().next().unwrap().0);
        // Two hours of the storms' clock, a minute at a time.
        let mut seen = Vec::new();
        for minute in 0..120 {
            world.resource_mut::<Storms>().unwrap().clock = f64::from(minute) * 60.0;
            storms::update(world, 0.0, true);
            let near = world.resource::<Storms>().unwrap().all();
            seen.extend(near.iter().map(|storm| (minute, storm.id, storm.middle)));
        }
        (seen, game)
    };
    let (seen, mut game) = run();
    let ids: std::collections::HashSet<u32> = seen.iter().map(|(_, id, _)| *id).collect();
    assert!(ids.len() >= 5, "storms came and went: {}", ids.len());
    assert!(seen.iter().all(|(_, id, _)| id & storms::NATURAL != 0));
    assert_eq!(run().0, seen, "the same storms from the same seed and clock");

    // Its great whirlpools stand where the seed puts them: with the camera at the nearest, the sea has it.
    let seed = WorldGen::from(sea_floor()).seed();
    let standing = storms::natural_whirlpools(seed, DVec2::ZERO, 30_000.0, &|_| 100.0);
    let nearest = standing
        .iter()
        .min_by(|a, b| DVec2::from(a.middle).length().total_cmp(&DVec2::from(b.middle).length()))
        .expect("a great whirlpool within thirty kilometres");
    let eye = find(&game.engine.world, "Eye");
    let origin = WorldOrigin::of(&game.engine.world).0;
    let there = Vec3::new((nearest.middle[0] - origin.x as f64) as f32 + 30.0, SEA + 3.0, (nearest.middle[1] - origin.z as f64) as f32);
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = there;
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    let world = &game.engine.world;
    let sea = world.resource::<RoughSea>().unwrap();
    let pool = sea.whirlpools.first().expect("the whirlpool in the sea");
    let in_level = WorldOrigin::of(world).to_level(Vec3::new(pool.middle.x, 0.0, pool.middle.y));
    assert!(DVec2::new(in_level.x, in_level.z).distance(DVec2::from(nearest.middle)) < 0.01 && pool.depth >= 7.0, "{pool:?}");
    assert!(world.resource::<SeaNow>().unwrap().0.at(pool.middle.x, pool.middle.y).sunk > 6.9);
}

/// A project whose world is a sea `deep` metres over a bed of sand with low bumps, `celsius` warm, growing every
/// starter plant of the sea's bed, seen from just over the bed.
fn sea_bed(celsius: f32, deep: f32) -> tempfile::TempDir {
    use voxear_engine::grown_plants;
    use voxear_render::StarterPlant;
    use voxear_terrain::{blocks::block::BlockDef, generate::Climate};
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.blocks[0] = BlockKind::of("Blocks/Sand.block");
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.02, 0.36, 0.44]) });
    let water = file.blocks.len() as BlockId;
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Sea.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        sea: water,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let flat = voxear_core::curve::Curve::flat;
    let climate = Climate { temperature: celsius, temperature_range: 0.0, cooling: 0.0, ..Climate::default() };
    let land = Landmass {
        coast: flat(-deep / 60.0),
        relief: flat(0.0),
        bumps: 2.0,
        height: 60.0,
        sea_level: SEA,
        climate,
        ..Preset::RollingHills.landmass()
    };
    let mut scene = format!(
        "format = 1\nlighting = \"builtin:highland_summer\"\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n\
         [obj.block_world]\ndata = \"Blocks/Sea.blockworld\"\n\n\
         [[obj]]\nid = 2\nname = \"Eye\"\nposition = [40.0, {}, 40.0]\nrotation = [14.0, 35.0, 0.0]\n\
         [obj.camera]\nfov = 60.0\n\n\
         [[obj]]\nid = 3\nname = \"Sun\"\nrotation = [55.0, -60.0, 0.0]\n[obj.light]\ntype = \"directional\"\n",
        SEA - deep + 3.5
    );
    std::fs::create_dir_all(dir.path().join("Plants")).unwrap();
    for (at, plant) in StarterPlant::ALL.into_iter().enumerate().filter(|(_, plant)| plant.under_water()) {
        let path = format!("Plants/{}.boxmodel", plant.name());
        std::fs::write(dir.path().join(&path), plant.model().to_text()).unwrap();
        let (mesh, material) = (format!("{path}#mesh0.0"), format!("{path}#material0"));
        let foliage = grown_plants::foliage_text(&mesh, Some(&material), Some(plant), (None, None));
        scene += &format!("\n[[obj]]\nid = {}\nname = \"{}\"\nparent = 1\n{foliage}", at + 10, plant.name());
    }
    let files = [
        ("voxear.toml", "[project]\nname = \"Sea\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Sea.scene\"\n".into()),
        ("Scenes/Sea.scene", scene),
        ("Blocks/Sea.blockworld", file.to_text()),
        ("Blocks/Sand.block", BlockDef::colored([0.82, 0.74, 0.52]).to_text()),
        ("Worlds/Sea.worldgen", WorldGen::from(land).to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// How many plants of each kind stand on the bed of [`sea_bed`]'s sea once its ground and plants are made; drawn,
/// and the picture kept as `name`, where there is a GPU.
fn grown_on_the_bed(celsius: f32, deep: f32, name: &str) -> Vec<(String, usize)> {
    use voxear_engine::{grown_plants, scene_file};
    use voxear_render::{Renderer, RendererSettings};
    let mut renderer = Renderer::new_offscreen(1280, 720, &RendererSettings::default()).ok();
    let dir = sea_bed(celsius, deep);
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(renderer.as_mut()).unwrap();
    for tick in 0..4000 {
        game.tick(1.0 / 60.0, renderer.as_mut());
        let made = voxear_engine::blocks::streamed(&game.engine.world) && grown_plants::settled(&game.engine.world);
        if made && tick >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    if let Some(renderer) = renderer.as_mut() {
        for frame in 0..40 {
            game.tick(1.0 / 60.0, Some(renderer));
            renderer.render(&game.engine.world, 1.0 + frame as f32 / 60.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
    }
    let world = &game.engine.world;
    let sources = world.query_ref::<(Entity, &Name, &scene_file::FoliageSource)>().into_iter();
    sources.map(|(entity, name, _)| (name.0.clone(), grown_plants::standing(world, entity).len())).collect()
}

#[test]
fn a_warm_shallow_sea_grows_a_reef_on_its_bed_and_a_cool_deep_one_a_forest_of_kelp() {
    let count = |counts: &[(String, usize)], name: &str| counts.iter().find(|(kind, _)| kind == name).map_or(0, |(_, n)| *n);
    // Warm and eight metres deep: coral of every kind, seagrass, sponges and clams, and no kelp.
    let reef = grown_on_the_bed(26.0, 8.0, "sea bed reef.png");
    for coral in ["Branch Coral", "Brain Coral", "Table Coral", "Fan Coral"] {
        assert!(count(&reef, coral) > 10, "{coral}: {reef:?}");
    }
    assert!(count(&reef, "Seagrass") > 200 && count(&reef, "Tube Sponge") > 3 && count(&reef, "Clam") > 3, "{reef:?}");
    assert_eq!(count(&reef, "Kelp"), 0, "too warm and too shallow for kelp: {reef:?}");
    // Cool and sixteen metres deep: kelp, and no coral or seagrass.
    let forest = grown_on_the_bed(9.0, 16.0, "sea bed kelp.png");
    assert!(count(&forest, "Kelp") > 100, "{forest:?}");
    for none in ["Branch Coral", "Brain Coral", "Table Coral", "Fan Coral", "Seagrass"] {
        assert_eq!(count(&forest, none), 0, "{none}: {forest:?}");
    }
    assert!(count(&forest, "Clam") > 0 && count(&forest, "Tube Sponge") > 0, "{forest:?}");
}

/// Pictures of storms over the sea of a project of blocks named by `VOXEAR_STORMS_PROJECT`, kept in
/// `VOXEAR_TEST_OUTPUT`: a storm coming over the sea from the shore, the same storm overhead, and a whirlpool.
/// Not a test of anything: run by hand, to look at.
#[test]
#[ignore = "draws the project VOXEAR_STORMS_PROJECT names"]
fn pictures_of_storms_over_a_projects_sea() {
    use voxear_engine::{blocks::BlockWorldAsset, grown_plants};
    use voxear_render::{Camera, Renderer, RendererSettings};
    let (Ok(project), Ok(out)) = (std::env::var("VOXEAR_STORMS_PROJECT"), std::env::var("VOXEAR_TEST_OUTPUT")) else {
        return eprintln!("skipping: needs VOXEAR_STORMS_PROJECT and VOXEAR_TEST_OUTPUT");
    };
    let mut renderer = Renderer::new_offscreen(1280, 720, &RendererSettings::default()).expect("a GPU");
    let mut game = Game::new(Project::open(std::path::Path::new(&project)).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    game.tick(1.0 / 60.0, Some(&mut renderer));
    // The world's generator, and where the level is in it.
    let world = &game.engine.world;
    let (blocks, asset) = world.query_ref::<(Entity, &BlockWorldAsset)>().into_iter().next().map(|(e, a)| (e, a.clone())).unwrap();
    let corner = world.get::<Transform>(blocks).unwrap().position;
    let position = asset.file.generate.as_ref().unwrap().position;
    let generator = voxear_engine::blocks::generator(&mut game.content, &asset.file).expect("a generated world");
    let depth = |x: f32, z: f32| generator.sea_level() - generator.height_at(x - corner.x + position[0], z - corner.z + position[1]);
    let top = corner.y + game.engine.world.get::<SeaLevel>(blocks).expect("a sea").0;
    // A place in shallow water near the level's middle, and the way out to the deepest water from it.
    let ways: Vec<Vec3> = (0..16).map(|i| Vec3::new((i as f32 * 0.3927).cos(), 0.0, (i as f32 * 0.3927).sin())).collect();
    let shore = (1..150)
        .flat_map(|ring| ways.iter().map(move |way| *way * (ring as f32 * 20.0)))
        .find(|at| (2.0..6.0).contains(&depth(at.x, at.z)) && ways.iter().any(|w| depth(at.x + w.x * 250.0, at.z + w.z * 250.0) > 15.0))
        .expect("a shore within three kilometres");
    let out_to_sea = *ways
        .iter()
        .max_by(|a, b| {
            let far = |w: &Vec3| depth(shore.x + w.x * 250.0, shore.z + w.z * 250.0) + depth(shore.x + w.x * 600.0, shore.z + w.z * 600.0);
            far(a).total_cmp(&far(b))
        })
        .unwrap();
    let camera = game.engine.world.query_ref::<(Entity, &Camera)>().into_iter().next().unwrap().0;
    game.engine.world.insert(camera, StormRules { natural: false, ..StormRules::default() });
    let mut look = |game: &mut Game, eye: Vec3, at: Vec3| {
        let towards = (at - eye).normalize();
        let rotation = voxear_core::math::Quat::from_rotation_y(towards.x.atan2(towards.z))
            * voxear_core::math::Quat::from_rotation_x(-towards.y.asin());
        *game.engine.world.get_mut::<Transform>(camera).unwrap() = Transform::new(eye, rotation, Vec3::ONE);
    };
    let mut frames = 0u32;
    let mut draw = |game: &mut Game, renderer: &mut Renderer, seconds: f32, name: Option<&str>| {
        for _ in 0..(seconds * 60.0) as u32 {
            game.tick(1.0 / 60.0, Some(renderer));
            frames += 1;
            renderer.render(&game.engine.world, frames as f32 / 60.0).unwrap();
        }
        let Some(name) = name else { return };
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    };
    let settle = |game: &mut Game, renderer: &mut Renderer| {
        for tick in 0..6000 {
            game.tick(1.0 / 60.0, Some(renderer));
            let made = voxear_engine::blocks::streamed(&game.engine.world) && grown_plants::settled(&game.engine.world);
            if made && tick >= 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    // From the shore, a calm sea; then a storm three kilometres out, and the same storm come in over the viewer.
    let eye = shore + Vec3::Y * (top + 4.0) - out_to_sea * 6.0;
    look(&mut game, eye, eye + out_to_sea * 100.0 + Vec3::Y * 12.0);
    settle(&mut game, &mut renderer);
    draw(&mut game, &mut renderer, 1.5, Some("endless calm sea.png"));
    let far_off = eye + out_to_sea * 3000.0;
    let here = storms::weather_at(&game.engine.world, eye);
    eprintln!("the open air at the shore: {:.1} °C, humidity {:.2}", here.celsius, here.humidity);
    let id = storms::start(&mut game.engine.world, far_off, 900.0, 1.0, StormKind::Thunderstorm);
    game.engine.world.resource_mut::<Storms>().unwrap().made_mut(id).unwrap().grows = 0.0;
    draw(&mut game, &mut renderer, 2.0, Some("endless storm coming.png"));
    let near = eye + out_to_sea * 250.0;
    game.engine.world.resource_mut::<Storms>().unwrap().made_mut(id).unwrap().middle = [near.x as f64, near.z as f64];
    draw(&mut game, &mut renderer, 6.0, Some("endless storm overhead.png"));
    look(&mut game, eye + Vec3::Y * 2.0, eye + out_to_sea * 30.0 + Vec3::Y * -3.0);
    draw(&mut game, &mut renderer, 1.0, Some("endless storm sea close.png"));
    // The storm gone, a whirlpool in deeper water, seen from over its rim.
    game.engine.world.resource_mut::<Storms>().unwrap().stop(id, 0.0);
    let pool = shore + out_to_sea * 260.0;
    storms::add_whirlpool(&mut game.engine.world, pool, 22.0, 8.0);
    let over = pool + Vec3::Y * (top + 16.0) - out_to_sea * 34.0;
    look(&mut game, over, pool + Vec3::Y * (top - 4.0));
    settle(&mut game, &mut renderer);
    draw(&mut game, &mut renderer, 2.0, Some("endless whirlpool.png"));
}
