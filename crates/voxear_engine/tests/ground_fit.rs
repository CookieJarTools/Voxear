//! Ground Fits and Streams With Ground in a world of blocks without end, as
//! the Infinite World template makes it, played without scripts: a fit
//! levels the ground under it as the world streams, the ground comes back
//! where it moved from, and an Obj kept for later goes with its ground and
//! comes back as it was.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    ground_fit::{FitBox, FitMode, GroundFit},
    ground_streaming::{StreamsWithGround, WhenUnloaded},
    new_project::{NewProject, create},
};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::BlockWorld;

fn named(game: &Game, name: &str) -> Option<Entity> {
    let objs = game.engine.world.query_ref::<(Entity, &Name)>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(entity, _)| entity)
}

/// The highest block of the world's ground at `at` along x and z, in metres
/// up the top of it; none where the column is not made.
fn top_at(game: &Game, world: Entity, at: Vec3) -> Option<f32> {
    let corner = game.engine.world.get::<Transform>(world)?.position;
    let blocks = game.engine.world.get::<BlockWorld>(world)?;
    let local = (at - corner) / blocks.block_size;
    let (x, z) = (local.x.floor() as i32, local.z.floor() as i32);
    let top = (-80..600).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0)?;
    Some(corner.y + (top + 1) as f32 * blocks.block_size)
}

/// Ticks until `done`, a minute of frames at most.
fn until(game: &mut Game, done: &mut dyn FnMut(&Game) -> bool) -> bool {
    for _ in 0..3600 {
        if done(game) {
            return true;
        }
        game.tick(1.0 / 30.0, None);
    }
    done(game)
}

