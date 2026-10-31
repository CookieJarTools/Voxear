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

#[test]
fn a_hovered_divider_shows_its_handle_over_the_scene_frame() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let mut dividers = Vec::new();
    with_class(&editor.ui, root, "split-divider", &mut dividers);
    // The divider beside the Inspector, on the Scene panel's frame.
    let ui = &editor.ui;
    let divider = dividers
        .into_iter()
        .filter(|&d| ui.rect(d)[2] < ui.rect(d)[3])
        .max_by(|a, b| ui.rect(*a)[0].total_cmp(&ui.rect(*b)[0]))
        .unwrap();
    let [x, y, w, h] = ui.rect(divider);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    // A frame to start the handle's fade, and one long enough to finish it.
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 0.5).unwrap();
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("divider.png")).unwrap();
        let mut encoder = png::Encoder::new(file, SIZE.0, SIZE.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let at = |px: f32, py: f32| {
        let i = ((py as u32 * width + px as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(x + w / 2.0, y + h / 2.0), [0x79, 0x88, 0xff], "the handle, over the frame around the Scene");
    assert_eq!(at(x + w / 2.0, y + 4.0), [0x0c, 0x0e, 0x10], "short of the divider's ends");
}

/// The Obj called `name` in the editor's world as it is now.
fn obj(editor: &voxear_editor::Editor, name: &str) -> Option<voxear_ecs::Entity> {
    let world = &editor.game.engine.world;
    world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e)
}

/// The only test that starts .NET, which can start once per process.
#[test]
fn play_runs_the_scripts_and_stop_brings_the_level_back() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some((mut renderer, mut editor)) = open() else { return };
    let frame = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer| {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    };
    let spinner = obj(&editor, "Spinner A").expect("Hello has Spinner A");
    editor.select(Some(spinner));
    let edited = {
        let mut moved = *editor.game.engine.world.get::<Transform>(spinner).unwrap();
        moved.position = voxear_core::math::Vec3::new(1.5, 0.5, 0.0);
        moved
    };
    *editor.game.engine.world.get_mut::<Transform>(spinner).unwrap() = edited;

    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let played = obj(&editor, "Spinner A").expect("the level comes across");
    assert_eq!(editor.game.engine.world.get::<Transform>(played).unwrap().position, edited.position, "edits and all");
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("play.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let turned = editor.game.engine.world.get::<Transform>(played).unwrap().rotation;
    assert!(turned.angle_between(edited.rotation) > 0.1, "Hello.Spinner ran and turned it");
    let root = editor.ui.root();
    let mut names = Vec::new();
    with_class(&editor.ui, root, "obj-name", &mut names);
    assert_eq!(editor.ui.text(names[0]), "Spinner A", "the selection follows into play");

    editor.set_paused(true);
    frame(&mut editor, &mut renderer);
    let held = editor.game.engine.world.get::<Transform>(played).unwrap().rotation;
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.get::<Transform>(played).unwrap().rotation, held, "paused, it holds still");
    editor.step();
    frame(&mut editor, &mut renderer);
    assert_ne!(editor.game.engine.world.get::<Transform>(played).unwrap().rotation, held, "a step moves it a frame");
    assert!(editor.is_paused(), "and leaves it paused");

    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!editor.is_playing());
    assert_eq!(obj(&editor, "Spinner A"), Some(spinner), "the edited level is back");
    assert_eq!(*editor.game.engine.world.get::<Transform>(spinner).unwrap(), edited, "as it was edited");
    assert!(editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).is_some(), "with no script running");

    // A second run reuses the scripts, since .NET starts only once.
    editor.play(&mut renderer);
    assert!(editor.is_playing(), "plays at once, with the scripts already built");
    frame(&mut editor, &mut renderer);
    editor.stop(&mut renderer);
    assert_eq!(editor.play_error(), None);
}

/// The first label anywhere in `node` whose text starts with `words`.
fn label_starting(ui: &Ui, node: NodeId, words: &str) -> Option<NodeId> {
    if ui.children(node).is_empty() && ui.text(node).starts_with(words) {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label_starting(ui, child, words))
}

#[test]
fn the_cookbook_reads_the_projects_docs() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "cookbook");
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let intro = label_starting(&editor.ui, root, "Hello's own notes").expect("the Cookbook opens on Hello's Docs");
    assert!(editor.ui.is_visible(intro));

    // Open the How Hello Works folder from the page tree.
    let mut rows = Vec::new();
    with_class(&editor.ui, root, "cb-row-title", &mut rows);
    let folder = rows.into_iter().find(|&r| editor.ui.text(r) == "How Hello Works").expect("the folder is in the tree");
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    assert!(label_starting(&editor.ui, root, "What happens when Hello plays").is_some(), "the folder's page opened");
    assert!(label_starting(&editor.ui, root, "In this folder").is_some(), "with what is in it");
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("cookbook.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}

#[test]
fn the_levels_panel_lists_levels_and_opens_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "levels");
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let list = editor.ui.find(root, "levels-list").unwrap();
    let mut names = Vec::new();
    with_class(&editor.ui, list, "row-name", &mut names);
    let titles: Vec<String> = names
        .iter()
        .filter(|&&n| editor.ui.is_visible(editor.ui.parent(n).unwrap()))
        .map(|&n| editor.ui.text(n))
        .collect();
    assert!(titles.starts_with(&["Levels".into(), "Day".into(), "Night".into(), "World".into()]), "{titles:?}");
    let night = names.into_iter().find(|&n| editor.ui.text(n) == "Night").unwrap();

    let [x, y, w, h] = editor.ui.rect(night);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    assert_eq!(editor.game.levels.current(), Some("Levels/Night.level"), "a click opens the level to edit");
    assert!(obj(&editor, "Lamp").is_some(), "with Night's Objs");
    assert!(obj(&editor, "Scene Camera").is_some(), "and the editor's own camera left alone");
    assert_eq!(editor.game.levels.active_scene(), Some("Scenes/Night.scene"));
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("levels.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}

/// A copy of the Hello sample's assets, for tests that save, so the sample
/// itself is never written.
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
    dir
}

#[test]
fn edits_undo_redo_and_save_to_the_scene_file() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let selected = editor.first_mesh().unwrap();
    let name = editor.game.engine.world.get::<Name>(selected).unwrap().0.clone();
    let start = editor.game.engine.world.get::<Transform>(selected).unwrap().position.x;
    editor.select(Some(selected));
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.save();
    assert!(!editor.has_unsaved_changes());

    let mut numbers = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "axis-input", &mut numbers);
    let ui = &mut editor.ui;
    ui.focus(numbers[0]);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input("7.25");
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let x = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<Transform>(selected).unwrap().position.x;
    assert_eq!(x(&editor), 7.25);
    assert!(editor.has_unsaved_changes(), "an edit is unsaved");

    editor.undo();
    assert_eq!(x(&editor), start, "undo takes typing back as one step");
    editor.redo();
    assert_eq!(x(&editor), 7.25, "and redo makes it again");

    editor.save();
    assert!(!editor.has_unsaved_changes(), "saved");
    let scenes = ["Scenes/Environment.scene", "Scenes/Day.scene"]
        .map(|p| voxear_engine::scene_file::read(&dir.path().join(p)).unwrap());
    let saved =
        scenes.iter().flat_map(|s| s.objs.iter()).find(|o| o.name == name).expect("the edited Obj is in a scene");
    assert_eq!(saved.position.map(|p| p[0]), Some(7.25), "the edit is in the file");
}
