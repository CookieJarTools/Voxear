//! The game's UI made from code with what its elements are given as they
//! are made: a handler in a button's constructor, a label bound to the
//! game's state, a tooltip, and keys' text for a toggle, a label with
//! values, a placeholder, a dropdown and a foldout, which follow the
//! language; two of the keys named as the table's typed keys, `Strings`.
//! On a copy of Hello with a script and a string table of its own. A file
//! of its own, so it runs in a process of its own, since .NET starts only
//! once per process. Skips itself without .NET.

use voxear_engine::{Engine, Game, Project, localization::Localization, scripts};
use voxear_ui::{NodeId, Ui};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class BoundHud : SystemBase
{
	private int clicks;

	protected override void OnCreate()
	{
		var hud = UiDocument.Create()!;
		var panel = hud.Root.Add(new Panel("hud"));
		panel.Add(new Label().Bind(() => $"Clicked {clicks}"));
		var button = panel.Add(new Button("Click me", () => clicks++) { Tooltip = "Counts a click" });
		button.SetStyle("width: 120px; height: 32px");
		panel.Add(new Toggle(Strings.Menu.Music, on: true));
		panel.Add(new Label().Localize(Strings.Hud.Coins, ("count", 3)));
		panel.Add(new TextField { Placeholder = new LocalizedString("menu.name") });
		panel.Add(new Dropdown(new LocalizedString("menu.easy"), "Custom"));
		panel.Add(new Foldout(new LocalizedString("menu.more")));
	}

	protected override void Update()
	{
	}
}
"#;

const MENU: &str = "[[entry]]\nkey = \"menu.music\"\nen = \"Music\"\nfr = \"Musique\"\n\n\
                    [[entry]]\nkey = \"hud.coins\"\nen = \"{count, plural, one {# coin} other {# coins}}\"\n\
                    fr = \"{count, plural, one {# pièce} other {# pièces}}\"\n\n\
                    [[entry]]\nkey = \"menu.name\"\nen = \"Your name\"\nfr = \"Votre nom\"\n\n\
                    [[entry]]\nkey = \"menu.easy\"\nen = \"Easy\"\nfr = \"Facile\"\n\n\
                    [[entry]]\nkey = \"menu.more\"\nen = \"More\"\nfr = \"Plus\"\n";

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

/// Every element's own text.
fn texts(ui: &Ui) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        found.push(ui.text(node));
        stack.extend(ui.children(node));
    }
    found
}

/// The element whose own text is `text`, with nothing in it.
fn reading(ui: &Ui, text: &str) -> Option<NodeId> {
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if ui.text(node) == text && ui.children(node).is_empty() {
            return Some(node);
        }
        stack.extend(ui.children(node));
    }
    None
}

#[test]
fn elements_made_in_code_run_their_handlers_show_bound_text_and_follow_the_language() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = hello_copy();
    let toml = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, format!("{project}\n\n[localization]\nlanguages = [\"en\", \"fr\"]\n")).unwrap();
    std::fs::create_dir_all(dir.path().join("Text")).unwrap();
    std::fs::write(dir.path().join("Text/Menu.strings"), MENU).unwrap();
    std::fs::write(dir.path().join("Scripts/BoundHud.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (800, 600), 1.0);

    // Keys' text shows in the game's language, with the label's values.
    let shown = texts(game.ui.ui());
    for text in ["Clicked 0", "Music", "3 coins", "Easy", "More"] {
        assert!(shown.contains(&text.to_string()), "{text} in {shown:?}");
    }
    let button = reading(game.ui.ui(), "Click me").expect("the button");
    assert_eq!(game.ui.ui().tooltip(button), Some("Counts a click"));

    // A click runs the handler given to the constructor, and the bound label shows what it changed.
    let ui = game.ui.ui();
    let [x, y, w, h] = ui.rect(button);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    game.tick(1.0 / 60.0, None);
    // A frame laid out lets go of the click, as one drawn does.
    game.ui.frame(1.0 / 60.0, (800, 600), 1.0);
    assert!(texts(game.ui.ui()).contains(&"Clicked 1".to_string()), "{:?}", texts(game.ui.ui()));

    // In French, every key's text is shown again by itself.
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("fr");
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    let shown = texts(game.ui.ui());
    for text in ["Musique", "3 pièces", "Facile", "Plus"] {
        assert!(shown.contains(&text.to_string()), "{text} in {shown:?}");
    }
}
