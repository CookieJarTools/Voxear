//! Dimensions in the game, headless (no GPU, no scripts): a second world of
//! its own, made from a `.dimension` file, at the same coordinates as the
//! level's; each world without end is made round the places in its own
//! dimension alone; a walker moved into the other stands on its ground,
//! under its own sky, and rays meet only its blocks; and letting it go takes
//! its Objs with it.

use std::sync::{Arc, Mutex};

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    dimensions::{self, DimensionId, InDimension},
    physics::{CharacterController, PhysicsScene},
};
use voxear_render::{Camera, Sky};
use voxear_scene::{Parent, Transform};
use voxear_terrain::{
    blocks::{
        BlockWorld,
        file::BlockFile,
        generate::{BlockEndless, BlockGenerate},
    },
    generate::Preset,
};

const FRAME: f32 = 1.0 / 60.0;
/// How large the worlds' blocks are, in metres.
const SIZE: f32 = 2.0;

fn write(dir: &std::path::Path, files: &[(&str, String)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// A world of blocks without end, made by the generator at `world`.
fn endless(world: &str) -> String {
    let mut file = BlockFile { block_size: SIZE, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some(world.into()),
        bottom: -4,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    file.to_text()
}

/// A level whose world without end is rolling hills, and a dimension of
/// caves whose own is canyons, both with their corner at the same place;
/// the game started, with a walker in the level held high over the middle of a column,
/// its camera at its eye.
fn game() -> (tempfile::TempDir, Game, Entity, Arc<Mutex<f32>>) {
    let dir = tempfile::tempdir().unwrap();
    let ground = |name: &str| {
        format!(
            "[[obj]]\nid = 1\nname = \"Ground\"\nposition = [-16.0, 0.0, -16.0]\n\
             [obj.block_world]\ndata = \"Blocks/{name}.blockworld\"\n"
        )
    };
    let sky = "[sky]\nzenith = [0.2, 0.4, 0.8]\nhorizon = [0.6, 0.7, 0.8]\nground = [0.2, 0.2, 0.2]\nhaze = 2.0\n";
    let scene = format!("format = 1\n{sky}\n{}", ground("Hills"));
    let caves = format!("format = 1\n\n[dimension]\nkind = \"caves\"\n\n{}", ground("Canyons"));
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Dimensions\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", scene),
            ("Dimensions/Caves.dimension", caves),
            ("Blocks/Hills.blockworld", endless("Worlds/Hills.worldgen")),
            ("Blocks/Canyons.blockworld", endless("Worlds/Canyons.worldgen")),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
            ("Worlds/Canyons.worldgen", Preset::Canyons.world().to_text()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &mut game.engine.world;
    let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(9.0, 300.0, 9.0))));
    world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    // How fast it falls, while it is let fall.
    let falling = Arc::new(Mutex::new(0.0f32));
    let fall = falling.clone();
    game.engine.schedule.add_system("Update", "fall", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let speed = *fall.lock().unwrap();
        if speed != 0.0 {
            physics.move_character(world, walker, -Vec3::Y * speed, FRAME);
        }
        world.insert_resource(physics);
    });
    (dir, game, walker, falling)
}

/// Ticks until the worlds of the dimension seen have made their blocks round
/// where it is seen from: a few first, as a camera moved is seen where it
/// went once its transforms are worked out, at the end of a tick.
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

/// The world of blocks in `dimension`.
fn ground_of(game: &Game, dimension: DimensionId) -> Entity {
    let world = &game.engine.world;
    let found = world.query_ref::<(Entity, &BlockWorld)>().into_iter().map(|(entity, _)| entity);
    found.into_iter().find(|&entity| dimensions::dimension_of(world, entity) == dimension).expect("a world there")
}

/// The top of the highest block under a place of a world, where its blocks
/// are made there.
fn top(game: &Game, ground: Entity, x: f32, z: f32) -> Option<f32> {
    let world = &game.engine.world;
    let corner = voxear_scene::world_matrix(world, ground).w_axis.truncate();
    let held = world.get::<BlockWorld>(ground)?;
    let column = [((x - corner.x) / SIZE).floor() as i32, ((z - corner.z) / SIZE).floor() as i32];
    let highest = (-8..200).rev().find(|&y| held.blocks.get([column[0], y, column[1]]) != 0)?;
    Some(corner.y + (highest + 1) as f32 * SIZE)
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    voxear_scene::world_matrix(&game.engine.world, entity).w_axis.truncate()
}

/// Lets the walker fall onto whatever is under it, from 2 m over `ground`,
/// and says where it came to stand.
fn stand(game: &mut Game, walker: Entity, falling: &Mutex<f32>, ground: f32) -> Vec3 {
    game.engine.world.get_mut::<Transform>(walker).unwrap().position.y = ground + 2.0;
    *falling.lock().unwrap() = 6.0;
    for _ in 0..90 {
        game.tick(FRAME, None);
    }
    *falling.lock().unwrap() = 0.0;
    at(game, walker)
}

