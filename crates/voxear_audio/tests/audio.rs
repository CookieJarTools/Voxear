//! Sound with no sound card: files decoded, and the mixer rendered by hand
//! into buffers.

use std::{path::Path, sync::Arc, time::Duration};

use voxear_audio::{Audio, Bus, Distance, Listener, Mixer, Sound, VoiceParams, probe};
use voxear_core::math::{Quat, Vec3};

const RATE: u32 = 48_000;

/// A 16-bit PCM WAV file of `samples`, interleaved.
fn wav(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&channels.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    file.extend_from_slice(&(channels * 2).to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&data.to_le_bytes());
    samples.iter().for_each(|s| file.extend_from_slice(&s.to_le_bytes()));
    file
}

/// A mono sound holding one level throughout, `frames` long.
fn level(value: f32, frames: usize, rate: u32) -> Arc<Sound> {
    Arc::new(Sound::from_samples(vec![value; frames], 1, rate))
}

/// Renders `frames` of stereo.
fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    mixer.render(&mut out, 2);
    out
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn tone(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sounds").join(name)).unwrap()
}

#[test]
fn a_wav_file_decodes_to_its_samples() {
    let samples: Vec<i16> = vec![0, 16384, -16384, 32767, 0, -32768];
    let file = wav(&samples, 2, 22_050);
    let info = probe(&file, Some("wav")).unwrap();
    assert_eq!((info.channels, info.sample_rate, info.frames), (2, 22_050, 3));

    let sound = Sound::decode(&file, Some("wav")).unwrap();
    assert!(!sound.is_streamed());
    let decoded = sound.samples().unwrap();
    assert_eq!(decoded.len(), 6);
    for (got, want) in decoded.iter().zip(&samples) {
        assert!((got - f32::from(*want) / 32768.0).abs() < 1e-4, "{decoded:?}");
    }
    assert!(Sound::decode(b"not a sound", Some("wav")).is_err());
}

#[test]
fn ogg_flac_and_mp3_files_decode() {
    // A quarter of a second of 440 Hz in each, as ffmpeg made them.
    for (name, extension, channels, rate) in
        [("Tone.ogg", "ogg", 1, 22_050), ("Tone.flac", "flac", 2, 44_100), ("Tone.mp3", "mp3", 1, 44_100)]
    {
        let file = tone(name);
        let info = probe(&file, Some(extension)).unwrap();
        assert_eq!((info.channels, info.sample_rate), (channels, rate), "{name}");
        assert!((info.seconds() - 0.25).abs() < 0.06, "{name} lasts {}", info.seconds());

        let sound = Sound::decode(&file, Some(extension)).unwrap();
        let samples = sound.samples().unwrap();
        let loudest = samples.iter().fold(0.0f32, |most, s| most.max(s.abs()));
        // ffmpeg makes its tones at an eighth of full scale.
        assert!(loudest > 0.05 && loudest < 0.2, "{name} holds a tone: {loudest}");
        assert_eq!(samples.len() as u64, sound.info().frames * u64::from(channels), "{name}");
    }
}

#[test]
fn a_2d_voice_plays_at_its_volume_and_ends() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(0.5, 1000, RATE);
    let voice = audio.play(&sound, VoiceParams { volume: 0.5, ..VoiceParams::default() });
    assert!(audio.is_playing(voice));

    let out = render(&mut mixer, 600);
    assert!(close(out[0], 0.25) && close(out[1], 0.25), "{:?}", &out[..4]);
    let out = render(&mut mixer, 600);
    assert!(close(out[2 * 399], 0.25), "the last frame plays");
    assert_eq!(out[2 * 400], 0.0, "then it has ended");
    assert_eq!(mixer.voices(), 0);
    audio.update();
    assert!(!audio.is_playing(voice) && !audio.exists(voice));
}

