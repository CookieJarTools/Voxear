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
fn a_block_worlds_blocks_are_drawn_as_their_files_say_and_a_block_saved_is_taken_up_at_once() {
    use std::sync::Arc;
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{
        block::{BlockDef, Liquid, MISSING},
        file::BlockKind,
        is_water,
    };
    let dir = project();
    // Grass a block of the project's, its top its own; dirt a block that is not there; stone the world's own kind.
    let grass = BlockDef { top: Some("Materials/Grass.material".into()), ..BlockDef::colored([0.2, 0.6, 0.1]) };
    std::fs::write(dir.path().join("Blocks/Grass.block"), grass.to_text()).unwrap();
    let mut file = BlockFile::default();
    (file.ground.size, file.ground.height) = ([32, 32], 6);
    file.blocks[0] = BlockKind::of("Blocks/Grass.block");
    file.blocks[1] = BlockKind::of("Blocks/Lost.block");
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let field = {
        let named = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        named.into_iter().find(|(_, name)| name.0 == "Blocks").unwrap().0
    };
    let asset = |game: &Game| game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    let blocks = |game: &Game| game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    // Its file is kept as written, naming its blocks; drawn, each has its block's look and its file's name.
    assert_eq!(asset(&game).file, file);
    let drawn = asset(&game).drawn.clone();
    assert_eq!((drawn.blocks[0].name.as_str(), drawn.blocks[0].color), ("Grass", [0.2, 0.6, 0.1]));
    assert_eq!(drawn.blocks[0].top.as_deref(), Some("Materials/Grass.material"));
    assert_eq!(drawn.blocks[1].color, MISSING, "a block that cannot be found shows loudly");
    assert_eq!(drawn.blocks[2].name, "Stone");
    let (looks, faces) = drawn.faces();
    assert_eq!(looks.len(), 4, "grass's top, its sides, the missing block and stone");
    assert_eq!(*blocks(&game).faces, faces[..]);
    assert_eq!([5, 4, 0].map(|y| blocks(&game).blocks.get([3, y, 3])), [1, 2, 3], "numbered as listed");

    // The block's colour changed and saved: the world listing it draws it so from the next tick, its faces as they
    // were, so nothing is meshed again.
    let guid = game.content.parse("Blocks/Grass.block").unwrap().guid().unwrap();
    let meshes = blocks(&game).meshes.clone();
    let paler = BlockDef { color: [0.4, 0.7, 0.3], ..grass.clone() };
    std::fs::write(dir.path().join("Blocks/Grass.block"), paler.to_text()).unwrap();
    game.content.forget_block(guid);
    game.tick(1.0 / 60.0, None);
    assert_eq!(asset(&game).drawn.blocks[0].color, [0.4, 0.7, 0.3]);
    assert_eq!(asset(&game).derived, game.content.blocks_version());
    assert!(Arc::ptr_eq(&meshes, &blocks(&game).meshes), "its faces were as they are");
    assert_eq!(asset(&game).file, file, "its own file is as it was");

    // Made water, its faces are another kind: made again, with nothing to stand on at the top.
    let water = BlockDef { liquid: Liquid::Water, ..BlockDef::colored([0.04, 0.46, 0.33]) };
    std::fs::write(dir.path().join("Blocks/Grass.block"), water.to_text()).unwrap();
    game.content.forget_block(guid);
    game.tick(1.0 / 60.0, None);
    let now = blocks(&game);
    assert!(is_water(&now.faces, 1) && !is_water(&now.faces, 2));
    assert!(!Arc::ptr_eq(&meshes, &now.meshes));
    let highest = now.meshes.values().flat_map(|mesh| mesh.positions.iter().map(|p| p[1])).fold(0.0, f32::max);
    assert_eq!(highest, 5.0, "the ground's top is the dirt under the water");
    // Loaded again, as a level is, the world is drawn as the block is now.
    let again = game.content.block_world(asset(&game).guid).expect("the world");
    assert!(is_water(&again.world.faces, 1));
    assert_eq!(again.derived, game.content.blocks_version());
}