#[test]
fn two_worlds_without_end_at_one_place_are_each_made_round_their_own_dimension_and_a_walker_stands_on_its_own() {
    let (_dir, mut game, walker, falling) = game();
    settle(&mut game);
    let hills = ground_of(&game, DimensionId::LEVEL);
    let under = top(&game, hills, 9.0, 9.0).expect("the level's ground made round the walker");
    let stood = stand(&mut game, walker, &falling, under);
    assert!((stood.y - under).abs() < 0.1, "on the hills: {stood:?}, ground at {under}");
    let level_sky = game.engine.world.resource::<Sky>().copied().unwrap();
    assert_eq!(level_sky.haze, 2.0, "the level's own sky");

    // The caves, made: their world is there, in their dimension, but nothing of it is made yet, as nobody is in it.
    let guid = game.content.db().guid_of("Dimensions/Caves.dimension").unwrap();
    let caves = game.make_dimension(guid, 0, None).unwrap();
    assert_ne!(caves, DimensionId::LEVEL);
    assert_eq!(game.make_dimension(guid, 0, None).unwrap(), caves, "made once");
    let canyons = ground_of(&game, caves);
    assert_eq!(game.engine.world.get::<InDimension>(canyons), Some(&InDimension(caves)));
    for _ in 0..30 {
        game.tick(FRAME, None);
    }
    assert!(top(&game, canyons, 9.0, 9.0).is_none(), "no ground made in a dimension nobody is in");
    assert_eq!(game.engine.world.resource::<Sky>().map(|sky| sky.haze), Some(2.0), "still the level's sky");

    // The walker moved into them: their ground is made round it, under their own dark, and it stands on it.
    assert!(dimensions::move_obj(&mut game.engine.world, walker, caves));
    settle(&mut game);
    assert_eq!(dimensions::current(&game.engine.world), caves, "the camera went with the walker");
    let sky = game.engine.world.resource::<Sky>().copied().unwrap();
    assert!(sky.ambient == 0.0 && sky.zenith == Vec3::ZERO, "caves have no sky: {sky:?}");
    let floor = top(&game, canyons, 9.0, 9.0).expect("the caves' ground made round the walker");
    assert!((floor - under).abs() > 4.0, "the canyons and hills differ there: {floor} and {under}");
    let stood = stand(&mut game, walker, &falling, floor);
    assert!((stood.y - floor).abs() < 0.1, "on the canyons, not the hills at {under}: {stood:?}, ground at {floor}");
    // A ray beside it asks in the dimension the game is seen from, and meets the canyons alone.
    let physics = game.engine.physics().unwrap();
    let hit = physics.raycast(Vec3::new(21.0, 300.0, 9.0), Vec3::NEG_Y, 400.0, u32::MAX, false).unwrap();
    assert_eq!(hit.obj, canyons);
    let met = top(&game, canyons, 21.0, 9.0).unwrap();
    assert!((hit.point.y - met).abs() < 0.01, "at the canyons' top there, {met}: {:?}", hit.point);

    // Far along in the caves, their ground is made there and the level's is not.
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(401.0, 300.0, 9.0);
    settle(&mut game);
    assert!(top(&game, canyons, 401.0, 9.0).is_some(), "the caves' ground follows the walker");
    assert!(top(&game, hills, 401.0, 9.0).is_none(), "the level's world is left as it was");

    // Back in the level: its ground is made there now, under its own sky again.
    assert!(dimensions::move_obj(&mut game.engine.world, walker, DimensionId::LEVEL));
    settle(&mut game);
    assert!(top(&game, hills, 401.0, 9.0).is_some(), "the level's ground follows the walker back");
    let sky = game.engine.world.resource::<Sky>().copied().unwrap();
    assert_eq!(
        (sky.haze, sky.zenith, sky.ambient),
        (level_sky.haze, level_sky.zenith, level_sky.ambient),
        "the level's sky"
    );
}

#[test]
fn letting_a_dimension_go_takes_its_objs_and_puts_back_the_levels_sky() {
    let (_dir, mut game, walker, _) = game();
    let guid = game.content.db().guid_of("Dimensions/Caves.dimension").unwrap();
    let caves = game.make_dimension(guid, 0, None).unwrap();
    assert!(dimensions::move_obj(&mut game.engine.world, walker, caves));
    game.tick(FRAME, None);
    assert_eq!(dimensions::current(&game.engine.world), caves);
    let canyons = ground_of(&game, caves);
    // Letting them go takes every Obj in them but persistent ones, as the walker is: it comes back to the level's.
    game.engine.world.insert(walker, voxear_scene::ScenePersistence);
    assert!(game.let_go_dimension(caves));
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    assert!(!game.engine.world.is_alive(canyons), "the caves' world went with them");
    assert!(game.engine.world.is_alive(walker), "a persistent walker stays");
    assert_eq!(dimensions::dimension_of(&game.engine.world, walker), DimensionId::LEVEL, "in the level's");
    assert_eq!(dimensions::current(&game.engine.world), DimensionId::LEVEL);
    assert_eq!(game.engine.world.resource::<Sky>().map(|sky| sky.haze), Some(2.0));
    assert!(!game.let_go_dimension(caves), "gone already");
}

