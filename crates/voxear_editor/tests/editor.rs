//! The editor on the Hello sample, offscreen. Skips itself without a GPU.

use voxear_core::{
    logging::LogCapture,
    math::{Vec2, Vec3},
};
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

/// The Obj called `name` in the editor's world as it is now, leaving out
/// Objs being destroyed, which go at the end of the frame.
fn obj(editor: &voxear_editor::Editor, name: &str) -> Option<voxear_ecs::Entity> {
    let world = &editor.game.engine.world;
    let objs = world.query_ref::<(voxear_ecs::Entity, &Name)>().without::<voxear_scene::Destroying>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e)
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

    // A second run reuses the scripts, since .NET starts only once. A value
    // marked to keep comes back into the edited level when play stops.
    editor.play(&mut renderer);
    assert!(editor.is_playing(), "plays at once, with the scripts already built");
    frame(&mut editor, &mut renderer);
    let played = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(played));
    frame(&mut editor, &mut renderer);
    let kept_at = voxear_core::math::Vec3::new(-3.0, 2.0, 1.0);
    editor.game.engine.world.get_mut::<Transform>(played).unwrap().position = kept_at;
    let position_row = prop_row(&editor.ui, "Position").expect("the Inspector shows Position");
    context_click(&mut editor.ui, position_row);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "keep-menu.png");
    let target = menu_item(&editor.ui, "Keep value after play").expect("offered while playing");
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.has_class(prop_row(&editor.ui, "Position").unwrap(), "kept"), "the row is marked");
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.play_error(), None);
    let back = *editor.game.engine.world.get::<Transform>(spinner).unwrap();
    assert_eq!(back.position, kept_at, "the kept position came back from play");
    assert_eq!(back.rotation, edited.rotation, "and nothing else did");
    assert!(editor.has_unsaved_changes());
    editor.undo(&mut renderer);
    assert_eq!(
        editor.game.engine.world.get::<Transform>(spinner).unwrap().position,
        edited.position,
        "undo takes it back"
    );
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
    assert!(obj(&editor, "Monolith").is_some(), "with Night's Objs");
    assert!(obj(&editor, "Lamp").is_none(), "and Day's gone");
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

    editor.undo(&mut renderer);
    assert_eq!(x(&editor), start, "undo takes typing back as one step");
    editor.redo(&mut renderer);
    assert_eq!(x(&editor), 7.25, "and redo makes it again");

    editor.save();
    assert!(!editor.has_unsaved_changes(), "saved");
    let scenes = ["Scenes/Environment.scene", "Scenes/Day.scene"]
        .map(|p| voxear_engine::scene_file::read(&dir.path().join(p)).unwrap());
    let saved =
        scenes.iter().flat_map(|s| s.objs.iter()).find(|o| o.name == name).expect("the edited Obj is in a scene");
    assert_eq!(saved.position.map(|p| p[0]), Some(7.25), "the edit is in the file");
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Moves the pointer as the window would tell the editor, so the Scene
/// view's tools see it as well as the UI.
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

#[test]
fn a_click_selects_an_obj_and_its_move_handle_drags_it() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let spinner = obj(&editor, "Spinner B").expect("Hello has Spinner B");
    let position =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<Transform>(spinner).unwrap().position;
    let start = position(&editor);
    let camera = editor.scene_camera().expect("the Scene view shows");

    let centre = camera.project(start).expect("in front of the camera");
    pointer_to(&mut editor, centre);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, true);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), Some(spinner), "a click on the cube selects it");

    // Move is the tool the strip starts on. Take the X arrow partway along,
    // and pull it further out.
    let along = camera.project(start + Vec3::X * camera.world_size(start, 60.0)).unwrap();
    pointer_to(&mut editor, along);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, true);
    frame(&mut editor, &mut renderer);
    let outwards = Vec2::from(along) + (Vec2::from(along) - Vec2::from(centre)).normalize() * 40.0;
    pointer_to(&mut editor, outwards.into());
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);

    let moved = position(&editor);
    assert!(moved.x > start.x + 0.1, "dragged along X: {start} to {moved}");
    assert!((moved.y - start.y).abs() < 1e-4 && (moved.z - start.z).abs() < 1e-4, "and only along X: {moved}");
    assert!(editor.has_unsaved_changes(), "a drag is an edit");
    editor.undo(&mut renderer);
    assert_eq!(position(&editor), start, "one undo takes the whole drag back");
}

