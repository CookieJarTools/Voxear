//! The console's lines run against a game's scripts: a `[Command]` method
//! by name with its arguments, and a `[ConsoleVariable]` read and set, on a
//! copy of Hello with a script of their own. A file of its own, so it runs
//! in a process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::{Name, Transform};

const CHEATS: &str = r#"using Voxear;

namespace Hello;

public static class Cheats
{
	[ConsoleVariable("How much faster the spinners turn")]
	public static float SpinBoost = 1f;

	[Command("Raise")]
	public static string Raise(string name, float height)
	{
		var obj = Obj.Find(name);
		obj.Position = obj.Position with { Y = height };
		return $"{name} is at {height}";
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

fn logged(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect()
}

#[test]
fn console_lines_run_commands_and_set_variables() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Cheats.cs"), CHEATS).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    assert!(game.engine.run_console("raise \"Spinner A\" 4.5").unwrap(), "a command by its name, any case");
    let world = &game.engine.world;
    let spinner = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner A").unwrap().0;
    assert_eq!(world.get::<Transform>(spinner).unwrap().position.y, 4.5, "its arguments reached it");
    assert!(logged(&log).contains(&"Spinner A is at 4.5".to_string()), "what it returns is printed");

    assert!(game.engine.run_console("SpinBoost 2.5").unwrap());
    assert!(game.engine.run_console("spinboost").unwrap());
    assert_eq!(logged(&log).iter().filter(|l| *l == "SpinBoost = 2.5").count(), 2, "set, then read");
    assert!(!game.engine.run_console("nothing").unwrap(), "an unknown name is the console's to report");

    let names = game.engine.scripts().unwrap().console_names();
    let raise = names.iter().find(|n| n.name == "Raise").expect("for completing names");
    assert_eq!((raise.usage.as_str(), raise.help.as_str()), ("Raise name height", "Raise"));
}
