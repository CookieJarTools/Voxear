//! Physics from scene files, headless (no GPU, no scripts): Colliders,
//! Rigidbodies and Character Controllers load, play, save and come through
//! prefabs as the other components do.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Content, Engine,
    physics::{CharacterController, Collider, ColliderShape, Rigidbody},
    scene_file,
};
use voxear_scene::{Name, Transform};

const BALL: &str = r#"
format = 1

[[obj]]
id = 1
name = "Ball"
collider = { shape = "sphere", radius = 0.5 }
rigidbody = { mass = 2.0 }
"#;

const YARD: &str = r#"
format = 1

[[obj]]
id = 1
name = "Ground"
position = [0.0, -0.5, 0.0]
collider = { size = [20.0, 1.0, 20.0] }

[[obj]]
id = 2
name = "Crate"
position = [0.0, 5.0, 0.0]
collider = {}
rigidbody = {}

[[obj]]
id = 3
name = "Gate"
position = [3.0, 3.0, 0.0]
collider = { size = [2.0, 1.0, 2.0], trigger = true }

[[obj]]
id = 4
prefab = "Prefabs/Ball.prefab"
name = "Big Ball"
position = [3.0, 6.0, 0.0]
collider = { shape = "sphere", radius = 1.0 }

[[obj]]
id = 5
name = "Walker"
position = [-3.0, 0.0, 0.0]
character = { height = 1.6 }
"#;

fn yard() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [("Prefabs/Ball.prefab", BALL), ("Scenes/Yard.scene", YARD)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn find(engine: &Engine, name: &str) -> Entity {
    let found = engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name);
    found.unwrap_or_else(|| panic!("no Obj named {name}")).0
}

#[test]
fn a_scene_plays_its_physics_and_saves_it_back() {
    let dir = yard();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Scenes/Yard.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();

    // The tables load, with the instance's override over its prefab's.
    let ball = find(&engine, "Big Ball");
    let collider = *engine.world.get::<Collider>(ball).unwrap();
    assert_eq!((collider.shape, collider.radius), (ColliderShape::Sphere, 1.0));
    assert_eq!(engine.world.get::<Rigidbody>(ball).unwrap().mass, 2.0, "kept from the prefab");
    assert!(engine.world.get::<Collider>(find(&engine, "Gate")).unwrap().trigger);
    assert_eq!(engine.world.get::<CharacterController>(find(&engine, "Walker")).unwrap().height, 1.6);

    // Played for three seconds, the crate lands on the ground and the ball
    // falls through the gate onto it.
    let mut events = Vec::new();
    for _ in 0..180 {
        engine.tick(1.0 / 60.0);
        events.extend_from_slice(engine.physics_events());
    }
    let height = |name| engine.world.get::<Transform>(find(&engine, name)).unwrap().position.y;
    assert!((height("Crate") - 0.5).abs() < 0.05, "the crate rests on the ground: {}", height("Crate"));
    assert!((height("Big Ball") - 1.0).abs() < 0.05, "the ball rests on the ground: {}", height("Big Ball"));
    let gate = find(&engine, "Gate");
    let through = |began| events.iter().filter(|e| e.trigger && e.began == began && [e.a, e.b].contains(&gate)).count();
    assert_eq!((through(true), through(false)), (1, 1), "the ball went in and out of the gate: {events:?}");

    // Asleep on the ground, the crate still touches it, as in Unity.
    let (ground, crate_obj) = (find(&engine, "Ground"), find(&engine, "Crate"));
    let physics = engine.physics().unwrap();
    assert!(!physics.world().is_active(physics.body(crate_obj).unwrap()), "the crate has gone to sleep");
    assert!(physics.touching(ground, crate_obj));
    let parted = events.iter().any(|e| !e.began && [e.a, e.b].contains(&crate_obj));
    assert!(!parted, "nothing told the crate left the ground: {events:?}");

    // Saved, only what differs from the defaults is written, and the
    // instance keeps the collider it changed, not the prefab's Rigidbody.
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let text = toml::to_string(&saved).unwrap();
    assert!(text.contains("trigger = true"), "{text}");
    assert!(!text.contains("bounciness"), "defaults are left out: {text}");
    let big = saved.objs.iter().find(|o| o.name == "Big Ball").unwrap();
    assert_eq!(big.collider.map(|c| c.radius), Some(1.0), "{text}");
    assert!(big.rigidbody.is_none(), "{text}");
}

