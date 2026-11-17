//! A script puts Objs inside others while the game runs, on a copy of
//! Hello: the Chrome Ball goes aboard the spinning Pyramid and stays where
//! it is in the world, the Lamp goes in keeping its own place and comes out
//! again, and going inside itself, inside what is inside it, or into
//! another scene is refused. A save of the scene keeps the ball aboard
//! through taking it off again and loading. A file of its own, so it runs
//! in a process of its own, since .NET starts only once per process. Skips
//! itself without .NET or the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scripting::{SaveFolder, SavePart, SaveSettings};

const SCRIPT: &str = r#"using System;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using Voxear;

namespace Hello;

public sealed class Boarding : SystemBase
{
	private static int frame;
	private static int reportAt = -1;
	private static Vector3 savedAt;

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			Board();
		}
		if (frame == reportAt)
		{
			Report();
		}
	}

	private static Obj Pyramid() => Obj.FindAll("Pyramid").First(pyramid => !pyramid.Parent.IsValid);

	private static string Refusal(Action asked)
	{
		try
		{
			asked();
			return "none";
		}
		catch (InvalidOperationException refused)
		{
			return refused.Message;
		}
	}

	private static void Board()
	{
		var ball = Obj.Find("Chrome Ball");
		var pyramid = Pyramid();
		var before = ball.WorldPosition;
		ball.SetParent(pyramid);
		Log.Info($"aboard: inside {ball.Parent == pyramid}, moved {Vector3.Distance(before, ball.WorldPosition) > 1e-3f}");
		Log.Info($"refused: {Refusal(() => pyramid.Parent = ball)}");
		Log.Info($"refused: {Refusal(() => ball.SetParent(ball))}");
		Log.Info($"refused: {Refusal(() => ball.SetParent(Obj.Find("Ground")))}");

		var lamp = Obj.Find("Lamp");
		var own = lamp.Position;
		lamp.SetParent(pyramid, keepWorldPlace: false);
		Log.Info($"lamp inside {lamp.Parent == pyramid}, its own place kept {lamp.Position == own}");
		var placed = lamp.WorldPosition;
		lamp.Parent = Obj.None;
		Log.Info($"lamp at the top {!lamp.Parent.IsValid}, moved {Vector3.Distance(placed, lamp.WorldPosition) > 1e-3f}");

		savedAt = ball.Position;
		Save.WriteScene("world", "Aboard");
		_ = LoadBack();
	}

	private static async Task LoadBack()
	{
		// Taken off after the save, which loading it undoes.
		Obj.Find("Chrome Ball").Parent = Obj.None;
		await Save.LoadScene("world");
		// The await resumes before this frame's systems run, so the frame after next is the one after the load.
		reportAt = frame + 2;
	}

	private static void Report()
	{
		var ball = Obj.Find("Chrome Ball");
		var at = Vector3.Distance(ball.Position, savedAt) < 1e-3f;
		Log.Info($"loaded: inside {ball.Parent == Pyramid()}, where it was {at}, lamp at the top {!Obj.Find("Lamp").Parent.IsValid}");
	}
}
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

#[test]
fn a_script_puts_objs_inside_others_and_a_save_keeps_them_there() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Boarding.cs"), SCRIPT).unwrap();
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
        if said(&log).iter().any(|line| line.starts_with("loaded: ")) {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().filter(|line| line.starts_with(start)).cloned().collect::<Vec<_>>();
    assert_eq!(report("aboard: "), ["aboard: inside True, moved False"], "kept where it is in the world: {lines:#?}");
    assert_eq!(
        report("refused: "),
        [
            "refused: \"Pyramid\" cannot go inside \"Chrome Ball\", which is inside it",
            "refused: \"Chrome Ball\" cannot go inside itself",
            "refused: \"Chrome Ball\" cannot go into Scenes/Environment.scene: an Obj a scene loaded stays in its \
             scene, and this one is in Scenes/Day.scene",
        ],
        "{lines:#?}"
    );
    assert_eq!(report("lamp inside "), ["lamp inside True, its own place kept True"], "{lines:#?}");
    assert_eq!(report("lamp at the top "), ["lamp at the top True, moved False"], "{lines:#?}");
    assert_eq!(
        report("loaded: "),
        ["loaded: inside True, where it was True, lamp at the top True"],
        "the save put the ball back aboard: {lines:#?}"
    );

    let data = String::from_utf8(SaveFolder::new(&saves).read("world", SavePart::Data).unwrap()).unwrap();
    assert!(data.contains("\"moved\""), "the save keeps where Objs went: {data}");
}
