//! The Inspector's Debug mode on a copy of Hello whose Spinner A has a
//! Tower with private, unsaved and hidden fields: switched from its menu,
//! each Inspector its own, showing the engine's state, every field of the
//! script, and while playing, the running instance's. A file of its own,
//! so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_editor::debug_inspector::DebugRow;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

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

/// The first label anywhere in `node` whose text starts with `words`.
fn label(ui: &Ui, node: NodeId, words: &str) -> Option<NodeId> {
    if ui.text(node).starts_with(words) && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label(ui, child, words))
}

/// Clicks the middle of the element with `class` whose text is `text`,
/// then gives the editor two frames to answer.
fn click_text(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, class: &str, text: &str) {
    let mut found = Vec::new();
    with_class(&editor.ui, editor.ui.root(), class, &mut found);
    let node = found
        .into_iter()
        .find_map(|node| label(&editor.ui, node, text).filter(|&l| editor.ui.text(l) == text))
        .unwrap_or_else(|| panic!("no {class} reads {text}"));
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
    frame(editor, renderer);
}

const TOWER: &str = r#"using System;
using Voxear;

namespace Hello;

public sealed class Tower : Behaviour
{
	public int Floors = 3;

	[NonSerialized] public int Built;

	private string secret = "kept";

	[HideInInspector] public float Hidden = 2.5f;

	protected override void Start()
	{
		Built = Floors * 10;
		secret = "changed while playing";
	}
}
"#;

/// The Debug row labelled `label` in `section`, as it shows now.
fn row<'a>(rows: &'a [DebugRow], section: &str, label: &str) -> Option<&'a DebugRow> {
    rows.iter().find(|row| row.section == section && row.label == label)
}

#[test]
fn debug_mode_shows_what_is_really_there_private_fields_engine_state_and_live_state() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Tower.cs"), TOWER).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let towered = "behaviours = [\"Hello.Spinner\", \"Hello.Tower\"]\n[obj.fields.\"Hello.Tower\"]\nfloors = 4";
    std::fs::write(&scene, text.replacen("behaviours = [\"Hello.Spinner\"]", towered, 1)).unwrap();
    let mut editor = open_described(&mut renderer, &dir);
    let world = &editor.game.engine.world;
    let spinner = world
        .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
        .into_iter()
        .find(|(_, n)| n.0 == "Spinner A")
        .unwrap()
        .0;
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);

    // Switched from the Inspector's ⋯ menu.
    let button = editor.inspector_mode_button().expect("the Inspector has a ⋯ button");
    let [x, y, w, h] = editor.ui.rect(button);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    click_text(&mut editor, &mut renderer, "menu-item", "Debug");
    assert!(editor.inspector_is_debug(0), "the first Inspector is in Debug mode");
    let read = |e: &mut voxear_editor::Editor| {
        let rows = e.debug_rows_shown();
        row(&rows, "Hello.Tower (C#)", "secret: String").is_some()
    };
    let started = std::time::Instant::now();
    while !read(&mut editor) {
        assert!(started.elapsed().as_secs() < 60, "the Debug rows never showed: {:?}", editor.debug_rows_shown());
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "inspector-debug.png");
    let rows = editor.debug_rows_shown();
    // The engine's own state.
    let entity = row(&rows, "Obj", "Entity").unwrap();
    assert!(entity.value.contains(&spinner.to_bits().to_string()), "{entity:?}");
    assert!(row(&rows, "Obj", "Scene path").unwrap().value.ends_with("Spinner A"));
    assert!(row(&rows, "World Transform", "Position").is_some());
    assert_eq!(row(&rows, "Behaviours", "Hello.Tower").unwrap().value, "not running");
    // Every field of a script, private and unsaved ones too, the attributes that hide them set aside.
    let secret = row(&rows, "Hello.Tower (C#)", "secret: String").unwrap();
    assert_eq!((secret.value.as_str(), secret.note.as_str()), ("\"kept\"", "private, not saved"));
    assert_eq!(row(&rows, "Hello.Tower (C#)", "Floors: Int32").unwrap().value, "4", "the scene's value");
    assert!(row(&rows, "Hello.Tower (C#)", "Built: Int32").unwrap().note.contains("not saved"));
    assert!(row(&rows, "Hello.Tower (C#)", "Hidden: Single").is_some(), "[HideInInspector] shows");
    assert!(
        rows.iter().any(|r| r.section == "Hello.Tower (C#)" && r.note.contains("from Behaviour")),
        "Behaviour's own"
    );
    // Every component as the scene file writes it.
    assert!(rows.iter().any(|r| r.section == "Scene File" && r.label == "name"), "{rows:?}");

    // Each Inspector its own: a second one opened stays Normal.
    let panel = editor.add_inspector("inspector");
    frame(&mut editor, &mut renderer);
    assert!(!editor.inspector_is_debug(1), "{panel} opens Normal");

    // While playing: the running instance, with what its code set.
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() || !editor.debug_rows_shown().iter().any(|r| r.value == "\"changed while playing\"") {
        assert!(
            started.elapsed().as_secs() < 120,
            "the running instance never showed: {:?}",
            editor.debug_rows_shown()
        );
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let rows = editor.debug_rows_shown();
    assert_eq!(row(&rows, "Hello.Tower (C#)", "Read from").unwrap().value, "running instance");
    assert_eq!(row(&rows, "Hello.Tower (C#)", "Built: Int32").unwrap().value, "40");
    assert!(row(&rows, "Behaviours", "Hello.Tower").unwrap().value.starts_with("handle "));
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);

    // Back to Normal from the menu.
    editor.set_inspector_debug(0, false);
    frame(&mut editor, &mut renderer);
    assert!(editor.debug_rows_shown().is_empty());
}

/// The editor as it looks, saved when `VOXEAR_TEST_OUTPUT` names a folder.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}
