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
    open_logging(LogCapture::default())
}

/// The same, its Console showing what is put in `log`.
fn open_logging(log: LogCapture) -> Option<(Renderer, voxear_editor::Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    // Without memory, so what one run sets cannot change the next.
    let mut editor = voxear_editor::Editor::open_without_memory(hello(), &mut renderer, log).unwrap();
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
fn a_selected_script_shows_its_code() {
    let Some((mut renderer, mut editor)) = open() else { return };
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let tile_named = |editor: &voxear_editor::Editor, name: &str| {
        let (ui, mut tiles) = (&editor.ui, Vec::new());
        with_class(ui, ui.root(), "tile", &mut tiles);
        tiles.into_iter().find(|&tile| ui.is_visible(tile) && ui.text(ui.children(tile)[1]).starts_with(name))
    };
    let press = |editor: &mut voxear_editor::Editor, tile: NodeId, times: usize| {
        let [x, y, w, h] = editor.ui.rect(tile);
        editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
        for _ in 0..times {
            editor.ui.pointer_button(true);
            editor.ui.pointer_button(false);
        }
    };
    let scripts = tile_named(&editor, "Scripts").expect("a tile for the Scripts folder");
    press(&mut editor, scripts, 2);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let spinner = tile_named(&editor, "Spinner").expect("a tile for Spinner.cs");
    press(&mut editor, spinner, 1);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut previews = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "code-preview", &mut previews);
    assert_eq!(previews.len(), 1, "the script's code, under its fields");
    let code = editor.ui.text(previews[0]);
    assert!(code.contains("public sealed class Spinner : Behaviour"), "{code}");
    assert!(!code.contains('\t'), "its tabs shown as spaces");
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
    // The scripts' own lines too, which come through tracing from C#.
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let Some((mut renderer, mut editor)) = open_logging(log.clone()) else { return };
    let frame = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer| {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    };
    let spinner = obj(&editor, "Spinner A").expect("Hello has Spinner A");

    // A scripts' [Command] from the palette: the scripts build and start for it, it runs on the edited level,
    // and what it changed is one step to undo.
    if !described(&mut editor, &mut renderer) {
        return;
    }
    let other = obj(&editor, "Spinner B").unwrap();
    let at = |editor: &voxear_editor::Editor, e| editor.game.engine.world.get::<Transform>(e).unwrap().position;
    let (a_was, b_was) = (at(&editor, spinner), at(&editor, other));
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("swap spin");
    frame(&mut editor, &mut renderer);
    assert_eq!(
        palette_results(&editor.ui).first().map(String::as_str),
        Some("Swap Spinners"),
        "Hello's Tools.cs adds it"
    );
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    let started = std::time::Instant::now();
    while at(&editor, spinner) == a_was {
        assert!(started.elapsed().as_secs() < 300, "the command never ran: {:?}", editor.play_error());
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!((at(&editor, spinner), at(&editor, other)), (b_was, a_was), "the spinners swapped places");
    assert!(!editor.is_playing(), "a command runs on the edited level, without playing");
    editor.undo(&mut renderer);
    assert_eq!((at(&editor, spinner), at(&editor, other)), (a_was, b_was), "and one undo swaps them back");

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

    // The Profiler, open while playing, has each behaviour's callbacks by class and method.
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.open_dock_panel(dock, "profiler");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let sampled = &editor.profiler().samples().back().expect("frames are sampled while the Profiler shows").cpu;
    let update = sampled.find("Update").unwrap();
    let spinners =
        sampled.nodes[update].children.iter().copied().find(|&c| sampled.name(c) == "Hello.Spinner.Update()");
    let spinners = spinners.expect("Spinner's Update, inside the game's Update");
    assert_eq!(sampled.nodes[spinners].calls, 3, "Spinner A's, B's and the Pyramid prefab's, as one row");
    editor.ui.close_dock_panel(dock, "profiler");
    frame(&mut editor, &mut renderer);
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

    // Error Pause, from the Console's toolbar: an error logged in play pauses it.
    editor.set_paused(false);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.show_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    let pause = console_button(&editor.ui, "Error Pause").unwrap();
    click(&mut editor.ui, pause);
    frame(&mut editor, &mut renderer);
    log_line(&log, voxear_core::logging::LogLine::new(tracing::Level::WARN, "only a warning"));
    frame(&mut editor, &mut renderer);
    assert!(!editor.is_paused(), "a warning plays on");
    log_line(&log, voxear_core::logging::LogLine::new(tracing::Level::ERROR, "it broke"));
    frame(&mut editor, &mut renderer);
    assert!(editor.is_paused(), "an error pauses play");

    // A script's line, with the stack .NET wrote for it, read into frames:
    // its own first, with its file and line, then the engine's folded.
    search_console(&mut editor, &mut renderer, "awake (spinner)");
    let lines = console_lines(&editor.ui);
    assert!(lines.iter().any(|l| l == "Awake    Spinner A (Spinner)"), "Hello's Spinner logs as it wakes: {lines:?}");
    let row = console_rows(&editor.ui)[0];
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut traces = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-trace", &mut traces);
    let trace = editor.ui.text(traces[0]);
    assert!(trace.starts_with("Hello.Spinner.Awake()  Spinner.cs:19\n"), "{trace}");
    assert!(trace.contains("not the project's own"), "the engine's frames fold: {trace}");
    shot(&mut renderer, "console_play.png");

    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!editor.is_playing());
    assert_eq!(obj(&editor, "Spinner A"), Some(spinner), "the edited level is back");
    assert_eq!(*editor.game.engine.world.get::<Transform>(spinner).unwrap(), edited, "as it was edited");
    assert!(editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).is_some(), "with no script running");

    // A second run reuses the scripts, since .NET starts only once. A value
    // marked to keep comes back into the edited level when play stops.
    editor.add_scene_view("scene");
    let edit_cameras = editor.scene_view_cameras();
    editor.play(&mut renderer);
    assert!(editor.is_playing(), "plays at once, with the scripts already built");
    frame(&mut editor, &mut renderer);
    // Each Scene view flies a camera of its own in the world playing, and takes back where it flew to.
    let played_cameras = editor.scene_view_cameras();
    assert_eq!(played_cameras.len(), 2);
    for (_, played) in &played_cameras {
        let named = editor.game.engine.world.get::<Name>(*played).map(|name| name.0.as_str());
        assert_eq!(named, Some("Scene Camera"), "a camera in the world playing");
    }
    let flown_to = voxear_core::math::Vec3::new(4.0, 3.0, -6.0);
    editor.game.engine.world.get_mut::<Transform>(played_cameras[1].1).unwrap().position = flown_to;
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
    assert_eq!(editor.scene_view_cameras(), edit_cameras, "each Scene view looks through its edited camera again");
    let second = editor.game.engine.world.get::<Transform>(edit_cameras[1].1).unwrap();
    assert_eq!(second.position, flown_to, "put where it flew to while playing");
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
fn voxdocs_reads_the_projects_docs() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "docs");
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    let intro = label_starting(&editor.ui, root, "Hello's own notes").expect("VoxDocs opens on Hello's Docs");
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
        let file = std::fs::File::create(std::path::Path::new(&dir).join("docs.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}

#[test]
fn the_levels_panel_lists_levels_and_opens_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    // The rail's Levels button opens it in the drawer.
    let rail = editor.ui.find(root, "rail-levels").unwrap();
    click(&mut editor.ui, rail);
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    assert_eq!(editor.drawer(), Some(voxear_editor::Drawer::Levels));
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
    // Without its scripts: the editor would build them in the background to describe them, and a build outlives
    // the test and its copy, holding up every other build until it is stopped.
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, project.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
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

#[test]
fn dragging_an_axis_letter_changes_that_axis_alone() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let selected = editor.first_mesh().unwrap();
    let start = editor.game.engine.world.get::<Transform>(selected).unwrap().position;
    editor.select(Some(selected));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let mut letters = Vec::new();
    with_class(&editor.ui, prop_row(&editor.ui, "Position").unwrap(), "axis-letter", &mut letters);
    let y = letters.into_iter().find(|&l| editor.ui.text(l) == "Y").expect("a Y on the Position row");
    let [left, top, _, height] = editor.ui.rect(y);
    let ui = &mut editor.ui;
    ui.pointer_move(left + 2.0, top + height / 2.0);
    assert!(ui.wants_scrub_cursor(), "a left-right cursor over the letter");
    ui.pointer_button(true);
    ui.pointer_move(left + 102.0, top + height / 2.0);
    ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let moved = editor.game.engine.world.get::<Transform>(selected).unwrap().position;
    assert!(moved.y > start.y + 1.0, "dragged right, Y goes up: {moved} from {start}");
    assert_eq!((moved.x, moved.z), (start.x, start.z), "and X and Z stay");
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

    // The square between X and Y, on the sides facing the camera, moves along both and not along Z.
    let facing = |axis: Vec3| if axis.dot(camera.position - start) >= 0.0 { axis } else { -axis };
    let square = start + (facing(Vec3::X) + facing(Vec3::Y)) * camera.world_size(start, 84.0) * 0.325;
    let grip = camera.project(square).unwrap();
    pointer_to(&mut editor, grip);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "move-handles.png");
    left_button(&mut editor, true);
    frame(&mut editor, &mut renderer);
    pointer_to(&mut editor, [grip[0] + 40.0, grip[1] - 30.0]);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    let moved = position(&editor);
    assert!((moved.x - start.x).abs() > 0.05 && (moved.y - start.y).abs() > 0.05, "along X and Y: {start} to {moved}");
    assert!((moved.z - start.z).abs() < 1e-4, "and not along Z: {moved}");
    editor.undo(&mut renderer);

    // Rotate and scale draw theirs too: rotate's view ring and ball, scale's squares.
    let tool = editor.ui.find(editor.ui.root(), "tool").expect("the tool strip");
    pointer_to(&mut editor, [10.0, 10.0]);
    for (index, name) in [(2, "rotate-handles.png"), (3, "scale-handles.png")] {
        editor.ui.set_segment(tool, index);
        frame(&mut editor, &mut renderer);
        frame(&mut editor, &mut renderer);
        shot(&mut renderer, name);
    }
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

/// A folder package's script adds its Lantern prefab to Create Object, as
/// a build of it describes it.
const LANTERN_OBJECTS: &str = r##"{"behaviours": [], "types": {}, "commands": [],
  "create_objects": [{"menu": "Lights/Lantern", "prefab": "Prefabs/Lantern.prefab", "assembly": "LanternKit"}]}"##;

#[test]
fn a_packages_prefab_is_made_from_create_object_as_plain_objs() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let kit = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::write(kit.path().join("package.toml"), "[package]\nname = \"lantern-kit\"\nversion = \"1.0.0\"\nengine = \">=0.1\"\n")
        .unwrap();
    std::fs::create_dir_all(kit.path().join("Prefabs")).unwrap();
    let lantern = "format = 1\n[[obj]]\nid = 1\nname = \"Lantern\"\n[[obj]]\nid = 2\nname = \"Flame\"\nparent = 1\n\
                   position = [0.0, 0.5, 0.0]\nlight = { type = \"point\" }\n";
    std::fs::write(kit.path().join("Prefabs/Lantern.prefab"), lantern).unwrap();
    let packages = format!("[packages]\nlantern-kit = {{ path = {:?} }}\n", kit.path().display().to_string());
    std::fs::write(dir.path().join("packages.toml"), packages).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(LANTERN_OBJECTS).unwrap());
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // The + menu offers it after the built-in Objs, under its folder's name.
    let add = editor.ui.find(editor.ui.root(), "hierarchy-add").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Lights").is_some() && menu_item(&editor.ui, "Lantern").is_some(), "in the + menu");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);

    // Made from the palette's New Lantern: the prefab's Objs, as plain Objs, selected, as one step to undo.
    run_command(&mut editor, &mut renderer, "New Lantern");
    let made = obj(&editor, "Lantern").expect("the Lantern was made");
    let flame = obj(&editor, "Flame").expect("with its Flame");
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<voxear_scene::Parent>(flame).map(|p| p.0), Some(made), "inside it");
    for entity in [made, flame] {
        assert!(!world.has::<voxear_engine::prefab::PrefabInstance>(entity), "no instance of the prefab");
        assert!(!world.has::<voxear_engine::prefab::FromPrefab>(entity), "nor part of one");
    }
    assert_eq!(editor.selected(), Some(made));
    assert!(editor.has_unsaved_changes());
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(obj(&editor, "Lantern").is_none() && obj(&editor, "Flame").is_none(), "one undo takes both");
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
    // As in Unity: a click pings what it points at, a double click selects it, and the arrow picks another.
    let label = first_with(&editor.ui, picker, "picker-label").unwrap();
    click(&mut editor.ui, label);
    frame(&mut editor, &mut renderer);
    assert_eq!(pinged_names(&editor.ui), ["Spinner A"], "its row is lit in the Hierarchy");
    assert!(!editor.picker_open() && editor.selected() == Some(spinner), "nothing else happens");
    click(&mut editor.ui, label);
    click(&mut editor.ui, label);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), obj(&editor, "Spinner A"), "a double click selects it");
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let picker = first_with(&editor.ui, prop_row(&editor.ui, "Leader").unwrap(), "value-picker").unwrap();
    let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "reference-picker.png");
    let offered = picker_rows(&editor.ui);
    assert!(offered.iter().any(|(label, _)| label == "Spinner B"), "Objs with a Spinner are offered: {offered:?}");
    assert!(!offered.iter().any(|(label, _)| label == "Ground"), "and only those");
    assert_eq!(offered[0].0, "None");
    let target = picker_row(&editor.ui, "None").unwrap();
    click(&mut editor.ui, target);
    frame(&mut editor, &mut renderer);
    assert!(!editor.picker_open(), "a click picks, and closes it");
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
    let items = |ui: &Ui| first_with(ui, collection_of(ui, prop_row(ui, "Cycle").unwrap()), "collection-items").unwrap();
    assert_eq!(editor.ui.children(items(&editor.ui)).len(), 2, "in a collection field under its row: {cycle:?}");
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
    // Each item's Level is a reference to a level, not a path to type: the picker offers the project's levels.
    let level = first_with(&editor.ui, prop_rows(&editor.ui, "Level")[0], "value-picker").expect("a level picker");
    assert_eq!(editor.ui.text(first_with(&editor.ui, level, "picker-label").unwrap()), "Night.level");
    let arrow = first_with(&editor.ui, level, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert!(
        offered.contains(&"World.level".to_string()) && !offered.iter().any(|o| o.ends_with(".scene")),
        "{offered:?}"
    );
    let world_row = picker_row(&editor.ui, "World.level").unwrap();
    click(&mut editor.ui, world_row);
    frame(&mut editor, &mut renderer);
    let picked = cycle_now(&editor)[0].get("level").and_then(|l| l.as_str().map(str::to_string)).unwrap_or_default();
    assert!(picked.ends_with("World.level"), "picked by its path: {picked}");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);

    let field = first_with(&editor.ui, prop_rows(&editor.ui, "Seconds")[1], "value-input").unwrap();
    type_into(&mut editor.ui, field, "7");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(cycle_now(&editor)[1].get("seconds"), Some(&toml::Value::Float(7.0)), "a value inside an item");

    // An item dragged by its grip below the next swaps with it, the rows easing into place, and undoes.
    let before = cycle_now(&editor);
    let grip = |ui: &Ui, at: usize| first_with(ui, ui.children(items(ui))[at], "collection-grip").unwrap();
    let from = centre(&editor.ui, grip(&editor.ui, 0));
    let [_, _, _, height] = editor.ui.rect(editor.ui.children(items(&editor.ui))[0]);
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=4 {
        editor.ui.pointer_move(from[0], from[1] + height * 0.3 * step as f32);
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "inspector-list-dragging.png");
    let lifted = editor.ui.children(items(&editor.ui))[0];
    assert!(editor.ui.has_class(lifted, "reorder-lifted") && editor.ui.slide(lifted) > height, "it follows the pointer");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert_eq!(cycle_now(&editor), vec![before[1].clone(), before[0].clone()], "swapped");
    assert_eq!(count(&editor), "2 items", "the rows stay, showing the moved items");
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.ui.children(items(&editor.ui)).iter().all(|&item| editor.ui.slide(item) == 0.0), "settled");
    // Edits this close together are one step, so this undoes the 7 above too.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let levels = |cycle: Vec<toml::Value>| cycle.iter().map(|stop| stop.get("level").cloned()).collect::<Vec<_>>();
    assert_eq!(levels(cycle_now(&editor)), levels(before), "undone");

    // The collection field's add button grows the list, and an item's remove button shrinks it.
    let collection = collection_of(&editor.ui, prop_row(&editor.ui, "Cycle").unwrap());
    let add = first_with(&editor.ui, collection, "collection-add").expect("the list can grow");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(count(&editor), "3 items");
    assert_eq!(cycle_now(&editor).len(), 3);
    let last = prop_row(&editor.ui, "Element 2").expect("the new item shows");
    let item = editor.ui.parent(editor.ui.parent(last).unwrap()).unwrap();
    assert!(editor.ui.has_class(item, "collection-item"), "an item of its own");
    let remove = first_with(&editor.ui, item, "collection-remove").unwrap();
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
    add_component(&mut editor, &mut renderer, "Light");
    let light = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<voxear_render::Light>(spinner).copied();
    assert!(light(&editor).is_some(), "the light was added");
    let intensity = prop_field(&editor.ui, "Intensity").expect("and shows its values");
    type_into(&mut editor.ui, intensity, "2.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(light(&editor).unwrap().intensity, 2.5);
    let shadows = first_with(&editor.ui, prop_row(&editor.ui, "Shadows").expect("a local light's Shadows"), "checkbox");
    assert!(light(&editor).unwrap().shadows, "on for a new light");
    click(&mut editor.ui, shadows.unwrap());
    frame(&mut editor, &mut renderer);
    assert!(!light(&editor).unwrap().shadows, "its toggle turns them off");
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
    add_component(&mut editor, &mut renderer, "FireflySpawner");
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

#[test]
fn a_model_opens_out_to_its_parts_and_a_field_takes_one() {
    let Some((mut renderer, mut editor)) = open() else { return };
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let folder = tile(&editor.ui, "Models").expect("the Project shows its folders as tiles");
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // The model's arrow opens it out, after it.
    let model = tile(&editor.ui, "Pyramid.gltf").expect("the model shows");
    let arrow = first_with(&editor.ui, model, "part-arrow").expect("a model has an arrow");
    assert!(editor.ui.is_visible(arrow));
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut parts = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "part", &mut parts);
    let names: Vec<String> = parts
        .into_iter()
        .filter(|&part| editor.ui.is_visible(part) && editor.ui.has_class(part, "tile"))
        .map(|part| editor.ui.text(editor.ui.children(part)[1]))
        .collect();
    assert!(
        names.len() >= 2 && names.last().map(String::as_str) == Some("Checker"),
        "its meshes, then its material, by its name: {names:?}"
    );
    assert_eq!(editor.selected(), None, "opening it out selects nothing");

    // A mesh of it, dragged onto a Mesh Renderer's Mesh, draws that.
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mesh = tile(&editor.ui, &names[0]).expect("the mesh's tile");
    let field = first_with(&editor.ui, prop_row(&editor.ui, "Mesh").unwrap(), "value-picker").unwrap();
    drag(&mut editor.ui, mesh, field);
    frame(&mut editor, &mut renderer);
    let renderer_part = editor.game.engine.world.get::<voxear_render::MeshRenderer>(spinner).unwrap().mesh;
    let reference = editor.game.content.mesh_ref(renderer_part).map(|r| r.to_string()).unwrap_or_default();
    assert!(
        reference.starts_with("277e6fd45f82419a894b65b6b72ed398#mesh"),
        "the Pyramid's mesh, by its model's GUID: {reference}"
    );
}

#[test]
fn a_models_import_settings_apply_where_it_is_drawn() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mesh = pyramid(&editor);
    let height = |renderer: &Renderer| renderer.mesh_bounds(mesh).map(|(min, max)| max.y - min.y).unwrap();
    let before = height(&renderer);

    open_models(&mut editor, &mut renderer);
    let model = tile(&editor.ui, "Pyramid.gltf").expect("the model shows");
    click(&mut editor.ui, model);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let scale = prop_field(&editor.ui, "Scale").expect("a model shows its import settings");
    assert_eq!(editor.ui.text(scale), "1");
    type_into(&mut editor.ui, scale, "2");
    frame(&mut editor, &mut renderer);
    assert_eq!(height(&renderer), before, "nothing changes before Apply");
    let apply = editor.ui.find(editor.ui.root(), "import-apply").unwrap();
    scroll_to(&mut editor, &mut renderer, apply);
    click(&mut editor.ui, apply);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let meta = std::fs::read_to_string(dir.path().join("Models/Pyramid.gltf.meta")).unwrap();
    assert!(meta.contains("[model]") && meta.contains("scale = 2.0"), "Apply writes them to the .meta: {meta}");
    assert_eq!(pyramid(&editor), mesh, "the Obj draws the same mesh");
    assert!(
        (height(&renderer) - before * 2.0).abs() < 1e-4,
        "now twice as tall: {} against {before}",
        height(&renderer)
    );

    // Revert puts back what was applied.
    type_into(&mut editor.ui, scale, "5");
    frame(&mut editor, &mut renderer);
    let revert = editor.ui.find(editor.ui.root(), "import-revert").unwrap();
    scroll_to(&mut editor, &mut renderer, revert);
    click(&mut editor.ui, revert);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(scale), "2");
}

#[test]
fn a_pictures_import_settings_apply_to_the_models_that_read_it() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    // The Pyramid reads its picture from a file of its own beside it.
    let model = dir.path().join("Models/Pyramid.gltf");
    let gltf = std::fs::read_to_string(&model).unwrap();
    let start = gltf.find("data:image/png;base64,").unwrap();
    let end = start + gltf[start..].find('"').unwrap();
    std::fs::write(&model, format!("{}Bricks.png{}", &gltf[..start], &gltf[end..])).unwrap();
    image::RgbaImage::from_pixel(64, 32, image::Rgba([200, 90, 40, 255]))
        .save(dir.path().join("Models/Bricks.png"))
        .unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_models(&mut editor, &mut renderer);
    let picture = tile(&editor.ui, "Bricks.png").expect("the picture shows");
    click(&mut editor.ui, picture);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let pixels = prop_row(&editor.ui, "Pixels").expect("a picture shows its size");
    assert_eq!(editor.ui.text(editor.ui.children(pixels)[1]), "64 × 32");
    let max_size = first_with(&editor.ui, prop_row(&editor.ui, "Max size").unwrap(), "value-choice").unwrap();
    assert_eq!(editor.ui.selected_option(max_size), 0, "its own size, to start with");
    choose(&mut editor, &mut renderer, max_size, 9);
    let flip = editor.ui.children(prop_row(&editor.ui, "Flip Green").expect("a Flip Green row"))[1];
    click(&mut editor.ui, flip);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_checked(flip));
    let apply = editor.ui.find(editor.ui.root(), "import-apply").unwrap();
    click(&mut editor.ui, apply);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let meta = std::fs::read_to_string(dir.path().join("Models/Bricks.png.meta")).unwrap();
    assert!(meta.contains("[texture]") && meta.contains("max_size = 32"), "Apply writes them to the .meta: {meta}");
    assert!(meta.contains("flip_green = true"), "{meta}");
    let status = editor.ui.text(editor.ui.find(editor.ui.root(), "status-left").unwrap());
    assert!(status.contains("Imported Pyramid.gltf again"), "the model reading it is imported again: {status}");
}

/// A WAV file holding a second of one level, mono at 48 kHz.
fn chime() -> Vec<u8> {
    let mut chime = Vec::new();
    chime.extend_from_slice(b"RIFF");
    chime.extend_from_slice(&(36u32 + 96_000).to_le_bytes());
    chime.extend_from_slice(b"WAVEfmt ");
    for value in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
        chime.extend_from_slice(&value);
    }
    chime.extend_from_slice(b"data");
    chime.extend_from_slice(&96_000u32.to_le_bytes());
    chime.resize(chime.len() + 96_000, 0x20);
    chime
}

#[test]
fn a_sound_shows_what_it_holds_plays_in_the_editor_and_goes_on_an_audio_source() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Models/Chime.wav"), chime()).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    // Silent, rather than through the machine's speakers.
    editor.game.audio = voxear_engine::audio::GameAudio::new(voxear_engine::audio::Audio::quiet());
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_models(&mut editor, &mut renderer);
    let sound = tile(&editor.ui, "Chime.wav").expect("the sound shows");
    click(&mut editor.ui, sound);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let value = |editor: &voxear_editor::Editor, label: &str| {
        let row = prop_row(&editor.ui, label).unwrap_or_else(|| panic!("a sound shows its {label}"));
        editor.ui.text(editor.ui.children(row)[1]).to_string()
    };
    assert_eq!(value(&editor, "Kind"), "Sound");
    assert_eq!(value(&editor, "Length"), "0:01.0");
    assert_eq!(value(&editor, "Channels"), "Mono");
    assert_eq!(value(&editor, "Sample rate"), "48000 Hz");
    shot(&mut renderer, "inspector-sound.png");

    let listen = editor.ui.find(editor.ui.root(), "listen").expect("a button to listen to it");
    let words = |editor: &voxear_editor::Editor| editor.ui.text(editor.ui.children(listen)[1]).to_string();
    assert_eq!(words(&editor), "Play");
    click(&mut editor.ui, listen);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.audio.previewing(), Some("Models/Chime.wav"), "it plays in the editor");
    assert_eq!(words(&editor), "Stop");
    click(&mut editor.ui, listen);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.audio.previewing(), None);
    assert_eq!(words(&editor), "Play");

    let load = first_with(&editor.ui, prop_row(&editor.ui, "Load").unwrap(), "value-choice").unwrap();
    assert_eq!(editor.ui.selected_option(load), 0, "by its length, to start with");
    choose(&mut editor, &mut renderer, load, 2);
    let apply = editor.ui.find(editor.ui.root(), "import-apply").unwrap();
    click(&mut editor.ui, apply);
    frame(&mut editor, &mut renderer);
    let meta = std::fs::read_to_string(dir.path().join("Models/Chime.wav.meta")).unwrap();
    assert!(meta.contains("[sound]") && meta.contains("load = \"stream\""), "Apply writes them to the .meta: {meta}");

    // Add Component gives an Obj an Audio Source, whose values the Inspector edits and undo takes back.
    let ground = obj(&editor, "Ground").unwrap();
    editor.select(Some(ground));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Audio Source");
    let source = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<voxear_engine::audio::AudioSource>(ground).copied()
    };
    assert!(source(&editor).is_some_and(|s| s.sound.is_none() && s.play_on_start), "added, with no sound yet");
    let picked = first_with(&editor.ui, prop_row(&editor.ui, "Sound").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(picked), "None");
    let volume = prop_field(&editor.ui, "Volume").expect("its volume shows");
    type_into(&mut editor.ui, volume, "0.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(source(&editor).unwrap().volume, 0.5);
    assert!(prop_row(&editor.ui, "Max distance").is_some(), "3D, so it shows how far it carries");
    shot(&mut renderer, "inspector-audio-source.png");
    editor.undo(&mut renderer);
    assert_eq!(source(&editor).unwrap().volume, 1.0, "undo takes the volume back");
}

/// The mesh the Day scene draws the Pyramid model with.
fn pyramid(editor: &voxear_editor::Editor) -> voxear_render::MeshHandle {
    let world = &editor.game.engine.world;
    let mut drawn = world.query_ref::<&voxear_render::MeshRenderer>().into_iter().map(|r| r.mesh).filter(|&mesh| {
        let reference = editor.game.content.mesh_ref(mesh).map(|r| r.to_string()).unwrap_or_default();
        reference.starts_with("277e6fd45f82419a894b65b6b72ed398")
    });
    drawn.next().expect("the Day scene draws the Pyramid")
}

/// Opens the Models folder in the Project panel, by double-clicking its tile.
fn open_models(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    let folder = tile(&editor.ui, "Models").expect("the Project shows its folders as tiles");
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn files_from_outside_come_in_when_dropped_or_when_the_editor_gets_the_focus() {
    use winit::event::WindowEvent;
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_models(&mut editor, &mut renderer);

    // A model dropped from a file manager over the files goes into the folder shown.
    let outside = tempfile::tempdir().unwrap();
    let rock = outside.path().join("Rock.gltf");
    std::fs::copy(dir.path().join("Models/Pyramid.gltf"), &rock).unwrap();
    let [x, y, w, h] = editor.ui.rect(tile(&editor.ui, "Pyramid.gltf").unwrap());
    pointer_to(&mut editor, [x + w / 2.0, y + h / 2.0]);
    editor.handle(&WindowEvent::DroppedFile(rock));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Models/Rock.gltf.meta").is_file(), "copied in, and given a GUID");
    assert!(tile(&editor.ui, "Rock.gltf").is_some(), "the Project panel shows it");
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Models/Rock.gltf"), "selected");

    // Changed while the editor was away: a model in use is imported again, and a new file shows.
    let mesh = pyramid(&editor);
    let height = |renderer: &Renderer| renderer.mesh_bounds(mesh).map(|(min, max)| max.y - min.y).unwrap();
    let before = height(&renderer);
    let meta = dir.path().join("Models/Pyramid.gltf.meta");
    let text = std::fs::read_to_string(&meta).unwrap();
    std::fs::write(&meta, format!("{text}\n[model]\nscale = 3.0\n")).unwrap();
    std::fs::copy(dir.path().join("Models/Pyramid.gltf"), dir.path().join("Models/Stone.gltf")).unwrap();
    frame(&mut editor, &mut renderer);
    assert_eq!(height(&renderer), before, "nothing is looked at while the editor is away");
    editor.handle(&WindowEvent::Focused(false));
    editor.handle(&WindowEvent::Focused(true));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!((height(&renderer) - before * 3.0).abs() < 1e-4, "imported again: {} against {before}", height(&renderer));
    assert_eq!(pyramid(&editor), mesh, "behind the same mesh");
    assert!(tile(&editor.ui, "Stone.gltf").is_some(), "the new file shows");
}

/// The command palette's results showing, top to bottom, by title.
fn palette_results(ui: &Ui) -> Vec<String> {
    let list = ui.find(ui.root(), "palette-list").unwrap();
    let mut rows: Vec<NodeId> =
        ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows.into_iter().filter_map(|row| first_with(ui, row, "palette-title").map(|t| ui.text(t))).collect()
}

#[test]
fn the_command_palette_finds_and_runs_things() {
    use voxear_editor::PaletteKey;
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let (field, palette) = (editor.ui.find(root, "command").unwrap(), editor.ui.find(root, "palette").unwrap());
    assert!(!editor.ui.is_visible(palette), "closed until the command bar has focus");

    editor.ui.focus(field);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_visible(palette));
    assert_eq!(
        palette_results(&editor.ui).first().map(String::as_str),
        Some("Play"),
        "with nothing typed, the commands"
    );
    shot(&mut renderer, "palette-commands.png");

    editor.ui.text_input("spin b");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "palette.png");
    assert_eq!(
        palette_results(&editor.ui).first().map(String::as_str),
        Some("Spinner B"),
        "{:?}",
        palette_results(&editor.ui)
    );
    editor.palette_key(PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), obj(&editor, "Spinner B"), "Enter selects the Obj");
    assert!(!editor.ui.is_visible(palette) && editor.ui.text(field).is_empty(), "and the palette closes, empty");

    // Down moves the highlight; a click runs a result too.
    editor.ui.focus(field);
    editor.ui.text_input("tool");
    frame(&mut editor, &mut renderer);
    // The four tools lead, with the manual's page on tool kinds among them.
    let results = palette_results(&editor.ui);
    assert_eq!(results.iter().take(5).filter(|r| r.ends_with("tool")).count(), 4, "{results:?}");
    let list = editor.ui.find(root, "palette-list").unwrap();
    let rotate = results.iter().position(|r| r == "Rotate tool").unwrap();
    let mut rows: Vec<NodeId> = editor
        .ui
        .children(list)
        .into_iter()
        .filter(|&r| editor.ui.has_class(r, "list-row") && editor.ui.is_visible(r))
        .collect();
    rows.sort_by(|a, b| editor.ui.rect(*a)[1].total_cmp(&editor.ui.rect(*b)[1]));
    click(&mut editor.ui, rows[rotate]);
    frame(&mut editor, &mut renderer);
    let tool = editor.ui.find(root, "tool").unwrap();
    assert_eq!(editor.ui.segment(tool), 2, "the Rotate tool is picked");

    editor.ui.focus(field);
    editor.ui.text_input("zzzz");
    frame(&mut editor, &mut renderer);
    assert!(palette_results(&editor.ui).is_empty());
    assert!(editor.ui.is_visible(editor.ui.find(root, "palette-empty").unwrap()), "and says so");
    editor.palette_key(PaletteKey::Escape);
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.is_visible(palette) && editor.ui.focused() != Some(field), "Escape closes it");
    assert!(editor.ui.text(field).is_empty(), "and empties the bar");
}

#[test]
fn settings_and_recent_palette_items_are_kept_per_project() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let open = |renderer: &mut Renderer| {
        let project = Project::open(dir.path()).unwrap();
        let mut editor = voxear_editor::Editor::open(project, renderer, LogCapture::default()).unwrap();
        frame(&mut editor, renderer);
        editor
    };
    let mut editor = open(&mut renderer);
    let root = editor.ui.root();
    assert_eq!(editor.settings().snap, 0.25);

    // The tool strip's snap button is the Snap distance setting.
    let snap = editor.ui.find(root, "snap").unwrap();
    click(&mut editor.ui, snap);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.settings().snap, 0.5);

    // The palette flips a toggle, and remembers that it ran it.
    let field = editor.ui.find(root, "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("show stats");
    frame(&mut editor, &mut renderer);
    assert_eq!(palette_results(&editor.ui).first().map(String::as_str), Some("Show stats"));
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    let stats = editor.ui.find(root, "scene-stats").expect("the stats chip");
    assert!(!editor.settings().show_stats && !editor.ui.is_visible(stats), "the stats chip goes");

    // A number opens the Settings panel at it, to type.
    editor.ui.focus(field);
    editor.ui.text_input("fly speed");
    frame(&mut editor, &mut renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    // A frame to bring the Settings tab to the front, and one to show the setting in it.
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let speed = editor.ui.focused().expect("the Fly speed field has focus");
    type_into(&mut editor.ui, speed, "12");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "settings.png");
    editor.ui.blur();
    assert_eq!(editor.settings().fly_speed, 12.0);
    assert_eq!(editor.fly().speed(), 12.0, "the Scene camera flies at it");
    drop(editor);

    // Opened again, all of it is as it was left, and the palette leads with what ran last.
    let mut editor = open(&mut renderer);
    let settings = editor.settings();
    assert!((settings.snap, settings.show_stats, settings.fly_speed) == (0.5, false, 12.0), "{settings:?}");
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    frame(&mut editor, &mut renderer);
    let results = palette_results(&editor.ui);
    assert_eq!(results[..2], ["Fly speed".to_string(), "Show stats".to_string()], "most recent first: {results:?}");
}

#[test]
fn code_built_on_the_editor_adds_palette_commands() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let ran = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = ran.clone();
    editor.add_command(voxear_editor::PaletteCommand {
        id: "tests.count".into(),
        title: "Count Runs".into(),
        keys: "Ctrl+Shift+J".into(),
        icon: "code",
        run: std::rc::Rc::new(move |editor: &mut voxear_editor::Editor| {
            counted.set(counted.get() + 1);
            editor.select(None);
        }),
    });
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("count runs");
    frame(&mut editor, &mut renderer);
    let results = palette_results(&editor.ui);
    assert_eq!(results.first().map(String::as_str), Some("Count Runs"), "{results:?}");
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    assert_eq!(ran.get(), 1, "it ran, with the editor to hand");
}

/// The Hierarchy's rows showing, top to bottom, by name.
fn hierarchy_names(ui: &Ui) -> Vec<String> {
    let tree = ui.find(ui.root(), "hierarchy-tree").unwrap();
    let mut rows: Vec<NodeId> =
        ui.children(tree).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows.into_iter().filter_map(|row| first_with(ui, row, "row-name").map(|n| ui.text(n))).collect()
}

#[test]
fn the_hierarchy_shows_scenes_in_their_order_and_reorders_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    frame(&mut editor, &mut renderer);
    let names = hierarchy_names(&editor.ui);
    assert_eq!(names[..3], ["Environment".to_string(), "Main Camera".into(), "Ground".into()], "{names:?}");
    let day = names.iter().position(|n| n == "Day").expect("each scene has a header");
    assert_eq!(names[day + 1..day + 4], ["Sun".to_string(), "Spinner A".into(), "Spinner B".into()], "{names:?}");
    let header = hierarchy_row(&editor.ui, "Day").unwrap();
    assert!(
        first_with(&editor.ui, header, "scene-header").is_some()
            || editor.ui.has_class(editor.ui.children(header)[2], "scene-header")
    );

    // Spinner B dragged to the top edge of Sun goes before it.
    let (from, onto) = (hierarchy_row(&editor.ui, "Spinner B").unwrap(), hierarchy_row(&editor.ui, "Sun").unwrap());
    let ([fx, fy, fw, fh], [ox, oy, ow, oh]) = (editor.ui.rect(from), editor.ui.rect(onto));
    editor.ui.pointer_move(fx + fw / 2.0, fy + fh / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_move(ox + ow / 2.0, oy + oh * 0.1);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "hierarchy-before.png");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let names = hierarchy_names(&editor.ui);
    let day = names.iter().position(|n| n == "Day").unwrap();
    assert_eq!(names[day + 1..day + 4], ["Spinner B".to_string(), "Sun".into(), "Spinner A".into()], "{names:?}");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let names = hierarchy_names(&editor.ui);
    assert_eq!(
        names[day + 1..day + 4],
        ["Sun".to_string(), "Spinner A".into(), "Spinner B".into()],
        "undone: {names:?}"
    );

    // The GameManager is an IInstance, so its row has a badge, once the scripts are described.
    if !described(&mut editor, &mut renderer) {
        return;
    }
    frame(&mut editor, &mut renderer);
    let manager = hierarchy_row(&editor.ui, "Game Manager").unwrap();
    let badge = first_with(&editor.ui, manager, "row-badge").unwrap();
    assert!(editor.ui.is_visible(badge) && editor.ui.text(badge) == "Instance");
    shot(&mut renderer, "hierarchy-scenes.png");
}

/// Logs a line as the engine would, for the Console to take in.
fn log_line(log: &LogCapture, line: voxear_core::logging::LogLine) {
    let mut log = log.lock().unwrap();
    log.lines.push_back(line);
    log.total += 1;
}

/// A clipboard the test can read back.
#[derive(Clone, Default)]
struct SharedClipboard(std::rc::Rc<std::cell::RefCell<Option<String>>>);