#[test]
fn a_world_holds_any_block_of_the_project_numbers_each_once_and_says_once_that_it_is_full() {
    use voxear_engine::blocks::{BlockWorldAsset, block_number, blocks_numbered_since, every_block};
    let dir = project();
    // More blocks than a world can draw, each a colour of its own.
    std::fs::create_dir_all(dir.path().join("Ores")).unwrap();
    let ore = |number: usize| format!("Ores/Ore {number:03}.block");
    for number in 0..300 {
        let text = format!("color = [{}, {}, 0.5]\n", (number % 20) as f32 / 20.0, (number / 20) as f32 / 20.0);
        std::fs::write(dir.path().join(ore(number)), text).unwrap();
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let field = game.engine.world.query_ref::<(voxear_ecs::Entity, &BlockWorld)>().into_iter().next().unwrap().0;
    let asset = |game: &Game| game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    // Every block of the project is there to hold, in the order of their paths, whatever worlds hold.
    let every = every_block(&game.content);
    assert_eq!(every.len(), 300);
    assert_eq!(every[7], game.content.db().guid_of(&ore(7)).unwrap().to_string());
    // The world as written keeps three kinds of its own, as worlds did before blocks were assets: it loads and is
    // drawn from them as it was, and a game makes no blocks of them.
    assert!(asset(&game).file.blocks.iter().all(|kind| kind.is_inline()));
    assert_eq!((asset(&game).from_file, asset(&game).drawn.looks()), (3, 3));
    let mut number = |game: &mut Game, block: usize, grow: bool| {
        block_number(&mut game.engine.world, &mut game.content, field, &ore(block), grow)
    };
    // A block it has not held has no number until it holds it: then the next, which is its number from then on.
    assert_eq!(number(&mut game, 0, false), None);
    assert_eq!(number(&mut game, 0, true), Some(4));
    assert_eq!((number(&mut game, 0, true), number(&mut game, 0, false)), (Some(4), Some(4)));
    assert_eq!(number(&mut game, 9, true), Some(5));
    // The numbering is kept in its file as it is in the game, the blocks named by their assets.
    let held = asset(&game);
    assert_eq!((held.file.blocks.len(), held.from_file), (5, 3));
    assert_eq!(held.file.blocks[3].block.as_deref(), Some(every[0].as_str()));
    assert_eq!(held.drawn.blocks[4].name, "Ore 009");
    assert_eq!(game.engine.world.get::<BlockWorld>(field).unwrap().faces.len(), 6, "air and five blocks");
    // Its ground is made from what it was made from before: blocks numbered since are none of its ground's.
    assert_eq!(held.drawn.for_ground().blocks.len(), 3);
    // What it numbered since its file was read is said with which block each is, for a save or a joining player.
    let since = blocks_numbered_since(&game.engine.world, field);
    assert_eq!(since, vec![(4, every[0].clone()), (5, every[9].clone())]);
    // It draws 256 looks at most: three of its own and 253 more. Past that a block gets no number, and the Console
    // is told once, plainly.
    let numbered: Vec<Option<u16>> = (0..300).map(|block| number(&mut game, block, true)).collect();
    assert_eq!(numbered.iter().flatten().count(), 253);
    assert!(numbered[253..].iter().all(Option::is_none), "{:?}", &numbered[250..]);
    assert_eq!(asset(&game).drawn.looks(), 256);
    assert_eq!(number(&mut game, 0, true), Some(4), "what it holds keeps its number");
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let full: Vec<&String> = lines.iter().filter(|line| line.contains("is full")).collect();
    assert_eq!(full.len(), 1, "{full:?}");
    assert!(full[0].starts_with("Blocks/Field.blockworld is full: a world draws 256 looks at most."), "{}", full[0]);
    // A game writes no world's file: its three kinds are all it says still.
    let text = std::fs::read_to_string(dir.path().join("Blocks/Field.blockworld")).unwrap();
    assert_eq!(BlockFile::parse(&text).unwrap().blocks.len(), 3);
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

#[test]
fn a_generated_world_drawn_smooth_lies_on_its_generators_heights_and_keeps_a_smooth_hollow() {
    use voxear_terrain::{
        blocks::{BlockMap, chunk::depth_of, edits::BlockEdits, generate::BlockGenerate},
        generate::{Preset, WorldGen},
    };
    let dir = project();
    let hills = Preset::RollingHills.world();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), hills.to_text()).unwrap();
    let mut file = BlockFile { smooth: true, ..BlockFile::default() };
    file.ground.size = [32, 32];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        ..Default::default()
    });
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    // A hollow scooped out of the ground half a block deep at (4, ·, 4), kept as a change from it.
    let hills = WorldGen::parse(&hills.to_text()).unwrap();
    let base = file.make_ground(Some(&hills));
    let top = |map: &BlockMap, x: i32, z: i32| (-8..200).rev().find(|&y| map.get([x, y, z]) != 0).unwrap();
    let mut built = base.clone();
    let scooped = top(&base, 4, 4);
    built.set_cell([4, scooped, 4], 2, 140);
    let edits = BlockEdits::between(&base, &built);
    std::fs::write(dir.path().join("Blocks/Field.blocks"), edits.to_bytes()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let blocks = game.engine.world.query_ref::<&BlockWorld>().into_iter().next().expect("the world").clone();
    assert!(blocks.smooth);
    // Each column's surface, where its top block's fill puts it, within a tenth of a block of its generator's.
    let surface = |map: &BlockMap, x: i32, z: i32| {
        let y = top(map, x, z);
        y as f32 + 0.5 + depth_of(map.fill([x, y, z]))
    };
    for (x, z) in [(1, 2), (20, 30), (31, 0)] {
        let height = hills.height_at(x as f32 + 0.5, z as f32 + 0.5);
        assert!((surface(&blocks.blocks, x, z) - height).abs() < 0.1, "column ({x}, {z}) at {height} m");
    }
    assert_eq!((blocks.blocks.get([4, scooped, 4]), blocks.blocks.fill([4, scooped, 4])), (2, 140), "kept");
    assert!(blocks.meshes.values().all(|mesh| mesh.buckets[1] as usize == mesh.indices.len() * 2), "drawn smooth");
}