/// A flat field of blocks 64 a side and `height` high, a block a metre.
fn field(height: u32) -> String {
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.ground.height = height;
    file.to_text()
}

/// A level of a field 4 high with a Gate in its middle facing -z, which
/// leads into the Way Back of a dimension of caves whose rock is 12 high,
/// facing +z: walking +z through the Gate, one comes out of the Way Back
/// still walking +z, 8 m higher. The caves are kept a second once unneeded.
/// A second Gate, 40 m along x, leads into a second dimension of its own.
fn gates() -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let level = "format = 1\n\n[[obj]]\nid = 1\nname = \"Field\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Gate\"\nposition = [32.0, 4.0, 20.0]\nrotation = [0.0, 180.0, 0.0]\n\
                 [obj.portal]\nsize = [3.0, 4.0]\ninto = { dimension = \"Dimensions/Caves.dimension\", portal = 2 }\n\n\
                 [[obj]]\nid = 3\nname = \"Second Gate\"\nposition = [72.0, 4.0, 20.0]\nrotation = [0.0, 180.0, 0.0]\n\
                 [obj.portal]\nsize = [3.0, 4.0]\ninto = { dimension = \"Dimensions/Deep.dimension\", portal = 2 }\n\n\
                 [[obj]]\nid = 4\nname = \"Far Field\"\nposition = [64.0, 0.0, 0.0]\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let caves = "format = 1\n\n[dimension]\nkind = \"caves\"\nkeep_loaded = 1.0\n\n\
                 [[obj]]\nid = 1\nname = \"Rock\"\n[obj.block_world]\ndata = \"Blocks/Rock.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Way Back\"\nposition = [32.0, 12.0, 32.0]\n[obj.portal]\nsize = [3.0, 4.0]\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Gates\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", level.into()),
            ("Dimensions/Caves.dimension", caves.into()),
            ("Dimensions/Deep.dimension", caves.replace("Way Back", "Deep Way Back").into()),
            ("Blocks/Field.blockworld", field(4)),
            ("Blocks/Rock.blockworld", field(12)),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (dir, game)
}

/// The Obj named `name`.
fn named(game: &Game, name: &str) -> Entity {
    let world = &game.engine.world;
    let found = world.query_ref::<(Entity, &voxear_scene::Name)>().into_iter();
    found.into_iter().find(|(_, own)| own.0 == name).map(|(entity, _)| entity).unwrap_or_else(|| panic!("no {name}"))
}

/// A walker on the field 10 m in front of the Gate, walking at it at
/// `speed`, its eye under it.
fn walker_at_the_gate(game: &mut Game, speed: f32) -> (Entity, Entity) {
    let world = &mut game.engine.world;
    let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(32.0, 4.1, 10.0))));
    let eye = world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    game.engine.schedule.add_system("Update", "walk", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let ahead = world.get::<Transform>(walker).map_or(Vec3::Z, |transform| transform.rotation * Vec3::Z);
        physics.move_character(world, walker, ahead * speed - Vec3::Y * 2.0, FRAME);
        world.insert_resource(physics);
    });
    (walker, eye)
}

#[test]
fn a_portal_into_a_dimension_makes_it_as_a_walker_comes_near_and_takes_the_walker_into_it() {
    let (_dir, mut game) = gates();
    let (walker, _) = walker_at_the_gate(&mut game, 3.0);
    let (gate, field) = (named(&game, "Gate"), named(&game, "Field"));
    assert!(game.engine.world.resource::<dimensions::Dimensions>().is_none(), "no caves before anyone comes near");
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    // Near enough to see through the Gate: the caves are made, and the Gate and their Way Back lead to each other.
    let back = named(&game, "Way Back");
    let caves = dimensions::dimension_of(&game.engine.world, back);
    assert_ne!(caves, DimensionId::LEVEL);
    let to = |portal: Entity| game.engine.world.get::<voxear_render::Portal>(portal).unwrap().to;
    assert_eq!((to(gate), to(back)), (Some(back), Some(gate)));
    let far = *game.engine.world.get::<voxear_render::PortalFar>(gate).unwrap();
    assert!(far.ready, "the caves' ground is all made at once, so the Gate is open: {far:?}");

    // It walks through and stands on the caves' rock, in the caves, under their dark.
    let mut passed = Vec::new();
    for _ in 0..300 {
        game.tick(FRAME, None);
        let events = game.engine.world.resource::<voxear_engine::portals::PortalEvents>();
        passed.extend(events.map(|events| events.passed.clone()).unwrap_or_default());
    }
    assert_eq!(passed.len(), 1, "{passed:?}");
    assert_eq!((passed[0].from, passed[0].to), (gate, back));
    let stood = at(&game, walker);
    assert!(stood.z > 33.0 && (stood.y - 12.0).abs() < 0.1, "on the rock past the Way Back: {stood:?}");
    assert_eq!(dimensions::dimension_of(&game.engine.world, walker), caves);
    assert_eq!(dimensions::current(&game.engine.world), caves, "its eye went with it");
    assert_eq!(game.engine.world.resource::<Sky>().map(|sky| sky.ambient), Some(0.0));
    let state = game.engine.physics().unwrap().character_state(walker).unwrap();
    assert!(matches!(state.ground, voxear_physics::Ground::Ground), "standing: {:?}", state.ground);

    // Taken back to the level far from the Gate, nothing needs the caves, and a second on they are let go.
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(32.0, 4.1, 400.0);
    assert!(dimensions::move_obj(&mut game.engine.world, walker, DimensionId::LEVEL));
    for _ in 0..40 {
        game.tick(FRAME, None);
    }
    assert!(game.engine.world.is_alive(back), "kept a second");
    for _ in 0..60 {
        game.tick(FRAME, None);
    }
    assert!(!game.engine.world.is_alive(back), "let go once unneeded for longer than they are kept");
    assert!(game.engine.world.is_alive(field) && game.engine.world.is_alive(gate));
    assert!(game.engine.world.resource::<dimensions::Dimensions>().is_none());
}

