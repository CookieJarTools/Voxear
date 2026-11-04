//! `Lighting.Apply` from C#: a system lights the level with lighting
//! settings it loaded, then with built-in lighting, on a copy of Hello. A
//! file of its own, so it runs in a process of its own, since .NET starts
//! only once per process. Skips itself without .NET.

use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Look, LookName, Sky, SunLook};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Storm : SystemBase
{
	private int frames;

	protected override void Update()
	{
		frames++;
		if (frames == 2)
		{
			Lighting.Apply(LightingSettings.Load("Lighting/Storm.lighting"));
		}
		if (frames == 6)
		{
			Lighting.Apply(LightingSettings.GoldenHour);
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
fn a_script_lights_the_level_with_lighting_settings() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Storm.cs"), SCRIPT).unwrap();
    std::fs::create_dir_all(dir.path().join("Lighting")).unwrap();
    std::fs::write(dir.path().join("Lighting/Storm.lighting"), "format = 1\n\n[sky]\nhaze = 4.0\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..4 {
        game.tick(1.0 / 60.0, None);
    }
    let lines = |log: &voxear_core::logging::LogCapture| -> Vec<String> {
        log.lock().unwrap().lines.iter().map(|l| format!("{} {}", l.target, l.message)).collect()
    };
    let haze = game.engine.world.resource::<Sky>().map(|sky| sky.haze);
    assert_eq!(haze, Some(4.0), "the storm's hazy sky: {:#?}", lines(&log));
    for _ in 0..4 {
        game.tick(1.0 / 60.0, None);
    }
    let golden = Look::of(LookName::GoldenHour);
    assert_eq!(game.engine.world.resource::<SunLook>().copied(), Some(golden.sun), "then the built-in golden hour");
}
