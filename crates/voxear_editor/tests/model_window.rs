//! The Model window: a box model opened from the Project shows its bones
//! and boxes in a tree and stands in a view; the bar adds a box, a click in
//! the view picks the box under it, a handle moves it a whole pixel at a
//! time, the Inspector's rows change it, each a step to undo, and the file
//! is saved once nothing is held. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_editor::model_panel::{Mode, Pace, Tool};
use voxear_editor::model_view::{Handle, Picked};
use voxear_engine::Project;
use voxear_render::{BoneRole, BoxModel, BoxSkin, Face, Renderer, RendererSettings};
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

    // A click off the model picks nothing, and the Inspector shows the model itself.
    // The view's corner, where nothing of the model is.
    let [left, top, ..] = editor.ui.rect(editor.model_window().view().frame_node());
    let floor = (left + 6.0, top + 6.0);
    click_at(&mut editor.ui, floor);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), None);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some(PATH));
}

/// Keeps the window as drawn, where `VOXEAR_TEST_OUTPUT` names a folder to keep pictures in.
fn keep_picture(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    image::save_buffer(std::path::Path::new(&dir).join(name), &pixels, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
}

/// Where a pixel of the head's front is on the model: its middle, in the model's space. The front's
/// pixels start at 8, 8 on the sheet, its first at the head's upper corner on the model's right.
fn on_head_front([x, y]: [u32; 2]) -> Vec3 {
    Vec3::new(0.25 - (x as f32 - 8.0 + 0.5) / 16.0, 2.0 - (y as f32 - 8.0 + 0.5) / 16.0, 0.25)
}

#[test]
fn a_box_model_is_painted_on_the_model_and_on_its_sheet_and_laid_out_again() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let project = hello_copy();
    let dir = project.path().to_path_buf();
    // The figure with every face a plain grey, to see it by.
    let mut grey = figure();
    for place in grey.used_places() {
        grey.sheet.fill([place[0], place[1]], [place[2], place[3]], [150, 150, 150, 255]);
    }
    std::fs::write(dir.join(PATH), grey.to_text()).unwrap();
    let mut editor =
        voxear_editor::Editor::open_without_memory(Project::open(&dir).unwrap(), &mut renderer, LogCapture::default())
            .unwrap();
    frames(&mut editor, &mut renderer, 2);
    assert!(editor.open_path(PATH, &mut renderer));
    frames(&mut editor, &mut renderer, 3);
    let sheet_frame = editor.model_window().sheet_view().frame_node();
    assert!(!editor.ui.is_visible(sheet_frame), "the sheet shows only while painting");

    editor.set_model_mode(Mode::Paint);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().mode(), Mode::Paint);
    assert!(editor.ui.is_visible(sheet_frame));
    let pixel =
        |editor: &voxear_editor::Editor, [x, y]: [u32; 2]| editor.model_window().model().unwrap().sheet.get(x, y);
    let point = |editor: &voxear_editor::Editor, at: Vec3| editor.model_window().view().point(&editor.ui, at).unwrap();
    let (red, blue) = ([220, 40, 40, 255], [40, 60, 220, 255]);

    // The pencil, pressed on the head's front: the pixel under it takes the colour, and nothing is picked.
    editor.model_window_mut().set_color(red);
    let at = point(&editor, on_head_front([8, 8]));
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(pixel(&editor, [8, 8]), red);
    assert_eq!(pixel(&editor, [9, 8]), [150, 150, 150, 255], "and no other");
    assert_eq!(editor.model_window().picked(), None, "a press paints, it does not pick");
    assert_eq!(saved(&dir).sheet.get(8, 8), red, "saved once let go");

    // Dragged across the face, it paints every pixel it passes over, however far it moves in a frame: one step.
    let (from, to) = (point(&editor, on_head_front([8, 10])), point(&editor, on_head_front([15, 10])));
    editor.ui.pointer_move(from.0, from.1);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to.0, to.1);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frames(&mut editor, &mut renderer, 3);
    for x in 8..=15 {
        assert_eq!(pixel(&editor, [x, 10]), red, "pixel {x} of the row");
    }
    assert!(editor.model_window_mut().undo());
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(pixel(&editor, [12, 10]), [150, 150, 150, 255], "the whole stroke goes");
    assert_eq!(pixel(&editor, [8, 8]), red, "the press before it stays");

    // With Symmetry, a press on one side of the face paints the other side too.
    editor.set_model_symmetry(true);
    editor.model_window_mut().set_color(blue);
    let at = point(&editor, on_head_front([9, 13]));
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!((pixel(&editor, [9, 13]), pixel(&editor, [14, 13])), (blue, blue));
    editor.set_model_symmetry(false);

    // Fill colours the face pressed, as far as pixels of its colour reach, and no other face.
    editor.set_model_tool(Tool::Fill);
    let body_front = point(&editor, Vec3::new(0.0, 1.0, 0.125));
    click_at(&mut editor.ui, body_front);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(
        (pixel(&editor, [20, 20]), pixel(&editor, [27, 31])),
        (blue, blue),
        "the body's front, corner to corner"
    );
    assert_eq!(pixel(&editor, [17, 25]), [150, 150, 150, 255], "its side is as it was");

    // Pick takes the colour under the press to paint with.
    editor.set_model_tool(Tool::Pick);
    let at = point(&editor, on_head_front([8, 8]));
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().color(), red);

    // On the sheet itself: the eraser makes a hole of the pixel pressed, and the pencil paints along a drag.
    editor.set_model_tool(Tool::Eraser);
    let at = editor.model_window().sheet_view().pixel_point(&editor.ui, [8, 8]);
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(pixel(&editor, [8, 8]), [0, 0, 0, 0]);
    editor.set_model_tool(Tool::Pencil);
    let (from, to) = (
        editor.model_window().sheet_view().pixel_point(&editor.ui, [40, 40]),
        editor.model_window().sheet_view().pixel_point(&editor.ui, [40, 46]),
    );
    editor.ui.pointer_move(from.0, from.1);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to.0, to.1);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frames(&mut editor, &mut renderer, 3);
    assert!((40..=46).all(|y| pixel(&editor, [40, y]) == red), "down the sheet, off any face");
    keep_picture(&mut renderer, "model window painting.png");

    // Lay Out gives the body room beside the head, taking its pixels with it.
    press(&mut editor.ui, "Lay Out");
    frames(&mut editor, &mut renderer, 3);
    let model = editor.model_window().model().unwrap().clone();
    assert_eq!(model.bones[0].boxes[0].skin, BoxSkin::Unfolded { at: [32, 0], mirror: false });
    let [left, top, ..] = model.bones[0].boxes[0].face_place(Face::Front).unwrap();
    assert_eq!(model.sheet.get(left, top), blue, "the body's front is still blue, where it now is");
    assert_eq!(saved(&dir), model);
    assert!(editor.model_window_mut().undo());
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(pixel(&editor, [20, 20]), blue, "back where it was");

    // Back to shaping, the sheet goes and a click picks again.
    editor.set_model_mode(Mode::Shape);
    frames(&mut editor, &mut renderer, 2);
    assert!(!editor.ui.is_visible(sheet_frame));
    let at = point(&editor, on_head_front([12, 12]));
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), Some(Picked::Box(1, 0)));
}

