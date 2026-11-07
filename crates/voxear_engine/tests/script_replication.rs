//! Replication from a script: a server's system makes a networked prefab for
//! each player who joins, theirs, and moves it; a client's game, in the same
//! process but without scripts, joins over UDP, makes the same prefab, and
//! shows it following the server's, smoothly and a little behind. Builds a
//! copy of Hello's scripts with one more; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, net, scripts};

const ARENA: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

[UpdateIn(Stage.FixedUpdate)]
public sealed class Arena : SystemBase
{
	private Obj ball;
	private float travelled;

	protected override void OnCreate()
	{
		Net.StartServer(47792);
		Net.PlayerJoined += player =>
		{
			ball = Prefab.Load("Prefabs/Ball.prefab").Instantiate(new Vector3(0f, 1f, 0f), Quaternion.Identity);
			var net = ball.NetObject;
			net.Owner = player;
			Log.Info($"ball for {player.Id}, owned by {ball.NetObject.Owner?.Id}");
		};
	}

	protected override void Update()
	{
		if (!ball.IsValid)
		{
			return;
		}
		travelled += Time.Delta;
		ball.Position = new Vector3(travelled, 1f, 0f);
	}
}
"#;

const BALL: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Ball\"\nnet = {}\n";

#[test]
fn a_prefab_made_on_the_server_is_made_on_the_client_and_follows() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Arena.cs"), ARENA).unwrap();
    std::fs::write(dir.path().join("Prefabs/Ball.prefab"), BALL).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut server = Game::new(project, engine).unwrap();
    server.input = voxear_input::Input::without_gamepads();
    server.start(None).unwrap();

    // The client's own copy of the game, the same but for its scripts and scenes: the ball, with its GUID.
    let other = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let name = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    let name = name.lines().find(|l| l.starts_with("name")).unwrap().to_string();
    let toml = format!("[project]\n{name}\nstart_level = \"Levels/Empty.level\"\n");
    std::fs::write(other.path().join("voxear.toml"), toml).unwrap();
    for folder in ["Prefabs", "Levels", "Scenes"] {
        std::fs::create_dir_all(other.path().join(folder)).unwrap();
    }
    std::fs::write(other.path().join("Scenes/Empty.scene"), "format = 1\n").unwrap();
    std::fs::write(other.path().join("Levels/Empty.level"), "format = 1\n[[scene]]\nscene = \"Scenes/Empty.scene\"\n").unwrap();
    for file in ["Ball.prefab", "Ball.prefab.meta"] {
        std::fs::copy(dir.path().join("Prefabs").join(file), other.path().join("Prefabs").join(file)).unwrap();
    }
    let mut client = Game::new(Project::open(other.path()).unwrap(), Engine::new()).unwrap();
    client.input = voxear_input::Input::without_gamepads();
    client.start(None).unwrap();
    client.start_session(net::NetStart::Connect("127.0.0.1:47792".parse().unwrap()), voxear_net::Conditions::PERFECT).unwrap();

    let ball_on = |game: &Game| {
        let world = &game.engine.world;
        world
            .query_ref::<(&net::Networked, &voxear_scene::Name, &voxear_scene::Transform)>()
            .into_iter()
            .find(|(_, name, _)| name.0 == "Ball")
            .map(|(networked, _, transform)| (networked.id, networked.owner, transform.position.x))
    };
    let mut xs = Vec::new();
    for frame in 0..240 {
        server.tick(1.0 / 60.0, None);
        client.tick(1.0 / 60.0, None);
        std::thread::sleep(std::time::Duration::from_millis(2));
        if frame > 120
            && let Some((_, _, x)) = ball_on(&client)
        {
            xs.push(x);
        }
    }
    let lines = || log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect::<Vec<_>>();
    assert!(lines().iter().any(|l| l == "ball for 1, owned by 1"), "{:?}", lines());
    let (server_id, _, server_x) = ball_on(&server).expect("the server's ball, shared");
    let (client_id, owner, client_x) = ball_on(&client).expect("made on the client");
    assert_eq!((client_id, owner), (server_id, Some(net::PlayerId(1))), "the same Obj, the player's own");
    assert!(server_x - client_x > 0.0 && server_x - client_x < 0.3, "a little behind: {client_x} against {server_x}");
    assert!(xs.windows(2).all(|w| w[1] >= w[0] - 1e-4), "never back: {xs:?}");
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
