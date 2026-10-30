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

#[test]
fn the_inspector_edits_the_selected_obj() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let mut editor = voxear_editor::Editor::open(hello(), &mut renderer, LogCapture::default()).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let selected = editor.first_mesh().expect("Hello has meshes");
    editor.select(Some(selected));
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();

    let root = editor.ui.root();
    let mut name = Vec::new();
    with_class(&editor.ui, root, "obj-name", &mut name);
    let expected = editor.game.engine.world.get::<Name>(selected).unwrap().0.clone();
    assert_eq!(editor.ui.text(name[0]), expected, "the Inspector shows the selected Obj");

    let mut numbers = Vec::new();
    with_class(&editor.ui, root, "number", &mut numbers);
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
