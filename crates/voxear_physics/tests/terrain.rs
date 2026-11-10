//! Terrain in physics: what falls lands on its ground, rays find it, and a
//! character walks over it.

use glam::Vec3;
use voxear_ecs::World;
use voxear_physics::scene::{CharacterController, Collider, ColliderShape, PhysicsScene, Rigidbody};
use voxear_scene::Transform;
use voxear_terrain::{HeightTile, Heightmap, Terrain};

fn hills() -> Heightmap {
    Heightmap::from_fn(2, 2, 0.5, |x, z| (x * 0.05).sin() * 3.0 + (z * 0.04).cos() * 2.0)
}

fn world_with(heights: Heightmap, corner: Vec3) -> (World, voxear_ecs::Entity) {
    let mut world = World::new();
    voxear_scene::register_obj_components(&mut world);
    let terrain = world.spawn((Terrain::new(heights), Transform::from_position(corner)));
    (world, terrain)
}

#[test]
fn a_ball_dropped_on_a_terrain_comes_to_rest_on_its_ground() {
    let corner = Vec3::new(-100.0, 2.0, -50.0);
    let (mut world, terrain) = world_with(hills(), corner);
    // Over the second tile, a little way in.
    let (x, z) = (40.0, 30.0);
    let ball = world.spawn((Transform::from_position(Vec3::new(x, 30.0, z)),));
    world.insert(ball, Collider { shape: ColliderShape::Sphere, radius: 0.5, ..Collider::default() });
    world.insert(ball, Rigidbody { angular_drag: 2.0, ..Rigidbody::default() });
    let mut physics = PhysicsScene::new();
    assert!(PhysicsScene::wanted(&world));
    for _ in 0..240 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let at = world.get::<Transform>(ball).unwrap().position;
    let ground = voxear_terrain::ground_at(&world, at.x, at.z).expect("ground under it");
    assert_eq!(ground.terrain, terrain);
    assert!(
        at.y - ground.height > 0.3 && at.y - ground.height < 0.7,
        "resting on the ground: {} over {}",
        at.y,
        ground.height
    );

    let hit = physics.raycast(Vec3::new(x, 50.0, z), Vec3::NEG_Y, 100.0, u32::MAX, false).expect("the ground");
    assert_eq!(hit.obj, terrain);
    let expected = voxear_terrain::ground_at(&world, x, z).unwrap().height;
    assert!((hit.point.y - expected).abs() < 0.05, "physics' ground is the terrain's: {} and {expected}", hit.point.y);
}

#[test]
fn a_character_walks_up_a_slope_on_the_ground() {
    let slope = Heightmap::from_fn(1, 1, 0.5, |x, _| x * 0.2);
    let (mut world, _) = world_with(slope, Vec3::ZERO);
    let start = Vec3::new(20.0, 4.05, 64.0);
    let walker = world.spawn((Transform::from_position(start),));
    world.insert(walker, CharacterController::default());
    let mut physics = PhysicsScene::new();
    physics.step(&mut world, 1.0 / 60.0);
    for _ in 0..120 {
        physics.move_character(&mut world, walker, Vec3::new(2.0, -2.0, 0.0), 1.0 / 60.0);
    }
    let at = world.get::<Transform>(walker).unwrap().position;
    assert!(at.x > 23.0, "walked up the slope: {at}");
    let ground = 0.2 * at.x;
    assert!((at.y - ground).abs() < 0.2, "on the ground, at {} where it is {ground}", at.y);
}

#[test]
fn a_changed_tile_and_a_moved_terrain_move_the_ground_with_them() {
    let (mut world, terrain) = world_with(Heightmap::flat(2, 1, 0.5, 0.0), Vec3::ZERO);
    let mut physics = PhysicsScene::new();
    physics.step(&mut world, 1.0 / 60.0);
    let down = |physics: &PhysicsScene, x: f32| {
        physics.raycast(Vec3::new(x, 50.0, 10.0), Vec3::NEG_Y, 100.0, u32::MAX, false).map(|hit| hit.point.y)
    };
    assert!(down(&physics, 150.0).is_some_and(|y| y.abs() < 0.01));
    // The second tile raised 5 m; the first left as it was.
    let raised = world.get::<Terrain>(terrain).unwrap().heights.with_tiles([(1, 0, HeightTile::flat(5.0))]);
    world.get_mut::<Terrain>(terrain).unwrap().heights = std::sync::Arc::new(raised);
    physics.step(&mut world, 1.0 / 60.0);
    assert!(down(&physics, 150.0).is_some_and(|y| (y - 5.0).abs() < 0.01));
    assert!(down(&physics, 50.0).is_some_and(|y| y.abs() < 0.01));
    // The Obj moved up and along: its ground goes with it.
    world.get_mut::<Transform>(terrain).unwrap().position = Vec3::new(10.0, 1.0, 0.0);
    physics.step(&mut world, 1.0 / 60.0);
    assert!(down(&physics, 55.0).is_some_and(|y| (y - 1.0).abs() < 0.01));
    assert_eq!(down(&physics, 5.0), None, "nothing left before its new corner");
    // Gone, it takes its ground.
    world.despawn(terrain);
    physics.step(&mut world, 1.0 / 60.0);
    assert_eq!(down(&physics, 55.0), None);
}

