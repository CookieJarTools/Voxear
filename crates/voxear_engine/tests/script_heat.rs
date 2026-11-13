//! Heat from C#: a behaviour asks how hot it is, sets off heat for a while,
//! reads the clock, makes its Obj a fire, and hears its Heat Body come to
//! feel scorching and change. On a copy of Hello. A file of its own, so it
//! runs in a process of its own, since .NET starts only once per process.
//! Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, heat::HeatSource, scripts};
use voxear_scene::Name;

const DT: f32 = 1.0 / 60.0;

const SCRIPT: &str = r##"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class HeatScript : Behaviour
{
	private int frames;

	protected override void Start()
	{
		HeatBody.Changed += change => Log.Info($"changed {change.Name} {change.Obj == Obj} {change.Became.IsValid} {change.Index}");
		HeatBody.FeelsChanged += (body, feels) => Log.Info($"feels {body.Obj == Obj} {feels}");
		Heat.AddSource(new Vector3(50f, 1f, 50f), 500f, 2f, 10f);
		Log.Info($"clock {Clock.Exists} {Clock.TimeOfDay} burst {Heat.At(new Vector3(50f, 1f, 50f)):0} far {Heat.At(new Vector3(500f, 1f, 500f)) < 100f}");

		var fire = Obj.AddHeatSource();
		fire.Celsius = 300f;
		fire.Radius = 2f;
		var body = Obj.HeatBody;
		body.ColdResistance = 20f;
		body.Temperature = 20f;
		var sample = Heat.Sample(Obj.Transform.Position);
		Log.Info($"cook {body.IsValid} {fire.Celsius} {sample.Celsius:0} {sample.Sources > 200f} {body.ColdResistance} {body.Feels} {body.ChangeCount} {body.ChangeName(0)}");
	}

	protected override void Update()
	{
		frames++;
		if (frames == 50)
		{
			var progress = Obj.HeatBody.Progress("Done");
			Log.Info($"progress {progress.Done} {progress.Share} {Obj.HeatBody.Temperature > 250f} {Obj.HeatBody.Beyond > 200f}");
		}
	}
}
"##;

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
fn a_script_asks_about_heat_makes_a_fire_and_hears_its_heat_body_change() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/HeatScript.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let added = "\n[[obj]]\nid = 900\nname = \"Cook\"\nposition = [300.0, 1.0, 300.0]\nbehaviours = [\"Hello.HeatScript\"]\n\
                 [obj.heat_body]\nwarms_in = 0.2\n[[obj.heat_body.changes]]\nname = \"Done\"\ncelsius = 200.0\nseconds = 0.2\n";
    std::fs::write(&scene, format!("{before}{added}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..60 {
        game.tick(DT, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |line: &str| lines.iter().any(|said| said == line);
    assert!(said("clock False 12 burst 500 far True"), "{lines:?}");
    assert!(said("cook True 300 300 True 20 Comfortable 1 Done"), "{lines:?}");
    assert!(said("feels True Scorching"), "{lines:?}");
    assert!(said("changed Done True False 0"), "{lines:?}");
    assert_eq!(lines.iter().filter(|line| line.starts_with("changed")).count(), 1, "once: {lines:?}");
    assert!(said("progress True 1 True True"), "{lines:?}");
    let world = &game.engine.world;
    let cook: Entity = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Cook").unwrap().0;
    assert_eq!(world.get::<HeatSource>(cook).map(|source| source.celsius), Some(300.0));
}
