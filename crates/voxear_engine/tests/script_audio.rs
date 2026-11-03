//! A script loads a sound, plays it once at a place, gives an Obj an Audio
//! Source and plays that, and turns the music down, through `Sound`,
//! `Audio` and `AudioSource`. Builds a copy of Hello's scripts with one
//! more; skips itself without .NET and the managed build. The mixer is
//! rendered by hand, so no sound card is needed.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    audio::{Audio, AudioSource, Bus, GameAudio, SoundSpace},
    scripts,
};
use voxear_scene::Name;

const SCRIPT: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Noises : SystemBase
{
	protected override void OnCreate()
	{
		var chime = Sound.Load("Sounds/Chime.wav");
		Log.Info($"chime {chime.IsValid}, {chime.Length:0.00} s, {chime.Channels} channel, {chime.SampleRate} Hz, streamed {chime.IsStreamed}");
		var shot = Audio.Play(chime, at: new Vector3(-3f, 0f, 0f), volume: 0.5f);
		Log.Info($"shot playing {shot.IsPlaying}, volume {shot.Volume}, pitch {shot.Pitch}");

		var source = Obj.Find("Ground").AddAudioSource(chime);
		source.Loop = true;
		source.Is3D = false;
		source.Volume = 0.25f;
		source.Bus = AudioBus.Music;
		source.Play();
		Log.Info($"source {source.IsValid}, playing {source.IsPlaying}, loop {source.Loop}, 3D {source.Is3D}, volume {source.Volume}, same sound {source.Sound == chime}");
		Log.Info($"the camera has one: {Obj.Find("Main Camera").AudioSource.IsValid}");

		Audio.SetLowPass(AudioBus.Music, 800f);
		Audio.Ducking = new Ducking(-12f, Release: 1f);
		Log.Info($"low pass {Audio.GetLowPass(AudioBus.Music)}, ducking {Audio.Ducking?.Amount} {Audio.Ducking?.Release}");
		Audio.TransitionTo(AudioSnapshot.Default, 0f);
		Audio.Ducking = null;
		Log.Info($"after {Audio.GetLowPass(AudioBus.Music)}, ducking {Audio.Ducking == null}");

		Audio.MusicVolume = 0.5f;
		Log.Info($"music at {Audio.MusicVolume}");

		Audio.Headphones = true;
		Log.Info($"headphones {Audio.Headphones}, available {Audio.HeadphonesAvailable}");
		Audio.Headphones = false;
	}

	protected override void Update()
	{
	}
}
"#;

/// A 16-bit mono WAV at 48 kHz holding one level for `frames`.
fn wav(level: i16, frames: usize) -> Vec<u8> {
    let data = (frames * 2) as u32;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for value in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
        file.extend_from_slice(&value);
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&data.to_le_bytes());
    (0..frames).for_each(|_| file.extend_from_slice(&level.to_le_bytes()));
    file
}

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against. Its own sounds stay out, as large files the
/// test does not need.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let sound = name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".mp3");
            if name.starts_with('.') || name == "bin" || name == "obj" || sound {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

#[test]
fn scripts_play_sounds_once_and_from_audio_sources() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Noises.cs"), SCRIPT).unwrap();
    std::fs::create_dir_all(dir.path().join("Sounds")).unwrap();
    std::fs::write(dir.path().join("Sounds/Chime.wav"), wav(16_384, 48_000)).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    let (audio, mut mixer) = Audio::offline(48_000);
    game.audio = GameAudio::new(audio);
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    let messages: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |line: &str| assert!(messages.iter().any(|m| m == line), "no `{line}` in {messages:#?}");
    said("chime True, 1.00 s, 1 channel, 48000 Hz, streamed False");
    said("shot playing True, volume 0.5, pitch 1");
    said("source True, playing True, loop True, 3D False, volume 0.25, same sound True");
    said("the camera has one: False");
    said("music at 0.5");
    said("low pass 800, ducking -12 1");
    said("after 22000, ducking True");
    let steam = if voxear_audio::steam_audio::SteamAudio::get().is_ok() { "True" } else { "False" };
    said(&format!("headphones {steam}, available {steam}"));

    let world = &game.engine.world;
    let ground = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Ground").unwrap().0;
    let source = *world.get::<AudioSource>(ground).unwrap();
    assert!(source.looping && source.space == SoundSpace::TwoD && source.volume == 0.25);
    assert!(game.audio.is_source_playing(world, ground));
    assert_eq!(game.audio.audio.voices(), 2, "the shot, and the source");
    assert_eq!(game.audio.audio.bus_volume(Bus::Music), 0.5);

    // The source plays on both sides, its 0.5 at a quarter through music at half; the shot is
    // quieter, off to the camera's left.
    let mut out = vec![0.0; 128];
    mixer.render(&mut out, 2);
    let (left, right) = (out[126], out[127]);
    assert!(right >= 0.0625 - 1e-3 && left > right, "{left} {right}");
}