#[test]
fn a_ball_falls_through_a_hole_and_one_beside_it_stays_on_the_ground() {
    // Flat ground half a metre apart, with a hole 3 m across round (40, 40).
    let mut bits = vec![0u32; voxear_terrain::heights::HOLE_WORDS];
    let side = voxear_terrain::TILE_SAMPLES;
    for z in 0..side {
        for x in 0..side {
            let (dx, dz) = (x as f32 * 0.5 - 40.0, z as f32 * 0.5 - 40.0);
            if dx * dx + dz * dz <= 9.0 {
                let index = (z * side + x) as usize;
                bits[index / 32] |= 1 << (index % 32);
            }
        }
    }
    let tile = HeightTile::flat(0.0).with_holes(Some(bits.into()));
    let heights = Heightmap::from_tiles(1, 1, 0.5, vec![std::sync::Arc::new(tile)]);
    let (mut world, _) = world_with(heights, Vec3::ZERO);
    let drop = |world: &mut World, x: f32| {
        let ball = world.spawn((Transform::from_position(Vec3::new(x, 3.0, 40.0)),));
        world.insert(ball, Collider { shape: ColliderShape::Sphere, radius: 0.5, ..Collider::default() });
        world.insert(ball, Rigidbody::default());
        ball
    };
    let (through, beside) = (drop(&mut world, 40.0), drop(&mut world, 50.0));
    let mut physics = PhysicsScene::new();
    for _ in 0..120 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let y = |ball| world.get::<Transform>(ball).unwrap().position.y;
    assert!(y(through) < -3.0, "fell through: {}", y(through));
    assert!((y(beside) - 0.5).abs() < 0.1, "rests beside it: {}", y(beside));
    assert!(physics.raycast(Vec3::new(40.0, 5.0, 40.0), Vec3::NEG_Y, 20.0, u32::MAX, false).is_none());
    assert!(voxear_terrain::ground_at(&world, 40.0, 40.0).is_none());
}

#[test]
fn a_ball_lands_on_a_pits_floor_and_a_character_walks_along_a_trench() {
    use voxear_terrain::runtime::{Edit, apply};
    // Flat ground, a pit 2.5 m round dug out of it, and a trench along x beside it, its floor 2.2 m down.
    let mut terrain = Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0));
    apply(&mut terrain, &Edit::Dig { at: [40.0, 0.0, 40.0], radius: 2.5, surface: None });
    let trench = Edit::Tunnel { from: [20.0, -1.0, 64.0], to: [44.0, -1.0, 64.0], radius: 1.2, surface: None };
    apply(&mut terrain, &trench);
    let mut world = World::new();
    voxear_scene::register_obj_components(&mut world);
    world.spawn((terrain, Transform::default()));
    let drop = |world: &mut World, x: f32| {
        let ball = world.spawn((Transform::from_position(Vec3::new(x, 3.0, 40.0)),));
        world.insert(ball, Collider { shape: ColliderShape::Sphere, radius: 0.5, ..Collider::default() });
        world.insert(ball, Rigidbody { angular_drag: 2.0, ..Rigidbody::default() });
        ball
    };
    let (into, beside) = (drop(&mut world, 40.0), drop(&mut world, 50.0));
    let walker = world.spawn((Transform::from_position(Vec3::new(22.0, -2.15, 64.0)),));
    world.insert(walker, CharacterController::default());
    let mut physics = PhysicsScene::new();
    for _ in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let y = |world: &World, ball| world.get::<Transform>(ball).unwrap().position.y;
    assert!((y(&world, into) + 2.0).abs() < 0.15, "rests in the pit's bowl: {}", y(&world, into));
    assert!((y(&world, beside) - 0.5).abs() < 0.1, "rests on the ground beside it: {}", y(&world, beside));
    // Beside the ball, 2.1 m from the middle, where the bowl is 1.32 m deep.
    let hit = physics.raycast(Vec3::new(38.5, 5.0, 41.5), Vec3::NEG_Y, 20.0, u32::MAX, false).expect("the pit's floor");
    assert!((hit.point.y + 1.32).abs() < 0.1, "a ray meets its side: {}", hit.point.y);
    let ground = voxear_terrain::ground_at(&world, 40.0, 40.0).expect("ground in the pit");
    assert!((ground.height + 2.5).abs() < 0.1, "and so does the ground under it: {}", ground.height);

    for _ in 0..120 {
        physics.move_character(&mut world, walker, Vec3::new(2.0, -2.0, 0.0), 1.0 / 60.0);
    }
    let at = world.get::<Transform>(walker).unwrap().position;
    assert!(at.x > 25.0, "walked along the trench: {at}");
    assert!((at.y + 2.2).abs() < 0.25, "on its floor: {at}");
}

