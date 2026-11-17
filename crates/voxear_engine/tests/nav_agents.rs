//! Nav Agents in a level with a world of blocks: one walks to where it is
//! sent round a wall, by its Character Controller and along the nav; two
//! walking at each other down a corridor pass; a crowd goes through a
//! doorway; one opens a shut door and one jumps a trench; and five hundred
//! cross a square both ways within the frame's time (headless: no GPU, no
//! scripts).

use std::time::{Duration, Instant};

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    nav_agent::{NavAgent, NavAgentState, NavMoves},
    physics::CharacterController,
};
use voxear_nav::PathStatus;
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const FRAME: f32 = 1.0 / 60.0;

/// A field of blocks `size` across and six deep, its corner at the world's.
fn game(size: u32) -> (tempfile::TempDir, Game, Entity) {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [size, size];
    file.ground.height = 6;
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Agents\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", scene.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Blocks/Door.block", "color = [0.5, 0.35, 0.2]\nshape = \"door\"\n".to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let field = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Blocks").unwrap().0;
    game.tick(FRAME, None);
    (dir, game, field)
}

/// An agent standing at `(x, z)` on the field's top, moving as `moves` says.
fn agent(game: &mut Game, x: f32, z: f32, moves: NavMoves) -> Entity {
    let at = Transform::from_position(Vec3::new(x, 6.0, z));
    let agent = NavAgent { moves, speed: 3.0, ..NavAgent::default() };
    match moves {
        NavMoves::CharacterController => game.engine.world.spawn((at, agent, CharacterController::default())),
        _ => game.engine.world.spawn((at, agent)),
    }
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

fn arrived(game: &Game, entity: Entity) -> bool {
    let agent = game.engine.world.get::<NavAgent>(entity).unwrap();
    game.engine.world.get::<NavAgentState>(entity).is_some_and(|state| state.is_arrived(agent))
}

/// Puts a wall of stone two blocks high on the field from `from` to `to` (inclusive), across x or z.
fn wall(game: &mut Game, field: Entity, from: [i32; 2], to: [i32; 2]) {
    let mut blocks = Vec::new();
    for x in from[0]..=to[0] {
        for z in from[1]..=to[1] {
            blocks.push(([x, 6, z], 1));
            blocks.push(([x, 7, z], 1));
        }
    }
    game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(blocks);
}

#[test]
fn an_agent_walks_round_a_wall_to_where_it_is_sent_by_its_controller_and_along_the_nav() {
    for moves in [NavMoves::CharacterController, NavMoves::AlongTheNav] {
        let (_dir, mut game, field) = game(32);
        // A wall across the field, with a gap at its far end.
        wall(&mut game, field, [16, 0], [16, 27]);
        game.tick(FRAME, None);
        let walker = agent(&mut game, 6.5, 6.5, moves);
        for _ in 0..30 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(26.5, 6.0, 6.5);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let mut frames = 0;
        let mut nearest_wall = f32::INFINITY;
        while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
            game.tick(FRAME, None);
            frames += 1;
            let now = at(&game, walker);
            if now.z < 27.5 {
                nearest_wall = nearest_wall.min((now.x - 16.5).abs());
            }
        }
        let now = at(&game, walker);
        assert!(frames < 1800, "{moves:?}: it got there, at {now} after {frames} frames");
        assert!((now - to).length() < 0.5, "{moves:?}: {now}");
        assert!((now.y - 6.0).abs() < 0.1, "{moves:?}: on the field's top: {now}");
        assert!(nearest_wall > 0.3, "{moves:?}: never through the wall: {nearest_wall}");
        let state = game.engine.world.get::<NavAgentState>(walker).unwrap();
        assert_eq!(state.status, PathStatus::Complete);
        // Round the end of the wall: some 20 blocks across and 40 along at 3 metres a second.
        assert!(frames > 60 * 12, "{moves:?}: on foot, not at once: {frames} frames");
    }
}

