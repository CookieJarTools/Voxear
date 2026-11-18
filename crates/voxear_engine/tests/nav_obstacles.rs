//! Nav Obstacles: a crate put in the only gap of a wall shuts the way
//! through it for an agent already walking there, in a world of blocks and
//! on a level's nav mesh, and taking it away opens the way again; one that
//! moves carves only once it stands still; one that never carves is gone
//! round as agents go round each other (headless: no GPU, no scripts).

use std::sync::Arc;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    nav_agent::{NavAgent, NavAgentState, NavMoves, area_at, find_way},
    nav_obstacle::{NavObstacle, counts, is_carving, settle},
    navigation::{LevelNav, bake_triangles},
    physics::Collider,
};
use voxear_nav::{NOT_WALKABLE, PathStatus};
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

/// A field of blocks 32 across and six deep, its corner at the world's,
/// and a wall two blocks high across it at x 16, open from z 28 on.
fn field() -> (tempfile::TempDir, Game) {
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let (dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Obstacles\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
        ("Scenes/Field.scene", scene.into()),
        ("Blocks/Field.blockworld", file.to_text()),
    ]);
    let field = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Blocks").unwrap().0;
    let wall: Vec<([i32; 3], u16)> = (0..28).flat_map(|z| [([16, 6, z], 1), ([16, 7, z], 1)]).collect();
    game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(wall);
    game.tick(FRAME, None);
    (dir, game)
}

/// A level of models: a floor 30 metres square, its top at 0, and a wall
/// across it at x 15 with a gap from z 24 on; its nav mesh baked. A crate
/// stands to one side, an obstacle, and is left out of the bake.
fn level() -> (tempfile::TempDir, Game, Entity) {
    let (dir, mut game) = project(&[
        ("voxear.toml", "[project]\nname = \"Mesh\"\nstart_level = \"Levels/Main.level\"\n".into()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".into()),
        ("Scenes/Main.scene", "format = 1\n".into()),
    ]);
    let block =
        |middle: Vec3, size: [f32; 3]| (Transform::from_position(middle), Collider { size, ..Collider::default() });
    game.engine.world.spawn(block(Vec3::new(15.0, -0.5, 15.0), [30.0, 1.0, 30.0]));
    game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 12.0), [1.0, 3.0, 24.0]));
    let obstacle = NavObstacle { size: [1.5, 1.5, 7.0], ..NavObstacle::default() };
    let (middle, collider) = block(Vec3::new(5.0, 0.75, 27.0), [1.5, 1.5, 7.0]);
    let crate_box = game.engine.world.spawn((middle, collider, obstacle));
    game.tick(FRAME, None);
    let mesh = voxear_nav::mesh::bake(&bake_triangles(&game.engine.world), Default::default()).unwrap();
    game.engine.world.insert_resource(LevelNav(Arc::new(mesh)));
    (dir, game, crate_box)
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

fn ticks(game: &mut Game, frames: usize) {
    for _ in 0..frames {
        game.tick(FRAME, None);
    }
}

/// Sends an agent from `from` to `to`, and after `before` frames does
/// `then`; how its way ends and where it is once it stops, or after `most`
/// frames, and whether it went through the wall's gap.
fn walk(
    game: &mut Game,
    from: Vec3,
    to: Vec3,
    before: usize,
    most: usize,
    then: impl FnOnce(&mut Game),
    gap: impl Fn(Vec3) -> bool,
) -> (PathStatus, Vec3, bool) {
    let walker = game.engine.world.spawn((
        Transform::from_position(from),
        NavAgent { moves: NavMoves::AlongTheNav, speed: 4.0, ..NavAgent::default() },
    ));
    ticks(game, 10);
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    ticks(game, before);
    then(game);
    let mut through = false;
    for _ in 0..most {
        game.tick(FRAME, None);
        through |= gap(at(game, walker));
        let state = game.engine.world.get::<NavAgentState>(walker).unwrap();
        if state.velocity.length() < 0.01
            && state.has_path()
            && !state.pending()
            && state.remaining(at(game, walker)) < 1.0
        {
            break;
        }
    }
    let state = game.engine.world.get::<NavAgentState>(walker).unwrap();
    (state.status, at(game, walker), through)
}

