//! A behaviour's field of an engine component, on a copy of Hello whose
//! Spinner A has a Radio with a Speaker: an `AudioSource`, which takes an
//! Obj that has an Audio Source. A file of its own, so it runs in a process
//! of its own, since .NET starts only once per process. Skips itself without
//! .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const RADIO: &str = r#"using Voxear;

namespace Hello;

public sealed class Radio : Behaviour
{
	public AudioSource Speaker;

	public string Heard = "";

	protected override void Start()
	{
		Heard = Speaker.IsValid ? Speaker.Obj.Name : "nothing";
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

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The first element with `class` inside `node`.
fn first_with(ui: &Ui, node: NodeId, class: &str) -> Option<NodeId> {
    let mut found = Vec::new();
    with_class(ui, node, class, &mut found);
    found.first().copied()
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
}

/// The labels of the object picker's rows, top to bottom.
fn picker_rows(ui: &Ui) -> Vec<String> {
    let Some(list) = ui.find(ui.root(), "picker-list") else { return Vec::new() };
    let mut rows: Vec<NodeId> =
        ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows.into_iter().map(|row| ui.text(ui.children(row)[1])).collect()
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

#[test]
fn a_component_field_takes_an_obj_that_has_the_component() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Radio.cs"), RADIO).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    // Spinner A's Radio plays through the Lamp, which has an Audio Source.
    let radio =
        "behaviours = [\"Hello.Spinner\", \"Hello.Radio\"]\n[obj.fields.\"Hello.Radio\"]\nspeaker = { obj = 5 }";
    let text = text.replacen("behaviours = [\"Hello.Spinner\"]", radio, 1);
    std::fs::write(&scene, text.replacen("name = \"Lamp\"\n", "name = \"Lamp\"\naudio = {}\n", 1)).unwrap();

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
    let named = |name: &str| {
        world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let spinner = named("Spinner A");
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // The field shows the Obj it plays through.
    let row = prop_row(&editor.ui, "Speaker").expect("the Speaker field");
    let picker = first_with(&editor.ui, row, "value-picker").expect("a picker, as any reference has");
    assert_eq!(editor.ui.text(first_with(&editor.ui, picker, "picker-label").unwrap()), "Lamp");

    // Only Objs with an Audio Source are offered.
    let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(picker_rows(&editor.ui), ["None", "Lamp"], "nothing without an Audio Source");

    // Picking None clears it, as a step to undo.
    let mut rows = Vec::new();
    with_class(&editor.ui, editor.ui.find(editor.ui.root(), "picker-list").unwrap(), "list-row", &mut rows);
    let none = rows.into_iter().find(|&r| editor.ui.text(editor.ui.children(r)[1]) == "None").unwrap();
    click(&mut editor.ui, none);
    frame(&mut editor, &mut renderer);
    let speaker = |editor: &voxear_editor::Editor| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        let radio = kept.0.iter().find(|s| s.type_name == "Hello.Radio").unwrap();
        radio.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case("speaker")).map(|(_, v)| v.to_string())
    };
    assert!(speaker(&editor).is_some_and(|v| !v.contains("entity") && !v.contains("obj")), "{:?}", speaker(&editor));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let row = prop_row(&editor.ui, "Speaker").unwrap();
    let label = first_with(&editor.ui, row, "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Lamp", "back after undo");

    // Played, the script holds the Lamp's Audio Source.
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while shows(&editor, "Heard") != "Lamp" {
        assert!(started.elapsed().as_secs() < 60, "the Radio never heard the Lamp: {:?}", shows(&editor, "Heard"));
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    editor.stop(&mut renderer);
}

/// What the Inspector's field on the row labelled `label` shows.
fn shows(editor: &voxear_editor::Editor, label: &str) -> String {
    let Some(row) = prop_row(&editor.ui, label) else { return String::new() };
    first_with(&editor.ui, row, "value-input").map(|input| editor.ui.text(input)).unwrap_or_default()
}
