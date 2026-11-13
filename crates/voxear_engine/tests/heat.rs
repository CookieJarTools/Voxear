//! Heat in a game, headless: a desert's generator makes it hot by day and
//! cold by night, magma blocks are hot round them, a cave is mild, and raw
//! meat sears into one prefab on a hot stone and freezes into another in
//! the snow.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    heat::{self, Climate, HeatState},
};
use voxear_scene::{Name, Transform};
use voxear_terrain::{
    blocks::{
        BlockWorld,
        block::{BlockDef, BlockHeat},
        file::{BlockFile, BlockKind},
        generate::BlockGenerate,
    },
    generate::{Landmass, Preset, WorldGen},
};

fn write(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn project(scene: String, more: &[(&str, String)]) -> tempfile::TempDir {
    let mut files = vec![
        ("voxear.toml", "[project]\nname = \"Heat\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".to_string()),
        ("Scenes/Main.scene", scene),
    ];
    files.extend(more.iter().cloned());
    write(&files)
}

fn started(dir: &tempfile::TempDir) -> Game {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    game
}

fn named(game: &Game, name: &str) -> Vec<Entity> {
    let world = &game.engine.world;
    world.query_ref::<(Entity, &Name)>().into_iter().filter(|(_, n)| n.0 == name).map(|(e, _)| e).collect()
}

fn set_hour(game: &mut Game, hours: f32) {
    let world = &mut game.engine.world;
    let climate = world.query::<&mut Climate>().into_iter().next().unwrap();
    climate.time_of_day = hours;
}

/// A dry, hot land with no sea, as flat as the battlefield's.
fn desert() -> WorldGen {
    let plains = Preset::BattlefieldPlains.landmass();
    let mut climate = plains.climate;
    (climate.temperature, climate.temperature_range, climate.humidity, climate.humidity_range) = (32.0, 0.0, 0.05, 0.0);
    WorldGen::from(Landmass { climate, ..plains })
}

#[test]
fn a_desert_is_hot_by_day_and_cold_by_night() {
    let mut file = BlockFile::default();
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Desert.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        ..BlockGenerate::default()
    });
    let scene = "format = 1\n\n[[obj]]\nid = 1\nname = \"Sand\"\n[obj.block_world]\ndata = \"Blocks/Sand.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Climate\"\n[obj.climate]\ntime_of_day = 15.0\nday_length = 0.0\nday_swing = 12.0\n"
        .to_string();
    let dir =
        project(scene, &[("Blocks/Sand.blockworld", file.to_text()), ("Worlds/Desert.worldgen", desert().to_text())]);
    let mut game = started(&dir);
    let ground = desert().height_at(32.0, 32.0);
    let over = Vec3::new(32.0, ground + 2.0, 32.0);

    let day = heat::sample(&mut game.engine.world, over);
    assert!(day.climate > 25.0 && day.humidity < 0.2, "the generator's desert: {day:?}");
    set_hour(&mut game, 4.0);
    let night = heat::sample(&mut game.engine.world, over);
    assert_eq!(night.climate, day.climate);
    assert!(day.celsius > 45.0 && night.celsius < 20.0, "{} by day, {} by night", day.celsius, night.celsius);
    assert!(day.celsius - night.celsius > 30.0, "a dry place swings wide");
}

/// A world of blocks on flat ground eight high, with a magma block it holds as its fourth, and a level as hot and
/// dry as `celsius` says, at three in the afternoon.
fn blocks_project(celsius: f32) -> tempfile::TempDir {
    let mut file = BlockFile::default();
    file.blocks.push(BlockKind::of("Blocks/Magma.block"));
    let magma =
        BlockDef { heat: Some(BlockHeat { celsius: 1100.0, reach: 4.0 }), ..BlockDef::colored([0.9, 0.3, 0.05]) };
    let scene = format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
         [[obj]]\nid = 2\nname = \"Climate\"\n[obj.climate]\ntime_of_day = 15.0\nday_length = 0.0\n\
         celsius = {celsius:.1}\nhumidity = 0.1\nday_swing = 8.0\n"
    );
    project(scene, &[("Blocks/Field.blockworld", file.to_text()), ("Blocks/Magma.block", magma.to_text())])
}

fn blocks(game: &mut Game) -> &mut BlockWorld {
    let entity = named(game, "Blocks")[0];
    game.engine.world.get_mut::<BlockWorld>(entity).unwrap()
}

/// Puts a hollow box of stone on the ground, its room from (22, 8, 22) to (32, 13, 32), under four blocks of roof.
fn dig_cave(game: &mut Game) {
    let mut changes = Vec::new();
    for x in 20..34 {
        for y in 8..18 {
            for z in 20..34 {
                let room = (22..32).contains(&x) && (8..13).contains(&y) && (22..32).contains(&z);
                changes.push(([x, y, z], if room { 0 } else { 3 }));
            }
        }
    }
    blocks(game).set_blocks(changes);
}

