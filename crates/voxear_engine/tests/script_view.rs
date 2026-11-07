//! A script and the First-Person View: it gives the camera's Obj a view,
//! sets how it looks and where, and reads the camera's field of view back
//! after changing it. Builds a copy of Hello with one more script; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, first_person::FirstPersonView, scripts};
use voxear_render::Camera;

const LOOKER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Looker : SystemBase
{
	protected override void OnCreate()
	{
		var eye = Obj.Find("Main Camera");
		var view = eye.AddFirstPersonView();
		view.MouseLook = false;
		view.Sensitivity = 0.25f;
		view.Yaw = 30f;
		view.Pitch = 120f;
		view.Eyes = new Vector3(0f, 0.1f, 0.2f);
		view.Camera = eye.Camera;
		var camera = eye.Camera;
		camera.FieldOfView = 50f;
		Log.Info($"view {view.IsValid} {view.Sensitivity} {view.Yaw} {view.Pitch} {view.Eyes.Z} {view.Camera.Obj == eye}");
		Log.Info($"fov {eye.Camera.FieldOfView} {eye.Camera.IsValid} {Obj.Find("Ground").Camera.IsValid}");
	}

	protected override void Update()
	{
	}
}
"#;

#[test]
fn a_script_gives_an_obj_a_first_person_view_and_sets_its_camera() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Looker.cs"), LOOKER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(lines.iter().any(|l| l == "view True 0.25 30 85 0.2 True"), "the tilt is held to its limit: {lines:#?}");
    assert!(lines.iter().any(|l| l == "fov 50 True False"), "{lines:#?}");
    let world = &game.engine.world;
    let views: Vec<FirstPersonView> = world.query_ref::<&FirstPersonView>().into_iter().cloned().collect();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].sensitivity, 0.25);
    let fovs: Vec<f32> = world.query_ref::<&Camera>().into_iter().map(|c| c.fov_y_degrees).collect();
    assert_eq!(fovs, [50.0]);
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
