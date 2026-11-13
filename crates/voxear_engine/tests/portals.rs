//! Portals in the game, headless (no GPU, no scripts): a character walks
//! through one and stands on the ground of a place 20 km off, a thrown body
//! keeps its speed, nothing is shown part-way between the two places, and a
//! camera that follows goes on seeing its subject through the opening.

use std::sync::{Arc, Mutex};

use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine,
    physics::{CharacterController, Collider, PhysicsScene, Rigidbody},
    portals::{PortalEvents, PortalPassed},
};
use voxear_render::{Camera, CameraFollows, Carry, Portal, PortalPose, PortalTraveller, ViewCarry};
use voxear_scene::{Parent, Transform};

/// The second place, and which way its Portal faces.
const THERE: Vec3 = Vec3::new(20_000.0, 50.0, -8_000.0);
const TURN: f32 = 1.1;
const FRAME: f32 = 1.0 / 60.0;

/// Two slabs of ground 20 km apart, each with a Portal that leads to the
/// other: the first 5 m along +z from the world's middle, facing back down
/// it; the second in the middle of its slab, turned `TURN`.
fn places() -> (Engine, Entity, Entity) {
    let mut engine = Engine::new();
    let ground = Collider { size: [40.0, 1.0, 40.0], ..Collider::default() };
    engine.world.spawn((ground, Transform::from_position(Vec3::new(0.0, -0.5, 0.0))));
    engine.world.spawn((ground, Transform::from_position(THERE - Vec3::Y * 0.5)));
    let facing_back = Quat::from_rotation_y(std::f32::consts::PI);
    let near = engine.world.spawn((Portal::default(), Transform::new(Vec3::new(0.0, 0.0, 5.0), facing_back, Vec3::ONE)));
    let far = engine.world.spawn((Portal::default(), Transform::new(THERE, Quat::from_rotation_y(TURN), Vec3::ONE)));
    *engine.world.get_mut::<Portal>(near).unwrap() = Portal::to(Vec2::new(2.0, 3.0), far);
    *engine.world.get_mut::<Portal>(far).unwrap() = Portal::to(Vec2::new(2.0, 3.0), near);
    (engine, near, far)
}

/// A character that walks the way its Obj faces at `speed`, falling as it
/// goes, moved in Update as a player's script moves one.
fn walker(engine: &mut Engine, at: Vec3, speed: f32) -> Entity {
    let walker = engine.world.spawn((CharacterController::default(), Transform::from_position(at)));
    engine.schedule.add_system("Update", "walk", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let ahead = world.get::<Transform>(walker).map_or(Vec3::Z, |transform| transform.rotation * Vec3::Z);
        physics.move_character(world, walker, ahead * speed - Vec3::Y * 2.0, FRAME);
        world.insert_resource(physics);
    });
    walker
}

fn at(engine: &Engine, entity: Entity) -> Vec3 {
    voxear_scene::world_matrix(&engine.world, entity).w_axis.truncate()
}

fn passed(engine: &Engine) -> Vec<PortalPassed> {
    engine.world.resource::<PortalEvents>().map(|events| events.passed.clone()).unwrap_or_default()
}