#[test]
fn buses_and_master_scale_their_voices() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(1.0, RATE as usize, RATE);
    audio.set_bus_volume(Bus::Effects, 0.5);
    audio.set_bus_volume(Bus::Music, 0.25);
    audio.set_master_volume(0.5);
    audio.play(&sound, VoiceParams::default());
    let out = render(&mut mixer, 100);
    assert!(close(out[0], 0.25), "effects × master: {}", out[0]);

    audio.play(&sound, VoiceParams { bus: Bus::Music, ..VoiceParams::default() });
    let out = render(&mut mixer, 100);
    assert!(close(out[0], 0.25 + 0.125), "and music × master: {}", out[0]);

    audio.play(&sound, VoiceParams { bus: Bus::Preview, ..VoiceParams::default() });
    let out = render(&mut mixer, 100);
    assert!(close(out[0], 0.375 + 1.0), "the editor's sounds have no bus: {}", out[0]);
    assert_eq!(audio.bus_volume(Bus::Music), 0.25);
}

#[test]
fn a_volume_change_slides_across_one_block() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let voice = audio.play(&level(1.0, RATE as usize, RATE), VoiceParams::default());
    render(&mut mixer, 100);
    audio.set_volume(voice, 0.0);
    let out = render(&mut mixer, 100);
    assert!(out[0] > 0.9 && out[2 * 50] > 0.4 && out[2 * 50] < 0.6, "halfway down halfway through");
    assert!(out[2 * 99] < 0.02);
    let out = render(&mut mixer, 100);
    assert!(out.iter().all(|&s| s == 0.0));
}

#[test]
fn a_looping_voice_starts_again_at_its_end() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    // A ramp, so where it is in the sound shows.
    let ramp: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
    let sound = Arc::new(Sound::from_samples(ramp, 1, RATE));
    let voice = audio.play(&sound, VoiceParams { looping: true, ..VoiceParams::default() });
    let out = render(&mut mixer, 250);
    assert!(close(out[2 * 150], 0.5), "round again: {}", out[2 * 150]);
    audio.update();
    assert!(audio.is_playing(voice));

    audio.set_looping(voice, false);
    render(&mut mixer, 100);
    audio.update();
    assert!(!audio.is_playing(voice), "it ends at its next end");
}

#[test]
fn a_voice_coming_towards_the_listener_plays_faster() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let ahead = Some((Vec3::new(0.0, 0.0, 10.0), Distance { min: 20.0, max: 50.0 }));
    let closing = Vec3::new(0.0, 0.0, -343.0 / 3.0);
    let params = VoiceParams { spatial: ahead, velocity: closing, ..VoiceParams::default() };
    let coming = audio.play(&level(1.0, 400, RATE), params);
    let still = audio.play(&level(1.0, 400, RATE), VoiceParams { spatial: ahead, ..VoiceParams::default() });
    // At 343 / (343 - 114.3), half as fast again, its 400 frames last about 267.
    render(&mut mixer, 300);
    audio.update();
    assert!(!audio.exists(coming) && audio.exists(still), "the one coming closer ended first");

    // With its Doppler level at 0, it plays as it is.
    let off = VoiceParams { spatial: ahead, velocity: closing, doppler: 0.0, ..VoiceParams::default() };
    let level_off = audio.play(&level(1.0, 400, RATE), off);
    render(&mut mixer, 300);
    audio.update();
    assert!(audio.exists(level_off));
}

#[test]
fn sounds_at_another_rate_or_pitch_are_resampled() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    // 100 frames at half the output's rate last 200 output frames.
    let voice = audio.play(&level(1.0, 100, RATE / 2), VoiceParams::default());
    let out = render(&mut mixer, 300);
    assert!(out[2 * 198] > 0.9 && out[2 * 201] == 0.0, "{} {}", out[2 * 198], out[2 * 201]);
    audio.update();
    assert!(!audio.exists(voice));

    // At twice the pitch, 100 frames last 50.
    audio.play(&level(1.0, 100, RATE), VoiceParams { pitch: 2.0, ..VoiceParams::default() });
    let out = render(&mut mixer, 100);
    assert!(out[2 * 49] > 0.9 && out[2 * 51] == 0.0);

    // Between two samples, the voice reads between them.
    let sound = Arc::new(Sound::from_samples(vec![0.0, 1.0, 1.0, 1.0], 1, RATE / 2));
    audio.play(&sound, VoiceParams::default());
    let out = render(&mut mixer, 4);
    assert!(close(out[2], 0.5), "{out:?}");
}

