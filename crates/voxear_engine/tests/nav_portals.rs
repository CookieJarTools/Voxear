//! Portals as Nav Agents take them: two Portals either side of a wall with
//! no way round are walked through, in a world of blocks and on a level's
//! nav mesh, by both ways of moving; and an agent sent into a dimension of
//! caves walks to the Gate that leads there, through it, and on to where it
//! was sent (headless: no GPU, no scripts).

use std::sync::Arc;

use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    dimensions::{self, DimensionId},
    nav_agent::{NavAgent, NavAgentState, NavMoves, find_way},
    navigation::{LevelNav, bake_triangles},
    physics::{CharacterController, Collider},
    portals::PortalEvents,
};
use voxear_nav::PathStatus;
use voxear_render::{Camera, Portal};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const FRAME: f32 = 1.0 / 60.0;

fn project(files: &[(&str, String)]) -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (dir, game)
}

/// Two Portals leading to each other, standing at `west` facing -x and at
/// `east` facing +x: walked into eastward at the first, one comes out of
/// the second still walking east.
fn portals(game: &mut Game, west: Vec3, east: Vec3) -> (Entity, Entity) {
    let world = &mut game.engine.world;
    let turn = |angle: f32| Quat::from_rotation_y(angle.to_radians());
    let near = world.spawn((Portal::default(), Transform::new(west, turn(-90.0), Vec3::ONE)));
    let far = world.spawn((Portal::default(), Transform::new(east, turn(90.0), Vec3::ONE)));
    *world.get_mut::<Portal>(near).unwrap() = Portal::to(Vec2::new(2.0, 3.0), far);
    *world.get_mut::<Portal>(far).unwrap() = Portal::to(Vec2::new(2.0, 3.0), near);
    (near, far)
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

fn arrived(game: &Game, entity: Entity) -> bool {
    let agent = game.engine.world.get::<NavAgent>(entity).unwrap();
    game.engine.world.get::<NavAgentState>(entity).is_some_and(|state| state.is_arrived(agent))
}

/// An agent at `from`, moving as `moves` says, sent to `to` until it is
/// there or `most` frames have gone: where it ends, how many frames went,
/// how many times it went through a Portal, and whether it was ever in the
/// wall between `wall` x.
fn walk(
    game: &mut Game,
    from: Vec3,
    to: Vec3,
    moves: NavMoves,
    most: usize,
    wall: [f32; 2],
) -> (Vec3, usize, usize, bool) {
    let agent = NavAgent { moves, speed: 3.0, ..NavAgent::default() };
    let start = Transform::from_position(from);
    let walker = match moves {
        NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
        _ => game.engine.world.spawn((start, agent)),
    };
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    let (mut frames, mut passed, mut inside) = (0, 0, false);
    while frames < most && !(frames > 10 && arrived(game, walker)) {
        game.tick(FRAME, None);
        frames += 1;
        let events = game.engine.world.resource::<PortalEvents>();
        passed += events.map_or(0, |events| events.passed.iter().filter(|passed| passed.obj == walker).count());
        let now = at(game, walker);
        inside |= now.x > wall[0] && now.x < wall[1];
    }
    (at(game, walker), frames, passed, inside)
}

/// A field of blocks 32 across and six deep, cut in two by a wall three
/// blocks high along x 16, with no way round.
fn field() -> (tempfile::TempDir, Game) {
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let (dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Portals\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
        ("Scenes/Field.scene", scene.into()),
        ("Blocks/Field.blockworld", file.to_text()),
    ]);
    let field = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Blocks").unwrap().0;
    let wall: Vec<([i32; 3], u16)> = (0..32).flat_map(|z| (6..9).map(move |y| ([16, y, z], 1))).collect();
    game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(wall);
    game.tick(FRAME, None);
    (dir, game)
}

