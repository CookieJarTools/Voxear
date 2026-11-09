//! A script copies Objs with `Obj.Instantiate`: Hello's Spinner B, whose
//! leader is outside the copy and stays its leader, and an instance of a
//! prefab of two Objs, whose root's field points at its child and, in the
//! copy, at the copy's child. Builds a copy of Hello with two more scripts
//! and the prefab; skips itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const POINTER: &str = r#"using Voxear;

namespace Hello;

public sealed class Pointer : Behaviour
{
	[SerializeField] private Obj target;

	public Obj Target => target;
}
"#;

const COPIER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Copier : SystemBase
{
	private int frame;

	protected override void Update()
	{
		if (++frame != 2)
		{
			return;
		}
		var spinner = Obj.Find("Spinner B");
		var copy = Obj.Instantiate(spinner);
		var near = Vector3.Distance(copy.WorldPosition, spinner.WorldPosition) < 0.001f;
		Log.Info($"spinner copy: {copy.Name} {copy != spinner} {near} {copy.GetBehaviour<Spinner>()?.DegreesPerSecond}");

		var pair = Prefab.Load("Pair.prefab").Instantiate(new Vector3(5f, 0f, 0f), Quaternion.Identity);
		var moved = Obj.Instantiate(pair, new Vector3(9f, 0f, 0f), Quaternion.Identity);
		var target = moved.GetBehaviour<Pointer>()!.Target;
		var its = Vector3.Distance(target.WorldPosition, new Vector3(9f, 0.5f, 0f)) < 0.001f;
		Log.Info($"pair copy: {target.Name} {target != pair.GetBehaviour<Pointer>()!.Target} {its}");
	}
}
"#;

const PAIR: &str = r#"format = 1

[[obj]]
id = 1
name = "Pair"
behaviours = ["Hello.Pointer"]
fields."Hello.Pointer" = { target = { obj = 2 } }

[[obj]]
id = 2
name = "Inner"
parent = 1
position = [0.0, 0.5, 0.0]
[obj.renderer]
mesh = "builtin:cube"
"#;

#[test]
fn a_script_copies_objs_with_their_behaviours_and_references() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Pointer.cs"), POINTER).unwrap();
    std::fs::write(dir.path().join("Scripts/Copier.cs"), COPIER).unwrap();
    std::fs::write(dir.path().join("Pair.prefab"), PAIR).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..4 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    // Following Spinner A the other way round, as the original does.
    assert!(lines.iter().any(|l| l == "spinner copy: Spinner B True True -90"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "Start    Spinner B (Spinner), score now 4"), "its behaviour starts: {lines:#?}");
    assert!(lines.iter().any(|l| l == "pair copy: Inner True True"), "its own child: {lines:#?}");
    assert!(!lines.iter().any(|l| l.contains("not being saved")), "{lines:#?}");
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