#[test]
fn a_3d_voice_pans_and_fades_from_the_listener() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(1.0, RATE as usize, RATE);
    let distance = Distance { min: 1.0, max: 20.0 };
    let right =
        audio.play(&sound, VoiceParams { spatial: Some((Vec3::new(4.0, 0.0, 0.0), distance)), ..Default::default() });
    let out = render(&mut mixer, 10);
    let fade = distance.falloff(4.0);
    assert!(close(out[0], 0.0) && close(out[1], fade), "on the right, faded: {:?}", &out[..2]);

    // The listener turns to face it: now it is ahead, on both sides.
    audio.set_listener(Listener { rotation: Quat::from_rotation_y(90f32.to_radians()), ..Listener::default() });
    render(&mut mixer, 10);
    let out = render(&mut mixer, 10);
    assert!(close(out[0], out[1]) && out[0] > 0.0, "{:?}", &out[..2]);

    // Moved out of reach, it is silent but still plays.
    audio.set_position(right, Vec3::new(40.0, 0.0, 0.0));
    render(&mut mixer, 10);
    let out = render(&mut mixer, 10);
    assert!(out.iter().all(|&s| s == 0.0));
    audio.update();
    assert!(audio.is_playing(right));
}

#[test]
fn a_stopped_voice_fades_out_over_a_block() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let voice = audio.play(&level(1.0, RATE as usize, RATE), VoiceParams::default());
    render(&mut mixer, 64);
    audio.stop(voice);
    assert!(!audio.is_playing(voice), "stopped at once, as far as the game knows");
    let out = render(&mut mixer, 64);
    assert!(out[0] > 0.9 && out[2 * 63] < 0.05, "faded, not cut: {} … {}", out[0], out[2 * 63]);
    assert_eq!(mixer.voices(), 0);
}

#[test]
fn a_paused_voice_keeps_its_place() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
    let voice = audio.play(&Arc::new(Sound::from_samples(ramp, 1, RATE)), VoiceParams::default());
    render(&mut mixer, 100);
    audio.set_paused(voice, true);
    assert!(!audio.is_playing(voice) && audio.exists(voice));
    render(&mut mixer, 100); // fades out, moving on 100 frames
    for _ in 0..5 {
        assert!(render(&mut mixer, 100).iter().all(|&s| s == 0.0), "held");
    }
    audio.set_paused(voice, false);
    let out = render(&mut mixer, 100);
    // It fades back in from where it was held, 200 frames in.
    assert!(close(out[2 * 99], 0.299), "{}", out[2 * 99]);
}

#[test]
fn the_games_pause_and_stop_leave_the_editors_sounds_alone() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(1.0, RATE as usize, RATE);
    let game = audio.play(&sound, VoiceParams::default());
    let preview = audio.play(&sound, VoiceParams { volume: 0.5, bus: Bus::Preview, ..VoiceParams::default() });
    render(&mut mixer, 10);
    audio.pause_game(true);
    render(&mut mixer, 10);
    let out = render(&mut mixer, 10);
    assert!(close(out[0], 0.5), "only the preview plays: {}", out[0]);

    audio.stop_game();
    assert!(!audio.exists(game) && audio.exists(preview));
    assert!(!audio.is_game_paused());
    render(&mut mixer, 10);
    assert_eq!(mixer.voices(), 1);
}

