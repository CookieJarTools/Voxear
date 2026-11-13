//! `Storms` and `Sea` from C#: a system starts a storm over the level,
//! hears it arrive, asks the weather under it, changes it, makes a
//! whirlpool, keeps the storms as bytes, stops the storm and puts it back,
//! on a copy of Hello. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET.

use voxear_engine::{
    Engine, Game, Project, scripts,
    storms::{StormKind, Storms},
};
use voxear_render::sea::{RoughSea, SeaLook};

const SCRIPT: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class StormWatch : SystemBase
{
	private int frames;
	private Storm storm;
	private Whirlpool pool;
	private byte[] kept = [];

	protected override void Update()
	{
		frames++;
		if (frames == 1)
		{
			Storms.Arrived += come => Log.Info($"watch arrived {come == storm} {come.Kind}");
			Storms.Left += gone => Log.Info($"watch left {gone == storm}");
		}
		if (frames == 2)
		{
			storm = Storms.Start(Vector3.Zero, 5000f, 1f, StormKind.Thunderstorm, 0f);
			Log.Info($"watch started {storm.Exists} natural {storm.IsNatural} radius {storm.Radius} endless {float.IsPositiveInfinity(storm.Lifetime)}");
		}
		if (frames == 10)
		{
			var here = Storms.At(new Vector3(0f, 1f, 0f));
			Log.Info($"watch under {here.Kind} strength {here.Strength} same {here.Storm == storm} in {here.InStorm} rain {here.Amounts.Rain}");
			Log.Info($"watch all {Storms.All.Count} now {storm.StrengthNow} at {storm.StrengthAt(Vector3.Zero)}");
		}
		if (frames == 12)
		{
			storm.Kind = StormKind.Blizzard;
			storm.Radius = 4000f;
			storm.Velocity = new Vector3(5f, 0f, 0f);
			storm.Eye = 0f;
			Sea.Look = SeaLook.BlockSteps;
			Sea.SwellHeight = 4f;
			pool = Sea.AddWhirlpool(new Vector3(10f, 0f, 10f), 20f, 6f);
		}
		if (frames == 16)
		{
			var here = Storms.At(new Vector3(0f, 1f, 0f));
			Log.Info($"watch now {here.Kind} snow {here.Amounts.Snow} look {Sea.Look} swell {Sea.SwellHeight} radius {storm.Radius}");
			Log.Info($"watch pool {pool.IsValid} in {Sea.WhirlpoolAt(new Vector3(10f, 0f, 10f))} out {Sea.WhirlpoolAt(new Vector3(200f, 0f, 10f))}");
			Log.Info($"watch moved {storm.Position.X > 0f} clock {Storms.Clock > 0.1}");
			kept = Storms.CaptureState();
			storm.Stop(0f);
		}
		if (frames == 20)
		{
			Log.Info($"watch stopped {storm.Exists} kept {kept.Length} all {Storms.All.Count}");
			Log.Info($"watch back {Storms.ApplyState(kept)} bad {Storms.ApplyState([1, 2, 3])}");
		}
		if (frames == 24)
		{
			Log.Info($"watch again {storm.Exists} {storm.Kind} all {Storms.All.Count} removed {pool.Remove()}");
		}
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
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
fn a_script_starts_a_storm_hears_it_arrive_changes_it_and_keeps_and_puts_back_the_storms() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/StormWatch.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let lines = |log: &voxear_core::logging::LogCapture| -> Vec<String> {
        log.lock().unwrap().lines.iter().map(|l| format!("{} {}", l.target, l.message)).collect()
    };
    let said = |log: &voxear_core::logging::LogCapture, words: &str| lines(log).iter().any(|line| line.contains(words));
    for _ in 0..14 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(said(&log, "watch started True natural False radius 5000 endless True"), "{:#?}", lines(&log));
    assert!(said(&log, "watch arrived True Thunderstorm"), "heard as it reached the viewer: {:#?}", lines(&log));
    assert!(said(&log, "watch under Thunderstorm strength 1 same True in True rain 1"), "{:#?}", lines(&log));
    assert!(said(&log, "watch all 1 now 1 at 1"), "{:#?}", lines(&log));
    // Changed from the script: a blizzard now, the sea drawn as block steps, with a whirlpool in it.
    game.tick(1.0 / 60.0, None);
    let world = &game.engine.world;
    let storms = world.resource::<Storms>().unwrap();
    assert_eq!((storms.made[0].kind, storms.made[0].radius, storms.whirlpools.len()), (StormKind::Blizzard, 4000.0, 1));
    let sea = world.resource::<RoughSea>().unwrap();
    assert_eq!((sea.look, sea.height, sea.whirlpools.len()), (SeaLook::Steps, 4.0, 1));
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(said(&log, "watch now Blizzard snow 1 look BlockSteps swell 4 radius 4000"), "{:#?}", lines(&log));
    assert!(said(&log, "watch pool True in 1 out 0"), "{:#?}", lines(&log));
    assert!(said(&log, "watch moved True clock True"), "{:#?}", lines(&log));
    // Stopped, it is gone and scripts hear it leave; put back from the bytes kept, it is there again.
    for _ in 0..10 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(said(&log, "watch stopped False kept 104 all 0"), "{:#?}", lines(&log));
    assert!(said(&log, "watch left True"), "{:#?}", lines(&log));
    assert!(said(&log, "watch back True bad False"), "{:#?}", lines(&log));
    assert!(said(&log, "watch again True Blizzard all 1 removed True"), "{:#?}", lines(&log));
    assert!(game.engine.world.resource::<Storms>().unwrap().whirlpools.is_empty());
}
