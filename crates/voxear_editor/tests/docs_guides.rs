//! A VoxDocs guide in the editor, on a copy of Hello: a step's button runs
//! the command it names, and a check ticks once what it looks for is there.
//! Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

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
