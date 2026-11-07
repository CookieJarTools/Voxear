//! Clips as assets: a model file's clips referenced by name, read without the
//! GPU, with the settings its `.meta` gives them (headless).

use voxear_assets::{ClipLoop, ClipMotion};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Content,
    animation::{Animator, Clips, MotionMatcher, MotionMatcherState, StateChoice, StateRole, give_clips, match_motion},
};
use voxear_render::ModelRenderer;
use voxear_scene::Transform;

/// A Motion Set walking with Walk, and with it backwards, and crouching
/// with Sneak.
const HERO: &str = r#"
[[state]]
name = "Walking"
role = "grounded"
[[state.clip]]
clip = "Models/Walks.gltf#clip:Walk"
[[state.clip]]
clip = "Models/Walks.gltf#clip:Walk"
backwards = true

[[state]]
name = "Crouching"
role = "crouched"
[[state.clip]]
clip = "Models/Walks.gltf#clip:Sneak"
"#;

/// A glTF of a root with hips under it and two clips a second long, "Walk",
/// moving the root a metre and a half along Z, and "Sneak", half a metre;
/// its buffer beside it.
const WALKS: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Walks.bin", "byteLength": 56 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
    { "buffer": 0, "byteOffset": 8, "byteLength": 24 },
    { "buffer": 0, "byteOffset": 32, "byteLength": 24 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] },
    { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" },
    { "bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3" }
  ],
  "nodes": [{ "name": "root", "children": [1] }, { "name": "hips", "translation": [0, 0.9, 0] }],
  "animations": [
    {
      "name": "Walk",
      "samplers": [{ "input": 0, "output": 1 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    },
    {
      "name": "Sneak",
      "samplers": [{ "input": 0, "output": 2 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    }
  ],
  "scenes": [{ "nodes": [0] }]
}"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("Models")).unwrap();
    std::fs::write(dir.path().join("Models/Walks.gltf"), WALKS).unwrap();
    let values = [0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
    let data: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(dir.path().join("Models/Walks.bin"), data).unwrap();
    dir
}

#[test]
fn a_clip_is_found_by_its_file_and_name_with_its_settings() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let walk = content.parse("Models/Walks.gltf#clip:Walk").unwrap();
    let source = content.clip(&walk).expect("the file's Walk");
    assert_eq!(source.clip().name, "Walk");
    assert_eq!(source.clip().frames, 61, "a second at sixty frames");
    assert_eq!(source.settings.motion, ClipMotion::Auto);
    assert!(content.clip(&content.parse("Models/Walks.gltf#clip:Run").unwrap()).is_none(), "no such clip");

    // Set to move to the right, read again once the file's clips are forgotten.
    let meta = dir.path().join("Models/Walks.gltf.meta");
    let text = std::fs::read_to_string(&meta).unwrap();
    std::fs::write(
        &meta,
        format!("{text}\n[clips.Walk]\nmotion = \"set\"\ndirection = 90.0\nspeed = 2.0\nlooping = \"on\"\n"),
    )
    .unwrap();
    let before = content.clips_version();
    content.forget_clips(walk.guid().unwrap());
    assert!(content.clips_version() > before);
    let settings = content.clip(&walk).unwrap().settings;
    assert_eq!(
        (settings.motion, settings.direction, settings.speed, settings.looping),
        (ClipMotion::Set, 90.0, 2.0, ClipLoop::On)
    );
}

#[test]
fn the_clips_animators_play_are_given_to_the_world() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let reference = "Models/Walks.gltf#clip:Walk";
    world.spawn((Animator { clip: reference.into(), ..Animator::default() },));
    world.spawn((Animator { clip: "Models/Walks.gltf#clip:Run".into(), ..Animator::default() },));
    give_clips(&mut world, &mut content);
    let clips = world.resource::<Clips>().expect("made");
    assert_eq!(clips.get(reference).map(|source| source.clip().name.as_str()), Some("Walk"));
    assert!(clips.get("Models/Walks.gltf#clip:Run").is_none());
}

/// A character drawing the Walks model, playing the Hero set, moved along
/// at `velocity` for `seconds`, facing ahead; what it plays most.
fn walk(world: &mut World, content: &mut Content, hero: Entity, velocity: Vec3, seconds: f32) -> String {
    let delta = 1.0 / 60.0;
    for _ in 0..(seconds / delta) as usize {
        if let Some(matcher) = world.get_mut::<MotionMatcher>(hero) {
            (matcher.wanted_velocity, matcher.wanted_facing) = (Some(velocity), Some(Vec3::Z));
        }
        if let Some(transform) = world.get_mut::<Transform>(hero) {
            transform.position += velocity * delta;
        }
        give_clips(world, content);
        match_motion(world, delta);
    }
    world.get::<MotionMatcherState>(hero).unwrap().playing()[0].0.clone()
}

#[test]
fn a_motion_set_plays_its_states_loops_and_backwards_ones_going_back() {
    let dir = project();
    std::fs::write(dir.path().join("Hero.motionset"), HERO).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let walks = content.parse("Models/Walks.gltf").unwrap().guid().unwrap();
    let model = ModelRenderer { animation: content.clip_file(walks), ..ModelRenderer::new(Vec::new()) };
    let mut world = World::new();
    let matcher =
        MotionMatcher { motion_set: "Hero.motionset".into(), turn_to_movement: false, ..MotionMatcher::default() };
    let hero = world.spawn((Transform::from_position(Vec3::ZERO), model, matcher));

    assert_eq!(walk(&mut world, &mut content, hero, Vec3::Z * 1.5, 1.0), "Walk");
    assert_eq!(
        walk(&mut world, &mut content, hero, Vec3::NEG_Z * 1.5, 1.0),
        "Walk (backwards)",
        "facing ahead, going back"
    );
    world.get_mut::<MotionMatcher>(hero).unwrap().state = Some(StateChoice::Role(StateRole::Crouched));
    assert_eq!(walk(&mut world, &mut content, hero, Vec3::Z * 0.5, 1.0), "Sneak");
    assert_eq!(world.get::<MotionMatcherState>(hero).unwrap().state(), 1);
}

#[test]
fn with_root_motion_the_clips_carry_the_character() {
    let dir = project();
    std::fs::write(dir.path().join("Hero.motionset"), HERO).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let walks = content.parse("Models/Walks.gltf").unwrap().guid().unwrap();
    let model = ModelRenderer { animation: content.clip_file(walks), ..ModelRenderer::new(Vec::new()) };
    let mut world = World::new();
    let matcher = MotionMatcher {
        motion_set: "Hero.motionset".into(),
        root_motion: true,
        wanted_velocity: Some(Vec3::Z * 1.5),
        ..MotionMatcher::default()
    };
    let hero = world.spawn((Transform::from_position(Vec3::ZERO), model, matcher));
    for _ in 0..120 {
        give_clips(&mut world, &mut content);
        match_motion(&mut world, 1.0 / 60.0);
    }
    let at = world.get::<Transform>(hero).unwrap().position;
    assert!(at.z > 2.0 && at.z < 3.0, "walked on at the walk's pace, once it faded in: {at}");
}
