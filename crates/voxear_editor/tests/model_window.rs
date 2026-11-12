//! The Model window: a box model opened from the Project shows its bones
//! and boxes in a tree and stands in a view; the bar adds a box, a click in
//! the view picks the box under it, a handle moves it a whole pixel at a
//! time, the Inspector's rows change it, each a step to undo, and the file
//! is saved once nothing is held. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_editor::model_view::{Handle, Picked};
use voxear_engine::Project;
use voxear_render::{BoxModel, Renderer, RendererSettings};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);
const PATH: &str = "Models/Figure.boxmodel";

/// A body with a head on it.
fn figure() -> BoxModel {
    let mut model = BoxModel::default();
    let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
    model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
    let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
    model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
    model
}

/// A copy of Hello, inside the repository, without its scripts, which the
/// editor would otherwise build, and with the figure among its models.
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
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> = project.lines().filter(|line| !line.starts_with("scripts")).collect();
    std::fs::write(&file, kept.join("\n")).unwrap();
    std::fs::write(dir.path().join(PATH), figure().to_text()).unwrap();
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn frames(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, count: usize) {
    (0..count).for_each(|_| frame(editor, renderer));
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

fn texts_in(ui: &Ui, node: NodeId) -> Vec<String> {
    let mut out = vec![ui.text(node)];
    for child in ui.children(node) {
        out.extend(texts_in(ui, child));
    }
    out.retain(|text| !text.is_empty());
    out
}

/// The Model window's button that says `text`.
fn button(ui: &Ui, text: &str) -> NodeId {
    let panel = ui.find(ui.root(), "model-panel").unwrap();
    let mut buttons = Vec::new();
    with_class(ui, panel, "console-button", &mut buttons);
    buttons.into_iter().find(|&button| ui.text(button) == text).unwrap_or_else(|| panic!("no {text} button"))
}

/// Clicks the Model window's button that says `text`.
fn press(ui: &mut Ui, text: &str) {
    let button = button(ui, text);
    click(ui, button);
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    click_at(ui, (x + w / 2.0, y + h / 2.0));
}

fn click_at(ui: &mut Ui, (x, y): (f32, f32)) {
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

fn type_into(ui: &mut Ui, field: NodeId, text: &str) {
    ui.focus(field);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input(text);
    ui.key_down(Key::Enter);
}

/// The model as its file has it.
fn saved(dir: &std::path::Path) -> BoxModel {
    BoxModel::parse(&std::fs::read_to_string(dir.join(PATH)).unwrap()).unwrap()
}

#[test]
fn a_box_model_is_opened_built_on_picked_moved_and_undone_in_the_model_window() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let project = hello_copy();
    let dir = project.path().to_path_buf();
    let mut editor =
        voxear_editor::Editor::open_without_memory(Project::open(&dir).unwrap(), &mut renderer, LogCapture::default())
            .unwrap();
    frames(&mut editor, &mut renderer, 2);

    // Opened as a double click on it in the Project does: its window shows, with its bones and boxes in the tree.
    assert!(editor.open_path(PATH, &mut renderer));
    frames(&mut editor, &mut renderer, 4);
    assert_eq!(editor.model_window().path(), Some(PATH));
    assert_eq!(editor.model_window().model(), Some(&figure()));
    let panel = editor.ui.find(editor.ui.root(), "model-panel").unwrap();
    assert!(editor.ui.is_visible(panel));
    let tree = editor.model_window().tree();
    let rows = texts_in(&editor.ui, tree);
    assert_eq!(rows, ["Body", "Box 1", "Head", "Box 1"], "every bone open, with its boxes");
    assert!(texts_in(&editor.ui, panel).contains(&"2 bones · 2 boxes".to_string()));
    assert!(!editor.ui.is_enabled(button(&editor.ui, "Delete")), "nothing is picked yet");

    // A click on the head in the view picks its box, which the tree lights and the Inspector shows.
    let head_front = editor.model_window().view().point(&editor.ui, Vec3::new(0.0, 1.75, 0.25)).expect("in view");
    click_at(&mut editor.ui, head_front);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), Some(Picked::Box(1, 0)));
    assert_eq!(editor.ui.selected_items(tree).len(), 1);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let shown = texts_in(&editor.ui, inspector);
    assert!(shown.contains(&"Head ▸ Box 1".to_string()), "{shown:#?}");
    for label in ["Position", "Size", "Rotation", "Sheet", "Mirror", "Inflate"] {
        assert!(shown.contains(&label.to_string()), "no {label} row: {shown:#?}");
    }

    // Its Y handle dragged up moves it up whole pixels, as one step, saved once let go.
    let model = editor.model_window().model().unwrap().clone();
    let handle = editor.model_window().view().handle_point(&editor.ui, &model, Picked::Box(1, 0), Handle::Move(1));
    let (x, y) = handle.expect("the handle is in view");
    editor.ui.pointer_move(x, y);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=6 {
        editor.ui.pointer_move(x, y - step as f32 * 10.0);
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(saved(&dir), figure(), "nothing is saved while the handle is held");
    editor.ui.pointer_button(false);
    frames(&mut editor, &mut renderer, 3);
    let moved = editor.model_window().model().unwrap().bones[1].boxes[0].from;
    assert!(moved.y >= 2.0 && moved.y.fract() == 0.0, "up by whole pixels: {moved}");
    assert_eq!((moved.x, moved.z), (-4.0, -4.0), "along its own axis alone");
    assert_eq!(saved(&dir).bones[1].boxes[0].from, moved, "saved once let go");
    assert!(editor.model_window_mut().undo());
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().model(), Some(&figure()), "the whole drag is one step");
    assert_eq!(saved(&dir), figure());
    assert!(editor.model_window_mut().redo());
    assert!(editor.model_window_mut().undo());

    // A size typed in the Inspector sizes the box; a step of its own.
    let mut fields = Vec::new();
    with_class(&editor.ui, inspector, "axis-input", &mut fields);
    // Position's three, then Size's.
    type_into(&mut editor.ui, fields[3], "10");
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().model().unwrap().bones[1].boxes[0].size, [10, 8, 8]);
    assert!(editor.model_window_mut().undo());
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().model(), Some(&figure()));

    // Add Box puts one on the picked box's bone, with room of its own on the sheet, and picks it.
    press(&mut editor.ui, "Add Box");
    frames(&mut editor, &mut renderer, 3);
    let now = editor.model_window().model().unwrap().clone();
    assert_eq!(now.bones[1].boxes.len(), 2);
    assert_eq!(editor.model_window().picked(), Some(Picked::Box(1, 1)));
    assert_eq!(now.bones[1].boxes[1].unfolded_size(), [16, 8]);
    assert_eq!(texts_in(&editor.ui, tree), ["Body", "Box 1", "Head", "Box 1", "Box 2"]);
    assert_eq!(saved(&dir), now);

    // Mirror copies it across the body; Delete takes the copy out again.
    press(&mut editor.ui, "Mirror");
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), Some(Picked::Box(1, 2)));
    press(&mut editor.ui, "Delete");
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().model(), Some(&now));
    assert_eq!(editor.model_window().picked(), None);

    // Add Bone with the head picked hangs a bone from it; its name typed in the Inspector names it.
    editor.model_window_mut().pick(Some(Picked::Bone(1)));
    frames(&mut editor, &mut renderer, 2);
    press(&mut editor.ui, "Add Bone");
    frames(&mut editor, &mut renderer, 3);
    let made = editor.model_window().picked().expect("the new bone is picked");
    let now = editor.model_window().model().unwrap().clone();
    assert_eq!((made, now.bones[2].parent, now.bones[2].name.as_str()), (Picked::Bone(2), Some(1), "Bone"));
    let mut names = Vec::new();
    with_class(&editor.ui, inspector, "value-input", &mut names);
    type_into(&mut editor.ui, names[0], "Hat");
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().model().unwrap().bones[2].name, "Hat");
    assert!(texts_in(&editor.ui, tree).contains(&"Hat".to_string()));
    assert_eq!(saved(&dir).bones[2].name, "Hat");

    // A click on the floor, off the model, picks nothing, and the Inspector shows the model itself.
    let floor = editor.model_window().view().point(&editor.ui, Vec3::new(1.0, 0.0, 0.2)).expect("in view");
    let [left, top, width, height] = editor.ui.rect(editor.model_window().view().frame_node());
    assert!((left..left + width).contains(&floor.0) && (top..top + height).contains(&floor.1), "{floor:?}");
    click_at(&mut editor.ui, floor);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), None);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some(PATH));
}
