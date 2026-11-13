//! Trees of blocks in a game: a log mined a slice at a time stands thinner
//! each time, and once it is cut through the tree over it topples away
//! from whoever cut it and breaks where it lands into what its blocks
//! drop, which lie on the ground until taken up. Saplings take root where
//! there is room and light, grow into trees, die without them, and are
//! scattered by a tree that falls.

use voxear_core::math::Vec3;
use voxear_engine::{Engine, Game, Project, item_drops, trees};
use voxear_terrain::blocks::{BlockWorld, block::BlockShape, file::BlockFile, shape::BlockState};

const FIELD: &str =
    "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";

/// The GUID the log's item is given.
const LOG_ITEM: &str = "290c6908e50afa6154bf857d820fba6b";

/// An oak that grows from a sapling in nine seconds, through three stages, and scatters two or three as it falls.
const OAK: &str = "height = [7.0, 8.0]\ntrunk = 1.0\ntaper = 0.5\ncrown_width = [5.0, 6.0]\ncrown_height = [4.0, 5.0]\n\
                   leaf_size = 2.6\nroots = 0\nwood = \"Blocks/Oak Log.block\"\nleaves = \"Blocks/Oak Leaves.block\"\n\
                   sapling = \"Blocks/Oak Sapling.block\"\nsaplings = [2, 3]\ngrow_time = 9.0\n";

fn project() -> tempfile::TempDir {
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Wood\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        (
            "Blocks/Oak Log.block",
            format!(
                "color = [0.5, 0.35, 0.2]\nshape = \"log\"\n\n[[drops]]\nitem = \"{LOG_ITEM}\"\ncount = [1, 1]\nchance = 1.0\n"
            ),
        ),
        ("Blocks/Oak Leaves.block", "color = [0.2, 0.5, 0.2]\nshape = \"bushy\"\nsways = true\n".to_string()),
        ("Blocks/Planks.block", "color = [0.7, 0.5, 0.3]\nshape = \"log\"\nfells = false\n".to_string()),
        ("Items/Oak Log.asset", "type = \"Voxear.Item\"\n\n[fields]\nplaces = \"Blocks/Oak Log.block\"\n".to_string()),
        ("Items/Oak Log.asset.meta", format!("guid = \"{LOG_ITEM}\"\n")),
        (
            "Blocks/Oak Sapling.block",
            "color = [0.3, 0.6, 0.2]\nshape = \"sapling\"\ngrows = \"Plants/Oak.voxplant\"\n".to_string(),
        ),
        ("Plants/Oak.voxplant", OAK.to_string()),
        (
            "Items/Oak Sapling.asset",
            "type = \"Voxear.Item\"\n\n[fields]\nplaces = \"Blocks/Oak Sapling.block\"\n".to_string(),
        ),
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

fn put(game: &mut Game, at: [i32; 3], block: &str, state: BlockState) {
    use voxear_engine::blocks::block_number_state;
    let entity = field(game);
    let number =
        block_number_state(&mut game.engine.world, &mut game.content, entity, block, state.to_byte(), true).unwrap();
    game.engine.world.get_mut::<BlockWorld>(entity).unwrap().set_blocks([(at, number)]);
}

/// A trunk of whole logs from the ground at x, z up to `top`, and a ball of leaves round its top.
fn tree(game: &mut Game, x: i32, z: i32, top: i32) {
    let whole = BlockState::default_for(BlockShape::Log);
    for y in 6..=top {
        put(game, [x, y, z], "Blocks/Oak Log.block", whole);
    }
    for dy in -2i32..=2 {
        for dz in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx * dx + dy * dy + dz * dz <= 5 && (dx, dz) != (0, 0) || dy > 0 && (dx, dz) == (0, 0) {
                    put(game, [x + dx, top + dy, z + dz], "Blocks/Oak Leaves.block", BlockState::default());
                }
            }
        }
    }
}

fn across_at(game: &Game, at: [i32; 3]) -> Option<u8> {
    let blocks = game.engine.world.get::<BlockWorld>(field(game)).unwrap();
    let id = blocks.blocks.get(at);
    let kind =
        game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(field(game)).unwrap().drawn.kind(id)?.clone();
    (id != 0).then(|| BlockState::from_byte(kind.state).across())
}