impl voxear_ui::Clipboard for SharedClipboard {
    fn read(&mut self) -> Option<String> {
        self.0.borrow().clone()
    }

    fn write(&mut self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_string());
    }
}

/// The Console's list of lines.
fn console_list(ui: &Ui) -> NodeId {
    let mut lists = Vec::new();
    with_class(ui, ui.root(), "console", &mut lists);
    lists[0]
}

/// The Console's rows on screen, top to bottom.
fn console_rows(ui: &Ui) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, console_list(ui), "list-row", &mut rows);
    rows.retain(|&row| ui.is_visible(row));
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows
}

/// The messages of the Console's rows on screen, top to bottom.
fn console_lines(ui: &Ui) -> Vec<String> {
    console_rows(ui).into_iter().map(|row| ui.text(ui.children(row)[3])).collect()
}

/// A Console button, in its toolbar or over its trace, that says `label`.
fn console_button(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut buttons = Vec::new();
    let console = ui.find(ui.root(), "console-panel")?;
    with_class(ui, console, "console-button", &mut buttons);
    buttons.into_iter().find(|&b| ui.text(b) == label || ui.children(b).iter().any(|&c| ui.text(c) == label))
}

/// Each of a class's elements' second child's text: the Console's level
/// counts, or its chips.
fn second_texts(ui: &Ui, class: &str) -> Vec<String> {
    let mut found = Vec::new();
    with_class(ui, ui.root(), class, &mut found);
    found.into_iter().map(|node| ui.text(ui.children(node)[1])).collect()
}

fn search_console(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, text: &str) {
    let mut fields = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-search", &mut fields);
    let field = editor.ui.children(fields[0])[1];
    editor.ui.focus(field);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.key_down(Key::Backspace);
    editor.ui.text_input(text);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// The open menu's items, each as its label and then any shortcut or count.
fn menu_texts(ui: &Ui) -> Vec<String> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items
        .into_iter()
        .map(|item| {
            ui.children(item).iter().map(|&part| ui.text(part)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// Clicks a link in a rich label, where it is drawn.
fn click_link(ui: &mut Ui, label: NodeId, url: &str) {
    let [x, y, w, h] = ui.link_rect(label, url).unwrap_or_else(|| panic!("{url} is drawn in {:?}", ui.text(label)));
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// Runs the command bar's first result for `words`.
fn run_command(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, words: &str) {
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input(words);
    frame(editor, renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// The names on the rows and tiles a log line has lit.
fn pinged_names(ui: &Ui) -> Vec<String> {
    let mut lit = Vec::new();
    with_class(ui, ui.root(), "pinged", &mut lit);
    let name = |node: NodeId| {
        let part = ui.children(node).into_iter().find(|&c| ui.has_class(c, "row-name") || ui.has_class(c, "tile-name"));
        part.map(|part| ui.text(part))
    };
    lit.into_iter().filter(|&node| ui.is_visible(node)).filter_map(name).collect()
}

#[test]
fn a_log_line_about_an_asset_lights_it_in_the_project_panel() {
    use voxear_core::logging::LogLine;
    let log = LogCapture::default();
    let Some((mut renderer, mut editor)) = open_logging(log.clone()) else { return };
    frame(&mut editor, &mut renderer);
    let about = LogLine {
        target: "script".into(),
        asset: Some("Prefabs/Lamp.prefab".into()),
        ..LogLine::new(tracing::Level::INFO, "The lamp is out")
    };
    log_line(&log, about);
    // The Console and the Project panel share a place, the Project panel in front.
    run_command(&mut editor, &mut renderer, "show console");
    search_console(&mut editor, &mut renderer, "lamp is out");
    let row = console_rows(&editor.ui)[0];
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    run_command(&mut editor, &mut renderer, "show project");
    let lit = pinged_names(&editor.ui);
    assert!(lit.iter().any(|name| name.starts_with("Lamp")), "the prefab is lit in its folder: {lit:?}");
    assert_eq!(editor.selected(), None, "and nothing is selected");
}

#[test]
fn the_console_sifts_searches_and_opens_stacks() {
    use tracing::Level;
    use voxear_core::logging::LogLine;
    let log = LogCapture::default();
    let Some((mut renderer, mut editor)) = open_logging(log.clone()) else { return };
    let clipboard = SharedClipboard::default();
    editor.ui.set_clipboard(Box::new(clipboard.clone()));
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let opens = opened.clone();
    editor.set_source_opener(Box::new(move |file, line| opens.borrow_mut().push((file.to_path_buf(), line))));
    // Real paths, as .NET writes them.
    let root = editor.game.project.root.canonicalize().unwrap();
    let spinner = obj(&editor, "Spinner A").unwrap();

    // An engine line, a script's line three times over about Spinner A, a
    // render warning, a script's exception, and a compiler error.
    let at = |seconds: f64, frame: u64, line: LogLine| LogLine { seconds, frame, ..line };
    let engine = LogLine {
        target: "voxear_engine::levels".into(),
        file: Some("crates/voxear_engine/src/levels.rs".into()),
        line: Some(120),
        ..LogLine::new(Level::INFO, "Level Main loaded")
    };
    log_line(&log, at(0.5, 1, engine));
    let spinner_file = root.join("Scripts/Spinner.cs");
    let stack = format!(
        "   at Hello.Spinner.Update() in {}:line 33\n   at Voxear.Scripting.Behaviours.Tick(Single dt)\n   at Voxear.Scripting.Host.Update(Single dt)",
        spinner_file.display()
    );
    for seconds in [2.0, 2.5, 3.0] {
        let hit = LogLine {
            target: "script".into(),
            stack: Some(stack.clone()),
            context: Some(spinner.to_bits()),
            ..LogLine::new(Level::INFO, "[Combat] Spinner A hit for 12")
        };
        log_line(&log, at(seconds, 7, hit));
    }
    let warning =
        LogLine { target: "voxear_render::materials".into(), ..LogLine::new(Level::WARN, "Rock has no albedo") };
    log_line(&log, at(4.0, 9, warning));
    let thrown = LogLine {
        target: "script".into(),
        stack: Some(format!(
            "   at System.Collections.Generic.List`1.get_Item(Int32 index)\n   at Hello.Pulse.Update() in {}:line 18",
            root.join("Scripts/Pulse.cs").display()
        )),
        ..LogLine::new(Level::ERROR, "NullReferenceException: Object reference not set")
    };
    log_line(&log, at(5.0, 11, thrown));
    let compiler = format!("{}(12,5): error CS1002: ; expected", root.join("Scripts/Pulse.cs").display());
    log_line(&log, at(6.0, 0, LogLine { target: "compiler".into(), ..LogLine::new(Level::ERROR, &compiler) }));

    // The Console given room, as dragging the divider over it up does.
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let roomy = "row(240, [hierarchy], row(-300, column(45%, [scene, game, docs], [console, project]), [inspector]))";
    editor.ui.set_dock_layout(dock, roomy).unwrap();
    editor.ui.show_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(second_texts(&editor.ui, "console-level"), ["4", "1", "2"], "Log, Warning and Error counts");
    let categories = console_button(&editor.ui, "Categories").unwrap();
    click(&mut editor.ui, categories);
    frame(&mut editor, &mut renderer);
    assert_eq!(
        menu_texts(&editor.ui),
        ["Show all categories", "Combat 3", "Compiler 1", "Engine 1", "Hello 4", "Render 1"],
        "categories nobody declared: a [tag], the scripts' namespace, the crate, the compiler"
    );
    // Hiding one keeps the menu open for the next; Show all categories brings them back.
    let render = menu_item(&editor.ui, "Render").unwrap();
    click(&mut editor.ui, render);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_menu_open(categories), "the menu stays open");
    assert!(console_button(&editor.ui, "Categories · 1 hidden").is_some());
    let all = menu_item(&editor.ui, "Show all categories").unwrap();
    click(&mut editor.ui, all);
    frame(&mut editor, &mut renderer);
    editor.ui.close_popup();
    assert!(console_button(&editor.ui, "Categories").is_some());
    assert_eq!(console_lines(&editor.ui).last().map(String::as_str), Some(compiler.as_str()), "it follows the end");

    // Search: a tag, then leaving one out, then a level hidden; a bad
    // expression says why.
    search_console(&mut editor, &mut renderer, "tag:combat");
    assert_eq!(console_lines(&editor.ui), ["[Combat] Spinner A hit for 12"; 3]);
    let collapse = console_button(&editor.ui, "Collapse").unwrap();
    click(&mut editor.ui, collapse);
    frame(&mut editor, &mut renderer);
    let rows = console_rows(&editor.ui);
    assert_eq!(rows.len(), 1, "collapsed, the same line shows once");
    assert_eq!(editor.ui.text(editor.ui.children(rows[0])[4]), "3", "with how many there were");
    search_console(&mut editor, &mut renderer, "-tag:hello");
    assert_eq!(console_lines(&editor.ui), ["Level Main loaded", "Rock has no albedo", compiler.as_str()]);
    let mut levels = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "level-error", &mut levels);
    click(&mut editor.ui, levels[0]);
    frame(&mut editor, &mut renderer);
    assert_eq!(console_lines(&editor.ui), ["Level Main loaded", "Rock has no albedo"], "errors hidden");
    click(&mut editor.ui, levels[0]);
    search_console(&mut editor, &mut renderer, "/[bad/");
    let mut problems = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-problem", &mut problems);
    assert!(editor.ui.is_visible(problems[0]));
    assert!(
        editor.ui.text(problems[0]).starts_with("/[bad/ is not a regular expression"),
        "{}",
        editor.ui.text(problems[0])
    );

    // The line about Spinner A: picking it lights the Obj in the Hierarchy,
    // leaving the selection alone, as Unity pings it; its stack shows below,
    // the engine's frames folded.
    search_console(&mut editor, &mut renderer, "spinner a hit");
    assert!(!editor.ui.is_visible(problems[0]), "a good search clears the complaint");
    let row = console_rows(&editor.ui)[0];
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_ne!(editor.selected(), Some(spinner), "the line's Obj is shown, not selected");
    assert_eq!(pinged_names(&editor.ui), ["Spinner A"], "its row is lit in the Hierarchy");
    let mut infos = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-info", &mut infos);
    assert_eq!(editor.ui.text(infos[0]), "00:03.000 · frame 7 · main thread · Combat, Hello");
    let mut traces = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-trace", &mut traces);
    let trace = traces[0];
    assert_eq!(editor.ui.text(trace), "Hello.Spinner.Update()  Spinner.cs:33\n… 2 frames not the project's own\n");
    shot(&mut renderer, "console.png");

    // A frame's file opens at its line, as does a double click on the line.
    click_link(&mut editor.ui, trace, "frame:0");
    frame(&mut editor, &mut renderer);
    assert_eq!(opened.borrow().last(), Some(&(spinner_file.clone(), 33)));
    click(&mut editor.ui, row);
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    assert_eq!(opened.borrow().len(), 2, "a double click opens where the line came from");

    // Unfolded, then without namespaces or parameters.
    click_link(&mut editor.ui, trace, "unfold");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.text(trace).contains("Voxear.Scripting.Behaviours.Tick(Single dt)"), "{}", editor.ui.text(trace));
    for toggle in ["Namespaces", "Parameters"] {
        let button = console_button(&editor.ui, toggle).unwrap();
        click(&mut editor.ui, button);
        frame(&mut editor, &mut renderer);
    }
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.text(trace).contains("\nBehaviours.Tick()\n"), "{}", editor.ui.text(trace));

    // Copy, then muting it from its menu.
    let copy = console_button(&editor.ui, "Copy").unwrap();
    click(&mut editor.ui, copy);
    frame(&mut editor, &mut renderer);
    let copied = clipboard.0.borrow().clone().unwrap_or_default();
    assert!(copied.starts_with("INFO [Combat] Spinner A hit for 12\n  at Hello.Spinner.Update()"), "{copied}");
    context_click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    let mute = menu_item(&editor.ui, "Mute this message").unwrap();
    click(&mut editor.ui, mute);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(console_lines(&editor.ui).is_empty(), "muted");
    let muted = console_button(&editor.ui, "Muted 1").expect("Muted says how many");
    click(&mut editor.ui, muted);
    frame(&mut editor, &mut renderer);
    assert_eq!(console_lines(&editor.ui), ["[Combat] Spinner A hit for 12"], "and brings them back");

    // Clear hides it all.
    let clear = console_button(&editor.ui, "Clear").unwrap();
    click(&mut editor.ui, clear);
    frame(&mut editor, &mut renderer);
    assert!(console_lines(&editor.ui).is_empty());
}

/// The dock tab that says `title`.
fn dock_tab(ui: &Ui, title: &str) -> Option<NodeId> {
    let mut tabs = Vec::new();
    with_class(ui, ui.root(), "tab", &mut tabs);
    // A tab with an icon has its title in a label beside it.
    let says = |tab: NodeId| ui.text(tab) == title || ui.children(tab).iter().any(|&part| ui.text(part) == title);
    tabs.into_iter().find(|&tab| ui.is_visible(tab) && says(tab))
}

#[test]
fn panels_close_from_their_tabs_and_open_from_the_window_menu() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let tab = dock_tab(&editor.ui, "Console").expect("the Console has a tab");
    context_click(&mut editor.ui, tab);
    frame(&mut editor, &mut renderer);
    let close = menu_item(&editor.ui, "Close").expect("its tab has a menu");
    click(&mut editor.ui, close);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dock_tab(&editor.ui, "Console").is_none(), "closed");

    // The top bar's Window button opens it again.
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let windows = editor.ui.find(editor.ui.root(), "windows").unwrap();
    click(&mut editor.ui, windows);
    frame(&mut editor, &mut renderer);
    let console = menu_item(&editor.ui, "Console").unwrap();
    click(&mut editor.ui, console);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dock_tab(&editor.ui, "Console").is_some(), "open again");

    // Every window's options button, at its tab bar's right end, has the same menu, with Maximize too.
    let tab = dock_tab(&editor.ui, "Console").unwrap();
    editor.ui.show_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    let area = editor.ui.parent(editor.ui.parent(tab).unwrap()).unwrap();
    let options = first_with(&editor.ui, area, "dock-menu").expect("an options button");
    click(&mut editor.ui, options);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Maximize").is_some(), "{:?}", menu_texts(&editor.ui));
    assert!(menu_item(&editor.ui, "Add Inspector").is_some(), "windows there can be more than one of");
    assert!(
        !menu_texts(&editor.ui).iter().any(|item| item.starts_with("Open ")),
        "closed panels are the Window button's"
    );
    let close = menu_item(&editor.ui, "Close").unwrap();
    click(&mut editor.ui, close);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dock_tab(&editor.ui, "Console").is_none(), "closed from its window's menu");
    let mut buttons = Vec::new();
    with_class(&editor.ui, dock, "dock-menu", &mut buttons);
    let shown = buttons.into_iter().filter(|&b| editor.ui.is_visible(b)).count();
    assert!(shown >= 4, "every window has one, the Game and Scene views too: {shown}");

    let windows = editor.ui.find(editor.ui.root(), "windows").unwrap();
    click(&mut editor.ui, windows);
    frame(&mut editor, &mut renderer);
    assert!(menu_texts(&editor.ui).iter().any(|item| item == "Console"), "{:?}", menu_texts(&editor.ui));
    let console = menu_item(&editor.ui, "Console").unwrap();
    click(&mut editor.ui, console);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let tab = dock_tab(&editor.ui, "Console").expect("open again");
    assert!(editor.ui.is_checked(tab), "and showing");

    // Float, from a tab's menu: a window of its own on the desktop; Dock puts it back.
    let inspector = dock_tab(&editor.ui, "Inspector").unwrap();
    context_click(&mut editor.ui, inspector);
    frame(&mut editor, &mut renderer);
    let float = menu_item(&editor.ui, "Float").expect("a docked panel can float");
    click(&mut editor.ui, float);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.ui.dock_panel_in_window(dock, "inspector"), "{}", editor.ui.dock_layout(dock));
    // Its tab is in that window now, where the pointer has to be.
    let inspector = dock_tab(&editor.ui, "Inspector").unwrap();
    let window = editor.ui.window_of(inspector);
    assert!(window.is_some(), "the tab is in a desktop window");
    editor.ui.set_pointer_window(window);
    context_click(&mut editor.ui, inspector);
    frame(&mut editor, &mut renderer);
    let back = menu_item(&editor.ui, "Dock").expect("a floating panel can dock again");
    click(&mut editor.ui, back);
    editor.ui.set_pointer_window(None);
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.dock_panel_floating(dock, "inspector"));
}

/// The settings rows on screen, by their labels.
fn shown_settings(ui: &Ui) -> Vec<String> {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "drawer-settings").unwrap(), "setting", &mut rows);
    rows.into_iter().filter(|&row| ui.is_visible(row)).map(|row| ui.text(ui.children(row)[0])).collect()
}

#[test]
fn settings_open_in_the_drawer_with_a_tab_for_each_group() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let rail = editor.ui.find(editor.ui.root(), "rail-settings").unwrap();
    click(&mut editor.ui, rail);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.drawer(), Some(voxear_editor::Drawer::Settings));
    let mut tabs = Vec::new();
    with_class(&editor.ui, editor.ui.find(editor.ui.root(), "settings-tabs").unwrap(), "settings-tab", &mut tabs);
    let names: Vec<String> = tabs.iter().map(|&t| editor.ui.text(t)).collect();
    assert_eq!(names, ["Scene view", "Play mode", "Console", "Appearance", "Project panel"]);
    assert!(shown_settings(&editor.ui).contains(&"Snap distance".to_string()), "the first group shows");
    assert!(!shown_settings(&editor.ui).contains(&"Pause on error".to_string()), "and only it");

    click(&mut editor.ui, tabs[1]);
    frame(&mut editor, &mut renderer);
    assert!(shown_settings(&editor.ui).contains(&"Pause on error".to_string()), "a tab shows its group");
    assert!(editor.ui.is_checked(tabs[1]) && !editor.ui.is_checked(tabs[0]), "and is the tab marked");
    for _ in 0..10 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "settings.png");

    let filter = editor.ui.find(editor.ui.root(), "settings-filter").unwrap();
    editor.ui.focus(filter);
    editor.ui.text_input("collapse");
    frame(&mut editor, &mut renderer);
    assert_eq!(shown_settings(&editor.ui), ["Collapse identical lines"], "a search reaches every group");

    click(&mut editor.ui, rail);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.drawer(), None, "the same button closes it");
}

/// The Settings panel's row for `title`: its control.
fn setting_control(ui: &Ui, title: &str) -> NodeId {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "drawer-settings").unwrap(), "setting", &mut rows);
    let row = rows.into_iter().find(|&row| ui.text(ui.children(row)[0]) == title).unwrap();
    ui.children(row)[1]
}

#[test]
fn the_theme_is_chosen_in_settings_and_colours_the_editor() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let rail = editor.ui.find(editor.ui.root(), "rail-settings").unwrap();
    click(&mut editor.ui, rail);
    frame(&mut editor, &mut renderer);
    let mut tabs = Vec::new();
    with_class(&editor.ui, editor.ui.find(editor.ui.root(), "settings-tabs").unwrap(), "settings-tab", &mut tabs);
    let appearance = tabs.into_iter().find(|&t| editor.ui.text(t) == "Appearance").unwrap();
    click(&mut editor.ui, appearance);
    frame(&mut editor, &mut renderer);
    assert_eq!(shown_settings(&editor.ui), ["Theme", "Base colour", "Accent colour", "Contrast"]);
    let mut areas = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dock-area", &mut areas);
    let panel = |editor: &voxear_editor::Editor| editor.ui.style(areas[0]).unwrap().background.unwrap();
    assert_eq!(panel(&editor), voxear_ui::Color::hex(0x191a1c), "the default theme's panels");

    let theme = setting_control(&editor.ui, "Theme");
    choose(&mut editor, &mut renderer, theme, 1);
    assert_eq!(editor.settings().theme(), voxear_ui::Theme::LIGHT);
    for _ in 0..12 {
        frame(&mut editor, &mut renderer);
    }
    assert!(panel(&editor).to_oklch()[0] > 0.9, "light panels: {:?}", panel(&editor));
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let [x, y, ..] = editor.ui.rect(editor.ui.root());
    let at = ((y as u32 + 1) * width + x as u32 + 1) as usize * 4;
    assert!(pixels[at] > 200, "the window's ground is light too");
    shot(&mut renderer, "theme-light.png");

    let accent = setting_control(&editor.ui, "Accent colour");
    click(&mut editor.ui, accent);
    frame(&mut editor, &mut renderer);
    assert!(editor.color_picker_open(), "an accent's swatch opens the colour picker");

    click(&mut editor.ui, rail);
    editor.select(obj(&editor, "Sun"));
    for _ in 0..12 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "theme-light-editor.png");
}

/// The collection field a list's row shows its items in: what follows the
/// row in its container.
fn collection_of(ui: &Ui, row: NodeId) -> NodeId {
    let siblings = ui.children(ui.parent(row).unwrap());
    let at = siblings.iter().position(|&node| node == row).unwrap();
    siblings[at + 1..].iter().copied().find(|&node| ui.has_class(node, "collection")).expect("a collection field")
}

/// Picks an option of a dropdown, as the pointer would.
fn choose(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, dropdown: NodeId, option: usize) {
    click(&mut editor.ui, dropdown);
    frame(editor, renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    click(&mut editor.ui, options[option]);
    frame(editor, renderer);
}

#[test]
fn the_scene_views_shading_gizmos_and_full_screen_work() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let find = |editor: &voxear_editor::Editor, name| editor.ui.find(editor.ui.root(), name).unwrap();
    let (shading, gizmos, maximize) =
        (find(&editor, "shading"), find(&editor, "gizmos"), find(&editor, "scene-maximize"));

    choose(&mut editor, &mut renderer, shading, 2);
    assert_eq!(editor.settings().scene_shading, 2, "Wireframe");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "wireframe.png");
    choose(&mut editor, &mut renderer, shading, 1);
    assert_eq!(editor.settings().scene_shading, 1, "Unlit");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "unlit.png");

    // Lit Wireframe is the lit scene with the meshes' edges over it: thin lines darker than either side.
    let viewport = find(&editor, "viewport");
    let thin_lines = |editor: &voxear_editor::Editor, renderer: &mut Renderer| {
        let (width, _, pixels) = renderer.read_pixels().unwrap();
        let [x, y, w, h] = editor.ui.rect(viewport).map(|v| v as usize);
        let light = |row: usize, column: usize| {
            let i = (row * width as usize + column) * 4;
            u32::from(pixels[i]) + u32::from(pixels[i + 1]) + u32::from(pixels[i + 2])
        };
        let rows = (y + 100..y + h - 50).flat_map(|row| (x + 2..x + w - 2).map(move |column| (row, column)));
        rows.filter(|&(row, column)| light(row, column) * 10 < light(row, column - 2).min(light(row, column + 2)) * 8)
            .count()
    };
    choose(&mut editor, &mut renderer, shading, 0);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let lit = thin_lines(&editor, &mut renderer);
    choose(&mut editor, &mut renderer, shading, 3);
    assert_eq!(editor.settings().scene_shading, 3, "Lit Wireframe");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "lit-wireframe.png");
    let edged = thin_lines(&editor, &mut renderer);
    assert!(edged > lit * 3 + 200, "the edges draw over the lit scene: {edged} thin lines against {lit}");
    choose(&mut editor, &mut renderer, shading, 0);

    // Gizmos are on to start with; the selected light shows which way it shines.
    assert!(editor.settings().show_gizmos && editor.ui.is_checked(gizmos));
    let sun = obj(&editor, "Sun").expect("Hello has a sun");
    editor.select(Some(sun));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "gizmos.png");
    click(&mut editor.ui, gizmos);
    frame(&mut editor, &mut renderer);
    assert!(!editor.settings().show_gizmos && !editor.ui.is_checked(gizmos), "the button hides them");

    click(&mut editor.ui, maximize);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(
        dock_tab(&editor.ui, "Hierarchy").is_none() && dock_tab(&editor.ui, "Scene").is_some(),
        "the Scene fills the dock"
    );
    let maximize = find(&editor, "scene-maximize");
    click(&mut editor.ui, maximize);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dock_tab(&editor.ui, "Hierarchy").is_some(), "and gives it back");
}

#[test]
fn the_project_panel_shows_assets_and_makes_renames_and_deletes_files() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let scenes = tile(&editor.ui, "Scenes").expect("the Scenes folder");
    click(&mut editor.ui, scenes);
    click(&mut editor.ui, scenes);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // A scene selected shows in the Inspector, with its Objs.
    let day = tile(&editor.ui, "Day.scene").expect("Day.scene in Scenes");
    click(&mut editor.ui, day);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Scenes/Day.scene"));
    fn texts(ui: &Ui, node: NodeId, out: &mut Vec<String>) {
        out.push(ui.text(node));
        for child in ui.children(node) {
            texts(ui, child, out);
        }
    }
    let mut shown = Vec::new();
    texts(&editor.ui, editor.ui.find(editor.ui.root(), "inspector").unwrap(), &mut shown);
    assert!(shown.iter().any(|t| t.starts_with("Objs ·")) && shown.contains(&"Sun".to_string()), "{shown:?}");

    // A new material from the menu, named in the rename bar.
    context_click(&mut editor.ui, day);
    frame(&mut editor, &mut renderer);
    let new = menu_item(&editor.ui, "New Material").expect("the menu makes materials");
    click(&mut editor.ui, new);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Scenes/New Material.material").is_file());
    let field = editor.ui.find(editor.ui.root(), "project-rename-field").unwrap();
    assert!(editor.ui.is_visible(field) && editor.ui.focused() == Some(field), "naming it straight away");
    assert_eq!(editor.ui.text(field), "New Material", "the name to type over, without its extension");
    type_into(&mut editor.ui, field, "Rock");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(
        dir.path().join("Scenes/Rock.material").is_file() && dir.path().join("Scenes/Rock.material.meta").is_file()
    );
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Scenes/Rock.material"));

    // Its roughness, edited in the Inspector, is saved.
    let roughness = prop_field(&editor.ui, "Roughness").expect("a material's roughness");
    type_into(&mut editor.ui, roughness, "0.2");
    frame(&mut editor, &mut renderer);
    let saved = std::fs::read_to_string(dir.path().join("Scenes/Rock.material")).unwrap();
    assert!(saved.contains("roughness = 0.2"), "{saved}");

    // Deleted, it goes to the project's trash.
    let rock = tile(&editor.ui, "Rock.material").expect("the renamed tile");
    context_click(&mut editor.ui, rock);
    frame(&mut editor, &mut renderer);
    let delete = menu_item(&editor.ui, "Delete").unwrap();
    click(&mut editor.ui, delete);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!dir.path().join("Scenes/Rock.material").exists());
    assert!(dir.path().join(".voxear/Trash/Rock.material").is_file());
    assert!(tile(&editor.ui, "Rock.material").is_none(), "and leaves the panel");
}

#[test]
fn dragging_an_obj_onto_the_project_panel_makes_a_prefab_of_it() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let original = obj(&editor, "Spinner B").unwrap();
    let row = hierarchy_row(&editor.ui, "Spinner B").expect("Spinner B in the Hierarchy");
    let files = editor.ui.find(editor.ui.root(), "project-files").unwrap();
    drag(&mut editor.ui, row, files);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let file = dir.path().join("Spinner B.prefab");
    let text = std::fs::read_to_string(&file).expect("the prefab was written");
    assert!(text.contains("name = \"Spinner B\"") && text.contains("Hello.Spinner"), "{text}");
    let instance = obj(&editor, "Spinner B").expect("an Obj of that name");
    assert_ne!(instance, original, "the Obj is now an instance of the prefab");
    assert!(editor.game.engine.world.has::<voxear_engine::prefab::PrefabInstance>(instance));
    assert_eq!(editor.selected(), Some(instance));
    assert!(tile(&editor.ui, "Spinner B.prefab").is_some(), "and the panel shows the prefab");

    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let back = obj(&editor, "Spinner B").unwrap();
    assert!(!editor.game.engine.world.has::<voxear_engine::prefab::PrefabInstance>(back), "undo brings the Obj back");
}

/// The Inspector section header titled `title`.
fn section_header(ui: &Ui, title: &str) -> Option<NodeId> {
    let mut headers = Vec::new();
    with_class(ui, ui.root(), "foldout-header", &mut headers);
    headers.into_iter().find(|&h| ui.children(h).iter().any(|&part| ui.text(part) == title))
}

/// Opens a menu by a right-click on `node` and picks `label` from it.
fn context_choose(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId, label: &str) {
    context_click(&mut editor.ui, node);
    frame(editor, renderer);
    let item = menu_item(&editor.ui, label).unwrap_or_else(|| panic!("no {label} in {:?}", menu_texts(&editor.ui)));
    click(&mut editor.ui, item);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn scripts_move_up_and_down_among_an_objs_scripts() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let day = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&day).unwrap();
    let text =
        text.replacen(r#"behaviours = ["Hello.Spinner"]"#, r#"behaviours = ["Hello.Spinner", "Hello.Pulse"]"#, 1);
    std::fs::write(&day, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let order = |editor: &voxear_editor::Editor| -> Vec<String> {
        let behaviours = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        behaviours.0.iter().map(|spec| spec.type_name.clone()).collect()
    };
    assert_eq!(order(&editor), ["Hello.Spinner", "Hello.Pulse"]);

    let header = section_header(&editor.ui, "Spinner").expect("the Spinner's section");
    context_click(&mut editor.ui, header);
    frame(&mut editor, &mut renderer);
    let up = menu_item(&editor.ui, "Move Up").unwrap();
    click(&mut editor.ui, up);
    frame(&mut editor, &mut renderer);
    assert_eq!(order(&editor), ["Hello.Spinner", "Hello.Pulse"], "the first script cannot move up");

    let header = section_header(&editor.ui, "Spinner").unwrap();
    context_choose(&mut editor, &mut renderer, header, "Move Down");
    assert_eq!(order(&editor), ["Hello.Pulse", "Hello.Spinner"]);
    let headers: Vec<f32> = ["Pulse", "Spinner"]
        .iter()
        .map(|title| editor.ui.rect(section_header(&editor.ui, title).unwrap())[1])
        .collect();
    assert!(headers[0] < headers[1], "and the Inspector shows them in their new order");
    editor.undo(&mut renderer);
    assert_eq!(order(&editor), ["Hello.Spinner", "Hello.Pulse"], "undo moves it back");
    editor.redo(&mut renderer);
    editor.save();
    let saved = voxear_engine::scene_file::read(&day).unwrap();
    let saved = saved.objs.iter().find(|o| o.name == "Spinner A").unwrap();
    assert_eq!(saved.behaviours, ["Hello.Pulse", "Hello.Spinner"], "the order saves");
}

#[test]
fn a_prefab_opens_on_its_own_to_lock_components_for_designers() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = hierarchy_row(&editor.ui, "Spinner B").unwrap();
    let files = editor.ui.find(editor.ui.root(), "project-files").unwrap();
    drag(&mut editor.ui, row, files);
    frame(&mut editor, &mut renderer);
    editor.save();
    frame(&mut editor, &mut renderer);

    // The Hierarchy's menu opens an instance's prefab, in place of the level.
    let row = hierarchy_row(&editor.ui, "Spinner B").unwrap();
    context_choose(&mut editor, &mut renderer, row, "Open Prefab");
    assert_eq!(editor.open_prefab_path(), Some("Spinner B.prefab"));
    let bar = editor.ui.find(editor.ui.root(), "prefab-bar").unwrap();
    assert!(editor.ui.is_visible(bar), "a bar over the Scene view says so");
    assert!(obj(&editor, "Spinner A").is_none(), "the level's other Objs are gone");
    assert!(obj(&editor, "Game Manager").is_none(), "persistent ones too: only a running game keeps them");
    let root = obj(&editor, "Spinner B").unwrap();
    assert_eq!(editor.selected(), Some(root), "the prefab's root is selected");
    let locks = |editor: &voxear_editor::Editor, entity| {
        editor.game.engine.world.get::<voxear_engine::prefab::Locks>(entity).map(|locks| locks.0.clone())
    };

    // Locked, the Mesh Renderer shows designers nothing until a value is shown to them.
    let header = section_header(&editor.ui, "Mesh Renderer").unwrap();
    context_choose(&mut editor, &mut renderer, header, "Lock for Designers");
    assert_eq!(locks(&editor, root), Some([("renderer".to_string(), Vec::new())].into()));
    let mesh = prop_row(&editor.ui, "Mesh").expect("in its prefab, every value still shows");
    assert!(editor.ui.has_class(mesh, "hidden-from-designers"));
    let material = prop_row(&editor.ui, "Material").unwrap();
    context_choose(&mut editor, &mut renderer, material, "Show to Designers");
    let shown = Some([("renderer".to_string(), vec!["material".to_string()])].into());
    assert_eq!(locks(&editor, root), shown);
    let material = prop_row(&editor.ui, "Material").unwrap();
    assert!(!editor.ui.has_class(material, "hidden-from-designers"));
    editor.undo(&mut renderer);
    assert_eq!(locks(&editor, root), Some([("renderer".to_string(), Vec::new())].into()), "undo takes it back");
    editor.redo(&mut renderer);
    editor.save();
    let saved = voxear_engine::scene_file::read(&dir.path().join("Spinner B.prefab")).unwrap();
    assert_eq!(saved.objs.len(), 1, "the prefab's file has only its own Obj");
    assert_eq!(Some(saved.objs[0].locks.clone()), shown, "and keeps its locks");

    // Close Prefab, as Back does, opens the level again, whose instance shows designers only what the prefab chose.
    let close = editor.ui.find(editor.ui.root(), "prefab-close").unwrap();
    assert!(editor.ui.is_visible(close));
    click(&mut editor.ui, close);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.open_prefab_path(), None);
    assert!(!editor.ui.is_visible(bar));
    assert!(obj(&editor, "Spinner A").is_some(), "the level is back");
    assert_eq!(editor.game.engine.world.query_ref::<&Name>().into_iter().filter(|n| n.0 == "Game Manager").count(), 1);
    let instance = obj(&editor, "Spinner B").unwrap();
    assert_eq!(locks(&editor, instance), shown, "its instance takes the locks saved");
    editor.select(Some(instance));
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Mesh").is_none(), "a value designers do not see is not shown");
    assert!(prop_row(&editor.ui, "Material").is_some(), "one they do is");
    let header = section_header(&editor.ui, "Mesh Renderer").unwrap();
    context_click(&mut editor.ui, header);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Lock for Designers").is_none(), "locks change only in the prefab");
    let remove = menu_item(&editor.ui, "Remove Component").unwrap();
    click(&mut editor.ui, remove);
    frame(&mut editor, &mut renderer);
    assert!(editor.game.engine.world.has::<voxear_render::MeshRenderer>(instance), "nor can it be removed");

    // A prefab's Open in the Inspector opens it too.
    editor.open_prefab("Spinner B.prefab", &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "prefab-mode.png");
    assert_eq!(editor.open_prefab_path(), Some("Spinner B.prefab"));
}

/// Every text in `node`, depth first.
fn texts_in(ui: &Ui, node: NodeId) -> Vec<String> {
    let mut out = vec![ui.text(node)];
    for child in ui.children(node) {
        out.extend(texts_in(ui, child));
    }
    out.retain(|text| !text.is_empty());
    out
}

#[test]
fn the_frame_debugger_lists_the_passes_and_shows_a_picked_image() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    let open: Vec<String> = editor.ui.dock_panels(dock).into_iter().filter(|(.., o)| *o).map(|(id, ..)| id).collect();
    assert!(open.contains(&"frames".to_string()), "Profiling has the Frame Debugger: {open:?}");
    editor.ui.show_dock_panel(dock, "frames");
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }

    let panel = editor.ui.find(editor.ui.root(), "frame-debugger-panel").unwrap();
    let names = |editor: &voxear_editor::Editor, list: NodeId| -> Vec<String> {
        let mut rows = Vec::new();
        with_class(&editor.ui, list, "entities-row", &mut rows);
        let rows = rows.into_iter().filter(|&row| editor.ui.is_visible(row));
        rows.map(|row| editor.ui.text(editor.ui.children(row)[0])).collect()
    };
    fn lists_in(ui: &Ui, node: NodeId, found: &mut Vec<NodeId>) {
        if ui.kind(node) == Some(voxear_ui::Kind::ListView) {
            found.push(node);
        }
        for child in ui.children(node) {
            lists_in(ui, child, found);
        }
    }
    let mut lists = Vec::new();
    lists_in(&editor.ui, panel, &mut lists);
    let passes = names(&editor, lists[0]);
    assert!(passes.iter().any(|p| p == "Cull (early)"), "{passes:?}");
    let all: Vec<&str> = renderer.frame_passes().iter().map(|p| p.name.as_str()).collect();
    assert!(all.contains(&"Composite") && all.contains(&"UI"), "the rest further down: {all:?}");
    // Picked by its place among every pass, as the list holds them, not only those scrolled into view.
    let opaque = all.iter().position(|p| *p == "Opaque (early)").expect("an opaque pass");
    let summary = editor.ui.text(first_with(&editor.ui, panel, "entities-summary").unwrap());
    assert!(summary.contains("passes") && summary.contains("objects drawn"), "{summary}");
    let hint = first_with(&editor.ui, panel, "entities-hint").unwrap();
    assert!(editor.ui.text(hint).starts_with("Pick a pass"), "{}", editor.ui.text(hint));

    editor.ui.set_selection(lists[0], &[opaque]);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let images = names(&editor, lists[1]);
    assert!(images.contains(&"hdr".to_string()) && images.contains(&"depth".to_string()), "{images:?}");
    let title = editor.ui.text(first_with(&editor.ui, panel, "frame-view-title").unwrap());
    assert!(title.starts_with("hdr · ") && title.contains("RGBA16 float"), "what the pass made first: {title}");
    let view = first_with(&editor.ui, panel, "frame-view-image").unwrap();
    assert!(editor.ui.is_visible(view) && editor.ui.rect(view)[2] > 0.0, "the image shows");
    shot(&mut renderer, "frame-debugger.png");

    let depth = images.iter().position(|i| i == "depth").unwrap();
    editor.ui.set_selection(lists[1], &[depth]);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let channels = editor.ui.children(first_with(&editor.ui, panel, "frame-view-controls").unwrap())[0];
    assert_eq!(editor.ui.segment(channels), 1, "depth shows its one channel");
    shot(&mut renderer, "frame-debugger-depth.png");

    editor.ui.show_dock_panel(dock, "profiler");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(renderer.frame_passes().is_empty(), "hidden, it stops the renderer noting passes");
}

