//! A script reads the game's input as actions, through `Input`, with
//! Voxear's own actions: Jump pressed and its event, Move from WASD, a key
//! read itself, and Jump rebound to the next key pressed, which then jumps.
//! Builds a copy of Hello's scripts with one more; skips itself without .NET
//! and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Controls : SystemBase
{
	private InputAction jump = null!;

	protected override void OnCreate()
	{
		jump = Input.Action("Jump");
		jump.Started += action => Log.Info($"started {action.Map}/{action.Name}");
		Log.Info($"maps {string.Join(",", Input.Maps)}");
	}

	protected override void Update()
	{
		if (Input.Pressed("Jump"))
		{
			Log.Info($"jumped, prompt {jump.Prompt}");
		}
		var move = Input.Vector2("Move");
		if (move != Vector2.Zero)
		{
			Log.Info($"move {move.X:0.00} {move.Y:0.00}");
		}
		if (Keyboard.WasPressed(Key.F1))
		{
			Rebind();
		}
	}

	private async void Rebind()
	{
		var taken = await Input.Rebind(jump);
		Log.Info($"rebound to {taken}");
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
fn scripts_read_actions_rebind_them_and_hear_their_events() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Controls.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture, text: &str| log.lock().unwrap().lines.iter().any(|line| line.message == text);
    let mut press = |game: &mut Game, key: &str, down: bool| {
        game.input.devices_mut().set_key(key, down);
        game.tick(1.0 / 60.0, None);
    };

    // A frame first, for the system to be made and listen to Jump.
    game.tick(1.0 / 60.0, None);
    press(&mut game, "Space", true);
    press(&mut game, "Space", false);
    assert!(said(&log, "maps Player,UI"), "Voxear's own actions, for a project with none");
    assert!(said(&log, "jumped, prompt Space"));
    assert!(said(&log, "started Player/Jump"), "the event fired");

    press(&mut game, "W", true);
    press(&mut game, "D", true);
    assert!(said(&log, "move 0.71 0.71"), "a diagonal as long as a straight line");
    press(&mut game, "W", false);
    press(&mut game, "D", false);

    // F1 starts listening; K is taken, and jumps from then on.
    press(&mut game, "F1", true);
    press(&mut game, "F1", false);
    press(&mut game, "K", true);
    press(&mut game, "K", false);
    press(&mut game, "K", false);
    assert!(said(&log, "rebound to Keyboard/K"), "{:?}", log.lock().unwrap().lines.iter().map(|l| &l.message).collect::<Vec<_>>());
    press(&mut game, "K", true);
    assert!(said(&log, "jumped, prompt K"), "K jumps now");
}