#[test]
fn edit_mode_leaves_physics_alone() {
    let dir = yard();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Scenes/Yard.scene")).unwrap();
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    for _ in 0..30 {
        engine.tick(1.0 / 60.0);
    }
    let crate_at = engine.world.get::<Transform>(find(&engine, "Crate")).unwrap().position;
    assert_eq!(crate_at, Vec3::new(0.0, 5.0, 0.0));
    assert!(engine.physics().is_none());
}

const PLATFORMS: &str = r#"
format = 1

[[obj]]
id = 1
name = "Floor"
position = [0.0, -0.5, 0.0]
collider2d = { size = [20.0, 1.0] }

[[obj]]
id = 2
name = "Coin"
position = [0.0, 2.0, 0.0]
collider2d = { shape = "circle", radius = 0.25, trigger = true }

[[obj]]
id = 3
name = "Hero"
position = [0.0, 5.0, 1.0]
collider2d = { shape = "capsule", radius = 0.3, height = 1.2 }
rigidbody2d = { mass = 60.0, fixed_rotation = true }
"#;

#[test]
fn a_2d_scene_plays_in_the_xy_plane_and_saves_back() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Platforms.scene"), PLATFORMS).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Platforms.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();

    let mut events = Vec::new();
    for _ in 0..150 {
        engine.tick(1.0 / 60.0);
        events.extend_from_slice(engine.physics_events_2d());
    }
    let hero = find(&engine, "Hero");
    let at = engine.world.get::<Transform>(hero).unwrap().position;
    assert!((at.y - 0.6).abs() < 0.03 && at.z == 1.0, "standing on the floor, in front of it: {at}");
    let coin = find(&engine, "Coin");
    let picked = events.iter().any(|e| e.trigger && e.began && [e.a, e.b].contains(&coin));
    assert!(picked, "the hero fell through the coin: {events:?}");
    assert!(engine.physics().is_none(), "3D physics is not made for a 2D game");

    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let text = toml::to_string(&scene_file::capture(&engine.world, &mut content, &entities)).unwrap();
    assert!(text.contains("fixed_rotation = true") && text.contains("shape = \"capsule\""), "{text}");
    assert!(!text.contains("gravity_scale"), "defaults are left out: {text}");
}

const BELL: &str = r#"
format = 1

[[obj]]
id = 1
name = "Hook"
position = [0.0, 5.0, 0.0]
collider = { size = [0.2, 0.2, 0.2] }

[[obj]]
id = 2
name = "Weight"
position = [2.0, 5.0, 0.0]
collider = { shape = "sphere", radius = 0.2 }
rigidbody = {}
joint = { type = "distance", connected = 1, anchor = [-2.0, 0.0, 0.0] }
"#;

#[test]
fn a_joint_holds_its_obj_to_the_one_it_names_and_saves_it_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Bell.scene"), BELL).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Bell.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let (hook, weight) = (find(&engine, "Hook"), find(&engine, "Weight"));
    assert_eq!(engine.world.get::<voxear_engine::physics::Joint>(weight).unwrap().connected, Some(hook));

    let mut lowest = f32::MAX;
    for _ in 0..90 {
        engine.tick(1.0 / 60.0);
        lowest = lowest.min(engine.world.get::<Transform>(weight).unwrap().position.y);
    }
    let at = engine.world.get::<Transform>(weight).unwrap().position;
    assert!((at.distance(Vec3::new(0.0, 5.0, 0.0)) - 2.0).abs() < 0.05, "on its rope from the hook: {at}");
    assert!(lowest < 3.2, "swung down under it: {lowest}");

    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let text = toml::to_string(&scene_file::capture(&engine.world, &mut content, &entities)).unwrap();
    assert!(text.contains("type = \"distance\"") && text.contains("connected = 1"), "{text}");
}
