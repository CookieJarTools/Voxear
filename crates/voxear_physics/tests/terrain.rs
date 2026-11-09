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
