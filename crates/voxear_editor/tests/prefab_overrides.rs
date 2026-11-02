//! Prefab overrides in the Inspector, on a copy of Hello with two lamps of
//! its Lamp prefab, one turned red: what the red one changes is marked,
//! reverted from a row's menu as a step to undo, and applied to the prefab
//! from the Overrides button, which the other lamp then shows. Skips itself
//! without a GPU.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Light, Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const LAMPS: &str = r#"
[[obj]]
id = 951
name = "Red lamp"
prefab = "Prefabs/Lamp.prefab"
position = [1.0, 0.6, 3.0]
light = { type = "point", color = [1.0, 0.2, 0.2], intensity = 6.0, range = 5.0 }

[[obj]]
id = 952
name = "Plain lamp"
prefab = "Prefabs/Lamp.prefab"
position = [-1.0, 0.6, 3.0]
"#;

/// A copy of Hello, inside the repository, with the lamps in its Day scene
/// and without its scripts, which the editor would otherwise build.
fn hello_with_lamps() -> tempfile::TempDir {
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
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    std::fs::write(&day, scene + LAMPS).unwrap();
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> = project.lines().filter(|line| !line.starts_with("scripts")).collect();
    std::fs::write(&file, kept.join("\n")).unwrap();
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn obj(editor: &voxear_editor::Editor, name: &str) -> Entity {
    let world = &editor.game.engine.world;
    let objs = world.query_ref::<(Entity, &Name)>().without::<voxear_scene::Destroying>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no Obj named {name}"))
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The Inspector's row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> NodeId {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    let found = rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
    found.unwrap_or_else(|| panic!("no {label} row"))
}

/// The Inspector's section titled `title`, by its header.
fn section(ui: &Ui, title: &str) -> NodeId {
    let mut headers = Vec::new();
    with_class(ui, ui.root(), "foldout-header", &mut headers);
    let found = headers.into_iter().find(|&h| ui.children(h).iter().any(|&part| ui.text(part) == title));
    found.unwrap_or_else(|| panic!("no {title} section"))
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

fn press(ui: &mut Ui, node: NodeId, secondary: bool) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    if secondary {
        ui.pointer_secondary(true);
        ui.pointer_secondary(false);
    } else {
        ui.pointer_button(true);
        ui.pointer_button(false);
    }
}

/// Clicks the open menu's item that says `label`.
fn choose(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str) {
    frame(editor, renderer);
    let item = menu_item(&editor.ui, label).unwrap_or_else(|| panic!("the menu offers {label}"));
    press(&mut editor.ui, item, false);
    for _ in 0..3 {
        frame(editor, renderer);
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

fn green(editor: &voxear_editor::Editor, lamp: Entity) -> f32 {
    editor.game.engine.world.get::<Light>(lamp).unwrap().color.y
}

#[test]
fn an_instances_changes_are_marked_reverted_and_applied_from_the_inspector() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_lamps();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let (red, plain) = (obj(&editor, "Red lamp"), obj(&editor, "Plain lamp"));
    let prefab_green = green(&editor, plain);
    editor.select(Some(red));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }

    // Its colour is marked, and its section; where it stands, which places it, is not.
    shot(&mut renderer, "prefab-overrides.png");
    let ui = &editor.ui;
    assert!(ui.has_class(prop_row(ui, "Colour"), "overridden"), "the colour it changes");
    assert!(!ui.has_class(prop_row(ui, "Intensity"), "overridden"), "the intensity it keeps");
    assert!(!ui.has_class(prop_row(ui, "Position"), "overridden"), "placing an instance is no change");
    assert!(ui.has_class(section(ui, "Light"), "overridden"));

    // A row's menu reverts it, as a step to undo.
    let colour = prop_row(&editor.ui, "Colour");
    press(&mut editor.ui, colour, true);
    assert!(menu_item(&editor.ui, "Apply to Prefab 'Lamp'").is_none(), "not open until the next frame");
    choose(&mut editor, &mut renderer, "Revert");
    assert!((green(&editor, red) - prefab_green).abs() < 1e-3, "the prefab's colour again");
    assert!(!editor.ui.has_class(prop_row(&editor.ui, "Colour"), "overridden"));
    editor.undo(&mut renderer);
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }
    let reddish = green(&editor, red);
    assert!(reddish < 0.1, "red again after undo: {reddish}");
    assert!(editor.ui.has_class(prop_row(&editor.ui, "Colour"), "overridden"));

    // Apply All writes it into the prefab, and the plain lamp takes it.
    let mut buttons = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "prefab-button", &mut buttons);
    let overrides = buttons.into_iter().find(|&b| editor.ui.text(b) == "Overrides").expect("the Overrides button");
    press(&mut editor.ui, overrides, false);
    choose(&mut editor, &mut renderer, "Apply All");
    let prefab = std::fs::read_to_string(dir.path().join("Prefabs/Lamp.prefab")).unwrap();
    assert!(prefab.contains("color = [1.0, 0.2, 0.2]"), "the prefab is red: {prefab}");
    assert!((green(&editor, plain) - reddish).abs() < 1e-3, "and so is the plain lamp");
    assert!(!editor.ui.has_class(prop_row(&editor.ui, "Colour"), "overridden"), "nothing left to apply");
}