#[test]
fn an_agent_goes_through_a_portal_pair_past_a_wall_with_no_way_round_in_a_world_of_blocks() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let (_dir, mut game) = field();
        let (from, to) = (Vec3::new(4.5, 6.0, 6.5), Vec3::new(28.5, 6.0, 6.5));
        assert_eq!(find_way(&mut game.engine.world, from, to, 1.8).0, PathStatus::Partial, "no way round the wall");
        portals(&mut game, Vec3::new(12.0, 6.0, 20.5), Vec3::new(21.0, 6.0, 20.5));
        game.tick(FRAME, None);
        let (status, corners, _) = find_way(&mut game.engine.world, from, to, 1.8);
        assert_eq!(status, PathStatus::Complete, "through the Portals: {corners:?}");
        let (now, frames, passed, inside) = walk(&mut game, from, to, moves, 1800, [15.6, 17.4]);
        assert!((now - to).length() < 0.6, "{moves:?}: got there, at {now} after {frames} frames");
        assert_eq!(passed, 1, "{moves:?}: through once");
        assert!(!inside, "{moves:?}: never in the wall");
    }
}

#[test]
fn an_agent_goes_through_a_portal_pair_past_a_wall_with_no_way_round_on_a_levels_nav_mesh() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let (_dir, mut game) = project(&[
            ("voxear.toml", "[project]\nname = \"Mesh\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".into()),
            ("Scenes/Main.scene", "format = 1\n".into()),
        ]);
        let block =
            |middle: Vec3, size: [f32; 3]| (Transform::from_position(middle), Collider { size, ..Default::default() });
        game.engine.world.spawn(block(Vec3::new(15.0, -0.5, 15.0), [30.0, 1.0, 30.0]));
        game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 15.0), [1.0, 3.0, 30.0]));
        game.tick(FRAME, None);
        let mesh = voxear_nav::mesh::bake(&bake_triangles(&game.engine.world), Default::default()).unwrap();
        game.engine.world.insert_resource(LevelNav(Arc::new(mesh)));
        let (from, to) = (Vec3::new(4.0, 0.0, 5.0), Vec3::new(26.0, 0.0, 5.0));
        assert_eq!(find_way(&mut game.engine.world, from, to, 1.8).0, PathStatus::Partial, "no way round the wall");
        portals(&mut game, Vec3::new(10.0, 0.0, 20.0), Vec3::new(20.0, 0.0, 20.0));
        game.tick(FRAME, None);
        let (status, corners, _) = find_way(&mut game.engine.world, from, to, 1.8);
        assert_eq!(status, PathStatus::Complete, "through the Portals: {corners:?}");
        let (now, frames, passed, inside) = walk(&mut game, from, to, moves, 1800, [14.3, 15.7]);
        assert!((now - to).length() < 0.6, "{moves:?}: got there, at {now} after {frames} frames");
        assert_eq!(passed, 1, "{moves:?}: through once");
        assert!(!inside, "{moves:?}: never in the wall");
    }
}

/// A flat field of blocks 64 a side and `height` high, a block a metre.
fn flat(height: u32) -> String {
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.ground.height = height;
    file.to_text()
}

#[test]
fn an_agent_sent_into_a_dimension_walks_to_the_gate_into_it_through_and_on() {
    // A field 4 high with a Gate facing -z leading into the Way Back of a dimension of caves whose rock is 12 high.
    let level = "format = 1\n\n[[obj]]\nid = 1\nname = \"Field\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Gate\"\nposition = [32.0, 4.0, 20.0]\nrotation = [0.0, 180.0, 0.0]\n\
                 [obj.portal]\nsize = [3.0, 4.0]\ninto = { dimension = \"Dimensions/Caves.dimension\", portal = 2 }\n";
    let caves = "format = 1\n\n[dimension]\nkind = \"caves\"\n\n\
                 [[obj]]\nid = 1\nname = \"Rock\"\n[obj.block_world]\ndata = \"Blocks/Rock.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Way Back\"\nposition = [32.0, 12.0, 32.0]\n[obj.portal]\nsize = [3.0, 4.0]\n";
    let (_dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Gates\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
        ("Scenes/Field.scene", level.into()),
        ("Dimensions/Caves.dimension", caves.into()),
        ("Blocks/Field.blockworld", flat(4)),
        ("Blocks/Rock.blockworld", flat(12)),
    ]);
    // A camera near the Gate, so the caves are made.
    game.engine.world.spawn((Camera::default(), Transform::from_position(Vec3::new(32.0, 6.0, 12.0))));
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let agent = NavAgent { moves, speed: 3.0, ..NavAgent::default() };
        let start = Transform::from_position(Vec3::new(30.5, 4.0, 6.5));
        let walker = match moves {
            NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
            _ => game.engine.world.spawn((start, agent)),
        };
        for _ in 0..10 {
            game.tick(FRAME, None);
        }
        let named = |game: &Game, name: &str| {
            let world = &game.engine.world;
            world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).map(|(entity, _)| entity)
        };
        let back = named(&game, "Way Back").expect("the caves are made as the camera is near");
        let caves = dimensions::dimension_of(&game.engine.world, back);
        assert_ne!(caves, DimensionId::LEVEL);
        let to = Vec3::new(36.5, 12.0, 44.5);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to_in(to, caves);
        let mut frames = 0;
        while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
            game.tick(FRAME, None);
            frames += 1;
        }
        let now = at(&game, walker);
        assert_eq!(dimensions::dimension_of(&game.engine.world, walker), caves, "{moves:?}: into the caves, at {now}");
        assert!((now - to).length() < 0.6, "{moves:?}: and on to where it was sent: {now} after {frames} frames");
        game.engine.world.despawn(walker);
    }
}

