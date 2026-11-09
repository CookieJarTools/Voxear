//! A LocalizedLabel from C#: a key's text with {0} an action's controls,
//! drawn for the device the player used last, and {1} a value worked out
//! each frame. On a copy of Hello. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use voxear_engine::{Engine, Game, Project, scripts};
use voxear_ui::{NodeId, Ui};

const DT: f32 = 1.0 / 60.0;

const STRINGS: &str = "[[entry]]\nkey = \"shop.buy\"\nen = \"Press {0} to buy for {1} coins\"\n";

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Shop : Behaviour
{
	private int price = 3;

	protected override void Start()
	{
		var hud = UiDocument.Create()!;
		hud.Root.Add(new LocalizedLabel("shop.buy", TextParam.Action(Input.Action("Submit")), TextParam.Bind(() => price)));
	}

	protected override void Update()
	{
		price = 5;
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

/// Every text shown under `node`, in order: the label's stretches and the
/// prompt's glyphs.
fn texts(ui: &Ui, node: NodeId, out: &mut Vec<String>) {
    let text = ui.text(node);
    if !text.is_empty() {
        out.push(text);
    }
    for child in ui.children(node) {
        texts(ui, child, out);
    }
}

fn shown(game: &mut Game) -> Vec<String> {
    let ui = game.ui.ui();
    let mut out = Vec::new();
    for document in ui.children(ui.root()) {
        texts(ui, document, &mut out);
    }
    out
}

#[test]
fn a_localized_label_shows_an_actions_controls_and_a_bound_value() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Text")).unwrap();
    std::fs::write(dir.path().join("Text/Shop.strings"), STRINGS).unwrap();
    std::fs::write(dir.path().join("Scripts/Shop.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let shop = "\n[[obj]]\nid = 900\nname = \"Shop\"\nbehaviours = [\"Hello.Shop\"]\n";
    std::fs::write(&scene, format!("{before}{shop}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();

    // The keyboard's key for Submit between the words, and the price as Start left it.
    game.tick(DT, None);
    let keyboard = shown(&mut game);
    assert_eq!(keyboard.len(), 3, "{keyboard:?}");
    assert_eq!((keyboard[0].as_str(), keyboard[2].as_str()), ("Press", "to buy for 3 coins"));

    // The price as Update set it, worked out again the next frame.
    game.tick(DT, None);
    assert_eq!(shown(&mut game)[2], "to buy for 5 coins");

    // A pad pressed: the same label shows the pad's button.
    let mut pad = voxear_input::Pad { family: voxear_input::Family::Xbox, ..voxear_input::Pad::default() };
    pad.buttons.insert("East", 1.0);
    game.input.devices_mut().set_pad(0, pad);
    game.tick(DT, None);
    let pad = shown(&mut game);
    assert_eq!(pad.len(), 3, "{pad:?}");
    assert_ne!(pad[1], keyboard[1], "the pad's button in place of the key");
    assert_eq!(pad[1], "A", "Submit is the pad's South button, A on an Xbox pad");
}