#[test]
fn the_profiling_layout_shows_recent_frames_in_the_profiler() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let open_panels = |editor: &mut voxear_editor::Editor| -> Vec<String> {
        editor.ui.dock_panels(dock).into_iter().filter(|(.., open)| *open).map(|(id, ..)| id).collect()
    };
    frame(&mut editor, &mut renderer);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let hint = ["Select an Obj in the Hierarchy to see it here."];
    assert_eq!(texts_in(&editor.ui, inspector), hint, "nothing picked in the Project panel shows either");
    assert!(!open_panels(&mut editor).contains(&"profiler".to_string()), "World has no Profiler");
    // Left with the Console closed, World is kept that way.
    editor.ui.close_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);

    // The Layout menu lists the layouts, the one showing ticked.
    let layouts = editor.ui.find(editor.ui.root(), "layouts").unwrap();
    click(&mut editor.ui, layouts);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Default").is_some() && menu_item(&editor.ui, "Save Layout As…").is_some());
    let profiling = menu_item(&editor.ui, "Profiling").expect("Profiling in the Layout menu");
    click(&mut editor.ui, profiling);
    for _ in 0..6 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(editor.layout_name(), "Profiling");
    let panels = open_panels(&mut editor);
    for id in ["profiler", "game", "console", "inspector"] {
        assert!(panels.contains(&id.to_string()), "{id} in {panels:?}");
    }
    shot(&mut renderer, "profiler.png");

    let samples = editor.profiler().samples();
    assert!(samples.len() >= 6, "a sample a frame");
    let last = samples.back().unwrap();
    let cpu = &last.cpu;
    let inside = |node: usize| -> Vec<String> { cpu.nodes[node].children.iter().map(|&c| cpu.name(c)).collect() };
    assert_eq!(inside(voxear_core::profiling::Samples::FRAME), ["Game", "Editor", "UI", "Render"]);
    let game = cpu.find("Game").unwrap();
    assert!(inside(game).contains(&"Update".to_string()), "the engine's steps inside the game's: {:?}", inside(game));
    let editor_node = cpu.find("Editor").unwrap();
    assert!(inside(editor_node).contains(&"Inspector".to_string()), "and each panel");
    assert!(cpu.find("propagate transforms").is_some(), "and each system");
    let render = cpu.find("Render").unwrap();
    assert!(cpu.total_ms(render) >= cpu.self_ms(render));
    // Waiting for the display is named so, and the CPU figure leaves it out.
    let display = cpu.find("Wait for Display").expect("the wait for the display has a sample");
    let worked = cpu.total_ms(voxear_core::profiling::Samples::FRAME) - cpu.total_ms(display);
    assert!(last.cpu_ms() <= worked + 1e-3, "{} of {worked}", last.cpu_ms());
    assert!(!last.gpu.is_empty(), "and the GPU's passes");
    let panel = editor.ui.find(editor.ui.root(), "profiler-panel").unwrap();
    let shown = texts_in(&editor.ui, panel);
    for text in ["Hierarchy", "Calls", "Self ms", "UI", "Objs drawn", "Render"] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }

    // The Timeline: the frame's runs as bars, named when wide enough, and the one pointed at described.
    assert!(shown.iter().any(|t| t == "GC Alloc"), "scripts' allocations have a column");
    let view = first_with(&editor.ui, panel, "profiler-view").unwrap();
    editor.ui.set_segment(view, 1);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut names = Vec::new();
    with_class(&editor.ui, panel, "profiler-bar-name", &mut names);
    let named: Vec<String> = names.iter().filter(|&&n| editor.ui.is_visible(n)).map(|&n| editor.ui.text(n)).collect();
    let tops = ["Game", "Editor", "UI", "Render"];
    assert!(named.iter().any(|n| tops.iter().any(|t| n.starts_with(t))), "the widest bars are named: {named:?}");
    let canvas = first_with(&editor.ui, panel, "profiler-canvas").unwrap();
    let [x, y, _, _] = editor.ui.rect(canvas);
    editor.ui.pointer_move(x + 2.0, y + 9.0);
    frame(&mut editor, &mut renderer);
    let hovered = editor.ui.text(first_with(&editor.ui, panel, "profiler-hovered").unwrap());
    assert!(hovered.contains("ms, from"), "the bar pointed at is described: {hovered}");

    // The wheel zooms in around the pointer, so the left edge then shows a later part of the frame,
    // and a double click shows the whole frame again.
    let from = |editor: &voxear_editor::Editor| -> f32 {
        let text = editor.ui.text(first_with(&editor.ui, panel, "profiler-hovered").unwrap());
        let after = text.split("ms, from ").nth(1).unwrap_or_default();
        after.split(' ').next().and_then(|ms| ms.parse().ok()).unwrap_or(-1.0)
    };
    let edge = from(&editor);
    let [_, _, width, _] = editor.ui.rect(canvas);
    editor.ui.pointer_move(x + width / 2.0, y + 9.0);
    assert!(editor.ui.pointer_scroll(0.0, -400.0), "the timeline takes the wheel");
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(x + 2.0, y + 9.0);
    frame(&mut editor, &mut renderer);
    assert!(from(&editor) > edge, "zoomed in, the left edge is later in the frame: {} after {edge}", from(&editor));
    assert!(editor.profiler().timeline_zoom() < 0.5, "{}", editor.profiler().timeline_zoom());
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.profiler().timeline_zoom(), 1.0, "a double click shows the whole frame again");
    shot(&mut renderer, "profiler-timeline.png");

    // A worker thread's samples show in a lane of their own, titled with its name, while it works.
    let (started, finish) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
    let (tell, wait) = (started.0, finish.1);
    let worker = std::thread::spawn(move || {
        voxear_core::profiling::register_thread("Worker");
        let _bake = voxear_core::profile_marker!("Bake lighting").scope();
        tell.send(()).unwrap();
        wait.recv().unwrap();
    });
    started.1.recv().unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let threads = |editor: &voxear_editor::Editor| {
        editor
            .profiler()
            .samples()
            .iter()
            .flat_map(|s| s.cpu.threads.iter().map(|t| t.name.clone()))
            .collect::<Vec<_>>()
    };
    assert!(threads(&editor).contains(&"Worker".to_string()), "the worker's runs came with a frame");
    // The timeline's details refresh a few times a second while live.
    let titled = |editor: &voxear_editor::Editor| {
        let mut titles = Vec::new();
        with_class(&editor.ui, panel, "profiler-thread-title", &mut titles);
        titles.iter().filter(|&&t| editor.ui.is_visible(t)).map(|&t| editor.ui.text(t)).collect::<Vec<_>>()
    };
    let started = std::time::Instant::now();
    while titled(&editor).is_empty() && started.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(titled(&editor), ["Worker"], "a lane for it under the main thread's");
    shot(&mut renderer, "profiler-worker.png");
    finish.0.send(()).unwrap();
    worker.join().unwrap();
    editor.ui.set_segment(view, 0);
    frame(&mut editor, &mut renderer);

    // A click on the chart picks a frame to break down, until Live.
    let toolbar = editor.ui.children(first_with(&editor.ui, panel, "console-toolbar").unwrap());
    let (record, live) = (toolbar[0], toolbar[2]);
    let picked = first_with(&editor.ui, panel, "profiler-picked").unwrap();
    assert_eq!(editor.ui.text(picked), "Live");
    assert!(!editor.ui.is_visible(live));
    let chart = first_with(&editor.ui, panel, "profiler-chart").unwrap();
    click(&mut editor.ui, chart);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.text(picked).starts_with("Frame "), "{}", editor.ui.text(picked));
    assert!(editor.ui.is_visible(live), "a Live button goes back to the newest");
    click(&mut editor.ui, live);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(picked), "Live");

    // Record stops taking frames.
    click(&mut editor.ui, record);
    frame(&mut editor, &mut renderer);
    let count = editor.profiler().samples().len();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.profiler().samples().len(), count);

    assert!(editor.use_layout("Default"));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.layout_name(), "Default");
    let panels = open_panels(&mut editor);
    assert!(!panels.contains(&"profiler".to_string()) && !panels.contains(&"console".to_string()), "{panels:?}");
    assert!(panels.contains(&"scene".to_string()));
}

/// The object picker's rows on screen, as label and detail.
fn picker_rows(ui: &Ui) -> Vec<(String, String)> {
    let Some(list) = ui.find(ui.root(), "picker-list") else { return Vec::new() };
    let mut rows: Vec<NodeId> =
        ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    rows.into_iter().map(|row| (ui.text(ui.children(row)[1]), ui.text(ui.children(row)[2]))).collect()
}

/// A click whose press and release come frames apart, as a hand's do, so
/// whatever the editor does between them happens, as it does when used.
fn click_across_frames(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    for _ in 0..3 {
        frame(editor, renderer);
    }
    editor.ui.pointer_button(false);
}

/// Adds a component as a user does: Add Component, its name typed into the
/// search, and a click on it.
fn add_component(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").expect("Add Component");
    click(&mut editor.ui, add);
    frame(editor, renderer);
    frame(editor, renderer);
    assert!(editor.picker_open(), "Add Component opens its search");
    editor.ui.text_input(name);
    frame(editor, renderer);
    let row = picker_row(&editor.ui, name).unwrap_or_else(|| panic!("{name} is offered: {:?}", picker_rows(&editor.ui)));
    click(&mut editor.ui, row);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// Scrolls whatever holds `node` until it shows at the top, as a hand
/// scrolls down to it.
fn scroll_to(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let mut at = editor.ui.parent(node);
    while let Some(parent) = at {
        if editor.ui.scroll_max(parent)[1] > 0.0 {
            let ([_, top, _, _], [_, y, _, _]) = (editor.ui.rect(parent), editor.ui.rect(node));
            let [x, now] = editor.ui.scroll_offset(parent);
            editor.ui.set_scroll(parent, [x, (now + y - top - 20.0).max(0.0)]);
            frame(editor, renderer);
            return;
        }
        at = editor.ui.parent(parent);
    }
}

/// The object picker's row that says `label`.
fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
}

/// Scripts in namespaces past the one they share, one placed by its
/// `[AddComponentMenu]`, and one it leaves out.
const SCRIPTS_IN_FOLDERS: &str = r##"{"behaviours": [
  {"name": "Game.Spinner", "bases": [], "interfaces": [], "fields": []},
  {"name": "Game.Enemies.Orc", "bases": [], "interfaces": [], "fields": []},
  {"name": "Game.CampFire", "bases": [], "interfaces": [], "fields": [], "menu": "Effects/Camp Fire"},
  {"name": "Game.Spawned", "bases": [], "interfaces": [], "fields": [], "menu": ""}
], "types": {}, "commands": []}"##;

#[test]
fn the_editors_style_sheets_read_without_warnings() {
    let sheets = [
        ("Editor.vss", include_str!("../ui/Editor.vss")),
        ("Default.vss", include_str!("../../voxear_ui/theme/Default.vss")),
    ];
    for (name, text) in sheets {
        let sheet = voxear_ui::StyleSheet::parse(name, text);
        assert!(sheet.warnings().is_empty(), "{name}: {:?}", sheet.warnings());
    }
}

/// Side bar buttons, one alone and two sharing a menu, and an item.
const SIDE_BAR: &str = r##"{"behaviours": [], "types": {}, "commands": [], "side_bar": {"buttons": [
  {"title": "Snap Everything", "icon": "grid", "type": "Game.Tools", "method": "Snap"},
  {"title": "Light Every Lamp", "icon": "light", "menu": "Lamps", "type": "Game.Tools", "method": "Light"},
  {"title": "Put Every Lamp Out", "icon": "light", "menu": "Lamps", "type": "Game.Tools", "method": "Out"}
], "items": ["Game.Beacon"]}}"##;

#[test]
fn the_game_adds_buttons_menus_and_items_to_the_side_bar() {
    let Some((mut renderer, mut editor)) = open() else { return };
    editor.set_script_types(voxear_engine::script_types::parse(SIDE_BAR).unwrap());
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.side_bar_count(), (2, 1), "a button alone, one for the menu, and the item");
    let slot = editor.ui.find(editor.ui.root(), "rail-scripts").unwrap();
    let buttons: Vec<NodeId> = editor.ui.children(slot).into_iter().filter(|&b| editor.ui.has_class(b, "rail-button")).collect();

    // Resting on a button shows what it does beside it.
    let [x, y, w, h] = editor.ui.rect(buttons[0]);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..40 {
        frame(&mut editor, &mut renderer);
    }
    let tooltip = editor.ui.find(editor.ui.root(), "tooltip").unwrap();
    assert!(editor.ui.is_visible(tooltip) && editor.ui.text(tooltip) == "Snap Everything", "{}", editor.ui.text(tooltip));
    assert!(editor.ui.rect(tooltip)[0] > x + w, "beside the button");

    // The menu's button opens a menu of its lines, beside it.
    click(&mut editor.ui, buttons[1]);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Light Every Lamp").is_some() && menu_item(&editor.ui, "Put Every Lamp Out").is_some());

    // When the scripts change, the side bar is built again from them.
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_LISTS).unwrap());
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.side_bar_count(), (0, 0));
    assert!(editor.ui.children(slot).is_empty());
}

#[test]
fn add_component_is_a_search_with_folders() {
    let Some((mut renderer, mut editor)) = open() else { return };
    editor.set_script_types(voxear_engine::script_types::parse(SCRIPTS_IN_FOLDERS).unwrap());
    let ground = obj(&editor, "Ground").unwrap();
    editor.select(Some(ground));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let labels = |ui: &Ui| picker_rows(ui).into_iter().map(|(label, _)| label).collect::<Vec<_>>();
    // The scripts' Effects folder is the built-in one, with the Particle System.
    let folders = [
        "Rendering",
        "Effects",
        "UI",
        "Networking",
        "Animation",
        "Water",
        "Paths",
        "Terrain",
        "Audio",
        "Physics",
        "Physics 2D",
        "Scripts",
    ];
    assert_eq!(labels(&editor.ui), folders, "folders first, as in Unity");
    shot(&mut renderer, "add-component.png");

    // A folder opens on a click, its scripts past the namespace they share, with a back button.
    let scripts = picker_row(&editor.ui, "Scripts").unwrap();
    scroll_to(&mut editor, &mut renderer, scripts);
    let scripts = picker_row(&editor.ui, "Scripts").unwrap();
    click(&mut editor.ui, scripts);
    frame(&mut editor, &mut renderer);
    assert_eq!(labels(&editor.ui), ["Enemies", "Spinner"]);
    shot(&mut renderer, "add-component-scripts.png");
    let back = editor.ui.find(editor.ui.root(), "picker-back").unwrap();
    assert!(editor.ui.is_visible(back));
    click(&mut editor.ui, back);
    frame(&mut editor, &mut renderer);
    assert_eq!(labels(&editor.ui)[0], "Rendering", "back at the top");

    // Typing searches every folder, each row saying where it is; an empty menu path is left out.
    editor.ui.text_input("fire");
    frame(&mut editor, &mut renderer);
    assert_eq!(picker_rows(&editor.ui), [("Camp Fire".to_string(), "Effects".to_string())]);
    let search = editor.ui.find(editor.ui.root(), "picker-field").unwrap();
    type_into(&mut editor.ui, search, "spawn");
    frame(&mut editor, &mut renderer);
    assert!(labels(&editor.ui).is_empty(), "{:?}", picker_rows(&editor.ui));
    editor.picker_key(voxear_editor::PaletteKey::Escape);
    frame(&mut editor, &mut renderer);

    // Picked, a component is added, as one step to undo.
    add_component(&mut editor, &mut renderer, "Orc");
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(ground).unwrap();
    assert_eq!(kept.0[0].type_name, "Game.Enemies.Orc");
    editor.undo(&mut renderer);
    assert!(editor.game.engine.world.get::<voxear_engine::EditBehaviours>(ground).is_none());
}

#[test]
fn a_mesh_collider_takes_its_objs_mesh() {
    use voxear_engine::physics::{Collider, ColliderMesh, ColliderShape};
    let Some((mut renderer, mut editor)) = open() else { return };
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Collider");

    // As a mesh, it has no size or center of its own, and may be its mesh's hull.
    let dropdown = first_with(&editor.ui, prop_row(&editor.ui, "Shape").unwrap(), "value-choice").unwrap();
    choose(&mut editor, &mut renderer, dropdown, 3);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.get::<Collider>(spinner).unwrap().shape, ColliderShape::Mesh);
    assert!(prop_row(&editor.ui, "Convex").is_some() && prop_row(&editor.ui, "Size").is_none());
    assert!(prop_row(&editor.ui, "Center").is_none());
    let convex = first_with(&editor.ui, prop_row(&editor.ui, "Convex").unwrap(), "checkbox").unwrap();
    click(&mut editor.ui, convex);
    frame(&mut editor, &mut renderer);
    assert!(editor.game.engine.world.get::<Collider>(spinner).unwrap().convex);

    // The engine gives it its cube's triangles, which the Scene view outlines.
    frame(&mut editor, &mut renderer);
    let mesh = editor.game.engine.world.get::<ColliderMesh>(spinner).expect("the cube's triangles");
    assert_eq!((mesh.points.len(), mesh.indices.len()), (24, 36));
    shot(&mut renderer, "mesh-collider.png");
}

#[test]
fn physics_components_show_what_their_shape_uses() {
    use voxear_engine::physics::{CharacterController, Collider, ColliderShape, Rigidbody};
    let Some((mut renderer, mut editor)) = open() else { return };
    let ground = obj(&editor, "Ground").unwrap();
    editor.select(Some(ground));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // A Collider starts as a box, and shows its size.
    add_component(&mut editor, &mut renderer, "Collider");
    assert!(editor.game.engine.world.has::<Collider>(ground));
    assert!(prop_row(&editor.ui, "Size").is_some() && prop_row(&editor.ui, "Radius").is_none());

    // A sphere shows its radius instead, and a capsule its height too.
    let shape = |ui: &Ui| first_with(ui, prop_row(ui, "Shape").unwrap(), "value-choice").unwrap();
    let dropdown = shape(&editor.ui);
    choose(&mut editor, &mut renderer, dropdown, 1);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.get::<Collider>(ground).unwrap().shape, ColliderShape::Sphere);
    assert!(prop_row(&editor.ui, "Radius").is_some() && prop_row(&editor.ui, "Size").is_none());
    let dropdown = shape(&editor.ui);
    choose(&mut editor, &mut renderer, dropdown, 2);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Radius").is_some() && prop_row(&editor.ui, "Height").is_some());

    // A Rigidbody and a Character Controller come from the same folder,
    // Add Component scrolled to below the sections.
    for name in ["Rigidbody", "Character Controller"] {
        let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
        scroll_to(&mut editor, &mut renderer, add);
        add_component(&mut editor, &mut renderer, name);
    }
    let world = &editor.game.engine.world;
    assert!(world.has::<Rigidbody>(ground) && world.has::<CharacterController>(ground));
    assert!(prop_row(&editor.ui, "Mass").is_some() && prop_row(&editor.ui, "Slope limit").is_some());
    shot(&mut renderer, "physics-inspector.png");

    // Each is undone.
    for _ in 0..4 {
        editor.undo(&mut renderer);
        frame(&mut editor, &mut renderer);
    }
    let world = &editor.game.engine.world;
    assert!(!world.has::<Collider>(ground) && !world.has::<Rigidbody>(ground) && !world.has::<CharacterController>(ground));

    // The 2D ones have a folder of their own, and a circle shows its radius.
    for name in ["Collider 2D", "Rigidbody 2D"] {
        let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
        scroll_to(&mut editor, &mut renderer, add);
        add_component(&mut editor, &mut renderer, name);
    }
    let world = &editor.game.engine.world;
    assert!(world.has::<voxear_engine::physics2d::Collider2D>(ground));
    assert!(prop_row(&editor.ui, "Gravity scale").is_some() && prop_row(&editor.ui, "Size").is_some());
    let dropdown = shape(&editor.ui);
    choose(&mut editor, &mut renderer, dropdown, 1);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Radius").is_some() && prop_row(&editor.ui, "Size").is_none());

    // A Joint holds to the world to start with; a hinge shows its axis, and its limits once they are on.
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
    scroll_to(&mut editor, &mut renderer, add);
    add_component(&mut editor, &mut renderer, "Joint");
    let connected = first_with(&editor.ui, prop_row(&editor.ui, "Connected").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(connected), "None");
    assert!(prop_row(&editor.ui, "Axis").is_none(), "a fixed joint has no axis");
    let kind = first_with(&editor.ui, prop_row(&editor.ui, "Type").unwrap(), "value-choice").unwrap();
    choose(&mut editor, &mut renderer, kind, 1);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Axis").is_some() && prop_row(&editor.ui, "Min").is_none());
    let limited = first_with(&editor.ui, prop_row(&editor.ui, "Use limits").unwrap(), "checkbox").unwrap();
    click(&mut editor.ui, limited);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let joint = *editor.game.engine.world.get::<voxear_engine::physics::Joint>(ground).unwrap();
    assert_eq!(joint.limits, Some([0.0, 90.0]), "a door's quarter turn to start with");
    assert!(prop_row(&editor.ui, "Min").is_some() && prop_row(&editor.ui, "Max").is_some());
    shot(&mut renderer, "joint-inspector.png");
}

#[test]
fn the_inspector_puts_objs_on_the_layers_the_project_names() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project_file = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&project_file).unwrap()
        + "\n[layers]\nnames = [\"Default\", \"\", \"Player\", \"Enemy\"]\n";
    std::fs::write(&project_file, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let layer = editor.ui.find(editor.ui.root(), "obj-layer").unwrap();
    assert_eq!(editor.ui.selected_option(layer), 0, "Default to start with");

    // The named layers in number order, the unnamed one left out: Enemy is the third.
    choose(&mut editor, &mut renderer, layer, 2);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.get::<voxear_scene::Layer>(spinner), Some(&voxear_scene::Layer(3)));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.game.engine.world.get::<voxear_scene::Layer>(spinner).is_none(), "undone, back on Default");
    let layer = editor.ui.find(editor.ui.root(), "obj-layer").unwrap();
    assert_eq!(editor.ui.selected_option(layer), 0);

    // Add Layer…, after the layers, shows the project's in the Inspector.
    choose(&mut editor, &mut renderer, layer, 3);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), None);
    let name = editor.ui.find(editor.ui.root(), "layer-name-4").expect("the Layers view");
    // A layer named and let go of is written to voxear.toml, and is in the matrix.
    type_into(&mut editor.ui, name, "Ghost");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let written = std::fs::read_to_string(&project_file).unwrap();
    assert!(written.contains("names = [\"Default\", \"\", \"Player\", \"Enemy\", \"Ghost\"]"), "{written}");
    assert!(written.starts_with("[project]"), "the rest of the file kept: {written}");
    let layers = |editor: &voxear_editor::Editor| editor.game.engine.world.resource::<voxear_scene::Layers>().cloned();
    assert_eq!(layers(&editor).unwrap().find("Ghost"), Some(4));
    // A tick taken off keeps Enemy and Ghost apart.
    let tick = editor.ui.find(editor.ui.root(), "layer-collide-3-4").expect("a tick for Enemy and Ghost");
    let inspector = first_with(&editor.ui, editor.ui.root(), "inspector").unwrap();
    let [_, top, ..] = editor.ui.rect(inspector);
    let [_, at, ..] = editor.ui.rect(tick);
    editor.ui.set_scroll(inspector, [0.0, at - top - 120.0]);
    frame(&mut editor, &mut renderer);
    click(&mut editor.ui, tick);
    frame(&mut editor, &mut renderer);
    let written = std::fs::read_to_string(&project_file).unwrap();
    assert!(written.contains("ignore = [[\"Enemy\", \"Ghost\"]]"), "{written}");
    assert!(!layers(&editor).unwrap().collide(3, 4) && layers(&editor).unwrap().collide(4, 4));
    // A name another layer has is refused.
    let name = editor.ui.find(editor.ui.root(), "layer-name-5").unwrap();
    type_into(&mut editor.ui, name, "Enemy");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert_eq!((layers(&editor).unwrap().name(5), editor.ui.text(name).as_str()), ("", ""));
    let [x, y, w, h] = editor.ui.rect(tick);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    frame(&mut editor, &mut renderer);
    let pair = first_with(&editor.ui, editor.ui.root(), "layer-pair").unwrap();
    assert_eq!(editor.ui.text(pair), "Enemy · Ghost", "the pair under the pointer");
    shot(&mut renderer, "layers.png");
}

#[test]
fn the_object_picker_searches_what_a_reference_can_take() {
    use voxear_editor::PaletteKey;
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    for name in ["Brick", "Glass"] {
        std::fs::write(dir.path().join(format!("Materials/{name}.material")), "color = [0.7, 0.3, 0.2]\n").unwrap();
    }
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    let renderer_of =
        |editor: &voxear_editor::Editor| *editor.game.engine.world.get::<voxear_render::MeshRenderer>(spinner).unwrap();
    let open_picker = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, row: &str| {
        let picker = first_with(&editor.ui, prop_row(&editor.ui, row).unwrap(), "value-picker").unwrap();
        let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
        click(&mut editor.ui, arrow);
        frame(editor, renderer);
        frame(editor, renderer);
        assert!(editor.picker_open(), "{row}");
    };

    // The material: the default, the built-in glass, then the project's, each with its folder.
    open_picker(&mut editor, &mut renderer, "Material");
    let title = editor.ui.find(editor.ui.root(), "picker-title").unwrap();
    assert_eq!(editor.ui.text(title), "Select Material");
    let offered = picker_rows(&editor.ui);
    assert_eq!(offered[0].0, "Default");
    assert_eq!(offered[1], ("Glass".into(), "Built in".into()));
    assert!(offered.contains(&("Brick.material".into(), "Materials".into())), "{offered:?}");
    shot(&mut renderer, "object-picker.png");
    // Typed letters narrow it, the best match first, and Enter picks it.
    editor.ui.text_input("glass.m");
    frame(&mut editor, &mut renderer);
    assert_eq!(picker_rows(&editor.ui).first().map(|(label, _)| label.as_str()), Some("Glass.material"));
    editor.picker_key(PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    assert!(!editor.picker_open());
    let material = renderer_of(&editor).material;
    let source = editor.game.content.material_source(material).cloned();
    let picked = match source {
        Some(voxear_engine::MaterialSource::Asset(reference)) => {
            reference.guid().and_then(|guid| editor.game.content.db().get(guid)).map(|e| e.path.clone())
        }
        _ => None,
    };
    assert_eq!(picked.as_deref(), Some("Materials/Glass.material"));
    let shown = first_with(&editor.ui, prop_row(&editor.ui, "Material").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(shown), "Glass.material");
    editor.undo(&mut renderer);
    assert_ne!(renderer_of(&editor).material, material, "a pick is a step to undo");
    frame(&mut editor, &mut renderer);

    // The mesh, with Escape closing it untouched.
    let mesh = renderer_of(&editor).mesh;
    open_picker(&mut editor, &mut renderer, "Mesh");
    assert_eq!(
        picker_rows(&editor.ui).iter().map(|(label, _)| label.as_str()).take(3).collect::<Vec<_>>(),
        ["Cube", "Sphere", "Plane"],
        "the built-in meshes first, then the project's"
    );
    editor.picker_key(PaletteKey::Escape);
    frame(&mut editor, &mut renderer);
    assert!(!editor.picker_open());
    assert_eq!(renderer_of(&editor).mesh, mesh);
    open_picker(&mut editor, &mut renderer, "Mesh");
    editor.ui.text_input("sph");
    frame(&mut editor, &mut renderer);
    editor.picker_key(PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    let mesh = renderer_of(&editor).mesh;
    assert_eq!(editor.game.content.mesh_ref(mesh).map(|r| r.to_string()).as_deref(), Some("builtin:sphere"));

    // A click elsewhere closes it.
    open_picker(&mut editor, &mut renderer, "Mesh");
    let hierarchy = hierarchy_row(&editor.ui, "Ground").unwrap();
    click(&mut editor.ui, hierarchy);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!editor.picker_open());
}

/// Opens a folder of the Project panel, as a double click on its tile does.
fn open_folder(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let folder = tile(&editor.ui, name).unwrap_or_else(|| panic!("a tile for {name}"));
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(editor, renderer);
}

/// What a tile's thumbnail shows: its preview's image when there is one.
fn tile_preview(ui: &Ui, name: &str) -> Option<NodeId> {
    let thumb = ui.children(tile(ui, name)?)[0];
    let image = ui.children(thumb)[1];
    ui.is_visible(image).then_some(image)
}

#[test]
fn assets_show_previews_in_the_project_fields_picker_and_inspector() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    for (name, color) in [("Brick", "[0.7, 0.3, 0.2]"), ("Moss", "[0.2, 0.6, 0.25]")] {
        std::fs::write(dir.path().join(format!("Materials/{name}.material")), format!("color = {color}\n")).unwrap();
    }
    let rust = image::RgbaImage::from_pixel(256, 256, image::Rgba([180, 90, 40, 255]));
    rust.save(dir.path().join("Materials/Rust.png")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // The Project's tiles: a material's preview in place of its icon, drawn a frame or two after it shows.
    open_folder(&mut editor, &mut renderer, "Materials");
    assert!(tile_preview(&editor.ui, "Brick.material").is_none(), "drawn over the next frames");
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert!(tile_preview(&editor.ui, "Brick.material").is_some(), "the material on a sphere");
    assert!(tile_preview(&editor.ui, "Moss.material").is_some());
    // A picture shows itself, once a thumbnail of it is decoded on a thread of its own.
    for _ in 0..60 {
        if tile_preview(&editor.ui, "Rust.png").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut editor, &mut renderer);
    }
    assert!(tile_preview(&editor.ui, "Rust.png").is_some(), "a picture's tile shows the picture");

    // Selected, it shows large in the Inspector, and a drag turns it.
    let brick = tile(&editor.ui, "Brick.material").unwrap();
    click(&mut editor.ui, brick);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let preview =
        editor.ui.find(editor.ui.root(), "inspector").and_then(|i| first_with(&editor.ui, i, "asset-preview"));
    let preview = preview.expect("a large preview");
    let image = first_with(&editor.ui, preview, "asset-preview-image").unwrap();
    assert!(editor.ui.is_visible(image), "drawn");
    assert_eq!(editor.inspector_preview_turn(), Some(voxear_core::math::Vec2::ZERO));
    let [x, y, w, h] = editor.ui.rect(preview);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(x + w / 2.0 + 40.0, y + h / 2.0 + 10.0);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_visible(image), "still shown while it turns, not blank until let go");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let turn = editor.inspector_preview_turn().unwrap();
    assert!(turn.x > 0.3 && turn.y > 0.05, "dragged right and down, it turns: {turn:?}");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "previews-material.png");

    // Prefabs show their Objs, a few new previews a frame.
    let folders = editor.ui.find(editor.ui.root(), "project-folders").unwrap();
    let mut names = Vec::new();
    with_class(&editor.ui, folders, "row-name", &mut names);
    let top = names.into_iter().find(|&n| editor.ui.text(n) == "Hello").unwrap();
    click(&mut editor.ui, top);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_folder(&mut editor, &mut renderer, "Prefabs");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    for prefab in ["Lamp.prefab", "Firefly.prefab"] {
        assert!(tile_preview(&editor.ui, prefab).is_some(), "{prefab} shows a preview");
    }
    shot(&mut renderer, "previews-prefabs.png");

    // A Mesh Renderer's fields show its mesh and material, and its picker's rows the materials.
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    for row in ["Mesh", "Material"] {
        let thumb = first_with(&editor.ui, prop_row(&editor.ui, row).unwrap(), "field-thumb").unwrap();
        assert!(editor.ui.is_visible(thumb), "{row}'s field shows a preview");
    }
    let picker = first_with(&editor.ui, prop_row(&editor.ui, "Material").unwrap(), "value-picker").unwrap();
    let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "previews-picker.png");
    let brick_row = picker_row(&editor.ui, "Brick.material").expect("a row for Brick");
    let thumb = editor.ui.children(editor.ui.children(brick_row)[0]);
    assert!(editor.ui.is_visible(thumb[1]) && !editor.ui.is_visible(thumb[0]), "its preview in place of its icon");
    editor.picker_key(voxear_editor::PaletteKey::Escape);
    frame(&mut editor, &mut renderer);
}

/// Presses at a point, moves to another while held, and lets go, a frame
/// at each, as a drag in a window does.
fn drag_between(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, from: [f32; 2], to: [f32; 2]) {
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(editor, renderer);
    editor.ui.pointer_move(to[0], to[1]);
    frame(editor, renderer);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
}

#[test]
fn the_colour_picker_edits_colours_and_hdr_emission() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Glow.material"), "color = [0.2, 0.2, 0.2]\nemissive = [4.0, 1.0, 2.0]\n")
        .unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let sun = obj(&editor, "Sun").unwrap();
    editor.select(Some(sun));
    frame(&mut editor, &mut renderer);
    let colour =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<voxear_render::Light>(sun).unwrap().color;
    let was = colour(&editor);

    // A click on the Light's swatch opens the picker under it.
    let swatch = first_with(&editor.ui, prop_row(&editor.ui, "Colour").unwrap(), "swatch").unwrap();
    click(&mut editor.ui, swatch);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.color_picker_open());
    let find = |editor: &voxear_editor::Editor, class: &str| first_with(&editor.ui, editor.ui.root(), class).unwrap();
    let title = editor.ui.find(editor.ui.root(), "color-title").unwrap();
    assert_eq!(editor.ui.text(title), "Colour", "named for the value, with no HDR for a light's colour");
    assert!(!editor.ui.is_visible(editor.ui.parent(find(&editor, "color-intensity-slider")).unwrap()));

    // The hue bar's last third is blue, and the square's top right is that hue at its fullest.
    let [x, y, w, h] = editor.ui.rect(find(&editor, "color-hue"));
    drag_between(&mut editor, &mut renderer, [x + w * 0.66, y + h / 2.0], [x + w * 0.667, y + h / 2.0]);
    let [x, y, w, _] = editor.ui.rect(find(&editor, "color-square"));
    drag_between(&mut editor, &mut renderer, [x + w * 0.5, y + 20.0], [x + w + 30.0, y - 30.0]);
    shot(&mut renderer, "colour-picker.png");
    let blue = colour(&editor);
    assert!(blue.z > 0.9 && blue.x < 0.05 && blue.y < 0.05, "dragged past the corner, it holds at pure blue: {blue:?}");
    assert!(editor.color_picker_open(), "drags inside keep it open");

    // A hex code typed in.
    let hex = find(&editor, "color-hex");
    type_into(&mut editor.ui, hex, "#00FF00");
    frame(&mut editor, &mut renderer);
    let green = colour(&editor);
    assert!(green.y > 0.99 && green.x < 0.01 && green.z < 0.01, "{green:?}");

    // A press outside closes it, and the whole visit is one step to undo.
    let elsewhere = hierarchy_row(&editor.ui, "Ground").unwrap();
    let [x, y, w, h] = editor.ui.rect(elsewhere);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    assert!(!editor.color_picker_open());
    editor.undo(&mut renderer);
    assert_eq!(colour(&editor), was);

    // A material's emission is HDR: its colour, and its intensity in stops.
    frame(&mut editor, &mut renderer);
    open_folder(&mut editor, &mut renderer, "Materials");
    let glow = tile(&editor.ui, "Glow.material").unwrap();
    click(&mut editor.ui, glow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let emission = first_with(&editor.ui, prop_row(&editor.ui, "Emission").unwrap(), "swatch").unwrap();
    click(&mut editor.ui, emission);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(title), "Emission · HDR");
    let stops = find(&editor, "color-intensity-input");
    assert!(editor.ui.is_visible(editor.ui.parent(stops).unwrap()));
    assert_eq!(editor.ui.text(stops), "2.00", "4 is two stops over white");
    type_into(&mut editor.ui, stops, "3");
    frame(&mut editor, &mut renderer);
    let saved: voxear_engine::MaterialData =
        toml::from_str(&std::fs::read_to_string(dir.path().join("Materials/Glow.material")).unwrap()).unwrap();
    let brightest = saved.emissive.into_iter().fold(0.0, f32::max);
    assert!((brightest - 8.0).abs() < 0.01, "three stops: {:?}", saved.emissive);
    assert!((saved.emissive[1] / saved.emissive[0] - 0.25).abs() < 0.01, "the same colour, brighter");
}

