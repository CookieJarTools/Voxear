//! VoxDocs pages that work with the editor, on a copy of Hello: a guide's
//! step runs the command it names and a check ticks once what it looks for
//! is there; and a live material shows its fields, to change on the page.
//! Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello without its scripts, which nothing here needs, so no
/// build outlives the test.
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
    let dir = tempfile::tempdir().unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, project.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
    dir
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

fn texts(ui: &Ui, class: &str) -> Vec<String> {
    let mut found = Vec::new();
    with_class(ui, ui.root(), class, &mut found);
    found.into_iter().map(|node| ui.text(node)).collect()
}

#[test]
fn a_guides_step_runs_a_command_and_its_check_ticks() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    // The guide, from the command bar.
    let root = editor.ui.root();
    let field = editor.ui.find(root, "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("step-by-step guides");
    frame(&mut editor, &mut renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(texts(&editor.ui, "cb-progress-line"), ["1 of 4 steps done"], "Hello has its lamp already");

    let mut buttons = Vec::new();
    with_class(&editor.ui, root, "md-step-action", &mut buttons);
    let light = buttons.into_iter().find(|&b| editor.ui.text(b) == "Add a point light").expect("the step's button");
    let [x, y, w, h] = editor.ui.rect(light);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let names = world.query_ref::<&voxear_scene::Name>().into_iter().filter(|n| n.0 == "Point Light").count();
    assert_eq!(names, 1, "the command made the light");
    assert_eq!(texts(&editor.ui, "cb-progress-line"), ["3 of 4 steps done"], "the action and its check");
}

#[test]
fn a_live_materials_fields_change_it_on_the_page() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    let material = dir.path().join("Materials/Brass.material");
    std::fs::write(&material, "color = [0.8, 0.6, 0.2]\nmetallic = 1.0\nroughness = 0.3\n").unwrap();
    std::fs::write(dir.path().join("Docs/Brass.md"), "Brass, for the lamps.\n\n![Brass](Materials/Brass.material)\n")
        .unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let root = editor.ui.root();
    let field = editor.ui.find(root, "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("brass");
    frame(&mut editor, &mut renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let mut places = Vec::new();
    with_class(&editor.ui, root, "md-live-fields", &mut places);
    assert_eq!(places.len(), 1, "a place for the material's fields");
    let mut labels = Vec::new();
    with_class(&editor.ui, places[0], "prop-label", &mut labels);
    let rows: Vec<String> = labels.iter().map(|&l| editor.ui.text(l)).collect();
    assert_eq!(rows, ["Colour", "Emission", "Metallic", "Roughness"], "its fields, as the Inspector has them");

    let roughness = editor.ui.parent(labels[3]).unwrap();
    let input = editor.ui.children(roughness)[1];
    editor.ui.focus(input);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("0.9");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let saved = std::fs::read_to_string(&material).unwrap();
    assert!(saved.contains("roughness = 0.9"), "saved to the material: {saved}");

    // Its colour's swatch opens the colour picker, as in the Inspector. The page drew again with the new
    // preview, and its fields with it.
    let mut places = Vec::new();
    with_class(&editor.ui, root, "md-live-fields", &mut places);
    let mut labels = Vec::new();
    with_class(&editor.ui, places[0], "prop-label", &mut labels);
    let colour = editor.ui.parent(labels[0]).unwrap();
    let mut swatches = Vec::new();
    with_class(&editor.ui, colour, "swatch", &mut swatches);
    let [x, y, w, h] = editor.ui.rect(swatches[0]);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert!(editor.color_picker_open());
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "docs-material-fields.png");
}