#[test]
fn bones_are_made_from_a_models_shape_and_a_leg_is_cut_for_a_knee() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let project = hello_copy();
    let dir = project.path().to_path_buf();
    // A figure built on one bone: a body, a head, two arms and two legs.
    let mut heap = BoxModel::default();
    let all = heap.add_bone("All", None, Vec3::ZERO);
    let boxes: [([f32; 3], [u32; 3]); 6] = [
        ([-4.0, 12.0, -2.0], [8, 12, 4]),
        ([-4.0, 24.0, -4.0], [8, 8, 8]),
        ([4.0, 12.0, -2.0], [4, 12, 4]),
        ([-8.0, 12.0, -2.0], [4, 12, 4]),
        ([0.0, 0.0, -2.0], [4, 12, 4]),
        ([-4.0, 0.0, -2.0], [4, 12, 4]),
    ];
    for (from, size) in boxes {
        heap.add_box_with_room(all, size);
        heap.bones[all].boxes.last_mut().unwrap().from = Vec3::from(from);
    }
    std::fs::write(dir.join(PATH), heap.to_text()).unwrap();
    let mut editor =
        voxear_editor::Editor::open_without_memory(Project::open(&dir).unwrap(), &mut renderer, LogCapture::default())
            .unwrap();
    frames(&mut editor, &mut renderer, 2);
    assert!(editor.open_path(PATH, &mut renderer));
    frames(&mut editor, &mut renderer, 3);
    assert!(!editor.ui.is_enabled(button(&editor.ui, "Add Joint")), "with no box picked there is nothing to cut");

    // Make Bones: a bone for each part, named and given its role, nothing moved; one step to undo.
    let stood = heap.bounds();
    press(&mut editor.ui, "Make Bones");
    frames(&mut editor, &mut renderer, 3);
    let model = editor.model_window().model().unwrap().clone();
    let names: Vec<&str> = model.bones.iter().map(|bone| bone.name.as_str()).collect();
    assert_eq!(names, ["Body", "Head", "RightArm", "LeftArm", "RightLeg", "LeftLeg"]);
    assert_eq!(model.bounds(), stood);
    let tree = editor.model_window().tree();
    assert!(texts_in(&editor.ui, tree).contains(&"LeftLeg".to_string()));
    assert_eq!(saved(&dir), model);

    // A bone's role shows in the Inspector, and another picked there is kept.
    let leg = model.find_bone("LeftLeg").unwrap();
    editor.model_window_mut().pick(Some(Picked::Bone(leg)));
    frames(&mut editor, &mut renderer, 2);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let shown = texts_in(&editor.ui, inspector);
    assert!(shown.contains(&"Role".to_string()) && shown.contains(&"Leg".to_string()), "{shown:#?}");

    // Add Joint on the leg's box: its lower half hangs from a new bone at the knee, which is picked.
    editor.model_window_mut().pick(Some(Picked::Box(leg, 0)));
    frames(&mut editor, &mut renderer, 2);
    press(&mut editor.ui, "Add Joint");
    frames(&mut editor, &mut renderer, 3);
    let model = editor.model_window().model().unwrap().clone();
    let knee = model.find_bone("LeftLegLower").expect("a bone for the lower leg");
    assert_eq!(editor.model_window().picked(), Some(Picked::Bone(knee)));
    assert_eq!((model.bones[knee].parent, model.bones[knee].role), (Some(leg), BoneRole::Leg));
    assert_eq!(model.bones[leg].boxes[0].size, [4, 6, 4]);
    assert_eq!(model.bounds(), stood, "the leg stands where it stood");
    assert_eq!(saved(&dir), model);

    // Both go back, a step each.
    assert!(editor.model_window_mut().undo());
    assert!(editor.model_window_mut().undo());
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().model(), Some(&heap));
}

