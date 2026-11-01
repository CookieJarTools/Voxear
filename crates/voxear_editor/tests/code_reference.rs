//! VoxDocs and the code, on a copy of Hello whose scripts the editor
//! builds: the Code Reference made from them, and `[Documentation]` and
//! `[Excerpt]` in the Inspector. Skips itself without a GPU or .NET.

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

/// The editor on a copy of Hello, once its scripts are described.
fn open_described(renderer: &mut Renderer, dir: &tempfile::TempDir) -> voxear_editor::Editor {
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, renderer);
    editor
}

/// A renderer, or why the test cannot run.
fn renderer() -> Option<Renderer> {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return None;
    }
    let renderer = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok();
    if renderer.is_none() {
        eprintln!("skipping: no usable GPU");
    }
    renderer
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

/// The editor as it looks, saved when `VOXEAR_TEST_OUTPUT` names a folder.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

fn click(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn voxdocs_shows_the_scripts_code_reference() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let mut editor = open_described(&mut renderer, &dir);

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

#[test]
fn the_inspector_shows_and_opens_what_code_documents() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let mut editor = open_described(&mut renderer, &dir);
    let root = editor.ui.root();
    let world = &editor.game.engine.world;
    let manager = world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == "Game Manager");
    editor.select(manager.map(|(entity, _)| entity));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // [Excerpt] on its cycle: the section of the page, in a fold that starts closed.
    let mut folds = Vec::new();
    with_class(&editor.ui, root, "excerpt", &mut folds);
    assert_eq!(folds.len(), 1, "one excerpt");
    let header = editor.ui.foldout_header(folds[0]).unwrap();
    assert!(label(&editor.ui, header, "Who drives it").is_some(), "titled by its heading");
    assert!(!editor.ui.is_open(folds[0]));
    click(&mut editor, &mut renderer, header);
    let words = label(&editor.ui, folds[0], "The GameManager keeps the score").expect("the section's words");
    assert!(editor.ui.is_visible(words));
    shot(&mut renderer, "inspector-excerpt.png");

    // [Documentation] on the class: its book button opens the page, which lists the code pointing at it.
    let mut books = Vec::new();
    with_class(&editor.ui, root, "docs-link", &mut books);
    assert_eq!(books.len(), 1, "a book on the Game Manager's section");
    click(&mut editor, &mut renderer, books[0]);
    let docs = editor.ui.find(root, "docs").unwrap();
    assert!(label(&editor.ui, docs, "Hello moves between two levels").is_some(), "the page opened in VoxDocs");
    assert!(label(&editor.ui, docs, "In the code").is_some());
    assert!(label(&editor.ui, docs, "GameManager.cycle").is_some(), "with what points at it");
    shot(&mut renderer, "docs-in-the-code.png");
}
