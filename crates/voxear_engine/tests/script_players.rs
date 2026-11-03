//! Several players on one machine, from a script: joining turned on, a pad
//! and then the keyboard joining as players, `PlayerJoined` hearing each,
//! and a player's Jump read from their own device. Builds a copy of Hello's
//! scripts with one more; skips itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const PARTY: &str = r#"using Voxear;

namespace Hello;

public sealed class Party : SystemBase
{
	protected override void OnCreate()
	{
		Input.Joining = true;
		Input.PlayerJoined += player => Log.Info($"joined {player.Index} {player.Device} {player.DeviceName}");
		Log.Info($"joining {Input.Joining}, players {Input.Players.Count}");
	}

	protected override void Update()
	{
		foreach (var player in Input.Players)
		{
			if (player.Pressed("Jump"))
			{
				Log.Info($"player {player.Index} jumped");
			}
		}
	}
}
"#;

#[test]
fn players_join_from_a_script_and_play_on_their_own_devices() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Party.cs"), PARTY).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    // Without the system's own pads, which would take the place of the test's.
    game.input = voxear_input::Input::without_gamepads();
    game.start(None).unwrap();
    let said = |log: &LogCapture, text: &str| log.lock().unwrap().lines.iter().any(|line| line.message == text);
    let pad = |south: f32| {
        let mut pad = voxear_input::Pad { id: 5, name: "Test Pad".into(), ..voxear_input::Pad::default() };
        pad.buttons.insert("South", south);
        pad
    };

    game.input.devices_mut().set_pad(0, pad(0.0));
    game.tick(1.0 / 60.0, None);
    assert!(said(&log, "joining True, players 0"));
    game.input.devices_mut().set_pad(0, pad(1.0));
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    assert!(
        said(&log, "joined 0 Gamepad Test Pad"),
        "{:?}",
        log.lock().unwrap().lines.iter().map(|l| &l.message).collect::<Vec<_>>()
    );

    game.input.devices_mut().set_pad(0, pad(0.0));
    game.tick(1.0 / 60.0, None);
    game.input.devices_mut().set_pad(0, pad(1.0));
    game.tick(1.0 / 60.0, None);
    assert!(said(&log, "player 0 jumped"));
    game.input.devices_mut().set_key("Space", true);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    assert!(said(&log, "joined 1 KeyboardMouse Keyboard and Mouse"), "the keyboard joins as the second player");
}

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