#[test]
fn a_portal_that_lets_only_one_layer_through_is_a_way_for_agents_of_that_layer_alone() {
    let (_dir, mut game) = field();
    let (near, far) = portals(&mut game, Vec3::new(12.0, 6.0, 20.5), Vec3::new(21.0, 6.0, 20.5));
    for portal in [near, far] {
        game.engine.world.get_mut::<Portal>(portal).unwrap().passes = 1 << 3;
    }
    game.tick(FRAME, None);
    let (from, to) = (Vec3::new(4.5, 6.0, 6.5), Vec3::new(28.5, 6.0, 6.5));
    assert_eq!(find_way(&mut game.engine.world, from, to, 1.8).0, PathStatus::Partial, "not for the Default layer");
    for (layer, through) in [(0, false), (3, true)] {
        let agent = NavAgent { moves: NavMoves::AlongTheNav, speed: 4.0, ..NavAgent::default() };
        let walker = game.engine.world.spawn((Transform::from_position(from), agent, voxear_scene::Layer(layer)));
        game.tick(FRAME, None);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        for _ in 0..900 {
            game.tick(FRAME, None);
        }
        let now = at(&game, walker);
        assert_eq!((now - to).length() < 0.6, through, "layer {layer}: at {now}");
        game.engine.world.despawn(walker);
    }
}

/// A field of blocks 32 across and six deep in `dimension`.
fn field_in(game: &mut Game, dimension: DimensionId, corner: Vec3) -> Entity {
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let (map, faces) = (file.make_ground(None), file.faces().1);
    let world = game.engine.world.spawn((BlockWorld::new(map, &faces, 1.0), Transform::from_position(corner)));
    voxear_scene::set_dimension(&mut game.engine.world, world, dimension);
    world
}

/// A Portal standing at `at` facing -z, in `from`, leading to one facing
/// +z at `out` in `into`.
fn portal_into(game: &mut Game, at: Vec3, from: DimensionId, out: Vec3, into: DimensionId) {
    let world = &mut game.engine.world;
    let far = world.spawn((Portal::default(), Transform::from_position(out)));
    voxear_scene::set_dimension(world, far, into);
    let near = world.spawn((
        Portal::to(Vec2::new(2.0, 3.0), far),
        Transform::new(at, Quat::from_rotation_y(std::f32::consts::PI), Vec3::ONE),
    ));
    voxear_scene::set_dimension(world, near, from);
}