#[test]
fn two_agents_walking_at_each_other_down_a_corridor_both_get_past() {
    let (_dir, mut game, field) = game(32);
    // A corridor three blocks wide along x.
    wall(&mut game, field, [2, 10], [29, 10]);
    wall(&mut game, field, [2, 14], [29, 14]);
    game.tick(FRAME, None);
    let (a, b) = (
        agent(&mut game, 4.5, 12.5, NavMoves::CharacterController),
        agent(&mut game, 27.5, 12.5, NavMoves::CharacterController),
    );
    for _ in 0..30 {
        game.tick(FRAME, None);
    }
    game.engine.world.get_mut::<NavAgent>(a).unwrap().go_to(Vec3::new(27.5, 6.0, 12.5));
    game.engine.world.get_mut::<NavAgent>(b).unwrap().go_to(Vec3::new(4.5, 6.0, 12.5));
    let mut closest = f32::INFINITY;
    for _ in 0..900 {
        game.tick(FRAME, None);
        let (pa, pb) = (at(&game, a), at(&game, b));
        closest = closest.min(Vec3::new(pa.x - pb.x, 0.0, pa.z - pb.z).length());
        if arrived(&game, a) && arrived(&game, b) {
            break;
        }
    }
    assert!(arrived(&game, a) && arrived(&game, b), "both got there: {} and {}", at(&game, a), at(&game, b));
    assert!(closest > 0.5, "they stepped round each other: {closest}");
}

#[test]
fn a_crowd_goes_through_a_doorway_and_none_is_left_stuck() {
    let (_dir, mut game, field) = game(48);
    // A wall across the field with a doorway two blocks wide in its middle.
    wall(&mut game, field, [24, 0], [24, 22]);
    wall(&mut game, field, [24, 25], [24, 47]);
    game.tick(FRAME, None);
    let mut crowd = Vec::new();
    for row in 0..5 {
        for column in 0..4 {
            let (x, z) = (8.5 + column as f32 * 2.0, 18.5 + row as f32 * 2.5);
            let walker = agent(&mut game, x, z, NavMoves::AlongTheNav);
            crowd.push((walker, Vec3::new(40.5 - column as f32 * 2.0, 6.0, z)));
        }
    }
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    for (walker, to) in &crowd {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(*to);
    }
    let mut frames = 0;
    while frames < 60 * 40 && !crowd.iter().all(|(walker, _)| arrived(&game, *walker)) {
        game.tick(FRAME, None);
        frames += 1;
    }
    let left: Vec<Vec3> =
        crowd.iter().filter(|(walker, _)| !arrived(&game, *walker)).map(|(walker, _)| at(&game, *walker)).collect();
    assert!(left.is_empty(), "after {frames} frames, still on their way: {left:?}");
    for (walker, to) in &crowd {
        let now = at(&game, *walker);
        assert!(now.x > 24.5, "through the doorway: {now}");
        assert!((now - *to).length() < 1.5, "near where it was sent: {now} for {to}");
    }
}

#[test]
fn five_hundred_agents_crossing_a_square_both_ways_take_little_of_the_frame() {
    let (_dir, mut game, _) = game(96);
    let mut agents = Vec::new();
    for row in 0..25 {
        for column in 0..10 {
            let (x, z) = (4.5 + column as f32 * 1.5, 10.5 + row as f32 * 3.0);
            agents.push((agent(&mut game, x, z, NavMoves::AlongTheNav), Vec3::new(91.5 - column as f32 * 1.5, 6.0, z)));
            agents.push((
                agent(&mut game, 91.5 - column as f32 * 1.5, z + 1.5, NavMoves::AlongTheNav),
                Vec3::new(x, 6.0, z + 1.5),
            ));
        }
    }
    for _ in 0..5 {
        game.tick(FRAME, None);
    }
    for (walker, to) in &agents {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(*to);
    }
    // Past the first searches: the time of a step with them walking and avoiding.
    for _ in 0..20 {
        game.tick(FRAME, None);
    }
    let mut least = Duration::MAX;
    for _ in 0..60 {
        let began = Instant::now();
        voxear_engine::nav_agent::step_agents(&mut game.engine.world, FRAME);
        least = least.min(began.elapsed());
        game.tick(FRAME, None);
    }
    let ms = least.as_secs_f64() * 1000.0;
    println!("{} agents along the nav: a step in {ms:.3} ms at least", agents.len());
    let slack = if cfg!(debug_assertions) { 4.0 } else { 1.0 };
    assert!(ms < 2.0 * slack, "{ms:.3} ms");
    let moving = agents
        .iter()
        .filter(|(walker, _)| {
            game.engine.world.get::<NavAgentState>(*walker).is_some_and(|state| state.velocity.length() > 0.5)
        })
        .count();
    assert!(moving > agents.len() / 2, "most of them on their way: {moving}");
}