/// A program that answers `vox build` as `script` does, for the Build window
/// to follow. `$2` is the project and `$4` the profile.
#[cfg(unix)]
fn fake_vox(dir: &std::path::Path, name: &str, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{script}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn the_build_window_lists_profiles_and_follows_a_build_and_a_cancel() {
    const BUILDS: &str = r#"echo '{"event":"stage","stage":"scripts","index":1,"of":3}'
echo "{\"event\":\"log\",\"level\":\"info\",\"message\":\"building $4\"}"
echo '{"event":"stage","stage":"content","index":2,"of":3}'
echo '{"event":"progress","stage":"content","done":1,"total":2,"item":"Levels/Day.level"}'
echo '{"event":"log","level":"info","message":"cooking Models/Pyramid.gltf"}'
echo '{"event":"log","level":"warn","message":"a warning"}'
echo '{"event":"stage","stage":"player","index":3,"of":3}'
echo "{\"event\":\"finished\",\"result\":\"succeeded\",\"seconds\":1.5,\"output\":\"$2/Builds/X\",\"program\":\"$2/Builds/X/Hello\",\"size\":1048576}"
"#;
    const WAITS: &str = r#"echo '{"event":"stage","stage":"scripts","index":1,"of":3}'
read line
echo "{\"event\":\"finished\",\"result\":\"cancelled\",\"seconds\":0.2,\"message\":\"asked to $line\"}"
"#;
    let dir = hello_copy();
    // Profiles saved in Hello by an editor that opened it are not this test's.
    let _ = std::fs::remove_dir_all(dir.path().join("Settings/BuildProfiles"));
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let find = |editor: &voxear_editor::Editor, name: &str| editor.ui.find(editor.ui.root(), name).unwrap();
    let frames_until =
        |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, done: &dyn Fn(&voxear_editor::Editor) -> bool| {
            for _ in 0..500 {
                if done(editor) {
                    return;
                }
                frame(editor, renderer);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the build window never got there: {:?}", editor.ui.text(find(editor, "build-result")));
        };

    click_named(&mut editor, "build");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let add = find(&editor, "build-add-defaults");
    assert!(editor.ui.is_visible(ui_parent(&editor.ui, add)), "a project without profiles is offered the defaults");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.is_visible(ui_parent(&editor.ui, add)));
    let texts = texts_in(&editor.ui, find(&editor, "build-panel"));
    for shown in ["Linux Development", "Linux Release", "Windows Development", "Windows Release", "Linux", "900"] {
        assert!(texts.iter().any(|text| text == shown), "{shown} in {texts:?}");
    }

    // The picked profile's settings change in place, each written to its file once typed or ticked.
    let release = dir.path().join("Settings/BuildProfiles/Linux Release.buildprofile");
    // Focused rather than clicked: the settings scroll, and Width may be out of sight.
    let width = find(&editor, "build-width");
    editor.ui.focus(width);
    frame(&mut editor, &mut renderer);
    editor.ui.select_all_text(width);
    editor.ui.text_input("1280");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    let written = std::fs::read_to_string(&release).unwrap();
    assert!(written.contains("width = 1280") && written.starts_with("# What a Linux"), "{written}");
    click_named(&mut editor, "build-enabled");
    frame(&mut editor, &mut renderer);
    assert!(std::fs::read_to_string(&release).unwrap().contains("[development]\nenabled = true"));
    click_named(&mut editor, "build-enabled");
    frame(&mut editor, &mut renderer);
    assert!(!std::fs::read_to_string(&release).unwrap().contains("[development]"), "back to the default, so left out");

    let tools = tempfile::tempdir().unwrap();
    editor.build_window().set_program(fake_vox(tools.path(), "vox-builds", BUILDS));
    click_named(&mut editor, "build-start");
    frame(&mut editor, &mut renderer);
    assert!(editor.build_window().is_building());
    frames_until(&mut editor, &mut renderer, &|editor| !editor.ui.text(find(editor, "build-result")).is_empty());
    assert_eq!(editor.ui.text(find(&editor, "build-result")), "Built in 1.5 s · 1.0 MB");
    let log = editor.build_window().log();
    let expected = [("info", "building Linux Release"), ("info", "cooking Models/Pyramid.gltf"), ("warn", "a warning")];
    assert_eq!(log, expected.map(|(level, message)| (level.to_string(), message.to_string())));
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_visible(find(&editor, "build-run")), "a built game can be run");
    let log_lines = |editor: &voxear_editor::Editor| {
        let mut rows = Vec::new();
        with_class(&editor.ui, find(editor, "build-log"), "list-row", &mut rows);
        let shown = rows.into_iter().filter(|&row| editor.ui.is_visible(row));
        shown.map(|row| editor.ui.text(editor.ui.children(row)[0])).collect::<Vec<_>>()
    };
    assert_eq!(editor.ui.text(find(&editor, "build-log-warnings")), "Warnings 1");
    click_named(&mut editor, "build-log-info");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(log_lines(&editor), ["a warning"], "info hidden, warnings shown");
    click_named(&mut editor, "build-log-info");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // A line naming a file in the project shows it in the Project panel.
    assert!(tile(&editor.ui, "Pyramid.gltf").is_none(), "the Project shows the top folder");
    let mut rows = Vec::new();
    with_class(&editor.ui, find(&editor, "build-log"), "list-row", &mut rows);
    let cooking =
        rows.into_iter().find(|&row| editor.ui.text(editor.ui.children(row)[0]).starts_with("cooking")).unwrap();
    click(&mut editor.ui, cooking);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(tile(&editor.ui, "Pyramid.gltf").is_some(), "the Project opened at Models");
    let mut done = Vec::new();
    with_class(&editor.ui, find(&editor, "build-panel"), "done", &mut done);
    assert_eq!(done.len(), 3, "every stage is done");
    shot(&mut renderer, "build.png");

    editor.build_window().set_program(fake_vox(tools.path(), "vox-waits", WAITS));
    click_named(&mut editor, "build-start");
    frames_until(&mut editor, &mut renderer, &|editor| editor.ui.is_visible(find(editor, "build-cancel")));
    assert!(!editor.ui.is_visible(find(&editor, "build-run")), "not while building");
    click_named(&mut editor, "build-cancel");
    frames_until(&mut editor, &mut renderer, &|editor| !editor.ui.text(find(editor, "build-result")).is_empty());
    assert_eq!(editor.ui.text(find(&editor, "build-result")), "Cancelled");
    assert!(!editor.build_window().is_building());
}

#[test]
fn build_profiles_are_made_named_and_given_levels_and_defines_in_fields() {
    let dir = hello_copy();
    let _ = std::fs::remove_dir_all(dir.path().join("Settings/BuildProfiles"));
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    click_named(&mut editor, "build");
    frame(&mut editor, &mut renderer);
    click_named(&mut editor, "build-add-defaults");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let profiles = dir.path().join("Settings/BuildProfiles");
    let read = |name: &str| std::fs::read_to_string(profiles.join(format!("{name}.buildprofile"))).unwrap_or_default();
    // A click on what may have scrolled out of sight, brought into it first.
    let press = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId| {
        let settings = first_with(&editor.ui, editor.ui.root(), "build-settings").unwrap();
        let [_, top, _, height] = editor.ui.rect(settings);
        let [_, y, _, h] = editor.ui.rect(node);
        if y < top || y + h > top + height {
            let [x, at] = editor.ui.scroll_offset(settings);
            editor.ui.set_scroll(settings, [x, (at + y - top - 20.0).max(0.0)]);
            frame(editor, renderer);
        }
        click(&mut editor.ui, node);
        frame(editor, renderer);
        frame(editor, renderer);
    };

    // New makes a profile based on the picked one, named in the title.
    click_named(&mut editor, "build-new-profile");
    frame(&mut editor, &mut renderer);
    assert!(read("New Profile").contains("base = "), "{}", read("New Profile"));
    let title = editor.ui.find(editor.ui.root(), "build-title").unwrap();
    assert_eq!(editor.ui.focused(), Some(title), "its name is ready to type");
    editor.ui.select_all_text(title);
    editor.ui.text_input("Demo");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    assert!(!read("Demo").is_empty() && read("New Profile").is_empty(), "renamed");

    // The start level is a level field: a click offers the project's levels, written by GUID.
    let night = editor.game.content.db().guid_of("Levels/Night.level").unwrap().to_string();
    let day = editor.game.content.db().guid_of("Levels/Day.level").unwrap().to_string();
    let start = editor.ui.find(editor.ui.root(), "build-start-level").unwrap();
    let field = first_with(&editor.ui, start, "build-level-field").unwrap();
    press(&mut editor, &mut renderer, field);
    let item = menu_item(&editor.ui, "Levels/Night.level").expect("the project's levels");
    click(&mut editor.ui, item);
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").contains(&format!("start = \"{night}\"")), "{}", read("Demo"));
    let field = first_with(&editor.ui, start, "build-level-field").unwrap();
    assert_eq!(editor.ui.text(field), "Night", "shown by name");

    // The levels that ship: a list of level fields.
    let add = editor.ui.find(editor.ui.root(), "build-include-add").unwrap();
    press(&mut editor, &mut renderer, add);
    let item = menu_item(&editor.ui, "Levels/Day.level").unwrap();
    click(&mut editor.ui, item);
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").contains(&format!("include = [\"{day}\"]")), "{}", read("Demo"));

    // The defines: a list of fields to type in.
    let add = editor.ui.find(editor.ui.root(), "build-defines-add").unwrap();
    press(&mut editor, &mut renderer, add);
    editor.ui.text_input("DEMO");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").contains("defines = [\"DEMO\"]"), "{}", read("Demo"));
    let add = editor.ui.find(editor.ui.root(), "build-defines-add").unwrap();
    press(&mut editor, &mut renderer, add);
    editor.ui.text_input("FAST");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").contains("defines = [\"DEMO\", \"FAST\"]"), "{}", read("Demo"));
    shot(&mut renderer, "build-lists.png");

    // A define dragged by its grip above the first goes first.
    let defines = editor.ui.find(editor.ui.root(), "build-defines").unwrap();
    let items = first_with(&editor.ui, defines, "collection-items").unwrap();
    let rows = editor.ui.children(items);
    let grip = first_with(&editor.ui, rows[1], "collection-grip").unwrap();
    let from = centre(&editor.ui, grip);
    let top = editor.ui.rect(rows[0])[1];
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=4 {
        editor.ui.pointer_move(from[0], from[1] + (top - from[1]) * step as f32 / 4.0);
        frame(&mut editor, &mut renderer);
    }
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").contains("defines = [\"FAST\", \"DEMO\"]"), "{}", read("Demo"));

    // Each taken out by its item's remove button.
    for _ in 0..2 {
        let defines = editor.ui.find(editor.ui.root(), "build-defines").unwrap();
        let remove = first_with(&editor.ui, defines, "collection-remove").unwrap();
        press(&mut editor, &mut renderer, remove);
    }
    assert!(!read("Demo").contains("defines"), "taken out again: {}", read("Demo"));

    // Remove moves the picked profile into the project's trash.
    click_named(&mut editor, "build-remove-profile");
    frame(&mut editor, &mut renderer);
    assert!(read("Demo").is_empty());
    assert!(dir.path().join(".voxear/Trash/Settings/BuildProfiles/Demo.buildprofile").is_file());
}

fn click_named(editor: &mut voxear_editor::Editor, name: &str) {
    let node = editor.ui.find(editor.ui.root(), name).unwrap();
    click(&mut editor.ui, node);
}

fn ui_parent(ui: &Ui, node: NodeId) -> NodeId {
    ui.parent(node).unwrap()
}

#[test]
fn layouts_are_kept_between_sessions() {
    let dir = hello_copy();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let open = |renderer: &mut Renderer| {
        let project = Project::open(dir.path()).unwrap();
        let mut editor = voxear_editor::Editor::open(project, renderer, LogCapture::default()).unwrap();
        frame(&mut editor, renderer);
        editor
    };
    let mut editor = open(&mut renderer);
    let named = |editor: &voxear_editor::Editor, name: &str| editor.ui.find(editor.ui.root(), name).unwrap();
    let dock = named(&editor, "dock");
    editor.ui.close_dock_panel(dock, "project");
    frame(&mut editor, &mut renderer);
    let world = editor.ui.dock_layout(dock);
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    editor.ui.close_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    let profiling = editor.ui.dock_layout(dock);
    assert_ne!(world, profiling);
    // Save Layout As asks for a name under the Layout menu's button, and shows the new layout.
    let layouts = named(&editor, "layouts");
    click(&mut editor.ui, layouts);
    frame(&mut editor, &mut renderer);
    let save_as = menu_item(&editor.ui, "Save Layout As…").unwrap();
    click(&mut editor.ui, save_as);
    // Its field takes the focus once the box has been laid out, as it shows.
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_visible(named(&editor, "layout-prompt")), "a box for its name");
    assert_eq!(editor.ui.focused(), Some(named(&editor, "layout-prompt-name")), "ready to type in");
    editor.ui.text_input("Lighting");
    frame(&mut editor, &mut renderer);
    let save = named(&editor, "layout-prompt-save");
    click(&mut editor.ui, save);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.layout_name(), "Lighting");
    assert!(!editor.ui.is_visible(named(&editor, "layout-prompt")));
    editor.ui.close_dock_panel(dock, "memory");
    frame(&mut editor, &mut renderer);
    let lighting = editor.ui.dock_layout(dock);
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    let profiling = editor.ui.dock_layout(dock);
    // The layout is looked at once a second.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    frame(&mut editor, &mut renderer);
    drop(editor);

    let mut editor = open(&mut renderer);
    let dock = named(&editor, "dock");
    assert_eq!(editor.layout_name(), "Profiling", "the layout it was closed in");
    assert_eq!(editor.ui.dock_layout(dock), profiling);
    assert!(editor.use_layout("Default"));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.dock_layout(dock), world, "and the others as they were left");
    assert!(editor.use_layout("Lighting"), "a saved layout is kept too");
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.dock_layout(dock), lighting);
}

#[test]
fn scene_views_and_inspectors_from_the_menu_come_back_with_their_layout() {
    let dir = hello_copy();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let open = |renderer: &mut Renderer| {
        let project = Project::open(dir.path()).unwrap();
        let mut editor = voxear_editor::Editor::open(project, renderer, LogCapture::default()).unwrap();
        frame(&mut editor, renderer);
        editor
    };
    let second = |editor: &voxear_editor::Editor| {
        let cameras = editor.scene_view_cameras();
        cameras.into_iter().find(|(panel, _)| panel == "scene-2").map(|(_, camera)| camera)
    };
    let place = |editor: &voxear_editor::Editor, camera| *editor.game.engine.world.get::<Transform>(camera).unwrap();
    // The Scene views' shading menus, the first's first.
    let shading_menus = |editor: &voxear_editor::Editor| {
        let mut menus = Vec::new();
        with_class(&editor.ui, editor.ui.root(), "shading", &mut menus);
        menus
    };

    // A second Scene view, looking from somewhere of its own and shaded as Wireframe, and a second Inspector,
    // floating.
    let mut editor = open(&mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    assert_eq!(editor.add_scene_view("scene"), "scene-2");
    assert_eq!(editor.add_inspector("inspector"), "inspector-2");
    editor.ui.float_dock_panel(dock, "inspector-2", None);
    frame(&mut editor, &mut renderer);
    let camera = second(&editor).unwrap();
    let mut flown = place(&editor, camera);
    flown.position += Vec3::new(3.0, 1.0, -2.0);
    flown.rotation = voxear_core::math::Quat::from_rotation_y(0.7);
    *editor.game.engine.world.get_mut::<Transform>(camera).unwrap() = flown;
    let menu = shading_menus(&editor)[1];
    choose(&mut editor, &mut renderer, menu, 2);
    let layout = editor.ui.dock_layout(dock);
    assert!(layout.contains("scene-2*]") && layout.contains("[inspector-2]"), "{layout}");
    // The layout is looked at once a second.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    frame(&mut editor, &mut renderer);
    drop(editor);

    // Opened again, both are back in place, the Scene view looking from where it was left and shaded as it was.
    let mut editor = open(&mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    assert_eq!(editor.ui.dock_layout(dock), layout, "both where they were");
    assert_eq!((editor.scene_view_count(), editor.inspector_count()), (2, 2));
    let camera = second(&editor).expect("the second Scene view");
    assert_eq!(place(&editor, camera).position, flown.position);
    assert!(place(&editor, camera).rotation.abs_diff_eq(flown.rotation, 1e-6));
    assert_eq!(editor.ui.selected_option(shading_menus(&editor)[1]), 2, "Wireframe");
    assert_eq!(editor.settings().scene_shading, 0, "the first's shading stays its own");

    // A layout that leaves them out takes them away for good, as closing them does; back on the one that names
    // them, they open again as they were.
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    assert_eq!((editor.scene_view_count(), editor.inspector_count()), (1, 1));
    let panels = editor.ui.dock_panels(dock);
    assert!(!panels.iter().any(|(id, ..)| id == "scene-2" || id == "inspector-2"), "gone, not just closed");
    assert!(!editor.game.engine.world.is_alive(camera), "its camera with it");
    assert!(editor.use_layout("Default"));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.dock_layout(dock), layout);
    let camera = second(&editor).expect("the second Scene view, again");
    assert_eq!(place(&editor, camera).position, flown.position);
    assert_eq!(editor.ui.selected_option(shading_menus(&editor)[1]), 2);
}

#[test]
fn voxdocs_shows_a_pages_pictures() {
    let dir = hello_copy();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "docs");
    let tree = editor.ui.find(root, "cb-tree").unwrap();
    for item in 0..12 {
        editor.ui.set_expanded(tree, item, true);
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut rows = Vec::new();
    with_class(&editor.ui, root, "cb-row-title", &mut rows);
    let page = rows.into_iter().find(|&row| editor.ui.text(row) == "The Day and Night Cycle").expect("the page's row");
    click(&mut editor.ui, page);
    let mut pictures = Vec::new();
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
        pictures.clear();
        with_class(&editor.ui, root, "md-picture", &mut pictures);
        if !pictures.is_empty() {
            break;
        }
    }
    let &[picture] = pictures.as_slice() else { panic!("the Day level's picture shows: {pictures:?}") };
    frame(&mut editor, &mut renderer);
    let [_, _, width, height] = editor.ui.rect(picture);
    assert!(width > 100.0 && (width / height - 16.0 / 9.0).abs() < 0.02, "fits the page, keeping its shape");
    assert!(!dir.path().join("Docs/How Hello Works/images/Day.png.meta").exists(), "a page's picture is not an asset");
    shot(&mut renderer, "docs-picture.png");
}

#[test]
fn the_console_saves_filters_hides_assemblies_and_colours_categories() {
    use tracing::Level;
    use voxear_core::logging::LogLine;
    let dir = hello_copy();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let log = LogCapture::default();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    let root = editor.game.project.root.canonicalize().unwrap();
    let engine = LogLine { target: "voxear_engine::levels".into(), ..LogLine::new(Level::INFO, "Level Main loaded") };
    log_line(&log, engine);
    let stack = format!(
        "   at Voxear.Log.Info(String message)\n   at Hello.Spinner.Start() in {}:line 33",
        root.join("Scripts/Spinner.cs").display()
    );
    let script =
        LogLine { target: "script".into(), stack: Some(stack), ..LogLine::new(Level::INFO, "Spinner started") };
    log_line(&log, script);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let roomy = "row(240, [hierarchy], row(-300, column(45%, [scene, game, docs], [console, project]), [inspector]))";
    editor.ui.set_dock_layout(dock, roomy).unwrap();
    editor.ui.show_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let categories: Vec<String> =
        console_rows(&editor.ui).into_iter().map(|row| editor.ui.text(editor.ui.children(row)[2])).collect();
    assert_eq!(categories, ["Engine", "Hello"], "each line shows its category");

    // A search saved as a filter comes back by name.
    search_console(&mut editor, &mut renderer, "spinner");
    let filters = console_button(&editor.ui, "Filters").unwrap();
    click(&mut editor.ui, filters);
    frame(&mut editor, &mut renderer);
    assert_eq!(menu_texts(&editor.ui), ["No saved filters yet", "Save this filter…", "Forget this filter"]);
    let save = menu_item(&editor.ui, "Save this filter…").unwrap();
    click(&mut editor.ui, save);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("Spinners");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    search_console(&mut editor, &mut renderer, "");
    assert_eq!(console_lines(&editor.ui).len(), 2);
    click(&mut editor.ui, filters);
    frame(&mut editor, &mut renderer);
    assert_eq!(menu_texts(&editor.ui)[0], "Spinners");
    let spinners = menu_item(&editor.ui, "Spinners").unwrap();
    click(&mut editor.ui, spinners);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(console_lines(&editor.ui), ["Spinner started"], "the filter's search is back");

    // Hiding an assembly folds its frames away in the trace.
    let row = console_rows(&editor.ui)[0];
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    let mine_only = console_button(&editor.ui, "My code only").unwrap();
    click(&mut editor.ui, mine_only);
    frame(&mut editor, &mut renderer);
    let hide = console_button(&editor.ui, "Hide libraries…").unwrap();
    click(&mut editor.ui, hide);
    frame(&mut editor, &mut renderer);
    assert_eq!(menu_texts(&editor.ui), ["Voxear", "Hello"], "the assemblies in this stack");
    let voxear = menu_item(&editor.ui, "Voxear").unwrap();
    click(&mut editor.ui, voxear);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut traces = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-trace", &mut traces);
    let trace = editor.ui.text(traces[0]);
    assert!(trace.starts_with("… frame from hidden assemblies"), "{trace}");
    assert!(trace.contains("Hello.Spinner.Start()"), "{trace}");

    // A category's colour is kept with the project. The Hide menu stays open
    // for the next choice, so Escape puts it away first.
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    context_click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    let colour = menu_item(&editor.ui, "Colour “Hello”…").unwrap();
    click(&mut editor.ui, colour);
    frame(&mut editor, &mut renderer);
    let blue = menu_item(&editor.ui, "Blue").unwrap();
    click(&mut editor.ui, blue);
    frame(&mut editor, &mut renderer);
    let kept = std::fs::read_to_string(dir.path().join("Settings/Console.toml")).unwrap();
    assert!(kept.contains("\"Hello\" = \"#6aa8ff\""), "{kept}");
}

/// Hello's Spinner as a build with a C# `Color` field on it describes it.
const SPINNER_WITH_TITLE: &str = r#"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "title", "type": {"kind": "localized"}, "default": ""}
]}], "types": {}, "commands": []}"#;

#[test]
fn a_localized_string_field_picks_a_key_beside_its_text() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let table = "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\n\n[[entry]]\nkey = \"menu.quit\"\nen = \"Quit\"\n";
    std::fs::write(dir.path().join("Menu.strings"), table).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_TITLE).unwrap());
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    let picker = first_with(&editor.ui, prop_row(&editor.ui, "Title").unwrap(), "value-picker").unwrap();
    let label = first_with(&editor.ui, picker, "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "None");
    let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered = picker_rows(&editor.ui);
    assert_eq!(offered[1..], [("menu.play".into(), "Play".into()), ("menu.quit".into(), "Quit".into())], "{offered:?}");
    shot(&mut renderer, "localized-string-picker.png");

    let row = picker_row(&editor.ui, "menu.quit").unwrap();
    click_across_frames(&mut editor, &mut renderer, row);
    frame(&mut editor, &mut renderer);
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
    assert_eq!(kept.0[0].fields.get("title"), Some(&toml::Value::String("menu.quit".into())), "written as its key");
    let label = first_with(&editor.ui, prop_row(&editor.ui, "Title").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "menu.quit");
}

#[test]
fn a_string_table_is_edited_in_the_inspector() {
    use voxear_engine::localization::{Localization, StringTable};
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project_file = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&project_file).unwrap() + "\n[localization]\nlanguages = [\"en\", \"fr\"]\n";
    std::fs::write(&project_file, text).unwrap();
    let table = "[[entry]]\nkey = \"menu.play\"\nlimit = 5\nen = \"Play\"\n\n[[entry]]\nkey = \"menu.quit\"\nen = \"Quit\"\nfr = \"Quitter\"\n";
    std::fs::write(dir.path().join("Menu.strings"), table).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let menu = tile(&editor.ui, "Menu.strings").expect("the table in the Project panel");
    click(&mut editor.ui, menu);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Menu.strings"));
    let read = || StringTable::parse(&std::fs::read_to_string(dir.path().join("Menu.strings")).unwrap()).unwrap();
    let mut fields = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "value-input", &mut fields);
    let text_of = |editor: &voxear_editor::Editor, shown: &str| {
        fields.iter().copied().find(|&f| editor.ui.text(f) == shown).unwrap_or_else(|| panic!("a field showing {shown}"))
    };
    // French has no text for menu.play: its field says so.
    let missing = fields.iter().copied().filter(|&f| editor.ui.has_class(f, "missing")).count();
    assert_eq!(missing, 1, "menu.play has no French");
    shot(&mut renderer, "string-table.png");

    // A text typed and let go of is written, and the game has it.
    let quit = text_of(&editor, "Quitter");
    type_into(&mut editor.ui, quit, "Sortir");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert_eq!(read().entries[1].text("fr"), Some("Sortir"));
    let mut french = editor.game.engine.world.resource::<Localization>().unwrap().clone();
    assert!(french.set_language("fr") && french.find("menu.quit") == Some("Sortir"), "read again");

    // A key another entry has is refused; Add Entry makes a new one.
    let key = text_of(&editor, "menu.quit");
    type_into(&mut editor.ui, key, "menu.play");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert_eq!((read().entries[1].key.as_str(), editor.ui.text(key).as_str()), ("menu.quit", "menu.quit"));
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").unwrap();
    scroll_to(&mut editor, &mut renderer, add);
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let keys: Vec<String> = read().entries.into_iter().map(|e| e.key).collect();
    assert_eq!(keys, ["menu.play", "menu.quit", "new.key"]);
}

#[test]
fn audio_sources_show_in_the_scene_view_with_their_reach() {
    use voxear_engine::audio::AudioSource;
    let Some((mut renderer, mut editor)) = open() else { return };
    let spinner = obj(&editor, "Spinner A").unwrap();
    let source = AudioSource { min_distance: 1.0, max_distance: 3.0, ..AudioSource::default() };
    editor.game.engine.world.insert(spinner, source);
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = prop_row(&editor.ui, "Doppler level").expect("a 3D source's Doppler level");
    assert!(first_with(&editor.ui, row, "value-input").is_some());
    shot(&mut renderer, "audio-gizmos.png");
}

/// Takes away a copy of Hello's own input actions, if it has any, for a test
/// to start from Voxear's.
fn without_own_actions(dir: &std::path::Path) {
    for name in ["Input.inputactions", "Input.inputactions.meta"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

const ACTIONS_WITH_CATEGORIES: &str = r##"format = 1

[[category]]
name = "Movement"
color = "#3fb950"

[[category]]
name = "Combat"
color = "#a371f7"

[[category]]
name = "Utility"
color = "#f0883e"

[[map]]
name = "Player"

[[map.action]]
name = "Move"
type = "vector2"
category = "Movement"
bindings = [{ up = "Keyboard/W", down = "Keyboard/S", left = "Keyboard/A", right = "Keyboard/D" }, "Gamepad/LeftStick"]

[[map.action]]
name = "Jump"
category = "Movement"
icon = "arrow-up"
bindings = ["Keyboard/Space", "Gamepad/South"]

[[map.action]]
name = "Sprint"
category = "Movement"
bindings = ["Keyboard/ShiftLeft", "Gamepad/LeftStickPress"]

[[map.action]]
name = "Attack"
category = "Combat"
bindings = ["Mouse/Left", "Gamepad/RightTrigger"]

[[map.action]]
name = "Block"
category = "Combat"
bindings = ["Mouse/Right", "Gamepad/LeftTrigger"]

[[map.action]]
name = "Cast Spell"
category = "Combat"
bindings = ["Keyboard/Q", "Gamepad/West"]

[[map.action]]
name = "Open Map"
category = "Utility"
bindings = ["Keyboard/M", "Gamepad/Select"]

[[map.action]]
name = "Inventory"
category = "Utility"
bindings = ["Keyboard/Tab", "Gamepad/North"]

[[map.action]]
name = "Look"
type = "vector2"
bindings = ["Mouse/Delta", "Gamepad/RightStick"]

[[map.action]]
name = "Pause"
bindings = ["Keyboard/Escape", "Gamepad/Start"]

[[map.action]]
name = "Noclip"
hidden = true
bindings = ["Keyboard/F9"]

[[map]]
name = "UI"
excludes = ["Player"]

[[map.action]]
name = "Submit"
bindings = ["Keyboard/Space"]

[[map]]
name = "Debug"

[[map.action]]
name = "Fly"
bindings = ["Keyboard/Q"]
"##;

#[test]
fn the_input_actions_window_draws_the_keyboard_and_the_pad_with_their_actions() {
    use voxear_editor::PaletteKey;
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    without_own_actions(dir.path());
    std::fs::write(dir.path().join("Controls.inputactions"), ACTIONS_WITH_CATEGORIES).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("input actions");
    frame(&mut editor, &mut renderer);
    editor.palette_key(PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    // It opens on its Actions, a row a binding with its control's name and device, every other row shaded.
    let view = editor.ui.find(editor.ui.root(), "input-view").expect("the Input Actions window");
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }
    let mut rows = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "input-row", &mut rows);
    let texts: Vec<Vec<String>> = rows.iter().map(|&row| texts_in(&editor.ui, row)).collect();
    assert!(texts.contains(&vec!["Space".to_string(), "Keyboard".to_string()]), "{texts:?}");
    assert!(rows.iter().any(|&row| editor.ui.has_class(row, "odd")));
    // The key maps are its Layout.
    editor.ui.set_segment(view, 1);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let keymap = editor.ui.find(editor.ui.root(), "input-keymap").unwrap();
    let mut keys = Vec::new();
    with_class(&editor.ui, keymap, "keymap-key", &mut keys);
    let lit = |editor: &voxear_editor::Editor, keys: &[NodeId]| {
        keys.iter().filter(|&&k| editor.ui.has_class(k, "assigned")).count()
    };
    assert_eq!(lit(&editor, &keys), 12, "W, A, S, D, Space, Shift, M, Tab, Esc and the mouse's left, right and body");
    let conflicts: Vec<String> =
        keys.iter().filter(|&&k| editor.ui.has_class(k, "conflict")).flat_map(|&k| texts_in(&editor.ui, k)).collect();
    assert_eq!(conflicts, ["Q"], "Fly's Debug map can be on with Player; UI's Submit on Space cannot");
    let mut callouts = Vec::new();
    with_class(&editor.ui, keymap, "keymap-callout", &mut callouts);
    assert_eq!(callouts.len(), 13, "Noclip's F9 is hidden, unlabelled");
    let mut icons = Vec::new();
    with_class(&editor.ui, keymap, "keymap-callout-icon", &mut icons);
    assert_eq!(icons.len(), 1, "Jump's icon beside it by Space");
    let mut body = Vec::new();
    with_class(&editor.ui, keymap, "keymap-body", &mut body);
    assert_eq!(body.len(), 2, "the keyboard and the mouse drawn behind their keys");
    shot(&mut renderer, "input-actions-keyboard.png");

    // Coloured by category, then the gamepad.
    let colors = editor.ui.find(editor.ui.root(), "input-colors").unwrap();
    let categories = editor.ui.children(colors)[1];
    click(&mut editor.ui, categories);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "input-actions-categories.png");
    let scheme = editor.ui.find(editor.ui.root(), "input-scheme").unwrap();
    let pad = editor.ui.children(scheme)[1];
    click(&mut editor.ui, pad);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut keys = Vec::new();
    with_class(&editor.ui, keymap, "keymap-key", &mut keys);
    assert_eq!(lit(&editor, &keys), 9, "Sprint shares the left stick, which it presses");
    shot(&mut renderer, "input-actions-gamepad.png");
}

#[test]
fn the_palette_pseudo_localises_the_games_text() {
    use voxear_editor::PaletteKey;
    use voxear_engine::localization::Localization;
    let Some((mut renderer, mut editor)) = open() else { return };
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    let pseudo = |editor: &voxear_editor::Editor| editor.game.engine.world.resource::<Localization>().unwrap().pseudo();
    for on in [true, false] {
        editor.ui.focus(field);
        frame(&mut editor, &mut renderer);
        editor.ui.text_input("pseudo");
        frame(&mut editor, &mut renderer);
        assert_eq!(palette_results(&editor.ui).first().map(String::as_str), Some("Pseudo-localise Text"));
        editor.palette_key(PaletteKey::Enter);
        frame(&mut editor, &mut renderer);
        assert_eq!(pseudo(&editor), on);
    }
}

const SPINNER_WITH_COLOUR: &str = r#"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "tint", "type": {"kind": "color"}, "default": [1.0, 0.5, 0.0, 0.5]}
]}], "types": {}, "commands": []}"#;

#[test]
fn a_scripts_colour_field_shows_a_swatch_and_keeps_its_alpha() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_COLOUR).unwrap());
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let tint = |editor: &voxear_editor::Editor| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0[0].fields.get("tint").cloned()
    };

    let row = prop_row(&editor.ui, "Tint").expect("a row for the colour");
    let swatch = first_with(&editor.ui, row, "swatch").expect("with a swatch");
    assert_eq!(editor.ui.style(swatch).unwrap().background, Some(voxear_ui::Color::hex(0xff8000)), "in its colour");
    let mut axes = Vec::new();
    with_class(&editor.ui, row, "axis-input", &mut axes);
    let shown: Vec<String> = axes.iter().map(|&a| editor.ui.text(a)).collect();
    assert_eq!(shown, ["1", "0.5", "0"], "red, green and blue as the script starts them");

    type_into(&mut editor.ui, axes[2], "0.25");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let written = tint(&editor).and_then(|v| v.as_array().cloned()).expect("the scene sets it now");
    let numbers: Vec<f64> = written.iter().filter_map(toml::Value::as_float).collect();
    assert_eq!(numbers, [1.0, 0.5, 0.25, 0.5], "the alpha stays as it was");

    // A scene can write it as hex.
    let kept = &mut editor.game.engine.world.get_mut::<voxear_engine::EditBehaviours>(spinner).unwrap().0[0];
    std::sync::Arc::make_mut(&mut kept.fields).insert("tint".into(), toml::Value::String("#00ff00".into()));
    editor.select(None);
    frame(&mut editor, &mut renderer);
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut axes = Vec::new();
    with_class(&editor.ui, prop_row(&editor.ui, "Tint").unwrap(), "axis-input", &mut axes);
    let shown: Vec<String> = axes.iter().map(|&a| editor.ui.text(a)).collect();
    assert_eq!(shown, ["0", "1", "0"], "hex reads as its numbers");

    let swatch_now = first_with(&editor.ui, prop_row(&editor.ui, "Tint").unwrap(), "swatch").unwrap();
    click(&mut editor.ui, swatch_now);
    frame(&mut editor, &mut renderer);
    assert!(editor.color_picker_open(), "the swatch opens the colour picker");
}

/// Hello's Spinner as a build with a table, a paged list and a palette describes it.
const SPINNER_WITH_LISTS: &str = r##"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "waves", "type": {"kind": "list", "element": {"kind": "struct", "name": "Hello.Wave"}}, "default": [],
   "inspector": {"table": true}},
  {"name": "scores", "type": {"kind": "list", "element": {"kind": "whole"}}, "default": [],
   "inspector": {"list": {"per_page": 3, "index_labels": true}}},
  {"name": "tint", "type": {"kind": "color"}, "default": [1.0, 1.0, 1.0, 1.0],
   "inspector": {"palette": ["#e63946", "#457b9d"]}}
]}], "types": {"Hello.Wave": {"fields": [
  {"name": "count", "type": {"kind": "whole"}, "default": 0},
  {"name": "delay", "type": {"kind": "number"}, "default": 0.5}
]}}, "commands": []}"##;

#[test]
fn tables_pages_and_palettes_show_lists_and_colours() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_LISTS).unwrap());
    let spinner = obj(&editor, "Spinner A").unwrap();
    {
        let kept = &mut editor.game.engine.world.get_mut::<voxear_engine::EditBehaviours>(spinner).unwrap().0[0];
        let fields = std::sync::Arc::make_mut(&mut kept.fields);
        let waves: toml::Value = toml::from_str("w = [{ count = 2, delay = 1.0 }, { count = 5 }]").unwrap();
        fields.insert("waves".into(), waves["w"].clone());
        fields.insert("scores".into(), toml::Value::Array((1..=7).map(|n| toml::Value::Integer(n * 10)).collect()));
    }
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let field = |editor: &voxear_editor::Editor, name: &str| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0[0].fields.get(name).cloned()
    };
    shot(&mut renderer, "inspector-lists.png");

    // [TableList]: a column for each of a wave's fields, a row for each wave.
    let table = first_with(&editor.ui, editor.ui.root(), "attr-table").expect("the waves as a table");
    let mut heads = Vec::new();
    with_class(&editor.ui, table, "attr-table-head", &mut heads);
    let heads: Vec<String> = heads.iter().map(|&h| editor.ui.text(h)).collect();
    assert_eq!(heads, ["Count", "Delay"]);
    let mut cells = Vec::new();
    with_class(&editor.ui, table, "value-input", &mut cells);
    let shown: Vec<String> = cells.iter().map(|&c| editor.ui.text(c)).collect();
    assert_eq!(shown, ["2", "1", "5", "0.5"], "each wave's values, with the default where it sets none");
    assert!(prop_row(&editor.ui, "Count").is_none(), "no rows of their own beside the table");
    type_into(&mut editor.ui, cells[2], "7");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let count = |editor: &voxear_editor::Editor, wave: usize| field(editor, "waves").unwrap()[wave]["count"].clone();
    assert_eq!(count(&editor, 1), toml::Value::Integer(7), "a cell edits its field");
    let mut removes = Vec::new();
    with_class(&editor.ui, table, "list-remove", &mut removes);
    click(&mut editor.ui, removes[0]);
    frame(&mut editor, &mut renderer);
    assert_eq!(field(&editor, "waves").unwrap().as_array().unwrap().len(), 1, "a row's button takes it out");

    // [ListDrawerSettings]: three scores at a time, each labelled by its index.
    let labels = |editor: &voxear_editor::Editor| -> Vec<String> {
        (0..7).map(|i| i.to_string()).filter(|l| prop_row(&editor.ui, l).is_some()).collect()
    };
    frame(&mut editor, &mut renderer);
    assert_eq!(labels(&editor), ["0", "1", "2"], "the first page");

    // An item's grip drags it onto another, which moves it there, as a step to undo.
    let scores = |editor: &voxear_editor::Editor| -> Vec<i64> {
        field(editor, "scores").unwrap().as_array().unwrap().iter().filter_map(toml::Value::as_integer).collect()
    };
    let item_of = |ui: &Ui, row: NodeId| {
        let mut at = Some(row);
        while let Some(node) = at.filter(|&node| !ui.has_class(node, "collection-item")) {
            at = ui.parent(node);
        }
        at.expect("a list's item")
    };
    let first = item_of(&editor.ui, prop_row(&editor.ui, "0").unwrap());
    let handle = first_with(&editor.ui, first, "collection-grip").expect("its grip");
    let third = item_of(&editor.ui, prop_row(&editor.ui, "2").unwrap());
    drag(&mut editor.ui, handle, third);
    frame(&mut editor, &mut renderer);
    assert_eq!(scores(&editor), [20, 30, 10, 40, 50, 60, 70], "the first score moved to the third place");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(scores(&editor), [10, 20, 30, 40, 50, 60, 70], "undone");
    let pager = first_with(&editor.ui, editor.ui.root(), "attr-pager").unwrap();
    assert_eq!(editor.ui.text(first_with(&editor.ui, pager, "attr-pager-text").unwrap()), "Page 1 of 3");
    let on = *editor.ui.children(pager).last().unwrap();
    click(&mut editor.ui, on);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(labels(&editor), ["3", "4", "5"], "the next page");

    // [ColorPalette]: a swatch picks its colour.
    let mut swatches = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "attr-palette-swatch", &mut swatches);
    assert_eq!(swatches.len(), 2);
    scroll_to(&mut editor, &mut renderer, swatches[1]);
    click(&mut editor.ui, swatches[1]);
    frame(&mut editor, &mut renderer);
    let tint = field(&editor, "tint").unwrap();
    let red = tint[0].as_float().unwrap();
    assert!((red - 0x45 as f64 / 255.0).abs() < 1e-3, "#457b9d's red: {tint}");
}

/// Hello's Spinner as a build with inspector attributes on it describes it.
const SPINNER_WITH_ATTRIBUTES: &str = r#"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "degreesPerSecond", "type": {"kind": "number"}, "default": 90.0,
   "inspector": {"group": {"kind": "foldout", "name": "Motion"}, "range": [0, 720], "suffix": "°/s", "tooltip": "How fast it turns."}},
  {"name": "mode", "type": {"kind": "enum", "names": ["Steady", "Wobble"]}, "default": "Steady",
   "inspector": {"group": {"kind": "foldout", "name": "Motion"}, "enum_buttons": true}},
  {"name": "wobble", "type": {"kind": "number"}, "default": 0.25,
   "inspector": {"group": {"kind": "foldout", "name": "Motion"}, "show_if": {"member": "mode", "value": "Wobble"}, "progress": [0, 1]}},
  {"name": "leader", "type": {"kind": "behaviour", "name": "Hello.Spinner"}, "default": null,
   "inspector": {"required": "", "info": [{"message": "Spins the other way at the leader's speed.", "type": "info"}]}},
  {"name": "notes", "type": {"kind": "text"}, "default": "", "inspector": {"order": -1, "label": "Notes", "multiline": 3}},
  {"name": "secret", "type": {"kind": "whole"}, "default": 7, "inspector": {"hidden": true}}
]}], "types": {}, "commands": []}"#;

