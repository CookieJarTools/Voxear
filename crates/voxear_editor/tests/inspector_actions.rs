//! A behaviour's code, run for the Inspector while editing, on a copy of
//! Hello whose Spinner A has a Tower: a `[Button]`, an `[OnValueChanged]`
//! callback, a `[ValidateInput]` check and a `[ValueDropdown]`, all without
//! playing. A file of its own, so it runs in a process of its own, since
//! .NET starts only once per process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const TOWER: &str = r#"using System.Collections.Generic;
using Voxear;

namespace Hello
{
	public sealed class Tower : Behaviour
	{
		[OnValueChanged(nameof(Rebuild))]
		public int Floors = 3;

		public float Height;

		[ValidateInput(nameof(IsColour), "Pick a colour the tower has")]
		[ValueDropdown(nameof(Colours))]
		public string Paint = "Red";

		[Button("Rebuild the tower")]
		private void Rebuild()
		{
			Height = Floors * 2.5f;
		}

		private bool IsColour(string paint)
		{
			return paint == "Red" || paint == "Blue";
		}

		private IEnumerable<ValueDropdownItem> Colours()
		{
			yield return new ValueDropdownItem("Brick red", "Red");
			yield return new ValueDropdownItem("Sky blue", "Blue");
		}
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

#[test]
fn buttons_callbacks_checks_and_dropdowns_run_the_behaviours_code_while_editing() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Tower.cs"), TOWER).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let towered = "behaviours = [\"Hello.Spinner\", \"Hello.Tower\"]\n[obj.fields.\"Hello.Tower\"]\nfloors = 4\npaint = \"Green\"";
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
    let field = |editor: &voxear_editor::Editor, name: &str| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        let tower = kept.0.iter().find(|s| s.type_name == "Hello.Tower").unwrap();
        tower.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
    };

    // [ValidateInput] starts the scripts without playing, and says the scene's paint is wrong.
    until(&mut editor, &mut renderer, "the check's error", |e| {
        label(&e.ui, e.ui.root(), "Pick a colour the tower has").is_some()
    });
    assert!(!editor.is_playing(), "nothing plays");
    shot(&mut renderer, "inspector-actions.png");

    // [ValueDropdown] offers the behaviour's colours by name; picking one sets it, and the error goes.
    let has_dropdown = |e: &voxear_editor::Editor| {
        let mut found = Vec::new();
        with_class(&e.ui, prop_row(&e.ui, "Paint").unwrap(), "attr-dropdown", &mut found);
        !found.is_empty()
    };
    until(&mut editor, &mut renderer, "the dropdown", has_dropdown);
    let mut dropdowns = Vec::new();
    with_class(&editor.ui, prop_row(&editor.ui, "Paint").unwrap(), "attr-dropdown", &mut dropdowns);
    assert_eq!(dropdowns.len(), 1, "Paint is a dropdown");
    let [x, y, w, h] = editor.ui.rect(dropdowns[0]);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    click_text(&mut editor, &mut renderer, "dropdown-option", "Sky blue");
    assert_eq!(field(&editor, "paint"), Some(toml::Value::String("Blue".into())));
    until(&mut editor, &mut renderer, "the error going", |e| {
        label(&e.ui, e.ui.root(), "Pick a colour the tower has").is_none_or(|l| !shown(&e.ui, l))
    });

    // [Button] runs on the scene's values; what it changes is a step to undo.
    click_text(&mut editor, &mut renderer, "attr-button", "Rebuild the tower");
    assert_eq!(field(&editor, "height").and_then(|v| v.as_float()), Some(10.0), "four floors");
    editor.undo(&mut renderer);
    assert_eq!(field(&editor, "height"), None, "undone");

    // [OnValueChanged] runs when Floors is changed, with the change.
    frame(&mut editor, &mut renderer);
    let mut inputs = Vec::new();
    with_class(&editor.ui, prop_row(&editor.ui, "Floors").unwrap(), "value-input", &mut inputs);
    type_into(&mut editor.ui, inputs[0], "6");
    frame(&mut editor, &mut renderer);
    assert_eq!(field(&editor, "floors"), Some(toml::Value::Integer(6)));
    assert_eq!(field(&editor, "height").and_then(|v| v.as_float()), Some(15.0), "the callback ran with it");
}

/// Shown when it and everything it is in are.
fn shown(ui: &Ui, node: NodeId) -> bool {
    std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n))
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
