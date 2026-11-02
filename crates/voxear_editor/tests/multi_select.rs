//! Several Objs selected at once, on a copy of Hello: from code, with Ctrl
//! and a click in the Scene view, and with a box dragged over it; the move
//! handle taking them all along as one step to undo; and Delete taking them
//! all. Skips itself without a GPU.

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, Transform};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository.
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

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn obj(editor: &voxear_editor::Editor, name: &str) -> Entity {
    let world = &editor.game.engine.world;
    let objs = world.query_ref::<(Entity, &Name)>().without::<voxear_scene::Destroying>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap_or_else(|| panic!("Hello has {name}"))
}

/// Moves the pointer as the window would tell the editor.
fn pointer_to(editor: &mut voxear_editor::Editor, [x, y]: [f32; 2]) {
    let device_id = winit::event::DeviceId::dummy();
    let position = winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y));
    editor.handle(&winit::event::WindowEvent::CursorMoved { device_id, position });
}

fn left_button(editor: &mut voxear_editor::Editor, pressed: bool) {
    use winit::event::{ElementState, MouseButton, WindowEvent};
    let device_id = winit::event::DeviceId::dummy();
    let state = if pressed { ElementState::Pressed } else { ElementState::Released };
    editor.handle(&WindowEvent::MouseInput { device_id, state, button: MouseButton::Left });
}

fn control(editor: &mut voxear_editor::Editor, held: bool) {
    use winit::keyboard::ModifiersState;
    let state = if held { ModifiersState::CONTROL } else { ModifiersState::empty() };
    editor.handle(&winit::event::WindowEvent::ModifiersChanged(state.into()));
}

fn position(editor: &voxear_editor::Editor, entity: Entity) -> Vec3 {
    editor.game.engine.world.get::<Transform>(entity).unwrap().position
}

#[test]
fn several_objs_are_selected_moved_and_deleted_together() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let (a, b, ball) = (obj(&editor, "Spinner A"), obj(&editor, "Spinner B"), obj(&editor, "Chrome Ball"));

    // Selected from code, the first is the one the Inspector and the handles show; every Hierarchy shows both.
    editor.select_many(vec![a, b]);
    frame(&mut editor, &mut renderer);
    assert_eq!((editor.selected(), editor.selected_objs()), (Some(a), vec![a, b]));
    let tree = editor.ui.find(editor.ui.root(), "hierarchy-tree").unwrap();
    assert_eq!(editor.ui.selected_items(tree).len(), 2, "the Hierarchy shows both");

    // Ctrl and a click in the Scene view adds an Obj, and takes it out again.
    let camera = editor.scene_camera().expect("the Scene view shows");
    let at_ball = camera.project(position(&editor, ball)).unwrap();
    control(&mut editor, true);
    for _ in 0..2 {
        pointer_to(&mut editor, at_ball);
        frame(&mut editor, &mut renderer);
        left_button(&mut editor, true);
        left_button(&mut editor, false);
        frame(&mut editor, &mut renderer);
        if editor.selected_objs().len() == 3 {
            assert_eq!(editor.selected_objs(), [a, b, ball], "added");
        }
    }
    assert_eq!(editor.selected_objs(), [a, b], "and taken out");
    control(&mut editor, false);

    // The move handle's X arrow takes both along, as one step to undo.
    let (start_a, start_b) = (position(&editor, a), position(&editor, b));
    let centre = camera.project(start_a).unwrap();
    let along = camera.project(start_a + Vec3::X * camera.world_size(start_a, 60.0)).unwrap();
    pointer_to(&mut editor, along);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, true);
    frame(&mut editor, &mut renderer);
    let outwards = [along[0] + (along[0] - centre[0]).signum() * 40.0, along[1]];
    pointer_to(&mut editor, outwards);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    let (moved_a, moved_b) = (position(&editor, a) - start_a, position(&editor, b) - start_b);
    assert!(moved_a.x.abs() > 0.1, "the active one moved: {moved_a}");
    assert!(moved_b.abs_diff_eq(moved_a, 1e-3), "and the other with it: {moved_b} and {moved_a}");
    editor.undo(&mut renderer);
    assert_eq!((position(&editor, a), position(&editor, b)), (start_a, start_b), "one undo takes both back");

    // The Inspector shows the active one, says how many it edits, and a value typed goes to both,
    // each keeping its other axes; one undo takes both back.
    editor.select_many(vec![a, b]);
    frame(&mut editor, &mut renderer);
    let says = |editor: &voxear_editor::Editor, text: &str| {
        let mut stack = vec![editor.ui.root()];
        let mut found = false;
        while let Some(node) = stack.pop() {
            found |= editor.ui.text(node) == text;
            stack.extend(editor.ui.children(node));
        }
        found
    };
    assert!(says(&editor, "Editing 2 Objs: a change goes to each that has it."));
    let mut numbers = Vec::new();
    let mut stack = vec![editor.ui.root()];
    while let Some(node) = stack.pop() {
        if editor.ui.has_class(node, "axis-input") {
            numbers.push(node);
        }
        stack.extend(editor.ui.children(node));
    }
    numbers.sort_by(|p, q| editor.ui.rect(*p)[1].total_cmp(&editor.ui.rect(*q)[1]).then(editor.ui.rect(*p)[0].total_cmp(&editor.ui.rect(*q)[0])));
    let ui = &mut editor.ui;
    ui.focus(numbers[0]);
    ui.set_modifiers(voxear_ui::Modifiers { shift: false, control: true });
    ui.key_down(voxear_ui::Key::Char('a'));
    ui.set_modifiers(voxear_ui::Modifiers::default());
    ui.text_input("3.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let (now_a, now_b) = (position(&editor, a), position(&editor, b));
    assert_eq!((now_a.x, now_b.x), (3.5, 3.5), "Position X typed goes to both");
    assert_eq!((now_a.y, now_a.z, now_b.y, now_b.z), (start_a.y, start_a.z, start_b.y, start_b.z), "the rest kept");
    editor.undo(&mut renderer);
    assert_eq!((position(&editor, a), position(&editor, b)), (start_a, start_b), "one undo takes both back");

    // A box over the whole Scene view selects all it shows.
    editor.select(None);
    let [x, y, w, h] = camera.rect;
    pointer_to(&mut editor, [x + 4.0, y + 40.0]);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, true);
    frame(&mut editor, &mut renderer);
    pointer_to(&mut editor, [x + w - 4.0, y + h - 4.0]);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    let boxed = editor.selected_objs();
    assert!([a, b, ball].iter().all(|o| boxed.contains(o)), "the box took them in: {} Objs", boxed.len());

    // Delete takes them all, and one undo brings them all back.
    editor.select_many(vec![a, b, ball]);
    editor.delete_selection(&mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert!(![a, b, ball].iter().any(|&o| world.is_alive(o)), "all three are gone");
    assert_eq!(editor.selected_objs(), []);
    editor.undo(&mut renderer);
    let back = ["Spinner A", "Spinner B", "Chrome Ball"].map(|name| {
        let world = &editor.game.engine.world;
        world.query_ref::<&Name>().into_iter().any(|n| n.0 == name)
    });
    assert_eq!(back, [true; 3], "one undo brings them all back");
}