#[test]
fn a_terrain_without_ends_ground_stays_where_it_is_as_its_window_moves() {
    use std::{collections::HashMap, sync::Arc};
    // Each tile of the world flat at its own height, 1 m apart for each tile along x and 10 m along z.
    let mut made: HashMap<[i32; 2], Arc<HeightTile>> = HashMap::new();
    let mut tile = |at: [i32; 2]| {
        made.entry(at).or_insert_with(|| Arc::new(HeightTile::flat(at[0] as f32 + 10.0 * at[1] as f32))).clone()
    };
    let mut window = |origin: [i32; 2], unmade: Option<[i32; 2]>| {
        let tiles = (0..9)
            .map(|i| [origin[0] + i % 3, origin[1] + i / 3])
            .map(|at| if Some(at) == unmade { voxear_terrain::endless::unmade().clone() } else { tile(at) })
            .collect();
        tiles
    };
    let heights = Heightmap::from_tiles(3, 3, 0.5, window([0, 0], Some([2, 2])));
    let (mut world, entity) = world_with(heights, Vec3::new(0.0, 1.0, 0.0));
    world.get_mut::<Terrain>(entity).unwrap().origin = Some([0, 0]);
    let mut physics = PhysicsScene::new();
    let down = |physics: &PhysicsScene, x: f32, z: f32| {
        physics.raycast(Vec3::new(x, 500.0, z), Vec3::NEG_Y, 1000.0, u32::MAX, false).map(|hit| hit.point.y)
    };
    // Each tile is 128 m across. A few tiles' bodies a step, those in the middle first: the 8 made in two.
    physics.step(&mut world, 1.0 / 60.0);
    assert!(down(&physics, 192.0, 192.0).is_some_and(|y| (y - 12.0).abs() < 0.01), "the middle tile, (1, 1)");
    physics.step(&mut world, 1.0 / 60.0);
    assert!(down(&physics, 64.0, 64.0).is_some_and(|y| (y - 1.0).abs() < 0.01));
    assert_eq!(down(&physics, 320.0, 320.0), None, "a tile not made yet has no ground");
    // A tile along x: the window from (1, 0), its tiles still in it where they were, the one not made made now.
    let heights = world.get::<Terrain>(entity).unwrap().heights.with_shared_tiles(window([1, 0], None));
    let terrain = world.get_mut::<Terrain>(entity).unwrap();
    (terrain.heights, terrain.origin) = (Arc::new(heights), Some([1, 0]));
    physics.step(&mut world, 1.0 / 60.0);
    assert!(down(&physics, 192.0, 192.0).is_some_and(|y| (y - 12.0).abs() < 0.01), "still where it was");
    assert!(down(&physics, 320.0, 320.0).is_some_and(|y| (y - 23.0).abs() < 0.01), "made now");
    assert!(down(&physics, 448.0, 64.0).is_some_and(|y| (y - 4.0).abs() < 0.01), "come into the window");
}

#[test]
fn a_ball_comes_to_rest_on_a_world_of_blocks_and_a_ray_meets_its_top() {
    use voxear_terrain::blocks::{BlockWorld, file::BlockFile};
    // Ground 4 blocks high of half-metre blocks, from (-10, 1, -10), and a step 2 blocks higher at its middle.
    let mut file = BlockFile::default();
    file.ground.height = 4;
    let mut blocks = file.make_ground(None);
    for (x, z) in [(32, 32), (33, 32), (32, 33), (33, 33)] {
        blocks.set([x, 4, z], 3);
        blocks.set([x, 5, z], 3);
    }
    let (_, faces) = file.faces();
    let mut world = World::new();
    voxear_scene::register_obj_components(&mut world);
    let corner = Vec3::new(-10.0, 1.0, -10.0);
    let entity = world.spawn((BlockWorld::new(blocks, &faces, 0.5), Transform::from_position(corner)));
    let ball = world.spawn((Transform::from_position(Vec3::new(0.0, 10.0, 0.0)),));
    world.insert(ball, Collider { shape: ColliderShape::Sphere, radius: 0.2, ..Collider::default() });
    world.insert(ball, Rigidbody { angular_drag: 2.0, ..Rigidbody::default() });
    let mut physics = PhysicsScene::new();
    assert!(PhysicsScene::wanted(&world));
    for _ in 0..240 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    // Over block (20, ·, 20), the ground's top 4 blocks up: 3 m in the world.
    let at = world.get::<Transform>(ball).unwrap().position;
    assert!((at.y - 3.2).abs() < 0.05, "resting on the blocks: {at}");
    let hit = physics.raycast(Vec3::new(6.2, 50.0, 6.2), Vec3::NEG_Y, 100.0, u32::MAX, false).expect("the step");
    assert_eq!(hit.obj, entity);
    assert!((hit.point.y - 4.0).abs() < 1e-3, "the step's top, 6 blocks up: {}", hit.point.y);
}
