//! A script's log lines say what they are about, an Obj or an asset, so the
//! Console can show it when the line is picked. Builds a copy of Hello's
//! scripts with one more; skips itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class LogAbout : SystemBase
{
	protected override void OnCreate()
	{
		var spinner = Obj.Find("Spinner A");
		Log.Info("about the spinner", spinner);
		Log.Warning("about the lamp", Prefab.Load("Prefabs/Lamp.prefab"));
		Log.Info("about the rare firefly", new Ref<Prefab>("Prefabs/RareFirefly.prefab"));
		Log.Info("about nothing");
	}

	protected override void Update()
	{
	}
}
"#;

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
fn a_scripts_log_line_names_the_obj_or_asset_it_is_about() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/LogAbout.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    let lines = log.lock().unwrap().lines.clone();
    let line = |message: &str| {
        lines.iter().find(|l| l.message == message).cloned().unwrap_or_else(|| panic!("no line {message:?}"))
    };
    let spinner = game
        .engine
        .world
        .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
        .into_iter()
        .find(|(_, name)| name.0 == "Spinner A")
        .map(|(entity, _)| entity)
        .unwrap();
    assert_eq!(line("about the spinner").context, Some(spinner.to_bits()), "an Obj, by its handle");
    assert_eq!(line("about the lamp").asset.as_deref(), Some("Prefabs/Lamp.prefab"), "a prefab, by its file");
    assert_eq!(
        line("about the rare firefly").asset.as_deref(),
        Some("Prefabs/RareFirefly.prefab"),
        "a reference, as it names the prefab"
    );
    let plain = line("about nothing");
    assert_eq!((plain.context, plain.asset), (None, None), "and a line about nothing names nothing");
}
