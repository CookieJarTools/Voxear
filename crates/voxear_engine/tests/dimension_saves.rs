//! What is changed in a dimension, kept apart from the level's: kept when
//! the dimension is let go and there again when it is made again, a world
//! without end's changes to chunks streamed out among them; written for a
//! save of the scene in a few bytes and put back from them; and a dungeon
//! grown a floor keeps what was changed in its floors above. Headless: no
//! GPU, no scripts.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project, dimension_saves,
    dimensions::{self, DimensionId},
    dungeons::{DungeonGate, DungeonInside, DungeonStage, DungeonStages},
};
use voxear_render::Camera;
use voxear_scene::{Parent, Transform};
use voxear_terrain::{
    blocks::{
        BlockWorld, BuiltBlocks,
        file::BlockFile,
        generate::{BlockEndless, BlockGenerate},
    },
    generate::Preset,
};

const FRAME: f32 = 1.0 / 60.0;
/// How large the caves' blocks are, in metres.
const SIZE: f32 = 2.0;
/// The block the tests build with: one the worlds' files number.
const BUILT: u16 = 1;

fn write(dir: &std::path::Path, files: &[(&str, String)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// A level of rolling hills without end, and a dimension of caves whose
/// own world without end is canyons; the game started, with a walker and
/// its camera in the level.
fn caves() -> (tempfile::TempDir, Game, Entity) {
    let dir = tempfile::tempdir().unwrap();
    let endless = |world: &str| {
        let mut file = BlockFile { block_size: SIZE, ..BlockFile::default() };
        file.generate = Some(BlockGenerate {
            world: Some(world.into()),
            bottom: -4,
            caves: 0.0,
            endless: Some(BlockEndless { distance: 70.0, far: 0.0, ..BlockEndless::default() }),
            ..BlockGenerate::default()
        });
        file.to_text()
    };
    let ground = |name: &str| {
        format!("[[obj]]\nid = 1\nname = \"Ground\"\n[obj.block_world]\ndata = \"Blocks/{name}.blockworld\"\n")
    };
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Saves\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", format!("format = 1\n\n{}", ground("Hills"))),
            (
                "Dimensions/Caves.dimension",
                format!("format = 1\n\n[dimension]\nkind = \"caves\"\n\n{}", ground("Canyons")),
            ),
            ("Blocks/Hills.blockworld", endless("Worlds/Hills.worldgen")),
            ("Blocks/Canyons.blockworld", endless("Worlds/Canyons.worldgen")),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
            ("Worlds/Canyons.worldgen", Preset::Canyons.world().to_text()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &mut game.engine.world;
    let walker = world.spawn((Transform::from_position(Vec3::new(9.0, 300.0, 9.0)),));
    world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    (dir, game, walker)
}

/// Ticks until the worlds of the dimension seen have made their blocks round where it is seen from.
fn settle(game: &mut Game) {
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    for _ in 0..6000 {
        game.tick(FRAME, None);
        if voxear_engine::blocks::streamed(&game.engine.world) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the blocks were never all made");
}

fn ground_of(game: &Game, dimension: DimensionId) -> Entity {
    let world = &game.engine.world;
    let found = world.query_ref::<(Entity, &BlockWorld)>().into_iter().map(|(entity, _)| entity);
    found.into_iter().find(|&entity| dimensions::dimension_of(world, entity) == dimension).expect("a world there")
}

/// The block of a world at a column and height, in blocks.
fn block(game: &Game, ground: Entity, at: [i32; 3]) -> u16 {
    game.engine.world.get::<BlockWorld>(ground).map_or(0, |held| held.blocks.get(at))
}

/// Builds a tower of three blocks on the highest block of a column, as a
/// script's change does: the world kept as built first. Gives its foot.
fn build_tower(game: &mut Game, ground: Entity, column: [i32; 2]) -> [i32; 3] {
    let world = &mut game.engine.world;
    let mut held = world.get::<BlockWorld>(ground).cloned().unwrap();
    let top = (-8..200).rev().find(|&y| held.blocks.get([column[0], y, column[1]]) != 0).expect("ground there");
    let foot = [column[0], top + 3, column[1]];
    if !world.has::<BuiltBlocks>(ground) {
        world.insert(ground, BuiltBlocks(held.blocks.clone()));
    }
    held.set_blocks((0..3).map(|up| ([foot[0], foot[1] + up, foot[2]], BUILT)));
    world.insert(ground, held);
    foot
}

fn tower_stands(game: &Game, ground: Entity, foot: [i32; 3]) -> bool {
    (0..3).all(|up| block(game, ground, [foot[0], foot[1] + up, foot[2]]) == BUILT)
}

#[test]
fn a_dimension_let_go_keeps_what_was_changed_in_it_and_a_save_keeps_it_in_a_few_bytes() {
    let (_dir, mut game, walker) = caves();
    let level = game.levels.current().map(str::to_string);
    let guid = game.content.db().guid_of("Dimensions/Caves.dimension").unwrap();
    let caves = game.make_dimension(guid, 0, None).unwrap();
    assert!(dimensions::move_obj(&mut game.engine.world, walker, caves));
    settle(&mut game);
    let canyons = ground_of(&game, caves);
    let foot = build_tower(&mut game, canyons, [4, 4]);
    assert!(tower_stands(&game, canyons, foot));
    // Nothing of it is in the level's own changes, and the caves' are a few bytes.
    let changed = dimension_saves::capture(&game.engine.world, &mut game.content, caves).expect("changed");
    assert!(changed.len() < 160, "three blocks changed, in {} bytes", changed.len());

    // Walked far off, the column streams out: its changes wait for it, and are still kept.
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(1201.0, 300.0, 9.0);
    settle(&mut game);
    assert_eq!(block(&game, canyons, foot), 0, "streamed out");
    let afar = dimension_saves::capture(&game.engine.world, &mut game.content, caves).expect("still changed");
    assert!(afar.len() < 160, "the same three blocks, kept while their chunk is not made: {} bytes", afar.len());

    // Let go, with the walker back in the level, and made again: the tower is there once its column is.
    assert!(dimensions::move_obj(&mut game.engine.world, walker, DimensionId::LEVEL));
    game.tick(FRAME, None);
    assert!(game.let_go_dimension(caves));
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    assert!(game.engine.world.resource::<dimensions::Dimensions>().is_none(), "nothing made but the level's");
    let kept = game.engine.world.resource::<dimension_saves::DimensionEdits>().expect("kept for when it is made");
    assert_eq!(kept.sizes().count(), 1);
    let caves = game.make_dimension(guid, 0, None).unwrap();
    let canyons = ground_of(&game, caves);
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(9.0, 300.0, 9.0);
    assert!(dimensions::move_obj(&mut game.engine.world, walker, caves));
    settle(&mut game);
    assert!(tower_stands(&game, canyons, foot), "made again with what was changed in it");

    // A save of the scene: the caves' changes in a few bytes, none for the level's, which nobody changed.
    let saved = dimension_saves::capture_all(&mut game.engine.world, &mut game.content, level.as_deref());
    assert!(!saved.is_empty() && saved.len() < 200, "one dimension, three blocks: {} bytes", saved.len());
    eprintln!("a save of one dimension with three blocks changed: {} bytes", saved.len());
    // Put back over a game that went on: the tower taken down, then the save loaded.
    let mut held = game.engine.world.get::<BlockWorld>(canyons).cloned().unwrap();
    held.set_blocks((0..3).map(|up| ([foot[0], foot[1] + up, foot[2]], 0)));
    game.engine.world.insert(canyons, held);
    assert!(!tower_stands(&game, canyons, foot));
    let started = std::time::Instant::now();
    let found =
        dimension_saves::apply_all(&mut game.engine.world, &mut game.content, level.as_deref(), &saved).unwrap();
    let took = started.elapsed();
    eprintln!("put back in {took:?}");
    assert_eq!(found, 1, "the caves, made now, took theirs");
    assert!(took < std::time::Duration::from_millis(250), "put back at once: {took:?}");
    assert!(tower_stands(&game, canyons, foot), "the save's tower");
    // And a save with none: nothing kept, and a dimension never changed has no entry.
    dimension_saves::apply_all(&mut game.engine.world, &mut game.content, level.as_deref(), &[]).unwrap();
    let kept = game.engine.world.resource::<dimension_saves::DimensionEdits>().unwrap();
    assert_eq!(kept.sizes().count(), 0);
    assert!(dimension_saves::DimensionEdits::from_bytes(b"VXDM\x09\0\0\0").is_err(), "from a newer Voxear");
}

/// A level of rolling hills without end with small dungeons in dimensions of their own, two floors each.
fn dungeons() -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -60,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0, ..BlockEndless::default() }),
        structures: vec!["Structures/Dungeon.structure".into()],
        ..BlockGenerate::default()
    });
    let structure = "ground = \"leave\"\n\n[placement]\nspacing = 240.0\nseparation = 60.0\nchance = 1.0\non = \"land\"\n\
                     height = [0.0, 500.0]\nslope = 60.0\n\n[dungeon]\nsize = \"small\"\nfloors = [2, 2]\n\n\
                     [dungeon.blocks]\nstone = \"Blocks/Temple Stone.block\"\nlight = \"Blocks/Fire.block\"\n";
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Hills\"\n[obj.block_world]\ndata = \"Blocks/Hills.blockworld\"\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Dungeons\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Hills.scene\"\n".into()),
            ("Scenes/Hills.scene", scene.into()),
            ("Blocks/Hills.blockworld", file.to_text()),
            ("Structures/Dungeon.structure", structure.into()),
            ("Blocks/Temple Stone.block", "color = [0.62, 0.58, 0.5]\n".into()),
            ("Blocks/Fire.block", "color = [1.0, 0.62, 0.25]\nlight = 15\n".into()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (dir, game)
}

