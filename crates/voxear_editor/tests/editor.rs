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
    let results = palette_results(&editor.ui);
    assert!(results.iter().take(4).all(|r| r.ends_with("tool")), "{results:?}");
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
    assert_eq!(editor.fly.speed(), 12.0, "the Scene camera flies at it");
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
    console_rows(ui).into_iter().map(|row| ui.text(ui.children(row)[2])).collect()
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
        ["Show all", "Combat 3", "Compiler 1", "Engine 1", "Hello 4", "Render 1"],
        "categories nobody declared: a [tag], the scripts' namespace, the crate, the compiler"
    );
    // Hiding one keeps the menu open for the next; Show all brings them back.
    let render = menu_item(&editor.ui, "Render").unwrap();
    click(&mut editor.ui, render);
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_menu_open(categories), "the menu stays open");
    assert!(console_button(&editor.ui, "Categories · 1 hidden").is_some());
    let all = menu_item(&editor.ui, "Show all").unwrap();
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
    assert_eq!(editor.ui.text(editor.ui.children(rows[0])[3]), "3", "with how many there were");
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

    // The line about Spinner A: selecting it selects the Obj, and its stack
    // shows below, the engine's frames folded.
    search_console(&mut editor, &mut renderer, "spinner a hit");
    assert!(!editor.ui.is_visible(problems[0]), "a good search clears the complaint");
    let row = console_rows(&editor.ui)[0];
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), Some(spinner), "the line's Obj is selected");
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
}

/// The settings rows on screen, by their labels.
fn shown_settings(ui: &Ui) -> Vec<String> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "setting", &mut rows);
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
    with_class(&editor.ui, editor.ui.root(), "settings-tab", &mut tabs);
    let names: Vec<String> = tabs.iter().map(|&t| editor.ui.text(t)).collect();
    assert_eq!(names, ["Scene view", "Play mode", "Console"]);
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
    type_into(&mut editor.ui, field, "Rock.material");
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

    // Back opens the level again, whose instance shows designers only what the prefab chose.
    let back = editor.ui.find(editor.ui.root(), "prefab-back").unwrap();
    click(&mut editor.ui, back);
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
fn the_profiling_workspace_shows_recent_frames_in_the_profiler() {
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

    let workspaces = editor.ui.find(editor.ui.root(), "workspaces").unwrap();
    let segments = editor.ui.children(workspaces);
    assert_eq!(segments.len(), 2, "World and Profiling; Animation waits until there is animation");
    click(&mut editor.ui, segments[1]);
    for _ in 0..6 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(editor.workspace(), 1);
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
    assert_eq!(inside(voxear_core::profiling::Samples::FRAME), ["Game", "Editor", "UI Layout", "Render"]);
    let game = cpu.find("Game").unwrap();
    assert!(inside(game).contains(&"Update".to_string()), "the engine's steps inside the game's: {:?}", inside(game));
    let editor_node = cpu.find("Editor").unwrap();
    assert!(inside(editor_node).contains(&"Inspector".to_string()), "and each panel");
    assert!(cpu.find("propagate transforms").is_some(), "and each system");
    let render = cpu.find("Render").unwrap();
    assert!(cpu.total_ms(render) >= cpu.self_ms(render));
    assert!(!last.gpu.is_empty(), "and the GPU's passes");
    let panel = editor.ui.find(editor.ui.root(), "profiler-panel").unwrap();
    let shown = texts_in(&editor.ui, panel);
    for text in ["Hierarchy", "Calls", "Self ms", "UI Layout", "Objs drawn", "Render"] {
        assert!(shown.iter().any(|t| t == text), "{text} in {shown:?}");
    }

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

    click(&mut editor.ui, segments[0]);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.workspace(), 0);
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

/// The object picker's row that says `label`.
fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
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
        click(&mut editor.ui, picker);
        frame(editor, renderer);
        frame(editor, renderer);
        assert!(editor.picker_open(), "{row}");
    };

    // The material: the default, then the project's, each with its folder.
    open_picker(&mut editor, &mut renderer, "Material");
    let title = editor.ui.find(editor.ui.root(), "picker-title").unwrap();
    assert_eq!(editor.ui.text(title), "Select Material");
    let offered = picker_rows(&editor.ui);
    assert_eq!(offered[0].0, "Default");
    assert!(offered.contains(&("Brick.material".into(), "Materials".into())), "{offered:?}");
    shot(&mut renderer, "object-picker.png");
    // Typed letters narrow it, the best match first, and Enter picks it.
    editor.ui.text_input("gls");
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
        picker_rows(&editor.ui).iter().map(|(label, _)| label.as_str()).collect::<Vec<_>>(),
        ["Cube", "Sphere", "Plane"]
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
    click(&mut editor.ui, picker);
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
echo '{"event":"log","level":"warn","message":"a warning"}'
echo '{"event":"stage","stage":"player","index":3,"of":3}'
echo "{\"event\":\"finished\",\"result\":\"succeeded\",\"seconds\":1.5,\"output\":\"$2/Builds/X\",\"program\":\"$2/Builds/X/Hello\",\"size\":1048576}"
"#;
    const WAITS: &str = r#"echo '{"event":"stage","stage":"scripts","index":1,"of":3}'
read line
echo "{\"event\":\"finished\",\"result\":\"cancelled\",\"seconds\":0.2,\"message\":\"asked to $line\"}"
"#;
    let dir = hello_copy();
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
    for shown in ["Linux Development", "Linux Release", "Windows Development", "Windows Release", "linux-x64"] {
        assert!(texts.iter().any(|text| text == shown), "{shown} in {texts:?}");
    }

    let tools = tempfile::tempdir().unwrap();
    editor.build_window().set_program(fake_vox(tools.path(), "vox-builds", BUILDS));
    click_named(&mut editor, "build-start");
    frame(&mut editor, &mut renderer);
    assert!(editor.build_window().is_building());
    frames_until(&mut editor, &mut renderer, &|editor| !editor.ui.text(find(editor, "build-result")).is_empty());
    assert_eq!(editor.ui.text(find(&editor, "build-result")), "Built in 1.5 s · 1.0 MB");
    let log = editor.build_window().log();
    let expected = [("info", "building Linux Release"), ("warn", "a warning")];
    assert_eq!(log, expected.map(|(level, message)| (level.to_string(), message.to_string())));
    frame(&mut editor, &mut renderer);
    assert!(editor.ui.is_visible(find(&editor, "build-run")), "a built game can be run");
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

fn click_named(editor: &mut voxear_editor::Editor, name: &str) {
    let node = editor.ui.find(editor.ui.root(), name).unwrap();
    click(&mut editor.ui, node);
}

fn ui_parent(ui: &Ui, node: NodeId) -> NodeId {
    ui.parent(node).unwrap()
}

#[test]
fn the_layout_and_workspace_are_kept_between_sessions() {
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
    editor.ui.set_segment(named(&editor, "workspaces"), 1);
    frame(&mut editor, &mut renderer);
    editor.ui.close_dock_panel(dock, "console");
    frame(&mut editor, &mut renderer);
    let profiling = editor.ui.dock_layout(dock);
    assert_ne!(world, profiling);
    // The layout is looked at once a second.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    frame(&mut editor, &mut renderer);
    drop(editor);

    let mut editor = open(&mut renderer);
    let dock = named(&editor, "dock");
    assert_eq!(editor.workspace(), 1, "the workspace it was closed in");
    assert_eq!(editor.ui.dock_layout(dock), profiling);
    editor.ui.set_segment(named(&editor, "workspaces"), 0);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.dock_layout(dock), world, "and the other workspace's as it was left");
}
