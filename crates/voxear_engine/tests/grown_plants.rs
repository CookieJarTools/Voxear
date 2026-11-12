//! Plants a Block World grows by itself: a Foliage under a world of blocks
//! without end grows its plants on the ground as it is made round the
//! camera, the same every time, and loses those whose ground is dug.

use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, blocks::BlockWorldAsset, grown_plants, scene_file};
use voxear_scene::{EditorOnly, Name, Plant, Plants, Transform};
use voxear_terrain::{
    blocks::{
        BlockWorld,
        block::BlockDef,
        file::{BlockFile, BlockKind},
        generate::{BlockEndless, BlockGenerate},
    },
    generate::{Preset, WorldGen},
};

/// A world of metre blocks without end on rolling hills, grass on top, with flowers on its grass and trees where
/// it is at least `wet` wet, and a camera over (40, 40).
fn project(wet: f32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.blocks[0] = BlockKind::of("Blocks/Grass.block");
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 300.0 }),
        ..BlockGenerate::default()
    });
    let scene = format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
         [[obj]]\nid = 2\nname = \"Flowers\"\nparent = 1\n[obj.foliage]\nmesh = \"builtin:cube\"\n\
         draw_distance = 40.0\n[obj.foliage.grow]\non = [\"Blocks/Grass.block\"]\nspacing = 2.0\n\
         separation = 1.0\nchance = 0.6\n\n\
         [[obj]]\nid = 3\nname = \"Trees\"\nparent = 1\n[obj.foliage]\nmesh = \"builtin:cube\"\n\
         draw_distance = 70.0\n[obj.foliage.trunk]\nradius = 0.3\nheight = 4.0\n[obj.foliage.grow]\n\
         humidity = [{wet}, 1.0]\nspacing = 6.0\nseparation = 3.0\nchance = 0.8\nseed = 1\n\n\
         [[obj]]\nid = 4\nname = \"Eye\"\nposition = [40.0, 60.0, 40.0]\n[obj.camera]\nfov = 60.0\n"
    );
    let files = [
        ("voxear.toml", "[project]\nname = \"Grown\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", scene),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Blocks/Grass.block", BlockDef::colored([0.2, 0.6, 0.1]).to_text()),
        ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// How wet half of the ground round the camera is at least.
fn half_as_wet() -> f32 {
    let world: WorldGen = Preset::RollingHills.world();
    let places = (0..20).flat_map(|z| (0..20).map(move |x| (x as f32 * 8.0 - 40.0, z as f32 * 8.0 - 40.0)));
    let mut wet: Vec<f32> = places.map(|(x, z)| world.climate(x, z, world.height_at(x, z)).1).collect();
    wet.sort_by(f32::total_cmp);
    wet[wet.len() / 2]
}

fn named(game: &Game, name: &str) -> Entity {
    let world = &game.engine.world;
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

/// Ticks until the ground round the camera is made and every plant on it grown.
fn settle(game: &mut Game) {
    // A camera just moved is seen where it was until its place is worked out, a tick on.
    for tick in 0..4000 {
        game.tick(1.0 / 60.0, None);
        let made = voxear_engine::blocks::streamed(&game.engine.world) && grown_plants::settled(&game.engine.world);
        if made && tick >= 3 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the world never settled");
}

fn started(dir: &tempfile::TempDir) -> Game {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game);
    game
}

/// Every plant standing, a line each, in order: what two runs are told apart by.
fn written(plants: &[Plant]) -> Vec<String> {
    let mut lines: Vec<String> =
        plants.iter().map(|plant| format!("{:?} {} {}", plant.position, plant.yaw, plant.scale)).collect();
    lines.sort();
    lines
}

#[test]
fn a_world_without_end_grows_flowers_on_its_grass_and_trees_where_it_is_wet_the_same_every_run() {
    let wet = half_as_wet();
    let dir = project(wet);
    let game = started(&dir);
    let world = &game.engine.world;
    let (field, flowers, trees) = (named(&game, "Blocks"), named(&game, "Flowers"), named(&game, "Trees"));
    let blocks = world.get::<BlockWorld>(field).unwrap().clone();
    let generator = Preset::RollingHills.world();
    // Flowers: on grass, each on the top of its block, none further than they are drawn.
    let grown = grown_plants::standing(world, flowers);
    assert!(grown.len() > 300, "{} flowers", grown.len());
    for flower in &grown {
        let [x, y, z] = flower.position;
        let under = [x.floor() as i32, y as i32 - 1, z.floor() as i32];
        assert_eq!(blocks.blocks.get(under), 1, "a flower on grass at {under:?}");
        assert_eq!(blocks.blocks.get([under[0], under[1] + 1, under[2]]), 0, "and nothing over it");
        // Drawn to 40 m, so grown on every column that comes within 40 m of the camera, over (40, 40).
        assert!((x - 40.0).abs().max((z - 40.0).abs()) <= 40.0 + 32.0, "a flower as far off as {x}, {z}");
    }
    // Trees: only where it is wet enough, and there are places it is not.
    let wood = grown_plants::standing(world, trees);
    assert!(wood.len() > 10, "{} trees", wood.len());
    for tree in &wood {
        let [x, y, z] = tree.position;
        assert!(generator.climate(x, z, y).1 >= wet, "a tree where it is {} wet", generator.climate(x, z, y).1);
    }
    let columns = grown_plants::columns_of(world, trees);
    let all: usize = blocks.columns.as_ref().map_or(0, |columns| columns.len());
    assert!(columns.len() < all, "{} of {all} columns have trees", columns.len());
    // Each column's plants are on an Obj of the engine's own, under the world: no name, in no scene.
    for (_, child) in columns.iter().chain(&grown_plants::columns_of(world, flowers)) {
        assert!(world.has::<EditorOnly>(*child) && !world.has::<Name>(*child));
        assert_eq!(world.get::<voxear_scene::Parent>(*child).map(|parent| parent.0), Some(field));
        assert_eq!(grown_plants::source_of(world, *child).is_some(), true);
    }
    // Trees have their Foliage's trunks, flowers none.
    let trunk = |source: Entity| {
        let (_, child) = grown_plants::columns_of(world, source)[0];
        world.get::<Plants>(child).unwrap().trunk
    };
    assert!(trunk(trees).is_some() && trunk(flowers).is_none());
    // The Foliage itself is saved as written, its rule and none of its plants.
    let mut game = game;
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[flowers]);
    let rule = saved.objs[0].foliage.as_ref().and_then(|foliage| foliage.grow.clone()).expect("its rule");
    assert_eq!((rule.on.len(), rule.spacing, rule.chance), (1, 2.0, 0.6));
    assert!(saved.objs[0].foliage.as_ref().unwrap().plants.is_empty());
    // Another run of the same project: the same plants.
    let again = started(&dir);
    let (flowers_again, trees_again) = (named(&again, "Flowers"), named(&again, "Trees"));
    assert_eq!(written(&grown_plants::standing(&again.engine.world, flowers_again)), written(&grown));
    assert_eq!(written(&grown_plants::standing(&again.engine.world, trees_again)), written(&wood));
}

#[test]
fn plants_come_and_go_with_the_ground_round_the_camera() {
    let dir = project(0.0);
    let mut game = started(&dir);
    let (flowers, eye) = (named(&game, "Flowers"), named(&game, "Eye"));
    let first = written(&grown_plants::standing(&game.engine.world, flowers));
    let first_columns: Vec<[i32; 2]> =
        grown_plants::columns_of(&game.engine.world, flowers).into_iter().map(|(column, _)| column).collect();
    assert!(first_columns.contains(&[1, 1]) && !first_columns.contains(&[9, 1]));
    // The camera 256 m along: the ground there has its flowers, the ground it left has none.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 296.0;
    settle(&mut game);
    let there: Vec<[i32; 2]> =
        grown_plants::columns_of(&game.engine.world, flowers).into_iter().map(|(column, _)| column).collect();
    assert!(there.contains(&[9, 1]) && !there.contains(&[1, 1]), "{there:?}");
    assert!(first_columns.iter().all(|column| !there.contains(column)));
    let holders = game.engine.world.query_ref::<(Entity, &grown_plants::GrownColumn)>().into_iter().count();
    let kept = ["Flowers", "Trees"].map(|name| grown_plants::columns_of(&game.engine.world, named(&game, name)).len());
    assert_eq!(holders, kept[0] + kept[1], "no column's Obj left behind");
    // Back where it was: the flowers it had.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 40.0;
    settle(&mut game);
    assert_eq!(written(&grown_plants::standing(&game.engine.world, flowers)), first);
    // A Foliage that grows no more has none.
    let mut source = game.engine.world.get::<scene_file::FoliageSource>(flowers).unwrap().clone();
    source.0.grow = None;
    game.engine.world.insert(flowers, source);
    game.tick(1.0 / 60.0, None);
    assert!(grown_plants::columns_of(&game.engine.world, flowers).is_empty());
}

#[test]
fn a_plant_goes_when_its_ground_is_dug_or_built_over_and_comes_back_with_it() {
    let dir = project(0.0);
    let mut game = started(&dir);
    let (field, flowers) = (named(&game, "Blocks"), named(&game, "Flowers"));
    let (column, child) = grown_plants::columns_of(&game.engine.world, flowers)[0];
    let plants = game.engine.world.get::<Plants>(child).unwrap().clone();
    let count = plants.standing().count();
    assert!(count > 3, "{count} flowers on column {column:?}");
    let block_under = |number: usize| {
        let [x, y, z] = plants.plants[number].position;
        [x.floor() as i32, y as i32 - 1, z.floor() as i32]
    };
    let set = |game: &mut Game, at: [i32; 3], block: u16| {
        let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
        blocks.set_blocks([(at, block)]);
        game.engine.world.insert(field, blocks);
        game.tick(1.0 / 60.0, None);
    };
    let gone = |game: &Game| game.engine.world.get::<Plants>(child).unwrap().removed.iter().collect::<Vec<u32>>();
    // The ground dug from under the first: gone, the rest as they were, and nothing built again.
    let (first, second) = (block_under(0), block_under(2));
    set(&mut game, first, 0);
    assert_eq!(gone(&game), [0]);
    let now = game.engine.world.get::<Plants>(child).unwrap();
    assert!(std::sync::Arc::ptr_eq(&now.plants, &plants.plants), "the same list, one marked gone");
    // A block built on the third's place: gone too.
    set(&mut game, [second[0], second[1] + 1, second[2]], 3);
    assert_eq!(gone(&game), [0, 2]);
    // The grass put back and the stone taken off: both back.
    set(&mut game, first, 1);
    set(&mut game, [second[0], second[1] + 1, second[2]], 0);
    assert!(gone(&game).is_empty());
    // Dug, then the ground let go and made again far off and back: still gone, as the change is kept.
    set(&mut game, first, 0);
    let eye = named(&game, "Eye");
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 400.0;
    settle(&mut game);
    assert!(game.engine.world.get::<BlockWorldAsset>(field).is_some_and(|asset| !asset.pending.chunks.is_empty()));
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 40.0;
    settle(&mut game);
    let (_, child) =
        grown_plants::columns_of(&game.engine.world, flowers).into_iter().find(|(at, _)| *at == column).unwrap();
    let back = game.engine.world.get::<Plants>(child).unwrap();
    assert_eq!(back.removed.iter().collect::<Vec<u32>>(), [0]);
    assert_eq!(back.plants, plants.plants, "the same plants by the same numbers");
}

#[test]
fn trees_stand_on_ground_past_the_worlds_own_blocks_and_are_the_same_trees_once_it_is_made() {
    use std::collections::HashSet;
    let dir = project(0.0);
    // Trees drawn to 200 m in a world whose own blocks reach 70 m, its coarser rings 300 m.
    let scene = dir.path().join("Scenes/Field.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    assert!(text.contains("draw_distance = 70.0"));
    std::fs::write(&scene, text.replace("draw_distance = 70.0", "draw_distance = 200.0")).unwrap();
    let mut game = started(&dir);
    let (field, trees, flowers) = (named(&game, "Blocks"), named(&game, "Trees"), named(&game, "Flowers"));
    let made = |game: &Game| -> HashSet<[i32; 2]> {
        let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
        blocks.columns.as_ref().map(|columns| (**columns).clone()).unwrap_or_default()
    };
    let plants_on = |game: &Game, source: Entity, column: [i32; 2]| -> Vec<Plant> {
        let columns = grown_plants::columns_of(&game.engine.world, source);
        let held = columns.into_iter().find(|(at, _)| *at == column).map(|(_, child)| child);
        held.map_or(Vec::new(), |child| game.engine.world.get::<Plants>(child).unwrap().plants.to_vec())
    };
    // Trees on columns the world has no blocks of its own for, as far as they are drawn; flowers, drawn to 40 m,
    // only on its own.
    let columns: Vec<[i32; 2]> =
        grown_plants::columns_of(&game.engine.world, trees).into_iter().map(|(column, _)| column).collect();
    let near = made(&game);
    let far: Vec<[i32; 2]> = columns.iter().copied().filter(|column| !near.contains(column)).collect();
    assert!(far.len() > 20 && columns.len() > far.len(), "{} of {} columns are far", far.len(), columns.len());
    for column in &far {
        let (dx, dz) = ((column[0] * 32 + 16 - 40).abs(), (column[1] * 32 + 16 - 40).abs());
        assert!(dx.max(dz) as f32 <= 200.0 + 48.0, "trees as far off as column {column:?}");
    }
    let flowered = grown_plants::columns_of(&game.engine.world, flowers);
    assert!(flowered.iter().all(|(column, _)| near.contains(column)));
    // The far column with the most trees: the camera goes to it, its blocks are made, and its trees are the
    // trees it had, each where it stood, no lower than it stood on the coarser ground.
    let column = *far.iter().max_by_key(|column| plants_on(&game, trees, **column).len()).unwrap();
    let before = plants_on(&game, trees, column);
    assert!(before.len() > 3, "{} trees on far column {column:?}", before.len());
    let eye = named(&game, "Eye");
    let over = [(column[0] * 32 + 16) as f32, (column[1] * 32 + 16) as f32];
    let position = &mut game.engine.world.get_mut::<Transform>(eye).unwrap().position;
    (position.x, position.z) = (over[0], over[1]);
    settle(&mut game);
    assert!(made(&game).contains(&column), "its own blocks are made now");
    let after = plants_on(&game, trees, column);
    assert_eq!(after.len(), before.len());
    let blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    for (far_off, made) in before.iter().zip(&after) {
        let ((far_at, at), same) =
            ((far_off.position, made.position), (far_off.yaw, far_off.scale) == (made.yaw, made.scale));
        assert!(same && [far_at[0], far_at[2]] == [at[0], at[2]], "{far_at:?} became {at:?}");
        assert!(far_at[1] <= at[1] && far_at[1] >= at[1] - 6.0, "{} then {}", far_at[1], at[1]);
        let under = [at[0].floor() as i32, at[1] as i32 - 1, at[2].floor() as i32];
        assert!(blocks.blocks.get(under) != 0 && blocks.blocks.get([under[0], under[1] + 1, under[2]]) == 0);
    }
}

#[test]
fn a_plant_a_script_takes_away_stays_gone_when_its_ground_is_made_again() {
    use voxear_scene::CutPlants;
    let dir = project(0.0);
    let mut game = started(&dir);
    let (field, trees) = (named(&game, "Blocks"), named(&game, "Trees"));
    let (column, child) = grown_plants::columns_of(&game.engine.world, trees)[0];
    let plants = game.engine.world.get::<Plants>(child).unwrap().clone();
    assert!(plants.plants.len() > 2 && plants.removed.is_empty());
    // Taken away as a script's `Plant.Remove` does: gone, and kept on the Foliage's Obj by column and number.
    assert!(game.engine.world.get_mut::<Plants>(child).unwrap().remove(1));
    game.tick(1.0 / 60.0, None);
    let cut = game.engine.world.get::<CutPlants>(trees).cloned().expect("kept on the Foliage");
    assert_eq!(cut.of(column).map(|gone| gone.iter().collect::<Vec<u32>>()), Some(vec![1]));
    // The ground dug from under another: gone too, but by its ground, which is not a cut.
    let [x, y, z] = plants.plants[0].position;
    let under = [x.floor() as i32, y as i32 - 1, z.floor() as i32];
    let set = |game: &mut Game, block: u16| {
        let mut blocks = game.engine.world.get::<BlockWorld>(field).unwrap().clone();
        blocks.set_blocks([(under, block)]);
        game.engine.world.insert(field, blocks);
        game.tick(1.0 / 60.0, None);
    };
    let was = game.engine.world.get::<BlockWorld>(field).unwrap().blocks.get(under);
    set(&mut game, 0);
    let gone = |game: &Game, child: Entity| {
        game.engine.world.get::<Plants>(child).unwrap().removed.iter().collect::<Vec<u32>>()
    };
    assert_eq!(gone(&game, child), [0, 1]);
    assert_eq!(game.engine.world.get::<CutPlants>(trees), Some(&cut), "only the cut one is kept");
    // Its ground put back, the dug one is back and the cut one is not.
    set(&mut game, was);
    assert_eq!(gone(&game, child), [1]);
    // The camera away and back, the column's plants made afresh: still cut.
    let eye = named(&game, "Eye");
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 500.0;
    settle(&mut game);
    assert!(!grown_plants::columns_of(&game.engine.world, trees).iter().any(|(at, _)| *at == column));
    game.engine.world.get_mut::<Transform>(eye).unwrap().position.x = 40.0;
    settle(&mut game);
    let (_, again) =
        grown_plants::columns_of(&game.engine.world, trees).into_iter().find(|(at, _)| *at == column).unwrap();
    assert_ne!(again, child, "an Obj made afresh");
    assert_eq!(gone(&game, again), [1]);
    assert_eq!(
        grown_plants::standing(&game.engine.world, trees).iter().filter(|plant| **plant == plants.plants[1]).count(),
        0
    );
    // Put on a Foliage as a save's are, before its columns are made: gone as they are made.
    let mut other = CutPlants::default();
    other.take(column, &[0u32, 2].into_iter().collect());
    game.engine.world.insert(trees, other);
    game.tick(1.0 / 60.0, None);
    assert_eq!(gone(&game, again), [0, 2], "what the Foliage's Obj says, as after a save is put back");
}

#[test]
fn a_wood_shows_in_the_far_view_as_its_far_block_past_where_its_trees_are_drawn() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(64, 64, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    // The far view's faces drawn as leaves, in a world whose trees, six metres across, show far off as leaves
    // (`far`) or do not.
    let mut leaves_far_off = |far: bool| -> (usize, bool) {
        let dir = project(0.0);
        let leaves = BlockDef::colored([0.1, 0.35, 0.12]);
        std::fs::write(dir.path().join("Blocks/Leaves.block"), leaves.to_text()).unwrap();
        // Coarser rings out to 1500 m, so the far view's begin, 400 m off.
        let world = dir.path().join("Blocks/Field.blockworld");
        let text = std::fs::read_to_string(&world).unwrap();
        assert!(text.contains("far = 300.0"));
        std::fs::write(&world, text.replace("far = 300.0", "far = 1500.0")).unwrap();
        let scene = dir.path().join("Scenes/Field.scene");
        let text = std::fs::read_to_string(&scene).unwrap();
        let far_block = if far { "far_block = \"Blocks/Leaves.block\"\n" } else { "" };
        let trees = format!("chance = 0.8\nseed = 1\nscale = [6.0, 6.0]\n{far_block}");
        assert!(text.contains("chance = 0.8\nseed = 1\n"));
        std::fs::write(&scene, text.replace("chance = 0.8\nseed = 1\n", &trees)).unwrap();
        let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
        game.start(Some(&mut renderer)).unwrap();
        for tick in 0..6000 {
            game.tick(1.0 / 60.0, Some(&mut renderer));
            let made = voxear_engine::blocks::streamed(&game.engine.world) && grown_plants::settled(&game.engine.world);
            if made && tick >= 30 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let field = named(&game, "Blocks");
        let asset = game.engine.world.get::<BlockWorldAsset>(field).unwrap();
        let guid = game.content.parse("Blocks/Leaves.block").unwrap().guid().unwrap();
        let Some(number) = asset.drawn.number_of(guid) else { return (0, false) };
        let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
        let look = blocks.faces[usize::from(number)].top;
        let far_view = blocks.far.iter().filter(|(key, _)| key[0] >= 3);
        let leafy = |mesh: &voxear_terrain::blocks::mesh::ChunkMesh| {
            mesh.quads.iter().filter(|quad| voxear_terrain::blocks::mesh::unpack(**quad).3 == look).count()
        };
        (far_view.map(|(_, mesh)| leafy(mesh)).sum(), true)
    };
    let (leafy, held) = leaves_far_off(true);
    assert!(held, "the world numbers its Foliage's far block");
    assert!(leafy > 50, "{leafy} faces of leaves in the far view");
    assert_eq!(leaves_far_off(false), (0, false), "none where no Foliage shows far off");
}

/// A project as [`project`]'s, its world growing every starter plant, each a model in its Plants folder, under a
/// low sun, seen from a little over the ground at (40, 40); with `plants` off, the same world bare.
fn starter_project(plants: bool) -> tempfile::TempDir {
    use voxear_render::StarterPlant;
    let dir = project(0.0);
    let ground = Preset::RollingHills.world().height_at(40.0, 40.0);
    let mut scene = format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
         [[obj]]\nid = 2\nname = \"Eye\"\nposition = [40.0, {}, 40.0]\nrotation = [22.0, 35.0, 0.0]\n\
         [obj.camera]\nfov = 60.0\n\n\
         [[obj]]\nid = 3\nname = \"Sun\"\nrotation = [40.0, -60.0, 0.0]\n[obj.light]\ntype = \"directional\"\n",
        ground + 7.0
    );
    std::fs::create_dir_all(dir.path().join("Plants")).unwrap();
    for (at, plant) in StarterPlant::ALL.into_iter().enumerate().filter(|_| plants) {
        let path = format!("Plants/{}.boxmodel", plant.name());
        std::fs::write(dir.path().join(&path), plant.model().to_text()).unwrap();
        let (mesh, material) = (format!("{path}#mesh0.0"), format!("{path}#material0"));
        let on = (Some("Blocks/Grass.block"), None);
        let foliage = grown_plants::foliage_text(&mesh, Some(&material), Some(plant), on);
        scene += &format!("\n[[obj]]\nid = {}\nname = \"{}\"\nparent = 1\n{foliage}", at + 10, plant.name());
    }
    std::fs::write(dir.path().join("Scenes/Field.scene"), scene).unwrap();
    dir
}

/// The world of [`starter_project`] drawn once its ground and plants are made: its pixels, and how many plants
/// of each kind stand.
fn drawn(renderer: &mut voxear_render::Renderer, plants: bool, name: &str) -> (Vec<u8>, Vec<(String, usize)>) {
    let dir = starter_project(plants);
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(renderer)).unwrap();
    for tick in 0..4000 {
        game.tick(1.0 / 60.0, Some(renderer));
        let made = voxear_engine::blocks::streamed(&game.engine.world) && grown_plants::settled(&game.engine.world);
        if made && tick >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for _ in 0..40 {
        game.tick(1.0 / 60.0, Some(renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
    }
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let world = &game.engine.world;
    let sources = world.query_ref::<(Entity, &Name, &scene_file::FoliageSource)>().into_iter();
    let counts =
        sources.map(|(entity, name, _)| (name.0.clone(), grown_plants::standing(world, entity).len())).collect();
    (pixels, counts)
}

#[test]
fn a_world_draws_the_starter_plants_it_grows() {
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(1280, 720, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let (bare, none) = drawn(&mut renderer, false, "grown-plants-bare.png");
    assert!(none.is_empty());
    let (grown, counts) = drawn(&mut renderer, true, "grown-plants.png");
    let count = |name: &str| counts.iter().find(|(kind, _)| kind == name).map_or(0, |(_, count)| *count);
    // Grass thick, flowers in patches, bushes and rocks here and there; each drawn from its own model.
    assert!(count("Grass Tuft") > 1500, "{counts:?}");
    assert!(count("Flower") > 20 && count("Flower") < count("Grass Tuft") / 4, "{counts:?}");
    assert!(count("Bush") > 5 && count("Rock") > 5, "{counts:?}");
    // The ground looks another way with them on it: a fifth of the picture or more is not the bare world's.
    let other = bare
        .chunks_exact(4)
        .zip(grown.chunks_exact(4))
        .filter(|(a, b)| (0..3).any(|part| (i32::from(a[part]) - i32::from(b[part])).abs() > 12));
    let share = other.count() as f32 / (bare.len() / 4) as f32;
    assert!(share > 0.2, "{:.0}% of the picture changed", share * 100.0);
}
