//! A UI Panel's document from C#: a script on the panel's Obj finds its
//! button through `Obj.UiPanel.Document` and hears it clicked by a ray, on a
//! copy of Hello. A file of its own, so it runs in a process of its own,
//! since .NET starts only once per process. Skips itself without .NET.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::Name;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

const TERMINAL: &str = r#"<vui><Panel name="screen" style="flex-direction: row; width: 100%; height: 100%"><Label name="said" style="flex-grow: 1">Waiting</Label><Button name="go" style="flex-grow: 1; height: 100%">Go</Button></Panel></vui>"#;

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Terminal : Behaviour
{
	private UiElement? said;

	protected override void Update()
	{
		if (said != null)
		{
			return;
		}
		var screen = Obj.UiPanel.Document;
		if (screen == null)
		{
			return;
		}
		said = screen.Find("said");
		screen.Find("go")!.Clicked += () => said!.Text = $"Opened by {Obj.Name}, {Obj.UiPanel.Size.X} m wide";
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
fn a_script_hears_a_click_on_its_panel() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    std::fs::write(dir.path().join("UI/Terminal.vui"), TERMINAL).unwrap();
    std::fs::write(dir.path().join("Scripts/Terminal.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let terminal = "\n[[obj]]\nid = 900\nname = \"Terminal\"\nposition = [0.0, 20.0, 5.0]\nbehaviours = [\"Hello.Terminal\"]\n\
                    [obj.ui_panel]\ndocument = \"UI/Terminal.vui\"\nsize = [2.0, 1.0]\n";
    std::fs::write(&scene, format!("{text}{terminal}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let panel: Entity = game
        .engine
        .world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .find(|(_, name)| name.0 == "Terminal")
        .map(|(entity, _)| entity)
        .expect("the terminal is in the scene");

    // The panel's document loads, and the script finds it the frame after.
    let frame = |game: &mut Game| {
        game.ui.sync_panels(&mut game.engine.world, &mut game.content, &HashMap::new());
        game.ui.lay_out_panels(1.0 / 60.0);
        game.tick(1.0 / 60.0, None);
    };
    frame(&mut game);
    frame(&mut game);
    let document = game.ui.panel_document(panel).expect("the panel shows its document");
    let said = game.ui.find(document, "said").unwrap();
    assert_eq!(game.ui.text(said).as_deref(), Some("Waiting"));

    // Pointed at from in front, at its right half, and clicked: the script hears it at the next frame.
    let from = Vec3::new(0.0, 20.0, 0.0);
    game.ui.point_panels(&game.engine.world, Some((from, Vec3::new(0.5, 0.0, 5.0).normalize())), |_, _| None);
    for state in [ElementState::Pressed, ElementState::Released] {
        let event = WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button: MouseButton::Left };
        assert!(game.ui.panel_input(&event), "the panel takes it");
    }
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(said).as_deref(), Some("Opened by Terminal, 2 m wide"));
}
