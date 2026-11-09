//! A script that dereferences null throws a NullReferenceException, which
//! the Console shows, and the game goes on, as it does for one the script
//! throws itself. .NET catches a null dereference in its fault handler,
//! which needs a larger signal stack than Rust gives its threads. Builds a
//! copy of Hello's scripts with one more; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Careless : SystemBase
{
	private static string? missing;
	private int frame;

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			Log.Info($"length {missing!.Length}");
		}
		if (frame == 3)
		{
			throw new System.InvalidOperationException("thrown on purpose");
		}
		Log.Info($"frame {frame} done");
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
fn a_scripts_null_dereference_is_an_exception_and_the_game_goes_on() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Careless.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..5 {
        game.tick(1.0 / 60.0, None);
    }

    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    let at = |text: &str| lines.iter().position(|line| line.starts_with(text));
    let null = at("NullReferenceException").unwrap_or_else(|| panic!("{lines:#?}"));
    let thrown = at("InvalidOperationException: thrown on purpose").unwrap_or_else(|| panic!("{lines:#?}"));
    assert!(at("frame 1 done") < Some(null), "{lines:#?}");
    assert!(at("frame 2 done").is_none() && at("frame 3 done").is_none(), "each stopped where it threw");
    assert!(at("frame 5 done") > Some(thrown), "the game goes on: {lines:#?}");
}