#[test]
fn the_model_window_shows_a_model_walking_and_puts_it_back_at_rest() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let project = hello_copy();
    let dir = project.path().to_path_buf();
    // The figure with two legs under it, every bone saying what it is.
    let mut walker = figure();
    for (name, x) in [("RightLeg", 2.0), ("LeftLeg", -2.0)] {
        let leg = walker.add_bone(name, Some(0), Vec3::new(x, 0.0, 0.0));
        walker.add_box(leg, Vec3::new(-2.0, -12.0, -2.0), [4, 12, 4], [0, 32]);
        walker.bones[leg].role = BoneRole::Leg;
    }
    (walker.bones[0].role, walker.bones[1].role) = (BoneRole::Body, BoneRole::Head);
    for place in walker.used_places() {
        walker.sheet.fill([place[0], place[1]], [place[2], place[3]], [150, 150, 150, 255]);
    }
    std::fs::write(dir.join(PATH), walker.to_text()).unwrap();
    let mut editor =
        voxear_editor::Editor::open_without_memory(Project::open(&dir).unwrap(), &mut renderer, LogCapture::default())
            .unwrap();
    frames(&mut editor, &mut renderer, 2);
    assert!(editor.open_path(PATH, &mut renderer));
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().walk_phase(), None, "nothing moves it while it is shaped");

    editor.set_model_mode(Mode::Motion);
    editor.set_model_pace(Pace::Walk);
    let mut phases = Vec::new();
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(15));
        frame(&mut editor, &mut renderer);
        phases.extend(editor.model_window().walk_phase());
    }
    assert!(phases.len() >= 10, "it is moved each frame");
    let turned: f32 = phases.windows(2).map(|pair| (pair[1] - pair[0]).rem_euclid(1.0)).sum();
    assert!(turned > 0.05, "its walk goes on as time does: {phases:?}");
    assert!(
        texts_in(&editor.ui, editor.ui.find(editor.ui.root(), "model-panel").unwrap()).contains(&"Attack".to_string())
    );
    keep_picture(&mut renderer, "model window motion.png");
    // A click on it picks nothing, and the model itself is as it was: watching changes nothing.
    let at = editor.model_window().view().point(&editor.ui, Vec3::new(0.0, 1.75, 0.25)).unwrap();
    click_at(&mut editor.ui, at);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.model_window().picked(), None);
    assert_eq!(editor.model_window().model(), Some(&walker));
    assert_eq!(saved(&dir), walker);

    editor.set_model_mode(Mode::Shape);
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.model_window().walk_phase(), None, "at rest again");
}
