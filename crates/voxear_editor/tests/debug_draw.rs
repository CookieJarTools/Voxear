//! Debug draw in the editor: its lines in the Scene view, and its texts as
//! labels at the points they name. A file of its own, so it runs in a
//! process of its own: debug draw keeps one list for the process, which
//! every editor open in it would show and count down. Skips itself without
//! a GPU.

use voxear_core::{debug_draw, logging::LogCapture, math::Vec3};
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

#[test]
fn texts_show_at_their_points_and_go_when_their_time_is_up() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let hello = Project::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello")).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(hello, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    let spot = Vec3::new(0.0, 1.0, 0.0);
    debug_draw::text(spot, "Look here", debug_draw::YELLOW, 0.1);
    debug_draw::wire_box(spot, Vec3::splat(0.5), Default::default(), debug_draw::CYAN, 0.1, true);
    frame(&mut editor, &mut renderer);
    let mut labels = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "debug-text", &mut labels);
    let label = labels.iter().copied().find(|&l| editor.ui.text(l) == "Look here").expect("the text shows as a label");
    assert!(editor.ui.is_visible(label));
    let camera = editor.scene_camera().expect("the Scene view shows");
    let at = camera.project(spot).unwrap();
    let [x, y, _, _] = editor.ui.rect(label);
    assert!((x - at[0]).abs() < 12.0 && (y - at[1]).abs() < 24.0, "beside its point: {at:?} and {x}, {y}");
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("debug-draw-editor.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    // Not playing, the editor counts their time down itself.
    for _ in 0..8 {
        frame(&mut editor, &mut renderer);
    }
    assert!(!editor.ui.is_visible(label), "gone once its time is up");
    assert!(debug_draw::lines().is_empty());
}
