//! Input actions named by reference from C#: a behaviour's
//! `InputActionRef` field, set by the action's id as the Inspector's picker
//! sets it, and the `Actions` class made from the project's actions, here
//! Voxear's own, as Hello has none of its own. On a copy of Hello. A file
//! of its own, so it runs in a process of its own, since .NET starts only
//! once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Controls : Behaviour
{
	[SerializeField] private InputActionRef jump;

	protected override void Start()
	{
		var coded = Actions.Player.Jump;
		Log.Info($"field {jump.Action.Name} {jump.Action.Exists}, code {coded.Action.Name} {coded == jump}, submit {Actions.UI.Submit.Action.Name}");
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
fn a_field_and_the_actions_class_name_input_actions_by_id() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    for entry in walk(dir.path()) {
        assert!(entry.extension().is_none_or(|e| e != "inputactions"), "Hello plays with Voxear's own actions");
    }
    let defaults: voxear_input::ActionsFile = toml::from_str(voxear_input::DEFAULT_ACTIONS).unwrap();
    let jump = defaults.maps[0].actions.iter().find(|action| action.name == "Jump").unwrap().id.clone();
    std::fs::write(dir.path().join("Scripts/Controls.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let obj = format!(
        "\n[[obj]]\nid = 900\nname = \"Controls\"\nbehaviours = [\"Hello.Controls\"]\nfields.\"Hello.Controls\" = {{ jump = \"{jump}\" }}\n"
    );
    std::fs::write(&scene, format!("{before}{obj}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    assert!(lines.iter().any(|line| line == "field Jump True, code Jump True, submit Submit"), "{lines:?}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}
