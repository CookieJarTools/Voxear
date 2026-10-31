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
    // A component's value can be kept too: the Lamp's intensity, typed in while playing.
    let lamp = obj(&editor, "Lamp").unwrap();
    let intensity =
        |editor: &voxear_editor::Editor, e| editor.game.engine.world.get::<voxear_render::Light>(e).unwrap().intensity;
    let lamp_was = intensity(&editor, lamp);
    editor.select(Some(lamp));
    frame(&mut editor, &mut renderer);
    let field = prop_field(&editor.ui, "Intensity").expect("the light's values show while playing");
    type_into(&mut editor.ui, field, "3.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    let target = prop_row(&editor.ui, "Intensity").unwrap();
    context_click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "Keep value after play").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);

    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.play_error(), None);
    let back = *editor.game.engine.world.get::<Transform>(spinner).unwrap();
    assert_eq!(back.position, kept_at, "the kept position came back from play");
    assert_eq!(back.rotation, edited.rotation, "and nothing else did");
    let lamp = obj(&editor, "Lamp").unwrap();
    assert_eq!(intensity(&editor, lamp), 3.5, "and so did the Lamp's intensity");
    assert!(editor.has_unsaved_changes());
    editor.undo(&mut renderer);
    assert_eq!(
        editor.game.engine.world.get::<Transform>(spinner).unwrap().position,
        edited.position,
        "undo takes them back"
    );
    assert_eq!(intensity(&editor, lamp), lamp_was, "both, as one step");
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

/// The Inspector field in the row labelled `label`: its first text field.
fn prop_field(ui: &Ui, label: &str) -> Option<NodeId> {
    let row = prop_row(ui, label)?;
    let mut fields = Vec::new();
    with_class(ui, row, "value-input", &mut fields);
    with_class(ui, row, "axis-input", &mut fields);
    fields.first().copied()
}

fn type_into(ui: &mut Ui, field: NodeId, text: &str) {
    ui.focus(field);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input(text);
}

/// Frames until the scripts are described from their build. `false`
/// without .NET and the managed build, which describing them needs.
fn described(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) -> bool {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return false;
    }
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(editor, renderer);
    true
}

/// Every Inspector row labelled `label`, top to bottom.
fn prop_rows(ui: &Ui, label: &str) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().filter(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label)).collect()
}

/// The first element with `class` inside `node`.
fn first_with(ui: &Ui, node: NodeId, class: &str) -> Option<NodeId> {
    let mut found = Vec::new();
    with_class(ui, node, class, &mut found);
    found.first().copied()
}

#[test]
fn script_fields_pick_references_and_edit_lists() {
    let Some((mut renderer, mut editor)) = open() else { return };
    if !described(&mut editor, &mut renderer) {
        return;
    }
    let spinner = obj(&editor, "Spinner B").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let picker =
        first_with(&editor.ui, prop_row(&editor.ui, "Leader").unwrap(), "value-picker").expect("a reference picker");
    assert_eq!(editor.ui.text(first_with(&editor.ui, picker, "picker-label").unwrap()), "Spinner A");
    click(&mut editor.ui, picker);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "reference-menu.png");
    assert!(menu_item(&editor.ui, "Spinner B").is_some(), "Objs with a Spinner are offered");
    assert!(menu_item(&editor.ui, "Ground").is_none(), "and only those");
    let target = menu_item(&editor.ui, "None").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    let leader_value = |editor: &voxear_editor::Editor| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0[0].fields.get("leader").cloned()
    };
    assert_eq!(leader_value(&editor), Some(toml::Value::Table(toml::Table::new())), "picked as None");
    frame(&mut editor, &mut renderer);
    let picker = first_with(&editor.ui, prop_row(&editor.ui, "Leader").unwrap(), "value-picker").unwrap();
    assert_eq!(editor.ui.text(first_with(&editor.ui, picker, "picker-label").unwrap()), "None");

    // The GameManager's cycle: a list of LevelStop tables.
    let manager = obj(&editor, "Game Manager").unwrap();
    editor.select(Some(manager));
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "inspector-list.png");
    let cycle = prop_row(&editor.ui, "Cycle").expect("the list shows");
    let count = |editor: &voxear_editor::Editor| {
        let row = prop_row(&editor.ui, "Cycle").unwrap();
        editor.ui.text(first_with(&editor.ui, row, "value").unwrap())
    };
    assert_eq!(count(&editor), "2 items");
    assert_eq!(prop_rows(&editor.ui, "Seconds").len(), 2, "each item's fields show");
    let cycle_now = |editor: &voxear_editor::Editor| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(manager).unwrap();
        kept.0[0].fields.get("cycle").and_then(|c| c.as_array().cloned()).unwrap()
    };
    let field = first_with(&editor.ui, prop_rows(&editor.ui, "Seconds")[1], "value-input").unwrap();
    type_into(&mut editor.ui, field, "7");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(cycle_now(&editor)[1].get("seconds"), Some(&toml::Value::Float(7.0)), "a value inside an item");

    let add = first_with(&editor.ui, cycle, "list-add").expect("the list can grow");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(count(&editor), "3 items");
    assert_eq!(cycle_now(&editor).len(), 3);
    let last = prop_row(&editor.ui, "Element 2").expect("the new item shows");
    let remove = first_with(&editor.ui, last, "list-remove").unwrap();
    click(&mut editor.ui, remove);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(count(&editor), "2 items", "and shrink");
    editor.undo(&mut renderer);
    assert_eq!(cycle_now(&editor)[1].get("seconds"), Some(&toml::Value::Float(5.0)), "undo takes the edits back");
}

