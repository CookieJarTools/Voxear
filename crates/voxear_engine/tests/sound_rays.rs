//! Ray-traced audio in a running game on a real GPU, with sound mixed by
//! hand: a source behind a concrete wall drawn by a model, with no collider,
//! is muffled by what concrete lets through, and comes back whole as the
//! wall goes; the listener's room rings once it is closed round it, and
//! not on open ground; and a level's baked reverb probes load with it and
//! give the room its reverb when no rays are traced. GPU tests skip
//! themselves (with a note) on machines without a usable Vulkan device.

use voxear_audio::Mixer;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    audio::{Audio, GameAudio, ReverbTimes},
    reverb_probes::{BakedReverb, ProbeGrid, reverb_file},
};
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;

const RATE: u32 = 48_000;

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

/// A slab of concrete drawn as a cube, `name`d, at `at` and `size`d.
fn slab(id: u32, name: &str, at: [f32; 3], size: [f32; 3]) -> String {
    format!(
        "[[obj]]\nid = {id}\nname = \"{name}\"\nposition = {at:?}\nscale = {size:?}\n[obj.renderer]\nmesh = \"builtin:cube\"\nmaterial = \"Materials/Concrete.material\"\n"
    )
}

/// A project whose scene is a camera looking along +Z, a source 6 m ahead
/// with occlusion on, and these slabs, with ray-traced audio and reverb
/// on as `audio` says.
fn project(slabs: &[String], audio: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, bytes: &[u8]| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    };
    let toml = format!("[project]\nname = \"Rays\"\nstart_level = \"Levels/Main.level\"\n[audio]\n{audio}");
    write("voxear.toml", toml.as_bytes());
    write("Levels/Main.level", b"format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
    write("Materials/Concrete.material", b"color = [0.5, 0.5, 0.5]\nacoustic = \"concrete\"\n");
    let mut scene = String::from(
        "format = 1\n[[obj]]\nid = 1\nname = \"Main Camera\"\nposition = [0.0, 0.0, 0.0]\n[obj.camera]\nfov = 60.0\n\
         [[obj]]\nid = 2\nname = \"Hum\"\nposition = [0.0, 0.0, 6.0]\n[obj.audio]\nsound = \"Sounds/Tone.wav\"\nloop = true\n\
         max_distance = 40.0\nocclusion = \"raycast\"\n",
    );
    for slab in slabs {
        scene.push_str(slab);
    }
    write("Scenes/Main.scene", scene.as_bytes());
    write("Sounds/Tone.wav", &wav(16_384, RATE as usize));
    dir
}

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(64, 64, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn game(dir: &tempfile::TempDir, renderer: &mut Renderer) -> (Game, Mixer) {
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let project = game.audio.tracer.project;
    let reverb = game.audio.reverb;
    let (audio, mixer) = Audio::offline(RATE);
    game.audio = GameAudio::new(audio);
    game.audio.tracer.project = project;
    game.audio.reverb = reverb;
    game.audio.audio.watch_voices(true);
    game.start(Some(renderer)).unwrap();
    game.audio.start_game();
    (game, mixer)
}

fn frames(game: &mut Game, renderer: &mut Renderer, mixer: &mut Mixer, count: u32, time: &mut f32) {
    let mut out = vec![0.0; 2 * 400];
    for _ in 0..count {
        *time += 1.0 / 60.0;
        game.tick(1.0 / 60.0, Some(renderer));
        renderer.render(&game.engine.world, *time).unwrap();
        mixer.render(&mut out, 2);
    }
}

fn obj(game: &Game, name: &str) -> Entity {
    let world = &game.engine.world;
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap()
}

/// The middle band of what reaches the listener of the Hum, as the mixer has it.
fn heard(game: &Game) -> Option<f32> {
    let voices = game.audio.heard_voices(&game.engine.world, &game.content);
    let hum = obj(game, "Hum");
    voices.into_iter().find(|voice| voice.source == Some(hum)).map(|voice| voice.report.muffle.map_or(1.0, |m| m.0[1]))
}

#[test]
fn a_source_behind_a_drawn_wall_is_heard_through_it_as_concrete_lets_it() {
    let Some(mut renderer) = renderer() else { return };
    // A wall 20 cm thick across the way at z = 3, drawn by a model, with no collider for Steam Audio to hear.
    let dir = project(&[slab(3, "Wall", [0.0, 0.0, 3.0], [10.0, 10.0, 0.2])], "ray_traced = true\n");
    let (mut game, mut mixer) = game(&dir, &mut renderer);
    let mut time = 0.0;
    frames(&mut game, &mut renderer, &mut mixer, 12, &mut time);
    assert!(game.audio.tracer.tracing(), "traced by rays");
    let through = heard(&game).expect("the Hum plays");
    assert!((through - 0.002).abs() < 1e-3, "concrete's middle band gets through: {through}");

    // Gone, the wall lets it all through.
    let wall = obj(&game, "Wall");
    game.engine.world.despawn(wall);
    frames(&mut game, &mut renderer, &mut mixer, 12, &mut time);
    let open = heard(&game).unwrap();
    assert!(open > 0.99, "in the open: {open}");
}

/// The six sides of a closed concrete room `half` metres from its middle each way, round the camera.
fn room(half: f32) -> Vec<String> {
    let (h, t, s) = (half, 0.2, 2.0 * half);
    let sides = [
        ([0.0, -h, 0.0], [s, t, s]),
        ([0.0, h, 0.0], [s, t, s]),
        ([-h, 0.0, 0.0], [t, s, s]),
        ([h, 0.0, 0.0], [t, s, s]),
        ([0.0, 0.0, -h], [s, s, t]),
        ([0.0, 0.0, h], [s, s, t]),
    ];
    sides.iter().enumerate().map(|(i, (at, size))| slab(10 + i as u32, &format!("Side {i}"), *at, *size)).collect()
}

#[test]
fn the_listeners_room_rings_once_it_is_closed_round_it_and_not_on_open_ground() {
    let Some(mut renderer) = renderer() else { return };
    let ring = |slabs: Vec<String>, renderer: &mut Renderer| -> ReverbTimes {
        let dir = project(&slabs, "ray_traced = true\nray_quality = \"high\"\nreverb = true\n");
        let (mut game, mut mixer) = game(&dir, renderer);
        let mut time = 0.0;
        frames(&mut game, renderer, &mut mixer, 40, &mut time);
        assert!(game.audio.reverb_on() && game.audio.tracer.tracing());
        game.audio.room()
    };
    let closed = ring(room(10.0), &mut renderer);
    let ground = ring(vec![slab(10, "Ground", [0.0, -2.0, 0.0], [60.0, 0.2, 60.0])], &mut renderer);
    eprintln!("hall {closed:?}, open ground {ground:?}");
    // A 20 m hall of concrete rings for seconds, as Steam Audio has it; the high band, which the air takes, less.
    assert!(closed.0[1] > 1.0 && closed.0[2] < closed.0[0], "the hall: {closed:?}");
    assert!(ground.0[1] < 0.2, "open ground: {ground:?}");
}

#[test]
fn a_levels_baked_reverb_probes_load_with_it_and_ring_its_room_without_rays() {
    // Two probes 4 m apart: a hall's long ring at the camera, a closet's short one beside it.
    let grid = ProbeGrid::new(Vec3::new(2.0, 0.0, 0.0), [8.0, 4.0, 4.0], 4.0);
    let times = vec![Some(ReverbTimes([2.4, 2.0, 1.2])), Some(ReverbTimes([0.5, 0.4, 0.3]))];
    let baked = BakedReverb { boxes: vec![(grid, times)] };
    let dir = project(&[], "reverb = true\n");
    std::fs::write(dir.path().join(reverb_file("Levels/Main.level")), baked.write()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let (audio, mut mixer) = Audio::offline(RATE);
    game.audio = GameAudio::new(audio);
    game.audio.reverb = true;
    game.start(None).unwrap();
    game.audio.start_game();
    let mut out = vec![0.0; 2 * 400];
    for _ in 0..4 {
        game.tick(1.0 / 60.0, None);
        mixer.render(&mut out, 2);
    }
    let loaded = game.audio.reverb_probes().expect("loaded with the level");
    assert_eq!(*loaded, baked);
    assert!(!game.audio.tracer.tracing(), "no rays without a renderer");
    let room = game.audio.room();
    assert!((room.0[1] - 2.0).abs() < 1e-2, "the hall's probe, at the camera: {room:?}");
}

/// A WAV of noise for `frames`, as a hand clap is: loud, then gone.
fn clap(frames: usize) -> Vec<u8> {
    let mut seed = 7u32;
    let mut file = wav(0, frames);
    for (i, sample) in file[44..].chunks_exact_mut(2).enumerate() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let fade = 1.0 - i as f32 / frames as f32;
        let level = ((seed >> 16) as f32 / 65_536.0 - 0.5) * 2.0 * 20_000.0 * fade;
        sample.copy_from_slice(&(level as i16).to_le_bytes());
    }
    file
}

/// Writes stereo samples as a 16-bit WAV.
fn write_wav(path: &std::path::Path, samples: &[f32]) {
    let data = (samples.len() * 2) as u32;
    let mut file = Vec::new();
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + data).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for value in [16u32.to_le_bytes(), [1, 0, 2, 0], RATE.to_le_bytes(), (RATE * 4).to_le_bytes(), [4, 0, 16, 0]] {
        file.extend_from_slice(&value);
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        file.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
    }
    std::fs::write(path, file).unwrap();
}

