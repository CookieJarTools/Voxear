//! Networked play between two processes on this machine, as `vox run` plays
//! it: a server without a window and a client joining it, on a copy of
//! Hello. The server makes a ball for the player who joins, and sets its
//! score, a `[Replicated]` property; the client's own copy of the ball takes
//! it, as its script logs. Skips itself without .NET.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCORE: &str = r#"using Voxear;

namespace Hello;

public sealed partial class Score : Behaviour
{
	[Replicated]
	public partial int Points { get; set; }

	private int shown = -1;

	protected override void Update()
	{
		if (!Net.IsServer && Points != shown)
		{
			shown = Points;
			Log.Info($"points {Points}");
		}
	}
}
"#;

const ARENA: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Arena : SystemBase
{
	protected override void OnCreate()
	{
		Net.PlayerJoined += player =>
		{
			if (!Net.IsServer)
			{
				return;
			}
			var ball = Prefab.Load("Prefabs/Ball.prefab").Instantiate(new Vector3(0f, 1f, 0f), Quaternion.Identity);
			ball.GetBehaviour<Score>()!.Points = 7;
			Log.Info($"ball made for {player.Id}");
		};
	}

	protected override void Update()
	{
	}
}
"#;

const BALL: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Ball\"\nbehaviours = [\"Hello.Score\"]\nnet = {}\n";

#[test]
fn a_client_joins_a_server_and_takes_a_replicated_property() {
    if voxear_scripting::find_dotnet().is_none() {
        return eprintln!("skipping: needs .NET");
    }
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Score.cs"), SCORE).unwrap();
    std::fs::write(dir.path().join("Scripts/Arena.cs"), ARENA).unwrap();
    std::fs::write(dir.path().join("Prefabs/Ball.prefab"), BALL).unwrap();
    let vox = env!("CARGO_BIN_EXE_vox");
    let project = dir.path().to_string_lossy().into_owned();

    let mut server = Command::new(vox)
        .args(["run", &project, "--server", "--port", "47793", "--frames", "1200"])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    for pipe in [Box::new(server.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>, Box::new(server.stderr.take().unwrap())] {
        let lines = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                lines.lock().unwrap().push(line);
            }
        });
    }
    let said = |text: &str| lines.lock().unwrap().iter().any(|line| line.contains(text));
    let started = Instant::now();
    while !said("ready:") {
        assert!(started.elapsed() < Duration::from_secs(120), "the server never got ready: {:?}", lines.lock().unwrap());
        std::thread::sleep(Duration::from_millis(50));
    }

    let client = Command::new(vox)
        .args(["run", &project, "--headless", "--no-build", "--connect", "127.0.0.1:47793", "--frames", "300"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let _ = server.kill();
    let _ = server.wait();
    let out = String::from_utf8_lossy(&client.stdout).into_owned() + &String::from_utf8_lossy(&client.stderr);
    assert!(said("player 1 joined") && said("ball made for 1"), "the server: {:?}", lines.lock().unwrap());
    assert!(out.contains("points 7"), "the client took the ball's score: {out}");
}

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
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
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}