#[test]
fn the_inspector_edits_components_and_script_fields() {
    let Some((mut renderer, mut editor)) = open() else { return };
    if !described(&mut editor, &mut renderer) {
        return;
    }
    let spinner = obj(&editor, "Spinner B").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "inspector-script.png");

    // Spinner B's scene sets only its leader; its speed shows as the script starts it.
    let speed = prop_field(&editor.ui, "Degrees Per Second").expect("the script's fields show");
    assert_eq!(editor.ui.text(speed), "90", "as Spinner.cs starts it");
    type_into(&mut editor.ui, speed, "45");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    let fields = |editor: &voxear_editor::Editor| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0[0].fields.get("degreesPerSecond").cloned()
    };
    assert_eq!(fields(&editor), Some(toml::Value::Float(45.0)), "typing sets the field for the scene");
    let leader = prop_row(&editor.ui, "Leader").expect("a reference shows");
    let label = first_with(&editor.ui, leader, "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Spinner A", "by the name of the Obj it points at");
    editor.undo(&mut renderer);
    assert_eq!(fields(&editor), None, "undo takes the field back to the script's value");

    // Add Component: a light, whose intensity is then edited, then removed.
    frame(&mut editor, &mut renderer);
    let mut add = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "add-component", &mut add);
    click(&mut editor.ui, add[0]);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "Light").expect("Add Component offers a light");
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let light = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<voxear_render::Light>(spinner).copied();
    assert!(light(&editor).is_some(), "the light was added");
    let intensity = prop_field(&editor.ui, "Intensity").expect("and shows its values");
    type_into(&mut editor.ui, intensity, "2.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(light(&editor).unwrap().intensity, 2.5);
    shot(&mut renderer, "inspector-light.png");

    let mut headers = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "foldout-header", &mut headers);
    let header = headers.into_iter().find(|&h| label_starting(&editor.ui, h, "Light").is_some()).unwrap();
    context_click(&mut editor.ui, header);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "Remove Component").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    assert!(light(&editor).is_none(), "removed");
    editor.undo(&mut renderer);
    assert_eq!(light(&editor).map(|l| l.intensity), Some(2.5), "undo brings it back as it was");

    // The mesh is picked from a menu.
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mesh_row = prop_row(&editor.ui, "Mesh").unwrap();
    let mut dropdowns = Vec::new();
    with_class(&editor.ui, mesh_row, "value-choice", &mut dropdowns);
    click(&mut editor.ui, dropdowns[0]);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let sphere = options.into_iter().find(|&o| label_starting(&editor.ui, o, "Sphere").is_some()).unwrap();
    click(&mut editor.ui, sphere);
    frame(&mut editor, &mut renderer);
    let mesh = editor.game.engine.world.get::<voxear_render::MeshRenderer>(spinner).unwrap().mesh;
    assert_eq!(editor.game.content.mesh_ref(mesh).map(|r| r.to_string()).as_deref(), Some("builtin:sphere"));
}

