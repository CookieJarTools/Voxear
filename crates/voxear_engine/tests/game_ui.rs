//! The game's own UI from C#: a document loaded from the project, its
//! elements changed, and a click heard, on a copy of Hello with a script of
//! its own. A file of its own, so it runs in a process of its own, since
//! .NET starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, game_ui::ClassChange, scripts};

const HUD: &str = r#"<vui>
    <Style src="Hud.vss" />
    <Panel name="hud" class="hud">
        <Label name="score">0</Label>
        <Panel name="bar" class="bar" />
        <Button name="button" class="button">Click me</Button>
    </Panel>
</vui>"#;

const SHEET: &str = ".hud { position: absolute; left: 20px; top: 20px; gap: 8px; }
.bar { width: 100px; height: 8px; background-color: #333333; }
.bar.full { background-color: #8b93ff; }
.button { width: 120px; height: 32px; }";

const SCRIPT: &str = r#"using Voxear;

namespace Hello
{
	public sealed class HudSystem : SystemBase
	{
		private UiElement? score;
		private int clicks;

		protected override void OnCreate()
		{
			var hud = UiDocument.Load("UI/Hud.vui")!;
			score = hud.Find("score");
			hud.Find("bar")!.AddClass("full");
			hud.Find("button")!.Clicked += () =>
			{
				clicks++;
				score!.Text = $"Clicked {clicks}";
			};
			if (UiDocument.Load("UI/Missing.vui") != null || hud.Find("nothing") != null)
			{
				Log.Error("a missing document or element should be null");
			}
		}

		protected override void Update()
		{
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
fn scripts_show_ui_documents_and_hear_their_clicks() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    std::fs::write(dir.path().join("UI/Hud.vui"), HUD).unwrap();
    std::fs::write(dir.path().join("UI/Hud.vss"), SHEET).unwrap();
    std::fs::write(dir.path().join("Scripts/HudSystem.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let errors: Vec<String> = log
        .lock()
        .unwrap()
        .lines
        .iter()
        .filter(|l| l.level == tracing::Level::ERROR)
        .map(|l| l.message.clone())
        .collect();
    assert_eq!(errors.len(), 1, "only the missing document's error: {errors:?}");
    assert!(errors[0].contains("UI/Missing.vui"), "{errors:?}");

    // The document shows, with the class the script added.
    assert!(!game.ui.is_empty());
    let score = game.ui.find(0, "score").expect("the document's elements");
    assert_eq!(game.ui.text(score).as_deref(), Some("0"));
    let bar = game.ui.find(0, "bar").unwrap();
    assert!(game.ui.class(bar, "full", ClassChange::Has), "the script added a class");

    // A click on the button reaches the script's handler at the start of the next frame.
    game.ui.frame(1.0 / 60.0, (800, 600), 1.0);
    let ui = game.ui.ui();
    let button = ui.find(ui.root(), "button").unwrap();
    let [x, y, w, h] = ui.rect(button);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    assert!(ui.pointer_button(true), "the game's UI takes a press over it");
    ui.pointer_button(false);
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(score).as_deref(), Some("Clicked 1"));
    game.ui.frame(1.0 / 60.0, (800, 600), 1.0);
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(score).as_deref(), Some("Clicked 1"), "one click, heard once");
}
