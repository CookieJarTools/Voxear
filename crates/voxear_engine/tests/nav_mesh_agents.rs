//! Nav Agents on a level made of colliders rather than blocks: its nav mesh
//! baked from its still colliders, kept and read back as a `.nav` file's
//! bytes, and agents walking round a wall on it, by their Character
//! Controllers and along the mesh (headless: no GPU, no scripts).

use std::sync::Arc;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    nav_agent::{NavAgent, NavAgentState, NavMoves},
    navigation::{LevelNav, bake_triangles},
    physics::{CharacterController, Collider},
};
use voxear_nav::mesh::{BakeSettings, NavMesh, bake};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;

/// An empty level with a floor 30 metres square, its top at 0, and a wall across it with a gap at one end.
fn game() -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Mesh\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n"),
        ("Scenes/Main.scene", "format = 1\n"),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let block =
        |middle: Vec3, size: [f32; 3]| (Transform::from_position(middle), Collider { size, ..Collider::default() });
    game.engine.world.spawn(block(Vec3::new(15.0, -0.5, 15.0), [30.0, 1.0, 30.0]));
    game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 12.0), [1.0, 3.0, 24.0]));
    game.tick(FRAME, None);
    // Baked, kept as a file's bytes, and read back, as the editor's bake and the level opening do.
    let mesh = bake(&bake_triangles(&game.engine.world), BakeSettings::default()).expect("something to stand on");
    let read = NavMesh::from_bytes(&mesh.to_bytes()).unwrap();
    game.engine.world.insert_resource(LevelNav(Arc::new(read)));
    (dir, game)
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

#[test]
fn agents_walk_round_a_wall_on_a_levels_baked_nav_mesh() {
    for moves in [NavMoves::CharacterController, NavMoves::AlongTheNav] {
        let (_dir, mut game) = game();
        let agent = NavAgent { moves, speed: 3.0, ..NavAgent::default() };
        let start = Transform::from_position(Vec3::new(5.0, 0.0, 5.0));
        let walker = match moves {
            NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
            _ => game.engine.world.spawn((start, agent)),
        };
        for _ in 0..20 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(25.0, 0.0, 5.0);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let (mut frames, mut through) = (0, false);
        let arrived = |game: &Game| {
            let agent = game.engine.world.get::<NavAgent>(walker).unwrap();
            game.engine.world.get::<NavAgentState>(walker).is_some_and(|state| state.is_arrived(agent))
        };
        while frames < 2400 && !(frames > 10 && arrived(&game)) {
            game.tick(FRAME, None);
            frames += 1;
            let now = at(&game, walker);
            through |= (14.5..15.5).contains(&now.x) && now.z < 23.5;
        }
        let now = at(&game, walker);
        assert!(frames < 2400 && (now - to).length() < 0.5, "{moves:?}: round the wall: {now} after {frames} frames");
        assert!(!through, "{moves:?}: never through the wall");
        assert!(now.y.abs() < 0.1, "{moves:?}: on the floor: {now}");
    }
}

#[test]
fn agents_walk_round_a_wall_and_up_a_hill_on_a_nav_mesh_baked_from_colliders_and_a_terrain() {
    use voxear_engine::navigation::bake_grounds;
    use voxear_terrain::{Heightmap, Terrain};
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            ("voxear.toml", "[project]\nname = \"Hill\"\nstart_level = \"Levels/Main.level\"\n"),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n"),
            ("Scenes/Main.scene", "format = 1\n"),
        ];
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
        game.start(None).unwrap();
        // Ground 64 metres square, flat, then rising 6 m from x 20 to 44; a wall standing on it across most of z.
        let hill = |x: f32| ((x - 20.0).clamp(0.0, 24.0)) * 0.25;
        let heights = Heightmap::from_fn(1, 1, 0.25, |x, _| hill(x));
        game.engine.world.spawn((Terrain::new(heights), Transform::default()));
        let wall = Collider { size: [1.0, 3.0, 50.0], ..Collider::default() };
        game.engine.world.spawn((Transform::from_position(Vec3::new(10.0, 1.5, 25.0)), wall));
        game.tick(FRAME, None);
        let grounds = bake_grounds(&game.engine.world);
        assert_eq!(grounds.len(), 1, "the terrain is ground to bake");
        let mesh =
            voxear_nav::mesh::bake_ground(&bake_triangles(&game.engine.world), &grounds, BakeSettings::default())
                .expect("something to stand on");
        game.engine.world.insert_resource(LevelNav(Arc::new(mesh)));
        let agent = NavAgent { moves, speed: 4.0, ..NavAgent::default() };
        let start = Transform::from_position(Vec3::new(4.0, 0.0, 5.0));
        let walker = match moves {
            NavMoves::CharacterController => game.engine.world.spawn((start, agent, CharacterController::default())),
            _ => game.engine.world.spawn((start, agent)),
        };
        for _ in 0..20 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(55.0, hill(55.0), 5.0);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let (mut frames, mut through) = (0, false);
        let arrived = |game: &Game| {
            let agent = game.engine.world.get::<NavAgent>(walker).unwrap();
            game.engine.world.get::<NavAgentState>(walker).is_some_and(|state| state.is_arrived(agent))
        };
        while frames < 3600 && !(frames > 10 && arrived(&game)) {
            game.tick(FRAME, None);
            frames += 1;
            let now = at(&game, walker);
            through |= (9.5..10.5).contains(&now.x) && now.z < 49.5;
        }
        let now = at(&game, walker);
        assert!((now - to).length() < 0.6, "{moves:?}: round the wall and up the hill: {now} after {frames} frames");
        assert!(!through, "{moves:?}: never through the wall");
    }
}