#[test]
fn a_body_thrown_through_a_portal_into_a_dimension_keeps_its_speed() {
    use voxear_engine::physics::{Collider, Rigidbody};
    let (_dir, mut game) = gates();
    // An eye to come near with, standing still.
    let world = &mut game.engine.world;
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(32.0, 6.0, 4.0))));
    let ball = world.spawn((Transform::from_position(Vec3::new(32.0, 6.0, 12.0)),));
    world.insert(ball, Collider { size: [0.3, 0.3, 0.3], ..Collider::default() });
    world.insert(ball, Rigidbody { gravity: false, ..Rigidbody::default() });
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    let body = game.engine.physics().unwrap().body(ball).unwrap();
    game.engine.world.resource_mut::<PhysicsScene>().unwrap().world_mut().set_velocity(body, Vec3::Z * 6.0, Vec3::ZERO);
    for _ in 0..120 {
        game.tick(FRAME, None);
    }
    let caves = dimensions::dimension_of(&game.engine.world, named(&game, "Way Back"));
    assert_eq!(dimensions::dimension_of(&game.engine.world, ball), caves, "it went into the caves");
    let place = at(&game, ball);
    // Two seconds at 6 m/s from 8 m short of the Gate: 4 m past the Way Back.
    assert!((place.z - 36.0).abs() < 0.2 && (place.y - 14.0).abs() < 0.1, "past the Way Back, 2 m over its sill: {place:?}");
    let physics = game.engine.physics().unwrap();
    let body = physics.body(ball).unwrap();
    let (moving, _) = physics.world().velocity(body);
    assert!(moving.distance(Vec3::Z * 6.0) < 0.05, "as fast as it was thrown, the same way: {moving:?}");
}

#[test]
fn a_camera_following_a_walker_into_a_dimension_looks_through_the_gate_then_goes_after_it() {
    let (_dir, mut game) = gates();
    let (walker, eye) = walker_at_the_gate(&mut game, 3.0);
    game.engine.world.remove::<Camera>(eye);
    // A camera 4 m behind and 2 m over the walker, set there after it moves, as an orbit camera's script does.
    let camera = game.engine.world.spawn((Camera::default(), voxear_render::CameraFollows(walker), Transform::default()));
    game.engine.schedule.add_system("PostUpdate", "follow", move |world| {
        let Some(&subject) = world.get::<Transform>(walker) else { return };
        if let Some(transform) = world.get_mut::<Transform>(camera) {
            transform.position = subject.position + subject.rotation * Vec3::new(0.0, 2.0, -4.0);
            transform.rotation = subject.rotation;
        }
    });
    // Each frame: the walker's dimension, the dimension the camera sees, and whether a Portal carries its view.
    let mut seen = Vec::new();
    for _ in 0..360 {
        game.tick(FRAME, None);
        let world = &game.engine.world;
        let carried = world.get::<voxear_render::ViewCarry>(camera).map(|carry| carry.1);
        seen.push((dimensions::dimension_of(world, walker), dimensions::current(world), carried));
    }
    let caves = dimensions::dimension_of(&game.engine.world, named(&game, "Way Back"));
    let level = DimensionId::LEVEL;
    assert_eq!(seen.first().unwrap(), &(level, level, None));
    assert_eq!(seen.last().unwrap(), &(caves, caves, None), "the camera ends in the caves after the walker");
    // Between, the walker is in the caves while the camera, still behind the Gate, looks through it from the level.
    let looked_through = seen.iter().filter(|frame| **frame == (caves, level, Some(level))).count();
    assert!(looked_through > 3, "looked through the Gate for {looked_through} frames: {seen:?}");
    // And at no frame does the camera see the caves from the level's side or the level from theirs.
    assert!(seen.iter().all(|(_, current, carried)| carried.is_none_or(|carried| carried == *current)));
}

/// A level of rolling hills without end with small dungeons in it, each in
/// a dimension of its own, started with `renderer` where there is one.
fn dungeons_drawn(renderer: Option<&mut voxear_render::Renderer>) -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    write_dungeons(dir.path(), "own_dimension");
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(renderer).unwrap();
    (dir, game)
}

