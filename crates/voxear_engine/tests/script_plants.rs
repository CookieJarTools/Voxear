//! Scripts and plants: a ray meets a tree's trunk and says which plant it
//! was, plants near it are found, it is cut down into its Foliage's Cut
//! prefab, and a save of the scene keeps it cut down. Runs on Hello with no
//! renderer, as a server would; skips itself without .NET and the managed
//! build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scripting::SaveSettings;

const SCRIPT: &str = r#"using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Woodcutter : SystemBase
{
	private static int frame;
	private static int reportAt = -1;
	private static readonly Vector3 Eye = new(204f, 1f, 200f);

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			Cut();
		}
		if (frame == reportAt)
		{
			Report();
		}
	}

	private static void Cut()
	{
		if (!Physics.Raycast(Eye, Vector3.UnitZ, out var hit, 30f) || hit.Plant is not { } tree)
		{
			Log.Info("no tree met");
			return;
		}
		Log.Info($"met plant {tree.Number} of {hit.Obj.Name} at x {tree.Position.X:0}, {hit.Distance:0.0} away");
		var near = new List<Plant>();
		Plants.Query(tree.Position, 5f, near);
		Log.Info($"near {near.Count}, nearest {near[0].Number}");
		var stump = tree.CutDown();
		Log.Info($"cut into {stump.Name} at x {stump.WorldPosition.X:0}, standing {tree.Exists}");
		Save.WriteScene("grove");
		_ = LoadBack();
	}

	private static async System.Threading.Tasks.Task LoadBack()
	{
		// Taken away after the save, which loading it undoes.
		new Plant(Obj.Find("Grove"), 2).Remove();
		await Save.LoadScene("grove");
		reportAt = frame + 2;
	}

	private static void Report()
	{
		var grove = Obj.Find("Grove");
		var standing = Enumerable.Range(0, 3).Select(number => new Plant(grove, number).Exists);
		Log.Info($"after loading: {string.Join(", ", standing)}");
		var met = Physics.Raycast(Eye, Vector3.UnitZ, out var hit, 30f) ? hit.Obj.Name : "nothing";
		Log.Info($"the ray now meets {met}; stumps {Obj.FindAll("Stump").Count()}");
	}
}
"#;

/// Three trees along x, 4 m apart, out past Hello's things, with trunks, and
/// a stump for each cut down.
const GROVE: &str = r#"
[[obj]]
id = 50
name = "Grove"
position = [200.0, 0.0, 210.0]
[obj.foliage]
mesh = "builtin:cube"
plants = [[0.0, 0.0, 0.0, 0.0, 1.0], [4.0, 0.0, 0.0, 0.0, 1.0], [8.0, 0.0, 0.0, 0.0, 1.0]]
cut = "Prefabs/Stump.prefab"
[obj.foliage.trunk]
radius = 0.3
height = 5.0
"#;

const STUMP: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Stump\"\n[obj.renderer]\nmesh = \"builtin:cube\"\n";

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

#[test]
fn a_script_cuts_a_tree_down_and_a_save_keeps_it_cut() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Woodcutter.cs"), SCRIPT).unwrap();
    std::fs::write(dir.path().join("Prefabs/Stump.prefab"), STUMP).unwrap();
    std::fs::write(dir.path().join("Prefabs/Stump.prefab.meta"), "guid = \"5a1e0000000000000000000000000077\"\n")
        .unwrap();
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap() + GROVE;
    std::fs::write(&day, scene).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(dir.path().join("Saves")), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    for _ in 0..300 {
        game.tick(1.0 / 60.0, None);
        if said(&log).iter().any(|line| line.starts_with("the ray now")) {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().find(|line| line.starts_with(start)).cloned().unwrap_or_default();
    assert_eq!(report("met "), "met plant 1 of Grove at x 204, 9.7 away", "{lines:#?}");
    assert_eq!(report("near "), "near 3, nearest 1", "{lines:#?}");
    assert_eq!(report("cut into "), "cut into Stump at x 204, standing False", "{lines:#?}");
    assert_eq!(report("after loading: "), "after loading: True, False, True", "cut as saved: {lines:#?}");
    assert_eq!(report("the ray now "), "the ray now meets nothing; stumps 1", "{lines:#?}");
    fn texts(folder: &std::path::Path, into: &mut String) {
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            match entry.path().is_dir() {
                true => texts(&entry.path(), into),
                false => into.push_str(&String::from_utf8_lossy(&std::fs::read(entry.path()).unwrap_or_default())),
            }
        }
    }
    let mut saved = String::new();
    texts(&dir.path().join("Saves"), &mut saved);
    assert!(saved.contains("plants") && saved.contains("gone"), "the save names the plants cut down: {saved}");
}
