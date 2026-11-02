//! Audio Sources in a running game, headless, with the mixer rendered by
//! hand so no sound card is needed.

use voxear_assets::{SoundImport, SoundLoad};
use voxear_audio::Mixer;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    ContentSettings, Engine, Game, Project,
    audio::{Audio, AudioListener, AudioSource, GameAudio, OneShot, SoundSpace, SourceRequest},
    content_build, scene_file,
};
use voxear_scene::{Name, Transform};

const RATE: u32 = 48_000;

const MAIN: &str = r#"
format = 1

[[obj]]
id = 1
name = "Main Camera"
[obj.camera]
fov = 60.0

[[obj]]
id = 2
name = "Waterfall"
position = [10.0, 0.0, 0.0]
[obj.audio]
sound = "Sounds/Tone.wav"
loop = true
max_distance = 40.0

[[obj]]
id = 3
name = "Chime"
[obj.audio]
sound = "Sounds/Chime.wav"
play_on_start = false
mode = "2d"
bus = "music"
"#;

/// A 16-bit mono WAV holding one level for `frames`.
fn wav(level: i16, frames: usize) -> Vec<u8> {
    let data = (frames * 2) as u32;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for value in [16u32.to_le_bytes(), [1, 0, 1, 0], RATE.to_le_bytes(), (RATE * 2).to_le_bytes(), [2, 0, 16, 0]] {
        file.extend_from_slice(&value);
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&data.to_le_bytes());
    (0..frames).for_each(|_| file.extend_from_slice(&level.to_le_bytes()));
    file
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, bytes: &[u8]| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    };
    write("voxear.toml", b"[project]\nname = \"Sounds\"\nstart_level = \"Levels/Main.level\"\n");
    write("Levels/Main.level", b"format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
    write("Scenes/Main.scene", MAIN.as_bytes());
    write("Sounds/Tone.wav", &wav(16_384, RATE as usize));
    write("Sounds/Chime.wav", &wav(8_192, 2_000));
    write("Sounds/Unused.wav", &wav(1, 10));
    dir
}

/// The project's game, started, with sound mixed by hand.
fn game(dir: &tempfile::TempDir) -> (Game, Mixer) {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let (audio, mixer) = Audio::offline(RATE);
    game.audio = GameAudio::new(audio);
    game.start(None).unwrap();
    (game, mixer)
}

fn obj(game: &Game, name: &str) -> Entity {
    let world = &game.engine.world;
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap()
}

fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    mixer.render(&mut out, 2);
    out
}

#[test]
fn sources_play_on_start_from_where_their_objs_are() {
    let dir = project();
    let (mut game, mut mixer) = game(&dir);
    let (waterfall, chime) = (obj(&game, "Waterfall"), obj(&game, "Chime"));
    let source = *game.engine.world.get::<AudioSource>(waterfall).unwrap();
    assert!(source.sound.is_some() && source.looping && source.play_on_start);
    assert_eq!((source.space, source.max_distance), (SoundSpace::ThreeD, 40.0));
    let world = &game.engine.world;
    assert!(game.audio.is_source_playing(world, waterfall), "playing on start from the moment it is in the game");
    assert!(!game.audio.is_source_playing(world, chime));

    game.tick(1.0 / 60.0, None);
    let out = render(&mut mixer, 64);
    // The camera faces +Z, so the waterfall at +X is on the listener's right.
    let (left, right) = (out[2 * 63], out[2 * 63 + 1]);
    assert!(right > 0.01 && left.abs() < 1e-4, "heard from the right: {left} {right}");

    // Walked out of reach, it plays on, silent.
    game.engine.world.get_mut::<Transform>(waterfall).unwrap().position = Vec3::new(100.0, 0.0, 0.0);
    game.tick(1.0 / 60.0, None);
    render(&mut mixer, 64);
    assert!(render(&mut mixer, 64).iter().all(|&s| s == 0.0));
    assert!(game.audio.is_source_playing(&game.engine.world, waterfall));
}

#[test]
fn scripts_play_pause_and_stop_sources() {
    let dir = project();
    let (mut game, mut mixer) = game(&dir);
    let (waterfall, chime) = (obj(&game, "Waterfall"), obj(&game, "Chime"));
    game.tick(1.0 / 60.0, None);

    assert!(game.audio.request(&game.engine.world, chime, SourceRequest::Play));
    assert!(game.audio.is_source_playing(&game.engine.world, chime));
    game.tick(1.0 / 60.0, None);
    let out = render(&mut mixer, 100);
    // The waterfall plays on the right, so the left is the chime's alone.
    assert!((out[0] - 0.25).abs() < 0.01, "2D, at its own level: {:?}", &out[..2]);

    // The chime is 2 000 frames long, and ends by itself.
    render(&mut mixer, 2_000);
    game.tick(1.0 / 60.0, None);
    assert!(!game.audio.is_source_playing(&game.engine.world, chime));

    game.audio.request(&game.engine.world, waterfall, SourceRequest::Pause);
    assert!(!game.audio.is_source_playing(&game.engine.world, waterfall));
    game.tick(1.0 / 60.0, None);
    game.audio.request(&game.engine.world, waterfall, SourceRequest::Play);
    game.tick(1.0 / 60.0, None);
    assert!(game.audio.is_source_playing(&game.engine.world, waterfall));
    game.audio.request(&game.engine.world, waterfall, SourceRequest::Stop);
    game.tick(1.0 / 60.0, None);
    assert!(!game.audio.is_source_playing(&game.engine.world, waterfall));
    assert!(!game.audio.request(&game.engine.world, obj(&game, "Main Camera"), SourceRequest::Play), "no source there");
}

