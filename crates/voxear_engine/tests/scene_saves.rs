//! Saves of the scene from C#, on a copy of Hello with scripts and a prefab
//! of its own: a door opened and a ball moved through values marked
//! [Saved], a lamp destroyed, and crates made from a prefab, one stacked on
//! another and one on the ball, with a keeper that lists them, and a C#
//! component marked [Saved] on the ball. The scene is
//! saved, changed again, and loaded back, and everything is as it was
//! saved. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scripting::{SaveFolder, SavePart, SaveSettings};

const SCRIPT: &str = r#"using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Door : Behaviour
{
	[Saved] public bool IsOpen;
	public float Speed = 2;

	protected override void Start() => Log.Info($"door starts {(IsOpen ? "open" : "shut")}, loading {Save.IsLoadingScene}");
}

[Saved]
public sealed class Crate : Behaviour
{
	public int Hits;
	public string Label = "";
}

public sealed class Lid : Behaviour
{
	[Saved] public bool Open;
}

[Saved]
public struct Charge : IComponent
{
	public float Amount;
}

public sealed class Keeper : Behaviour
{
	[Saved] private List<Obj> crates = [];
	[Saved] public Crate? Favourite;

	public List<Obj> Crates => crates;
}

public sealed class SceneCheck : SystemBase
{
	private static int frame;
	private static int reportAt = -1;

	protected override void Update()
	{
		frame++;
		// Added a frame ahead, as adding waits for the systems to finish.
		if (frame == 1)
		{
			Obj.Find("Chrome Ball").Entity.Add(new Charge { Amount = 1 });
		}
		if (frame == 2)
		{
			Change();
		}
		if (frame == reportAt)
		{
			Report();
		}
	}

	private static void Change()
	{
		var ball = Obj.Find("Chrome Ball");
		ball.GetBehaviour<Door>()!.IsOpen = true;
		ball.Position = new Vector3(5, 1, 0);
		ball.Entity.Set(new Charge { Amount = 3 });
		Obj.Find("Lamp").Destroy();

		var crate = Prefab.Load("Prefabs/Crate.prefab");
		var keeper = Obj.Find("Glow").GetBehaviour<Keeper>()!;
		for (var i = 0; i < 2; i++)
		{
			var made = crate.Instantiate(new Vector3(i * 4, 0, 3), Quaternion.Identity);
			made.GetBehaviour<Crate>()!.Hits = 10 + i;
			made.GetBehaviour<Crate>()!.Label = $"crate {i}";
			keeper.Crates.Add(made);
		}
		keeper.Favourite = keeper.Crates[1].GetBehaviour<Crate>();
		Obj.FindAll("Lid").First(lid => lid.WorldPosition.X < 1).GetBehaviour<Lid>()!.Open = true;
		var stacked = crate.Instantiate(keeper.Crates[0]);
		stacked.Position = new Vector3(0, 2, 0);
		stacked.GetBehaviour<Crate>()!.Label = "stacked";
		crate.Instantiate(ball).GetBehaviour<Crate>()!.Label = "on the ball";

		Save.WriteScene("world", "Day, changed");
		Log.Info("saved the scene");
		_ = LoadBack();
	}

	private static async System.Threading.Tasks.Task LoadBack()
	{
		// Changes after the save, which loading it undoes.
		Obj.Find("Chrome Ball").GetBehaviour<Door>()!.IsOpen = false;
		Obj.Find("Chrome Ball").Entity.Set(new Charge { Amount = 9 });
		Prefab.Load("Prefabs/Crate.prefab").Instantiate().GetBehaviour<Crate>()!.Label = "extra";
		await Save.LoadScene("world");
		Log.Info("loaded the scene");
		// The await resumes before this frame's systems run, so the frame after next is the one after the load.
		reportAt = frame + 2;
	}

	private static void Report()
	{
		var ball = Obj.Find("Chrome Ball");
		var keeper = Obj.Find("Glow").GetBehaviour<Keeper>()!;
		var crates = Obj.FindAll("Crate").Select(crate => crate.GetBehaviour<Crate>()!.Label).Order();
		Log.Info($"put back: door {ball.GetBehaviour<Door>()!.IsOpen}, ball at {ball.Position}, lamp {Obj.Find("Lamp").IsValid}, loading {Save.IsLoadingScene}");
		Log.Info($"crates {string.Join(", ", crates)}");
		Log.Info($"kept {string.Join(", ", keeper.Crates.Select(crate => crate.GetBehaviour<Crate>()!.Hits))}, favourite {keeper.Favourite?.Label}");
		var stacked = Obj.FindAll("Crate").First(crate => crate.GetBehaviour<Crate>()!.Label == "stacked");
		var onBall = Obj.FindAll("Crate").First(crate => crate.GetBehaviour<Crate>()!.Label == "on the ball");
		Log.Info($"stacked at {stacked.WorldPosition}, on the ball at {onBall.WorldPosition}");
		var open = Obj.FindAll("Lid").Where(lid => lid.GetBehaviour<Lid>()!.Open).Select(lid => lid.WorldPosition);
		Log.Info($"open lids at {string.Join(", ", open)}");
		Log.Info($"charge {(ball.Entity.TryGet(out Charge charge) ? charge.Amount : -1)}");
	}
}
"#;