#[test]
fn inspector_attributes_shape_how_script_fields_show() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_ATTRIBUTES).unwrap());
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let field = |editor: &voxear_editor::Editor, name: &str| {
        let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
        kept.0[0].fields.get(name).cloned()
    };
    let top = |editor: &voxear_editor::Editor, label: &str| editor.ui.rect(prop_row(&editor.ui, label).unwrap())[1];
    let within = |editor: &voxear_editor::Editor, node: NodeId, class: &str| {
        let mut at = Some(node);
        while let Some(n) = at {
            if editor.ui.has_class(n, class) {
                return true;
            }
            at = editor.ui.parent(n);
        }
        false
    };
    shot(&mut renderer, "inspector-attributes.png");

    // [PropertyOrder], [LabelText], [Multiline], [HideInInspector] and [FoldoutGroup].
    assert!(top(&editor, "Notes") < top(&editor, "Degrees Per Second"), "Notes comes first");
    let notes = first_with(&editor.ui, prop_row(&editor.ui, "Notes").unwrap(), "attr-multiline");
    assert!(notes.is_some(), "a multiline field");
    assert!(prop_row(&editor.ui, "Secret").is_none(), "a hidden field does not show");
    let speed = prop_row(&editor.ui, "Degrees Per Second").unwrap();
    assert!(within(&editor, speed, "attr-foldout"), "in the Motion foldout");

    // [Range] clamps what is typed, with its slider; [Suffix] shows the unit.
    assert!(first_with(&editor.ui, speed, "attr-slider").is_some());
    assert_eq!(editor.ui.text(first_with(&editor.ui, speed, "attr-suffix").unwrap()), "°/s");
    let typed = first_with(&editor.ui, speed, "attr-range-input").unwrap();
    type_into(&mut editor.ui, typed, "800");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    assert_eq!(field(&editor, "degreesPerSecond"), Some(toml::Value::Float(720.0)), "held to the range");

    // [Required] and [InfoBox] above the leader.
    let mut boxes = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "info-text", &mut boxes);
    let messages: Vec<String> = boxes.iter().map(|&b| editor.ui.text(b)).collect();
    assert!(messages.contains(&"Leader is required".to_string()), "{messages:?}");
    assert!(messages.contains(&"Spins the other way at the leader's speed.".to_string()));

    // [EnumToggleButtons], and [ShowIf] following the enum, with a [ProgressBar] dragged.
    assert!(prop_row(&editor.ui, "Wobble").is_none(), "shown only while the mode is Wobble");
    let buttons = first_with(&editor.ui, prop_row(&editor.ui, "Mode").unwrap(), "attr-buttons").unwrap();
    let segments = editor.ui.children(buttons);
    click(&mut editor.ui, segments[1]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(field(&editor, "mode"), Some(toml::Value::String("Wobble".into())));
    let wobble = prop_row(&editor.ui, "Wobble").expect("shown now");
    let bar = first_with(&editor.ui, wobble, "attr-progress").unwrap();
    let [x, y, w, h] = editor.ui.rect(bar);
    editor.ui.pointer_move(x + w * 0.75, y + h / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let wobbled = field(&editor, "wobble").and_then(|v| v.as_float()).unwrap();
    assert!((wobbled - 0.75).abs() < 0.02, "the bar sets it: {wobbled}");
    let opened = prop_row(&editor.ui, "Degrees Per Second").unwrap();
    assert!(within(&editor, opened, "attr-foldout"), "built again, still in its foldout");

    // [Tooltip], once the pointer rests on the label.
    let label = editor.ui.children(prop_row(&editor.ui, "Degrees Per Second").unwrap())[0];
    let [x, y, w, h] = editor.ui.rect(label);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    let tooltip = editor.ui.find(editor.ui.root(), "tooltip").unwrap();
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.is_visible(tooltip), "not at once");
    for _ in 0..40 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.ui.is_visible(tooltip));
    assert_eq!(editor.ui.text(tooltip), "How fast it turns.");
    shot(&mut renderer, "inspector-attributes-wobble.png");
}

/// Hello's Spinner as a build with a value whose clip shows only for one
/// mode describes it, the clip a value of its own.
const SPINNER_WITH_A_HIDDEN_VALUE: &str = r#"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "shot", "type": {"kind": "struct", "name": "Hello.Shot"}, "default": {}}
]}], "types": {"Hello.Shot": {"fields": [
  {"name": "mode", "type": {"kind": "enum", "names": ["Plain", "Clip"]}, "default": "Plain"},
  {"name": "clip", "type": {"kind": "struct", "name": "Hello.Take"}, "default": {},
   "inspector": {"show_if": {"member": "mode", "value": "Clip"}}}
]}, "Hello.Take": {"fields": [
  {"name": "start", "type": {"kind": "number"}, "default": 0.0}
]}}, "commands": []}"#;

#[test]
fn a_value_shown_only_if_hides_what_is_in_it() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(SPINNER_WITH_A_HIDDEN_VALUE).unwrap());
    let (plain, clipped) = (obj(&editor, "Spinner A").unwrap(), obj(&editor, "Spinner B").unwrap());
    {
        let kept = &mut editor.game.engine.world.get_mut::<voxear_engine::EditBehaviours>(clipped).unwrap().0[0];
        let shot: toml::Value = toml::from_str("shot = { mode = \"Clip\", clip = { start = 0.5 } }").unwrap();
        std::sync::Arc::make_mut(&mut kept.fields).insert("shot".into(), shot["shot"].clone());
    }
    editor.select(Some(plain));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Mode").is_some());
    assert!(prop_row(&editor.ui, "Clip").is_none() && prop_row(&editor.ui, "Start").is_none(), "the clip and its start hide");

    editor.select(Some(clipped));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Clip").is_some() && prop_row(&editor.ui, "Start").is_some(), "both show for a clip");
}

/// Hello's build with a C# component in it, as ScriptInfo describes one.
const WITH_A_COMPONENT: &str = r#"{"behaviours": [], "components": [{"name": "Hello.Spin", "bases": [],
  "interfaces": ["Voxear.IComponent"], "fields": [
  {"name": "Speed", "type": {"kind": "number"}, "default": 0.0, "inspector": {"suffix": "°/s"}},
  {"name": "Axis", "type": {"kind": "vector", "size": 3}, "default": [0.0, 0.0, 0.0]}
]}], "types": {}, "commands": []}"#;

#[test]
fn csharp_components_are_added_edited_and_saved_like_scripts() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(WITH_A_COMPONENT).unwrap());
    let chrome = obj(&editor, "Chrome Ball").unwrap();
    editor.select(Some(chrome));
    frame(&mut editor, &mut renderer);

    add_component(&mut editor, &mut renderer, "Spin");
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(chrome).unwrap();
    assert_eq!((kept.0[0].type_name.as_str(), kept.0[0].component), ("Hello.Spin", true));

    // Its fields show as a script's do, and edit the same way.
    let speed = prop_row(&editor.ui, "Speed").expect("its fields show");
    let input = first_with(&editor.ui, speed, "value-input").unwrap();
    type_into(&mut editor.ui, input, "45");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(chrome).unwrap();
    assert_eq!(kept.0[0].fields.get("Speed"), Some(&toml::Value::Float(45.0)));
    shot(&mut renderer, "inspector-component.png");

    editor.save();
    let day = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    let saved: voxear_engine::scene_file::SceneFile = toml::from_str(&day).unwrap();
    let ball = saved.objs.iter().find(|o| o.name == "Chrome Ball").unwrap();
    assert_eq!(ball.components, ["Hello.Spin"]);
    assert!(ball.behaviours.is_empty());
    assert_eq!(ball.fields["Hello.Spin"]["Speed"], toml::Value::Float(45.0));
}

#[test]
fn voxdocs_shows_prefabs_and_materials_live() {
    let dir = hello_copy();
    let page = "Hello's lamp, as it is now:\n\n![The lamp](Prefabs/Lamp.prefab)\n";
    std::fs::write(dir.path().join("Docs/The Lamp.md"), page).unwrap();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let root = editor.ui.root();
    let dock = editor.ui.find(root, "dock").unwrap();
    editor.ui.show_dock_panel(dock, "docs");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut rows = Vec::new();
    with_class(&editor.ui, root, "cb-row-title", &mut rows);
    let lamp = rows.into_iter().find(|&row| editor.ui.text(row) == "The Lamp").expect("the page's row");
    click(&mut editor.ui, lamp);
    let mut pictures = Vec::new();
    for _ in 0..60 {
        frame(&mut editor, &mut renderer);
        pictures.clear();
        with_class(&editor.ui, root, "md-picture", &mut pictures);
        if !pictures.is_empty() {
            break;
        }
    }
    let &[picture] = pictures.as_slice() else { panic!("the lamp's preview shows") };
    frame(&mut editor, &mut renderer);
    let [_, _, width, height] = editor.ui.rect(picture);
    assert!(width > 100.0 && (width - height).abs() < 1.0, "a square preview, {width} by {height}");
    shot(&mut renderer, "docs-live.png");
}

#[test]
fn the_memory_panel_takes_snapshots_and_compares_them() {
    let Some((mut renderer, mut editor)) = open() else { return };
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.show_dock_panel(dock, "memory");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.ui.find(editor.ui.root(), "memory-panel").unwrap();
    let summary = editor.ui.text(first_with(&editor.ui, panel, "entities-summary").unwrap());
    assert!(summary.contains("GPU") && summary.contains("ECS chunks"), "the parts' totals, live: {summary}");

    let take = editor.memory_panel().take_button();
    click(&mut editor.ui, take);
    frame(&mut editor, &mut renderer);
    let first = &editor.memory_panel().snapshots()[0];
    assert!(first.label.starts_with("Snapshot 1"), "{}", first.label);
    assert!(first.entries.iter().any(|e| e.category == "GPU" && e.name == "hdr"), "the GPU's allocations by name");
    let rows = |editor: &voxear_editor::Editor| {
        let mut rows = Vec::new();
        with_class(&editor.ui, editor.memory_panel().amounts(), "entities-row", &mut rows);
        let shown = rows.into_iter().filter(|&row| editor.ui.is_visible(row));
        shown.map(|row| texts_in(&editor.ui, row).join(" ")).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>()
    };
    assert!(!rows(&editor).is_empty(), "the snapshot's amounts show");

    // The amounts' columns: the line after Part drags, and every row's Part cell follows it.
    let part_width = |editor: &voxear_editor::Editor| {
        let mut rows = Vec::new();
        with_class(&editor.ui, editor.memory_panel().amounts(), "table-row", &mut rows);
        let row = rows.into_iter().find(|&row| editor.ui.is_visible(row)).expect("a row of amounts");
        editor.ui.rect(editor.ui.children(row)[0])[2]
    };
    let before = part_width(&editor);
    let header = first_with(&editor.ui, panel, "table-header").unwrap();
    let line = first_with(&editor.ui, header, "split-divider").unwrap();
    let [x, y, w, h] = editor.ui.rect(line);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_move(x + w / 2.0 + 60.0, y + h / 2.0);
    editor.ui.pointer_button(false);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let after = part_width(&editor);
    assert!((after - before - 60.0).abs() < 2.0, "the Part column went from {before} to {after}");

    // More entities, a second snapshot, and the difference from the first.
    for _ in 0..2000 {
        editor.game.engine.world.spawn((Transform::IDENTITY,));
    }
    click(&mut editor.ui, take);
    frame(&mut editor, &mut renderer);
    let compare = editor.memory_panel().compare();
    click(&mut editor.ui, compare);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let named = |option: NodeId| texts_in(&editor.ui, option).iter().any(|t| t.starts_with("Snapshot 1"));
    let first_option = options.iter().copied().find(|&o| named(o)).expect("the first snapshot to compare with");
    click(&mut editor.ui, first_option);
    frame(&mut editor, &mut renderer);
    let changed = rows(&editor);
    assert!(
        changed.iter().any(|r| r.contains("ECS chunks") && r.contains("Transform") && r.contains('+')),
        "{changed:?}"
    );
    shot(&mut renderer, "memory.png");
}

#[test]
fn the_entities_panel_lists_archetypes_entities_and_their_components() {
    let Some((mut renderer, mut editor)) = open() else { return };
    assert!(editor.use_layout("Profiling"));
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.show_dock_panel(dock, "entities");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.entities_panel();
    let (filter, archetypes, entities, details) =
        (panel.filter(), panel.archetypes(), panel.entities(), panel.details());
    let rows = |editor: &voxear_editor::Editor, list: NodeId| {
        let mut rows = Vec::new();
        with_class(&editor.ui, list, "entities-row-name", &mut rows);
        // Rows past the list's end are kept, hidden, for when it grows.
        let shown = |r: &NodeId| editor.ui.parent(*r).is_some_and(|row| editor.ui.is_visible(row));
        rows.into_iter().filter(shown).map(|r| editor.ui.text(r)).filter(|t| !t.is_empty()).collect::<Vec<_>>()
    };
    assert!(!rows(&editor, archetypes).is_empty(), "Hello's Objs make archetypes");

    // A filter of component names keeps the archetypes with them all.
    type_into(&mut editor.ui, filter, "MeshRenderer, Transform");
    frame(&mut editor, &mut renderer);
    let kept = rows(&editor, archetypes);
    assert!(!kept.is_empty() && kept.iter().all(|r| r.contains("MeshRenderer") && r.contains("Transform")), "{kept:?}");

    // An archetype's entities, and a picked one's components with their values.
    editor.ui.set_selection(archetypes, &[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let named = rows(&editor, entities);
    assert!(!named.is_empty(), "the archetype's entities are listed");
    editor.ui.set_selection(entities, &[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut cards = Vec::new();
    with_class(&editor.ui, details, "entities-component-name", &mut cards);
    let cards: Vec<String> = cards.into_iter().map(|c| editor.ui.text(c)).collect();
    assert!(cards.contains(&"Transform".to_string()) && cards.contains(&"MeshRenderer".to_string()), "{cards:?}");
    let mut values = Vec::new();
    with_class(&editor.ui, details, "entities-value-name", &mut values);
    assert!(values.iter().any(|&v| editor.ui.text(v) == "Position"), "the Transform's values show");
    let selected = editor.selected().expect("an Obj picked there is selected");
    assert_eq!(editor.game.engine.world.get::<Name>(selected).unwrap().0, named[0]);
    let mut inputs = Vec::new();
    with_class(&editor.ui, details, "entities-value-input", &mut inputs);
    assert!(inputs.is_empty(), "an Obj is changed in the Inspector, not here");

    // The picked archetype's chunks, each filled as far as it is.
    let mut bars = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "entities-chunk", &mut bars);
    assert!(!bars.is_empty(), "a bar for each chunk");
    let mut hints = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "entities-hint", &mut hints);
    let summary = hints.iter().map(|&h| editor.ui.text(h)).find(|t| t.contains("% full")).expect("how full they are");
    assert!(summary.contains("chunk"), "{summary}");
    shot(&mut renderer, "entities-panel.png");

    // A plain entity, which no scene saves, is changed here, in the world.
    let plain = editor.game.engine.world.spawn((Transform::from_position(Vec3::new(5.0, 0.0, 0.0)),));
    editor.ui.set_text(filter, "");
    frame(&mut editor, &mut renderer);
    let archetype = editor
        .game
        .engine
        .world
        .archetypes()
        .iter()
        .position(|a| (0..a.chunk_count()).any(|c| a.chunk_entities(c).contains(&plain)));
    assert!(archetype.is_some());
    let shown = rows(&editor, archetypes);
    let plain_row = shown.iter().position(|names| !names.contains("Name") && names.contains("Transform"));
    editor.ui.set_selection(archetypes, &[plain_row.unwrap_or_else(|| panic!("no plain archetype in {shown:?}"))]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let listed = editor.entities_panel().entities();
    let position = rows(&editor, listed).len();
    assert!(position >= 1, "its entities are listed");
    editor.ui.set_selection(listed, &[position - 1]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut inputs = Vec::new();
    with_class(&editor.ui, details, "entities-value-input", &mut inputs);
    assert_eq!(inputs.len(), 3, "its position, rotation and scale are fields");
    assert_eq!(editor.ui.text(inputs[0]), "5.00, 0.00, 0.00");
    type_into(&mut editor.ui, inputs[0], "1, 2, 3");
    frame(&mut editor, &mut renderer);
    let moved = editor.game.engine.world.get::<Transform>(plain).unwrap().position;
    assert_eq!(moved, Vec3::new(1.0, 2.0, 3.0), "what is typed goes into the world");
    type_into(&mut editor.ui, inputs[0], "1, 2");
    frame(&mut editor, &mut renderer);
    let kept = editor.game.engine.world.get::<Transform>(plain).unwrap().position;
    assert_eq!(kept, Vec3::new(1.0, 2.0, 3.0), "half a vector changes nothing");
}

#[test]
fn the_consoles_settings_show_a_sample_and_change_it_at_once() {
    use tracing::Level;
    use voxear_core::logging::LogLine;
    let dir = hello_copy();
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let log = LogCapture::default();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    for (level, message) in [(Level::INFO, "one"), (Level::WARN, "two"), (Level::INFO, "three")] {
        log_line(&log, LogLine { target: "voxear_engine::levels".into(), ..LogLine::new(level, message) });
    }
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let roomy = "row(240, [hierarchy], row(-300, column(45%, [scene, game, docs], [console, project]), [inspector]))";
    editor.ui.set_dock_layout(dock, roomy).unwrap();
    editor.ui.show_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(console_lines(&editor.ui), ["one", "two", "three"]);

    // Settings opens in the Console's place, with a sample drawn as its lines are.
    let mut gears = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-settings-button", &mut gears);
    click(&mut editor.ui, gears[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // Shown when it and everything it is in are.
    let shown = |ui: &Ui, node: NodeId| {
        let mut at = Some(node);
        while let Some(node) = at {
            if !ui.is_visible(node) {
                return false;
            }
            at = ui.parent(node);
        }
        true
    };
    assert!(!shown(&editor.ui, console_list(&editor.ui)), "the list makes way");
    let mut samples = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-sample", &mut samples);
    let sample_rows = |ui: &Ui| {
        let mut rows = Vec::new();
        with_class(ui, samples[0], "list-row", &mut rows);
        rows
    };
    assert_eq!(sample_rows(&editor.ui).len(), 3);
    shot(&mut renderer, "console-settings.png");

    // A choice changes the sample and the setting at once.
    let segment = |ui: &Ui, name: &str| {
        let mut found = Vec::new();
        with_class(ui, ui.root(), "console-segment", &mut found);
        found.into_iter().find(|&b| ui.text(b) == name).unwrap()
    };
    let tall = segment(&editor.ui, "Tall");
    click(&mut editor.ui, tall);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.settings().console_row_height, 24.0);
    assert_eq!(editor.ui.rect(sample_rows(&editor.ui)[0])[3], 24.0, "the sample shows it");
    // Further down the screen, scrolled to.
    let mut screens = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "console-settings-body", &mut screens);
    let dot = segment(&editor.ui, "Dot");
    let down = editor.ui.rect(dot)[1] - editor.ui.rect(screens[0])[1] - 40.0;
    editor.ui.set_scroll(screens[0], [0.0, down]);
    frame(&mut editor, &mut renderer);
    click(&mut editor.ui, dot);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(editor.ui.children(sample_rows(&editor.ui)[1])[2]), "●", "a category as a dot");

    // Done brings the list back, drawn the new way.
    let done = console_button(&editor.ui, "Done").unwrap();
    click(&mut editor.ui, done);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(shown(&editor.ui, console_list(&editor.ui)));
    let rows = console_rows(&editor.ui);
    assert_eq!(editor.ui.rect(rows[0])[3], 24.0, "the list's lines are taller");
    assert_eq!(editor.ui.text(editor.ui.children(rows[0])[2]), "●");
}

/// The middle of an element on screen.
fn centre(ui: &Ui, node: NodeId) -> [f32; 2] {
    let [x, y, w, h] = ui.rect(node);
    [x + w / 2.0, y + h / 2.0]
}

/// The editor on a copy of Hello it may save, two frames in, or `None` without a GPU.
fn open_copy() -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor)> {
    open_copy_with(|_| {})
}

/// The same, with the copy changed by `prepare` before it opens.
fn open_copy_with(
    prepare: impl FnOnce(&std::path::Path),
) -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor)> {
    let mut renderer = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok()?;
    let dir = hello_copy();
    prepare(dir.path());
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    Some((dir, renderer, editor))
}

#[test]
fn prefabs_dragged_from_the_project_place_instances() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    open_folder(&mut editor, &mut renderer, "Prefabs");
    frame(&mut editor, &mut renderer);
    let parent = |editor: &voxear_editor::Editor, e: voxear_ecs::Entity| {
        editor.game.engine.world.get::<voxear_scene::Parent>(e).map(|p| p.0)
    };
    let is_instance =
        |editor: &voxear_editor::Editor, e| editor.game.engine.world.has::<voxear_engine::prefab::PrefabInstance>(e);

    // Onto an Obj's row, held there for a frame: the row is marked, and the lamp goes inside it, as one step.
    let spinner = obj(&editor, "Spinner A").unwrap();
    let from = centre(&editor.ui, tile(&editor.ui, "Lamp.prefab").expect("the Prefabs folder shows"));
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Spinner A").unwrap());
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to[0], to[1]);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to[0], to[1] + 1.0);
    let row = hierarchy_row(&editor.ui, "Spinner A").unwrap();
    assert!(editor.ui.has_class(row, "drop-target"), "the row it would land in is marked");
    editor.update(&mut renderer, SIZE, 1.0, 0.5).unwrap();
    shot(&mut renderer, "place-in-hierarchy.png");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let lamp = obj(&editor, "Lamp (1)").expect("Hello has a Lamp, so the instance is numbered");
    assert!(is_instance(&editor, lamp));
    assert_eq!(parent(&editor, lamp), Some(spinner), "inside the Obj it was let go of on");
    let position = editor.game.engine.world.get::<Transform>(lamp).unwrap().position;
    assert_eq!(position, Vec3::new(0.0, 0.6, 0.0), "at its middle, as the prefab places its root");
    assert_eq!(editor.selected(), Some(lamp));
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Lamp (1)").is_none(), "one undo takes it away");

    // Onto a scene's header, at the top of that scene.
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Day").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let lamp = obj(&editor, "Lamp (1)").unwrap();
    assert_eq!(parent(&editor, lamp), None);
    let day = editor.game.levels.scene_order().into_iter().find(|(_, path, _)| *path == "Scenes/Day.scene");
    assert_eq!(editor.game.levels.scene_of(lamp), day.map(|(guid, ..)| guid));

    // Onto the Scene view, where the pointer meets the ground, on the grid across it.
    let camera = editor.scene_camera().unwrap();
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let [x, y, w, h] = editor.ui.rect(viewport);
    let point = [x + w * 0.2, y + h * 0.85];
    let world = &editor.game.engine.world;
    let ground = obj(&editor, "Ground").unwrap();
    let markers = voxear_editor::scene_tools::Markers { lights: true, cameras: true, sounds: true, colliders: true };
    let picked = voxear_editor::scene_tools::pick(world, &renderer, &camera, point, markers);
    assert_eq!(picked, Some(ground), "only ground there");
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * (-origin.y / direction.y);
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(point[0], point[1]);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(point[0], point[1]);
    assert!(editor.ui.has_class(viewport, "drop-hover"), "the Scene view lights up for a prefab");
    editor.update(&mut renderer, SIZE, 1.0, 0.5).unwrap();
    shot(&mut renderer, "place-in-scene.png");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let lamp = obj(&editor, "Lamp (2)").expect("placed on the ground");
    let position = editor.game.engine.world.get::<Transform>(lamp).unwrap().position;
    let snap = editor.settings().snap;
    assert!((position.y - 0.6).abs() < 1e-4, "on the ground, as the prefab stands its root: {position}");
    for (placed, aimed) in [(position.x, hit.x), (position.z, hit.z)] {
        assert!((placed - aimed).abs() <= snap / 2.0 + 1e-4, "where the pointer was: {position} for {hit}");
        assert!((placed / snap - (placed / snap).round()).abs() < 1e-4, "on the grid: {position}");
    }

    editor.save();
    let saved = voxear_engine::scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    let placed: Vec<&str> = saved
        .objs
        .iter()
        .filter(|o| o.prefab.as_deref() == Some("Prefabs/Lamp.prefab"))
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(placed, ["Lamp (1)", "Lamp (2)"], "both are saved as instances");

    // In a prefab open on its own, what lands at the top goes inside its root, and the prefab never inside itself.
    editor.open_prefab("Prefabs/Lamp.prefab", &mut renderer);
    frame(&mut editor, &mut renderer);
    let root = obj(&editor, "Lamp").unwrap();
    let from = centre(&editor.ui, tile(&editor.ui, "Firefly.prefab").unwrap());
    drag_between(&mut editor, &mut renderer, from, point);
    let firefly = editor.selected().expect("the firefly placed is selected");
    assert_eq!(parent(&editor, firefly), Some(root), "a prefab keeps one root");
    let objs = editor.game.engine.world.query_ref::<&Name>().into_iter().count();
    let from = centre(&editor.ui, tile(&editor.ui, "AmberLamp.prefab").unwrap());
    drag_between(&mut editor, &mut renderer, from, point);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.query_ref::<&Name>().into_iter().count(), objs, "the amber lamp holds a lamp");
    assert!(
        label_starting(&editor.ui, editor.ui.root(), "A prefab cannot hold").is_some(),
        "and the status bar says so"
    );
}

#[test]
fn a_model_dragged_from_the_project_places_objs_drawing_it() {
    // Beside Hello's pyramid, a model of two pyramids side by side.
    let pair = |root: &std::path::Path| {
        let text = std::fs::read_to_string(root.join("Models/Pyramid.gltf")).unwrap();
        let mut model: serde_json::Value = serde_json::from_str(&text).unwrap();
        model["nodes"] = serde_json::json!([
            { "name": "Left", "mesh": 0, "translation": [-1.0, 0.0, 0.0] },
            { "name": "Right", "mesh": 0, "translation": [1.0, 0.0, 0.0] },
        ]);
        model["scenes"] = serde_json::json!([{ "nodes": [0, 1] }]);
        std::fs::write(root.join("Models/Pair.gltf"), model.to_string()).unwrap();
    };
    let Some((dir, mut renderer, mut editor)) = open_copy_with(pair) else { return };
    open_folder(&mut editor, &mut renderer, "Models");
    frame(&mut editor, &mut renderer);
    let from = centre(&editor.ui, tile(&editor.ui, "Pyramid.gltf").expect("the Models folder shows"));
    let camera = editor.scene_camera().unwrap();
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let [x, y, w, h] = editor.ui.rect(viewport);
    let point = [x + w * 0.2, y + h * 0.85];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * (-origin.y / direction.y);
    drag_between(&mut editor, &mut renderer, from, point);

    // One part, so one Obj, named for the model and numbered as Hello has a Pyramid, standing where it was let go.
    let pyramid = obj(&editor, "Pyramid (1)").expect("an Obj for the model");
    assert_eq!(editor.selected(), Some(pyramid));
    let world = &editor.game.engine.world;
    assert!(
        world.get::<voxear_scene::Parent>(pyramid).is_none()
            && world.get::<voxear_render::MeshRenderer>(pyramid).is_some()
    );
    let position = world.get::<Transform>(pyramid).unwrap().position;
    assert!(
        position.y.abs() < 1e-4 && (position.x - hit.x).abs() <= 0.13 && (position.z - hit.z).abs() <= 0.13,
        "{position}"
    );
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Pyramid (1)").is_none(), "one step to undo");
    editor.redo(&mut renderer);

    // Onto an Obj's row, inside it, and saved naming the model's mesh and material.
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Spinner A").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let inside = obj(&editor, "Pyramid (2)").unwrap();
    let spinner = obj(&editor, "Spinner A");
    assert_eq!(editor.game.engine.world.get::<voxear_scene::Parent>(inside).map(|p| p.0), spinner);
    editor.save();
    let text = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    let guid = std::fs::read_to_string(dir.path().join("Models/Pyramid.gltf.meta")).unwrap();
    let guid = guid.split('"').nth(1).unwrap();
    assert_eq!(text.matches(&format!("mesh = \"{guid}#mesh0.0\"")).count(), 3, "Hello's and the two placed:\n{text}");
    assert_eq!(text.matches(&format!("material = \"{guid}#material0\"")).count(), 3, "{text}");

    // A model of two parts: one Obj named for it, drawing every part where the model puts it, not an Obj each.
    let from = centre(&editor.ui, tile(&editor.ui, "Pair.gltf").unwrap());
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Day").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let pair = obj(&editor, "Pair").expect("an Obj for the model");
    // Where the model puts its parts, read as the engine reads glTF, whose X it turns around.
    let guid = editor.game.content.db().guid_of("Models/Pair.gltf").unwrap();
    let parts: Vec<(String, Vec3)> =
        editor.game.content.model(&mut renderer, guid).unwrap().iter().map(|n| (n.name.clone(), n.position)).collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].1.x, -parts[1].1.x, "side by side");
    let world = &editor.game.engine.world;
    let drawn = world.get::<voxear_render::ModelRenderer>(pair).expect("a Model Renderer");
    assert_eq!(drawn.parts.len(), 2, "both parts");
    assert!(world.get::<voxear_render::MeshRenderer>(pair).is_none());
    assert!(parts.iter().all(|(name, _)| obj(&editor, name).is_none()), "and no Obj for either");
    assert!(prop_row(&editor.ui, "Parts").is_some(), "the Inspector shows the Model Renderer");
    editor.save();
    let text = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    assert!(text.contains(&format!("[obj.model]\nmodel = \"{guid}\"")), "saved naming the model:\n{text}");

    // In the Hierarchy, its row has a button of its own, apart from its arrow, that lists its parts under it,
    // greyed; a part clicked selects the Obj, as the parts are the model's.
    let row = hierarchy_row(&editor.ui, "Pair").unwrap();
    let button = first_with(&editor.ui, row, "row-parts").expect("a parts button");
    assert!(editor.ui.is_visible(button), "on a model's Obj");
    let spinner_row = hierarchy_row(&editor.ui, "Spinner A").unwrap();
    let spinner_button = first_with(&editor.ui, spinner_row, "row-parts");
    assert!(!spinner_button.is_some_and(|b| editor.ui.is_visible(b)), "not on a cube");
    assert!(hierarchy_row(&editor.ui, "Left").is_none(), "its parts are not listed until asked for");
    click(&mut editor.ui, button);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let left = hierarchy_row(&editor.ui, "Left").expect("its parts listed under it");
    assert!(hierarchy_row(&editor.ui, "Right").is_some());
    assert!(first_with(&editor.ui, left, "model-part").is_some(), "greyed, as the model's");
    editor.select(None);
    click(&mut editor.ui, left);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), Some(pair), "a part selects its model's Obj");

    // The Inspector shows the part: where it sits, typed to move it, kept on the Obj and one step to undo.
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let names: Vec<String> = {
        let mut found = Vec::new();
        with_class(&editor.ui, inspector, "asset-name", &mut found);
        found.into_iter().map(|n| editor.ui.text(n)).collect()
    };
    assert_eq!(names, ["Left"], "the part's own Inspector");
    assert_eq!(editor.inspected_part(), Some((pair, 0)));
    let x = first_with(&editor.ui, prop_row(&editor.ui, "Position").unwrap(), "axis-input").unwrap();
    type_into(&mut editor.ui, x, "-3");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    let moved = |editor: &voxear_editor::Editor| {
        let drawn = editor.game.engine.world.get::<voxear_render::ModelRenderer>(pair).unwrap();
        drawn.parts.iter().find(|part| part.node == 0).map(|part| part.transform.w_axis.x)
    };
    assert_eq!(moved(&editor), Some(-3.0), "the part moves");
    let source = editor.game.engine.world.get::<voxear_engine::scene_file::ModelSource>(pair).unwrap();
    assert_eq!(source.0.part(0).and_then(|part| part.position).map(|p| p[0]), Some(-3.0), "kept on the Obj");
    editor.undo(&mut renderer);
    assert_eq!(moved(&editor), Some(parts[0].1.x), "one step to undo");
    editor.redo(&mut renderer);
    assert_eq!(moved(&editor), Some(-3.0));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_part(), Some((pair, 0)), "still showing the part through undo and redo");

    // Add Component takes the part out into an Obj of its own inside the model's, where it was; the rest stay one.
    let mut buttons = Vec::new();
    with_class(&editor.ui, inspector, "add-component", &mut buttons);
    let add = buttons.into_iter().find(|&b| editor.ui.text(b) == "Add Component").expect("Add Component on a part");
    scroll_to(&mut editor, &mut renderer, add);
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let taken = obj(&editor, "Left").expect("an Obj for the part");
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<voxear_scene::Parent>(taken).map(|p| p.0), Some(pair));
    assert_eq!(world.get::<Transform>(taken).unwrap().position.x, -3.0, "where it was moved to");
    assert!(world.get::<voxear_render::MeshRenderer>(taken).is_some());
    assert_eq!(world.get::<voxear_render::ModelRenderer>(pair).unwrap().parts.len(), 1, "the other stays drawn whole");
    assert_eq!(editor.selected(), Some(taken), "selected, ready for components");
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Left").is_none(), "one step to undo");
    assert_eq!(editor.game.engine.world.get::<voxear_render::ModelRenderer>(pair).unwrap().parts.len(), 2);
    assert!(hierarchy_row(&editor.ui, "Left").is_some(), "listed again as a part");

    // The Model Renderer's menu in the Inspector unpacks it, and so does a part's in the Hierarchy: an Obj inside
    // it for each part, where the model puts it, as one step.
    editor.select(Some(pair));
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_part(), None, "the Obj selected shows the Obj");
    let header = section_header(&editor.ui, "Model Renderer").unwrap();
    context_click(&mut editor.ui, header);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Unpack to Objs").is_some(), "the Model Renderer's menu unpacks it");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    let left = hierarchy_row(&editor.ui, "Left").unwrap();
    context_click(&mut editor.ui, left);
    frame(&mut editor, &mut renderer);
    let unpack = menu_item(&editor.ui, "Unpack to Objs").expect("a part's menu in the Hierarchy unpacks it");
    click(&mut editor.ui, unpack);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert!(world.get::<voxear_render::ModelRenderer>(pair).is_none(), "which draws nothing itself now");
    for (name, position) in &parts {
        let part = obj(&editor, name).unwrap();
        assert_eq!(world.get::<voxear_scene::Parent>(part).map(|p| p.0), Some(pair));
        // Left was moved on the Obj, and stays where it was moved to.
        let wanted = if name == "Left" { Vec3::new(-3.0, position.y, position.z) } else { *position };
        assert_eq!(world.get::<Transform>(part).unwrap().position, wanted, "{name}");
        assert!(world.get::<voxear_render::MeshRenderer>(part).is_some());
    }
    editor.undo(&mut renderer);
    let world = &editor.game.engine.world;
    assert!(world.get::<voxear_render::ModelRenderer>(pair).is_some(), "undo draws it whole again");
    assert!(parts.iter().all(|(name, _)| obj(&editor, name).is_none()), "without its parts' Objs");
}

#[test]
fn what_is_dragged_over_the_scene_view_is_there_following_the_pointer() {
    let Some((_dir, mut renderer, mut editor)) = open_copy() else { return };
    open_folder(&mut editor, &mut renderer, "Models");
    frame(&mut editor, &mut renderer);
    let from = centre(&editor.ui, tile(&editor.ui, "Pyramid.gltf").expect("the Models folder shows"));
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let [x, y, w, h] = editor.ui.rect(viewport);
    let position = |editor: &voxear_editor::Editor| {
        obj(editor, "Pyramid (1)").map(|e| editor.game.engine.world.get::<Transform>(e).unwrap().position)
    };

    // Over the Scene view, as in Unity, it is there already, and moves with the pointer.
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(x + w * 0.3, y + h * 0.8);
    frame(&mut editor, &mut renderer);
    let first = position(&editor).expect("placed while dragged over the view");
    editor.ui.pointer_move(x + w * 0.7, y + h * 0.8);
    frame(&mut editor, &mut renderer);
    let moved = position(&editor).unwrap();
    assert!(moved.x > first.x + 0.5, "followed the pointer: {first} to {moved}");
    assert!(moved.y.abs() < 1e-4, "on the ground, not on itself: {moved}");

    // Off the view it goes, and back over the view and let go of, it stays, as one step to undo.
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let [ix, iy, iw, ih] = editor.ui.rect(inspector);
    editor.ui.pointer_move(ix + iw / 2.0, iy + ih / 2.0);
    frame(&mut editor, &mut renderer);
    assert!(position(&editor).is_none(), "taken off the view, it goes");
    editor.ui.pointer_move(x + w * 0.5, y + h * 0.8);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let kept = obj(&editor, "Pyramid (1)").expect("kept where let go of");
    assert_eq!(editor.selected(), Some(kept));
    assert!(obj(&editor, "Pyramid (2)").is_none(), "one, not another beside it");
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Pyramid (1)").is_none(), "one step to undo");
}

#[test]
fn a_sound_dragged_from_the_project_places_an_obj_playing_it() {
    let write = |root: &std::path::Path| std::fs::write(root.join("Models/Chime.wav"), chime()).unwrap();
    let Some((dir, mut renderer, mut editor)) = open_copy_with(write) else { return };
    // Silent, rather than through the machine's speakers.
    editor.game.audio = voxear_engine::audio::GameAudio::new(voxear_engine::audio::Audio::quiet());
    open_folder(&mut editor, &mut renderer, "Models");
    frame(&mut editor, &mut renderer);
    let from = centre(&editor.ui, tile(&editor.ui, "Chime.wav").expect("the Models folder shows"));
    let camera = editor.scene_camera().unwrap();
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let [x, y, w, h] = editor.ui.rect(viewport);
    let point = [x + w * 0.2, y + h * 0.85];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * (-origin.y / direction.y);
    drag_between(&mut editor, &mut renderer, from, point);

    // An Obj named for the sound, with an Audio Source playing it, standing where it was let go.
    let chime = obj(&editor, "Chime").expect("an Obj for the sound");
    assert_eq!(editor.selected(), Some(chime));
    let world = &editor.game.engine.world;
    let source = world.get::<voxear_engine::audio::AudioSource>(chime).copied().expect("with an Audio Source");
    let guid = editor.game.content.db().guid_of("Models/Chime.wav").unwrap();
    assert_eq!(source.sound.and_then(|id| editor.game.content.sound_guid(id)), Some(guid));
    assert!(source.play_on_start, "it plays as the game starts");
    let position = world.get::<Transform>(chime).unwrap().position;
    assert!(
        position.y.abs() < 1e-4 && (position.x - hit.x).abs() <= 0.13 && (position.z - hit.z).abs() <= 0.13,
        "{position}"
    );
    editor.undo(&mut renderer);
    assert!(obj(&editor, "Chime").is_none(), "one step to undo");
    editor.redo(&mut renderer);
    assert!(obj(&editor, "Chime").is_some(), "and to redo");

    // Onto an Obj's row, inside it, numbered as a copy is, and saved naming the sound.
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Spinner A").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let inside = obj(&editor, "Chime (1)").expect("a second Obj for the sound");
    let spinner = obj(&editor, "Spinner A");
    assert_eq!(editor.game.engine.world.get::<voxear_scene::Parent>(inside).map(|p| p.0), spinner);
    editor.save();
    let text = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    assert_eq!(text.matches(&format!("sound = \"{guid}\"")).count(), 2, "both are saved:\n{text}");
}

