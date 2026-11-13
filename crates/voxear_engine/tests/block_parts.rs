//! Blocks of parts in a game: a lever opens the door it is linked to and a
//! button opens one for a moment, both moving by their motions; the links
//! and the open door are saved beside the world and come back with it; and
//! a generated world's structures put their doors down turned as each
//! structure stands; and a block of parts painted keeps its shape and the
//! way it stands, a generated slab becoming the painted block's slab; and
//! an item dropped on a slab rests on its top, half a block down.

use voxear_engine::{Engine, Game, Project};
use voxear_terrain::blocks::{
    BlockWorld,
    file::BlockFile,
    shape::{BlockFacing, BlockHalf, BlockState},
    usable::state_of,
};

const FIELD: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";

fn project(file: &BlockFile) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Parts\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Blocks/Door.block", "color = [0.5, 0.35, 0.2]\nshape = \"door\"\n".to_string()),
        ("Blocks/Lever.block", "color = [0.4, 0.4, 0.4]\nshape = \"lever\"\n".to_string()),
        ("Blocks/Button.block", "color = [0.4, 0.4, 0.4]\nshape = \"button\"\npress_time = 0.5\n".to_string()),
        ("Blocks/Planks.block", "color = [0.7, 0.5, 0.3]\n".to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn field(game: &Game) -> voxear_ecs::Entity {
    game.engine.world.query_ref::<(voxear_ecs::Entity, &BlockWorld)>().into_iter().next().expect("the world").0
}

/// Puts `block` placed in `state` at `at`, with a door's upper block over it.
fn put(game: &mut Game, at: [i32; 3], block: &str, state: BlockState) {
    use voxear_engine::blocks::{block_number_state, with_door_halves};
    let entity = field(game);
    let number = block_number_state(&mut game.engine.world, &mut game.content, entity, block, state.to_byte(), true).unwrap();
    let changes = with_door_halves(&mut game.engine.world, &mut game.content, entity, vec![(at, number)]);
    game.engine.world.get_mut::<BlockWorld>(entity).unwrap().set_blocks(changes);
}

fn state_at(game: &Game, at: [i32; 3]) -> BlockState {
    let blocks = game.engine.world.get::<BlockWorld>(field(game)).unwrap();
    state_of(&blocks.faces, blocks.blocks.get(at)).unwrap_or_default()
}

#[test]
fn a_lever_and_a_button_drive_the_doors_they_are_linked_to_and_the_links_are_saved() {
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let dir = project(&file);
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let entity = field(&game);
    let door = BlockState::default();
    put(&mut game, [4, 6, 4], "Blocks/Door.block", door);
    put(&mut game, [8, 6, 4], "Blocks/Door.block", door);
    put(&mut game, [12, 6, 4], "Blocks/Lever.block", BlockState::default_for(voxear_terrain::blocks::block::BlockShape::Lever));
    put(&mut game, [14, 6, 4], "Blocks/Button.block", BlockState::default_for(voxear_terrain::blocks::block::BlockShape::Button));
    assert_eq!(state_at(&game, [4, 7, 4]).half, BlockHalf::Upper, "a door has its upper block");
    let shut_triangles = game.engine.world.get::<BlockWorld>(entity).unwrap().meshes[&[0, 0, 0]].positions.clone();
    {
        let blocks = game.engine.world.get_mut::<BlockWorld>(entity).unwrap();
        assert!(blocks.link([12, 6, 4], [4, 6, 4]) && blocks.link([14, 6, 4], [8, 7, 4]));
    }
    // The lever opens the first door, whole, moving by its motion.
    let used = voxear_engine::blocks::use_block(&mut game.engine.world, &mut game.content, entity, [12, 6, 4]).unwrap();
    assert!(used.now && used.changed == 3, "{used:?}");
    assert!(state_at(&game, [4, 6, 4]).open && state_at(&game, [4, 7, 4]).open && state_at(&game, [12, 6, 4]).on);
    let blocks = game.engine.world.get::<BlockWorld>(entity).unwrap();
    assert_eq!(blocks.moving.len(), 3, "the door's blocks and the lever move");
    assert_ne!(blocks.meshes[&[0, 0, 0]].positions, shut_triangles, "physics stands the open door at its side");
    // The button opens the second for half a second, then shuts it again.
    voxear_engine::blocks::use_block(&mut game.engine.world, &mut game.content, entity, [14, 6, 4]).unwrap();
    assert!(state_at(&game, [8, 6, 4]).open && state_at(&game, [14, 6, 4]).on);
    for _ in 0..20 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(state_at(&game, [8, 6, 4]).open, "still pressed");
    assert!(game.engine.world.get::<BlockWorld>(entity).unwrap().moving.iter().all(|moving| moving.at[0] == 8 || moving.at[0] == 14), "the first door has come to rest");
    for _ in 0..20 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(!state_at(&game, [8, 6, 4]).open && !state_at(&game, [14, 6, 4]).on, "the button let go, the door shut");
    // Saved beside the world, and back with it: the open door, and both links.
    voxear_engine::blocks::keep(&game.engine.world, &mut game.content);
    voxear_engine::blocks::save(&mut game.content, dir.path()).unwrap();
    assert!(dir.path().join("Blocks/Field.blocks").is_file());
    let mut again = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    again.start(None).unwrap();
    assert!(state_at(&again, [4, 6, 4]).open && state_at(&again, [4, 7, 4]).open && state_at(&again, [12, 6, 4]).on);
    let links = again.engine.world.get::<BlockWorld>(field(&again)).unwrap().link_list();
    assert_eq!(links, vec![([12, 6, 4], [4, 6, 4]), ([14, 6, 4], [8, 7, 4])]);
}

#[test]
fn a_structure_puts_its_doors_down_turned_as_it_stands() {
    use voxear_terrain::{
        blocks::{
            generate::BlockGenerate,
            structure::{GroundFit, PaletteState, StructureDef, StructureModel},
        },
        generate::{Preset, structures::Placement},
    };
    // A hut 3 by 3 by 3 of planks, hollow, with a door in its north wall.
    let mut blocks = vec![1u16; 27];
    let index = |x: usize, y: usize, z: usize| (y * 3 + z) * 3 + x;
    blocks[index(1, 1, 1)] = 0;
    blocks[index(1, 2, 1)] = 0;
    blocks[index(1, 1, 0)] = 2;
    blocks[index(1, 2, 0)] = 3;
    let palette = vec!["Blocks/Planks.block".into(), "Blocks/Door.block".into(), "Blocks/Door.block".into()];
    let mut model = StructureModel::of([3, 3, 3], palette, &blocks);
    model.states = vec![
        PaletteState { entry: 2, state: BlockState { facing: BlockFacing::North, ..BlockState::default() } },
        PaletteState { entry: 3, state: BlockState { facing: BlockFacing::North, half: BlockHalf::Upper, ..BlockState::default() } },
    ];
    let hut = StructureDef {
        ground: GroundFit::Flatten,
        model: Some(model),
        placement: Placement { spacing: 14.0, separation: 6.0, chance: 1.0, slope: 40.0, ..Placement::default() },
        ..StructureDef::default()
    };
    let mut file = BlockFile::default();
    file.ground.size = [96, 96];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        structures: vec!["Structures/Hut.structure".into()],
        ..BlockGenerate::default()
    });
    let dir = project(&file);
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::create_dir_all(dir.path().join("Structures")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), Preset::RollingHills.world().to_text()).unwrap();
    std::fs::write(dir.path().join("Structures/Hut.structure"), hut.to_text()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = game.engine.world.get::<BlockWorld>(field(&game)).unwrap().clone();
    let doors: Vec<([i32; 3], BlockState)> = (0..96)
        .flat_map(|z| (0..96).flat_map(move |x| (-8..200).map(move |y| [x, y, z])))
        .filter_map(|at| {
            let id = world.blocks.get(at);
            let kind = world.faces.get(usize::from(id))?;
            (kind.shape == voxear_terrain::blocks::block::BlockShape::Door).then(|| (at, BlockState::from_byte(kind.state)))
        })
        .collect();
    let lower: Vec<_> = doors.iter().filter(|(_, state)| state.half == BlockHalf::Lower).collect();
    assert!(lower.len() >= 4, "a door in each hut: {}", lower.len());
    // Each lower block has its upper over it, facing the same way; and the huts stand turned more than one way.
    for (at, state) in &lower {
        let over = doors.iter().find(|(place, _)| *place == [at[0], at[1] + 1, at[2]]);
        assert!(over.is_some_and(|(_, upper)| upper.half == BlockHalf::Upper && upper.facing == state.facing), "{at:?}");
    }
    let mut ways: Vec<BlockFacing> = lower.iter().map(|(_, state)| state.facing).collect();
    ways.sort_by_key(|way| way.index());
    ways.dedup();
    assert!(ways.len() >= 2, "turned as their huts are: {ways:?}");
}

