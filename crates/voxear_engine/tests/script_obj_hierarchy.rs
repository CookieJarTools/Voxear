//! A script reads where Objs are in the hierarchy: Hello's Pyramid, the
//! model inside the Pyramid it stands on, and the Sun, inside nothing.
//! Builds a copy of Hello with one more script; skips itself without .NET
//! or the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const FAMILY: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class Family : SystemBase
{
	protected override void Update()
	{
		var pyramids = Obj.FindAll("Pyramid");
		var outer = pyramids.First(pyramid => !pyramid.Parent.IsValid);
		var inner = outer.Children.Single();
		var sun = Obj.Find("Sun");
		Log.Info($"family: {outer.Children.Length} in the pyramid, its parent {inner.Parent == outer}, {inner.Children.Length} further in, the sun inside {sun.Parent.IsValid}");
	}
}
"#;

#[test]
fn a_script_reads_an_objs_parent_and_children() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Family.cs"), FAMILY).unwrap();
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
    let wanted = "family: 1 in the pyramid, its parent True, 0 further in, the sun inside False";
    assert!(lines.iter().any(|l| l == wanted), "{lines:#?}");
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
