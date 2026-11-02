//! Panels floated out of the editor's window into desktop windows of their
//! own, offscreen: each window is an output of the renderer, opened and
//! closed as the editor asks, as its window code does with real windows.
//! Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{OutputId, Renderer, RendererSettings};
use voxear_ui::{DesktopWindow, NodeId, Ui};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseButton, WindowEvent},
};

const SIZE: (u32, u32) = (1280, 800);

fn hello() -> Project {
    Project::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello")).unwrap()
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
    let mut editor = voxear_editor::Editor::open_without_memory(hello(), &mut renderer, LogCapture::default()).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    Some((renderer, editor))
}

/// The desktop windows open, as offscreen outputs, by the editor's windows.
type Opened = Vec<(DesktopWindow, OutputId)>;

/// Opens an output for each desktop window the editor wants and closes
/// those it no longer does, as its window code opens and closes windows,
/// then runs a frame.
fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, opened: &mut Opened) {
    let wanted = editor.wanted_windows();
    opened.retain(|&(window, output)| {
        let keep = wanted.iter().any(|w| w.window == window);
        if !keep {
            assert_eq!(editor.forget_desktop_window(window), Some(output));
            renderer.remove_output(output);
        }
        keep
    });
    for want in wanted {
        if opened.iter().any(|(window, _)| *window == want.window) {
            continue;
        }
        let size = (want.size.0 as u32, want.size.1 as u32);
        let output = renderer.add_offscreen_output(size.0, size.1).unwrap();
        editor.open_desktop_window(want.window, output, size);
        opened.push((want.window, output));
    }
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

fn dock_tab(ui: &Ui, title: &str) -> Option<NodeId> {
    let mut tabs = Vec::new();
    with_class(ui, ui.root(), "tab", &mut tabs);
    let says = |tab: NodeId| ui.text(tab) == title || ui.children(tab).iter().any(|&part| ui.text(part) == title);
    tabs.into_iter().find(|&tab| ui.is_visible(tab) && says(tab))
}

fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

fn centre(ui: &Ui, node: NodeId) -> (f32, f32) {
    let [x, y, w, h] = ui.rect(node);
    (x + w / 2.0, y + h / 2.0)
}

/// The pointer moving to `at` in a desktop window, or the main one with `None`.
fn pointer_to(editor: &mut voxear_editor::Editor, window: Option<DesktopWindow>, (x, y): (f32, f32)) {
    let position = PhysicalPosition::new(f64::from(x), f64::from(y));
    editor.handle_in(window, &WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position });
}

fn button(editor: &mut voxear_editor::Editor, window: Option<DesktopWindow>, button: MouseButton, pressed: bool) {
    let state = if pressed { ElementState::Pressed } else { ElementState::Released };
    editor.handle_in(window, &WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button });
}

/// Floats a panel out from its tab's menu, as a user would.
fn float_out(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, opened: &mut Opened, title: &str) {
    let tab = dock_tab(&editor.ui, title).unwrap_or_else(|| panic!("no tab {title}"));
    let at = centre(&editor.ui, tab);
    let window = editor.ui.window_of(tab);
    pointer_to(editor, window, at);
    button(editor, window, MouseButton::Right, true);
    button(editor, window, MouseButton::Right, false);
    frame(editor, renderer, opened);
    let item = menu_item(&editor.ui, "Float").expect("a docked panel can float out");
    let at = centre(&editor.ui, item);
    pointer_to(editor, window, at);
    button(editor, window, MouseButton::Left, true);
    button(editor, window, MouseButton::Left, false);
    for _ in 0..3 {
        frame(editor, renderer, opened);
    }
}

/// How many pixels of an output are not `color`.
fn not_colored(pixels: &[u8], color: [u8; 3]) -> usize {
    pixels.chunks(4).filter(|p| p[..3] != color).count()
}

