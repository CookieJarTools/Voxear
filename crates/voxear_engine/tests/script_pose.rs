//! A behaviour's PoseUpdate and a system in `Stage.Pose`: they run each
//! frame after Update and before LateUpdate, the behaviours first. Builds a
//! copy of Hello with one more script; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const POSER: &str = r#"using Voxear;

namespace Hello;

public sealed class Poser : Behaviour
{
	protected override void Update() => Log.Info("pose order: Update");

	protected override void PoseUpdate() => Log.Info("pose order: PoseUpdate");

	protected override void LateUpdate() => Log.Info("pose order: LateUpdate");
}

[UpdateIn(Stage.Pose)]
public sealed class PoseSystem : SystemBase
{
	protected override void OnCreate()
	{
		Obj.Find("Ground").AddBehaviour<Poser>();
	}

	protected override void Update() => Log.Info("pose order: Stage.Pose");
}
"#;

#[test]
fn pose_update_and_pose_systems_run_after_update_and_before_late_update() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Poser.cs"), POSER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    let order: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix("pose order: ")).collect();
    let frame = ["Update", "PoseUpdate", "Stage.Pose", "LateUpdate"];
    let whole = order.windows(4).filter(|window| *window == frame).count();
    assert!(whole >= 2, "each frame runs them in order: {order:?}");
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