#[test]
fn a_voice_past_the_limit_takes_the_oldest_effects_place() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(0.001, RATE as usize, RATE);
    let music = audio.play(&sound, VoiceParams { bus: Bus::Music, ..VoiceParams::default() });
    let first = audio.play(&sound, VoiceParams::default());
    for _ in 2..voxear_audio::MAX_VOICES {
        audio.play(&sound, VoiceParams::default());
    }
    render(&mut mixer, 10);
    assert_eq!(mixer.voices(), voxear_audio::MAX_VOICES);
    let last = audio.play(&sound, VoiceParams::default());
    render(&mut mixer, 10);
    audio.update();
    assert_eq!(mixer.voices(), voxear_audio::MAX_VOICES);
    assert!(audio.exists(music) && audio.exists(last) && !audio.exists(first));
}

#[test]
fn a_streamed_sound_plays_the_same_samples_as_a_decoded_one() {
    // Longer than a few blocks, so the streaming thread has to keep up.
    let frames = 20_000;
    let samples: Vec<i16> = (0..frames).map(|i| ((i % 200) as i16 - 100) * 100).collect();
    let file: Arc<[u8]> = wav(&samples, 1, RATE).into();
    let decoded = Arc::new(Sound::decode(&file, Some("wav")).unwrap());
    let streamed = Arc::new(Sound::streamed(file.clone(), Some("wav")).unwrap());
    assert!(streamed.is_streamed() && streamed.samples().is_none());
    assert_eq!(streamed.info(), decoded.info());
    assert!(streamed.bytes() < decoded.bytes());

    let played = |sound: &Arc<Sound>| {
        let (mut audio, mut mixer) = Audio::offline(RATE);
        audio.play(sound, VoiceParams::default());
        let mut out = Vec::new();
        while out.len() < frames * 2 {
            out.extend(render(&mut mixer, 500));
            // At about the pace of a device, which the streaming thread stays well ahead of.
            std::thread::sleep(Duration::from_millis(3));
        }
        out
    };
    let (a, b) = (played(&decoded), played(&streamed));
    assert!(a.iter().any(|&s| s != 0.0));
    assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5), "the same samples in the same order");
}

#[test]
fn a_looping_stream_goes_round_without_end() {
    let samples: Vec<i16> = vec![8000; 5000];
    let streamed = Arc::new(Sound::streamed(wav(&samples, 1, RATE).into(), Some("wav")).unwrap());
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let voice = audio.play(&streamed, VoiceParams { looping: true, ..VoiceParams::default() });
    let mut heard = 0;
    for _ in 0..200 {
        heard += render(&mut mixer, 480).chunks(2).filter(|frame| frame[0] > 0.2).count();
        std::thread::sleep(Duration::from_millis(1));
    }
    audio.update();
    assert!(audio.is_playing(voice));
    assert!(heard > 20_000, "it went round several times: {heard} frames");
}

/// A mono sound of noise, `frames` long.
fn noise(frames: usize, rate: u32) -> Arc<Sound> {
    let mut seed = 7u32;
    let samples = (0..frames)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5
        })
        .collect();
    Arc::new(Sound::from_samples(samples, 1, rate))
}