const CRATE: &str = r#"# A crate with a lid, for a save of the scene to make again.
format = 1

[[obj]]
id = 1
name = "Crate"
behaviours = ["Hello.Crate"]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 2
name = "Lid"
parent = 1
position = [0.0, 0.5, 0.0]
behaviours = ["Hello.Lid"]
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against. Its own sounds stay out, as large files the
/// test does not need.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let sound = name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".mp3");
            if name.starts_with('.') || name == "bin" || name == "obj" || sound {
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

/// Gives the Obj named `name` in a scene file these behaviours.
fn add_behaviours(scene: &str, name: &str, behaviours: &str) -> String {
    let line = format!("name = \"{name}\"\n");
    assert!(scene.contains(&line), "{name} is in the scene");
    scene.replacen(&line, &format!("{line}behaviours = [{behaviours}]\n"), 1)
}

#[test]
fn a_save_of_the_scene_puts_it_back_as_it_was() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/SceneCheck.cs"), SCRIPT).unwrap();
    std::fs::write(dir.path().join("Prefabs/Crate.prefab"), CRATE).unwrap();
    std::fs::write(dir.path().join("Prefabs/Crate.prefab.meta"), "guid = \"c4a7e0c1d2b34f5e8a9b0c1d2e3f4a5b\"\n")
        .unwrap();
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    let scene = add_behaviours(&scene, "Chrome Ball", r#""Hello.Door", "Voxear.SavedTransform""#);
    let scene = add_behaviours(&scene, "Glow", r#""Hello.Keeper""#);
    std::fs::write(&day, scene).unwrap();

    let saves = dir.path().join("Saves");
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    for _ in 0..300 {
        game.tick(1.0 / 60.0, None);
        if said(&log).iter().any(|line| line.starts_with("open lids")) {
            break;
        }
    }
    let lines = said(&log);
    assert!(
        lines.contains(&"saved the scene".to_string()) && lines.contains(&"loaded the scene".to_string()),
        "{lines:#?}"
    );
    let report = |start: &str| lines.iter().find(|line| line.starts_with(start)).cloned().unwrap_or_default();
    assert_eq!(report("put back: "), "put back: door True, ball at <5, 1, 0>, lamp False, loading False", "{lines:#?}");
    assert_eq!(report("crates "), "crates crate 0, crate 1, on the ball, stacked", "the extra one is gone: {lines:#?}");
    assert_eq!(report("kept "), "kept 10, 11, favourite crate 1", "references to the copies made again: {lines:#?}");
    assert_eq!(report("stacked "), "stacked at <0, 2, 3>, on the ball at <5, 1, 0>", "under their parents: {lines:#?}");
    assert_eq!(report("open lids "), "open lids at <0, 0.5, 3>", "an Obj inside a copy: {lines:#?}");
    assert_eq!(report("charge "), "charge 3", "the component as saved, on the ball loaded afresh: {lines:#?}");
    let starts: Vec<&String> = lines.iter().filter(|line| line.starts_with("door starts")).collect();
    assert_eq!(starts.first().map(|s| s.as_str()), Some("door starts shut, loading False"), "{lines:#?}");
    assert_eq!(
        starts.last().map(|s| s.as_str()),
        Some("door starts open, loading True"),
        "Start sees what was put back, while the scene loads: {lines:#?}"
    );

    let folder = SaveFolder::new(&saves);
    let data = String::from_utf8(folder.read("world", SavePart::Data).unwrap()).unwrap();
    let day_guid = std::fs::read_to_string(dir.path().join("Scenes/Day.scene.meta")).unwrap();
    let day_guid = day_guid.split('"').nth(1).unwrap();
    assert!(data.contains(&format!("\"{day_guid}:5\"")), "the lamp is kept as gone: {data}");
    assert_eq!(data.matches("c4a7e0c1d2b34f5e8a9b0c1d2e3f4a5b").count(), 4, "four crates: {data}");
    let lid = data.split("\"spawned:").skip(1).any(|key| key.split('"').next().is_some_and(|key| key.ends_with("/1")));
    assert!(lid, "a lid by its crate's number and its place in the prefab: {data}");
}