#[test]
fn a_log_thins_a_slice_a_hit_and_cut_through_its_tree_topples_away_and_breaks_into_logs_on_the_ground() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let entity = field(&game);
    tree(&mut game, 12, 12, 13);
    let foot = [12, 6, 12];
    // Each hit takes a quarter of it off.
    let mut thick = Vec::new();
    for _ in 0..3 {
        let chopped = trees::chop(&mut game.engine.world, &mut game.content, entity, foot, Vec3::X);
        assert_eq!(chopped, trees::Chopped::Thinner);
        thick.push(across_at(&game, foot).unwrap());
    }
    assert_eq!(thick, [12, 8, 4]);
    // The fourth cuts it through: the tree over it comes loose, out of the world.
    let chopped = trees::chop(&mut game.engine.world, &mut game.content, entity, foot, Vec3::X);
    let trees::Chopped::Felled(falling) = chopped else { panic!("{chopped:?}") };
    let blocks = game.engine.world.get::<BlockWorld>(entity).unwrap();
    assert_eq!(blocks.blocks.get(foot), 0);
    assert!((6..=15).all(|y| blocks.blocks.get([12, y, 12]) == 0), "the trunk and the leaves over it are gone");
    assert_eq!(game.engine.world.get::<trees::FallingTree>(falling).unwrap().blocks.len() > 40, true);
    // It topples east, faster as it goes, and lands within two seconds.
    let mut angles = Vec::new();
    let mut felled = Vec::new();
    for _ in 0..180 {
        if let Some(tree) = game.engine.world.get::<trees::FallingTree>(falling) {
            angles.push(tree.angle);
        }
        game.tick(1.0 / 60.0, None);
        felled.extend(trees::take_felled(&mut game.engine.world));
    }
    assert!(angles.windows(3).all(|w| w[2] - w[1] >= w[1] - w[0] - 1e-4 || w[2] == w[1]), "faster as it goes");
    assert!(angles.len() > 30 && angles.len() < 150, "{} frames falling", angles.len());
    assert_eq!(felled.len(), 1, "it broke where it landed");
    let felled = &felled[0];
    assert!(felled.landed.x > 14.0 && (felled.landed.z - 12.5).abs() < 2.0, "east of its stump: {:?}", felled.landed);
    // Its seven logs over the cut each drop a log; its leaves drop nothing.
    assert_eq!(felled.drops.len(), 7, "{felled:?}");
    // They fall onto the ground and rest there, one stack where they lie close.
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    let lying: Vec<(item_drops::ItemDrop, Vec3)> = game
        .engine
        .world
        .query_ref::<(&item_drops::ItemDrop, &voxear_scene::Transform)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .map(|(drop, transform)| (drop.clone(), transform.position))
        .collect();
    assert_eq!(lying.iter().map(|(drop, _)| drop.count).sum::<u32>(), 7);
    assert!(
        lying.iter().all(|(drop, at)| drop.resting && (at.y - 6.125).abs() < 0.01 && drop.item == LOG_ITEM),
        "{lying:?}"
    );
    // Taken up near where they lie.
    let near = item_drops::near(&game.engine.world, felled.landed, 6.0);
    assert!(!near.is_empty());
    let taken: u32 =
        near.iter().filter_map(|&drop| item_drops::take(&mut game.engine.world, drop)).map(|(_, n)| n).sum();
    assert!(taken > 0 && item_drops::take(&mut game.engine.world, near[0]).is_none(), "once each");
}

#[test]
fn logs_with_no_leaves_or_held_another_way_stay_and_a_log_that_does_not_fell_is_only_cut() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let entity = field(&game);
    let whole = BlockState::default_for(BlockShape::Log);
    // A post of logs: no tree, it stays standing.
    for y in 6..10 {
        put(&mut game, [4, y, 4], "Blocks/Oak Log.block", whole);
    }
    for _ in 0..4 {
        trees::chop(&mut game.engine.world, &mut game.content, entity, [4, 6, 4], Vec3::Z);
    }
    let blocks = game.engine.world.get::<BlockWorld>(entity).unwrap();
    assert!(blocks.blocks.get([4, 6, 4]) == 0 && blocks.blocks.get([4, 7, 4]) != 0, "cut, and the rest stays");
    // A tree whose crown also stands on a second trunk does not come down when one is cut.
    tree(&mut game, 20, 20, 12);
    for y in 6..=12 {
        put(&mut game, [21, y, 20], "Blocks/Oak Log.block", whole);
    }
    let mut last = trees::Chopped::No;
    for _ in 0..4 {
        last = trees::chop(&mut game.engine.world, &mut game.content, entity, [20, 6, 20], Vec3::X);
    }
    assert_eq!(last, trees::Chopped::Cut);
    // A block that does not chop breaks as any other.
    put(&mut game, [8, 6, 8], "Blocks/Oak Leaves.block", BlockState::default());
    assert_eq!(trees::chop(&mut game.engine.world, &mut game.content, entity, [8, 6, 8], Vec3::X), trees::Chopped::No);
    // Planks that chop but do not fell: cut, and nothing comes down.
    tree(&mut game, 10, 10, 10);
    put(&mut game, [10, 6, 10], "Blocks/Planks.block", whole);
    let mut last = trees::Chopped::No;
    for _ in 0..4 {
        last = trees::chop(&mut game.engine.world, &mut game.content, entity, [10, 6, 10], Vec3::X);
    }
    assert_eq!(last, trees::Chopped::Cut);
}

