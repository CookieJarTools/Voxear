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

fn write(dir: &std::path::Path, files: &[(&str, String)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn a_character_walks_through_to_ground_made_for_it_far_off_in_a_world_without_end() {
    use voxear_core::math::DVec3;
    use voxear_engine::{Game, Project};
    use voxear_render::PortalFar;
    use voxear_scene::WorldOrigin;
    use voxear_terrain::{
        blocks::{
            file::BlockFile,
            generate::{BlockEndless, BlockGenerate},
        },
        generate::Preset,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile { block_size: 2.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -4,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let scene = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                 [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let project = "[project]\nname = \"Blocks\"\nstart_level = \"Levels/Main.level\"\n\n\
                   [floating_origin]\nmode = \"endless\"\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", project.into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", scene.into()),
            ("Blocks/Field.blockworld", file.to_text()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    // The top of the highest block under a place of the world, where its blocks are made.
    let blocks = game.engine.world.query_ref::<(Entity, &voxear_terrain::blocks::BlockWorld)>().into_iter().next().unwrap().0;
    let top = |game: &Game, x: f32, z: f32| -> Option<f32> {
        let world = &game.engine.world;
        let corner = at(&game.engine, blocks);
        let held = world.get::<voxear_terrain::blocks::BlockWorld>(blocks)?;
        let column = [((x - corner.x) / 2.0).floor() as i32, ((z - corner.z) / 2.0).floor() as i32];
        let highest = (-8..200).rev().find(|&y| held.blocks.get([column[0], y, column[1]]) != 0)?;
        Some(corner.y + (highest + 1) as f32 * 2.0)
    };
    // A walker with its eye under it, held over the middle until there is ground to stand on.
    let world = &mut game.engine.world;
    let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(8.0, 300.0, 8.0))));
    world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
    // How fast it walks, and how fast it falls.
    let walking = Arc::new(Mutex::new((0.0f32, 0.0f32)));
    let speed = walking.clone();
    game.engine.schedule.add_system("Update", "walk", move |world| {
        let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
        let ahead = world.get::<Transform>(walker).map_or(Vec3::Z, |transform| transform.rotation * Vec3::Z);
        let (walk, fall) = *speed.lock().unwrap();
        if walk != 0.0 || fall != 0.0 {
            physics.move_character(world, walker, ahead * walk - Vec3::Y * fall, FRAME);
        }
        world.insert_resource(physics);
    });
    let settle = |game: &mut Game| {
        for _ in 0..6000 {
            game.tick(FRAME, None);
            if voxear_engine::blocks::streamed(&game.engine.world) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("the blocks were never all made");
    };
    settle(&mut game);
    let under = top(&game, 8.0, 8.0).expect("ground under the walker");
    game.engine.world.get_mut::<Transform>(walker).unwrap().position.y = under + 2.0;
    walking.lock().unwrap().1 = 6.0;
    for _ in 0..90 {
        game.tick(FRAME, None);
    }
    let stood = at(&game.engine, walker);
    assert!((stood.y - under).abs() < 0.1, "the walker stands on the hills: {stood:?}, ground at {under}");

    // A Portal 6 m in front of it, tall enough that the hill it stands on does not hide its opening, and the
    // Portal that one leads to 9 km off.
    let there = Vec3::new(9000.0, 0.0, -2500.0);
    let facing_back = Quat::from_rotation_y(std::f32::consts::PI);
    let world = &mut game.engine.world;
    let near = world.spawn((Portal::default(), Transform::new(stood + Vec3::new(0.0, -6.0, 6.0), facing_back, Vec3::ONE)));
    let far = world.spawn((Portal::default(), Transform::new(there, Quat::from_rotation_y(TURN), Vec3::ONE)));
    *world.get_mut::<Portal>(near).unwrap() = Portal::to(Vec2::new(4.0, 40.0), far);
    *world.get_mut::<Portal>(far).unwrap() = Portal::to(Vec2::new(4.0, 40.0), near);

    // Its far side is not there yet: the Portal is shut until the ground round the far one is made.
    game.tick(FRAME, None);
    let shut = *game.engine.world.get::<PortalFar>(near).expect("a world without end says how far along it is");
    assert!(!shut.ready && shut.shown == 0.0, "{shut:?}");
    assert!(top(&game, there.x, there.z).is_none(), "no ground made 9 km off before a Portal led there");
    settle(&mut game);
    for _ in 0..40 {
        game.tick(FRAME, None);
    }
    let open = *game.engine.world.get::<PortalFar>(near).unwrap();
    assert!(open.ready && open.shown == 1.0, "made, and its mist cleared: {open:?}");
    let far_ground = top(&game, there.x, there.z).expect("the far ground is made");
    // The far Portal set where coming out of it lands a little over that ground: the walker goes in 6 m up the
    // near one's opening.
    game.engine.world.get_mut::<Transform>(far).unwrap().position.y = far_ground - 5.0;
    game.tick(FRAME, None);
    assert_eq!(WorldOrigin::of(&game.engine.world), WorldOrigin::default(), "nothing moved the world's middle yet");

    // It walks through, and four seconds on stands on the hills 9 km off, the world's middle having come with it.
    walking.lock().unwrap().0 = 3.0;
    let mut events = Vec::new();
    for _ in 0..240 {
        game.tick(FRAME, None);
        events.extend(passed(&game.engine));
    }
    assert_eq!(events.len(), 1, "{events:?}");
    walking.lock().unwrap().0 = 0.0;
    for _ in 0..120 {
        game.tick(FRAME, None);
    }
    let origin = WorldOrigin::of(&game.engine.world);
    assert_ne!(origin, WorldOrigin::default(), "the world's middle came with the camera");
    let now = origin.to_level(at(&game.engine, walker));
    let away = now.distance(DVec3::new(f64::from(there.x), now.y, f64::from(there.z)));
    assert!(away < 20.0, "by the far Portal: {now:?}");
    let here = at(&game.engine, walker);
    let under = top(&game, here.x, here.z).expect("ground under it");
    assert!((here.y - under).abs() < 0.1, "standing on the hills there, at {} on ground at {under}", here.y);
    let state = game.engine.physics().unwrap().character_state(walker).unwrap();
    assert!(matches!(state.ground, voxear_physics::Ground::Ground), "{:?}", state.ground);
    // The near side is the far side now, and still made, to look back into.
    let back = *game.engine.world.get::<PortalFar>(far).unwrap();
    assert!(back.ready, "the way back is open: {back:?}");
}

const GATES: &str = r#"format = 1

[[obj]]
id = 1
name = "Gate"
position = [0.0, 0.0, 5.0]
rotation = [0.0, 180.0, 0.0]

[obj.portal]
to = 2

[[obj]]
id = 2
name = "Far Gate"
position = [900.0, 20.0, -300.0]

[obj.portal]
to = 1
size = [3.0, 4.0]
open = false
passes = [0, 2]
mist = [1.0, 0.5, 0.25]
view_distance = 48.0

[[obj]]
id = 3
name = "Eye"
camera_follows = 4

[obj.camera]
fov = 60.0

[[obj]]
id = 4
name = "Walker"
character = {}
"#;

#[test]
fn a_scene_file_keeps_its_portals_and_what_its_cameras_follow() {
    use voxear_engine::{Content, scene_file};
    use voxear_scene::Name;
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), &[("Scenes/Gates.scene", GATES.into())]);
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Scenes/Gates.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let find = |name: &str| {
        engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let (gate, far, eye, walker) = (find("Gate"), find("Far Gate"), find("Eye"), find("Walker"));
    let portal = *engine.world.get::<Portal>(gate).unwrap();
    assert_eq!(portal, Portal::to(Vec2::new(2.0, 3.0), far), "one as it is made, but for where it leads");
    let back = *engine.world.get::<Portal>(far).unwrap();
    assert_eq!((back.to, back.size, back.open, back.passes), (Some(gate), Vec2::new(3.0, 4.0), false, 0b101));
    assert_eq!((back.mist, back.view_distance), (Vec3::new(1.0, 0.5, 0.25), 48.0));
    assert_eq!(engine.world.get::<CameraFollows>(eye), Some(&CameraFollows(walker)));
    // Saved, it is the file it was read from.
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let text = toml::to_string(&saved).unwrap();
    for line in ["to = 2", "to = 1", "size = [3.0, 4.0]", "open = false", "passes = [0, 2]", "camera_follows = 4"] {
        assert!(text.contains(line), "{line} in:\n{text}");
    }
    assert_eq!(text.matches("view_distance").count(), 1, "what is as made is left out:\n{text}");
}