/// Writes the project of [`dungeons_drawn`] into `dir`, its dungeons placed
/// as `place` says.
fn write_dungeons(dir: &std::path::Path, place: &str) {
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -60,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        structures: vec!["Structures/Dungeon.structure".into()],
        ..BlockGenerate::default()
    });
    let structure = "ground = \"leave\"\n\n[placement]\nspacing = 240.0\nseparation = 60.0\nchance = 1.0\non = \"land\"\n\
                     height = [0.0, 500.0]\nslope = 60.0\n\n[dungeon]\nsize = \"small\"\nfloors = [2, 2]\nplace = \"{place}\"\n\n\
                     [dungeon.blocks]\nstone = \"Blocks/Temple Stone.block\"\nlight = \"Blocks/Fire.block\"\n\
                     metal = \"Blocks/Iron.block\"\n";
    let structure = structure.replace("{place}", place);
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Hills\"\n[obj.block_world]\ndata = \"Blocks/Hills.blockworld\"\n";
    write(
        dir,
        &[
            ("voxear.toml", "[project]\nname = \"Dungeons\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Hills.scene\"\n".into()),
            ("Scenes/Hills.scene", scene.into()),
            ("Blocks/Hills.blockworld", file.to_text()),
            ("Structures/Dungeon.structure", structure),
            ("Blocks/Temple Stone.block", "color = [0.62, 0.58, 0.5]\n".into()),
            ("Blocks/Fire.block", "color = [1.0, 0.62, 0.25]\nlight = 15\n".into()),
            ("Blocks/Iron.block", "color = [0.25, 0.25, 0.27]\n".into()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
}

fn dungeons() -> (tempfile::TempDir, Game) {
    dungeons_drawn(None)
}

#[test]
fn a_dungeon_is_its_gate_in_the_land_and_a_walker_goes_through_its_portal_into_its_own_dimension() {
    let (_dir, mut game) = dungeons();
    let world = &mut game.engine.world;
    let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(0.0, 300.0, 0.0))));
    world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    // How it walks: which way and how fast, and how fast it falls.
    let walking = Arc::new(Mutex::new((Vec3::ZERO, 0.0f32)));
    let walk = walking.clone();
    game.engine.schedule.add_system("Update", "walk", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let (way, fall) = *walk.lock().unwrap();
        if way != Vec3::ZERO || fall != 0.0 {
            physics.move_character(world, walker, way - Vec3::Y * fall, FRAME);
        }
        world.insert_resource(physics);
    });
    settle(&mut game);
    // The nearest dungeon's gate, as scripts find it.
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 800.0])
        .unwrap();
    let found: serde_json::Value = serde_json::from_str(&json).unwrap();
    let first = &found["dungeons"][0];
    assert_eq!(first["spaces"].as_array().map(Vec::len), Some(1), "its gate alone in the land: the landing");
    let entrance: Vec<f32> =
        first["entrance"]["at"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    // Over its entrance, a Portal is stood in the landing at the foot of its stair, leading nowhere yet.
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(entrance[0], 300.0, entrance[2]);
    settle(&mut game);
    // The gate of the dungeon whose entrance it is, of those stood round.
    let by = Vec3::new(entrance[0], entrance[1], entrance[2]);
    let stood = |game: &Game| {
        let world = &game.engine.world;
        let gates =
            world.query_ref::<(Entity, &voxear_engine::dungeons::DungeonGate)>().into_iter().map(|(entity, _)| entity);
        gates.min_by(|a, b| at(game, *a).distance(by).total_cmp(&at(game, *b).distance(by)))
    };
    let gate = (0..120).find_map(|_| {
        game.tick(FRAME, None);
        stood(&game)
    });
    let gate = gate.expect("a Portal stood in the gate");
    assert!(game.engine.world.get::<voxear_render::Portal>(gate).unwrap().to.is_none(), "nothing loaded from afar");
    assert!(game.engine.world.resource::<dimensions::Dimensions>().is_none());
    // Close to it, held still a second while the ground's collision there is made, the dungeon's dimension is
    // loaded and the Portal leads into it.
    let pose = voxear_render::PortalPose::of_obj(&game.engine.world, gate).unwrap();
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = pose.position + pose.front() * 1.5 + Vec3::Y;
    *walking.lock().unwrap() = (Vec3::ZERO, 0.0);
    for _ in 0..60 {
        game.tick(FRAME, None);
    }
    assert!(game.engine.world.get::<voxear_render::Portal>(gate).unwrap().to.is_some(), "loaded once close");
    let world = &game.engine.world;
    let back = world.get::<voxear_render::Portal>(gate).unwrap().to.unwrap();
    let inside = dimensions::dimension_of(world, back);
    assert_ne!(inside, DimensionId::LEVEL);
    let made = world.resource::<dimensions::Dimensions>().unwrap();
    assert_eq!(made.settings(inside).kind, dimensions::DimensionKind::Dungeon);
    // Stood on the landing in front of the Portal, clear of the landing's columns, the walker walks into it.
    let pose = voxear_render::PortalPose::of_obj(world, gate).unwrap();
    let front = pose.front();
    let start = pose.position + front * 1.5 + Vec3::Y;
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = start;
    *walking.lock().unwrap() = (Vec3::ZERO, 6.0);
    for _ in 0..30 {
        game.tick(FRAME, None);
    }
    *walking.lock().unwrap() = (-front * 3.0, 6.0);
    let mut passed = Vec::new();
    for _ in 0..120 {
        game.tick(FRAME, None);
        let events = game.engine.world.resource::<voxear_engine::portals::PortalEvents>();
        passed.extend(events.map(|events| events.passed.clone()).unwrap_or_default());
    }
    *walking.lock().unwrap() = (Vec3::ZERO, 6.0);
    for _ in 0..30 {
        game.tick(FRAME, None);
    }
    assert_eq!(passed.len(), 1, "{passed:?}");
    assert_eq!((passed[0].from, passed[0].to), (gate, back));
    // In the dungeon's own dimension, standing on its rock by the way down, in the dark.
    let world = &game.engine.world;
    assert_eq!(dimensions::dimension_of(world, walker), inside);
    assert_eq!(dimensions::current(world), inside);
    assert_eq!(world.resource::<Sky>().map(|sky| sky.ambient), Some(0.0));
    let state = game.engine.physics().unwrap().character_state(walker).unwrap();
    assert!(matches!(state.ground, voxear_physics::Ground::Ground), "standing: {:?}", state.ground);
    let here = at(&game, walker);
    let out = voxear_render::PortalPose::of_obj(world, back).unwrap();
    assert!(here.distance(out.position) < 8.0, "by the way out: {here:?}, it at {:?}", out.position);
    assert!((here.y - out.position.y).abs() < 0.2, "on the rock it stands on");
}