#[test]
fn a_block_worlds_colours_stay_loaded_while_its_look_draws_with_them() {
    use voxear_engine::{MaterialData, MaterialSource, scene_file::default_material};
    use voxear_render::{BlockLook, Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(64, 64, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let look = game.engine.world.query_ref::<&BlockLook>().into_iter().next().expect("its look").clone();
    // Changing level lets go of what nothing draws with, as stopping play does: grass is drawn with its colour.
    let grass = MaterialSource::Inline(MaterialData { color: [0.33, 0.52, 0.2], ..default_material() });
    assert_eq!(game.content.loaded_material(&grass), Some(look.surfaces[0].material), "kept as the level loaded");
    game.release_unused(Some(&mut renderer));
    assert_eq!(game.content.loaded_material(&grass), Some(look.surfaces[0].material), "kept after");
}

/// A project as [`project`]'s whose world holds leaves, a block drawn with a material cut out of a picture
/// with holes in it (or, not `cut_out`, drawn whole), and a tree of them on its ground: a trunk of stone three
/// high and a crown five by five by three round its top, seen from the side against the sky.
fn tree_project(cut_out: bool) -> (tempfile::TempDir, Game) {
    use voxear_terrain::blocks::{block::BlockDef, file::BlockKind};
    let dir = project();
    // Leaves: green where the picture is solid, clear in a third of it.
    let mut pixels = Vec::new();
    for y in 0..16u32 {
        for x in 0..16u32 {
            let clear = (x / 2 + y / 2) % 3 == 0;
            pixels.extend([40, 140 + (x * 5) as u8, 50, if clear { 0 } else { 255 }]);
        }
    }
    std::fs::create_dir_all(dir.path().join("Textures")).unwrap();
    let file = std::fs::File::create(dir.path().join("Textures/Leaves.png")).unwrap();
    let mut encoder = png::Encoder::new(file, 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    let leaves = BlockDef { material: Some("Materials/Leaves.material".into()), ..BlockDef::colored([0.2, 0.6, 0.2]) };
    std::fs::write(dir.path().join("Blocks/Leaves.block"), leaves.to_text()).unwrap();
    let mut file = BlockFile::default();
    (file.ground.size, file.ground.height) = ([32, 32], 6);
    file.blocks.push(BlockKind::of("Blocks/Leaves.block"));
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    let scene = format!(
        "{FIELD}\n[[obj]]\nid = 3\nname = \"Eye\"\nposition = [0.0, 8.5, -11.0]\nrotation = [-4.0, 0.0, 0.0]\n\
         [obj.camera]\nfov = 50.0\n\n[[obj]]\nid = 4\nname = \"Sun\"\nrotation = [50.0, 30.0, 0.0]\n\
         [obj.light]\ntype = \"directional\"\n"
    );
    std::fs::write(dir.path().join("Scenes/Field.scene"), scene.replace("position = [0.0, 20.0, 0.0]", "position = [9.0, 20.0, 9.0]"))
        .unwrap();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    // The picture's GUID is its project's to give: the material is written once the project knows it.
    std::fs::write(dir.path().join("Materials/Leaves.material"), "color = [1.0, 1.0, 1.0]\n").unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let picture = game.content.parse("Textures/Leaves.png").unwrap().guid().expect("the picture is an asset");
    let cutout = if cut_out { "cutout = 0.5\n" } else { "" };
    let material = format!("color = [1.0, 1.0, 1.0]\nroughness = 1.0\ncolor_map = \"{picture}\"\n{cutout}");
    std::fs::write(dir.path().join("Materials/Leaves.material"), material).unwrap();
    game.content.rescan().unwrap();
    (dir, game)
}

/// Builds the tree of [`tree_project`] on the world of blocks on `field`: its corner at (-16, 0, -16), so the
/// tree stands over the world's middle.
fn build_tree(game: &mut Game, field: voxear_ecs::Entity) {
    let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let trunk = (6..9).map(|y| ([16, y, 16], 3u16));
    let crown = (8..11).flat_map(|y| (14..19).flat_map(move |z| (14..19).map(move |x| ([x, y, z], 4u16))));
    blocks.set_blocks(crown.chain(trunk));
    game.engine.world.insert(field, blocks);
}

#[test]
fn a_block_drawn_with_a_cut_out_material_is_seen_through_and_hides_no_face() {
    use voxear_engine::blocks::BlockWorldAsset;
    let field_of = |game: &Game| {
        let named = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        named.into_iter().find(|(_, name)| name.0 == "Blocks").unwrap().0
    };
    let quads = |game: &Game, field| {
        let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
        blocks.meshes.values().map(|mesh| mesh.quads.len()).sum::<usize>()
    };
    // Cut out: the world knows its leaves are seen through, from their material, with no renderer too.
    let (_dir, mut game) = tree_project(true);
    game.start(None).unwrap();
    let field = field_of(&game);
    let asset = game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    assert!(asset.drawn.blocks[3].cutout && !asset.drawn.blocks[0].cutout);
    assert!(!asset.file.blocks[3].cutout && !asset.file.to_text().contains("cutout"), "nothing its file says");
    let faces = game.engine.world.get::<BlockWorld>(field).unwrap().faces.clone();
    assert!(faces[4].see_through && !faces[1].see_through && !faces[3].see_through);
    build_tree(&mut game, field);
    let seen_through = quads(&game, field);
    // Drawn whole, the same tree has only its crown's outside.
    let (_dir, mut whole) = tree_project(false);
    whole.start(None).unwrap();
    let field = field_of(&whole);
    assert!(!whole.engine.world.get::<BlockWorld>(field).unwrap().faces[4].see_through);
    build_tree(&mut whole, field);
    assert!(seen_through > quads(&whole, field), "{seen_through} quads with every leaf's faces");
    // The ball still comes to rest on the grass beside the tree, and one dropped on the crown, on its leaves.
    for _ in 0..180 {
        game.tick(1.0 / 60.0, None);
    }
    let ball = {
        let named = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        named.into_iter().find(|(_, name)| name.0 == "Ball").unwrap().0
    };
    let at = game.engine.world.get::<voxear_scene::Transform>(ball).unwrap().position;
    assert!((at.y - 6.5).abs() < 0.05, "on the grass: {at}");
    game.engine.world.get_mut::<voxear_scene::Transform>(ball).unwrap().position = voxear_core::math::Vec3::new(0.5, 20.0, 0.5);
    for _ in 0..240 {
        game.tick(1.0 / 60.0, None);
    }
    let at = game.engine.world.get::<voxear_scene::Transform>(ball).unwrap().position;
    assert!((at.y - 11.5).abs() < 0.05, "on the leaves, which are stood on: {at}");
}

#[test]
fn leaves_cut_out_of_their_picture_show_the_sky_and_the_trunk_through_them() {
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(640, 480, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let mut drawn = |cut_out: bool, name: &str| -> Vec<u8> {
        let (_dir, mut game) = tree_project(cut_out);
        game.start(Some(&mut renderer)).unwrap();
        let field = {
            let named = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
            named.into_iter().find(|(_, name)| name.0 == "Blocks").unwrap().0
        };
        build_tree(&mut game, field);
        for _ in 0..30 {
            game.tick(1.0 / 60.0, Some(&mut renderer));
            renderer.render(&game.engine.world, 1.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        pixels
    };
    let (holes, whole) = (drawn(true, "leaf-blocks.png"), drawn(false, "leaf-blocks-whole.png"));
    // The crown's place on screen, found where the tree drawn whole is green: there, cut out, a good part of the
    // picture is no longer leaves, and none of it where the crown was not.
    let green = |p: &[u8]| p[1] > p[0].saturating_add(30) && p[1] > p[2].saturating_add(30);
    let pairs = || whole.chunks_exact(4).zip(holes.chunks_exact(4));
    let crown = pairs().filter(|(w, _)| green(w)).count();
    let through = pairs().filter(|(w, h)| green(w) && !green(h)).count();
    assert!(crown > 5000, "{crown} pixels of crown");
    let share = through as f32 / crown as f32;
    assert!((0.03..0.6).contains(&share), "{:.0}% of the crown is seen through", share * 100.0);
    // The sun passes the leaves where their picture is clear: the tree's shadow on the grass, a whole block's
    // dark where they are drawn whole, has lit ground in it where they are cut out.
    let light = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let grass = |p: &[u8]| p[1] > p[0] && p[1] > p[2];
    let shaded: Vec<usize> = (0..whole.len() / 4)
        .filter(|at| at / 640 > 300 && grass(&whole[at * 4..at * 4 + 4]) && light(&whole[at * 4..at * 4 + 4]) < 150)
        .collect();
    assert!(shaded.len() > 1500, "{} pixels of the whole tree's shadow", shaded.len());
    let lit = shaded.iter().filter(|&&at| light(&holes[at * 4..at * 4 + 4]) > light(&whole[at * 4..at * 4 + 4]) + 30);
    let share = lit.count() as f32 / shaded.len() as f32;
    assert!(share > 0.1, "{:.0}% of the shadow is lit through the leaves", share * 100.0);
}