#[test]
fn a_crate_put_in_the_only_gap_of_a_wall_stops_an_agent_short_in_a_world_of_blocks_and_taking_it_away_lets_it_on() {
    let (_dir, mut game) = field();
    let (from, to) = (Vec3::new(6.5, 6.0, 6.5), Vec3::new(26.5, 6.0, 6.5));
    let in_gap = |at: Vec3| (15.5..17.5).contains(&at.x) && at.z > 27.0;
    let open = find_way(&mut game.engine.world, from, to, 1.8);
    assert_eq!(open.0, PathStatus::Complete, "the gap is open at first");
    // Halfway to the gap, a crate is put in it.
    let mut crate_box = None;
    let (status, now, through) = walk(
        &mut game,
        from,
        to,
        90,
        900,
        |game| {
            let middle = Transform::from_position(Vec3::new(16.5, 7.0, 30.0));
            crate_box =
                Some(game.engine.world.spawn((middle, NavObstacle { size: [1.0, 2.0, 4.0], ..Default::default() })));
        },
        in_gap,
    );
    assert_ne!(status, PathStatus::Complete, "no way through now: stopped at {now}");
    // As near the destination as it gets: against the wall, across from it.
    assert!(!through && (14.5..16.0).contains(&now.x), "stopped short of the crate, at the wall: {now}");
    assert_eq!(counts(&game.engine.world), (1, 1));
    let under = area_at(&mut game.engine.world, Vec3::new(16.5, 6.0, 30.5), 0.5);
    assert_eq!(under, Some(NOT_WALKABLE), "under the crate");
    // The crate taken away, the way is open again.
    game.engine.world.despawn(crate_box.unwrap());
    ticks(&mut game, 2);
    assert_eq!(counts(&game.engine.world), (0, 0));
    assert_eq!(find_way(&mut game.engine.world, now, to, 1.8).0, PathStatus::Complete);
}

#[test]
fn an_obstacle_carves_only_once_it_stands_still_and_one_that_never_carves_is_gone_round() {
    let (_dir, mut game) = field();
    let obstacle = NavObstacle { size: [1.0, 2.0, 4.0], ..Default::default() };
    let cart = game.engine.world.spawn((Transform::from_position(Vec3::new(4.5, 7.0, 20.0)), obstacle));
    ticks(&mut game, 2);
    assert!(is_carving(&game.engine.world, cart), "one first seen carves at once");
    // Pushed along a little each frame, it is let go, and carves again half a second after it stops.
    for step in 0..30 {
        game.engine.world.get_mut::<Transform>(cart).unwrap().position.x = 4.5 + step as f32 * 0.1;
        game.tick(FRAME, None);
        // A frame may come before the step that sees it moved.
        assert!(step < 4 || !is_carving(&game.engine.world, cart), "moving, step {step}");
    }
    ticks(&mut game, 20);
    assert!(!is_carving(&game.engine.world, cart), "not yet still for long");
    ticks(&mut game, 20);
    assert!(is_carving(&game.engine.world, cart), "still for half a second");
    game.engine.world.despawn(cart);

    // A post that never carves, in an agent's straight way across the open half of the field.
    let post = NavObstacle { size: [2.0, 2.0, 2.0], carve: false, ..Default::default() };
    game.engine.world.spawn((Transform::from_position(Vec3::new(8.5, 7.0, 6.5)), post));
    let middle = Vec3::new(8.5, 6.0, 6.5);
    let mut nearest = f32::INFINITY;
    let walker = game.engine.world.spawn((
        Transform::from_position(Vec3::new(2.5, 6.0, 6.5)),
        NavAgent { moves: NavMoves::AlongTheNav, speed: 3.0, ..NavAgent::default() },
    ));
    ticks(&mut game, 5);
    let to = Vec3::new(14.5, 6.0, 6.5);
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    for _ in 0..600 {
        game.tick(FRAME, None);
        nearest = nearest.min((at(&game, walker) - middle).length());
    }
    assert_eq!(counts(&game.engine.world), (1, 0));
    assert!((at(&game, walker) - to).length() < 0.6, "got past the post: {}", at(&game, walker));
    assert!(nearest > 1.0, "turned aside round the post: came within {nearest:.2} m of its middle");
}

#[test]
fn a_crate_pushed_into_the_gap_of_a_wall_on_a_levels_nav_mesh_shuts_the_way_and_pushed_out_opens_it() {
    let (_dir, mut game, crate_box) = level();
    let (from, to) = (Vec3::new(5.0, 0.0, 5.0), Vec3::new(25.0, 0.0, 5.0));
    let in_gap = |at: Vec3| (14.0..16.0).contains(&at.x) && at.z > 23.5;
    ticks(&mut game, 2);
    settle(&mut game.engine.world);
    assert!(is_carving(&game.engine.world, crate_box));
    // Standing to one side, the crate leaves the gap open, and the bake left it out: nothing stands where it was.
    assert_eq!(find_way(&mut game.engine.world, from, to, 1.8).0, PathStatus::Complete);
    // As an agent walks to the gap, the crate is pushed into it.
    let (status, now, through) = walk(
        &mut game,
        from,
        to,
        60,
        1200,
        |game| game.engine.world.get_mut::<Transform>(crate_box).unwrap().position = Vec3::new(15.0, 0.75, 27.0),
        in_gap,
    );
    assert_eq!(status, PathStatus::Partial, "no way through now: stopped at {now}");
    assert!(!through && (13.5..15.0).contains(&now.x), "stopped short of the crate, at the wall: {now}");
    // Pushed back out, it opens the way once it stands still again.
    game.engine.world.get_mut::<Transform>(crate_box).unwrap().position = Vec3::new(5.0, 0.75, 27.0);
    ticks(&mut game, 40);
    settle(&mut game.engine.world);
    let (status, corners, _) = find_way(&mut game.engine.world, now, to, 1.8);
    assert_eq!(status, PathStatus::Complete, "{corners:?}");
}