/// Keeps a picture where `VOXEAR_TEST_OUTPUT` names a folder.
fn keep(name: &str, (width, height): (u32, u32), pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// How bright a picture is on average, 0 to 255, and how many of its pixels
/// are bright with fire: much more red than blue.
fn brightness(pixels: &[u8]) -> (f32, usize) {
    let lit = pixels.chunks_exact(4).filter(|p| p[0] > 120 && p[0] as i32 > p[2] as i32 + 40).count();
    let sum: u64 = pixels.chunks_exact(4).map(|p| (p[0] as u64 + p[1] as u64 + p[2] as u64) / 3).sum();
    (sum as f32 / (pixels.len() / 4) as f32, lit)
}

#[test]
fn a_dungeons_gate_shows_its_own_dimension_through_its_portal_and_inside_its_fires_light_its_halls() {
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(960, 540, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let (_dir, mut game) = dungeons_drawn(Some(&mut renderer));
    let world = &mut game.engine.world;
    let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(0.0, 300.0, 0.0))));
    let eye = world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    let falling = Arc::new(Mutex::new(0.0f32));
    let fall = falling.clone();
    game.engine.schedule.add_system("Update", "fall", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let speed = *fall.lock().unwrap();
        if speed != 0.0 {
            physics.move_character(world, walker, -Vec3::Y * speed, FRAME);
        }
        world.insert_resource(physics);
    });
    let mut time = 1.0f32;
    let mut frames = |game: &mut Game, renderer: &mut Renderer, count: u32| {
        for _ in 0..count {
            game.tick(FRAME, Some(renderer));
            renderer.render(&game.engine.world, time).unwrap();
            time += FRAME;
        }
        renderer.read_pixels().unwrap().2
    };
    settle(&mut game);
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 800.0])
        .unwrap();
    let found: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entrance: Vec<f32> =
        found["dungeons"][0]["entrance"]["at"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = Vec3::new(entrance[0], 300.0, entrance[2]);
    settle(&mut game);
    let gate = (0..120)
        .find_map(|_| {
            frames(&mut game, &mut renderer, 1);
            let world = &game.engine.world;
            let by = Vec3::new(entrance[0], entrance[1], entrance[2]);
            let gates = world
                .query_ref::<(Entity, &voxear_engine::dungeons::DungeonGate)>()
                .into_iter()
                .map(|(entity, _)| entity);
            gates.min_by(|a, b| at(&game, *a).distance(by).total_cmp(&at(&game, *b).distance(by)))
        })
        .expect("a Portal in the gate");
    // On the landing, 6 m in front of the Portal, looking at it: the dungeon's hall through it, lit by its fires.
    let pose = voxear_render::PortalPose::of_obj(&game.engine.world, gate).unwrap();
    let front = pose.front();
    let facing = -front;
    let turn = voxear_core::math::Quat::from_rotation_y(facing.x.atan2(facing.z));
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = pose.position + front * 6.0 + Vec3::Y;
    game.engine.world.get_mut::<Transform>(walker).unwrap().rotation = turn;
    // Held still a second while the ground's collision there is made, then let stand.
    *falling.lock().unwrap() = 0.0;
    frames(&mut game, &mut renderer, 60);
    *falling.lock().unwrap() = 6.0;
    let through = frames(&mut game, &mut renderer, 90);
    keep("dungeon gate.png", (960, 540), &through);
    // Only what shows in the opening, the gate's own fires round it left out.
    let opening = {
        use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
        let (_, rotation, position) =
            voxear_scene::world_matrix(&game.engine.world, eye).to_scale_rotation_translation();
        let view = Mat4::from_rotation_translation(rotation, position).inverse();
        let lens = Camera::default();
        let projection =
            vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), 960.0 / 540.0, lens.near);
        let corners = pose.corners(voxear_core::math::Vec2::new(3.0, 4.0));
        let [x0, y0, x1, y1] =
            voxear_render::portals::rect_in_view(corners, projection * view).expect("the Portal in view");
        let px = |v: f32, most: f32| (((v * 0.5 + 0.5) * most) as i32).clamp(0, most as i32) as usize;
        [px(x0, 960.0) + 4, px(y0, 540.0) + 4, px(x1, 960.0) - 4, px(y1, 540.0) - 4]
    };
    let mut inside_opening = Vec::new();
    for y in opening[1]..opening[3] {
        inside_opening.extend_from_slice(&through[(y * 960 + opening[0]) * 4..(y * 960 + opening[2]) * 4]);
    }
    let (through_light, _) = brightness(&inside_opening);

    // Through it: standing by the way out, looking over the hall at the sinkhole and its stair.
    let back = game.engine.world.get::<voxear_render::Portal>(gate).unwrap().to.unwrap();
    let inside = dimensions::dimension_of(&game.engine.world, back);
    let out = voxear_render::PortalPose::of_obj(&game.engine.world, back).unwrap();
    assert!(dimensions::move_obj(&mut game.engine.world, walker, inside));
    let ahead = out.front();
    game.engine.world.get_mut::<Transform>(walker).unwrap().position = out.position + ahead * 2.0 + Vec3::Y * 0.5;
    game.engine.world.get_mut::<Transform>(walker).unwrap().rotation =
        voxear_core::math::Quat::from_rotation_y(ahead.x.atan2(ahead.z));
    game.engine.world.get_mut::<Transform>(eye).unwrap().rotation = voxear_core::math::Quat::from_rotation_x(0.35);
    let hall = frames(&mut game, &mut renderer, 90);
    keep("dungeon arrival.png", (960, 540), &hall);
    let (hall_light, hall_fire) = brightness(&hall);

    // And in a space of the second floor with a fire in it, lighting it.
    let world = &game.engine.world;
    let kept = world
        .query_ref::<(Entity, &voxear_engine::dungeons::DungeonInside)>()
        .into_iter()
        .map(|(e, inside)| (e, inside.0.clone()));
    let layout = kept
        .into_iter()
        .find(|(rock, _)| dimensions::dimension_of(world, *rock) == inside)
        .map(|(_, layout)| layout)
        .expect("the dungeon's world keeps its layout");
    assert_eq!(layout.floors, 2);
    let world = &game.engine.world;
    let rock = world.query_ref::<(Entity, &voxear_terrain::blocks::BlockWorld)>().into_iter();
    let rock =
        rock.into_iter().find(|(e, _)| dimensions::dimension_of(world, *e) == inside).map(|(_, b)| b.clone()).unwrap();
    // A fire of a space that burns: its fire block's light full over its stand.
    let burning = |space: &voxear_terrain::blocks::dungeon::Space| {
        layout.lights.iter().map(|light| light.at).find(|at| {
            let within = (at[0] - space.at[0]).abs() <= space.half[0] && (at[2] - space.at[2]).abs() <= space.half[1];
            within && (at[1] - space.at[1]).abs() <= space.height && rock.light.get([at[0], at[1] + 2, at[2]]) >= 12
        })
    };
    let (room, fire) = layout
        .spaces
        .iter()
        .filter(|space| space.height >= 0 && space.floor == 1)
        .find_map(|space| Some((space, burning(space)?)))
        .expect("a space on the second floor with a fire burning in it");
    let middle = Vec3::new(room.at[0] as f32 + 0.5, (room.at[1] + 1) as f32, room.at[2] as f32 + 0.5);
    let toward = Vec3::new(fire[0] as f32 - middle.x, 0.0, fire[2] as f32 - middle.z).normalize();
    let walking_to = &mut game.engine.world;
    walking_to.get_mut::<Transform>(walker).unwrap().position = middle + Vec3::Y * 0.5;
    walking_to.get_mut::<Transform>(walker).unwrap().rotation =
        voxear_core::math::Quat::from_rotation_y(toward.x.atan2(toward.z));
    walking_to.get_mut::<Transform>(eye).unwrap().rotation = voxear_core::math::Quat::from_rotation_x(0.1);
    // Long enough for the eye to get used to the dark after the bright hall.
    let deep = frames(&mut game, &mut renderer, 300);
    keep("dungeon room.png", (960, 540), &deep);
    let (light, _) = brightness(&deep);
    assert!(through_light > 4.0, "the dungeon's hall shows through the gate, lit by its fires: {through_light} bright");
    assert!(
        hall_fire > 0 && hall_light > 4.0,
        "the fires light the hall: {hall_light} bright, {hall_fire} fire pixels"
    );
    assert!(light > 4.0, "a room deep down is lit by its own fires: {light}");
}

