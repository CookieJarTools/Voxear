//! The editor on the Hello sample, offscreen. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, Transform};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

fn hello() -> Project {
    Project::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello")).unwrap()
}

/// Every element in `node` with `class`, in tree order.
fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The editor open on Hello, a frame in, or `None` without a GPU.
fn open() -> Option<(Renderer, voxear_editor::Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let mut editor = voxear_editor::Editor::open(hello(), &mut renderer, LogCapture::default()).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    Some((renderer, editor))
}

#[test]
fn the_inspector_edits_the_selected_obj() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let selected = editor.first_mesh().expect("Hello has meshes");
    editor.select(Some(selected));
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();

    let root = editor.ui.root();
    let mut name = Vec::new();
    with_class(&editor.ui, root, "obj-name", &mut name);
    let expected = editor.game.engine.world.get::<Name>(selected).unwrap().0.clone();
    assert_eq!(editor.ui.text(name[0]), expected, "the Inspector shows the selected Obj");

    let mut numbers = Vec::new();
    with_class(&editor.ui, root, "axis-input", &mut numbers);
    assert_eq!(numbers.len(), 9, "position, rotation and scale, three axes each");
    let ui = &mut editor.ui;
    ui.focus(numbers[0]);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input("5.5");
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let position = editor.game.engine.world.get::<Transform>(selected).unwrap().position;
    assert_eq!(position.x, 5.5, "typing in Position X moved the Obj");

    if let Some(transform) = editor.game.engine.world.get_mut::<Transform>(selected) {
        transform.position.y = 2.25;
    }
    editor.ui.blur();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    assert_eq!(editor.ui.text(numbers[1]), "2.25", "changes made elsewhere show in the fields");
}

#[test]
fn the_project_shows_folders_as_tiles_and_opens_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    // The files measure their width, then turn to tiles.
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let root = editor.ui.root();
    let path = editor.ui.find(root, "project-path").unwrap();
    assert_eq!(editor.ui.text(path), "Hello");

    let mut tiles = Vec::new();
    with_class(&editor.ui, root, "tile", &mut tiles);
    let ui = &editor.ui;
    let models = tiles.into_iter().find(|&tile| ui.is_visible(tile) && ui.text(ui.children(tile)[1]) == "Models");
    let [x, y, w, h] = ui.rect(models.expect("a tile for the Models folder"));
    let ui = &mut editor.ui;
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        ui.pointer_button(true);
        ui.pointer_button(false);
    }
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    assert_eq!(editor.ui.text(path), "Hello  /  Models", "a double click opens the folder");
    let folders = editor.ui.find(root, "project-folders").unwrap();
    assert_eq!(editor.ui.selection(folders).len(), 1, "and the tree marks it");
}