/// A game of the Infinite World template, played without its scripts, so the
/// camera stays where the test puts it; and the folder it is in.
fn game_of(name: &str) -> (Game, tempfile::TempDir) {
    let parent = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = parent.path().join(name);
    let request = NewProject {
        folder: folder.clone(),
        name: name.into(),
        template: "infinite-world".into(),
        setup: None,
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    create(&request, &mut |_| {}).unwrap();
    let scene_path = folder.join("Scenes/Infinite World.scene");
    let mut scene = voxear_engine::scene_file::read(&scene_path).unwrap();
    for obj in &mut scene.objs {
        obj.behaviours.clear();
        obj.fields.clear();
        obj.enabled.clear();
    }
    voxear_engine::scene_file::write(&scene_path, &scene).unwrap();
    let mut game = Game::new(Project::open(&folder).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (game, parent)
}

/// The block at `at` of the world, and its top's block.
fn block_at(game: &Game, world: Entity, at: Vec3) -> u16 {
    let corner = game.engine.world.get::<Transform>(world).unwrap().position;
    let blocks = game.engine.world.get::<BlockWorld>(world).unwrap();
    let local = (at - corner) / blocks.block_size;
    blocks.blocks.get([local.x.floor() as i32, local.y.floor() as i32, local.z.floor() as i32])
}

#[test]
fn a_graded_road_is_laid_through_the_streamed_world_with_its_blocks_and_found_from_a_place() {
    use voxear_engine::roads::{MixBlock, Road, RoadGround, RoadPoint};
    let (mut game, _folder) = game_of("Roads");
    let world = named(&game, "World").expect("the template's world");
    let camera = named(&game, "Main Camera").expect("the template's camera");
    let eye = game.engine.world.get::<Transform>(camera).unwrap().position;
    let (start, end) = (Vec3::new(eye.x - 10.0, 0.0, eye.z + 10.0), Vec3::new(eye.x + 30.0, 0.0, eye.z + 10.0));
    assert!(until(&mut game, &mut |game| top_at(game, world, start).is_some() && top_at(game, world, end).is_some()));
    // A road 40 m long at one height a few metres over the ground at its start, laid with gravel.
    let height = top_at(&game, world, start).unwrap() + 3.0;
    let gravel: voxear_assets::AssetGuid = "b15aba9a8e7047689d6bffb862d6679c".parse().unwrap();
    let road = Road {
        points: vec![
            RoadPoint { at: [0.0, 0.0, 0.0], width: 5.0 },
            RoadPoint { at: [end.x - start.x, 0.0, 0.0], width: 5.0 },
        ],
        mix: vec![MixBlock { block: Some(gravel), weight: 1 }],
        ground: RoadGround::Grade,
        tunnel: false,
        ..Road::default()
    };
    game.engine.world.spawn((Name("Road".into()), Transform::from_position(Vec3::new(start.x, height, start.z)), road));
    let middle = Vec3::new((start.x + end.x) * 0.5, 0.0, start.z);
    let laid = |game: &Game| top_at(game, world, middle).is_some_and(|top| (top - height).abs() < 0.01);
    assert!(until(&mut game, &mut |game| laid(game)), "laid at {height}, not {:?}", top_at(&game, world, middle));
    let asset = game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(world).unwrap();
    let number = asset.drawn.number_of(gravel);
    assert_eq!(Some(block_at(&game, world, Vec3::new(middle.x, height - 0.5, middle.z))), number, "laid with gravel");
    // Found from a place beside it.
    let beside = Vec3::new(middle.x, height, middle.z + 6.0);
    let blocks = world;
    let near = voxear_engine::roads::nearest(&game.engine.world, blocks, beside, 20.0, None);
    let (found, point, _) = near.expect("a road near");
    assert_eq!(game.engine.world.get::<Name>(found).unwrap().0, "Road");
    assert!((point.z - start.z).abs() < 0.5, "{point}");
    assert!(voxear_engine::roads::nearest(&game.engine.world, blocks, beside + Vec3::Z * 100.0, 20.0, None).is_none());
}

#[test]
fn a_walkable_platform_is_stood_on_in_the_worlds_navigation_and_a_door_is_left_open() {
    use voxear_engine::walkable_models::Walkable;
    use voxear_physics::scene::{Collider, ColliderShape};
    let (mut game, _folder) = game_of("Walkable");
    let world = named(&game, "World").expect("the template's world");
    let camera = named(&game, "Main Camera").expect("the template's camera");
    let eye = game.engine.world.get::<Transform>(camera).unwrap().position;
    let spot = Vec3::new(eye.x + 8.0, 0.0, eye.z + 8.0);
    assert!(until(&mut game, &mut |game| top_at(game, world, spot).is_some()));
    // A platform 8 m square, its top 6 m over the ground, held up by nothing: only its own surfaces are stood on.
    let ground = top_at(&game, world, spot).unwrap();
    let top = ground + 6.0;
    let platform = Collider { shape: ColliderShape::Box, size: [8.0, 1.0, 8.0], ..Collider::default() };
    game.engine.world.spawn((
        Name("Platform".into()),
        Transform::from_position(Vec3::new(spot.x, top - 0.5, spot.z)),
        platform,
        Walkable::default(),
    ));
    voxear_engine::navigation::nav_of(&mut game.engine.world, world).expect("the world's navigation");
    let stood = |game: &mut Game| {
        let place =
            voxear_engine::nav_agent::nearest_place(&mut game.engine.world, Vec3::new(spot.x, top + 0.5, spot.z), 1.0);
        place.is_some_and(|place| (place.y - top).abs() < 1.1)
    };
    let mut on = false;
    for _ in 0..600 {
        game.tick(1.0 / 30.0, None);
        if stood(&mut game) {
            on = true;
            break;
        }
    }
    assert!(on, "a place to stand on the platform's top at {top}");
    assert!(voxear_engine::walkable_models::stamped_cells(&game.engine.world, world) > 0);
    // Marked a door, it is left out: nothing stands on it there.
    let platform = named(&game, "Platform").unwrap();
    game.engine.world.insert(platform, Walkable { door: true });
    let mut gone = false;
    for _ in 0..600 {
        game.tick(1.0 / 30.0, None);
        if !stood(&mut game) {
            gone = true;
            break;
        }
    }
    assert!(gone, "a door is walked through, not stood on");
}

#[test]
fn a_ground_fit_levels_the_streamed_ground_and_objs_kept_for_later_go_and_come_back_with_it() {
    let parent = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = parent.path().join("Fitted");
    let request = NewProject {
        folder: folder.clone(),
        name: "Fitted".into(),
        template: "infinite-world".into(),
        setup: None,
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    create(&request, &mut |_| {}).unwrap();
    // Played without its scripts, the camera stays where the test puts it.
    let scene_path = folder.join("Scenes/Infinite World.scene");
    let mut scene = voxear_engine::scene_file::read(&scene_path).unwrap();
    for obj in &mut scene.objs {
        obj.behaviours.clear();
        obj.fields.clear();
        obj.enabled.clear();
    }
    voxear_engine::scene_file::write(&scene_path, &scene).unwrap();
    let project = Project::open(&folder).unwrap();
    let mut game = Game::new(project, Engine::new()).unwrap();
    game.start(None).unwrap();
    let world = named(&game, "World").expect("the template's world");
    let camera = named(&game, "Main Camera").expect("the template's camera");
    let eye = game.engine.world.get::<Transform>(camera).unwrap().position;
    let plot = Vec3::new(eye.x + 12.0, 0.0, eye.z + 12.0);
    assert!(until(&mut game, &mut |game| top_at(game, world, plot).is_some()), "the ground under the plot is made");
    let ground = top_at(&game, world, plot).unwrap();

    // A house's plot 8 m square, levelled 4 m over the ground there.
    let level = ground + 4.0;
    let fit = GroundFit {
        mode: FitMode::Flatten,
        boxes: vec![FitBox { centre: [0.0, 0.0], size: [8.0, 8.0] }],
        ..GroundFit::default()
    };
    let house = game.engine.world.spawn((
        Name("House".into()),
        Transform::from_position(Vec3::new(plot.x, level, plot.z)),
        fit,
    ));
    let levelled = |game: &Game| top_at(game, world, plot).is_some_and(|top| (top - level).abs() < 0.01);
    assert!(until(&mut game, &mut |game| levelled(game)), "levelled at {level}, not {:?}", top_at(&game, world, plot));

    // Moved 60 m on: levelled there, and the ground where it stood as it was.
    let moved = Vec3::new(plot.x + 60.0, level, plot.z);
    game.engine.world.get_mut::<Transform>(house).unwrap().position = moved;
    let back = |game: &Game| top_at(game, world, plot).is_some_and(|top| (top - ground).abs() < 0.01);
    assert!(
        until(&mut game, &mut |game| back(game)),
        "the ground back at {ground}, not {:?}",
        top_at(&game, world, plot)
    );
    let there = |game: &Game| top_at(game, world, moved).is_some_and(|top| (top - level).abs() < 0.01);
    assert!(until(&mut game, &mut |game| there(game)));

    // A barrel kept for later, and a mob destroyed, on the ground near the camera.
    let barrel = game.engine.world.spawn((
        Name("Barrel".into()),
        Transform::from_position(Vec3::new(plot.x, ground, plot.z)),
        StreamsWithGround { when_unloaded: WhenUnloaded::KeepForLater },
    ));
    let wolf = game.engine.world.spawn((
        Name("Wolf".into()),
        Transform::from_position(Vec3::new(plot.x, ground, plot.z + 4.0)),
        StreamsWithGround { when_unloaded: WhenUnloaded::Destroy },
    ));
    for _ in 0..3 {
        game.tick(1.0 / 30.0, None);
    }
    assert!(
        game.engine.world.is_alive(barrel) && game.engine.world.is_alive(wolf),
        "both stay while their ground does"
    );

    // Every camera and the player 5 km off: their ground goes, and they with it.
    let movers: Vec<(Entity, Vec3)> = {
        let world = &game.engine.world;
        let cameras = world.query_ref::<(Entity, &voxear_render::Camera, &Transform)>();
        let mut movers: Vec<(Entity, Vec3)> = cameras.into_iter().map(|(e, _, t)| (e, t.position)).collect();
        movers.extend(named(&game, "Player").and_then(|p| Some((p, world.get::<Transform>(p)?.position))));
        movers.retain(|(e, _)| !world.has::<voxear_scene::Parent>(*e));
        movers
    };
    let away = |game: &mut Game, by: Vec3| {
        for (mover, was) in &movers {
            game.engine.world.get_mut::<Transform>(*mover).unwrap().position = *was + by;
        }
    };
    away(&mut game, Vec3::new(5000.0, 0.0, 0.0));
    let gone = |game: &Game| named(game, "Barrel").is_none() && named(game, "Wolf").is_none();
    assert!(until(&mut game, &mut |game| gone(game)), "both go with their ground");
    assert!(named(&game, "House").is_none(), "a fitted Obj is kept for later too");
    assert_eq!(voxear_engine::ground_streaming::kept_count(&game.engine.world), 2, "the barrel and the house kept");

    // And back: the barrel and the house as they were, the wolf not.
    away(&mut game, Vec3::ZERO);
    let returned = |game: &Game| named(game, "Barrel").is_some() && named(game, "House").is_some();
    assert!(until(&mut game, &mut |game| returned(game)), "kept Objs come back with their ground");
    assert!(named(&game, "Wolf").is_none());
    let barrel = named(&game, "Barrel").unwrap();
    assert!(game.engine.world.get::<StreamsWithGround>(barrel).is_some());
    let house = named(&game, "House").unwrap();
    assert_eq!(game.engine.world.get::<Transform>(house).unwrap().position, moved);
    assert!(game.engine.world.get::<GroundFit>(house).is_some());
    assert!(until(&mut game, &mut |game| there(game)), "and its ground levelled again");
}