#[test]
fn a_character_walks_through_and_stands_on_the_far_ground() {
    let (mut engine, near, far) = places();
    // A little left of the opening's middle, so which side it comes out on shows.
    let walker = walker(&mut engine, Vec3::new(0.4, 0.0, 0.0), 3.0);
    let far_pose = PortalPose::of_obj(&engine.world, far).unwrap();
    let mut events = Vec::new();
    let mut steps = Vec::new();
    for _ in 0..240 {
        engine.tick(FRAME);
        events.extend(passed(&engine));
        steps.push(at(&engine, walker));
    }
    assert_eq!(events.len(), 1, "it went through once: {events:?}");
    let event = events[0];
    assert_eq!((event.obj, event.from, event.to), (walker, near, far));
    // No frame found it anywhere but by one Portal or the other.
    for (frame, step) in steps.iter().enumerate() {
        let by = step.distance(Vec3::new(0.0, 0.0, 5.0)).min(step.distance(THERE));
        assert!(by < 13.0, "frame {frame}: {step:?} is by neither Portal");
    }
    // Four seconds at 3 m/s: 5 m to the first opening, then 7 m out of the front of the second, on the side of
    // its middle that mirrors the side it went in at, on the ground there, facing out.
    let local = far_pose.local(at(&engine, walker));
    assert!((local.z - 7.0).abs() < 0.3, "out of its front: {local:?}");
    assert!((local.x - 0.4).abs() < 0.05, "mirrored across the opening: {local:?}");
    assert!(local.y.abs() < 0.05, "standing on the far ground: {local:?}");
    let state = engine.physics().unwrap().character_state(walker).unwrap();
    assert!(matches!(state.ground, voxear_physics::Ground::Ground), "on the ground: {:?}", state.ground);
    let facing = engine.world.get::<Transform>(walker).unwrap().rotation * Vec3::Z;
    assert!(facing.distance(far_pose.front()) < 1e-3, "facing out of the far Portal: {facing:?}");
    // Each frame's step is as long, the frame it went through too, measured as the Portal joins the places.
    let through = steps.iter().position(|step| step.distance(THERE) < 13.0).unwrap();
    let step = event.carry.point(steps[through - 1]).distance(steps[through]);
    assert!((step - 3.0 * FRAME).abs() < 0.01, "the step through it is a step: {step}");
}

#[test]
fn a_thrown_body_keeps_its_speed_and_its_spin() {
    let (mut engine, near, far) = places();
    let ball = Collider { shape: voxear_engine::physics::ColliderShape::Sphere, radius: 0.2, ..Collider::default() };
    let body = Rigidbody { gravity: false, angular_drag: 0.0, ..Rigidbody::default() };
    let thrown = engine.world.spawn((ball, body, Transform::from_position(Vec3::new(-0.3, 1.5, 2.0))));
    for _ in 0..3 {
        engine.tick(FRAME);
    }
    let (speed, spin) = (Vec3::new(0.5, 0.2, 9.0), Vec3::new(0.0, 4.0, 1.0));
    let physics = engine.world.resource_mut::<PhysicsScene>().unwrap();
    let id = physics.body(thrown).unwrap();
    physics.world_mut().set_velocity(id, speed, spin);
    let mut events = Vec::new();
    for _ in 0..40 {
        engine.tick(FRAME);
        events.extend(passed(&engine));
    }
    assert_eq!(events.len(), 1, "{events:?}");
    let carry = Carry::between(
        &PortalPose::of_obj(&engine.world, near).unwrap(),
        &PortalPose::of_obj(&engine.world, far).unwrap(),
    );
    let physics = engine.physics().unwrap();
    let (now, turning) = physics.world().velocity(physics.body(thrown).unwrap());
    assert!(now.distance(carry.direction(speed)) < 0.05, "its speed, turned: {now:?}");
    assert!(turning.distance(carry.direction(spin)) < 0.05, "its spin, turned: {turning:?}");
    assert!(at(&engine, thrown).distance(THERE) < 12.0, "it is in the far place: {:?}", at(&engine, thrown));
}

#[test]
fn an_obj_moved_on_ticks_is_never_shown_between_the_places() {
    let (mut engine, _, far) = places();
    // Walked 3 m/s along its own forward on each tick, shown on a display faster than the ticks.
    let obj = engine.world.spawn((PortalTraveller, Transform::from_position(Vec3::new(0.0, 1.0, 3.0))));
    engine.schedule.add_system("FixedUpdate", "walk", move |world| {
        if let Some(transform) = world.get_mut::<Transform>(obj) {
            let ahead = transform.rotation * Vec3::Z;
            transform.position += ahead * 3.0 / 60.0;
        }
    });
    let far_pose = PortalPose::of_obj(&engine.world, far).unwrap();
    let mut shown = Vec::new();
    let mut carry = None;
    for _ in 0..288 {
        engine.tick(1.0 / 144.0);
        carry = carry.or(passed(&engine).first().map(|passed| passed.carry));
        shown.push(at(&engine, obj));
    }
    let carry = carry.expect("it went through");
    // Laid out as one walk, with what was shown before it went through carried over: even steps all the way.
    let even = 3.0 / 144.0;
    let through = shown.iter().position(|place| place.distance(THERE) < 20.0).unwrap();
    let walk: Vec<Vec3> =
        shown.iter().enumerate().map(|(frame, &place)| if frame < through { carry.point(place) } else { place }).collect();
    for (frame, pair) in walk.windows(2).enumerate().skip(10) {
        let moved = pair[0].distance(pair[1]);
        assert!((moved - even).abs() < even * 0.5, "frame {frame} moved {moved}, not about {even}");
    }
    assert!(far_pose.local(*shown.last().unwrap()).z > 0.5);
}

