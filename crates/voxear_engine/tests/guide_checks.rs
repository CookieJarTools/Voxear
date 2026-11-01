//! A guide's checks run against a game's scripts: a static method that
//! returns `bool` or `string`, called through the interop as the editor
//! calls it while a guide shows, on a copy of Hello with a script of its
//! own. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::{Name, Transform};

const CHECKS: &str = r#"using Voxear;

namespace Hello
{
	public static class TutorialChecks
	{
		public static bool SunIsThere()
		{
			return Obj.Find("Sun").IsValid;
		}

		public static string SpinnerRaised()
		{
			return Obj.Find("Spinner A").Position.Y > 3f ? "" : "Spinner A is still near the ground";
		}

		public static bool Broken()
		{
			throw new System.InvalidOperationException("this check is broken");
		}
	}
}
"#;

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
fn checks_pass_or_say_what_is_still_wrong() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/TutorialChecks.cs"), CHECKS).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    let checks = "Hello.TutorialChecks";
    assert_eq!(game.engine.run_check(checks, "SunIsThere").unwrap(), Ok(()), "true passes");
    let still = game.engine.run_check(checks, "SpinnerRaised").unwrap();
    assert_eq!(still, Err("Spinner A is still near the ground".to_string()), "text says what is still wrong");

    let world = &mut game.engine.world;
    let spinner = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner A").unwrap().0;
    world.get_mut::<Transform>(spinner).unwrap().position.y = 4.0;
    assert_eq!(game.engine.run_check(checks, "SpinnerRaised").unwrap(), Ok(()), "and no text passes, once it is done");

    let broken = game.engine.run_check(checks, "Broken").unwrap_err();
    assert!(format!("{broken:#}").contains("threw"), "{broken:#}");
    assert!(
        log.lock().unwrap().lines.iter().any(|l| l.message.contains("this check is broken")),
        "what it threw is logged"
    );
    let missing = game.engine.run_check(checks, "Nothing").unwrap_err();
    assert!(format!("{missing:#}").contains("has no check Nothing"), "{missing:#}");
}
