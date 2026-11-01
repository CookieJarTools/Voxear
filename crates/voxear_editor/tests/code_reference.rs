//! VoxDocs' Code Reference, made from a copy of Hello's scripts as the
//! editor builds them: the folder after the project's pages, a namespace's
//! folder, and a type's page with its `///` comments. Skips itself without a
//! GPU or .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
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

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The first label anywhere in `node` whose text starts with `words`.
fn label(ui: &Ui, node: NodeId, words: &str) -> Option<NodeId> {
    if ui.text(node).starts_with(words) && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label(ui, child, words))
}

/// Clicks the middle of the element with `class` whose text is `text`,
/// then gives the editor two frames to answer.
fn click_text(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, class: &str, text: &str) {
    let mut found = Vec::new();
    with_class(&editor.ui, editor.ui.root(), class, &mut found);
    let node = found
        .into_iter()
        .find_map(|node| label(&editor.ui, node, text).filter(|&l| editor.ui.text(l) == text))
        .unwrap_or_else(|| panic!("no {class} reads {text}"));
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn voxdocs_shows_the_scripts_code_reference() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, &mut renderer);

    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "docs");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    click_text(&mut editor, &mut renderer, "cb-row-title", "Code Reference");
    assert!(label(&editor.ui, root, "Every public type").is_some(), "the Code Reference's own page opened");

    // Its folders link to Hello's namespace, and that to Spinner's page.
    click_text(&mut editor, &mut renderer, "cb-child", "Hello");
    click_text(&mut editor, &mut renderer, "cb-child", "Spinner");
    let summary = label(&editor.ui, root, "Spins its Obj around the up axis").expect("Spinner's comment shows");
    assert!(editor.ui.is_visible(summary));
    assert!(label(&editor.ui, root, "public sealed class Spinner : Behaviour").is_some(), "with its declaration");
    assert!(label(&editor.ui, root, "protected override void Awake()").is_some(), "and its members");
    let edit = editor.ui.find(root, "cb-edit").unwrap();
    assert!(!editor.ui.is_visible(edit), "a page made from the code is not written here");
}