/// A rug a metre square, as 3ds Max writes FBX: Z up, in centimetres.
const RUG: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7400
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",1
	}
}
Objects:  {
	Geometry: 1, "Geometry::Rug", "Mesh" {
		Vertices: *12 {
			a: -50,-50,0,50,-50,0,50,50,0,-50,50,0
		}
		PolygonVertexIndex: *4 {
			a: 0,1,2,-4
		}
	}
	Model: 2, "Model::Rug", "Mesh" {
	}
	Material: 3, "Material::Wool", "" {
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",0.6,0.1,0.1
		}
	}
}
Connections:  {
	C: "OO",2,0
	C: "OO",1,2
	C: "OO",3,2
}
"#;

#[test]
fn an_fbx_model_shows_and_places_as_a_gltf_one_does() {
    let rug = |root: &std::path::Path| std::fs::write(root.join("Models/Rug.fbx"), RUG).unwrap();
    let Some((dir, mut renderer, mut editor)) = open_copy_with(rug) else { return };
    open_folder(&mut editor, &mut renderer, "Models");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    assert!(tile_preview(&editor.ui, "Rug.fbx").is_some(), "the Project draws a preview of it");

    let from = centre(&editor.ui, tile(&editor.ui, "Rug.fbx").unwrap());
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Day").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let rug = obj(&editor, "Rug").expect("an Obj for the model");
    let world = &editor.game.engine.world;
    let transform = world.get::<Transform>(rug).unwrap();
    assert_eq!(transform.rotation, Default::default(), "standing up, with no turn left on it from Z up");
    assert!(world.get::<voxear_render::MeshRenderer>(rug).is_some());
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "fbx-rug.png");
    editor.save();
    let text = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    let guid = std::fs::read_to_string(dir.path().join("Models/Rug.fbx.meta")).unwrap();
    let guid = guid.split('"').nth(1).unwrap();
    assert!(text.contains(&format!("mesh = \"{guid}#mesh0.0\"")), "{text}");
    assert!(text.contains(&format!("material = \"{guid}#material0\"")), "{text}");
}

#[test]
fn scenes_open_alongside_the_level_and_join_it_from_the_hierarchy() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    let level = || std::fs::read_to_string(dir.path().join("Levels/Day.level")).unwrap();
    let open =
        |editor: &voxear_editor::Editor| editor.game.levels.loaded_scenes().map(str::to_string).collect::<Vec<_>>();
    let badge =
        |ui: &Ui| hierarchy_row(ui, "Night").and_then(|row| first_with(ui, row, "row-badge")).map(|b| ui.text(b));
    open_folder(&mut editor, &mut renderer, "Scenes");
    frame(&mut editor, &mut renderer);
    // Dragged below the Hierarchy's rows, Night opens alongside Environment and Day, marked as not in the level.
    let from = centre(&editor.ui, tile(&editor.ui, "Night.scene").expect("the Scenes folder shows"));
    let tree = editor.ui.find(editor.ui.root(), "hierarchy-tree").unwrap();
    let [x, y, w, h] = editor.ui.rect(tree);
    drag_between(&mut editor, &mut renderer, from, [x + w / 2.0, y + h - 20.0]);
    frame(&mut editor, &mut renderer);
    assert_eq!(open(&editor), ["Scenes/Environment.scene", "Scenes/Day.scene", "Scenes/Night.scene"]);
    assert!(hierarchy_names(&editor.ui).contains(&"Moon".to_string()), "its Objs show");
    assert_eq!(badge(&editor.ui).as_deref(), Some("Not in level"));
    assert_eq!(editor.game.levels.active_scene(), Some("Scenes/Day.scene"), "new Objs still go into Day");
    let header = hierarchy_row(&editor.ui, "Night").unwrap();
    context_click(&mut editor.ui, header);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "scene-alongside.png");
    editor.ui.key_down(voxear_ui::Key::Escape);
    frame(&mut editor, &mut renderer);

    // Closed from its header's menu, then opened again from its own menu in the Project.
    let header = hierarchy_row(&editor.ui, "Night").unwrap();
    context_choose(&mut editor, &mut renderer, header, "Close Scene");
    assert!(!open(&editor).contains(&"Scenes/Night.scene".to_string()));
    assert!(!hierarchy_names(&editor.ui).contains(&"Moon".to_string()));
    let scene_tile = tile(&editor.ui, "Night.scene").unwrap();
    context_choose(&mut editor, &mut renderer, scene_tile, "Open Additively");
    assert!(open(&editor).contains(&"Scenes/Night.scene".to_string()));

    // Its Objs save to its own file, and the level is left as it was.
    let moon = obj(&editor, "Moon").unwrap();
    editor.game.engine.world.get_mut::<Name>(moon).unwrap().0 = "Full Moon".into();
    editor.save();
    let night = std::fs::read_to_string(dir.path().join("Scenes/Night.scene")).unwrap();
    assert!(night.contains("name = \"Full Moon\""), "{night}");
    assert!(!level().contains("Night"));

    // Its header adds it to the level, which keeps every scene open.
    frame(&mut editor, &mut renderer);
    let header = hierarchy_row(&editor.ui, "Night").unwrap();
    context_choose(&mut editor, &mut renderer, header, "Add to the Day Level");
    assert!(level().contains("  # Scenes/Night.scene\n"), "{}", level());
    assert_eq!(open(&editor), ["Scenes/Environment.scene", "Scenes/Day.scene", "Scenes/Night.scene"]);
    frame(&mut editor, &mut renderer);
    assert_eq!(badge(&editor.ui).as_deref(), Some(""), "one of the level's own now");

    // Dragged onto the Scene view, a scene opens the same way.
    let header = hierarchy_row(&editor.ui, "Night").unwrap();
    context_choose(&mut editor, &mut renderer, header, "Close Scene");
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let (from, to) = (centre(&editor.ui, tile(&editor.ui, "Night.scene").unwrap()), centre(&editor.ui, viewport));
    drag_between(&mut editor, &mut renderer, from, to);
    assert!(open(&editor).contains(&"Scenes/Night.scene".to_string()));
}

#[test]
fn a_levels_scenes_are_added_and_taken_out_in_the_inspector() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    let level = || std::fs::read_to_string(dir.path().join("Levels/Day.level")).unwrap();
    // A scene's row: its field, which says the scene's name, then its boxes and remove button.
    let scene_row = |ui: &Ui, scene: &str| {
        let mut rows = Vec::new();
        with_class(ui, ui.root(), "level-scene", &mut rows);
        let named = |row: &NodeId| first_with(ui, *row, "picker-label").is_some_and(|label| ui.text(label) == scene);
        rows.into_iter().find(named)
    };
    let parts = |ui: &Ui, scene: &str| {
        let row = scene_row(ui, scene).unwrap_or_else(|| panic!("a row for {scene}"));
        let mut flags = Vec::new();
        with_class(ui, row, "level-flag", &mut flags);
        let item = ui.parent(row).unwrap();
        (flags[0], flags[1], first_with(ui, item, "collection-remove").unwrap())
    };
    let field = |ui: &Ui, scene: &str| first_with(ui, scene_row(ui, scene).unwrap(), "value-picker").unwrap();
    open_folder(&mut editor, &mut renderer, "Levels");
    frame(&mut editor, &mut renderer);
    let day = tile(&editor.ui, "Day.level").unwrap();
    click(&mut editor.ui, day);
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Stands on").is_some());
    let (active, required, _) = parts(&editor.ui, "Day");
    assert!(editor.ui.is_checked(active) && editor.ui.is_checked(required), "its only scene is active and required");

    // Add Scene opens the object picker on the scenes not in it; the open level takes the one picked at once.
    let add_scene = |ui: &Ui| {
        let mut buttons = Vec::new();
        with_class(ui, ui.root(), "collection-add", &mut buttons);
        buttons.into_iter().find(|&b| ui.text(b) == "Add Scene").expect("an Add Scene button")
    };
    let add = add_scene(&editor.ui);
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.picker_open(), "Add Scene opens the object picker");
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert!(offered.contains(&"Night.scene".to_string()), "{offered:?}");
    assert!(!offered.contains(&"Day.scene".to_string()), "not one it has: {offered:?}");
    let night = picker_row(&editor.ui, "Night.scene").unwrap();
    click(&mut editor.ui, night);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("  # Scenes/Night.scene\n"), "written with its path beside it:\n{}", level());
    assert!(editor.game.levels.loaded_scenes().any(|s| s == "Scenes/Night.scene"), "the open level loads it");
    assert!(hierarchy_names(&editor.ui).contains(&"Night".to_string()), "and the Hierarchy shows it");
    assert!(level().starts_with("# Voxear level\n"), "the file's comments are kept");
    shot(&mut renderer, "level-scenes.png");

    // A click on a scene's field shows it in the Project.
    let day_field = field(&editor.ui, "Day");
    click(&mut editor.ui, day_field);
    frame(&mut editor, &mut renderer);
    let day_tile = tile(&editor.ui, "Day.scene").expect("the Project opens the Scenes folder");
    assert!(editor.ui.has_class(day_tile, "pinged"), "and lights the scene up");

    // Night made the active scene, then not required.
    let (active, ..) = parts(&editor.ui, "Night");
    click(&mut editor.ui, active);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("active = true"), "{}", level());
    assert_eq!(editor.game.levels.active_scene(), Some("Scenes/Night.scene"));
    let (day_active, ..) = parts(&editor.ui, "Day");
    assert!(!editor.ui.is_checked(day_active), "only one scene is active");
    let (_, required, _) = parts(&editor.ui, "Night");
    click(&mut editor.ui, required);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("required = false"), "{}", level());
    // Reload: the scene loads again whenever the level comes in.
    let row = scene_row(&editor.ui, "Night").unwrap();
    let mut flags = Vec::new();
    with_class(&editor.ui, row, "level-flag", &mut flags);
    assert_eq!(flags.len(), 3, "Active, Required and Reload");
    click(&mut editor.ui, flags[2]);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("reload = true"), "{}", level());

    // Night dragged by its grip above Day goes first in the file, and the list eases into its new order.
    let item_of = |ui: &Ui, scene: &str| ui.parent(scene_row(ui, scene).unwrap()).unwrap();
    let night_grip = first_with(&editor.ui, item_of(&editor.ui, "Night"), "collection-grip").unwrap();
    let from = centre(&editor.ui, night_grip);
    let [_, top, _, _] = editor.ui.rect(item_of(&editor.ui, "Day"));
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=4 {
        editor.ui.pointer_move(from[0], from[1] + (top - from[1]) * step as f32 / 4.0);
        frame(&mut editor, &mut renderer);
    }
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let (day_at, night_at) = (level().find("Scenes/Day.scene").unwrap(), level().find("Scenes/Night.scene").unwrap());
    assert!(night_at < day_at, "Night first now:\n{}", level());
    let [_, night_top, _, _] = editor.ui.rect(item_of(&editor.ui, "Night"));
    assert!(night_top < editor.ui.rect(item_of(&editor.ui, "Day"))[1], "and first in the list");
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    let items = editor.ui.parent(item_of(&editor.ui, "Night")).unwrap();
    assert!(editor.ui.slides_of(items).iter().all(|&slide| slide == 0.0), "settled");

    // Taken out, it unloads, and Day is active again.
    let (.., remove) = parts(&editor.ui, "Night");
    click(&mut editor.ui, remove);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!level().contains("Night"), "{}", level());
    assert!(!editor.game.levels.loaded_scenes().any(|s| s == "Scenes/Night.scene"));
    assert!(!hierarchy_names(&editor.ui).contains(&"Night".to_string()));
    assert_eq!(editor.game.levels.active_scene(), Some("Scenes/Day.scene"));
    assert!(scene_row(&editor.ui, "Night").is_none(), "nor does the Inspector list it");

    // A scene dragged from the Project onto Add Scene goes in; onto a scene's field, it takes that one's place.
    let night = tile(&editor.ui, "Night.scene").expect("the Scenes folder shows");
    let add = add_scene(&editor.ui);
    drag(&mut editor.ui, night, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("  # Scenes/Night.scene\n"), "dropped on Add Scene:\n{}", level());
    let environment = tile(&editor.ui, "Environment.scene").unwrap();
    let night_field = field(&editor.ui, "Night");
    drag(&mut editor.ui, environment, night_field);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("  # Scenes/Environment.scene\n") && !level().contains("Night"), "{}", level());
    assert!(scene_row(&editor.ui, "Environment").is_some());
    let day = tile(&editor.ui, "Day.scene").unwrap();
    let environment_field = field(&editor.ui, "Environment");
    drag(&mut editor.ui, day, environment_field);
    frame(&mut editor, &mut renderer);
    assert!(level().contains("Environment"), "a scene the level has already is turned away:\n{}", level());
}

#[test]
fn the_persistent_box_marks_an_obj_to_stay_through_level_changes() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    let persistent = |ui: &Ui| first_with(ui, ui.root(), "persistent-check").expect("Persistent, under the name");
    let marked = |editor: &voxear_editor::Editor, e| editor.game.engine.world.has::<voxear_scene::ScenePersistence>(e);
    let manager = obj(&editor, "Game Manager").unwrap();
    editor.select(Some(manager));
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_checked(persistent(&editor.ui)), "Hello's Game Manager is persistent");

    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.is_checked(persistent(&editor.ui)));
    let tick = persistent(&editor.ui);
    click(&mut editor.ui, tick);
    frame(&mut editor, &mut renderer);
    assert!(marked(&editor, spinner) && editor.has_unsaved_changes());
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "persistent.png");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!marked(&editor, spinner), "undo takes it back");
    assert!(!editor.ui.is_checked(persistent(&editor.ui)), "and the box shows it");
    editor.redo(&mut renderer);
    editor.save();
    let saved = voxear_engine::scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    let written = saved.objs.iter().find(|o| o.name == "Spinner A").unwrap();
    assert!(written.persistent, "saved as persistent = true");
}

/// The row in the Project's folder tree for the folder called `name`.
fn folder_row(ui: &Ui, name: &str) -> Option<NodeId> {
    let tree = ui.find(ui.root(), "project-folders")?;
    let mut rows = Vec::new();
    with_class(ui, tree, "folder-row", &mut rows);
    rows.into_iter().find(|&row| ui.is_visible(row) && ui.children(row).get(1).is_some_and(|&n| ui.text(n) == name))
}

#[test]
fn an_obj_dragged_into_the_project_becomes_an_instance_of_its_new_prefab() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    let instance = |editor: &voxear_editor::Editor, name: &str| {
        obj(editor, name).is_some_and(|e| editor.game.engine.world.has::<voxear_engine::prefab::PrefabInstance>(e))
    };

    // Chrome Ball, held over the Prefabs folder's tile, lights it up, and its prefab is made in that folder.
    let row = hierarchy_row(&editor.ui, "Chrome Ball").unwrap();
    let folder = tile(&editor.ui, "Prefabs").expect("the top folder shows its folders");
    let (from, to) = (centre(&editor.ui, row), centre(&editor.ui, folder));
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to[0], to[1]);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(to[0], to[1] + 1.0);
    assert!(editor.ui.has_class(folder, "drop-hover"), "the folder it would go in lights up");
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Prefabs/Chrome Ball.prefab").is_file(), "made in the folder let go of on");
    assert!(instance(&editor, "Chrome Ball"), "the Obj is an instance of it now");

    // And it shows: the Hierarchy's prefab cube, tinted, and the Inspector's Prefab row.
    let row = hierarchy_row(&editor.ui, "Chrome Ball").unwrap();
    let icon = first_with(&editor.ui, row, "row-icon").unwrap();
    assert!(editor.ui.has_class(icon, "prefab-root"));
    let prefab = prop_row(&editor.ui, "Prefab").expect("a Prefab row for an instance");
    let file = first_with(&editor.ui, prefab, "prefab-file").unwrap();
    assert_eq!(editor.ui.text(file), "Chrome Ball.prefab");
    shot(&mut renderer, "prefab-instance.png");

    // Pyramid, onto the Models folder in the tree; the Obj inside it is part of the instance.
    let (from, to) = (
        centre(&editor.ui, hierarchy_row(&editor.ui, "Pyramid").unwrap()),
        centre(&editor.ui, folder_row(&editor.ui, "Models").unwrap()),
    );
    drag_between(&mut editor, &mut renderer, from, to);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Models/Pyramid.prefab").is_file());
    // Hello's Pyramid holds a model of the same name: the instance is the one selected.
    let pyramid = editor.selected().unwrap();
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<Name>(pyramid).map(|n| n.0.as_str()), Some("Pyramid"));
    assert!(world.has::<voxear_engine::prefab::PrefabInstance>(pyramid));
    let inside =
        world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Parent)>().into_iter().find(|(_, p)| p.0 == pyramid);
    editor.select(inside.map(|(e, _)| e));
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "In prefab").is_some(), "an Obj inside an instance names its prefab");

    // The row's Select shows the prefab in the Project and the Inspector; its Open opens it on its own.
    editor.save();
    let ball = obj(&editor, "Chrome Ball");
    editor.select(ball);
    frame(&mut editor, &mut renderer);
    let select = prefab_button(&editor.ui, "Select");
    click(&mut editor.ui, select);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Prefabs/Chrome Ball.prefab"));
    editor.select(ball);
    frame(&mut editor, &mut renderer);
    let open = prefab_button(&editor.ui, "Open");
    click(&mut editor.ui, open);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.open_prefab_path(), Some("Prefabs/Chrome Ball.prefab"));
}

/// The Inspector's Prefab row's button labelled `label`.
fn prefab_button(ui: &Ui, label: &str) -> NodeId {
    let mut buttons = Vec::new();
    with_class(ui, ui.root(), "prefab-button", &mut buttons);
    buttons.into_iter().find(|&b| ui.text(b) == label).unwrap_or_else(|| panic!("no {label} button"))
}

#[test]
fn inspectors_open_more_than_once_and_lock_to_what_they_show() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let (a, b) = (obj(&editor, "Spinner A").unwrap(), obj(&editor, "Spinner B").unwrap());
    editor.select(Some(a));
    frame(&mut editor, &mut renderer);

    // Add Inspector, from the Inspector's own window menu: a second, beside it, showing the selection too.
    let tab = dock_tab(&editor.ui, "Inspector").unwrap();
    context_click(&mut editor.ui, tab);
    frame(&mut editor, &mut renderer);
    let add = menu_item(&editor.ui, "Add Inspector").expect("in the window's menu");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspector_count(), 2);
    assert_eq!(editor.inspectors_shown(), [Some(a), Some(a)]);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    assert!(editor.ui.dock_panel_neighbours(dock, "inspector-2").contains(&"inspector".to_string()));

    // The second's padlock keeps it on Spinner A as the selection moves on, and edits there change Spinner A.
    let visible = |editor: &voxear_editor::Editor, class: &str| {
        let mut found = Vec::new();
        with_class(&editor.ui, editor.ui.root(), class, &mut found);
        found
            .into_iter()
            .find(|&node| std::iter::successors(Some(node), |&n| editor.ui.parent(n)).all(|n| editor.ui.is_visible(n)))
    };
    let lock = visible(&editor, "inspector-lock").expect("the second's padlock, showing");
    assert_ne!(lock, editor.inspector_lock());
    click(&mut editor.ui, lock);
    frame(&mut editor, &mut renderer);
    editor.select(Some(b));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspectors_shown(), [Some(b), Some(a)], "the first follows, the locked one stays");
    assert_eq!(editor.selected(), Some(b));
    let name = visible(&editor, "obj-name").expect("the second's name field");
    editor.ui.focus(name);
    editor.ui.set_modifiers(voxear_ui::Modifiers { shift: false, control: true });
    editor.ui.key_down(voxear_ui::Key::Char('a'));
    editor.ui.set_modifiers(voxear_ui::Modifiers::default());
    editor.ui.text_input("Spinner A, locked");
    frame(&mut editor, &mut renderer);
    let named = |e: voxear_ecs::Entity| editor.game.engine.world.get::<Name>(e).map(|n| n.0.clone());
    assert_eq!(named(a).as_deref(), Some("Spinner A, locked"), "the locked Inspector edits what it shows");
    assert_eq!(named(b).as_deref(), Some("Spinner B"));

    // Unlocked, it follows the selection again; closed from its menu, it goes for good.
    click(&mut editor.ui, lock);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspectors_shown(), [Some(b), Some(b)]);
    let mut tabs = Vec::new();
    with_class(&editor.ui, dock, "tab", &mut tabs);
    let second = tabs.into_iter().filter(|&t| editor.ui.text(t) == "Inspector").last().unwrap();
    context_click(&mut editor.ui, second);
    frame(&mut editor, &mut renderer);
    let close = menu_item(&editor.ui, "Close").unwrap();
    click(&mut editor.ui, close);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspector_count(), 1);
    assert!(!editor.ui.dock_panels(dock).iter().any(|(id, ..)| id == "inspector-2"), "gone, not just closed");
}

#[test]
fn a_floating_game_view_draws_beside_the_scene_view() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let note = editor.ui.find(editor.ui.root(), "game-empty").unwrap();
    editor.ui.float_dock_panel(dock, "game", None);
    for _ in 0..3 {
        // The second view has buffers and images of its own, which a GPU full of every other test's may not fit.
        match editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0) {
            Ok(()) => {}
            Err(error) if format!("{error:#}").contains("Out of memory") => {
                return eprintln!("skipping: no room on the GPU for a second view ({error:#})");
            }
            Err(error) => panic!("{error:#}"),
        }
    }
    assert!(!editor.ui.is_visible(note), "the level has a camera, and the game draws");
    shot(&mut renderer, "float-game.png");

    // The middle of each view is the 3D world, not the panels' dark grey: both draw, each through its camera.
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let at = |node: NodeId| {
        let [x, y, w, h] = editor.ui.rect(node);
        let (x, y) = ((x + w / 2.0) as usize, (y + h * 0.2) as usize);
        let i = (y * width as usize + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let game = at(editor.ui.find(editor.ui.root(), "game-viewport").unwrap());
    let scene = at(editor.ui.find(editor.ui.root(), "viewport").unwrap());
    let bright = |rgb: [u8; 3]| rgb.iter().map(|&c| u32::from(c)).sum::<u32>() > 200;
    assert!(bright(game), "the Game view shows the sky: {game:?}");
    assert!(bright(scene), "and the Scene view too: {scene:?}");

    // The Scene view's light gizmos, drawn under the floating window, do not show through it.
    let [x, y, w, h] = editor.ui.rect(editor.ui.find(editor.ui.root(), "game-viewport").unwrap()).map(|v| v as usize);
    let gizmo = [0xf2, 0xc9, 0x4c].map(i32::from);
    let near_gizmo = (y..y + h)
        .flat_map(|row| (x..x + w).map(move |column| (row * width as usize + column) * 4))
        .filter(|&i| (0..3).all(|c| (i32::from(pixels[i + c]) - gizmo[c]).abs() < 24))
        .count();
    assert_eq!(near_gizmo, 0, "no light gizmo's yellow in the Game view");
}

/// How many pixels in the middle of `rect` are thin dark lines, as a mesh's
/// edges are: darker than the pixels two either side of them.
fn thin_lines(renderer: &mut Renderer, [x, y, w, h]: [f32; 4]) -> usize {
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let light = |row: usize, column: usize| {
        let i = (row * width as usize + column) * 4;
        u32::from(pixels[i]) + u32::from(pixels[i + 1]) + u32::from(pixels[i + 2])
    };
    let (left, right) = ((x + w * 0.1) as usize, (x + w * 0.9) as usize);
    let rows = (y + h * 0.25) as usize..(y + h * 0.8) as usize;
    let at = rows.flat_map(|row| (left..right).map(move |column| (row, column)));
    at.filter(|&(row, column)| light(row, column) * 10 < light(row, column - 2).min(light(row, column + 2)) * 8).count()
}

#[test]
fn scene_views_open_more_than_once_each_through_its_own_camera() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let place = |editor: &voxear_editor::Editor, camera| *editor.game.engine.world.get::<Transform>(camera).unwrap();

    // Add Scene View, from the Scene view's own window menu: a second, looking from where the first does.
    let tab = dock_tab(&editor.ui, "Scene").unwrap();
    context_click(&mut editor.ui, tab);
    frame(&mut editor, &mut renderer);
    let add = menu_item(&editor.ui, "Add Scene View").expect("in the window's menu");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.scene_view_count(), 2);
    let cameras = editor.scene_view_cameras();
    let panels: Vec<&str> = cameras.iter().map(|(panel, _)| panel.as_str()).collect();
    assert_eq!(panels, ["scene", "scene-2"]);
    let (first, second) = (cameras[0].1, cameras[1].1);
    assert_ne!(first, second, "each looks through a camera of its own");
    assert_eq!(place(&editor, second), place(&editor, first), "from where the first looks");
    assert!(editor.ui.dock_panel_neighbours(dock, "scene-2").contains(&"scene".to_string()), "beside it");

    // Floating low over the Project, the second draws beside the first, each shaded as its own menu says: Lit
    // Wireframe in the second draws the meshes' edges there and not in the first.
    editor.ui.float_dock_panel(dock, "scene-2", Some((560.0, f32::MAX)));
    editor.ui.show_dock_panel(dock, "scene");
    frame(&mut editor, &mut renderer);
    let mut viewports = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "viewport", &mut viewports);
    let game = editor.ui.find(editor.ui.root(), "game-viewport").unwrap();
    viewports.retain(|&node| node != game);
    let (one, two) = (viewports[0], viewports[1]);
    let mut menus = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "shading", &mut menus);
    choose(&mut editor, &mut renderer, menus[1], 3);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "scene-views.png");
    assert_eq!(editor.settings().scene_shading, 0, "the first's shading, which is kept, stays Lit");
    let (lit, edged) = (thin_lines(&mut renderer, editor.ui.rect(one)), thin_lines(&mut renderer, editor.ui.rect(two)));
    assert!(edged > lit * 3 + 100, "the second draws edges and the first does not: {edged} thin lines against {lit}");

    // The pointer over a Scene view puts it in use: flying moves that one's camera alone.
    let centre = |rect: [f32; 4]| [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
    let over_second = centre(editor.ui.rect(two));
    pointer_to(&mut editor, over_second);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.scene_view_in_use(), "scene-2");
    let (first_was, second_was) = (place(&editor, first), place(&editor, second));
    editor.fly_mut().set_looking(true);
    editor.fly_mut().key(winit::keyboard::KeyCode::KeyW, true);
    for _ in 0..10 {
        frame(&mut editor, &mut renderer);
    }
    editor.fly_mut().key(winit::keyboard::KeyCode::KeyW, false);
    editor.fly_mut().set_looking(false);
    assert_ne!(place(&editor, second).position, second_was.position, "the second flew");
    assert_eq!(place(&editor, first), first_was, "the first stayed");
    let [x, y, _, h] = editor.ui.rect(one);
    pointer_to(&mut editor, [x + 20.0, y + h - 20.0]);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.scene_view_in_use(), "scene", "the first again, under the pointer");

    // Closed from its menu, the second goes for good, its camera with it.
    let mut tabs = Vec::new();
    with_class(&editor.ui, dock, "tab", &mut tabs);
    let says = |tab: NodeId| editor.ui.children(tab).iter().any(|&part| editor.ui.text(part) == "Scene");
    let last = tabs.into_iter().rfind(|&tab| says(tab)).unwrap();
    context_click(&mut editor.ui, last);
    frame(&mut editor, &mut renderer);
    let close = menu_item(&editor.ui, "Close").unwrap();
    click(&mut editor.ui, close);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.scene_view_count(), 1);
    assert!(!editor.game.engine.world.is_alive(second));
    assert!(editor.game.engine.world.is_alive(first));
    assert_eq!(editor.ui.dock_panels(dock).iter().filter(|(id, ..)| id.starts_with("scene")).count(), 1);
}

#[test]
fn the_mesh_picker_offers_the_meshes_of_the_projects_models() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    // A model no level uses, so nothing has loaded it.
    std::fs::write(dir.path().join("Models/Rug.fbx"), RUG).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let mesh_of = |editor: &voxear_editor::Editor| {
        let mesh = editor.game.engine.world.get::<voxear_render::MeshRenderer>(spinner).unwrap().mesh;
        editor.game.content.mesh_ref(mesh).unwrap().to_string()
    };
    let search = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, text: &str| {
        let picker = first_with(&editor.ui, prop_row(&editor.ui, "Mesh").unwrap(), "value-picker").unwrap();
        let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
        click(&mut editor.ui, arrow);
        for _ in 0..3 {
            frame(editor, renderer);
        }
        editor.ui.text_input(text);
        frame(editor, renderer);
        picker_rows(&editor.ui)
    };

    // A loaded model's mesh, found by its model's name, called by the name the model gives it and its part.
    let rows = search(&mut editor, &mut renderer, "pyramid");
    assert!(rows.contains(&("Pyramid #mesh0.0".to_string(), "Pyramid.gltf".to_string())), "{rows:?}");
    let row = picker_row(&editor.ui, "Pyramid #mesh0.0").unwrap();
    click_across_frames(&mut editor, &mut renderer, row);
    frame(&mut editor, &mut renderer);
    let same = |editor: &voxear_editor::Editor, a: &str, b: &str| editor.game.content.parse(a).ok() == editor.game.content.parse(b).ok();
    let picked = mesh_of(&editor);
    assert!(same(&editor, &picked, "Models/Pyramid.gltf#mesh0.0"), "{picked}");
    let label = first_with(&editor.ui, prop_row(&editor.ui, "Mesh").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Pyramid #mesh0.0", "the field names it as the picker does");

    // Another model, offered by its mesh once loaded, as for its preview, or whole until then, which picked gives
    // its first mesh.
    let rows = search(&mut editor, &mut renderer, "rug");
    let offered = ["Rug #mesh0.0", "Rug.fbx"].into_iter().find(|label| rows.iter().any(|(l, _)| l == label));
    let row = picker_row(&editor.ui, offered.unwrap_or_else(|| panic!("the rug is offered: {rows:?}"))).unwrap();
    click_across_frames(&mut editor, &mut renderer, row);
    frame(&mut editor, &mut renderer);
    let picked = mesh_of(&editor);
    let (model, part) = picked.split_once('#').expect("a part of the model");
    assert!(same(&editor, model, "Models/Rug.fbx") && part.starts_with("mesh"), "its first mesh: {picked}");
}

#[test]
fn the_object_picker_sets_a_mesh_renderers_material() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Red.material"), "color = [0.9, 0.1, 0.1]\n").unwrap();
    std::fs::write(dir.path().join("Models/Rug.fbx"), RUG).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_folder(&mut editor, &mut renderer, "Models");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let from = centre(&editor.ui, tile(&editor.ui, "Rug.fbx").unwrap());
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Day").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    let mut cases = vec![("an FBX model placed from the Project", obj(&editor, "Rug").expect("an Obj for the model"))];
    // A cube with a material of its own, and a model's part, whose material is the model's.
    let world = &editor.game.engine.world;
    let part = world
        .query_ref::<(voxear_ecs::Entity, &Name, &voxear_scene::Parent)>()
        .into_iter()
        .find(|(_, name, _)| name.0 == "Pyramid")
        .map(|(entity, ..)| entity)
        .expect("the Pyramid's part");
    cases.extend([("a cube", obj(&editor, "Spinner A").unwrap()), ("a model's part", part)]);
    for (what, entity) in cases {
        editor.select(Some(entity));
        for _ in 0..3 {
            frame(&mut editor, &mut renderer);
        }
        let picker = first_with(&editor.ui, prop_row(&editor.ui, "Material").unwrap(), "value-picker").unwrap();
        let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
        click(&mut editor.ui, arrow);
        for _ in 0..3 {
            frame(&mut editor, &mut renderer);
        }
        let red = picker_row(&editor.ui, "Red.material").unwrap_or_else(|| panic!("Red is offered for {what}"));
        // Pressed and let go a few frames apart, as a hand does, not within one frame.
        click_across_frames(&mut editor, &mut renderer, red);
        for _ in 0..3 {
            frame(&mut editor, &mut renderer);
        }
        let handle = editor.game.engine.world.get::<voxear_render::MeshRenderer>(entity).unwrap().material;
        let source = editor.game.content.material_source(handle).map(|s| format!("{s:?}"));
        let red_guid = editor.game.content.db().guid_of("Materials/Red.material").unwrap().to_string();
        assert!(source.as_deref().is_some_and(|s| s.contains(&red_guid)), "{what} draws with Red: {source:?}");
        let label = first_with(&editor.ui, prop_row(&editor.ui, "Material").unwrap(), "picker-label").unwrap();
        assert!(editor.ui.text(label).starts_with("Red"), "{what}'s field says Red: {}", editor.ui.text(label));
    }
}

#[test]
fn a_models_materials_extract_into_material_assets_it_draws_with() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_folder(&mut editor, &mut renderer, "Models");
    let pyramid = tile(&editor.ui, "Pyramid.gltf").unwrap();
    click(&mut editor.ui, pyramid);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    // The model's Materials list its one material, by its name, drawn with the file's own, which the field
    // shows by name too, as the model's.
    let row = prop_row(&editor.ui, "Checker").expect("the Pyramid's material, by name");
    let label = first_with(&editor.ui, row, "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Checker");
    assert!(first_with(&editor.ui, row, "picker-embedded").is_some(), "marked as the model's own");

    let mut buttons = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "add-component", &mut buttons);
    let extract = buttons.into_iter().find(|&b| editor.ui.text(b) == "Extract Materials").expect("Extract Materials");
    scroll_to(&mut editor, &mut renderer, extract);
    click(&mut editor.ui, extract);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let db = editor.game.content.db();
    let guid = |path: &str| db.guid_of(path).unwrap_or_else(|| panic!("{path} is an asset"));
    let checker = guid("Models/Pyramid Materials/Checker.material");
    let picture = guid("Models/Pyramid Materials/Checker Colour.png");
    let model = guid("Models/Pyramid.gltf");
    let data: voxear_engine::MaterialData = toml::from_str(
        &std::fs::read_to_string(dir.path().join("Models/Pyramid Materials/Checker.material")).unwrap(),
    )
    .unwrap();
    assert_eq!(data.color_map, Some(picture), "the picture inside the file, written out beside it");
    assert!((data.roughness - 0.55).abs() < 1e-4, "the file's values: {data:?}");
    let remap = voxear_assets::MaterialRemap::read(&dir.path().join("Models/Pyramid.gltf"));
    assert_eq!(remap.get("material0"), Some(checker), "the model draws with what it extracted");
    assert_eq!(editor.game.content.material_remap(model).and_then(|r| r.get("material0")), Some(checker));
    let row = prop_row(&editor.ui, "Checker").unwrap();
    assert_eq!(editor.ui.text(first_with(&editor.ui, row, "picker-label").unwrap()), "Checker");
    assert!(first_with(&editor.ui, row, "picker-embedded").is_none(), "now the extracted asset");

    // The extracted material's colour map is a field, emptied from the object picker.
    let folder = dir.path().join("Models/Pyramid Materials");
    assert!(folder.is_dir());
    open_folder(&mut editor, &mut renderer, "Pyramid Materials");
    let checker = tile(&editor.ui, "Checker.material").unwrap();
    click(&mut editor.ui, checker);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let map = first_with(&editor.ui, prop_row(&editor.ui, "Colour Map").unwrap(), "value-picker").unwrap();
    scroll_to(&mut editor, &mut renderer, map);
    assert_eq!(editor.ui.text(first_with(&editor.ui, map, "picker-label").unwrap()), "Checker Colour");
    let arrow = first_with(&editor.ui, map, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert!(offered.contains(&"Checker Colour.png".to_string()), "pictures are offered: {offered:?}");
    let none = picker_row(&editor.ui, "None").unwrap();
    click_across_frames(&mut editor, &mut renderer, none);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let data: voxear_engine::MaterialData =
        toml::from_str(&std::fs::read_to_string(folder.join("Checker.material")).unwrap()).unwrap();
    assert_eq!(data.color_map, None, "saved without its map");
}

#[test]
fn a_material_dragged_onto_an_obj_shows_on_it_and_stays_where_let_go() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Red.material"), "color = [0.9, 0.1, 0.1]\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    open_folder(&mut editor, &mut renderer, "Materials");
    frame(&mut editor, &mut renderer);
    let red_guid = editor.game.content.db().guid_of("Materials/Red.material").unwrap().to_string();
    let draws_red = |editor: &voxear_editor::Editor, entity| {
        let handle = editor.game.engine.world.get::<voxear_render::MeshRenderer>(entity).unwrap().material;
        editor.game.content.material_source(handle).is_some_and(|source| format!("{source:?}").contains(&red_guid))
    };
    let spinner = obj(&editor, "Spinner A").unwrap();
    let position = editor.game.engine.world.get::<Transform>(spinner).unwrap().position;
    let camera = editor.scene_camera().expect("the Scene view shows");
    let on_spinner = camera.project(position).expect("Spinner A is in view");
    let sky = [on_spinner[0], camera.rect[1] + 30.0];

    // Held over Spinner A, it shows Red; moved off, its own comes back.
    let from = centre(&editor.ui, tile(&editor.ui, "Red.material").expect("the material's tile"));
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(from[0] + 30.0, from[1] - 30.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(on_spinner[0], on_spinner[1]);
    frame(&mut editor, &mut renderer);
    assert!(draws_red(&editor, spinner), "it shows on the Obj under the pointer");
    editor.ui.pointer_move(sky[0], sky[1]);
    frame(&mut editor, &mut renderer);
    assert!(!draws_red(&editor, spinner), "its own comes back when the pointer moves off");

    // Let go of on it, Spinner A keeps Red, as one step to undo.
    editor.ui.pointer_move(on_spinner[0], on_spinner[1]);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(draws_red(&editor, spinner), "it keeps the material it was let go of with");
    shot(&mut renderer, "material-dropped.png");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!draws_red(&editor, spinner), "undo puts its own back");

    // Let go of on an Obj's row in the Hierarchy, that Obj takes it.
    let other = obj(&editor, "Spinner B").unwrap();
    let from = centre(&editor.ui, tile(&editor.ui, "Red.material").unwrap());
    let to = centre(&editor.ui, hierarchy_row(&editor.ui, "Spinner B").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    frame(&mut editor, &mut renderer);
    assert!(draws_red(&editor, other), "dropped on its row in the Hierarchy");
}

