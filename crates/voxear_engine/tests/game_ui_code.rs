//! The game's own UI made from code, as Unity's UI Toolkit makes it: an
//! empty document filled with `new Panel()`, `new Label()` and the rest,
//! handlers on what was made, and elements taken out and put back, on a
//! copy of Hello with a script of its own. A file of its own, so it runs in
//! a process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class CodeHud : SystemBase
{
	private Label score = null!;
	private Panel spare = null!;
	private int clicks;

	protected override void OnCreate()
	{
		var hud = UiDocument.Create()!;
		var panel = hud.Root.Add(new Panel("hud"));
		score = panel.Add(new Label("0"));
		var button = panel.Add(new Button("Click me"));
		button.SetStyle("width: 120px; height: 32px");
		button.Clicked += () => score.Text = $"Clicked {++clicks}";
		var volume = panel.Add(new Slider(0, 10, value: 4));
		volume.Moved += value => Log.Info($"moved to {value}");
		var sound = panel.Add(new Toggle("Sound", on: true));
		Log.Info($"slider {volume.Value}, sound {sound.Checked}, quality {panel.Add(new Dropdown("Low", "High")).Options.Count}");

		// Made before it is on screen: what is set on it waits, then shows.
		spare = new Panel("spare");
		spare.Add(new Label("inside"));
		spare.AddClass("waiting");
		Log.Info($"spare shown {spare.IsShown}, has waiting {spare.HasClass("waiting")}");
		panel.Add(spare);
		Log.Info($"spare shown {spare.IsShown}");

		// A picture of the project, as an item's icon: an image, which shows it where there is a renderer to.
		var icon = panel.Add(new Image(Texture.Load("Docs/How Hello Works/images/Day.png")));
		icon.AddClass("icon");
		Log.Info($"icon shown {icon.IsShown}, of {icon.Picture.IsSet}");
	}

	protected override void Update()
	{
		if (clicks == 1 && spare.Parent != null)
		{
			spare.Parent.Remove(spare);
			Log.Info($"spare removed, shown {spare.IsShown}, children {spare.Children.Count}");
		}
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
fn scripts_build_their_ui_in_code_and_hear_it() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/CodeHud.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    let lines = said(&log);
    assert!(lines.contains(&"slider 4, sound True, quality 2".to_string()), "{lines:#?}");
    assert!(lines.contains(&"spare shown False, has waiting True".to_string()), "{lines:#?}");
    assert!(lines.contains(&"spare shown True".to_string()), "{lines:#?}");

    // What was made is on screen, with what was set on it before.
    assert!(!game.ui.is_empty(), "the made document shows");
    game.ui.frame(1.0 / 60.0, (800, 600), 1.0);
    let ui = game.ui.ui();
    let mut inside = None;
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if ui.text(node) == "inside" {
            inside = Some(node);
        }
        stack.extend(ui.children(node));
    }
    let inside = inside.expect("the spare panel's label shows");
    let spare = ui.parent(inside).unwrap();
    assert!(ui.has_class(spare, "spare") && ui.has_class(spare, "waiting"), "its classes came with it");
    // The image made in code is an image on screen, beside the spare panel.
    assert!(lines.contains(&"icon shown True, of True".to_string()), "{lines:#?}");
    let hud = ui.parent(spare).unwrap();
    let icon = ui.children(hud).into_iter().find(|&node| ui.has_class(node, "icon")).expect("the icon");
    assert_eq!(ui.kind(icon), Some(voxear_ui::Kind::Image));

    // A click on the made button reaches its handler at the start of the next frame.
    let mut buttons = Vec::new();
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if ui.text(node) == "Click me" {
            buttons.push(node);
        }
        stack.extend(ui.children(node));
    }
    let [x, y, w, h] = ui.rect(buttons[0]);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    assert!(ui.pointer_button(true), "the game's UI takes a press over it");
    ui.pointer_button(false);
    game.tick(1.0 / 60.0, None);
    let ui = game.ui.ui();
    let mut texts = Vec::new();
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        texts.push(ui.text(node));
        stack.extend(ui.children(node));
    }
    assert!(texts.contains(&"Clicked 1".to_string()), "{texts:?}");
    let lines = said(&log);
    assert!(lines.contains(&"spare removed, shown False, children 1".to_string()), "{lines:#?}");
    assert!(!texts.contains(&"inside".to_string()), "taken off the screen with what is in it");
}
