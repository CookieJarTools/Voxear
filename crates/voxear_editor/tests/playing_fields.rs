//! A running behaviour in the Inspector while the game plays, on a copy of
//! Hello whose Spinner A has a Counter: its fields read from the instance as
//! they change, a value typed in given to the instance, with its
//! `[OnValueChanged]` callback, and its `[Button]` run on it. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const COUNTER: &str = r#"using Voxear;

namespace Hello;

public sealed class Counter : Behaviour
{
	[OnValueChanged(nameof(Changed))]
	public int Step = 1;

	public int Total;

	public int Changes;

	public int Ticks;

	protected override void Update()
	{
		Ticks++;
	}

	[Button]
	private void Add()
	{
		Total += Step;
	}

	private void Changed()
	{
		Changes++;
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

/// The editor on a copy of Hello, once its scripts are described.
fn open_described(renderer: &mut Renderer, dir: &tempfile::TempDir) -> voxear_editor::Editor {
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, renderer);
    editor
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

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

fn type_into(ui: &mut Ui, field: NodeId, text: &str) {
    ui.focus(field);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input(text);
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
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

/// What the Inspector's field on the row labelled `label` shows.
fn shows(editor: &voxear_editor::Editor, label: &str) -> String {
    let Some(row) = prop_row(&editor.ui, label) else { return String::new() };
    let mut inputs = Vec::new();
    with_class(&editor.ui, row, "value-input", &mut inputs);
    inputs.first().map(|&input| editor.ui.text(input)).unwrap_or_default()
}

#[test]
fn a_running_behaviours_fields_show_and_edit_while_playing() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Counter.cs"), COUNTER).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let counted = "behaviours = [\"Hello.Spinner\", \"Hello.Counter\"]\n[obj.fields.\"Hello.Counter\"]\nstep = 2";
    std::fs::write(&scene, text.replacen("behaviours = [\"Hello.Spinner\"]", counted, 1)).unwrap();
    let mut editor = open_described(&mut renderer, &dir);
    let spinner = |editor: &voxear_editor::Editor| {
        let world = &editor.game.engine.world;
        world
            .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
            .into_iter()
            .find(|(_, n)| n.0 == "Spinner A")
            .unwrap()
            .0
    };
    let edited = spinner(&editor);
    editor.select(Some(edited));
    editor.play(&mut renderer);
    until(&mut editor, &mut renderer, "playing", |e| e.is_playing() && prop_row(&e.ui, "Ticks").is_some());

    // The fields are read from the running instance as the game changes them.
    until(&mut editor, &mut renderer, "the ticks counting", |e| shows(e, "Ticks").parse::<u32>().unwrap_or(0) > 2);
    assert_eq!(shows(&editor, "Step"), "2", "the scene's value, as it started");

    // A value typed in goes to the instance, with its callback, which runs there.
    let mut inputs = Vec::new();
    with_class(&editor.ui, prop_row(&editor.ui, "Step").unwrap(), "value-input", &mut inputs);
    type_into(&mut editor.ui, inputs[0], "5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    until(&mut editor, &mut renderer, "the callback", |e| shows(e, "Changes") == "1");
    assert_eq!(shows(&editor, "Step"), "5");

    // A button runs on the running behaviour, with the value given to it.
    let mut buttons = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "attr-button", &mut buttons);
    let add = buttons.into_iter().find(|&b| editor.ui.text(b) == "Add").expect("the Add button");
    assert!(!editor.ui.has_class(add, "disabled"), "not waiting while the game plays");
    let [x, y, w, h] = editor.ui.rect(add);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    until(&mut editor, &mut renderer, "the button's addition", |e| shows(e, "Total") == "5");

    // Stopping leaves the scene as it was.
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner(&editor)).unwrap();
    let counter = kept.0.iter().find(|s| s.type_name == "Hello.Counter").unwrap();
    assert_eq!(counter.fields.get("step"), Some(&toml::Value::Integer(2)), "what play changed goes with it");
}