#[test]
fn with_headphones_a_3d_voice_is_heard_through_the_hrtf_and_plays_out() {
    if voxear_audio::steam_audio::SteamAudio::get().is_err() {
        return eprintln!("skipping: Steam Audio's library is not fetched");
    }
    let (mut audio, mut mixer) = Audio::offline(RATE);
    audio.set_headphones(true);
    let at = Vec3::new(5.0, 0.0, 0.0);
    let voice = audio.play(
        &noise(RATE as usize, RATE),
        VoiceParams { spatial: Some((at, Distance::default())), ..Default::default() },
    );
    let started = std::time::Instant::now();
    while !mixer.hears_binaurally() {
        assert!(started.elapsed() < Duration::from_secs(10), "the HRTF is made");
        render(&mut mixer, 256);
        std::thread::sleep(Duration::from_millis(5));
    }
    render(&mut mixer, 1024);
    let out = render(&mut mixer, 4096);
    let energy = |side: usize| out.as_chunks::<2>().0.iter().map(|frame| frame[side] * frame[side]).sum::<f32>();
    assert!(energy(1) > energy(0) * 2.0, "louder on the right: {} {}", energy(1), energy(0));
    assert!(energy(0) > 0.0, "and heard a little on the left, unlike hard panning");

    // Stopped, it plays out what the effect holds, then ends.
    audio.stop(voice);
    let out = render(&mut mixer, 64);
    assert!(out.iter().any(|&s| s != 0.0), "not cut off");
    render(&mut mixer, 512);
    assert_eq!(mixer.voices(), 0);

    // Off again, a new voice pans as before.
    audio.set_headphones(false);
    let sound = level(1.0, RATE as usize, RATE);
    audio.play(&sound, VoiceParams { spatial: Some((at, Distance::default())), ..Default::default() });
    let out = render(&mut mixer, 10);
    assert!(close(out[0], 0.0) && out[1] > 0.0, "panned hard right: {:?}", &out[..2]);
}

#[test]
fn a_watched_mixer_reports_its_voices() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(1.0, RATE as usize, RATE);
    let at = Vec3::new(4.0, 0.0, 0.0);
    audio.play(&sound, VoiceParams { volume: 0.5, looping: true, bus: Bus::Music, ..Default::default() });
    audio.play(&sound, VoiceParams { spatial: Some((at, Distance::default())), ..Default::default() });
    render(&mut mixer, 64);
    assert!(audio.voice_reports().is_empty(), "nothing said while nobody watches");

    audio.watch_voices(true);
    render(&mut mixer, 64);
    let reports = audio.voice_reports();
    assert_eq!(reports.len(), 2);
    let music = reports.iter().find(|r| r.bus == Bus::Music).unwrap();
    assert!(music.looping && close(music.volume, 0.5) && close(music.heard, 0.5) && music.at.is_none());
    let placed = reports.iter().find(|r| r.bus == Bus::Effects).unwrap();
    assert_eq!(placed.at, Some(at));
    assert!(placed.heard > 0.0 && placed.heard < 1.0 && !placed.paused && !placed.ending);
}

#[test]
fn a_transition_slides_the_volumes_and_a_low_pass_dulls_a_bus() {
    let (mut audio, mut mixer) = Audio::offline(RATE);
    let sound = level(1.0, RATE as usize * 2, RATE);
    audio.play(&sound, VoiceParams { bus: Bus::Music, looping: true, ..Default::default() });
    render(&mut mixer, 480);
    audio.set_transition(1.0);
    audio.set_bus_volume(Bus::Music, 0.0);
    render(&mut mixer, 24_000);
    let halfway = render(&mut mixer, 10)[0];
    assert!((halfway - 0.5).abs() < 0.05, "half way down after half the second: {halfway}");
    render(&mut mixer, 24_000);
    assert!(render(&mut mixer, 10)[0].abs() < 1e-3, "down after the second");

    // A low-pass well below a sound's notes holds it back; a steady level is below every note.
    audio.set_transition(0.0);
    audio.set_bus_volume(Bus::Music, 1.0);
    audio.set_low_pass(Bus::Music, 200.0);
    assert_eq!(audio.filters(Bus::Music), [200.0, voxear_audio::OPEN_HIGH_PASS]);
    render(&mut mixer, 4_800);
    let steady = render(&mut mixer, 10)[0];
    assert!((steady - 1.0).abs() < 0.02, "a steady level passes a low-pass: {steady}");
    audio.set_high_pass(Bus::Music, 200.0);
    render(&mut mixer, 9_600);
    assert!(render(&mut mixer, 10)[0].abs() < 0.02, "and not a high-pass");
}