/// Drags one element onto another with the pointer, as the window would.
fn drag(ui: &mut Ui, from: NodeId, to: NodeId) {
    let ([fx, fy, fw, fh], [tx, ty, tw, th]) = (ui.rect(from), ui.rect(to));
    ui.pointer_move(fx + fw / 2.0, fy + fh / 2.0);
    ui.pointer_button(true);
    ui.pointer_move(fx + fw / 2.0 + 20.0, fy + fh / 2.0 + 20.0);
    ui.pointer_move(tx + tw / 2.0, ty + th / 2.0);
    ui.pointer_button(false);
}

/// The Project tile named `name`, among those showing.
fn tile(ui: &Ui, name: &str) -> Option<NodeId> {
    let mut tiles = Vec::new();
    with_class(ui, ui.root(), "tile", &mut tiles);
    tiles.into_iter().find(|&t| ui.is_visible(t) && ui.children(t).get(1).is_some_and(|&n| ui.text(n) == name))
}

#[test]
fn references_take_objs_and_assets_dragged_onto_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    if !described(&mut editor, &mut renderer) {
        return;
    }
    let spinner = obj(&editor, "Spinner B").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let field = |editor: &voxear_editor::Editor, name: &str| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0.iter().find_map(|spec| spec.fields.get(name).cloned())
    };
    let picker = |ui: &Ui, label: &str| first_with(ui, prop_row(ui, label).unwrap(), "value-picker").unwrap();
    let leader_was = field(&editor, "leader");

    // The Leader takes neither an Obj in another scene nor one with no Spinner, and the status bar says why.
    for (name, why) in [("Ground", "Ground is in another scene"), ("Chrome Ball", "Chrome Ball has no Spinner")] {
        let (from, to) = (hierarchy_row(&editor.ui, name).unwrap(), picker(&editor.ui, "Leader"));
        drag(&mut editor.ui, from, to);
        frame(&mut editor, &mut renderer);
        assert_eq!(field(&editor, "leader"), leader_was, "unchanged by {name}");
        frame(&mut editor, &mut renderer);
        assert!(label_starting(&editor.ui, editor.ui.root(), why).is_some(), "says: {why}");
    }
    assert_eq!(editor.selected(), Some(spinner), "a drag selects nothing");

    let (from, to) = (hierarchy_row(&editor.ui, "Spinner A").unwrap(), picker(&editor.ui, "Leader"));
    let spinner_a = obj(&editor, "Spinner A").unwrap();
    // Held over the field for a frame: it lights up, and the ghost names what is dragged.
    let ([fx, fy, fw, fh], [tx, ty, tw, th]) = (editor.ui.rect(from), editor.ui.rect(to));
    editor.ui.pointer_move(fx + fw / 2.0, fy + fh / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_move(tx + tw / 2.0, ty + th / 2.0);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.has_class(to, "drop-hover"));
    // Long enough for the border's fade to finish.
    editor.update(&mut renderer, SIZE, 1.0, 0.5).unwrap();
    shot(&mut renderer, "drag-over.png");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let leader = field(&editor, "leader").unwrap();
    assert_eq!(leader.get("entity").and_then(toml::Value::as_integer), Some(spinner_a.to_bits() as i64));
    drag(&mut editor.ui, from, to);
    frame(&mut editor, &mut renderer);
    let leader = field(&editor, "leader").unwrap();
    assert_eq!(leader.get("entity").and_then(toml::Value::as_integer), Some(spinner_a.to_bits() as i64));

    // A FireflySpawner's prefab, dragged from the Project's Prefabs folder.
    let mut add = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "add-component", &mut add);
    click(&mut editor.ui, add[0]);
    frame(&mut editor, &mut renderer);
    let target = menu_item(&editor.ui, "FireflySpawner").expect("the project's scripts are offered");
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let folder = tile(&editor.ui, "Prefabs").expect("the Project shows its folders as tiles");
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let (from, to) = (tile(&editor.ui, "Firefly.prefab").expect("the prefabs show"), picker(&editor.ui, "Firefly"));
    drag(&mut editor.ui, from, to);
    frame(&mut editor, &mut renderer);
    assert_eq!(field(&editor, "firefly"), Some(toml::Value::String("Prefabs/Firefly.prefab".into())));
    frame(&mut editor, &mut renderer);
    let label = first_with(&editor.ui, picker(&editor.ui, "Firefly"), "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Firefly.prefab");

    let (from, to) = (tile(&editor.ui, "Lamp.prefab").unwrap(), picker(&editor.ui, "Leader"));
    drag(&mut editor.ui, from, to);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(label_starting(&editor.ui, editor.ui.root(), "Lamp.prefab is an asset").is_some(), "an asset is no Obj");
}