#[test]
fn only_the_nearest_portal_into_a_dimension_is_open_and_one_such_dimension_at_most_is_loaded() {
    let (_dir, mut game) = gates();
    let world = &mut game.engine.world;
    let eye = world.spawn((Camera::default(), Transform::from_position(Vec3::new(32.0, 6.0, 120.0))));
    let (gate, second) = (named(&game, "Gate"), named(&game, "Second Gate"));
    let loaded = |game: &Game| {
        let made = game.engine.world.resource::<dimensions::Dimensions>();
        made.map_or(0, |made| made.on_demand().count())
    };
    let leads = |game: &Game, portal: Entity| game.engine.world.get::<voxear_render::Portal>(portal).unwrap().to;
    let mut most = 0;
    let mut tick = |game: &mut Game, count: u32| {
        for _ in 0..count {
            game.tick(FRAME, None);
            most = most.max(loaded(game));
        }
    };
    // 100 m off, well within sight of both Gates, nothing is loaded: they show their mist.
    tick(&mut game, 10);
    assert_eq!(loaded(&game), 0);
    assert!(leads(&game, gate).is_none() && leads(&game, second).is_none());
    // Close to the first, its caves are loaded, and the second's dimension is not.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(36.0, 6.0, 12.0);
    tick(&mut game, 10);
    assert_eq!(loaded(&game), 1);
    assert!(leads(&game, gate).is_some() && leads(&game, second).is_none());
    // Nearer the second, its dimension is loaded in place of the first's, at once.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(68.0, 6.0, 12.0);
    tick(&mut game, 10);
    assert_eq!(loaded(&game), 1);
    let second_to = leads(&game, second).expect("the second Gate leads into its dimension");
    let deep = dimensions::dimension_of(&game.engine.world, second_to);
    let made = game.engine.world.resource::<dimensions::Dimensions>().unwrap();
    assert_eq!(made.on_demand().collect::<Vec<_>>(), vec![deep], "the first's caves let go");
    // Away from both, the last is kept a second, then let go.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(52.0, 6.0, 140.0);
    tick(&mut game, 40);
    assert_eq!(loaded(&game), 1);
    tick(&mut game, 60);
    assert_eq!(loaded(&game), 0);
    assert_eq!(most, 1, "never more than one loaded at once");
}