/// Puts a door, shut, at `at`, with its upper block over it, in a wall along z: walked through along x.
fn door(game: &mut Game, field: Entity, at: [i32; 3]) {
    use voxear_engine::blocks::{block_number_state, with_door_halves};
    use voxear_terrain::blocks::shape::{BlockFacing, BlockState};
    let state = BlockState { facing: BlockFacing::East, ..BlockState::default() }.to_byte();
    let number =
        block_number_state(&mut game.engine.world, &mut game.content, field, "Blocks/Door.block", state, true).unwrap();
    let changes = with_door_halves(&mut game.engine.world, &mut game.content, field, vec![(at, number)]);
    game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(changes);
}

/// Sends `walker` to `to` and ticks until it is there, or `most` frames have gone: how many went.
fn walk(game: &mut Game, walker: Entity, to: Vec3, most: usize) -> usize {
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    let mut frames = 0;
    while frames < most && !(frames > 10 && arrived(game, walker)) {
        game.tick(FRAME, None);
        frames += 1;
    }
    frames
}

#[test]
fn an_agent_opens_a_shut_door_on_its_way_and_one_that_opens_none_stops_at_the_wall() {
    for opens in [true, false] {
        let (_dir, mut game, field) = game(32);
        // A wall across the field, its only way through a shut door.
        wall(&mut game, field, [16, 0], [16, 7]);
        wall(&mut game, field, [16, 9], [16, 31]);
        door(&mut game, field, [16, 6, 8]);
        game.tick(FRAME, None);
        let walker = agent(&mut game, 6.5, 8.5, NavMoves::CharacterController);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().opens_doors = opens;
        let to = Vec3::new(26.5, 6.0, 8.5);
        let frames = walk(&mut game, walker, to, 1200);
        let now = at(&game, walker);
        let state = game.engine.world.get::<NavAgentState>(walker).unwrap().status;
        if opens {
            assert!(frames < 1200 && (now - to).length() < 0.5, "it went through the door: {now} after {frames}");
            assert_eq!(state, PathStatus::Complete);
            let blocks = game.engine.world.get::<BlockWorld>(field).unwrap();
            let open = voxear_terrain::blocks::shape::BlockState::from_byte(
                blocks.faces[usize::from(blocks.blocks.get([16, 6, 8]))].state,
            )
            .open;
            assert!(open, "and the door stands open");
        } else {
            assert!(now.x < 16.0, "it never got past the wall: {now}");
            assert_eq!(state, PathStatus::Partial, "as near as it could get");
        }
    }
}

#[test]
fn an_agent_jumps_a_trench_by_its_controller_and_along_the_nav() {
    for moves in [NavMoves::CharacterController, NavMoves::AlongTheNav] {
        let (_dir, mut game, field) = game(32);
        // A trench across the field, a block wide and four deep.
        let mut dug = Vec::new();
        for z in 0..32 {
            for y in 2..6 {
                dug.push(([16, y, z], 0));
            }
        }
        game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(dug);
        game.tick(FRAME, None);
        let walker = agent(&mut game, 8.5, 8.5, moves);
        let to = Vec3::new(24.5, 6.0, 8.5);
        let mut lowest = f32::INFINITY;
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let mut frames = 0;
        while frames < 900 && !(frames > 10 && arrived(&game, walker)) {
            game.tick(FRAME, None);
            frames += 1;
            lowest = lowest.min(at(&game, walker).y);
        }
        let now = at(&game, walker);
        assert!(frames < 900 && (now - to).length() < 0.5, "{moves:?}: across the trench: {now} after {frames}");
        assert!(lowest > 5.5, "{moves:?}: over it, never down in it: {lowest}");
    }
}

#[test]
fn an_agent_goes_round_a_nav_area_nothing_walks_on() {
    use voxear_engine::navigation::NavArea;
    use voxear_terrain::blocks::block::BlockNavArea;
    let (_dir, mut game, _) = game(32);
    // A box nothing walks on across the field, its far end open.
    let middle = Transform::from_position(Vec3::new(16.0, 6.5, 11.0));
    game.engine.world.spawn((middle, NavArea { size: [4.0, 3.0, 22.0], area: BlockNavArea::NotWalkable }));
    let walker = agent(&mut game, 6.5, 8.5, NavMoves::AlongTheNav);
    let to = Vec3::new(26.5, 6.0, 8.5);
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    let mut frames = 0;
    let mut inside = false;
    while frames < 1800 && !(frames > 10 && arrived(&game, walker)) {
        game.tick(FRAME, None);
        frames += 1;
        let now = at(&game, walker);
        inside |= (14.0..18.0).contains(&now.x) && now.z < 22.0;
    }
    let now = at(&game, walker);
    assert!(frames < 1800 && (now - to).length() < 0.5, "it got there: {now} after {frames}");
    assert!(!inside, "never through the box");
}
