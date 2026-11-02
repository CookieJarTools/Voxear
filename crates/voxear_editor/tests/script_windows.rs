//! Windows written in the game's C#, on a copy of Hello with a Level Tools
//! window: opened twice from a window's menu, each its own instance with its
//! own clicks, updated while it shows, and closed for good. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const LEVEL_TOOLS: &str = r#"using Voxear;

namespace Hello;

[Window("Level Tools")]
public sealed class LevelTools : EditorWindow
{
	private Label count = null!;
	private Label turn = null!;
	private int clicks;

	protected override void Build(UiElement root)
	{
		count = root.Add(new Label());
		turn = root.Add(new Label());
		root.Add(new Button("Count")).Clicked += () => clicks++;
	}

	protected override void Update(UiDocument ui)
	{
		var spinner = Obj.Find("Spinner A");
		count.Text = $"{spinner.Name}: {clicks} clicks";
		turn.Text = $"Turned {spinner.Rotation.Y:F4}";
	}
}
"#;

const SIZE: (u32, u32) = (1280, 800);

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

/// A renderer, or why the test cannot run.
fn renderer() -> Option<Renderer> {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return None;
    }
    let renderer = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok();
    if renderer.is_none() {
        eprintln!("skipping: no usable GPU");
    }
    renderer
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Frames until `done`, which must come within a minute.
fn until(
    editor: &mut voxear_editor::Editor,
    renderer: &mut Renderer,
    what: &str,
    done: impl Fn(&voxear_editor::Editor) -> bool,
) {
    let started = std::time::Instant::now();
    while !done(editor) {
        assert!(started.elapsed().as_secs() < 60, "{what} never happened");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// The first element in `node`, or `node` itself, whose own text is `text`.
fn reading(ui: &Ui, node: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(node) == text && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| reading(ui, child, text))
}

/// Shown when it and everything it is in are.
fn shown(ui: &Ui, node: NodeId) -> bool {
    std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n))
}

/// The element showing `text`, where it can be seen.
fn showing(ui: &Ui, text: &str) -> Option<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if ui.text(node) == text && ui.children(node).is_empty() && shown(ui, node) {
            found.push(node);
        }
        stack.extend(ui.children(node));
    }
    found.pop()
}

#[test]
fn the_games_own_windows_open_more_than_once_each_its_own() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/LevelTools.cs"), LEVEL_TOOLS).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.script_window_types(), [("Hello.LevelTools".to_string(), "Level Tools".to_string())]);

    // Opened twice, beside the Inspector, each panel titled as its [Window] says.
    let first = editor.add_script_window("Hello.LevelTools", "inspector").expect("the window opens");
    let second = editor.add_script_window("Hello.LevelTools", "inspector").expect("and again");
    assert_ne!(first, second);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let titles: Vec<String> = editor.ui.dock_panels(dock).into_iter().map(|(_, title, _)| title).collect();
    assert_eq!(titles.iter().filter(|t| *t == "Level Tools").count(), 2, "{titles:?}");

    // The one showing builds and updates, on the level being edited, without playing.
    editor.ui.show_dock_panel(dock, &first);
    until(&mut editor, &mut renderer, "the first window updating", |e| showing(&e.ui, "Spinner A: 0 clicks").is_some());
    assert!(!editor.is_playing());

    // Its button counts its own clicks.
    let count = showing(&editor.ui, "Count").expect("its button");
    let [x, y, w, h] = editor.ui.rect(count);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    until(&mut editor, &mut renderer, "the click counted", |e| showing(&e.ui, "Spinner A: 1 clicks").is_some());

    // The other is an instance of its own, with none.
    editor.ui.show_dock_panel(dock, &second);
    until(&mut editor, &mut renderer, "the second window updating", |e| {
        showing(&e.ui, "Spinner A: 0 clicks").is_some() && showing(&e.ui, "Spinner A: 1 clicks").is_none()
    });

    // Closed, it goes for good, and the other stays.
    assert!(editor.remove_script_window(&second));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.script_window_count(), 1);
    editor.ui.show_dock_panel(dock, &first);
    until(&mut editor, &mut renderer, "the first again", |e| showing(&e.ui, "Spinner A: 1 clicks").is_some());
    assert!(reading(&editor.ui, editor.ui.root(), "Level Tools").is_some(), "its tab");

    // While the game plays, it keeps updating, as the same window, on the level that plays: Spinner A turns.
    let turned = |e: &voxear_editor::Editor| {
        let mut stack = vec![e.ui.root()];
        let mut found = None;
        while let Some(node) = stack.pop() {
            let text = e.ui.text(node);
            if text.starts_with("Turned ") && shown(&e.ui, node) {
                found = Some(text);
            }
            stack.extend(e.ui.children(node));
        }
        found.unwrap_or_default()
    };
    let still = turned(&editor);
    assert!(!still.is_empty(), "it says how Spinner A is turned");
    editor.select(None);
    editor.play(&mut renderer);
    until(&mut editor, &mut renderer, "Spinner A turning in the window", |e| e.is_playing() && turned(e) != still);
    assert!(showing(&editor.ui, "Spinner A: 1 clicks").is_some(), "the same instance");
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(showing(&editor.ui, "Spinner A: 1 clicks").is_some(), "and after");

    // A layout keeps the window it shows: left out by another, it goes, and comes back with the layout.
    let kept = std::time::Instant::now();
    while kept.elapsed().as_millis() < 1200 {
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.script_window_count(), 0, "Profiling does not show it");
    assert!(editor.use_layout("Default"));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.script_window_count(), 1, "Default opens it again");
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.show_dock_panel(dock, &first);
    until(&mut editor, &mut renderer, "the window again", |e| showing(&e.ui, "Spinner A: 0 clicks").is_some());
}
