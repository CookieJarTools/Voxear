//! The Inspector asks a behaviour's code in edit mode, through the interop:
//! a `[Button]` method called on a copy made from the scene's values, a
//! `[ValidateInput]` method asked about a field, and a `[ValueDropdown]`
//! member's values, on a copy of Hello with a script of its own. A file of
//! its own, so it runs in a process of its own, since .NET starts only once
//! per process. Skips itself without .NET.

use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::{BehaviourSpec, Name};
use voxear_scripting::{InspectorAnswer, InspectorAsk};

const TOWER: &str = r#"using System.Collections.Generic;
using Voxear;

namespace Hello
{
	public sealed class Tower : Behaviour
	{
		[OnValueChanged(nameof(Rebuild))]
		public int Floors = 3;

		public float Height;

		[ValidateInput(nameof(IsColour), "Pick a colour the tower has")]
		[ValueDropdown(nameof(Colours))]
		public string Paint = "Red";

		[Button("Rebuild the tower")]
		private void Rebuild()
		{
			Height = Floors * 2.5f + Obj.Position.Y;
		}

		private bool IsColour(string paint)
		{
			return paint == "Red" || paint == "Blue";
		}

		private IEnumerable<ValueDropdownItem> Colours()
		{
			yield return new ValueDropdownItem("Brick red", "Red");
			yield return new ValueDropdownItem("Sky blue", "Blue");
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
fn the_inspector_asks_a_behaviours_code_in_edit_mode() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Tower.cs"), TOWER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let world = &game.engine.world;
    let spinner = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner A").unwrap().0;
    let y = world.get::<voxear_scene::Transform>(spinner).unwrap().position.y;

    // No Tower is on the Obj: the Inspector's copy is made from the scene's values alone.
    let mut tower = BehaviourSpec::new("Hello.Tower");
    let fields = std::sync::Arc::make_mut(&mut tower.fields);
    fields.insert("floors".into(), toml::Value::Integer(4));
    fields.insert("paint".into(), toml::Value::String("Green".into()));

    let changed = game.engine.ask_inspector(spinner, &tower, InspectorAsk::Call("Rebuild")).unwrap();
    let InspectorAnswer::Changed(changed) = changed else { panic!("a call answers with what changed: {changed:?}") };
    assert_eq!(changed.keys().collect::<Vec<_>>(), ["Height"], "only what the call changed");
    let height = changed["Height"].as_float().unwrap();
    assert!((height - (10.0 + f64::from(y))).abs() < 1e-4, "the scene's floors, and its Obj's height: {height}");

    let ask = InspectorAsk::Validate { method: "IsColour", field: "Paint" };
    assert_eq!(game.engine.ask_inspector(spinner, &tower, ask).unwrap(), InspectorAnswer::Valid(false, String::new()));
    std::sync::Arc::make_mut(&mut tower.fields).insert("paint".into(), toml::Value::String("Blue".into()));
    assert_eq!(game.engine.ask_inspector(spinner, &tower, ask).unwrap(), InspectorAnswer::Valid(true, String::new()));

    let options = game.engine.ask_inspector(spinner, &tower, InspectorAsk::Dropdown("Colours")).unwrap();
    let expected =
        [("Brick red", "Red"), ("Sky blue", "Blue")].map(|(l, v)| (l.to_string(), toml::Value::String(v.into())));
    assert_eq!(options, InspectorAnswer::Options(expected.to_vec()));

    let missing = game.engine.ask_inspector(spinner, &tower, InspectorAsk::Call("Demolish")).unwrap_err();
    assert!(format!("{missing:#}").contains("no method Demolish"), "{missing:#}");
}
