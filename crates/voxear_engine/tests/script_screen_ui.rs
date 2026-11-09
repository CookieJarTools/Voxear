//! A Screen UI from C#: its behaviour subscribes in OnUiBuilt and hears its
//! button, through switching its Obj off and on; and the same behaviour
//! loads a document by a field picked as an asset and styles it with typed
//! properties, keeping what its markup's style attribute says. On a copy of
//! Hello. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scene::Name;

const DT: f32 = 1.0 / 60.0;

const HUD: &str = r#"<vui><Panel name="hud" style="position: absolute; left: 0px; top: 300px"><Label name="coins">0</Label><Button name="add" style="width: 120px; height: 32px">Add</Button></Panel></vui>"#;

const SHOP: &str = r#"<vui><Panel name="shop" style="height: 50px"><Label name="title">Shop</Label></Panel></vui>"#;

const SCRIPT: &str = r##"using Voxear;

namespace Hello;

public sealed class HudScript : Behaviour
{
	[SerializeField] private UiDocumentAsset shop;
	private int coins;
	private int builds;

	protected override void Start()
	{
		var panel = UiDocument.Load(shop)!.Find("shop")!;
		panel.SetStyles(position: Position.Absolute, left: 20, top: 30, width: 120, backgroundColor: Color.Hex("#202020"));
		panel.Style.Width = Length.Percent(50);
		Log.Info($"shop style {panel.Style.Left} {panel.Style.Width} {panel.Style.Height?.ToString() ?? "unset"}");
	}

	protected override void OnUiBuilt(UiDocument ui)
	{
		builds++;
		var label = ui.Find("coins")!;
		ui.Find("add")!.Clicked += () => label.Text = $"{++coins} coins, built {builds}";
	}
}
"##;

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

/// One frame as the window runs it: the tick, then the screen laid out,
/// which also clears what was clicked.
fn frame(game: &mut Game) {
    game.tick(DT, None);
    game.ui.frame(DT, (800, 600), 1.0);
}

/// Clicks the screen's element named `name`; whether the UI took it.
fn click(game: &mut Game, name: &str) -> bool {
    let ui = game.ui.ui();
    let node = ui.find(ui.root(), name).expect("on the screen");
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    let taken = ui.pointer_button(true);
    ui.pointer_button(false);
    taken
}

#[test]
fn a_screen_ui_is_subscribed_once_and_styled_in_code() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    std::fs::write(dir.path().join("UI/Hud.vui"), HUD).unwrap();
    std::fs::write(dir.path().join("UI/Shop.vui"), SHOP).unwrap();
    std::fs::write(dir.path().join("Scripts/HudScript.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let hud = "\n[[obj]]\nid = 900\nname = \"Hud\"\nbehaviours = [\"Hello.HudScript\"]\n\
               fields.\"Hello.HudScript\" = { shop = \"UI/Shop.vui\" }\n[obj.screen_ui]\ndocument = \"UI/Hud.vui\"\n";
    std::fs::write(&scene, format!("{before}{hud}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let obj: Entity = game
        .engine
        .world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .find(|(_, name)| name.0 == "Hud")
        .map(|(entity, _)| entity)
        .expect("the HUD is in the scene");

    frame(&mut game);
    let document = game.ui.screen_document(obj).expect("the Screen UI shows its document");
    let coins = game.ui.find(document, "coins").unwrap();
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    assert!(lines.iter().any(|line| line == "shop style 20px 50% unset"), "{lines:?}");

    // The shop, loaded by its field and styled in code, keeps its style attribute's height.
    let ui = game.ui.ui();
    let shop = ui.find(ui.root(), "shop").expect("the shop's document");
    assert_eq!(ui.rect(shop), [20.0, 30.0, 400.0, 50.0]);

    // A click: OnUiBuilt's handler hears it, once.
    assert!(click(&mut game, "add"));
    frame(&mut game);
    frame(&mut game);
    assert_eq!(game.ui.text(coins).as_deref(), Some("1 coins, built 1"));

    // Switched off, hidden; back on, the same handler.
    let whole = game.ui.find(document, "").unwrap();
    voxear_scene::set_active(&mut game.engine.world, obj, false);
    frame(&mut game);
    assert!(!game.ui.visible(whole));
    voxear_scene::set_active(&mut game.engine.world, obj, true);
    frame(&mut game);
    assert!(game.ui.visible(whole));
    assert!(click(&mut game, "add"));
    frame(&mut game);
    assert_eq!(game.ui.text(coins).as_deref(), Some("2 coins, built 1"));

    // Destroyed, its document goes with it.
    game.engine.destroy_objs(&[obj]);
    frame(&mut game);
    assert!(game.ui.find(document, "coins").is_none());
}
