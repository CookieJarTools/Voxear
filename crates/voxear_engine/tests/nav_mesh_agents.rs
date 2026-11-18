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

#[test]
fn an_agent_type_too_wide_for_a_doorway_walks_its_own_baked_mesh_round_by_a_wider_gap() {
    use voxear_engine::{
        nav_obstacle::NavObstacle,
        navigation::{baked_types, level_nav_for, put_meshes},
    };
    use voxear_nav::mesh::{bake, meshes_from_bytes, meshes_to_bytes};
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Ogres\"\nstart_level = \"Levels/Main.level\"\n"),
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
    // The project's first own type, an ogre, is far wider than the Default type.
    let navigation = &mut game.project.file.navigation;
    (navigation.agent_1_name, navigation.agent_1_radius) = ("Ogre".into(), 0.8);
    let settings = game.project.file.navigation.clone();
    settings.put(&mut game.engine.world);
    // A floor 30 m square and a wall across it at x 15: a doorway 1.6 m wide at z 5, and a gap 4 m wide at z 24.
    let block =
        |middle: Vec3, size: [f32; 3]| (Transform::from_position(middle), Collider { size, ..Collider::default() });
    game.engine.world.spawn(block(Vec3::new(15.0, -0.5, 15.0), [30.0, 1.0, 30.0]));
    game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 2.1), [1.0, 3.0, 4.2]));
    game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 13.9), [1.0, 3.0, 16.2]));
    game.engine.world.spawn(block(Vec3::new(15.0, 1.5, 28.0), [1.0, 3.0, 4.0]));
    game.tick(FRAME, None);
    // Baked for each, kept as a file's bytes and read back, as the editor's bake and the level opening do.
    let triangles = bake_triangles(&game.engine.world);
    let types = settings.agent_types();
    let meshes: Vec<(u8, NavMesh)> =
        [0u8, 1].map(|slot| (slot, bake(&triangles, types[usize::from(slot)].bake_settings()).unwrap())).into();
    put_meshes(&mut game.engine.world, meshes_from_bytes(&meshes_to_bytes(&meshes)).unwrap());
    assert_eq!(baked_types(&game.engine.world), [0, 1]);
    let mut gaps = Vec::new();
    for slot in [0u8, 1] {
        let agent = NavAgent { moves: NavMoves::AlongTheNav, speed: 4.0, agent_type: slot, ..NavAgent::default() };
        let walker = game.engine.world.spawn((Transform::from_position(Vec3::new(5.0, 0.0, 5.0)), agent));
        for _ in 0..10 {
            game.tick(FRAME, None);
        }
        let to = Vec3::new(25.0, 0.0, 5.0);
        game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
        let (mut frames, mut through) = (0, None);
        let arrived = |game: &Game| {
            let agent = game.engine.world.get::<NavAgent>(walker).unwrap();
            game.engine.world.get::<NavAgentState>(walker).is_some_and(|state| state.is_arrived(agent))
        };
        while frames < 2400 && !(frames > 10 && arrived(&game)) {
            game.tick(FRAME, None);
            frames += 1;
            let now = at(&game, walker);
            if (14.5..15.5).contains(&now.x) {
                through = Some(now.z);
            }
        }
        let now = at(&game, walker);
        assert!((now - to).length() < 0.6, "type {slot}: got there: {now} after {frames} frames");
        gaps.push(through.expect("through the wall somewhere"));
        game.engine.world.despawn(walker);
    }
    assert!(gaps[0] < 6.0 && gaps[1] > 21.5, "the Default type by the doorway, the ogre by the wide gap: {gaps:?}");
    // A crate carved into the wide gap cuts the ogre's mesh too: its only way through is shut.
    let ogre_way = |game: &Game| {
        let (mesh, offset) = level_nav_for(&game.engine.world, 1).unwrap();
        mesh.find_path(Vec3::new(5.0, 0.0, 5.0) + offset, Vec3::new(25.0, 0.0, 5.0) + offset, 1.0).status
    };
    assert_eq!(ogre_way(&game), voxear_nav::mesh::MeshPathStatus::Complete);
    let crate_box = NavObstacle { size: [1.0, 1.0, 4.0], ..NavObstacle::default() };
    game.engine.world.spawn((Transform::from_position(Vec3::new(15.0, 0.5, 24.0)), crate_box));
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    voxear_engine::nav_obstacle::settle(&mut game.engine.world);
    assert_eq!(ogre_way(&game), voxear_nav::mesh::MeshPathStatus::Partial, "the ogre's mesh is cut round it");
}
