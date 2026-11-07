//! Networked play between two processes on this machine, as `vox run` plays
//! it: a server without a window and a client joining it, on a copy of
//! Hello. The server makes a ball for the player who joins, theirs, and a
//! spare, and sets their scores, a `[Replicated]` property; the client's own
//! copies take them, as its script logs. The client then asks for more on
//! both with a `[ServerRpc]`: the server gives it only for the player's own,
//! and cheers with a `[ClientRpc]`. Skips itself without .NET.

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
	private bool asked;

	protected override void Update()
	{
		if (Net.IsServer || Points == shown)
		{
			return;
		}
		shown = Points;
		Log.Info($"points {Points}");
		if (Points == 7 && !asked)
		{
			asked = true;
			AskOnServer(3);
		}
	}

	[ServerRpc]
	private void Ask(int more)
	{
		Points += more;
		var whose = Obj.NetObject.Owner is null ? "the spare" : "their own";
		Log.Info($"{Net.Caller?.Id} asked for {more} on {whose}");
		CheeredOnClients($"now {Points}");
	}

	[ClientRpc]
	private void Cheered(string text)
	{
		Log.Info($"cheered {text}");
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
			var prefab = Prefab.Load("Prefabs/Ball.prefab");
			var ball = prefab.Instantiate(new Vector3(0f, 1f, 0f), Quaternion.Identity);
			var net = ball.NetObject;
			net.Owner = player;
			ball.GetBehaviour<Score>()!.Points = 7;
			prefab.Instantiate(new Vector3(2f, 1f, 0f), Quaternion.Identity).GetBehaviour<Score>()!.Points = 7;
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
fn a_client_joins_a_server_takes_replicated_properties_and_makes_rpcs() {
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
    assert!(said("1 asked for 3 on their own"), "the server ran the owner's RPC: {:?}", lines.lock().unwrap());
    assert!(!said("on the spare"), "but not one on an Obj not theirs: {:?}", lines.lock().unwrap());
    assert!(out.contains("cheered now 10") && out.contains("points 10"), "the server's RPC, and the score after: {out}");
}

const RUNNER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

[Predicted]
public sealed partial class Runner : Behaviour
{
	[Replicated]
	public partial float Speed { get; set; }

	private int ticks;

	protected override void FixedUpdate()
	{
		if (Obj.NetObject.Owner is null)
		{
			return;
		}
		// The server has its own idea after two seconds, which the player's machine cannot guess.
		if (Net.IsServer && ++ticks == 120)
		{
			Speed = 2f;
			Log.Info($"faster from tick {Net.Tick}");
		}
		Obj.Position += new Vector3(Speed == 0f ? 1f : Speed, 0f, 0f) * Time.Delta;
		if (Net.Tick % 30 == 0)
		{
			Log.Info($"tick {Net.Tick} x {Obj.Position.X:F3}");
		}
	}
}
"#;

const START: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Start : SystemBase
{
	protected override void OnCreate()
	{
		Net.PlayerJoined += player =>
		{
			if (!Net.IsServer)
			{
				return;
			}
			var runner = Prefab.Load("Prefabs/Runner.prefab").Instantiate(Vector3.Zero, Quaternion.Identity);
			var net = runner.NetObject;
			net.Owner = player;
		};
	}

	protected override void Update()
	{
	}
}
"#;

const RUNNER_PREFAB: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Runner\"\nbehaviours = [\"Hello.Runner\"]\nnet = {}\n";

#[test]
fn a_players_own_obj_is_predicted_and_put_right_with_its_replicated_properties() {
    if voxear_scripting::find_dotnet().is_none() {
        return eprintln!("skipping: needs .NET");
    }
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Runner.cs"), RUNNER).unwrap();
    std::fs::write(dir.path().join("Scripts/Start.cs"), START).unwrap();
    std::fs::write(dir.path().join("Prefabs/Runner.prefab"), RUNNER_PREFAB).unwrap();
    let (server, client) = play(dir.path(), 47794, 480);
    // Where the runner was on each tick both logged, the client's last word on it, after a replay.
    let at = |lines: &[String]| -> std::collections::BTreeMap<u64, f32> {
        lines
            .iter()
            .filter_map(|line| {
                let rest = &line[line.find("tick ")? + 5..];
                let (tick, x) = rest.split_once(" x ")?;
                Some((tick.trim().parse().ok()?, x.trim().parse().ok()?))
            })
            .collect()
    };
    let faster = server.iter().find_map(|l| l.split("faster from tick ").nth(1)?.trim().parse::<u64>().ok());
    let faster = faster.unwrap_or_else(|| panic!("the server changed speed: {server:?}"));
    let (on_server, on_client) = (at(&server), at(&client));
    let late: Vec<(u64, f32, f32)> =
        on_client.iter().filter(|(t, _)| **t > faster + 60).filter_map(|(t, x)| Some((*t, *x, *on_server.get(t)?))).collect();
    assert!(late.len() >= 3, "ticks both logged after the change: {late:?}\n{client:?}");
    for (tick, client_x, server_x) in late {
        assert!((client_x - server_x).abs() < 0.01, "tick {tick}: the client had {client_x}, the server {server_x}");
    }
}

/// Plays a server and a client of the project at `dir` on `port`, the client for `frames` frames: each one's log.
fn play(dir: &Path, port: u16, frames: u32) -> (Vec<String>, Vec<String>) {
    let vox = env!("CARGO_BIN_EXE_vox");
    let project = dir.to_string_lossy().into_owned();
    let port = port.to_string();
    let mut server = Command::new(vox)
        .args(["run", &project, "--server", "--port", &port, "--frames", "3000"])
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
    let started = Instant::now();
    while !lines.lock().unwrap().iter().any(|line| line.contains("ready:")) {
        assert!(started.elapsed() < Duration::from_secs(120), "the server never got ready: {:?}", lines.lock().unwrap());
        std::thread::sleep(Duration::from_millis(50));
    }
    let address = format!("127.0.0.1:{port}");
    let frames = frames.to_string();
    let client = Command::new(vox)
        .args(["run", &project, "--headless", "--no-build", "--connect", &address, "--frames", &frames])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let _ = server.kill();
    let _ = server.wait();
    let out = String::from_utf8_lossy(&client.stdout).into_owned() + &String::from_utf8_lossy(&client.stderr);
    let server = lines.lock().unwrap().clone();
    (server, out.lines().map(str::to_string).collect())
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