#[test]
fn an_agent_sent_two_dimensions_away_goes_through_one_portal_and_then_the_next() {
    let (_dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Chain\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".into()),
        ("Scenes/Main.scene", "format = 1\n".into()),
    ]);
    // Made in the open level: one made in another is let go as the level changes.
    let made = |game: &mut Game| {
        let level = game.levels.current().map(str::to_string);
        let settings = dimensions::DimensionSettings::default();
        dimensions::make_empty(&mut game.engine.world, settings, None, level.as_deref()).unwrap()
    };
    let (first, second) = (made(&mut game), made(&mut game));
    for dimension in [DimensionId::LEVEL, first, second] {
        field_in(&mut game, dimension, Vec3::ZERO);
    }
    // From the level into the first, and from the first into the second, at the other end of its field.
    portal_into(&mut game, Vec3::new(16.0, 6.0, 12.0), DimensionId::LEVEL, Vec3::new(8.0, 6.0, 4.0), first);
    portal_into(&mut game, Vec3::new(24.0, 6.0, 26.0), first, Vec3::new(16.0, 6.0, 4.0), second);
    let agent = NavAgent { moves: NavMoves::AlongTheNav, speed: 4.0, ..NavAgent::default() };
    let walker = game.engine.world.spawn((Transform::from_position(Vec3::new(16.5, 6.0, 2.5)), agent));
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    let to = Vec3::new(16.5, 6.0, 24.5);
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to_in(to, second);
    let (mut frames, mut passed) = (0, Vec::new());
    while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
        game.tick(FRAME, None);
        frames += 1;
        let events = game.engine.world.resource::<PortalEvents>();
        passed.extend(
            events
                .into_iter()
                .flat_map(|events| events.passed.iter().filter(|p| p.obj == walker).map(|p| p.dimensions)),
        );
    }
    let now = at(&game, walker);
    assert_eq!(passed, [[DimensionId::LEVEL, first], [first, second]], "through one and then the next, at {now}");
    assert_eq!(dimensions::dimension_of(&game.engine.world, walker), second);
    assert!((now - to).length() < 0.6, "on to where it was sent: {now} after {frames} frames");
}

#[test]
fn an_agent_crosses_a_nav_link_from_one_world_of_blocks_into_another() {
    use voxear_engine::navigation::{NavLink, NavLinkKind};
    let (_dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Two\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".into()),
        ("Scenes/Main.scene", "format = 1\n".into()),
    ]);
    // Two fields with eight metres of nothing between them, and a plank across it.
    field_in(&mut game, DimensionId::LEVEL, Vec3::ZERO);
    field_in(&mut game, DimensionId::LEVEL, Vec3::new(40.0, 0.0, 0.0));
    let plank = NavLink { end: [9.0, 0.0, 0.0], kind: NavLinkKind::Walk, both_ways: true, cost: 0.0 };
    game.engine.world.spawn((Transform::from_position(Vec3::new(31.5, 6.0, 16.5)), plank));
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let agent = NavAgent { moves, speed: 4.0, ..NavAgent::default() };
        let start = Transform::from_position(Vec3::new(6.5, 6.0, 6.5));
        let walker = match moves {
            NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
            _ => game.engine.world.spawn((start, agent)),
        };
        for _ in 0..10 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(60.5, 6.0, 6.5);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let mut frames = 0;
        while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
            game.tick(FRAME, None);
            frames += 1;
        }
        let now = at(&game, walker);
        assert!((now - to).length() < 0.6, "{moves:?}: over the plank and on: {now} after {frames} frames");
        game.engine.world.despawn(walker);
    }
}

#[test]
fn an_agent_walks_from_one_world_of_blocks_into_another_where_they_touch() {
    let (_dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Touching\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".into()),
        ("Scenes/Main.scene", "format = 1\n".into()),
    ]);
    // Two fields edge to edge along x, the second a block higher: a step up from one to the other.
    field_in(&mut game, DimensionId::LEVEL, Vec3::ZERO);
    field_in(&mut game, DimensionId::LEVEL, Vec3::new(32.0, 1.0, 0.0));
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let agent = NavAgent { moves, speed: 4.0, ..NavAgent::default() };
        let start = Transform::from_position(Vec3::new(6.5, 6.0, 6.5));
        let walker = match moves {
            NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
            _ => game.engine.world.spawn((start, agent)),
        };
        for _ in 0..10 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(50.5, 7.0, 20.5);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let mut frames = 0;
        while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
            game.tick(FRAME, None);
            frames += 1;
        }
        let now = at(&game, walker);
        assert!((now - to).length() < 0.6, "{moves:?}: across where they touch and on: {now} after {frames} frames");
        game.engine.world.despawn(walker);
    }
}
