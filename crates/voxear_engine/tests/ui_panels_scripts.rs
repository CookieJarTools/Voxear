//! UI from C#, subscribed once: a script on a UI Panel's Obj finds its
//! button in Start, another in OnUiBuilt, and a third loads the screen's UI
//! in Start, each hearing clicks from then on. The panel's handlers keep
//! running with it switched off and on and its file saved again; OnUiBuilt
//! subscribes again for a new document, and after the scripts reload. On a
//! copy of Hello. A file of its own, so it runs in a process of its own,
//! since .NET starts only once per process. Skips itself without .NET.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::Name;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

const DT: f32 = 1.0 / 60.0;

const TERMINAL: &str = r#"<vui><Panel name="screen" style="flex-direction: row; width: 100%; height: 100%"><Panel style="flex-grow: 1"><Label name="said">Waiting</Label><Label name="built">Not built</Label></Panel><Button name="go" style="flex-grow: 1; height: 100%">Go</Button></Panel></vui>"#;

const HUD: &str = r#"<vui><Panel style="position: absolute; left: 20px; top: 20px"><Label name="count">0</Label><Button name="add" style="width: 120px; height: 32px">Add</Button></Panel></vui>"#;

/// Subscribes once, in Start.
const TERMINAL_SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Terminal : Behaviour
{
	private int opened;

	protected override void Start()
	{
		var screen = Obj.UiPanel.Document!;
		var said = screen.Find("said")!;
		screen.Find("go")!.Clicked += () => said.Text = $"Opened {++opened} by {Obj.Name}, {Obj.UiPanel.Size.X} m wide";
	}
}
"#;

/// Subscribes in OnUiBuilt, each time the panel's document is built.
const GATE_SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Gate : Behaviour
{
	private int builds;
	private int clicks;

	protected override void OnUiBuilt(UiDocument ui)
	{
		builds++;
		var built = ui.Find("built")!;
		ui.Find("go")!.Clicked += () => built.Text = $"Built {builds}, clicked {++clicks}";
	}
}
"#;