#[test]
fn hierarchies_and_project_panels_open_more_than_once() {
    let Some((mut renderer, mut editor)) = open() else { return };
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // Add Hierarchy and Add Project, from each window's own menu, as Add Inspector is.
    for (title, item) in [("Hierarchy", "Add Hierarchy"), ("Project", "Add Project")] {
        let tab = dock_tab(&editor.ui, title).unwrap();
        context_click(&mut editor.ui, tab);
        frame(&mut editor, &mut renderer);
        let add = menu_item(&editor.ui, item).unwrap_or_else(|| panic!("{item} is in the window's menu"));
        click(&mut editor.ui, add);
        frame(&mut editor, &mut renderer);
        frame(&mut editor, &mut renderer);
    }
    assert_eq!((editor.hierarchy_count(), editor.project_panel_count()), (2, 2));
    fn named(ui: &Ui, node: NodeId, name: &str, found: &mut Vec<NodeId>) {
        if ui.name(node) == Some(name) {
            found.push(node);
        }
        for child in ui.children(node) {
            named(ui, child, name, found);
        }
    }
    let all_named = |ui: &Ui, name: &str| {
        let mut found = Vec::new();
        named(ui, ui.root(), name, &mut found);
        found
    };
    let row_in = |ui: &Ui, tree: NodeId, name: &str| {
        let mut names = Vec::new();
        with_class(ui, tree, "row-name", &mut names);
        let label = names.into_iter().find(|&n| ui.is_visible(n) && ui.text(n) == name)?;
        std::iter::successors(Some(label), |&n| ui.parent(n)).find(|&n| ui.has_class(n, "list-row"))
    };

    // A click on a row of the second Hierarchy selects in both, and the Inspector shows it.
    let trees = all_named(&editor.ui, "hierarchy-tree");
    assert_eq!(trees.len(), 2);
    let spinner = obj(&editor, "Spinner A").unwrap();
    let row = row_in(&editor.ui, trees[1], "Spinner A").expect("the second lists the Objs too");
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), Some(spinner));
    assert_eq!(editor.ui.selected_items(trees[0]), [spinner.to_bits()], "the first shows it selected");
    assert_eq!(editor.inspectors_shown(), [Some(spinner)]);

    // A click on the space below a Hierarchy's rows selects nothing: the second's, the tab showing.
    let [x, y, w, h] = editor.ui.rect(trees[1]);
    editor.ui.pointer_move(x + w / 2.0, y + h - 8.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), None, "nothing is selected");
    assert!(editor.ui.selected_items(trees[0]).is_empty(), "in either Hierarchy");

    // The second Project panel opens a folder of its own, and the first stays where it was.
    let files = all_named(&editor.ui, "project-files");
    assert_eq!(files.len(), 2);
    let mut tiles = Vec::new();
    with_class(&editor.ui, files[1], "tile", &mut tiles);
    let models =
        tiles.into_iter().find(|&t| editor.ui.is_visible(t) && editor.ui.text(editor.ui.children(t)[1]) == "Models");
    let [x, y, w, h] = editor.ui.rect(models.expect("the second shows the project's folders"));
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let paths: Vec<String> = all_named(&editor.ui, "project-path").into_iter().map(|p| editor.ui.text(p)).collect();
    assert!(!paths[0].contains("Models") && paths[1].contains("Models"), "{paths:?}");

    // Closed from their menus, both go for good.
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    for title in ["Hierarchy", "Project"] {
        let mut tabs = Vec::new();
        with_class(&editor.ui, dock, "tab", &mut tabs);
        let second = tabs.into_iter().filter(|&t| editor.ui.text(t) == title).last().unwrap();
        context_click(&mut editor.ui, second);
        frame(&mut editor, &mut renderer);
        let close = menu_item(&editor.ui, "Close").unwrap();
        click(&mut editor.ui, close);
        frame(&mut editor, &mut renderer);
    }
    assert_eq!((editor.hierarchy_count(), editor.project_panel_count()), (1, 1));
    let ids: Vec<String> = editor.ui.dock_panels(dock).into_iter().map(|(id, ..)| id).collect();
    assert!(!ids.contains(&"hierarchy-2".to_string()) && !ids.contains(&"project-2".to_string()), "{ids:?}");
}

/// The Project panel's files shown as rows, by their names, with the rows.
fn file_rows(ui: &Ui) -> Vec<(String, NodeId)> {
    let Some(files) = ui.find(ui.root(), "project-files") else { return Vec::new() };
    let mut rows = Vec::new();
    with_class(ui, files, "list-row", &mut rows);
    rows.into_iter()
        .filter(|&row| ui.is_visible(row))
        .filter_map(|row| Some((ui.text(*ui.children(row).get(1)?), row)))
        .collect()
}

#[test]
fn files_move_between_folders_by_drag_and_the_tiles_resize_down_to_rows() {
    // A folder that lists first among the top folder's, so its row shows, with a file in it.
    let prepare = |root: &std::path::Path| {
        std::fs::create_dir_all(root.join("0 Box")).unwrap();
        std::fs::write(root.join("0 Box/Grey.material"), "color = [0.5, 0.5, 0.5]\n").unwrap();
    };
    let Some((dir, mut renderer, mut editor)) = open_copy_with(prepare) else { return };
    let guid = std::fs::read_to_string(dir.path().join("Models/Pyramid.gltf.meta")).unwrap();
    open_folder(&mut editor, &mut renderer, "Models");

    // A tile dragged onto a folder in the tree moves there, with its .meta.
    let from = centre(&editor.ui, tile(&editor.ui, "Pyramid.gltf").unwrap());
    let to = centre(&editor.ui, folder_row(&editor.ui, "Prefabs").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Prefabs/Pyramid.gltf").exists() && !dir.path().join("Models/Pyramid.gltf").exists());
    let moved = std::fs::read_to_string(dir.path().join("Prefabs/Pyramid.gltf.meta")).unwrap();
    assert_eq!(moved, guid, "its GUID goes with it, so references to it still work");
    assert!(editor.game.content.db().guid_of("Prefabs/Pyramid.gltf").is_some());

    // Onto the project's own folder in the tree, it moves out.
    open_folder_in_tree(&mut editor, &mut renderer, "Prefabs");
    let from = centre(&editor.ui, tile(&editor.ui, "Pyramid.gltf").unwrap());
    let to = centre(&editor.ui, folder_row(&editor.ui, "Hello").unwrap());
    drag_between(&mut editor, &mut renderer, from, to);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Pyramid.gltf").exists(), "moved out to the top");

    // The slider at its smallest shows rows, where a row dragged onto a folder's row moves into it.
    let slider = editor.ui.find(editor.ui.root(), "project-size").unwrap();
    editor.ui.set_slider_value(slider, 0.0);
    open_folder_in_tree(&mut editor, &mut renderer, "Hello");
    frame(&mut editor, &mut renderer);
    assert!(tile(&editor.ui, "Models").is_none(), "no tiles at the smallest size");
    let rows = file_rows(&editor.ui);
    let row = |name: &str| rows.iter().find(|(n, _)| n == name).map(|(_, row)| *row).unwrap();
    let (first, models) = (row("0 Box"), row("Models"));
    let (from, to) = (centre(&editor.ui, first), centre(&editor.ui, models));
    drag_between(&mut editor, &mut renderer, from, to);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Models/0 Box/Grey.material").exists(), "a folder moves with what is in it");

    // Each file's size ends in the same place, whether or not its row has an arrow, clear of the list's edge:
    // the model at the top has one, and the scenes have none.
    let size_ends = |editor: &voxear_editor::Editor| -> Vec<f32> {
        file_rows(&editor.ui)
            .into_iter()
            .map(|(_, row)| editor.ui.children(row)[2])
            .filter(|&size| !editor.ui.text(size).is_empty())
            .map(|size| {
                let [left, _, width, _] = editor.ui.rect(size);
                left + width
            })
            .collect()
    };
    open_folder_in_tree(&mut editor, &mut renderer, "Hello");
    let mut ends = size_ends(&editor);
    open_folder_in_tree(&mut editor, &mut renderer, "Scenes");
    ends.extend(size_ends(&editor));
    let list = editor.ui.find(editor.ui.root(), "project-files").unwrap();
    let [x, _, w, _] = editor.ui.rect(list);
    assert!(ends.len() > 2, "files with sizes: {ends:?}");
    assert!(ends.iter().all(|end| (end - ends[0]).abs() < 0.5), "lined up: {ends:?}");
    assert!(x + w - ends[0] > 30.0, "clear of the arrows and the edge: {ends:?} in {x} + {w}");

    // Larger, the tiles are wider.
    open_folder_in_tree(&mut editor, &mut renderer, "Hello");
    editor.ui.set_slider_value(slider, 0.13);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let small = editor.ui.rect(tile(&editor.ui, "Models").unwrap())[2];
    editor.ui.set_slider_value(slider, 1.0);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let large = editor.ui.rect(tile(&editor.ui, "Models").unwrap())[2];
    shot(&mut renderer, "project-large-tiles.png");
    assert!(large > small * 1.8, "{small} then {large}");
}

/// Opens a folder by its row in the Project panel's tree.
fn open_folder_in_tree(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let row = folder_row(&editor.ui, name).unwrap_or_else(|| panic!("a folder row for {name}"));
    click(&mut editor.ui, row);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn the_level_a_level_stands_on_and_its_scenes_weights_are_set_in_the_inspector() {
    let Some((dir, mut renderer, mut editor)) = open_copy() else { return };
    let level = |name: &str| std::fs::read_to_string(dir.path().join("Levels").join(name)).unwrap();
    open_folder(&mut editor, &mut renderer, "Levels");
    frame(&mut editor, &mut renderer);
    let day = tile(&editor.ui, "Day.level").unwrap();
    click(&mut editor.ui, day);
    frame(&mut editor, &mut renderer);
    let base = |ui: &Ui| first_with(ui, prop_row(ui, "Stands on").unwrap(), "value-picker").unwrap();
    let says = |editor: &voxear_editor::Editor, field: NodeId| {
        editor.ui.text(first_with(&editor.ui, field, "picker-label").unwrap())
    };
    assert_eq!(says(&editor, base(&editor.ui)), "World", "Hello's Day stands on World");

    // Its arrow offers the other levels, and None, which takes it away.
    let arrow = first_with(&editor.ui, base(&editor.ui), "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert!(offered.contains(&"Night.level".to_string()) && !offered.contains(&"Day.level".to_string()), "{offered:?}");
    let none = picker_row(&editor.ui, "None").unwrap();
    click_across_frames(&mut editor, &mut renderer, none);
    frame(&mut editor, &mut renderer);
    assert!(!level("Day.level").contains("stands_on"), "{}", level("Day.level"));

    // A level dropped on it is written by its GUID, with its path beside it.
    let world = tile(&editor.ui, "World.level").unwrap();
    let field = base(&editor.ui);
    drag(&mut editor.ui, world, field);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let guid = editor.game.content.db().guid_of("Levels/World.level").unwrap();
    assert!(level("Day.level").contains(&format!("stands_on = \"{guid}\"  # Levels/World.level")), "{}", level("Day.level"));
    assert_eq!(says(&editor, base(&editor.ui)), "World");

    // A scene's weight, typed and entered.
    let weight = first_with(&editor.ui, editor.ui.root(), "level-weight").unwrap();
    type_into(&mut editor.ui, weight, "3");
    editor.ui.key_down(voxear_ui::Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(level("Day.level").contains("weight = 3.0"), "{}", level("Day.level"));

    // World standing on Day would go round in a circle, so it is turned away.
    let world = tile(&editor.ui, "World.level").unwrap();
    click(&mut editor.ui, world);
    frame(&mut editor, &mut renderer);
    let day = tile(&editor.ui, "Day.level").unwrap();
    let field = base(&editor.ui);
    drag(&mut editor.ui, day, field);
    frame(&mut editor, &mut renderer);
    assert!(!level("World.level").contains("stands_on"), "{}", level("World.level"));
}

#[test]
fn the_audio_panel_lists_the_voices_playing() {
    let Some((mut renderer, mut editor)) = open() else { return };
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.open_dock_panel_beside(dock, "audio", "scene");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let summary = editor.audio_panel().summary();
    assert_eq!(editor.ui.text(summary), "0 voices");

    let sound = std::sync::Arc::new(voxear_engine::audio::Sound::from_samples(vec![0.25; 48_000], 1, 48_000));
    let params = voxear_engine::audio::VoiceParams { looping: true, ..Default::default() };
    editor.game.audio.audio.play(&sound, params);
    let started = std::time::Instant::now();
    while editor.ui.text(summary) != "1 voice" {
        assert!(started.elapsed().as_secs() < 10, "the voice is listed: {}", editor.ui.text(summary));
        std::thread::sleep(std::time::Duration::from_millis(20));
        frame(&mut editor, &mut renderer);
    }
    let mut rows = Vec::new();
    with_class(&editor.ui, editor.audio_panel().list(), "entities-row", &mut rows);
    let shown: Vec<String> = rows
        .into_iter()
        .filter(|&row| editor.ui.is_visible(row))
        .map(|row| texts_in(&editor.ui, row).join(" "))
        .filter(|text| !text.trim().is_empty())
        .collect();
    let listed = shown.iter().any(|row| row.starts_with("One-shot") && row.contains("Effects") && row.contains("Loop"));
    assert!(listed, "{shown:?}");
}

/// The first node under `root` whose text is `text`.
fn with_text(ui: &Ui, root: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(root) == text {
        return Some(root);
    }
    ui.children(root).into_iter().find_map(|child| with_text(ui, child, text))
}

#[test]
fn the_plus_on_an_actions_row_adds_a_binding_to_it() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    without_own_actions(dir.path());
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.open_dock_panel_beside(dock, "input", "scene");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.ui.find(editor.ui.root(), "input-panel").unwrap();
    let bindings = |text: &str, action: &str| {
        let file: voxear_input::ActionsFile = toml::from_str(text).unwrap();
        file.maps.iter().flat_map(|map| &map.actions).find(|a| a.name == action).map_or(0, |a| a.bindings.len())
    };
    let before = bindings(voxear_input::DEFAULT_ACTIONS, "Move");

    // Each action's row has a plus, at the top Move's; a binding's row has none.
    let mut adds = Vec::new();
    with_class(&editor.ui, panel, "input-row-add", &mut adds);
    let shown: Vec<_> = adds.iter().copied().filter(|&add| editor.ui.is_visible(add)).collect();
    assert!(!shown.is_empty() && shown.len() < adds.len(), "on actions' rows, not bindings'");
    click(&mut editor.ui, shown[0]);
    frame(&mut editor, &mut renderer);
    let binding = menu_item(&editor.ui, "Binding").expect("the row's plus offers a binding");
    assert!(menu_item(&editor.ui, "2D Vector").is_some() && menu_item(&editor.ui, "Action").is_none());
    click(&mut editor.ui, binding);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(dir.path().join("Input.inputactions")).expect("written to the project's own file");
    assert_eq!(bindings(&text, "Move"), before + 1, "a binding added to Move: {text}");
    assert!(with_text(&editor.ui, panel, "Listen").is_some(), "the new binding picked, to give it a control");
}

#[test]
fn the_input_actions_window_adds_actions_and_listens_for_their_keys() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    without_own_actions(dir.path());
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.open_dock_panel_beside(dock, "input", "scene");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.ui.find(editor.ui.root(), "input-panel").unwrap();
    let file = dir.path().join("Input.inputactions");
    assert!(!file.exists(), "Hello has no actions of its own");

    let add = editor.ui.find(panel, "input-add").expect("the plus over the actions");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let action = menu_item(&editor.ui, "Action").expect("the plus offers an action");
    click(&mut editor.ui, action);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(&file).expect("the actions written to a file of the project's own");
    assert!(text.contains("name = \"New Action\""), "{text}");
    assert!(editor.game.input.action("New Action").is_some(), "the game reads it at once");

    // It is renamed in its row at once, its name selected to type over; Enter keeps what is typed.
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let field = editor.ui.focused().expect("the new action's name, focused");
    assert!(editor.ui.has_class(field, "input-row-rename") && editor.ui.text(field) == "New Action");
    shot(&mut renderer, "input-actions-rename.png");
    editor.ui.text_input("Jump Higher");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(std::fs::read_to_string(&file).unwrap().contains("name = \"Jump Higher\""), "typed over its name");

    // Another, with Escape, keeps the name it was given.
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let action = menu_item(&editor.ui, "Action").unwrap();
    click(&mut editor.ui, action);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let field = editor.ui.focused().expect("the second action's name, focused");
    editor.ui.text_input("Not This");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_ne!(editor.ui.focused(), Some(field), "the rename let go");
    let mut names = Vec::new();
    with_class(&editor.ui, panel, "entities-row-name", &mut names);
    let shown = |name: &str| names.iter().any(|&n| editor.ui.is_visible(n) && editor.ui.text(n) == name);
    assert!(shown("New Action") && shown("Jump Higher"), "the rows show their names again");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("name = \"New Action\"") && !text.contains("Not This"), "{text}");

    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let binding = menu_item(&editor.ui, "Binding").expect("and a binding of the action picked");
    click(&mut editor.ui, binding);
    frame(&mut editor, &mut renderer);
    let listen = with_text(&editor.ui, panel, "Listen").expect("the new binding's Listen button");
    click(&mut editor.ui, listen);
    frame(&mut editor, &mut renderer);
    let shown: Vec<String> = texts_in(&editor.ui, panel);
    assert!(with_text(&editor.ui, panel, "Press a Key").is_some(), "{shown:?}");
    editor.game.input.devices_mut().set_key("K", true);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"Keyboard/K\""), "the key pressed is bound: {text}");
    let mut glyphs = Vec::new();
    with_class(&editor.ui, panel, "prompt-glyph", &mut glyphs);
    assert_eq!(glyphs.len(), 1, "the binding's prompt: one control");
    assert!(editor.ui.has_class(glyphs[0], "sprite"), "K as players see it: Voxear's picture of it");

    // Picked from every control, searched for, a pad's button is shown as an Xbox pad marks it until a pad says
    // otherwise.
    let control = editor.ui.find(panel, "input-control").expect("the binding's Control");
    assert_eq!(texts_in(&editor.ui, control), ["K Key"]);
    click(&mut editor.ui, control);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.picker_open(), "the control picker");
    editor.ui.text_input("south");
    frame(&mut editor, &mut renderer);
    editor.picker_key(voxear_editor::PaletteKey::Enter);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"Gamepad/South\""));
    let mut glyphs = Vec::new();
    with_class(&editor.ui, panel, "prompt-glyph", &mut glyphs);
    assert!(editor.ui.has_class(glyphs[0], "sprite"), "an Xbox pad's A, as Voxear's picture of it");
    shot(&mut renderer, "input-actions-prompt.png");

    // Drawn, the glyph as Voxear draws it.
    let mut drawn = editor.game.input.authored_file().clone();
    drawn.prompt_icons = Some(voxear_engine::prompt_icons::DRAWN.into());
    std::fs::write(&file, drawn.to_text().unwrap()).unwrap();
    editor.game.load_input_actions();
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }
    let mut glyphs = Vec::new();
    with_class(&editor.ui, panel, "prompt-glyph", &mut glyphs);
    assert!(editor.ui.has_class(glyphs[0], "round") && texts_in(&editor.ui, glyphs[0]) == ["A"]);

    // A processor is picked from a menu of them, and its value given in a field of its own.
    let add = editor.ui.find(panel, "input-add-processor").expect("the binding's plus for processors");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let scale = menu_item(&editor.ui, "Scale").expect("Scale among the processors");
    click(&mut editor.ui, scale);
    frame(&mut editor, &mut renderer);
    assert!(std::fs::read_to_string(&file).unwrap().contains("scale(1)"));
    let mut values = Vec::new();
    with_class(&editor.ui, panel, "input-value", &mut values);
    assert_eq!(values.len(), 1, "Scale's Factor");
    type_into(&mut editor.ui, values[0], "2.5");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(std::fs::read_to_string(&file).unwrap().contains("scale(2.5)"));
    shot(&mut renderer, "input-actions.png");
}

#[test]
fn the_localization_tables_window_lists_finds_and_renames_keys() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    let toml = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, format!("{project}\n\n[localization]\nlanguages = [\"en\", \"fr\"]\n")).unwrap();
    let table = "format = 1\n\n[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\nfr = \"Jouer\"\n\n\
                 [[entry]]\nkey = \"menu.quit\"\nen = \"Quit\"\n";
    std::fs::create_dir_all(dir.path().join("Text")).unwrap();
    std::fs::write(dir.path().join("Text/Menu.strings"), table).unwrap();
    std::fs::write(dir.path().join("Menu.vui"), "<vui>\n<Button text=\"@menu.play\" />\n</vui>\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.open_dock_panel_beside(dock, "localization", "scene");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.ui.find(editor.ui.root(), "localization-panel").unwrap();
    assert!(texts_in(&editor.ui, panel).contains(&"2 keys   1 missing".to_string()), "{:?}", texts_in(&editor.ui, panel));
    let list = editor.localization_panel().list().expect("the keys' list");
    let rows = |editor: &voxear_editor::Editor| {
        let mut rows = Vec::new();
        with_class(&editor.ui, list, "entities-row", &mut rows);
        let shown = rows.into_iter().filter(|&row| editor.ui.is_visible(row));
        shown.map(|row| texts_in(&editor.ui, row).join(" ")).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>()
    };
    assert!(rows(&editor).contains(&"menu.quit Menu.strings Quit —".to_string()), "{:?}", rows(&editor));

    let search = editor.ui.find(panel, "localization-search").unwrap();
    type_into(&mut editor.ui, search, "jouer");
    frame(&mut editor, &mut renderer);
    assert_eq!(rows(&editor), ["menu.play Menu.strings Play Jouer"]);

    editor.ui.set_selection(list, &[0]);
    frame(&mut editor, &mut renderer);
    let details = editor.localization_panel().details();
    assert!(texts_in(&editor.ui, details).contains(&"Used once".to_string()), "{:?}", texts_in(&editor.ui, details));
    shot(&mut renderer, "localization-tables.png");
    let rename = editor.ui.find(details, "localization-rename").unwrap();
    type_into(&mut editor.ui, rename, "menu.start");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    let ui = std::fs::read_to_string(dir.path().join("Menu.vui")).unwrap();
    assert!(ui.contains("@menu.start"), "renamed where it is used: {ui}");
    let table = std::fs::read_to_string(dir.path().join("Text/Menu.strings")).unwrap();
    assert!(table.contains("key = \"menu.start\""));
}

#[test]
fn a_string_table_goes_out_to_a_sheet_and_comes_back_from_one() {
    use voxear_engine::localization::StringTable;
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let project_file = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&project_file).unwrap() + "\n[localization]\nlanguages = [\"en\", \"fr\"]\n";
    std::fs::write(&project_file, text).unwrap();
    std::fs::write(dir.path().join("Menu.strings"), "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let menu = tile(&editor.ui, "Menu.strings").expect("the table in the Project panel");
    click(&mut editor.ui, menu);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();

    let export = with_text(&editor.ui, inspector, "Export CSV").expect("the sheet's buttons");
    click(&mut editor.ui, export);
    frame(&mut editor, &mut renderer);
    let sheet = dir.path().join("Menu.csv");
    let written = std::fs::read_to_string(&sheet).expect("the sheet beside the table");
    assert!(written.starts_with("key,comment,limit,en,fr"), "{written}");

    std::fs::write(&sheet, "key,comment,limit,en,fr\nmenu.play,,,Play,Jouer\nmenu.quit,,,Quit,Quitter\n").unwrap();
    let import = with_text(&editor.ui, inspector, "Import CSV").unwrap();
    click(&mut editor.ui, import);
    frame(&mut editor, &mut renderer);
    let shown = texts_in(&editor.ui, inspector);
    let report = shown.iter().find(|text| text.starts_with("1 added")).expect("what the sheet would do");
    assert!(report.contains("+ menu.quit") && report.contains("~ menu.play (fr)"), "{report}");
    let table = StringTable::parse(&std::fs::read_to_string(dir.path().join("Menu.strings")).unwrap()).unwrap();
    assert_eq!(table.entries.len(), 1, "nothing kept before Apply");

    let apply = with_text(&editor.ui, inspector, "Apply").unwrap();
    click(&mut editor.ui, apply);
    frame(&mut editor, &mut renderer);
    let table = StringTable::parse(&std::fs::read_to_string(dir.path().join("Menu.strings")).unwrap()).unwrap();
    assert_eq!(table.entries.len(), 2);
    assert_eq!(table.entries[0].text("fr"), Some("Jouer"));
}

#[test]
fn a_new_particle_system_plays_in_the_scene_and_saves_with_it() {
    use voxear_render::{ParticlePlayback, ParticleSystem};
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    // A copy of Hello, as this saves.
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    run_command(&mut editor, &mut renderer, "new particle system");
    let system = obj(&editor, "Particle System").expect("a Particle System made from the palette");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<ParticleSystem>(system).copied(), Some(ParticleSystem::default()));
    assert!(world.has::<ParticlePlayback>(system), "playing as it is edited");
    editor.save();
    let scenes = std::fs::read_dir(dir.path().join("Scenes")).unwrap();
    let saved = scenes
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path()).ok())
        .any(|text| text.contains("name = \"Particle System\"") && text.contains("[obj.particles]"));
    assert!(saved, "saved with its scene");
}

#[test]
fn a_new_smoke_fills_round_it_in_the_editor() {
    use voxear_render::{Smoke, SmokeFill};
    let Some((mut renderer, mut editor)) = open() else { return };
    run_command(&mut editor, &mut renderer, "new smoke");
    let smoke = obj(&editor, "Smoke").expect("a Smoke made from the palette");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<Smoke>(smoke).copied(), Some(Smoke::default()));
    let fill = world.get::<SmokeFill>(smoke).expect("filled as it is edited");
    assert!(fill.cells.iter().any(|&c| c != 0), "some cells filled");
}

#[test]
fn a_new_mist_gathers_round_it_in_the_editor() {
    use voxear_render::{Mist, MistTrail};
    let Some((mut renderer, mut editor)) = open() else { return };
    run_command(&mut editor, &mut renderer, "new mist");
    let mist = obj(&editor, "Mist").expect("a Mist made from the palette");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<Mist>(mist).copied(), Some(Mist::default()));
    let trail = world.get::<MistTrail>(mist).expect("a trail as it is edited");
    assert!(!trail.points.is_empty(), "standing still, a cloud round it");
}

#[test]
fn a_curve_is_edited_by_pressing_and_dragging_its_keys() {
    use voxear_render::ParticleSystem;
    let Some((mut renderer, mut editor)) = open() else { return };
    run_command(&mut editor, &mut renderer, "new particle system");
    let system = obj(&editor, "Particle System").unwrap();
    editor.select(Some(system));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = prop_row(&editor.ui, "Size Over Life").expect("its size over life");
    scroll_to(&mut editor, &mut renderer, row);
    let graph = first_with(&editor.ui, prop_row(&editor.ui, "Size Over Life").unwrap(), "curve-field").unwrap();
    let particles = |editor: &voxear_editor::Editor| *editor.game.engine.world.get::<ParticleSystem>(system).unwrap();
    let curve = |editor: &voxear_editor::Editor| particles(editor).size_curve;
    assert!(curve(&editor).is_empty());
    let [left, top, width, height] = editor.ui.rect(graph);
    // Pressed halfway along and a quarter of the way up: a key there, as 0 to 1 shows.
    editor.ui.pointer_move(left + width * 0.5, top + height * 0.75);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    let key = curve(&editor).keys()[0];
    assert!((key[0] - 0.5).abs() < 0.03 && (key[1] - 0.25 * 1.1).abs() < 0.05, "a key where pressed: {key:?}");
    // Dragged on to three quarters along.
    editor.ui.pointer_move(left + width * 0.75, top + height * 0.75);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert!((curve(&editor).keys()[0][0] - 0.75).abs() < 0.03, "{:?}", curve(&editor).keys());
    // Picked up again, dragged off the graph and let go: taken out.
    editor.ui.pointer_move(left + width * 0.75, top + height * 0.75);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(left + width * 0.75, top + height + 60.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    assert!(curve(&editor).is_empty(), "taken out: {:?}", curve(&editor).keys());

    // A gradient: a press on its strip adds a colour there, picked, whose red is typed.
    let row = prop_row(&editor.ui, "Colour Over Life").expect("its colour over life");
    let strip = first_with(&editor.ui, row, "gradient-strip").expect("a strip");
    let [left, top, width, height] = editor.ui.rect(strip);
    editor.ui.pointer_move(left + width * 0.25, top + height * 0.5);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let gradient = |editor: &voxear_editor::Editor| particles(editor).color_gradient;
    assert_eq!(gradient(&editor).keys().len(), 1, "a colour where pressed");
    let red = first_with(&editor.ui, prop_row(&editor.ui, "Colour Over Life").unwrap(), "axis-input").unwrap();
    type_into(&mut editor.ui, red, "0");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    let key = gradient(&editor).keys()[0];
    assert!((key[0] - 0.25).abs() < 0.03 && key[1] == 0.0 && key[2] == 1.0, "no red, the rest white: {key:?}");
    shot(&mut renderer, "curve-fields.png");
}

#[test]
fn a_new_car_has_a_vehicle_on_four_wheels() {
    use voxear_engine::physics::{Rigidbody, Vehicle, Wheel};
    let Some((mut renderer, mut editor)) = open() else { return };
    run_command(&mut editor, &mut renderer, "new car");
    let car = obj(&editor, "Car").expect("a car made from the palette");
    let world = &editor.game.engine.world;
    assert!(world.has::<Vehicle>(car) && world.has::<Rigidbody>(car));
    let wheels: Vec<Wheel> = world
        .query_ref::<(&Wheel, &voxear_scene::Parent)>()
        .into_iter()
        .filter(|(_, parent)| parent.0 == car)
        .map(|(wheel, _)| *wheel)
        .collect();
    assert_eq!(wheels.len(), 4);
    assert_eq!(wheels.iter().filter(|w| w.steer_angle > 0.0).count(), 2, "the front two steer");
    assert_eq!(wheels.iter().filter(|w| w.powered).count(), 2, "the back two are driven");
}

#[test]
fn the_lighting_page_names_the_scenes_lighting_and_edits_its_asset() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Lighting")).unwrap();
    let dusk = dir.path().join("Lighting/Dusk.lighting");
    std::fs::write(&dusk, "# Dusk, for the yard\nformat = 1\nlook = \"golden_hour\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let rail = editor.ui.find(editor.ui.root(), "rail-lighting").unwrap();
    click(&mut editor.ui, rail);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.drawer(), Some(voxear_editor::Drawer::Lighting));
    assert_eq!(editor.lighting_shown(), None, "Hello's scenes set their own sky");

    // New lighting settings made from a scene without any carry its own sky over, and the scene names them.
    let top = editor.ui.find(editor.ui.root(), "lighting-scene").unwrap();
    let copy = first_with(&editor.ui, top, "lighting-copy").unwrap();
    let sky = *editor.game.engine.world.resource::<voxear_render::Sky>().unwrap();
    click(&mut editor.ui, copy);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.lighting_shown(), Some("Lighting/New Lighting.lighting"));
    let made = std::fs::read_to_string(dir.path().join("Lighting/New Lighting.lighting")).unwrap();
    assert!(made.contains("[sky]"), "{made}");
    let lit = editor.game.engine.world.resource::<voxear_render::Sky>().unwrap();
    assert_eq!((lit.mode, lit.ambient), (sky.mode, sky.ambient), "lit as it was");

    // The scene's menu: None, every look built in, then the project's.
    let menu = first_with(&editor.ui, top, "value-choice").unwrap();
    // Far down a long menu: picked with the keys, as its option may be below the window's edge.
    editor.ui.focus(menu);
    let dusk_at = 1 + voxear_render::LookName::ALL.len();
    while editor.ui.selected_option(menu) != dusk_at {
        let key = if editor.ui.selected_option(menu) > dusk_at { Key::Up } else { Key::Down };
        editor.ui.key_down(key);
    }
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.lighting_shown(), Some("Lighting/Dusk.lighting"));
    let haze = |editor: &voxear_editor::Editor| editor.game.engine.world.resource::<voxear_render::Sky>().unwrap().haze;
    let golden = voxear_render::Look::of(voxear_render::LookName::GoldenHour);
    assert_eq!(haze(&editor), golden.sky.haze, "the scene is lit by the asset");
    assert!(editor.has_unsaved_changes(), "and names it once saved");

    // A setting changed on the page is written into the asset, and lights the scene at once.
    let field = first_with(&editor.ui, editor.lighting_row("Haze").unwrap(), "value-input").unwrap();
    type_into(&mut editor.ui, field, "4");
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(&dusk).unwrap();
    assert!(text.starts_with("# Dusk, for the yard\n") && text.contains("haze = 4"), "{text}");
    assert_eq!(haze(&editor), 4.0);
    shot(&mut renderer, "lighting-page.png");

    // Built-in lighting is not changed, but a copy of it can be, which the scene then names.
    choose(&mut editor, &mut renderer, menu, 1);
    assert_eq!(editor.lighting_shown(), Some("builtin:golden_hour"));
    let field = first_with(&editor.ui, editor.lighting_row("Haze").unwrap(), "value-input").unwrap();
    assert!(!editor.ui.is_enabled(field), "built-in settings are shown, not changed");
    click(&mut editor.ui, copy);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.lighting_shown(), Some("Lighting/New Lighting 1.lighting"));
    let made = std::fs::read_to_string(dir.path().join("Lighting/New Lighting 1.lighting")).unwrap();
    assert!(made.contains("look = \"golden_hour\""), "{made}");

    editor.save();
    let named: Vec<Option<String>> = ["Scenes/Environment.scene", "Scenes/Day.scene"]
        .map(|p| voxear_engine::scene_file::read(&dir.path().join(p)).unwrap().lighting)
        .into_iter()
        .collect();
    assert!(named.contains(&Some("Lighting/New Lighting 1.lighting".into())), "the active scene names it: {named:?}");

    // Selected in the Project panel, lighting settings show every setting in the Inspector, to change in place.
    // The copies made above were selected, which opened their folder.
    if tile(&editor.ui, "Dusk.lighting").is_none() {
        open_folder(&mut editor, &mut renderer, "Lighting");
    }
    let tile_of_dusk = tile(&editor.ui, "Dusk.lighting").expect("Dusk in the Project panel");
    click(&mut editor.ui, tile_of_dusk);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Lighting/Dusk.lighting"));
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let mut tabs = Vec::new();
    with_class(&editor.ui, inspector, "settings-tab", &mut tabs);
    let fog_tab = tabs.into_iter().find(|&tab| editor.ui.text(tab) == "Fog").expect("a tab for the fog");
    click(&mut editor.ui, fog_tab);
    frame(&mut editor, &mut renderer);
    let row = |editor: &voxear_editor::Editor, title: &str| {
        let mut rows = Vec::new();
        with_class(&editor.ui, inspector, "setting", &mut rows);
        rows.into_iter().find(|&row| editor.ui.children(row).first().is_some_and(|&l| editor.ui.text(l) == title))
    };
    let density = row(&editor, "Density").expect("the fog's density");
    assert!(editor.ui.is_visible(density), "shown while there is fog");
    let name = editor.ui.children(density)[0];
    assert!(editor.ui.tooltip(name).is_some_and(|text| text.contains("thick")), "its name says what it does when rested on");
    let fog = first_with(&editor.ui, row(&editor, "Fog").unwrap(), "checkbox").unwrap();
    click(&mut editor.ui, fog);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(&dusk).unwrap();
    assert!(text.contains("[post.fog]") && text.contains("enabled = false"), "written into the file: {text}");
    assert!(!editor.ui.is_visible(density), "a setting that does nothing without fog is hidden");
    let mut tabs = Vec::new();
    with_class(&editor.ui, inspector, "settings-tab", &mut tabs);
    let checked: Vec<String> = tabs.iter().filter(|&&tab| editor.ui.is_checked(tab)).map(|&tab| editor.ui.text(tab)).collect();
    assert_eq!(checked, ["Fog"], "only the tab shown is picked");
    assert!(!editor.ui.is_checked(fog), "the fog's toggle shows it off");
    shot(&mut renderer, "lighting-inspector.png");
}

#[test]
fn a_click_picks_the_mesh_it_meets_not_a_box_round_the_camera() {
    use voxear_core::math::Quat;
    use voxear_render::{MeshRenderer, Primitive};
    let Some((mut renderer, mut editor)) = open() else { return };
    let camera = editor.scene_camera().unwrap();
    let viewport = editor.ui.find(editor.ui.root(), "viewport").unwrap();
    let [x, y, w, h] = editor.ui.rect(viewport);
    let middle = [x + w * 0.5, y + h * 0.5];
    let (eye, ahead) = camera.ray(middle);
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&voxear_render::MaterialDesc::default()).unwrap();
    let world = &mut editor.game.engine.world;
    // A room round the camera, as a building's parts are round it in Sponza, and a ball in front.
    let around = Transform::new(eye, Quat::IDENTITY, Vec3::splat(60.0));
    let room = world.spawn((MeshRenderer::new(sphere, material), around));
    let at = eye + ahead * 5.0;
    let ball = world.spawn((MeshRenderer::new(sphere, material), Transform::new(at, Quat::IDENTITY, Vec3::ONE)));
    frame(&mut editor, &mut renderer);
    let markers = voxear_editor::scene_tools::Markers::NONE;
    let pick = |editor: &voxear_editor::Editor, renderer: &Renderer, point| {
        voxear_editor::scene_tools::pick(&editor.game.engine.world, renderer, &camera, point, markers)
    };
    assert_eq!(pick(&editor, &renderer, middle), Some(ball), "the ball, not the room the camera stands in");
    // Inside the ball's box, but past its round edge: whatever is behind it, Hello's own Objs or the room.
    let right = ahead.cross(Vec3::Y).normalize();
    let up = right.cross(ahead);
    let corner = camera.project(at + (right + up) * 0.45).unwrap();
    let behind = pick(&editor, &renderer, corner);
    assert!(behind.is_some() && behind != Some(ball), "a click past the ball's edge goes by it: {behind:?}");
    assert_ne!(pick(&editor, &renderer, middle), Some(room));
}