#[test]
fn it_is_hot_near_magma_blocks_and_mild_in_a_cave() {
    let dir = blocks_project(35.0);
    let mut game = started(&dir);
    let open = heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 20.0));
    assert!(open > 40.0, "a hot afternoon in the open: {open}");
    let beside = heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 12.0));

    blocks(&mut game).set_blocks([([10, 8, 10], 4)]);
    let near = heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 12.0));
    let farther = heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 13.5));
    assert!(near > 300.0 && farther < near && farther > open, "{near} near the magma, {farther} farther off");
    assert_eq!(heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 20.0)), open, "past its reach");
    // Taken away, it is as warm as round about again.
    blocks(&mut game).set_blocks([([10, 8, 10], 0)]);
    assert_eq!(heat::at(&mut game.engine.world, Vec3::new(10.5, 9.0, 12.0)), beside);

    dig_cave(&mut game);
    let inside = Vec3::new(27.0, 9.5, 27.0);
    let open = heat::at(&mut game.engine.world, Vec3::new(50.0, 9.5, 50.0));
    let cave = heat::sample(&mut game.engine.world, inside);
    assert!(cave.shelter > 0.6, "under its roof: {cave:?}");
    assert!(cave.celsius < open - 10.0 && cave.celsius > 10.0, "cool in a hot land: {} against {open}", cave.celsius);

    // In a cold land before dawn, the same cave is far warmer than the open.
    let cold = blocks_project(-25.0);
    let mut game = started(&cold);
    dig_cave(&mut game);
    set_hour(&mut game, 4.0);
    let open = heat::at(&mut game.engine.world, Vec3::new(50.0, 9.5, 50.0));
    let cave = heat::at(&mut game.engine.world, inside);
    assert!(cave > open + 20.0 && cave < 0.0, "warm in the snow, if not warm: {cave} against {open}");
}

const RAW_MEAT: &str = r#"
format = 1

[[obj]]
id = 1
name = "Raw Meat"
[obj.heat_body]
warms_in = 1.0

[[obj.heat_body.changes]]
name = "Seared"
when = "above"
celsius = 80.0
seconds = 3.0
becomes = "Prefabs/Seared.prefab"

[[obj.heat_body.changes]]
name = "Frozen"
when = "below"
celsius = -5.0
seconds = 3.0
becomes = "Prefabs/Frozen.prefab"
"#;

#[test]
fn raw_meat_sears_on_a_hot_stone_and_freezes_in_the_snow() {
    let scene = "format = 1\n\n\
        [[obj]]\nid = 1\nname = \"Climate\"\n[obj.climate]\ncelsius = 15.0\nday_swing = 0.0\n\n\
        [[obj]]\nid = 2\nname = \"Hot Stone\"\n[obj.heat_source]\ncelsius = 250.0\nradius = 2.0\n\n\
        [[obj]]\nid = 3\nprefab = \"Prefabs/RawMeat.prefab\"\nposition = [0.0, 0.2, 0.0]\n\n\
        [[obj]]\nid = 4\nname = \"Snowfield\"\nposition = [50.0, 0.0, 0.0]\n\
        [obj.heat_zone]\ncelsius = -25.0\nsize = [10.0, 4.0, 10.0]\nedge = 0.0\n\n\
        [[obj]]\nid = 5\nprefab = \"Prefabs/RawMeat.prefab\"\nposition = [50.0, 0.2, 0.0]\n\n\
        [[obj]]\nid = 6\nprefab = \"Prefabs/RawMeat.prefab\"\nposition = [-30.0, 0.2, 0.0]\n"
        .to_string();
    let seared = "format = 1\n\n[[obj]]\nid = 1\nname = \"Seared Meat\"\n[obj.heat_body]\nwarms_in = 1.0\n";
    let frozen = "format = 1\n\n[[obj]]\nid = 1\nname = \"Frozen Meat\"\n";
    let dir = project(
        scene,
        &[
            ("Prefabs/RawMeat.prefab", RAW_MEAT.to_string()),
            ("Prefabs/Seared.prefab", seared.to_string()),
            ("Prefabs/Frozen.prefab", frozen.to_string()),
        ],
    );
    let mut game = started(&dir);
    assert_eq!(named(&game, "Raw Meat").len(), 3);

    // Two seconds in, both are on their way, and neither has changed.
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    assert_eq!(named(&game, "Raw Meat").len(), 3);
    let warming: Vec<f32> = named(&game, "Raw Meat")
        .into_iter()
        .map(|meat| game.engine.world.get::<HeatState>(meat).unwrap().temperature)
        .collect();
    assert!(warming.iter().any(|&t| t > 80.0) && warming.iter().any(|&t| t < -5.0), "{warming:?}");

    for _ in 0..360 {
        game.tick(1.0 / 60.0, None);
    }
    let seared = named(&game, "Seared Meat");
    let frozen = named(&game, "Frozen Meat");
    assert_eq!((seared.len(), frozen.len()), (1, 1), "each became its prefab");
    let world = &game.engine.world;
    let at = |entity: Entity| world.get::<Transform>(entity).unwrap().position;
    assert_eq!(at(seared[0]), Vec3::new(0.0, 0.2, 0.0), "in the raw meat's place");
    assert_eq!(at(frozen[0]), Vec3::new(50.0, 0.2, 0.0));
    let still_hot = world.get::<HeatState>(seared[0]).unwrap().temperature;
    assert!(still_hot > 80.0, "as hot as the meat it was: {still_hot}");
    let left = named(&game, "Raw Meat");
    assert_eq!(left.len(), 1, "the one left out in the mild air stays raw");
    assert_eq!(at(left[0]), Vec3::new(-30.0, 0.2, 0.0));
}