#[test]
fn a_block_of_parts_painted_keeps_its_shape_and_the_way_it_stands() {
    use voxear_engine::blocks::{BlockWorldAsset, block_number_state, painted, resolved};
    use voxear_terrain::{
        blocks::{block::BlockShape, file::BlockKind, generate::BlockGenerate},
        generate::Preset,
    };
    // Generated hills of planks, their top laid in half blocks.
    let mut file = BlockFile::default();
    file.blocks[0] = BlockKind::of("Blocks/Planks.block");
    file.ground.size = [32, 32];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        slabs: true,
        ..BlockGenerate::default()
    });
    let dir = project(&file);
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), Preset::RollingHills.world().to_text()).unwrap();
    std::fs::write(dir.path().join("Blocks/Stone.block"), "color = [0.5, 0.5, 0.5]\n").unwrap();
    std::fs::write(dir.path().join("Blocks/Slab.block"), "color = [0.6, 0.6, 0.6]\nshape = \"slab\"\n").unwrap();
    std::fs::write(dir.path().join("Blocks/Stairs.block"), "color = [0.6, 0.6, 0.6]\nshape = \"stairs\"\n").unwrap();
    std::fs::write(dir.path().join("Blocks/Steps.block"), "color = [0.7, 0.5, 0.3]\nshape = \"stairs\"\n").unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let entity = field(&game);
    // An upright slab facing east and a stair facing west in its upper half, put down by hand.
    let east = BlockState { facing: BlockFacing::East, ..BlockState::default() };
    let upper_west = BlockState { facing: BlockFacing::West, half: BlockHalf::Upper, ..BlockState::default() };
    let world = &mut game.engine.world;
    let upright =
        block_number_state(world, &mut game.content, entity, "Blocks/Slab.block", east.to_byte(), true).unwrap();
    let stair = block_number_state(world, &mut game.content, entity, "Blocks/Stairs.block", upper_west.to_byte(), true)
        .unwrap();
    let asset = game.engine.world.get::<BlockWorldAsset>(entity).unwrap().clone();
    let planks_slab = asset.file.generate.as_ref().unwrap().halves.as_ref().expect("slabs numbered")[1];
    let mut file = asset.file.clone();
    let guid = |game: &mut Game, path: &str| game.content.block(path).unwrap().0;
    let (stone, steps) = (guid(&mut game, "Blocks/Stone.block"), guid(&mut game, "Blocks/Steps.block"));
    let paint = |game: &mut Game, file: &mut BlockFile, with, over| painted(&mut game.content, file, with, over);
    // A generated slab painted with stone is stone's slab, lying low.
    let stone_slab = paint(&mut game, &mut file, stone, planks_slab).expect("stone's slab");
    let drawn = resolved(&mut game.content, &file);
    let kind = drawn.kind(stone_slab).unwrap();
    assert!(kind.slab && kind.shape == BlockShape::Slab && drawn.guid(stone_slab) == Some(stone), "{kind:?}");
    assert_eq!(BlockState::from_byte(kind.state).facing, BlockFacing::Down);
    // Painted again, it is the same number; and a whole block of planks painted is stone itself.
    let held = file.blocks.len();
    assert_eq!(paint(&mut game, &mut file, stone, planks_slab), Some(stone_slab));
    let whole = paint(&mut game, &mut file, stone, 1).expect("stone");
    assert_eq!(file.blocks.len(), held + 1);
    let drawn = resolved(&mut game.content, &file);
    let kind = drawn.kind(whole).unwrap();
    assert!(!kind.slab && kind.shape == BlockShape::Cube, "{kind:?}");
    assert_eq!(drawn.number_of(stone), Some(whole), "stone is placed whole, never as its slab");
    // The upright slab painted with stone is stone's slab standing up facing east, and stays so written and read.
    let stone_upright = paint(&mut game, &mut file, stone, upright).expect("stone's upright slab");
    assert_ne!(stone_upright, stone_slab);
    let read = BlockFile::parse(&file.to_text()).unwrap();
    let drawn = resolved(&mut game.content, &read);
    let kind = drawn.kind(stone_upright).unwrap();
    assert!(kind.slab && BlockState::from_byte(kind.state) == east, "{kind:?}");
    assert_eq!(BlockState::from_byte(drawn.kind(stone_slab).unwrap().state).facing, BlockFacing::Down);
    // The stair painted with other stairs is those, standing as it stood; painted with stone, a cube, it stays a stair.
    let steps_stair = paint(&mut game, &mut file, steps, stair).expect("steps");
    let drawn = resolved(&mut game.content, &file);
    assert_eq!(drawn.guid(steps_stair), Some(steps));
    assert_eq!(BlockState::from_byte(drawn.kind(steps_stair).unwrap().state), upper_west);
    assert_eq!(paint(&mut game, &mut file, stone, stair), None, "stone has no stairs");
}

