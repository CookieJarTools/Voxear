//! Crowds: Nav Agents and their tiers by how near they are to the game's
//! camera, a Crowd Member's own tier and importance, and agents far off
//! planning and moving less often yet getting where they are sent, in less
//! of the step (headless: no GPU, no scripts).

use std::time::{Duration, Instant};

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    crowds::{CrowdMember, CrowdSettings, CrowdTier, CrowdTierChoice, CrowdTiers},
    nav_agent::{NavAgent, NavAgentState, NavMoves},
    physics::CharacterController,
};
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
        ("Blocks/Vine.block", "color = [0.2, 0.5, 0.2]\nshape = \"vine\"\n".to_string()),
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

/// The game's camera, at `at`.
fn camera(game: &mut Game, at: Vec3) -> Entity {
    game.engine.world.spawn((voxear_render::Camera::default(), Transform::new(at, Quat::IDENTITY, Vec3::ONE)))
}

fn tier(game: &Game, entity: Entity) -> CrowdTier {
    game.engine.world.resource::<CrowdTiers>().map(|tiers| tiers.of(entity)).unwrap_or_default()
}

#[test]
fn agents_are_near_middle_or_far_by_their_distance_unless_a_crowd_member_says() {
    let (_dir, mut game, _) = game(32);
    camera(&mut game, Vec3::new(4.5, 8.0, 4.5));
    let near = agent(&mut game, 10.5, 4.5, NavMoves::AlongTheNav);
    let middle = agent(&mut game, 4.5, 104.5, NavMoves::AlongTheNav);
    let far = agent(&mut game, 304.5, 4.5, NavMoves::AlongTheNav);
    let kept_near = agent(&mut game, 304.5, 8.5, NavMoves::AlongTheNav);
    game.engine.world.insert(kept_near, CrowdMember { tier: CrowdTierChoice::Near, ..CrowdMember::default() });
    let important = agent(&mut game, 4.5, 104.5, NavMoves::AlongTheNav);
    game.engine.world.insert(important, CrowdMember { importance: 4.0, ..CrowdMember::default() });
    game.tick(FRAME, None);
    assert_eq!(tier(&game, near), CrowdTier::Near);
    assert_eq!(tier(&game, middle), CrowdTier::Middle);
    assert_eq!(tier(&game, far), CrowdTier::Far);
    assert_eq!(tier(&game, kept_near), CrowdTier::Near, "its own tier");
    assert_eq!(tier(&game, important), CrowdTier::Near, "a hundred metres off, at four times the importance");
    let counts = game.engine.world.resource::<CrowdTiers>().unwrap().counts;
    assert_eq!(counts, [3, 1, 1]);
    // Tiers switched off, or with no camera at all, every one is near.
    CrowdSettings { enabled: false, ..CrowdSettings::default() }.put(&mut game.engine.world);
    game.tick(FRAME, None);
    assert_eq!(game.engine.world.resource::<CrowdTiers>().unwrap().counts, [5, 0, 0]);
}

/// Sends a crowd of twenty across the field, as the camera at `camera_at` sees
/// it, and gives back how many frames until every one is there.
fn crowd_crossing(moves: NavMoves, camera_at: Vec3) -> usize {
    let (_dir, mut game, _) = game(32);
    camera(&mut game, camera_at);
    let crowd: Vec<Entity> = (0..20).map(|n| agent(&mut game, 2.5 + n as f32 * 1.4, 4.5, moves)).collect();
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    for (n, walker) in crowd.iter().enumerate() {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(Vec3::new(2.5 + n as f32 * 1.4, 6.0, 26.5));
    }
    let mut frames = 0;
    while frames < 1800 && !crowd.iter().all(|walker| arrived(&game, *walker)) {
        game.tick(FRAME, None);
        frames += 1;
    }
    for walker in &crowd {
        let now = at(&game, *walker);
        assert!(arrived(&game, *walker), "{moves:?}: every one arrived, this one at {now} after {frames} frames");
        assert!(now.z > 25.5, "{moves:?}: across the field: {now}");
        assert!((now.y - 6.0).abs() < 0.2, "{moves:?}: on its top: {now}");
    }
    frames
}

#[test]
fn far_agents_plan_and_move_less_often_yet_get_where_they_are_sent_as_soon() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let near = crowd_crossing(moves, Vec3::new(16.0, 10.0, 16.0));
        let far = crowd_crossing(moves, Vec3::new(616.0, 10.0, 16.0));
        // 22 metres at 3 metres a second: about 7 seconds either way.
        let slower = far as f32 / near as f32;
        assert!((0.9..1.15).contains(&slower), "{moves:?}: near {near} frames, far {far}");
    }
}

/// How long a step of two thousand agents crossing a square both ways takes at least, all far from the camera, or
/// all near with `tiers` off.
fn crowd_step(moves: NavMoves, tiers: bool) -> f64 {
    let size = 316u32;
    let (_dir, mut game, _) = game(size);
    camera(&mut game, Vec3::new(3000.0, 10.0, 158.0));
    CrowdSettings { enabled: tiers, ..CrowdSettings::default() }.put(&mut game.engine.world);
    let far = size as f32 - 4.5;
    let mut agents = Vec::new();
    for row in 0..100 {
        for column in 0..10 {
            let (x, z) = (4.5 + column as f32 * 1.5, 4.5 + row as f32 * 3.0);
            agents.push((agent(&mut game, x, z, moves), Vec3::new(far - column as f32 * 1.5, 6.0, z)));
            agents.push((agent(&mut game, far - column as f32 * 1.5, z + 1.5, moves), Vec3::new(x, 6.0, z + 1.5)));
        }
    }
    for _ in 0..5 {
        game.tick(FRAME, None);
    }
    for (walker, to) in &agents {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(*to);
    }
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
    let moving = agents
        .iter()
        .filter(|(walker, _)| {
            game.engine.world.get::<NavAgentState>(*walker).is_some_and(|s| s.velocity.length() > 0.5)
        })
        .count();
    assert!(moving > agents.len() / 2, "{moves:?}: most of them on their way: {moving}");
    least.as_secs_f64() * 1000.0
}

#[test]
fn two_thousand_far_agents_take_less_of_the_step_than_near_ones() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let near = crowd_step(moves, false);
        let far = crowd_step(moves, true);
        println!("2000 agents {moves:?}: a step in {near:.3} ms near, {far:.3} ms far, at least");
        assert!(far < near * 0.8, "{moves:?}: {far:.3} ms far against {near:.3} ms near");
    }
}
