//! Clips as assets: a model file's clips referenced by name, read without the
//! GPU, with the settings its `.meta` gives them (headless).

use voxear_assets::{ClipLoop, ClipMotion};
use voxear_ecs::World;
use voxear_engine::{
    Content,
    animation::{Animator, Clips, give_clips},
};

/// A glTF of a root with hips under it and one clip, "Walk", moving the
/// root a metre and a half along Z in its one second; its buffer beside it.
const WALKS: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Walks.bin", "byteLength": 32 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
    { "buffer": 0, "byteOffset": 8, "byteLength": 24 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] },
    { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" }
  ],
  "nodes": [{ "name": "root", "children": [1] }, { "name": "hips", "translation": [0, 0.9, 0] }],
  "animations": [{
    "name": "Walk",
    "samplers": [{ "input": 0, "output": 1 }],
    "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
  }],
  "scenes": [{ "nodes": [0] }]
}"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("Models")).unwrap();
    std::fs::write(dir.path().join("Models/Walks.gltf"), WALKS).unwrap();
    let data: Vec<u8> = [0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5].iter().flat_map(|v| v.to_le_bytes()).collect();
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
