//! Plants in physics: a Foliage's plants with trunks stand a capsule each,
//! which rays meet, saying which plant, and a plant taken away takes its
//! trunk with it.

use glam::{Quat, Vec3};
use voxear_ecs::World;
use voxear_physics::scene::PhysicsScene;
use voxear_scene::{Plant, Plants, Transform, Trunk};

#[test]
fn a_ray_meets_a_plants_trunk_and_passes_where_it_was_taken_away() {
    let mut world = World::new();
    voxear_scene::register_obj_components(&mut world);
    // Three trees along x, 4 m apart, the middle one twice as large, on an Obj 10 m along z.
    let mut middle = Plant::new([4.0, 0.0, 0.0], 0.0, 2.0);
    middle.lean = [0.0, 0.0];
    let trees = vec![Plant::new([0.0, 0.0, 0.0], 0.0, 1.0), middle, Plant::new([8.0, 0.0, 0.0], 0.0, 1.0)];
    let plants = Plants { trunk: Some(Trunk { radius: 0.3, height: 5.0, solid: None }), ..Plants::new(trees) };
    let grove = world.spawn((plants, Transform::new(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY, Vec3::ONE)));
    let mut physics = PhysicsScene::new();
    assert!(PhysicsScene::wanted(&world), "trunks want physics");
    physics.step(&mut world, 1.0 / 60.0);
    let along_z =
        |physics: &PhysicsScene, x: f32, y: f32| physics.raycast(Vec3::new(x, y, 0.0), Vec3::Z, 50.0, u32::MAX, false);
    let hit = along_z(&physics, 4.0, 1.0).expect("the middle tree");
    assert_eq!((hit.obj, hit.plant), (grove, Some(1)));
    assert!((hit.distance - (10.0 - 0.6)).abs() < 0.05, "its trunk twice as thick: {}", hit.distance);
    assert!(along_z(&physics, 4.0, 9.5).is_some(), "twice as tall, so 10 m");
    assert!(along_z(&physics, 0.0, 5.5).is_none(), "a small tree's trunk is 5 m");
    assert!(along_z(&physics, 2.0, 1.0).is_none(), "between them");

    // The middle tree cut down: only its trunk goes.
    world.get_mut::<Plants>(grove).unwrap().remove(1);
    physics.step(&mut world, 1.0 / 60.0);
    assert!(along_z(&physics, 4.0, 1.0).is_none(), "gone");
    assert_eq!(along_z(&physics, 8.0, 1.0).and_then(|hit| hit.plant), Some(2));

    // Trunks switched off take the rest away too.
    world.get_mut::<Plants>(grove).unwrap().trunk = None;
    physics.step(&mut world, 1.0 / 60.0);
    assert!(along_z(&physics, 8.0, 1.0).is_none());
}

#[test]
fn a_solid_plant_is_a_box_on_the_ground_sized_and_turned_with_it() {
    let mut world = World::new();
    voxear_scene::register_obj_components(&mut world);
    // Two rocks 2 m wide, 1 m high and 1 m deep: one as made, one twice as large and turned a quarter round.
    let quarter = std::f32::consts::FRAC_PI_2;
    let rocks = vec![Plant::new([0.0, 0.0, 0.0], 0.0, 1.0), Plant::new([10.0, 0.0, 0.0], quarter, 2.0)];
    let plants = Plants { trunk: Some(Trunk::solid([2.0, 1.0, 1.0])), ..Plants::new(rocks) };
    let field = world.spawn((plants, Transform::new(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY, Vec3::ONE)));
    let mut physics = PhysicsScene::new();
    assert!(PhysicsScene::wanted(&world), "solid plants want physics");
    physics.step(&mut world, 1.0 / 60.0);
    let along_z =
        |physics: &PhysicsScene, x: f32, y: f32| physics.raycast(Vec3::new(x, y, 0.0), Vec3::Z, 50.0, u32::MAX, false);
    let hit = along_z(&physics, 0.9, 0.5).expect("the first rock, near its end");
    assert_eq!((hit.obj, hit.plant), (field, Some(0)));
    assert!((hit.distance - 9.5).abs() < 0.05, "half a metre deep either way: {}", hit.distance);
    assert!(along_z(&physics, 1.2, 0.5).is_none(), "past its end");
    assert!(along_z(&physics, 0.0, 1.2).is_none(), "over it");
    // The second: 4 m by 2 m by 2 m, turned so its length lies along z.
    let hit = along_z(&physics, 10.9, 1.8).expect("the second rock, near its top and its side");
    assert_eq!(hit.plant, Some(1));
    assert!((hit.distance - 8.0).abs() < 0.05, "two metres either way along z: {}", hit.distance);
    assert!(along_z(&physics, 11.2, 0.5).is_none(), "and one either way along x");
}