/// The screen's UI, loaded and subscribed in Start.
const HUD_SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Hud : Behaviour
{
	private int added;

	protected override void Start()
	{
		var hud = UiDocument.Load("UI/Hud.vui")!;
		var count = hud.Find("count")!;
		hud.Find("add")!.Clicked += () => count.Text = $"Added {++added}";
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

/// Lays the panels and the screen's UI out as the window does after each
/// frame's tick, which also clears what was clicked.
fn draw(game: &mut Game) {
    game.ui.sync_panels(&mut game.engine.world, &mut game.content, &HashMap::new());
    game.ui.lay_out_panels(DT);
    game.ui.frame(DT, (800, 600), 1.0);
}

/// One frame as the window runs it.
fn frame(game: &mut Game) {
    game.tick(DT, None);
    draw(game);
}

/// Points at the panel's button, on its right half, and clicks; whether the
/// panel took the click.
fn click_panel(game: &mut Game) -> bool {
    let from = Vec3::new(0.0, 20.0, 0.0);
    game.ui.point_panels(&game.engine.world, Some((from, Vec3::new(0.5, 0.0, 5.0).normalize())), |_, _| None);
    [ElementState::Pressed, ElementState::Released].into_iter().all(|state| {
        game.ui.panel_input(&WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button: MouseButton::Left })
    })
}

fn text(game: &Game, element: u32) -> String {
    game.ui.text(element).unwrap_or_default()
}

#[test]
fn scripts_subscribe_to_ui_once_and_keep_hearing_it() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    std::fs::write(dir.path().join("UI/Terminal.vui"), TERMINAL).unwrap();
    std::fs::write(dir.path().join("UI/Gate.vui"), TERMINAL.replace("Waiting", "Gate")).unwrap();
    std::fs::write(dir.path().join("UI/Hud.vui"), HUD).unwrap();
    std::fs::write(dir.path().join("Scripts/Terminal.cs"), TERMINAL_SCRIPT).unwrap();
    std::fs::write(dir.path().join("Scripts/Gate.cs"), GATE_SCRIPT).unwrap();
    std::fs::write(dir.path().join("Scripts/Hud.cs"), HUD_SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text_before = std::fs::read_to_string(&scene).unwrap();
    let terminal = "\n[[obj]]\nid = 900\nname = \"Terminal\"\nposition = [0.0, 20.0, 5.0]\n\
                    behaviours = [\"Hello.Terminal\", \"Hello.Gate\", \"Hello.Hud\"]\n\
                    [obj.ui_panel]\ndocument = \"UI/Terminal.vui\"\nsize = [2.0, 1.0]\n";
    std::fs::write(&scene, format!("{text_before}{terminal}")).unwrap();
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

    // The first frame: Start and OnUiBuilt find the panel's document before it is ever drawn.
    game.tick(DT, None);
    let document = game.ui.panel_document(panel).expect("loaded when Start asked for it");
    let said = game.ui.find(document, "said").unwrap();
    let built = game.ui.find(document, "built").unwrap();
    assert_eq!(text(&game, said), "Waiting");
    draw(&mut game);

    // A click: both handlers hear it the next frame, once.
    assert!(click_panel(&mut game), "the panel takes it");
    frame(&mut game);
    frame(&mut game);
    assert_eq!(text(&game, said), "Opened 1 by Terminal, 2 m wide");
    assert_eq!(text(&game, built), "Built 1, clicked 1");

    // The screen's UI, loaded in Start, hears its button too.
    let ui = game.ui.ui();
    let add = ui.find(ui.root(), "add").expect("the screen's document");
    let [x, y, w, h] = ui.rect(add);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    assert!(ui.pointer_button(true), "the screen's UI takes a press over it");
    ui.pointer_button(false);
    frame(&mut game);
    let count = game.ui.find(0, "count").unwrap();
    assert_eq!(text(&game, count), "Added 1");

    // Switched off, it takes no clicks; back on, its handlers still run.
    voxear_scene::set_active(&mut game.engine.world, panel, false);
    frame(&mut game);
    assert!(!click_panel(&mut game), "nothing to click while it is off");
    voxear_scene::set_active(&mut game.engine.world, panel, true);
    frame(&mut game);
    assert!(click_panel(&mut game));
    frame(&mut game);
    assert_eq!(text(&game, said), "Opened 2 by Terminal, 2 m wide");
    assert_eq!(text(&game, built), "Built 1, clicked 2");

    // Its file saved again: the same elements, found again by name, with their handlers.
    std::fs::write(dir.path().join("UI/Terminal.vui"), TERMINAL.replace(">Go<", ">Open<")).unwrap();
    game.ui.reload_panels(&mut game.content);
    frame(&mut game);
    let go = game.ui.find(document, "go").unwrap();
    assert_eq!(text(&game, go), "Open");
    assert!(click_panel(&mut game));
    frame(&mut game);
    assert_eq!(text(&game, said), "Opened 3 by Terminal, 2 m wide");
    assert_eq!(text(&game, built), "Built 1, clicked 3", "not built again");

    // Given another document: OnUiBuilt subscribes to it. Start's handler stays with the old one.
    let gate = game.content.db().guid_of("UI/Gate.vui").expect("the gate's document");
    game.engine.world.get_mut::<voxear_render::UiPanel>(panel).unwrap().document = Some(gate);
    frame(&mut game);
    let document = game.ui.panel_document(panel).unwrap();
    let said = game.ui.find(document, "said").unwrap();
    let built = game.ui.find(document, "built").unwrap();
    assert!(click_panel(&mut game));
    frame(&mut game);
    assert_eq!(text(&game, said), "Gate");
    assert_eq!(text(&game, built), "Built 2, clicked 4");

    // The scripts reloaded: the old scripts' handlers are let go, and OnUiBuilt subscribes again.
    let gate_file = dir.path().join("Scripts/Gate.cs");
    std::fs::write(&gate_file, GATE_SCRIPT.replace("Built {builds}", "Rebuilt {builds}")).unwrap();
    scripts::build(&Project::open(dir.path()).unwrap()).unwrap();
    game.engine.reload_scripts().unwrap();
    frame(&mut game);
    assert!(click_panel(&mut game));
    frame(&mut game);
    assert_eq!(text(&game, built), "Rebuilt 3, clicked 5", "heard once, by the new scripts");
}