/// How long the rendered sound takes to fall by 60 dB after its loudest, from its fall from -5 to -25 dB, in
/// seconds; and its loudness, the root of its mean square.
fn decay_and_level(samples: &[f32]) -> (f32, f32) {
    let window = (RATE / 100) as usize * 2;
    let energy: Vec<f32> = samples.chunks(window).map(|w| w.iter().map(|s| s * s).sum::<f32>()).collect();
    let peak = energy.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(0, |(at, _)| at);
    let left: Vec<f32> = (0..energy.len()).map(|at| energy[at..].iter().sum()).collect();
    let db = |at: usize| 10.0 * (left[at] / left[peak]).max(1e-12).log10();
    let reach = |target: f32| (peak..energy.len()).find(|&at| db(at) <= target).unwrap_or(energy.len()) as f32;
    let rt60 = (reach(-25.0) - reach(-5.0)) * 0.01 * 3.0;
    let level = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    (rt60, level)
}

/// Renders three scenes through the mixer and writes them, with what was measured of each, where
/// `VOXEAR_TEST_OUTPUT` says: a clap in a 20 m concrete hall and on open ground, and a hum behind a concrete
/// wall and with it gone. Run with `--ignored --nocapture`.
#[test]
#[ignore = "a listening check, not a check"]
fn sound_rays_by_ear() {
    let Some(mut renderer) = renderer() else { return };
    let out = std::path::PathBuf::from(std::env::var("VOXEAR_TEST_OUTPUT").unwrap_or_else(|_| ".".into()));
    let mut listen = |slabs: Vec<String>, audio: &str, clapped: bool, name: &str| {
        let dir = project(&slabs, audio);
        if clapped {
            std::fs::write(dir.path().join("Sounds/Tone.wav"), clap(RATE as usize / 20)).unwrap();
            let scene = std::fs::read_to_string(dir.path().join("Scenes/Main.scene")).unwrap();
            let scene = scene
                .replace("loop = true", "loop = false")
                .replace("position = [0.0, 0.0, 6.0]", "position = [0.0, 0.0, 3.0]");
            std::fs::write(dir.path().join("Scenes/Main.scene"), scene).unwrap();
        }
        let (mut game, mut mixer) = game(&dir, &mut renderer);
        let mut time = 0.0;
        let mut samples = Vec::new();
        let mut block = vec![0.0; 2 * 800];
        // A moment for the rays to settle before the sound starts, then three seconds.
        let hum = obj(&game, "Hum");
        game.audio.request(&game.engine.world, hum, voxear_engine::audio::SourceRequest::Stop);
        frames(&mut game, &mut renderer, &mut mixer, 40, &mut time);
        game.audio.request(&game.engine.world, hum, voxear_engine::audio::SourceRequest::Play);
        for _ in 0..180 {
            time += 1.0 / 60.0;
            game.tick(1.0 / 60.0, Some(&mut renderer));
            renderer.render(&game.engine.world, time).unwrap();
            mixer.render(&mut block, 2);
            samples.extend_from_slice(&block);
        }
        write_wav(&out.join(format!("{name}.wav")), &samples);
        let (rt60, level) = decay_and_level(&samples);
        println!("{name}: room {:?}, rendered decay {rt60:.2} s, level {level:.4}", game.audio.room());
    };
    let reverb = "ray_traced = true\nray_quality = \"high\"\nreverb = true\n";
    listen(room(10.0), reverb, true, "clap-in-a-concrete-hall");
    listen(vec![slab(10, "Ground", [0.0, -2.0, 0.0], [60.0, 0.2, 60.0])], reverb, true, "clap-on-open-ground");
    let wall = vec![slab(3, "Wall", [0.0, 0.0, 3.0], [10.0, 10.0, 0.2])];
    listen(wall, "ray_traced = true\n", false, "hum-behind-a-concrete-wall");
    listen(Vec::new(), "ray_traced = true\n", false, "hum-in-the-open");
}