/// Writes what the renderer last drew to `name` in `VOXEAR_TEST_OUTPUT`, when set.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

fn context_click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
}

/// The Hierarchy row showing the Obj called `name`.
fn hierarchy_row(ui: &Ui, name: &str) -> Option<NodeId> {
    let tree = ui.find(ui.root(), "hierarchy-tree")?;
    let mut names = Vec::new();
    with_class(ui, tree, "row-name", &mut names);
    let label = names.into_iter().find(|&n| ui.is_visible(n) && ui.text(n) == name)?;
    let mut at = Some(label);
    while let Some(node) = at {
        if ui.has_class(node, "list-row") {
            return Some(node);
        }
        at = ui.parent(node);
    }
    None
}

#[test]
fn the_hierarchy_makes_duplicates_deletes_and_reparents_objs() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let parent = |editor: &voxear_editor::Editor, e: voxear_ecs::Entity| {
        editor.game.engine.world.get::<voxear_scene::Parent>(e).map(|p| p.0)
    };
    let add = editor.ui.find(editor.ui.root(), "hierarchy-add").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "Cube").expect("the + menu offers a Cube");
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    let cube = obj(&editor, "Cube").expect("a Cube was made");
    assert_eq!(editor.selected(), Some(cube), "and selected");
    assert!(editor.game.engine.world.get::<voxear_render::MeshRenderer>(cube).is_some(), "with a mesh");
    assert!(editor.has_unsaved_changes());

    let target = hierarchy_row(&editor.ui, "Cube").expect("the Cube is in the Hierarchy");
    context_click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "hierarchy-menu.png");
    let target = menu_item(&editor.ui, "Duplicate").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    let copy = obj(&editor, "Cube (1)").expect("the copy is numbered");
    assert_eq!(editor.selected(), Some(copy));

    // Dragged onto the Cube, the copy goes inside it.
    let (from, onto) = (hierarchy_row(&editor.ui, "Cube (1)").unwrap(), hierarchy_row(&editor.ui, "Cube").unwrap());
    let ([fx, fy, fw, fh], [ox, oy, ow, oh]) = (editor.ui.rect(from), editor.ui.rect(onto));
    editor.ui.pointer_move(fx + fw / 2.0, fy + fh / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_move(ox + ow / 2.0, oy + oh / 2.0);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert_eq!(parent(&editor, copy), Some(cube), "dropped into the Cube");

    // Deleting the Cube takes the copy with it; undo brings both back.
    let target = hierarchy_row(&editor.ui, "Cube").unwrap();
    context_click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "Delete").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    assert!(obj(&editor, "Cube").is_none() && obj(&editor, "Cube (1)").is_none(), "both went");
    assert_eq!(editor.selected(), None);
    frame(&mut editor, &mut renderer);
    editor.undo(&mut renderer);
    let (cube, copy) = (obj(&editor, "Cube").expect("back"), obj(&editor, "Cube (1)").expect("both"));
    assert_eq!(editor.selected(), Some(cube));
    assert_eq!(parent(&editor, copy), Some(cube), "the copy is inside again");
    editor.undo(&mut renderer);
    assert_eq!(parent(&editor, copy), None, "the move undoes");
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Cube (1)").is_none(), "then the copy");
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Cube").is_none(), "then the Cube");
    editor.redo(&mut renderer);
    assert!(obj(&editor, "Cube").is_some(), "and redo makes it again");
}
