//! A behaviour drawn whole by a `BehaviourInspector` written in C#, on a
//! copy of Hello whose Spinner A has a Tower: the inspector's own elements,
//! with the Tower's usual fields in its "next" element, a button that
//! changes two fields as one step to undo, and the usual fields alone while
//! playing. A file of its own, so it runs in a process of its own, since
//! .NET starts only once per process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const TOWER: &str = r#"using Voxear;

namespace Hello;

public sealed class Tower : Behaviour
{
	public int Floors = 3;

	public float Height;
}
"#;

const TOWER_INSPECTOR: &str = r#"using Voxear;

namespace Hello;

public sealed class TowerInspector : BehaviourInspector<Tower>
{
	public override string Markup => """
		<vui>
			<Label name="summary" />
			<Button name="raise">Add a floor</Button>
			<Panel name="next" />
		</vui>
		""";

	protected override void Draw(Tower tower, UiDocument ui)
	{
		if (ui.Find("raise")!.WasClicked)
		{
			tower.Floors++;
			tower.Height = tower.Floors * 2.5f;
		}
		ui.Find("summary")!.Text = $"{tower.Floors} floors on {tower.Obj.Name}";
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

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The first element in `node`, or `node` itself, whose own text is `text`.
fn reading(ui: &Ui, node: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(node) == text && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| reading(ui, child, text))
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
}

/// Shown when it and everything it is in are.
fn shown(ui: &Ui, node: NodeId) -> bool {
    std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n))
}

#[test]
fn a_behaviour_inspector_draws_the_whole_behaviour_and_changes_it() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Tower.cs"), TOWER).unwrap();
    std::fs::write(dir.path().join("Scripts/TowerInspector.cs"), TOWER_INSPECTOR).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let towered = "behaviours = [\"Hello.Spinner\", \"Hello.Tower\"]\n[obj.fields.\"Hello.Tower\"]\nfloors = 4";
    std::fs::write(&scene, text.replacen("behaviours = [\"Hello.Spinner\"]", towered, 1)).unwrap();

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let world = &editor.game.engine.world;
    let spinner = world
        .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
        .into_iter()
        .find(|(_, n)| n.0 == "Spinner A")
        .unwrap()
        .0;
    editor.select(Some(spinner));
    let field = |editor: &voxear_editor::Editor, name: &str| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        let tower = kept.0.iter().find(|s| s.type_name == "Hello.Tower").unwrap();
        tower.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
    };
    let says = |editor: &voxear_editor::Editor, text: &str| reading(&editor.ui, editor.ui.root(), text).is_some();

    // The inspector draws the Tower, on its Obj, with the scene's values, and the usual fields inside it.
    until(&mut editor, &mut renderer, "the inspector drawing", |e| says(e, "4 floors on Spinner A"));
    assert!(!editor.is_playing(), "nothing plays");
    let mut drawn = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "field-drawer", &mut drawn);
    let inspector = *drawn.iter().find(|&&d| reading(&editor.ui, d, "Add a floor").is_some()).expect("its elements");
    let floors = prop_row(&editor.ui, "Floors").expect("the usual Floors field");
    assert!(editor.ui.is_within(floors, inspector) && shown(&editor.ui, floors), "in the inspector's next");

    // Its button changes two fields, saved in the scene as one step to undo.
    let raise = reading(&editor.ui, inspector, "Add a floor").unwrap();
    let [x, y, w, h] = editor.ui.rect(raise);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    until(&mut editor, &mut renderer, "the floor added", |e| says(e, "5 floors on Spinner A"));
    assert_eq!(field(&editor, "floors"), Some(toml::Value::Integer(5)));
    assert_eq!(field(&editor, "height").and_then(|v| v.as_float()), Some(12.5));
    editor.undo(&mut renderer);
    assert_eq!(field(&editor, "floors"), Some(toml::Value::Integer(4)), "undone together");
    assert_eq!(field(&editor, "height"), None);
    until(&mut editor, &mut renderer, "the inspector drawing the undo", |e| says(e, "4 floors on Spinner A"));

    // While playing, the Tower shows its usual fields alone.
    editor.play(&mut renderer);
    until(&mut editor, &mut renderer, "playing", |e| e.is_playing() && prop_row(&e.ui, "Floors").is_some());
    assert!(!says(&editor, "Add a floor"), "no inspector while playing");
    editor.stop(&mut renderer);
}