fn saplings(game: &Game) -> Vec<voxear_terrain::blocks::sapling::Sapling> {
    game.engine
        .world
        .get::<voxear_terrain::blocks::sapling::Saplings>(field(game))
        .map(|s| s.list.clone())
        .unwrap_or_default()
}

fn ticks(game: &mut Game, seconds: f32) {
    for _ in 0..(seconds * 30.0) as u32 {
        game.tick(1.0 / 30.0, None);
    }
}

#[test]
fn a_sapling_takes_root_in_the_light_grows_into_a_tree_and_dies_without_room_and_a_fallen_tree_scatters_more() {
    use voxear_engine::saplings;
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let entity = field(&game);
    let sapling = "Blocks/Oak Sapling.block";
    // Under a roof it has no light, and in the air nothing to stand on: it takes root in neither.
    for x in 2..5 {
        put(&mut game, [x, 9, 2], "Blocks/Planks.block", BlockState::default());
    }
    assert!(!saplings::plant(&mut game.engine.world, &mut game.content, entity, [3, 6, 2], sapling));
    assert!(!saplings::plant(&mut game.engine.world, &mut game.content, entity, [8, 9, 8], sapling));
    // In the open, on the ground, it does; and one hemmed in by a block two over it, too.
    assert!(saplings::plant(&mut game.engine.world, &mut game.content, entity, [8, 6, 8], sapling));
    assert!(saplings::plant(&mut game.engine.world, &mut game.content, entity, [24, 6, 24], sapling));
    put(&mut game, [24, 8, 24], "Blocks/Planks.block", BlockState::default());
    assert_eq!(saplings(&game).len(), 2);
    let blocks = |game: &Game| game.engine.world.get::<BlockWorld>(field(game)).unwrap().blocks.clone();
    let tall = |game: &Game, x: i32, z: i32| (6..30).filter(|&y| blocks(game).get([x, y, z]) != 0).max().unwrap_or(0);
    // A stage every three seconds: taller each time, a whole tree at the last.
    let mut heights = Vec::new();
    for _ in 0..3 {
        ticks(&mut game, 3.1);
        heights.push(tall(&game, 8, 8));
    }
    assert!(heights.windows(2).all(|pair| pair[1] > pair[0]), "{heights:?}");
    assert!(heights[2] >= 13, "{heights:?}");
    let oak = voxear_terrain::blocks::plant::PlantDef::parse(OAK).unwrap();
    let whole = oak.grow(voxear_terrain::blocks::sapling::seed_of([8, 6, 8]));
    assert_eq!(heights[2], 6 + whole.bounds().unwrap().1[1], "grown into the tree its place's seed grows");
    assert!(across_at(&game, [8, 6, 8]).is_some_and(|across| across == 16), "a whole log at its foot");
    // The one hemmed in waited, then died: gone, and no longer growing.
    ticks(&mut game, 30.0);
    assert_eq!(blocks(&game).get([24, 6, 24]), 0);
    assert!(saplings(&game).is_empty(), "{:?}", saplings(&game));
    // Felled, the grown oak scatters saplings where its crown lands.
    for _ in 0..4 {
        trees::chop(&mut game.engine.world, &mut game.content, entity, [8, 6, 8], Vec3::Z);
    }
    ticks(&mut game, 4.0);
    let scattered = saplings(&game);
    assert!((1..=3).contains(&scattered.len()), "{scattered:?}");
    assert!(scattered.iter().all(|s| s.at[2] > 9 && s.at[1] == 6), "south of the stump, on the ground: {scattered:?}");
    // A sapling item lying on the ground takes root there after a few seconds.
    item_drops::drop_item(&mut game.engine.world, "Items/Oak Sapling.asset", 1, Vec3::new(4.5, 7.0, 20.5), Vec3::ZERO);
    ticks(&mut game, saplings::TAKES_ROOT + 2.0);
    assert!(saplings(&game).iter().any(|s| s.at == [4, 6, 20]), "{:?}", saplings(&game));
    let lying = game
        .engine
        .world
        .query_ref::<&item_drops::ItemDrop>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|drop| drop.item.contains("Sapling"))
        .count();
    assert_eq!(lying, 0, "the item is the sapling now; the felled oak's logs lie still");
}
