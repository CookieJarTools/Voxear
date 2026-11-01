//! A script reads the window's size and asks for another, and hides and
//! holds the mouse, through `Screen` and `Cursor`, for whatever shows the
//! game to make so. Builds a copy of Hello's scripts with one more; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scripting::CursorLock;

const SCRIPT: &str = r#"using Voxear;

namespace Hello
{
	public sealed class Settings : SystemBase
	{
		protected override void OnCreate()
		{
			Log.Info($"window {Screen.Width}x{Screen.Height}, full screen {Screen.Fullscreen}");
			Screen.SetResolution(800, 600, false);
			Cursor.Lock = CursorLock.Locked;
			Cursor.Visible = false;
			Log.Info($"cursor {Cursor.Lock}, visible {Cursor.Visible}");
		}

		protected override void Update()
		{
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
fn scripts_read_the_window_and_ask_for_a_size_and_a_held_mouse() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Settings.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    (game.display.width, game.display.height) = (1600, 900);
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    let messages: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    assert!(messages.iter().any(|m| m == "window 1600x900, full screen False"), "{messages:?}");
    assert!(messages.iter().any(|m| m == "cursor Locked, visible False"), "{messages:?}");
    assert_eq!(game.display.asked, Some((800, 600, false)), "a size asked for, for the window to take");
    assert_eq!((game.display.cursor_visible, game.display.cursor_lock), (false, CursorLock::Locked));
    assert!(game.display.cursor_changed, "for the window to make so");
}
