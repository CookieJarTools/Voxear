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
