//! A script changes the sky, the sun's look, a Post Volume, a Wind Zone, a
//! Pool, an Ocean and a Stream while the game runs, on a copy of Hello, and
//! the engine draws with what it set. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips itself
//! without .NET or the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{CloudKind, Liquid, Ocean, Pool, PostVolume, SkyMode, VolumeShape, WindZone, WornLook};

const SCRIPT: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Changing : SystemBase
{
	private static int frame;

	protected override void Update()
	{
		frame++;
		var lamp = Obj.Find("Lamp");
		if (frame == 2)
		{
			Lighting.ChangeSky(sky =>
			{
				sky.Mode = SkyMode.Atmosphere;
				sky.Clouds.Cover = 0.75f;
				sky.Clouds.Kind = CloudKind.Blocks;
			});
			Lighting.ChangeSun(sun => sun.Strength = 0.25f);
			var sky = Lighting.Sky;
			Log.Info($"sky: {sky.Mode} {sky.Clouds.Kind} {sky.Clouds.Cover} sun {Lighting.SunLook.Strength}");

			var volume = lamp.AddPostVolume();
			volume.Shape = PostVolumeShape.Global;
			volume.ChangePost(post => post.Bloom.Intensity = 0.5f);
			Log.Info($"volume: {lamp.PostVolume?.Shape} {volume.Post.Bloom.Intensity} {volume.PostToJson()}");

			var wind = lamp.AddWindZone();
			wind.Strength = 2.5f;
			wind.Shape = WindShape.Box;
			Log.Info($"wind: {wind.Strength} {wind.Shape} {wind.Size.X}");

			var pool = lamp.AddPool();
			pool.Liquid = Liquid.Lava;
			pool.Size = new Vector2(3f, 5f);
			pool.Deep = new Color(1f, 0.2f, 0f);
			Log.Info($"pool: {pool.Liquid} {pool.Size.Y} {pool.Deep.G}");

			var sea = lamp.AddOcean();
			sea.Weather = SeaWeather.Stormy;
			sea.Choppiness = 5f;
			Log.Info($"sea: {lamp.Ocean?.Weather} {sea.WindSource}");
			Log.Info($"no stream: {lamp.Stream is null} seasons {Seasons.Exists} climate {Climate.Exists}");
		}
		if (frame == 4)
		{
			Log.Info("done");
		}
	}
}
"#;

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
fn a_script_changes_the_sky_the_sun_and_the_levels_water_and_wind() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Changing.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    for _ in 0..300 {
        game.tick(1.0 / 60.0, None);
        if said(&log).iter().any(|line| line == "done") {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().filter(|line| line.starts_with(start)).cloned().collect::<Vec<_>>();
    assert_eq!(report("sky: "), ["sky: Atmosphere Blocks 0.75 sun 0.25"], "{lines:#?}");
    assert_eq!(report("volume: "), [r#"volume: Global 0.5 {"bloom":{"intensity":0.5}}"#], "{lines:#?}");
    assert_eq!(report("wind: "), ["wind: 2.5 Box 60"], "{lines:#?}");
    assert_eq!(report("pool: "), ["pool: Lava 5 0.2"], "{lines:#?}");
    assert_eq!(report("sea: "), ["sea: Stormy Own"], "{lines:#?}");
    assert_eq!(report("no stream: "), ["no stream: True seasons False climate False"], "{lines:#?}");

    let world = &game.engine.world;
    let worn = world.resource::<WornLook>().expect("drawn with what the script set");
    assert_eq!((worn.sky.mode, worn.sky.clouds.kind), (SkyMode::Atmosphere, CloudKind::Blocks));
    assert_eq!(worn.sun.map(|sun| sun.strength), Some(0.25));
    let volume = world.query_ref::<&PostVolume>().into_iter().next().cloned().expect("a Post Volume");
    assert_eq!(volume.shape, VolumeShape::Global);
    let wind = world.query_ref::<&WindZone>().into_iter().next().copied().expect("a Wind Zone");
    assert_eq!(wind.strength, 2.5);
    let pool = world.query_ref::<&Pool>().into_iter().next().copied().expect("a Pool");
    assert_eq!((pool.liquid, pool.size), (Liquid::Lava, [3.0, 5.0]));
    let sea = world.query_ref::<&Ocean>().into_iter().next().copied().expect("an Ocean");
    assert_eq!(sea.weather, voxear_render::ocean::Weather::Stormy);
}