#[test]
fn panels_float_out_into_desktop_windows_and_dock_back() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let mut opened = Opened::new();

    // The Inspector, from its tab's menu: a window of its own, drawn there and not in the main window.
    float_out(&mut editor, &mut renderer, &mut opened, "Inspector");
    assert_eq!(opened.len(), 1, "{:?}", editor.wanted_windows());
    let (window, output) = opened[0];
    assert_eq!(editor.wanted_windows()[0].panels, ["inspector"]);
    let tab = dock_tab(&editor.ui, "Inspector").unwrap();
    assert_eq!(editor.ui.window_of(tab), Some(window));
    assert!(editor.ui.rect(tab)[1] < 32.0, "its tab is at its own window's top: {:?}", editor.ui.rect(tab));
    let layout = editor.ui.dock_layout(dock);
    assert!(layout.contains(" window(") && layout.ends_with("[inspector])"), "the layout keeps it: {layout}");
    let (_, _, pixels) = renderer.read_output_pixels(output).unwrap();
    let base = pixels[..3].to_vec();
    assert!(not_colored(&pixels, [base[0], base[1], base[2]]) > 500, "its UI is drawn in its own output");

    // Input from its window reaches what it shows, and only that.
    let at = centre(&editor.ui, tab);
    pointer_to(&mut editor, Some(window), at);
    assert!(editor.ui.is_hovered(tab));
    pointer_to(&mut editor, None, at);
    assert!(!editor.ui.is_hovered(tab), "the same place in the main window is the main window's");

    // The Scene view floats out with its 3D, drawn into its window's output.
    float_out(&mut editor, &mut renderer, &mut opened, "Scene");
    assert_eq!(opened.len(), 2);
    let (scene_window, scene_output) = opened[1];
    let scene = dock_tab(&editor.ui, "Scene").unwrap();
    assert_eq!(editor.ui.window_of(scene), Some(scene_window));
    let (width, height, pixels) = renderer.read_output_pixels(scene_output).unwrap();
    let middle = (((height / 2) * width + width / 2) * 4) as usize;
    assert_ne!(pixels[middle..middle + 3], base[..], "the Scene view's 3D is drawn in its window");

    // Closed from its own frame, its panels close, and its window goes.
    editor.close_desktop_window(window);
    frame(&mut editor, &mut renderer, &mut opened);
    assert_eq!(opened.len(), 1);
    assert!(renderer.read_output_pixels(output).is_err(), "its output is gone");
    let panels = editor.ui.dock_panels(dock);
    assert!(panels.iter().any(|(id, _, open)| id == "inspector" && !open), "the Inspector closed: {panels:?}");

    // The Scene view's tab dragged from its window onto the main window docks it there.
    let scene = dock_tab(&editor.ui, "Scene").unwrap();
    let from = centre(&editor.ui, scene);
    pointer_to(&mut editor, Some(scene_window), from);
    button(&mut editor, Some(scene_window), MouseButton::Left, true);
    pointer_to(&mut editor, Some(scene_window), (from.0 + 40.0, from.1 + 30.0));
    let project = dock_tab(&editor.ui, "Project").unwrap();
    let area = editor.ui.parent(editor.ui.parent(project).unwrap()).unwrap();
    let [x, y, w, h] = editor.ui.rect(editor.ui.children(area)[0]);
    pointer_to(&mut editor, None, (x + w * 0.75, y + h / 2.0));
    button(&mut editor, None, MouseButton::Left, false);
    frame(&mut editor, &mut renderer, &mut opened);
    assert!(opened.is_empty(), "its emptied window goes");
    let scene = dock_tab(&editor.ui, "Scene").unwrap();
    assert_eq!(editor.ui.window_of(scene), None);
    assert!(editor.ui.dock_layout(dock).contains("scene"), "{}", editor.ui.dock_layout(dock));
    assert!(editor.ui.dock_panel_neighbours(dock, "scene").contains(&"project".to_string()));
}

/// A layout that names a desktop window opens it, and the views in it draw there.
#[test]
fn a_layout_with_a_desktop_window_opens_it() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let mut opened = Opened::new();
    let layout = "row(240, [hierarchy], row(-300, column(66%, [scene, docs], [project, console]), [inspector])) \
                  window(500, 360, [game])";
    editor.ui.set_dock_layout(dock, layout).unwrap();
    frame(&mut editor, &mut renderer, &mut opened);
    frame(&mut editor, &mut renderer, &mut opened);
    let wanted = editor.wanted_windows();
    assert_eq!((wanted.len(), wanted[0].size), (1, (500.0, 360.0)));
    assert_eq!(renderer.output_size(opened[0].1), Some((500, 360)));
    editor.resize_desktop_window(opened[0].0, (640, 400));
    frame(&mut editor, &mut renderer, &mut opened);
    assert!(editor.ui.dock_layout(dock).ends_with(" window(640, 400, [game])"), "{}", editor.ui.dock_layout(dock));
}