/// The nearest dungeon's gate, as scripts read it.
fn first_gate(game: &mut Game) -> DungeonGate {
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 800.0])
        .unwrap();
    let found: serde_json::Value = serde_json::from_str(&json).unwrap();
    let site = &found["dungeons"][0]["site"];
    assert_eq!(found["dungeons"][0]["stage"], 0, "{found}");
    DungeonGate {
        world: Entity::from_bits(site["world"].as_u64().unwrap()),
        structure: site["structure"].as_u64().unwrap() as usize,
        site: [site["at"][0].as_f64().unwrap() as f32, site["at"][1].as_f64().unwrap() as f32],
        seed: site["seed"].as_u64().unwrap() as u32,
    }
}

/// A dungeon dimension's world of rock and its layout's floors.
fn rock_of(game: &Game, id: DimensionId) -> (Entity, u32) {
    let world = &game.engine.world;
    let mut found = world.query_ref::<(Entity, &DungeonInside)>().into_iter();
    let (rock, inside) = found.find(|(entity, _)| dimensions::dimension_of(world, *entity) == id).expect("its rock");
    (rock, inside.0.floors)
}

#[test]
fn a_dungeon_grown_a_floor_keeps_what_was_changed_in_its_floors_above_and_scripts_read_it_from_inside() {
    let (_dir, mut game) = dungeons();
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    let gate = first_gate(&mut game);
    let guid = game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(gate.world).unwrap().guid;
    assert_eq!(gate.key(guid), gate.key(guid));
    assert_ne!(
        gate.key(guid),
        DungeonGate { seed: gate.seed, site: [gate.site[0] + 1.0, gate.site[1]], ..gate }.key(guid)
    );
    let (inside, _) = game.make_dungeon_dimension(gate, None).unwrap();
    let (rock, floors) = rock_of(&game, inside);
    assert_eq!(floors, 2);
    // A tower in its first room, as a player might build.
    let layout = game.engine.world.get::<DungeonInside>(rock).unwrap().0.clone();
    let room = layout
        .spaces
        .iter()
        .find(|space| {
            space.floor == 0 && space.height >= 3 && space.kind != voxear_terrain::blocks::dungeon::SpaceKind::Landing
        })
        .unwrap();
    let foot = [room.at[0], room.at[1] + 1, room.at[2]];
    let mut held = game.engine.world.get::<BlockWorld>(rock).cloned().unwrap();
    game.engine.world.insert(rock, BuiltBlocks(held.blocks.clone()));
    held.set_blocks((0..3).map(|up| ([foot[0], foot[1] + up, foot[2]], BUILT)));
    game.engine.world.insert(rock, held);
    assert!(tower_stands(&game, rock, foot));

    // Read from inside its dimension, Dungeons lists it whole, at its stage.
    game.engine.world.insert_resource(voxear_scene::CurrentDimension(inside));
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 10.0])
        .unwrap();
    game.engine.world.insert_resource(voxear_scene::CurrentDimension(DimensionId::LEVEL));
    let read: serde_json::Value = serde_json::from_str(&json).unwrap();
    let whole = &read["dungeons"][0];
    assert_eq!(
        (whole["inside"].as_bool(), whole["floors"].as_u64(), whole["stage"].as_u64()),
        (Some(true), Some(2), Some(0))
    );
    assert!(whole["spaces"].as_array().unwrap().len() > 3, "its spaces, not only its gate: {whole}");
    assert_eq!(
        whole["dimension"].as_u64().map(|handle| handle as u32),
        dimensions::handle_of(&game.engine.world, inside)
    );

    // Not grown while somebody is in it.
    let visitor = game.engine.world.spawn((Camera::default(), Transform::default()));
    dimensions::set_dimension(&mut game.engine.world, visitor, inside);
    assert!(!voxear_engine::dimension_scripts::grow(&mut game.engine.world, gate), "somebody is in it");
    assert_eq!(DungeonStages::of(&game.engine.world, gate.site()), DungeonStage::default());
    game.engine.world.despawn(visitor);
    // Grown a stage with nobody in it: let go, and made again a floor deeper with the tower where it was.
    assert!(voxear_engine::dimension_scripts::grow(&mut game.engine.world, gate));
    game.tick(FRAME, None);
    assert!(
        dimensions::from_handle(&game.engine.world, whole["dimension"].as_u64().unwrap() as u32).is_none(),
        "let go to grow"
    );
    let stage = DungeonStages::of(&game.engine.world, gate.site());
    DungeonStages::set(&mut game.engine.world, gate.site(), DungeonStage { cleared: true, ..stage });
    let (inside, _) = game.make_dungeon_dimension(gate, None).unwrap();
    let (rock, floors) = rock_of(&game, inside);
    assert_eq!(floors, 3, "a floor more");
    assert!(tower_stands(&game, rock, foot), "what was changed in its floors above kept");
    let grown = &game.engine.world.get::<DungeonInside>(rock).unwrap().0;
    let first_floor = |layout: &voxear_terrain::blocks::dungeon::DungeonLayout| -> Vec<[i32; 3]> {
        layout.spaces.iter().filter(|space| space.floor == 0).map(|space| space.at).collect()
    };
    assert_eq!(first_floor(grown), first_floor(&layout), "its first floor laid as it was");
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 800.0])
        .unwrap();
    let read: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        (read["dungeons"][0]["stage"].as_u64(), read["dungeons"][0]["cleared"].as_bool()),
        (Some(1), Some(true))
    );
}
