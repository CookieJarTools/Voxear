//! Networked play from a script: a system serves the game and hears a
//! player join, and reads their Jump a tick at a time as they pressed it on
//! their own machine, here a client in another world joining over UDP on
//! this machine. Builds a copy of Hello's scripts with one more; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_ecs::World;
use voxear_engine::{Engine, Game, Project, net, scripts};
use voxear_input::ActionState;

const LOBBY: &str = r#"using Voxear;

namespace Hello;

[UpdateIn(Stage.FixedUpdate)]
public sealed class Lobby : SystemBase
{
	protected override void OnCreate()
	{
		Net.PlayerJoined += player => Log.Info($"joined {player.Id}");
		Log.Info($"serving {Net.StartServer(47791)} as {Net.Role}");
	}

	protected override void Update()
	{
		foreach (var player in Net.Players)
		{
			if (player.Pressed("Jump"))
			{
				Log.Info($"player {player.Id} jumped");
			}
		}
	}
}
"#;

#[test]
fn a_script_serves_and_reads_a_joined_players_input() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Lobby.cs"), LOBBY).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let settings = net::NetSettings::for_project(project.name(), voxear_engine::engine_version(), &project.file.network);
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.input = voxear_input::Input::without_gamepads();
    game.start(None).unwrap();
    let said = |text: &str| log.lock().unwrap().lines.iter().filter(|line| line.message == text).count();
    let jump = game.input.action("Jump").expect("Hello has a Jump");
    let actions = game.input.actions().len();

    let mut client = World::new();
    net::connect(&mut client, settings, "127.0.0.1:47791".parse().unwrap()).unwrap();
    let frame = |game: &mut Game, client: &mut World, pressing: bool| {
        game.tick(1.0 / 60.0, None);
        let mut states = vec![ActionState::default(); actions];
        states[jump.0] = ActionState { held: pressing, pressed: pressing, ..ActionState::default() };
        net::frame_begin(client, 1.0 / 60.0, &states);
        net::fixed_tick(client);
        net::frame_end(client);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    for _ in 0..120 {
        frame(&mut game, &mut client, false);
    }
    let lines = || log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect::<Vec<_>>();
    assert_eq!(said("serving True as Server"), 1, "{:?}", lines());
    assert_eq!(said("joined 1"), 1, "{:?}", lines());
    assert!(client.resource::<net::NetSession>().unwrap().is_connected());

    frame(&mut game, &mut client, true);
    for _ in 0..30 {
        frame(&mut game, &mut client, false);
    }
    assert_eq!(said("player 1 jumped"), 1, "one press, read once: {:?}", lines());
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
