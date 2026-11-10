//! A Block World in a scene: its blocks made from its `.blockworld` asset
//! as it loads, with or without a renderer, stood on by physics, and
//! written back as written.

use voxear_engine::{
    Engine, Game, Project,
    scene_file::{self, BlockWorldSource},
};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const FIELD: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                     [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n[[obj]]\nid = 2\nname = \"Ball\"\n\
                     position = [0.0, 20.0, 0.0]\n[obj.collider]\nshape = \"sphere\"\nradius = 0.5\n[obj.rigidbody]\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let files = [
        ("voxear.toml", "[project]\nname = \"Blocks\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_block_world_is_made_from_its_asset_stood_on_and_saved_as_written() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let named = |game: &Game, name: &str| {
        let world = &game.engine.world;
        world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let (field, ball) = (named(&game, "Blocks"), named(&game, "Ball"));
    let blocks = game.engine.world.get::<BlockWorld>(field).expect("its blocks, without a renderer too");
    assert_eq!((blocks.blocks.get([0, 5, 0]), blocks.blocks.get([31, 0, 31]), blocks.blocks.get([0, 6, 0])), (1, 3, 0));
    assert_eq!(blocks.meshes.len(), 1, "one chunk of ground");
    // Dropped from 20 m, the ball comes to rest on the grass, 6 blocks up.
    for _ in 0..180 {
        game.tick(1.0 / 60.0, None);
    }
    let at = game.engine.world.get::<voxear_scene::Transform>(ball).unwrap().position;
    assert!((at.y - 6.5).abs() < 0.05, "on the blocks: {at}");
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[field]);
    let written = saved.objs[0].block_world.as_ref().expect("the world of blocks is saved");
    assert_eq!((written.data.as_str(), written.cast_shadows), ("Blocks/Field.blockworld", true));
    assert!(game.engine.world.get::<BlockWorldSource>(field).is_some());
}

#[test]
fn a_block_world_loads_with_the_changes_kept_beside_it() {
    use voxear_terrain::blocks::edits::BlockEdits;
    let dir = project();
    // A tower of stone on the grass, kept as changes from the ground.
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let base = file.make_ground(None);
    let mut built = base.clone();
    for y in 6..10 {
        built.set([5, y, 5], 3);
    }
    let edits = BlockEdits::between(&base, &built);
    std::fs::write(dir.path().join("Blocks/Field.blocks"), edits.to_bytes()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let blocks = game.engine.world.query_ref::<&BlockWorld>().into_iter().next().expect("the world").clone();
    assert_eq!([5, 6, 9, 10].map(|y| blocks.blocks.get([5, y, 5])), [1, 3, 3, 0]);
}

#[test]
fn a_generated_block_world_follows_its_generator_and_keeps_changes_from_that_ground() {
    use voxear_terrain::{
        blocks::{edits::BlockEdits, generate::BlockGenerate},
        generate::{Preset, WorldGen},
    };
    let dir = project();
    let hills = Preset::RollingHills.world();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), hills.to_text()).unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    let generate = BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        ..BlockGenerate::default()
    };
    file.generate = Some(generate);
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    // A block taken out of the generated ground, kept as a change from it.
    let hills = WorldGen::parse(&hills.to_text()).unwrap();
    let base = file.make_ground(Some(&hills));
    let top = |map: &voxear_terrain::blocks::BlockMap, x: i32, z: i32| {
        (-8..200).rev().find(|&y| map.get([x, y, z]) != 0).unwrap()
    };
    let mut built = base.clone();
    let dug = top(&base, 4, 4);
    built.set([4, dug, 4], 0);
    std::fs::write(dir.path().join("Blocks/Field.blocks"), BlockEdits::between(&base, &built).to_bytes()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let blocks = game.engine.world.query_ref::<&BlockWorld>().into_iter().next().expect("the world").clone();
    for (x, z) in [(1, 2), (20, 30), (31, 0)] {
        let height = hills.height_at((x as f32 + 0.5) * 1.0, (z as f32 + 0.5) * 1.0);
        assert!(((top(&blocks.blocks, x, z) + 1) as f32 - height).abs() <= 1.0, "column ({x}, {z}) at {height} m");
    }
    assert_eq!(top(&blocks.blocks, 4, 4), dug - 1, "the block dug out stays out");
}

#[test]
fn a_block_world_without_end_is_made_round_the_camera_and_saves_every_change() {
    use voxear_terrain::{
        blocks::{
            edits::BlockEdits,
            generate::{BlockEndless, BlockGenerate},
        },
        generate::Preset,
    };
    let dir = project();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), Preset::RollingHills.world().to_text()).unwrap();
    let mut file = BlockFile { block_size: 2.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -4,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 300.0 }),
        ..BlockGenerate::default()
    });
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    // A change kept for far away: stone high over (2000 m, 0).
    let mut far = voxear_terrain::blocks::BlockMap::default();
    far.set([1000, 100, 0], 3);
    std::fs::write(dir.path().join("Blocks/Field.blocks"), BlockEdits::between(&Default::default(), &far).to_bytes())
        .unwrap();
    let scene =
        format!("{FIELD}\n[[obj]]\nid = 3\nname = \"Eye\"\nposition = [40.0, 60.0, 40.0]\n[obj.camera]\nfov = 60.0\n");
    std::fs::write(dir.path().join("Scenes/Field.scene"), scene).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    for _ in 0..3000 {
        game.tick(1.0 / 60.0, None);
        if voxear_engine::blocks::streamed(&game.engine.world)
            && game.engine.world.query_ref::<&BlockWorld>().into_iter().next().is_some_and(|b| !b.far.is_empty())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let entity = game.engine.world.query_ref::<(voxear_ecs::Entity, &BlockWorld)>().into_iter().next().unwrap().0;
    let blocks = game.engine.world.get::<BlockWorld>(entity).unwrap().clone();
    // The ground under the camera (corner at (-16, 0, -16)), coarser chunks beyond.
    assert!((-4..100).any(|y| blocks.blocks.get([28, y, 28]) != 0), "ground under the camera");
    assert!(!blocks.far.is_empty(), "coarser chunks drawn beyond");
    // A block dug near, then saved: the far change kept with it.
    let mut dug = blocks.clone();
    let top = (-4..100).rev().find(|&y| dug.blocks.get([28, y, 28]) != 0).unwrap();
    dug.set_blocks([([28, top, 28], 0)]);
    game.engine.world.insert(entity, dug);
    voxear_engine::blocks::keep(&game.engine.world, &mut game.content);
    let written = voxear_engine::blocks::save(&mut game.content, dir.path()).unwrap();
    assert_eq!(written, vec!["Blocks/Field.blockworld".to_string()]);
    let saved = BlockEdits::from_bytes(&std::fs::read(dir.path().join("Blocks/Field.blocks")).unwrap()).unwrap();
    assert!(saved.chunks.contains_key(&[31, 3, 0]), "the far change kept");
    assert!(saved.chunks.keys().any(|key| key[0] == 0 && key[2] == 0), "the near one with it");
}