#[test]
fn a_change_saved_in_a_dungeon_under_the_land_goes_once_dungeons_are_in_dimensions_of_their_own() {
    use voxear_terrain::blocks::edits::{BlockEdits, ChunkChange};
    // Under the land, as dungeons were: a room of the nearest dungeon, from its layout.
    let dir = tempfile::tempdir().unwrap();
    write_dungeons(dir.path(), "in_the_land");
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let json = voxear_engine::dungeons::dungeons_json(&game.engine.world, &mut game.content, 0, [0.0, 0.0, 0.0, 800.0])
        .unwrap();
    let found: serde_json::Value = serde_json::from_str(&json).unwrap();
    let spaces = found["dungeons"][0]["spaces"].as_array().unwrap().clone();
    assert!(spaces.len() > 3, "a whole dungeon under the land");
    let room = spaces.iter().find(|space| space["kind"] == "Room").expect("a room");
    let at: Vec<i32> = room["at"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap().floor() as i32).collect();
    // A lamp put on that room's floor, and a block high in the open sky, saved by a version before dimensions.
    let side = 32;
    let change = |block: [i32; 3]| {
        let key = block.map(|v| v.div_euclid(side));
        let [x, y, z] = block.map(|v| v.rem_euclid(side) as u16);
        (key, x + z * side as u16 + y * (side * side) as u16)
    };
    let in_room = [at[0], at[1] + 1, at[2]];
    let in_sky = [5, 200, 5];
    let mut edits = BlockEdits::default();
    for (block, number) in [(in_room, 3), (in_sky, 3)] {
        let (key, index) = change(block);
        edits.chunks.insert(key, ChunkChange::Blocks(vec![(index, number)]));
    }
    let mut bytes = edits.to_bytes();
    bytes[8..12].copy_from_slice(&3u32.to_le_bytes());
    // The same world with its dungeons in dimensions of their own, those changes beside it.
    let moved = tempfile::tempdir().unwrap();
    write_dungeons(moved.path(), "own_dimension");
    std::fs::write(moved.path().join("Blocks/Hills.blocks"), &bytes).unwrap();
    let mut game = Game::new(Project::open(moved.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let pending =
        world.query_ref::<&voxear_engine::blocks::BlockWorldAsset>().into_iter().next().unwrap().pending.clone();
    let kept: Vec<[i32; 3]> = pending.chunks.keys().copied().collect();
    assert_eq!(kept, vec![change(in_sky).0], "the lamp in the room that is rock now went; the block in the sky stayed");
    // Saved again, they are of this version, and nothing more goes.
    let again = BlockEdits::from_bytes(&pending.to_bytes()).unwrap();
    assert_eq!(
        voxear_terrain::blocks::edits::version_of(&pending.to_bytes()),
        Some(voxear_terrain::blocks::edits::VERSION)
    );
    assert_eq!(again.chunks.len(), 1);
}