#[test]
fn a_sources_voice_stops_when_its_obj_goes() {
    let dir = project();
    let (mut game, mut mixer) = game(&dir);
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.audio.audio.voices(), 1);
    game.engine.world.despawn(obj(&game, "Waterfall"));
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.audio.audio.voices(), 0);
    render(&mut mixer, 64);
    assert_eq!(mixer.voices(), 0);

    // A sound played since the last sweep stays through one more, as a prefab does; then nothing
    // plays the waterfall's, and it goes. The chime's Audio Source still holds the chime.
    assert_eq!(game.release_unused(None).sounds, 0);
    assert_eq!(game.release_unused(None).sounds, 1);
    assert_eq!(game.content.loaded_sounds(), 1);
}

#[test]
fn one_shots_play_in_2d_or_at_a_place() {
    let dir = project();
    let (mut game, mut mixer) = game(&dir);
    game.engine.world.despawn(obj(&game, "Waterfall"));
    game.tick(1.0 / 60.0, None);
    let reference = game.content.parse("Sounds/Chime.wav").unwrap();
    let chime = game.content.sound(&reference).unwrap();
    let left = OneShot { at: Some((Vec3::new(-5.0, 0.0, 0.0), Default::default())), ..OneShot::default() };
    game.audio.play(&mut game.content, chime, left).unwrap();
    let out = render(&mut mixer, 10);
    assert!(out[0] > 0.01 && out[1].abs() < 1e-4, "{:?}", &out[..2]);
}

#[test]
fn scenes_save_audio_sources_and_listeners_back() {
    let dir = project();
    let (mut game, _) = game(&dir);
    let waterfall = obj(&game, "Waterfall");
    game.engine.world.insert(waterfall, AudioListener);
    let entities: Vec<Entity> = game.engine.world.query_ref::<Entity>().into_iter().collect();
    let scene = scene_file::capture(&game.engine.world, &mut game.content, &entities);
    let saved = scene.objs.iter().find(|o| o.name == "Waterfall").unwrap();
    let audio = saved.audio.as_ref().unwrap();
    let guid = game.content.db().guid_of("Sounds/Tone.wav").unwrap();
    assert_eq!(audio.sound.as_deref(), Some(guid.to_string().as_str()), "saved by GUID");
    assert!(audio.looping && audio.max_distance == 40.0);
    assert!(saved.audio_listener);
    let text = toml::to_string(&scene).unwrap();
    assert!(text.contains("loop = true") && !text.contains("volume"), "only what differs: {text}");
    assert!(text.contains("mode = \"2d\"") && text.contains("bus = \"music\""), "{text}");

    // It reads back the same.
    let again = scene_file::parse(&text, "saved").unwrap();
    let chime = |scene: &scene_file::SceneFile| scene.objs.iter().find(|o| o.name == "Chime").unwrap().audio.clone();
    assert_eq!(chime(&again), chime(&scene));
}

#[test]
fn the_listener_is_an_audio_listener_or_else_the_camera() {
    let dir = project();
    let (mut game, _) = game(&dir);
    let heard = voxear_engine::audio::listener(&game.engine.world).unwrap();
    assert_eq!(heard.position, Vec3::ZERO, "the camera's");
    game.engine.world.insert(obj(&game, "Waterfall"), AudioListener);
    game.tick(1.0 / 60.0, None);
    let heard = voxear_engine::audio::listener(&game.engine.world).unwrap();
    assert_eq!(heard.position, Vec3::new(10.0, 0.0, 0.0), "the Audio Listener's");
}

#[test]
fn built_content_packs_the_sounds_scenes_use_with_their_settings() {
    let dir = project();
    let tone = dir.path().join("Sounds/Tone.wav");
    // Scanned first, so it has a .meta to write settings into.
    Project::open(dir.path()).unwrap();
    voxear_assets::AssetDb::scan(dir.path()).unwrap();
    SoundImport { load: SoundLoad::Stream, volume: 0.5 }.write(&tone).unwrap();

    let out = dir.path().join("Built");
    let report = content_build::build(dir.path(), &out, &ContentSettings::default(), &[]).unwrap();
    let packed: Vec<&String> = report.bundles.iter().flat_map(|b| &b.assets).collect();
    assert!(packed.iter().any(|p| *p == "Sounds/Tone.wav") && packed.iter().any(|p| *p == "Sounds/Chime.wav"));
    assert!(report.excluded.contains(&"Sounds/Unused.wav".to_string()), "nothing plays it: {:?}", report.excluded);

    let mut content = voxear_engine::Content::open_bundles(dir.path(), &out).unwrap();
    let id = content.sound(&content.parse("Sounds/Tone.wav").unwrap()).unwrap();
    let sound = content.loaded_sound(id).unwrap();
    assert!(sound.is_streamed(), "its .meta shipped beside it");
    assert_eq!(sound.volume, 0.5);
    assert_eq!(sound.info().frames, u64::from(RATE));
}