#[test]
fn a_material_picks_a_custom_shader_and_edits_its_properties() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Shaders")).unwrap();
    let shader = "struct Tinted\n{\n    [Property, Color] float3 tint = float3(1.0, 0.5, 0.2);\n    \
                  [Property, Range(0, 2)] float strength = 1.0;\n}\n\n\
                  void surface(Tinted tinted, SurfaceInput input, inout SurfaceOutput surface)\n{\n    \
                  surface.color *= tinted.tint * tinted.strength;\n}\n";
    std::fs::write(dir.path().join("Shaders/Tinted.shader"), shader).unwrap();
    let guid = "5a1d0000000000000000000000000001";
    std::fs::write(dir.path().join("Shaders/Tinted.shader.meta"), format!("guid = \"{guid}\"\n")).unwrap();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Rust.material"), format!("shader = \"{guid}\"\n")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    // The project's shaders compile on a thread of their own as it opens.
    for _ in 0..600 {
        frame(&mut editor, &mut renderer);
        if *renderer.custom_shaders() != voxear_render::CustomShaderState::Compiling {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(*renderer.custom_shaders(), voxear_render::CustomShaderState::Ready);
    frame(&mut editor, &mut renderer);

    // The shader's own Inspector: its functions, properties and that it compiled.
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let texts = |editor: &voxear_editor::Editor| {
        let mut values = Vec::new();
        with_class(&editor.ui, inspector, "value", &mut values);
        values.into_iter().map(|v| editor.ui.text(v)).collect::<Vec<String>>()
    };
    open_folder(&mut editor, &mut renderer, "Shaders");
    let tinted = tile(&editor.ui, "Tinted.shader").expect("the shader's tile");
    click(&mut editor.ui, tinted);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let shown = texts(&editor);
    assert!(shown.iter().any(|t| t == "Tinted") && shown.iter().any(|t| t == "Surface"), "{shown:?}");
    assert!(shown.iter().any(|t| t == "Compiled"), "{shown:?}");

    // The material's Inspector: its Shader, and the shader's properties, whose edits go into its file.
    assert!(editor.inspect_path("Materials/Rust.material"), "the material is in the project");
    frame(&mut editor, &mut renderer);
    let label = first_with(&editor.ui, prop_row(&editor.ui, "Shader").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Tinted");
    let row = prop_row(&editor.ui, "Strength").expect("its property");
    let strength = first_with(&editor.ui, row, "value-input").unwrap();
    assert_eq!(editor.ui.text(strength), "1");
    type_into(&mut editor.ui, strength, "1.5");
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(dir.path().join("Materials/Rust.material")).unwrap();
    assert!(text.contains("strength = 1.5"), "written into the material: {text}");
    assert!(!text.contains("tint"), "a property at its default is left out: {text}");
    assert!(prop_row(&editor.ui, "Tint").is_some(), "a colour property, with its swatch");
}

#[test]
fn a_material_picks_a_built_in_shader_and_one_is_copied_into_the_project() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Paint.material"), "color = [0.8, 0.8, 0.8]\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    assert_eq!(*renderer.custom_shaders(), voxear_render::CustomShaderState::None, "nothing compiled for none");

    // The Shader field offers the built-in shaders after Standard, and Toon is compiled in once picked.
    assert!(editor.inspect_path("Materials/Paint.material"));
    frame(&mut editor, &mut renderer);
    let field = first_with(&editor.ui, prop_row(&editor.ui, "Shader").unwrap(), "value-picker").unwrap();
    let arrow = first_with(&editor.ui, field, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert_eq!(offered[..3], ["Standard", "Toon", "Storybook"], "{offered:?}");
    let toon = picker_row(&editor.ui, "Toon").unwrap();
    click_across_frames(&mut editor, &mut renderer, toon);
    for _ in 0..600 {
        frame(&mut editor, &mut renderer);
        if *renderer.custom_shaders() != voxear_render::CustomShaderState::Compiling {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(*renderer.custom_shaders(), voxear_render::CustomShaderState::Ready);
    let text = std::fs::read_to_string(dir.path().join("Materials/Paint.material")).unwrap();
    assert!(text.contains("shader = \"builtin:toon\""), "{text}");
    frame(&mut editor, &mut renderer);
    let label = first_with(&editor.ui, prop_row(&editor.ui, "Shader").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(label), "Toon");
    assert!(prop_row(&editor.ui, "Steps").is_some() && prop_row(&editor.ui, "Rim width").is_some(), "its settings");

    // New Shader offers what one starts as, the built-in ones to copy and change among them.
    open_folder(&mut editor, &mut renderer, "Materials");
    let paint = tile(&editor.ui, "Paint.material").unwrap();
    context_click(&mut editor.ui, paint);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "New Unlit Shader").is_none(), "one item for shaders");
    let item = menu_item(&editor.ui, "New Shader…").expect("the menu makes shaders");
    click(&mut editor.ui, item);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
    assert_eq!(offered[..4], ["Lit", "Unlit", "UI", "Toon"], "{offered:?}");
    let water = picker_row(&editor.ui, "Water").unwrap();
    click_across_frames(&mut editor, &mut renderer, water);
    frame(&mut editor, &mut renderer);
    let copy = std::fs::read_to_string(dir.path().join("Materials/Water.shader")).expect("a copy of Water");
    assert!(copy.contains("struct Water") && copy.contains("float3 vertex(Water water"), "named as its file");
}

/// A data asset's Definition, as a build describes it: a number and a list,
/// placed in the Create menu by its `[CreateAsset]`.
const WEAPON_STATS: &str = r##"{"behaviours": [], "definitions": [{"name": "Hello.WeaponStats", "bases": [], "interfaces": [],
  "menu": "Weapons/Weapon Stats", "fields": [
  {"name": "damage", "type": {"kind": "number"}, "default": 10.0},
  {"name": "shots", "type": {"kind": "list", "element": {"kind": "whole"}}, "default": []}
]}], "types": {}, "commands": []}"##;

#[test]
fn a_data_asset_made_from_the_create_menu_is_edited_in_the_inspector_into_its_file() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Weapons")).unwrap();
    std::fs::write(dir.path().join("Weapons/Rifle.asset"), "type = \"Hello.WeaponStats\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(WEAPON_STATS).unwrap());
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);

    // New Data Asset… offers each Definition where its [CreateAsset] puts it, and makes a file of the one picked.
    open_folder(&mut editor, &mut renderer, "Weapons");
    assert!(tile(&editor.ui, "Rifle.asset").is_some(), "the rifle's stats in the Project panel");
    // On the space around the files, whose menu is short enough to show whole.
    let files = editor.ui.find(editor.ui.root(), "project-files").unwrap();
    context_click(&mut editor.ui, files);
    frame(&mut editor, &mut renderer);
    let item = menu_item(&editor.ui, "New Data Asset…").expect("the menu makes data assets");
    click(&mut editor.ui, item);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let labels = |ui: &Ui| picker_rows(ui).into_iter().map(|(label, _)| label).collect::<Vec<_>>();
    assert_eq!(labels(&editor.ui), ["Weapons"], "in its menu's folder");
    let weapons = picker_row(&editor.ui, "Weapons").unwrap();
    click(&mut editor.ui, weapons);
    frame(&mut editor, &mut renderer);
    let stats = picker_row(&editor.ui, "Weapon Stats").expect("named as its menu names it");
    click_across_frames(&mut editor, &mut renderer, stats);
    frame(&mut editor, &mut renderer);
    let file = dir.path().join("Weapons/New Weapon Stats.asset");
    let read = || std::fs::read_to_string(&file).unwrap_or_default();
    assert!(read().starts_with("type = \"Hello.WeaponStats\"\n"), "{}", read());

    // Selected, it shows its Definition's fields under its name and type: no Transform, nothing to add.
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Weapons/New Weapon Stats.asset"));
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let text_of = |editor: &voxear_editor::Editor, class: &str| {
        first_with(&editor.ui, inspector, class).map(|node| editor.ui.text(node)).unwrap_or_default()
    };
    assert_eq!((text_of(&editor, "data-name"), text_of(&editor, "data-type")), ("New Weapon Stats".into(), "Weapon Stats".into()));
    assert!(prop_row(&editor.ui, "Position").is_none(), "no Transform");
    assert!(first_with(&editor.ui, inspector, "add-component").is_none(), "no Add Component");
    shot(&mut renderer, "data-asset.png");

    // A number typed is written into the file at once, the level left as it was.
    let damage = first_with(&editor.ui, prop_row(&editor.ui, "Damage").unwrap(), "value-input").unwrap();
    assert_eq!(editor.ui.text(damage), "10", "as the script starts it");
    type_into(&mut editor.ui, damage, "35.5");
    frame(&mut editor, &mut renderer);
    let text = read();
    assert!(text.starts_with("type = \"Hello.WeaponStats\"\n") && text.contains("damage = 35.5"), "{text}");
    assert!(!editor.has_unsaved_changes(), "the level has nothing to save");

    // A list grows by its button, and its item is edited as any field is.
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, inspector, "collection-add").expect("the list can grow");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(read().contains("shots = [0]"), "{}", read());
    let shot_field = first_with(&editor.ui, prop_row(&editor.ui, "Element 0").unwrap(), "value-input").unwrap();
    type_into(&mut editor.ui, shot_field, "3");
    frame(&mut editor, &mut renderer);
    assert!(read().contains("shots = [3]") && read().contains("damage = 35.5"), "{}", read());

    // Its fields are kept on an Obj of the editor's own, out of the Hierarchy, gone once something else shows.
    let holder = obj(&editor, "New Weapon Stats").expect("kept on an Obj while it shows");
    assert!(hierarchy_row(&editor.ui, "New Weapon Stats").is_none());
    editor.ui.blur();
    editor.select(obj(&editor, "Spinner A"));
    frame(&mut editor, &mut renderer);
    assert!(!editor.game.engine.world.is_alive(holder), "its Obj went with it");
    assert!(!editor.has_unsaved_changes());
    let text = read();
    assert!(text.contains("damage = 35.5") && text.contains("shots = [3]"), "{text}");
}

/// Definitions, two of which derive one from the other, one named as a
/// built-in kind is, and a behaviour with a field of two of them.
const DEFINITION_FIELDS: &str = r##"{"behaviours": [{"name": "Hello.Spinner", "bases": [], "interfaces": [], "fields": [
  {"name": "stats", "type": {"kind": "asset", "asset": "Hello.WeaponStats"}, "default": null},
  {"name": "handling", "type": {"kind": "asset", "asset": "Hello.GunHandling"}, "default": null}
]}], "definitions": [
  {"name": "Hello.WeaponStats", "bases": [], "interfaces": [], "fields": []},
  {"name": "Hello.SniperStats", "bases": ["Hello.WeaponStats"], "interfaces": [], "fields": []},
  {"name": "Hello.Armour", "bases": [], "interfaces": [], "fields": []},
  {"name": "Hello.GunHandling", "bases": [], "interfaces": [], "fields": []}
], "types": {}, "commands": []}"##;

#[test]
fn a_definition_field_offers_only_its_own_types_data_assets() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Weapons/Rifle.asset", "type = \"Hello.WeaponStats\"\n");
    write("Weapons/Sniper.asset", "type = \"Hello.SniperStats\"\n");
    write("Weapons/Plate.asset", "type = \"Hello.Armour\"\n");
    write("Weapons/Grip.asset", "type = \"Hello.GunHandling\"\n");
    write("Guns/Rifle.gunhandling", "kind = \"bolt_action\"\n");
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    editor.set_script_types(voxear_engine::script_types::parse(DEFINITION_FIELDS).unwrap());
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str| {
        let field = first_with(&editor.ui, prop_row(&editor.ui, label).unwrap(), "value-picker").unwrap();
        let arrow = first_with(&editor.ui, field, "picker-arrow").unwrap();
        click(&mut editor.ui, arrow);
        for _ in 0..3 {
            frame(editor, renderer);
        }
        let offered: Vec<String> = picker_rows(&editor.ui).into_iter().map(|(label, _)| label).collect();
        offered
    };

    // Its own type's, and those deriving from it; not another Definition's.
    assert_eq!(offered(&mut editor, &mut renderer, "Stats"), ["None", "Rifle.asset", "Sniper.asset"]);
    let sniper = picker_row(&editor.ui, "Sniper.asset").unwrap();
    click_across_frames(&mut editor, &mut renderer, sniper);
    frame(&mut editor, &mut renderer);
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner).unwrap();
    assert_eq!(kept.0[0].fields.get("stats"), Some(&toml::Value::String("Weapons/Sniper.asset".into())));

    // A Definition named as a built-in kind is still its own: its data assets, not Gun Handlings.
    assert_eq!(offered(&mut editor, &mut renderer, "Handling"), ["None", "Grip.asset"]);
}

#[test]
fn a_scenes_inspector_shows_its_lighting_to_change() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Lighting")).unwrap();
    let dusk = dir.path().join("Lighting/Dusk.lighting");
    std::fs::write(&dusk, "format = 1\nlook = \"golden_hour\"\n").unwrap();
    let yard = dir.path().join("Scenes/Yard.scene");
    std::fs::write(&yard, "# The yard\nformat = 1\n\n[post]\nlook = \"storybook\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let select = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, folder: &str, name: &str| {
        if tile(&editor.ui, name).is_none() {
            open_folder(editor, renderer, folder);
        }
        let found = tile(&editor.ui, name).unwrap_or_else(|| panic!("{name} in the Project panel"));
        click(&mut editor.ui, found);
        frame(editor, renderer);
    };
    let pick = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, at: usize| {
        let menu = first_with(&editor.ui, inspector, "value-choice").expect("the scene's lighting menu");
        // Far down a long menu: picked with the keys, as its option may be below the window's edge.
        editor.ui.focus(menu);
        while editor.ui.selected_option(menu) != at {
            let key = if editor.ui.selected_option(menu) > at { Key::Up } else { Key::Down };
            editor.ui.key_down(key);
        }
        frame(editor, renderer);
    };

    // A scene that is not open: its own look shows, and the lighting picked is written into its file.
    select(&mut editor, &mut renderer, "Scenes", "Yard.scene");
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Scenes/Yard.scene"));
    let row = |editor: &voxear_editor::Editor, title: &str| {
        let mut rows = Vec::new();
        with_class(&editor.ui, inspector, "setting", &mut rows);
        rows.into_iter().find(|&row| editor.ui.children(row).first().is_some_and(|&l| editor.ui.text(l) == title))
    };
    let look = first_with(&editor.ui, row(&editor, "Look").unwrap(), "value-choice").unwrap();
    use voxear_render::LookName;
    let storybook = LookName::ALL.iter().position(|(name, ..)| *name == LookName::Storybook);
    assert_eq!(Some(editor.ui.selected_option(look)), storybook.map(|at| at + 1), "the scene's own look");
    let dusk_at = 1 + voxear_render::LookName::ALL.len();
    pick(&mut editor, &mut renderer, dusk_at);
    let text = std::fs::read_to_string(&yard).unwrap();
    assert!(text.starts_with("# The yard\n") && text.contains("lighting = \"Lighting/Dusk.lighting\""), "{text}");
    assert!(!editor.has_unsaved_changes(), "a scene that is not open is changed in its file");

    // The settings it is lit by are changed in place, in their own file.
    let field = first_with(&editor.ui, row(&editor, "Haze").unwrap(), "value-input").unwrap();
    assert!(editor.ui.is_enabled(field));
    type_into(&mut editor.ui, field, "3");
    frame(&mut editor, &mut renderer);
    assert!(std::fs::read_to_string(&dusk).unwrap().contains("haze = 3"));

    // The open scene's lighting is changed as the Lighting page changes it, and kept once saved.
    let active = editor.game.levels.active_scene().unwrap().to_string();
    let name = active.rsplit('/').next().unwrap().to_string();
    select(&mut editor, &mut renderer, "Scenes", &name);
    pick(&mut editor, &mut renderer, 1);
    assert_eq!(editor.game.levels.lighting_of(&active), Some(Some("builtin:golden_hour")));
    assert!(editor.has_unsaved_changes());
    shot(&mut renderer, "scene-inspector-lighting.png");

    // None takes it out again; a copy of its own sky and post-processing is then named in its place.
    select(&mut editor, &mut renderer, "Scenes", "Yard.scene");
    pick(&mut editor, &mut renderer, 0);
    assert!(!std::fs::read_to_string(&yard).unwrap().contains("lighting"));
    let copy = first_with(&editor.ui, inspector, "lighting-copy").unwrap();
    assert_eq!(editor.ui.text(copy), "New Lighting Settings");
    click(&mut editor.ui, copy);
    frame(&mut editor, &mut renderer);
    let made = std::fs::read_to_string(dir.path().join("Lighting/New Lighting.lighting")).unwrap();
    assert!(made.contains("look = \"storybook\""), "{made}");
    let text = std::fs::read_to_string(&yard).unwrap();
    assert!(text.contains("lighting = \"Lighting/New Lighting.lighting\""), "{text}");
}

#[test]
fn an_asset_field_pings_its_file_in_every_project_panel_not_locked() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    for name in ["Brick", "Moss"] {
        std::fs::write(dir.path().join(format!("Materials/{name}.material")), "color = [0.7, 0.3, 0.2]\n").unwrap();
    }
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    editor.add_project_panel("project");
    frame(&mut editor, &mut renderer);

    // By its GUID, as a field holds it: both panels open its folder and light it.
    let brick = editor.game.content.db().guid_of("Materials/Brick.material").unwrap();
    editor.ping_asset(&brick.to_string());
    frame(&mut editor, &mut renderer);
    let both = Some("Materials/Brick.material".to_string());
    assert_eq!(editor.pinged_in_projects(), [both.clone(), both.clone()], "lit in both Project panels");

    // A locked panel stays as it is.
    let mut locks = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "panel-lock", &mut locks);
    assert_eq!(locks.len(), 2, "every Project panel has a padlock");
    click(&mut editor.ui, locks[1]);
    frame(&mut editor, &mut renderer);
    editor.ping_asset("Materials/Moss.material");
    frame(&mut editor, &mut renderer);
    let pinged = editor.pinged_in_projects();
    assert_eq!(pinged.iter().filter(|p| p.as_deref() == Some("Materials/Moss.material")).count(), 1, "{pinged:?}");
    assert!(pinged.contains(&both), "the locked panel keeps what it showed: {pinged:?}");

    // A click on an Obj's Material or Mesh field pings the model they come from, as Unity's object fields do.
    let world = &editor.game.engine.world;
    let pyramid = world
        .query_ref::<(voxear_ecs::Entity, &Name, &voxear_render::MeshRenderer)>()
        .into_iter()
        .find(|(_, name, _)| name.0 == "Pyramid")
        .map(|(entity, ..)| entity)
        .expect("Hello's pyramid");
    editor.select(Some(pyramid));
    frame(&mut editor, &mut renderer);
    for field in ["Material", "Mesh"] {
        let picker = first_with(&editor.ui, prop_row(&editor.ui, field).unwrap(), "value-picker").unwrap();
        click(&mut editor.ui, picker);
        frame(&mut editor, &mut renderer);
        assert_eq!(editor.pinged_in_projects()[0].as_deref(), Some("Models/Pyramid.gltf"), "{field} pings its model");
    }
}

#[test]
fn a_plant_picks_its_foliage_and_things_dropped_land_among_plants() {
    use voxear_editor::scene_tools::{Markers, pick, surface_at};
    use voxear_render::{Foliage, Plant, Primitive};
    let Some((mut renderer, mut editor)) = open() else { return };
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&voxear_render::MaterialDesc::default()).unwrap();
    let camera = editor.scene_camera().unwrap();
    // A row of tall plants a few metres in front of the Scene view's camera, standing on the ground.
    let ahead = camera.position + camera.forward * 6.0;
    let base = Vec3::new(ahead.x, 0.0, ahead.z);
    let plants: Vec<Plant> =
        (-2..=2).map(|i| Plant::new([i as f32 * 0.6, 1.0, 0.0], 0.0, 0.4)).collect();
    let foliage = Foliage {
        mesh: cube,
        material,
        draw_distance: 50.0,
        fade_distance: 2.0,
        shadow_distance: 10.0,
        healthy_color: [1.0; 3],
        dry_color: [1.0; 3],
        color_spread: 8.0,
        impostors: voxear_render::Impostors::Auto,
        impostor_distance: 60.0,
    };
    let grown = voxear_scene::Plants::new(plants);
    let field = editor.game.engine.world.spawn((Name("Field".into()), foliage, grown, Transform::from_position(base)));
    frame(&mut editor, &mut renderer);
    let camera = editor.scene_camera().unwrap();
    let on_plant = camera.project(base + Vec3::Y).expect("the plants are in view");
    let markers = Markers { lights: false, cameras: false, sounds: false, colliders: false };
    assert_eq!(pick(&editor.game.engine.world, &renderer, &camera, on_plant, markers), Some(field), "a plant picks its Foliage");
    let landing = surface_at(&editor.game.engine.world, &renderer, &camera, on_plant).expect("somewhere to land");
    assert!(landing.y.abs() < 0.05, "on the ground among the plants, not on top of them: {landing}");
}

#[test]
fn the_play_menu_button_fits_what_it_says_beside_step() {
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let find = |ui: &Ui, name: &str| ui.find(root, name).unwrap();
    for (play_as, players) in [(voxear_editor::PlayAs::Offline, 0), (voxear_editor::PlayAs::Server, 7)] {
        editor.set_networked_play(play_as, players, voxear_editor::TestNetwork::Good);
        frame(&mut editor, &mut renderer);
        frame(&mut editor, &mut renderer);
        let ui = &editor.ui;
        let [step_x, _, step_w, _] = ui.rect(find(ui, "step"));
        let [x, y, w, h] = ui.rect(find(ui, "multiplayer"));
        let [label_x, label_y, label_w, label_h] = ui.rect(find(ui, "multiplayer-label"));
        let label = ui.text(find(ui, "multiplayer-label"));
        assert!(step_x + step_w <= x, "{label}: the button starts after Step");
        assert!(
            label_x >= x && label_x + label_w <= x + w && label_y >= y && label_y + label_h <= y + h,
            "{label}: the button holds what it says"
        );
        let mut pills = Vec::new();
        with_class(ui, root, "play", &mut pills);
        let [pill_x, _, pill_w, _] = ui.rect(pills[0]);
        assert!(x + w <= pill_x + pill_w, "{label}: the button sits in the pill");
        shot(&mut renderer, &format!("play-menu-{players}.png"));
    }
}

#[test]
fn a_shot_with_the_mouse_locked_stays_in_the_game_though_the_pointer_was_on_play() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some((mut renderer, mut editor)) = open() else { return };
    let root = editor.ui.root();
    let play = editor.ui.find(root, "play").unwrap();
    let [x, y, w, h] = editor.ui.rect(play);
    pointer_to(&mut editor, [x + w / 2.0, y + h / 2.0]);
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // The game locks the mouse, as a shooter does, with the pointer still on the Play button.
    editor.game.display.cursor_lock = voxear_scripting::CursorLock::Locked;
    frame(&mut editor, &mut renderer);
    let middle = editor.game_view_middle().expect("the Game view shows as play starts");
    assert!(
        (middle.0 - (x + w / 2.0)).abs() > 1.0 || (middle.1 - (y + h / 2.0)).abs() > 1.0,
        "the middle is elsewhere"
    );
    left_button(&mut editor, true);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    assert!(editor.is_playing(), "the press was the game's, not the Play button's");
    assert_eq!(editor.game_cursor().map(|(_, lock)| lock), Some(voxear_scripting::CursorLock::Locked), "still held");
}

#[test]
fn project_settings_switch_a_module_off_in_the_project_file_and_add_component() {
    use voxear_engine::modules::Module;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    editor.select(editor.first_mesh());
    frame(&mut editor, &mut renderer);
    let offers = |editor: &voxear_editor::Editor, title: &str| editor.component_offers().iter().any(|t| t == title);
    // Hello was made before modules, so it has Water: an Ocean goes on.
    assert!(offers(&editor, "Ocean"), "Hello predates modules, so Ocean is offered");
    let selected = editor.selected().unwrap();
    editor.game.engine.world.insert(selected, voxear_render::Ocean::default());
    editor.select(None);
    frame(&mut editor, &mut renderer);
    editor.select(Some(selected));
    frame(&mut editor, &mut renderer);

    editor.open_drawer(Some(voxear_editor::Drawer::Project));
    frame(&mut editor, &mut renderer);
    let mut rows = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "module-row", &mut rows);
    let water = rows.iter().copied().find(|&row| label_starting(&editor.ui, row, "Water").is_some()).unwrap();
    let mut toggles = Vec::new();
    with_class(&editor.ui, water, "checkbox", &mut toggles);
    click(&mut editor.ui, toggles[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "project-settings.png");
    let written = Project::open(dir.path()).unwrap();
    assert!(!written.modules().has(Module::Water), "voxear.toml has water off");
    assert!(written.modules().has(Module::Networking), "and the rest as they were");

    editor.open_drawer(None);
    frame(&mut editor, &mut renderer);
    assert!(!offers(&editor, "Pool"), "no Pool without Water");
    let note = label_starting(&editor.ui, editor.ui.root(), "Needs the Water module");
    assert!(note.is_some(), "the Ocean says what it needs");
    shot(&mut renderer, "needs-module.png");
    let button = editor.ui.children(editor.ui.parent(note.unwrap()).unwrap())[1];
    click(&mut editor.ui, button);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(Project::open(dir.path()).unwrap().modules().has(Module::Water), "Turn On switches Water back on");
    assert!(label_starting(&editor.ui, editor.ui.root(), "Needs the Water module").is_none());
    assert!(offers(&editor, "Pool"));

    // Without Networking, Play plays offline, with no button to say otherwise.
    let button = editor.ui.find(editor.ui.root(), "multiplayer").unwrap();
    assert!(editor.ui.is_visible(button));
    editor.set_module(Module::Networking, false);
    frame(&mut editor, &mut renderer);
    assert!(!editor.ui.is_visible(button), "no networked Play without Networking");
    assert!(!offers(&editor, "Net Object"));
}

#[test]
fn project_settings_quality_tab_sets_the_particle_budget_in_the_project_file() {
    use voxear_render::{DEFAULT_PARTICLE_BUDGET, ParticleBudget};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let budget = |editor: &voxear_editor::Editor| editor.game.engine.world.resource::<ParticleBudget>().map(|b| b.0);
    assert_eq!(budget(&editor), Some(DEFAULT_PARTICLE_BUDGET), "a million until the project says otherwise");
    editor.open_drawer(Some(voxear_editor::Drawer::Project));
    frame(&mut editor, &mut renderer);
    let tabs = editor.ui.find(editor.ui.root(), "project-tabs").unwrap();
    let quality = label_starting(&editor.ui, tabs, "Quality").expect("a Quality tab");
    click(&mut editor.ui, quality);
    frame(&mut editor, &mut renderer);
    let label = label_starting(&editor.ui, editor.ui.root(), "Budget").expect("the budget's row");
    let row = editor.ui.parent(label).unwrap();
    assert!(editor.ui.is_visible(editor.ui.parent(row).unwrap()), "the Quality tab shows it");
    let mut fields = Vec::new();
    with_class(&editor.ui, row, "number-input", &mut fields);
    assert_eq!(editor.ui.text(fields[0]), "1000000");
    type_into(&mut editor.ui, fields[0], "250,000");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(Project::open(dir.path()).unwrap().file.quality.particles, 250_000, "voxear.toml has it");
    assert_eq!(budget(&editor), Some(250_000), "and the renderer keeps to it from now on");
    // Back to the default, the file says nothing of it.
    editor.set_quality(&voxear_engine::QualitySettings::default());
    let text = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(!text.contains("[quality]"), "{text}");
}

#[test]
fn project_settings_world_tab_switches_the_floating_origin_on_in_the_project_file() {
    use voxear_engine::origin::{OriginMode, OriginSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    assert_eq!(editor.game.project.file.floating_origin.mode, OriginMode::Off, "off until switched on");
    editor.open_drawer(Some(voxear_editor::Drawer::Project));
    frame(&mut editor, &mut renderer);
    let tabs = editor.ui.find(editor.ui.root(), "project-tabs").unwrap();
    let world = label_starting(&editor.ui, tabs, "World").expect("a World tab");
    click(&mut editor.ui, world);
    frame(&mut editor, &mut renderer);
    let heading = label_starting(&editor.ui, editor.ui.root(), "Floating Origin").expect("its section");
    let body = editor.ui.parent(heading).unwrap();
    assert!(editor.ui.is_visible(body), "the World tab shows the floating origin");
    let mut rows = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "module-row", &mut rows);
    let modules = editor.ui.parent(rows[0]).unwrap();
    assert!(!editor.ui.is_visible(modules), "in place of the modules");

    let mut choices = Vec::new();
    with_class(&editor.ui, body, "value-choice", &mut choices);
    choose(&mut editor, &mut renderer, choices[0], 1);
    let mut fields = Vec::new();
    with_class(&editor.ui, body, "number-input", &mut fields);
    type_into(&mut editor.ui, fields[1], "512");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "project-settings-world.png");
    let on = OriginSettings { mode: OriginMode::Endless, step: 512, ..OriginSettings::default() };
    assert_eq!(Project::open(dir.path()).unwrap().file.floating_origin, on, "voxear.toml has it on");
    assert_eq!(editor.game.project.file.floating_origin, on, "and the game plays with it");
}

#[test]
fn project_settings_world_tab_sets_the_block_worlds_budget_by_preset_or_by_number() {
    use voxear_terrain::{WorldBudget, WorldQuality};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let budget = |editor: &voxear_editor::Editor| WorldBudget::of(&editor.game.engine.world);
    assert_eq!(budget(&editor), WorldBudget::default(), "High until the project says otherwise");
    editor.open_drawer(Some(voxear_editor::Drawer::Project));
    frame(&mut editor, &mut renderer);
    let tabs = editor.ui.find(editor.ui.root(), "project-tabs").unwrap();
    let world = label_starting(&editor.ui, tabs, "World").expect("a World tab");
    click(&mut editor.ui, world);
    frame(&mut editor, &mut renderer);
    let heading = label_starting(&editor.ui, editor.ui.root(), "Block Worlds").expect("its section");
    let body = editor.ui.parent(heading).unwrap();
    assert!(editor.ui.is_visible(body), "on the World tab");
    // Low: every number its own, in the file and in the world at once.
    let mut choices = Vec::new();
    with_class(&editor.ui, body, "value-choice", &mut choices);
    choose(&mut editor, &mut renderer, choices[1], 0);
    frame(&mut editor, &mut renderer);
    let low = WorldQuality::Low.budget().unwrap();
    assert_eq!(Project::open(dir.path()).unwrap().file.world, low, "voxear.toml has it");
    assert_eq!(budget(&editor), low, "and worlds take it up from now on");
    let mut fields = Vec::new();
    with_class(&editor.ui, body, "number-input", &mut fields);
    // After the floating origin's distance and step: threads, columns, upload, view and far distance.
    let shown: Vec<String> = fields[2..].iter().map(|&field| editor.ui.text(field)).collect();
    assert_eq!(shown, ["2", "2", "4", "64", "512"]);
    // A number typed makes it Custom, the rest kept.
    type_into(&mut editor.ui, fields[3], "6");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let custom = WorldBudget { quality: WorldQuality::Custom, columns: 6, ..low };
    assert_eq!(Project::open(dir.path()).unwrap().file.world, custom);
    assert_eq!(budget(&editor), custom);
    assert_eq!(editor.ui.selected_option(choices[1]), 4, "Custom shows");
    shot(&mut renderer, "project-settings-block-worlds.png");
    // High again, the file says nothing of it.
    editor.set_world_budget(&WorldBudget::default());
    let text = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(!text.contains("[world]"), "{text}");
}


/// A model file with two clips, Walk and Sneak, on a two-bone skeleton, its
/// data beside it.
const WALKS: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Walks.bin", "byteLength": 56 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
    { "buffer": 0, "byteOffset": 8, "byteLength": 24 },
    { "buffer": 0, "byteOffset": 32, "byteLength": 24 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] },
    { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" },
    { "bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3" }
  ],
  "nodes": [{ "name": "root", "children": [1] }, { "name": "hips", "translation": [0, 0.9, 0] }],
  "animations": [
    {
      "name": "Walk",
      "samplers": [{ "input": 0, "output": 1 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    },
    {
      "name": "Sneak",
      "samplers": [{ "input": 0, "output": 2 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    }
  ],
  "scenes": [{ "nodes": [0] }]
}"#;

fn write_walks(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("Models")).unwrap();
    std::fs::write(dir.join("Models/Walks.gltf"), WALKS).unwrap();
    let values = [0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
    std::fs::write(dir.join("Models/Walks.bin"), values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())
        .unwrap();
}

#[test]
fn a_gun_handlings_actions_are_based_on_clips_in_its_inspector() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    write_walks(dir.path());
    let file = dir.path().join("voxear.toml");
    std::fs::write(&file, std::fs::read_to_string(&file).unwrap() + "\n[modules]\nguns = true\n").unwrap();
    let handling = dir.path().join("Prefabs/Rifle.gunhandling");
    std::fs::write(&handling, "kind = \"bolt_action_rifle\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    assert!(editor.inspect_path("Prefabs/Rifle.gunhandling"));
    frame(&mut editor, &mut renderer);
    assert!(prop_row(&editor.ui, "Gun Kind").is_some() && prop_row(&editor.ui, "Hip").is_some(), "its kind and holds");
    assert!(prop_row(&editor.ui, "Clip").is_none() && prop_row(&editor.ui, "Gun").is_none(), "each action as built");
    let mode = |editor: &voxear_editor::Editor, at: usize| {
        first_with(&editor.ui, prop_rows(&editor.ui, "Mode")[at], "value-choice").unwrap()
    };

    // Based on a clip, the reload's clip field picks a clip, which the file then bases the reload on.
    let dropdown = mode(&editor, 0);
    scroll_to(&mut editor, &mut renderer, dropdown);
    let dropdown = mode(&editor, 0);
    choose(&mut editor, &mut renderer, dropdown, 1);
    frame(&mut editor, &mut renderer);
    let picker = first_with(&editor.ui, prop_row(&editor.ui, "Clip").unwrap(), "value-picker").unwrap();
    scroll_to(&mut editor, &mut renderer, picker);
    let arrow = first_with(&editor.ui, picker, "picker-arrow").unwrap();
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.picker_open());
    editor.ui.text_input("walks");
    frame(&mut editor, &mut renderer);
    editor.picker_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let read = || toml::from_str::<voxear_engine::guns::handling::GunHandling>(&std::fs::read_to_string(&handling).unwrap()).unwrap();
    let reload = read().offsets(voxear_engine::guns::actions::ActionKind::Reload).and_then(|o| o.clip.clone());
    let reload = reload.expect("the reload is based on the clip");
    assert!(reload.clip.ends_with("#clip:Walk"), "the file's first clip: {}", reload.clip);
    let shown = first_with(&editor.ui, prop_row(&editor.ui, "Clip").unwrap(), "picker-label").unwrap();
    assert_eq!(editor.ui.text(shown), "Walk · Walks.gltf");

    // Based on a clip, how much the gun follows it is typed in, and saved.
    let gun = prop_field(&editor.ui, "Gun").expect("the clip's weights show");
    scroll_to(&mut editor, &mut renderer, gun);
    type_into(&mut editor.ui, gun, "0.5");
    frame(&mut editor, &mut renderer);
    let reload = read().offsets(voxear_engine::guns::actions::ActionKind::Reload).and_then(|o| o.clip.clone()).unwrap();
    assert_eq!((reload.gun, reload.body), (0.5, 1.0), "the gun half, the body as the clip");

    // Played from clips, working the bolt starts with a part, whose arms clip is picked, and takes a mark.
    let dropdown = mode(&editor, 1);
    scroll_to(&mut editor, &mut renderer, dropdown);
    let dropdown = mode(&editor, 1);
    choose(&mut editor, &mut renderer, dropdown, 2);
    frame(&mut editor, &mut renderer);
    let cycle = |handling: voxear_engine::guns::handling::GunHandling| {
        handling.offsets(voxear_engine::guns::actions::ActionKind::Cycle).map(|o| o.parts.clone()).unwrap_or_default()
    };
    assert_eq!(cycle(read()).len(), 1, "a part to start with");
    // The part's Arms clip field, not the reload's Arms weight.
    let arms = prop_rows(&editor.ui, "Arms").into_iter().find_map(|row| first_with(&editor.ui, row, "picker-arrow"));
    let arms = arms.expect("the part's Arms field");
    scroll_to(&mut editor, &mut renderer, arms);
    click(&mut editor.ui, arms);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("walks");
    frame(&mut editor, &mut renderer);
    editor.picker_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(cycle(read())[0].arms.ends_with("#clip:Walk"), "{:?}", cycle(read()));
    let mut adds = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "collection-add", &mut adds);
    let add_mark = adds.into_iter().find(|&add| texts_in(&editor.ui, add).iter().any(|t| t == "Add Mark"));
    let add_mark = add_mark.expect("the part's Add Mark button");
    scroll_to(&mut editor, &mut renderer, add_mark);
    click(&mut editor.ui, add_mark);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let time = prop_field(&editor.ui, "Time").expect("the mark's time");
    scroll_to(&mut editor, &mut renderer, time);
    type_into(&mut editor.ui, time, "0.77");
    frame(&mut editor, &mut renderer);
    let marks = cycle(read())[0].marks.clone();
    assert_eq!(marks.len(), 1);
    assert_eq!((marks[0].at, marks[0].event), (0.77, voxear_engine::guns::actions::Event::Eject), "{marks:?}");

    // The carrier is a bone of the arms clips' skeleton, offered from the clip.
    let carrier = first_with(&editor.ui, prop_row(&editor.ui, "Carrier").unwrap(), "value-choice").unwrap();
    scroll_to(&mut editor, &mut renderer, carrier);
    click(&mut editor.ui, carrier);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let named: Vec<String> = options.iter().map(|&option| texts_in(&editor.ui, option).concat()).collect();
    assert!(named.first().is_some_and(|first| first == "Auto") && named.len() > 1, "Auto, then the bones: {named:?}");
    click(&mut editor.ui, options[0]);
    frame(&mut editor, &mut renderer);

    // The camera looks from where the clips were filmed, or from the eyes when picked.
    let camera = first_with(&editor.ui, prop_row(&editor.ui, "Camera").unwrap(), "value-choice").unwrap();
    scroll_to(&mut editor, &mut renderer, camera);
    click(&mut editor.ui, camera);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let named: Vec<String> = options.iter().map(|&option| texts_in(&editor.ui, option).concat()).collect();
    assert_eq!(named, ["As Filmed", "At the Eyes"]);
    click(&mut editor.ui, options[1]);
    frame(&mut editor, &mut renderer);
    assert_eq!(read().pack.camera, voxear_engine::guns::handling::PackCamera::Eyes);
    shot(&mut renderer, "gun-handling-inspector.png");
}