#[test]
fn a_camera_goes_on_seeing_what_it_follows_through_the_opening() {
    let (mut engine, _, far) = places();
    let walker = walker(&mut engine, Vec3::ZERO, 3.0);
    // A camera 4 m behind and 2 m over what it follows, set there after it moves, as an orbit camera's script does.
    let camera = engine.world.spawn((Camera::default(), CameraFollows(walker), Transform::default()));
    engine.schedule.add_system("PostUpdate", "follow", move |world| {
        let Some(&subject) = world.get::<Transform>(walker) else { return };
        if let Some(transform) = world.get_mut::<Transform>(camera) {
            transform.position = subject.position + subject.rotation * Vec3::new(0.0, 2.0, -4.0);
            transform.rotation = subject.rotation;
        }
    });
    // And an eye a little ahead of a second walker's feet, under it, as a first-person camera is.
    let second = engine.world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(-0.5, 0.0, 0.0))));
    engine.schedule.add_system("Update", "walk second", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let ahead = world.get::<Transform>(second).map_or(Vec3::Z, |transform| transform.rotation * Vec3::Z);
        physics.move_character(world, second, ahead * 0.6 - Vec3::Y * 2.0, FRAME);
        world.insert_resource(physics);
    });
    let eye = engine.world.spawn((
        Camera::default(),
        Parent(second),
        Transform::from_position(Vec3::new(0.0, 1.6, 0.3)),
    ));
    let seen: Arc<Mutex<Vec<[(Vec3, bool); 2]>>> = Arc::default();
    let far_pose = PortalPose::of_obj(&engine.world, far).unwrap();
    for _ in 0..600 {
        engine.tick(FRAME);
        voxear_scene::propagate_transforms(&mut engine.world);
        let looks = [camera, eye].map(|camera| {
            let (_, rotation, position) = voxear_scene::world_matrix(&engine.world, camera).to_scale_rotation_translation();
            (ViewCarry::of(&engine.world, camera, position, rotation).0, engine.world.has::<ViewCarry>(camera))
        });
        seen.lock().unwrap().push(looks);
    }
    let seen = seen.lock().unwrap();
    for (which, name) in ["the camera that follows", "the eye"].iter().enumerate() {
        let looks: Vec<(Vec3, bool)> = seen.iter().map(|frame| frame[which]).collect();
        // Where it looks from moves a step a frame, but for the one frame it goes through itself, and it was
        // carried for a while before that: while its subject was through and it was not.
        let jumps: Vec<usize> =
            looks.windows(2).enumerate().filter(|(_, pair)| pair[0].0.distance(pair[1].0) > 1.0).map(|(frame, _)| frame).collect();
        assert_eq!(jumps.len(), 1, "{name} went through once: {jumps:?}");
        let carried = looks.iter().filter(|(_, carried)| *carried).count();
        assert!(carried > 3, "{name} was carried for {carried} frames");
        // The camera that follows is carried from when its subject goes through until it does; the eye from when
        // it goes through, ahead of the feet under it, until they do.
        assert_ne!(looks[jumps[0]].1, looks[jumps[0] + 1].1, "{name} went through as its carry began or ended");
        assert!(looks[..jumps[0]].iter().all(|(place, _)| place.length() < 30.0), "{name} looked from the near place till then");
        let last = looks.last().unwrap().0;
        assert!(far_pose.local(last).z > 0.0 && last.distance(THERE) < 30.0, "{name} ends in the far place: {last:?}");
    }
}