#[test]
fn an_item_dropped_on_a_slab_rests_on_its_top_half_a_block_down() {
    use voxear_engine::item_drops::{SIZE, drop_item};
    let mut file = BlockFile::default();
    file.ground.size = [16, 16];
    file.ground.height = 6;
    let dir = project(&file);
    std::fs::write(dir.path().join("Blocks/Slab.block"), "color = [0.6, 0.6, 0.6]\nshape = \"slab\"\n").unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let low = BlockState::default_for(voxear_terrain::blocks::block::BlockShape::Slab);
    let high = BlockState { facing: BlockFacing::Up, ..BlockState::default() };
    put(&mut game, [4, 6, 4], "Blocks/Slab.block", low);
    put(&mut game, [8, 6, 4], "Blocks/Slab.block", high);
    put(&mut game, [12, 6, 4], "Blocks/Planks.block", BlockState::default());
    let at = |x: f32| voxear_core::math::Vec3::new(x, 9.0, 4.5);
    let drops: Vec<_> = [4.5, 8.5, 12.5]
        .map(|x| drop_item(&mut game.engine.world, "Items/Stone.item", 1, at(x), voxear_core::math::Vec3::ZERO))
        .into();
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    let rests = |entity| game.engine.world.get::<voxear_scene::Transform>(entity).unwrap().position.y - SIZE * 0.5;
    let [on_low, on_high, on_planks] = [rests(drops[0]), rests(drops[1]), rests(drops[2])];
    assert!((on_low - 6.5).abs() < 1e-3, "on the low slab's top: {on_low}");
    assert!((on_high - 7.0).abs() < 1e-3, "on the high slab's: {on_high}");
    assert!((on_planks - 7.0).abs() < 1e-3, "on a whole block's: {on_planks}");
}
