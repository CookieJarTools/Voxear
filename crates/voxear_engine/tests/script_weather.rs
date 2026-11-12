//! `Weather` from C#: a system turns the weather to a storm over a second,
//! reads it back as it turns, then sets amounts of its own at once, on a
//! copy of Hello. A file of its own, so it runs in a process of its own,
//! since .NET starts only once per process. Skips itself without .NET.

use voxear_engine::{
    Engine, Game, Project, scripts,
    weather::{Weather, WeatherKind},
};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Forecast : SystemBase
{
	private int frames;

	protected override void Update()
	{
		frames++;
		if (frames == 2)
		{
			Weather.Set(WeatherKind.Storm, 1f);
		}
		if (frames == 20)
		{
			Log.Info($"forecast {Weather.Kind} turning {Weather.IsTurning} rain {Weather.Amounts.Rain > 0f}");
		}
		if (frames == 80)
		{
			Weather.Set(new WeatherAmounts { Fog = 0.5f, Snow = 2f }, 0f);
		}
		if (frames == 82)
		{
			Log.Info($"forecast {Weather.Kind} turning {Weather.IsTurning} snow {Weather.Amounts.Snow}");
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
fn a_script_turns_the_weather_and_reads_it_as_it_turns() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Forecast.cs"), SCRIPT).unwrap();
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
    for _ in 0..30 {
        game.tick(1.0 / 60.0, None);
    }
    // Half a second into a storm that takes one to come.
    let weather = game.engine.world.resource::<Weather>().copied().expect("the script set the weather");
    assert_eq!(weather.kind(), WeatherKind::Storm);
    assert!(weather.now().rain > 0.0 && weather.now().rain < 1.0 && !weather.settled(), "{weather:?}");
    assert!(said(&log, "forecast Storm turning True rain True"), "read back as it turns: {:#?}", lines(&log));
    for _ in 0..40 {
        game.tick(1.0 / 60.0, None);
    }
    let storm = game.engine.world.resource::<Weather>().copied().unwrap();
    assert!(storm.settled() && storm.now().rain == 1.0, "the storm has come: {storm:?}");
    for _ in 0..20 {
        game.tick(1.0 / 60.0, None);
    }
    // Amounts of its own, at once, each kept within 0 and 1.
    let own = game.engine.world.resource::<Weather>().copied().unwrap();
    assert_eq!((own.kind(), own.now().snow, own.now().fog, own.now().rain), (WeatherKind::Custom, 1.0, 0.5, 0.0));
    assert!(said(&log, "forecast Custom turning False snow 1"), "{:#?}", lines(&log));
}
