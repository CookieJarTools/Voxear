//! A script floods a hold with real water while the game runs, on a copy of
//! Hello: it gives the Lamp a Dry Area, raises its Bilge a little each
//! frame, finding the water in it at once where it set it, switches the
//! Bilge off and on and takes the Dry Area off again. A file of its own, so
//! it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Flooding : SystemBase
{
	private static int frame;
	private static int wrong;

	private static float Shown(float value) => MathF.Round(value, 2) + 0f;

	protected override void Update()
	{
		frame++;
		var lamp = Obj.Find("Lamp");
		if (frame == 2)
		{
			Begin(lamp);
		}
		if (frame > 2 && frame <= 62)
		{
			Flood(lamp, frame - 2);
		}
		if (frame == 63)
		{
			Switch(lamp);
		}
	}

	private static void Begin(Obj lamp)
	{
		Log.Info($"before: {lamp.DryArea is null}");
		var hold = lamp.AddDryArea();
		hold.Size = new Vector3(4f, 2f, 6f);
		Log.Info($"added: {hold.IsValid} {hold.Shape} {hold.Size.Z} tall {Shown(hold.InsideHeight)} bilge {Shown(hold.Bilge)} on {hold.BilgeEnabled}");
	}

	// A little more each frame, half full after a second; the water there is where it was set, at once.
	private static void Flood(Obj lamp, int step)
	{
		var hold = lamp.DryArea!;
		hold.BilgeFill = step / 120f;
		var bottom = lamp.WorldPosition.Y - 1f;
		var low = lamp.WorldPosition with { Y = bottom + 0.005f };
		if (!Water.TrySample(low, out var water) || MathF.Abs(water.Height - (bottom + hold.Bilge)) > 1e-4f)
		{
			wrong++;
		}
	}

	private static void Switch(Obj lamp)
	{
		var hold = lamp.DryArea!;
		var under = lamp.WorldPosition with { Y = lamp.WorldPosition.Y - 0.5f };
		Log.Info($"flooded: fill {Shown(hold.BilgeFill)} bilge {Shown(hold.Bilge)} wrong {wrong} under {Water.IsUnderwater(under)}");
		hold.BilgeEnabled = false;
		Log.Info($"off: under {Water.IsUnderwater(under)} bilge {Shown(hold.Bilge)} on {hold.BilgeEnabled}");
		hold.BilgeEnabled = true;
		hold.Bilge = 1.5f;
		Log.Info($"on: under {Water.IsUnderwater(under)} fill {Shown(hold.BilgeFill)}");
		// Drained in one line, as a pump does.
		lamp.DryArea?.Bilge = 0f;
		Log.Info($"pumped: under {Water.IsUnderwater(under)} bilge {Shown(hold.Bilge)}");
		hold.Remove();
		Log.Info($"removed: {lamp.DryArea is null} {hold.IsValid}");
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
fn a_script_floods_a_hold_frame_by_frame_and_switches_its_bilge_off_and_on() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Flooding.cs"), SCRIPT).unwrap();
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
        if said(&log).iter().any(|line| line.starts_with("removed: ")) {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().filter(|line| line.starts_with(start)).cloned().collect::<Vec<_>>();
    assert_eq!(report("before: "), ["before: True"], "{lines:#?}");
    assert_eq!(report("added: "), ["added: True Box 6 tall 2 bilge 0 on True"], "{lines:#?}");
    assert_eq!(
        report("flooded: "),
        ["flooded: fill 0.5 bilge 1 wrong 0 under True"],
        "the water followed each frame: {lines:#?}"
    );
    assert_eq!(report("off: "), ["off: under False bilge 1 on False"], "dry, its depth kept: {lines:#?}");
    assert_eq!(report("on: "), ["on: under True fill 0.75"], "{lines:#?}");
    assert_eq!(report("pumped: "), ["pumped: under False bilge 0"], "{lines:#?}");
    assert_eq!(report("removed: "), ["removed: True False"], "{lines:#?}");
}
