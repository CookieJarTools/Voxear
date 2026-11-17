//! The pointer over the playing Game view, on Hello: the game's UI hears
//! the wheel and every button as a built game's does, and a drag on it ends
//! where the button is let go, after a click the game took the mouse on. A
//! file of its own, so it runs in a process of its own, since .NET starts
//! only once per process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scripting::CursorLock;
use winit::event::{DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};

const SIZE: (u32, u32) = (1280, 800);

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Moves the pointer as the window would tell the editor.
fn pointer_to(editor: &mut voxear_editor::Editor, (x, y): (f32, f32)) {
    let position = winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y));
    editor.handle(&WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position });
}

fn button(editor: &mut voxear_editor::Editor, button: MouseButton, pressed: bool) -> bool {
    let state = if pressed { ElementState::Pressed } else { ElementState::Released };
    editor.handle(&WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button })
}

#[test]
fn the_game_view_hands_the_games_ui_the_wheel_every_button_and_the_end_of_a_drag() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello");
    let project = Project::open(&hello).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, &mut renderer);
    let middle = editor.game_view_middle().expect("the Game view shows as play starts");
    pointer_to(&mut editor, middle);

    // A click on the view gives it the focus, and the game takes the mouse before it is let go, as a shooter does
    // on that click. The editor's UI had the press: it is left holding none, or it would hold every release after.
    button(&mut editor, MouseButton::Left, true);
    assert!(editor.ui.holds_press(), "the editor's UI had the press on the view");
    editor.game.display.cursor_lock = CursorLock::Locked;
    frame(&mut editor, &mut renderer);
    assert!(button(&mut editor, MouseButton::Left, false), "the release is the game's while it holds the mouse");
    assert!(!editor.ui.holds_press(), "the editor's UI let go of its press, with no click");

    // The game frees the mouse, as an opened map does, and shows a list taller than the screen.
    editor.game.display.cursor_lock = CursorLock::None;
    editor.game.ui.new_document();
    let ui = editor.game.ui.ui();
    let root = ui.root();
    let view = ui.panel(root, voxear_ui::Style::column().scroll().at(0.0, 0.0).width(8000.0).height(8000.0));
    for i in 0..400 {
        let item = ui.panel(view, voxear_ui::Style::default().height(30.0));
        ui.label(item, &format!("Item {i}"), voxear_ui::TextStyle::default());
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    pointer_to(&mut editor, middle);

    // The wheel scrolls it, and the game hears nothing of it.
    let wheel = WindowEvent::MouseWheel {
        device_id: DeviceId::dummy(),
        delta: MouseScrollDelta::LineDelta(0.0, -1.0),
        phase: TouchPhase::Moved,
    };
    assert!(editor.handle(&wheel), "the wheel over the game's UI is its own");
    assert!(editor.game.ui.ui().scroll_offset(view)[1] > 0.0, "the list scrolled");

    // The other buttons reach it: a right click, and the middle button held.
    assert!(button(&mut editor, MouseButton::Right, true), "a right press on the game's UI is its own");
    button(&mut editor, MouseButton::Right, false);
    assert!(editor.game.ui.ui().context_clicked(view), "a right click on the list");
    assert!(button(&mut editor, MouseButton::Middle, true), "a middle press on the game's UI is its own");
    assert!(editor.game.ui.ui().is_middle_pressed(view));
    button(&mut editor, MouseButton::Middle, false);
    assert!(!editor.game.ui.ui().is_middle_pressed(view), "and let go");

    // A drag on it ends where the button is let go, though the pointer went off the Game view meanwhile.
    assert!(button(&mut editor, MouseButton::Left, true));
    frame(&mut editor, &mut renderer);
    assert!(editor.game.ui.ui().is_pressed(view), "held while the button is down");
    pointer_to(&mut editor, (2.0, 2.0));
    button(&mut editor, MouseButton::Left, false);
    frame(&mut editor, &mut renderer);
    assert!(!editor.game.ui.ui().holds_press(), "let go, the drag is over");
    assert!(!editor.ui.holds_press(), "and the editor's UI holds nothing either");
}
