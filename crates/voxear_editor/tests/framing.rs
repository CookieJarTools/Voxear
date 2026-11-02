//! Frame Selected, Align With View, Unpack Prefab and copying components,
//! on a copy of Hello with a lamp of its Lamp prefab: a double click in the
//! Hierarchy glides the Scene view to the lamp, Align With View moves the
//! lamp to the camera, the Hierarchy's Unpack Prefab makes it Objs of the
//! scene, and its Light is copied onto the ground, each a step to undo.
//! Skips itself without a GPU.

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Project,
    prefab::{FromPrefab, PrefabInstance},
};
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, Transform};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const LAMP: &str = r#"
[[obj]]
id = 961
name = "Far lamp"
prefab = "Prefabs/Lamp.prefab"
position = [30.0, 0.6, -20.0]
"#;

/// A copy of Hello, inside the repository, with the lamp in its Day scene
/// and without its scripts, which the editor would otherwise build.
fn hello_with_a_lamp() -> tempfile::TempDir {
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
    std::fs::write(&day, scene + LAMP).unwrap();
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> = project.lines().filter(|line| !line.starts_with("scripts")).collect();
    std::fs::write(&file, kept.join("\n")).unwrap();
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn frames(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, count: usize) {
    for _ in 0..count {
        frame(editor, renderer);
    }
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

/// The Hierarchy's row for the Obj named `name`.
fn hierarchy_row(ui: &Ui, name: &str) -> NodeId {
    let tree = ui.find(ui.root(), "hierarchy-tree").unwrap();
    let mut names = Vec::new();
    with_class(ui, tree, "row-name", &mut names);
    let label = names.into_iter().find(|&n| ui.is_visible(n) && ui.text(n) == name).expect("a row for it");
    let mut at = Some(label);
    while let Some(node) = at {
        if ui.has_class(node, "list-row") {
            return node;
        }
        at = ui.parent(node);
    }
    panic!("no row for {name}");
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

fn point_at(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
}

/// Runs a command by its name from the command bar, as Ctrl+K does.
fn run(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input(name);
    frame(editor, renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frames(editor, renderer, 2);
}

fn camera(editor: &voxear_editor::Editor) -> voxear_editor::scene_tools::ViewCamera {
    editor.scene_camera().expect("the Scene view shows")
}

#[test]
fn objs_are_framed_aligned_with_the_view_and_unpacked() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_lamp();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frames(&mut editor, &mut renderer, 2);
    let lamp = obj(&editor, "Far lamp");

    // A double click on its row glides the camera to it, still looking the same way.
    let looking = camera(&editor).forward;
    let row = hierarchy_row(&editor.ui, "Far lamp");
    point_at(&mut editor.ui, row);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frames(&mut editor, &mut renderer, 30);
    assert_eq!(editor.selected(), Some(lamp));
    let view = camera(&editor);
    assert!(view.forward.abs_diff_eq(looking, 1e-4), "framing keeps the way it looks");
    let to_lamp = Vec3::new(30.0, 0.3, -20.0) - view.position;
    let ahead = view.forward.dot(to_lamp.normalize());
    assert!(ahead > 0.98, "the lamp is in the middle of the view: {ahead}");
    assert!(to_lamp.length() < 6.0, "and near enough to fill it: {}", to_lamp.length());

    // Align With View puts it where the camera is, as a step to undo.
    run(&mut editor, &mut renderer, "align with view");
    let placed = *editor.game.engine.world.get::<Transform>(lamp).unwrap();
    assert!(placed.position.abs_diff_eq(view.position, 1e-3), "{} at {}", placed.position, view.position);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let back = editor.game.engine.world.get::<Transform>(lamp).unwrap().position;
    assert!(back.abs_diff_eq(Vec3::new(30.0, 0.6, -20.0), 1e-3), "{back}");

    // Unpacking makes its Objs the scene's own, until undone.
    let glow = voxear_engine::prefab_overrides::members(&editor.game.engine.world, lamp)
        .into_iter()
        .find(|(path, _)| !path.is_root())
        .map(|(_, glow)| glow)
        .unwrap();
    let row = hierarchy_row(&editor.ui, "Far lamp");
    point_at(&mut editor.ui, row);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    let unpack = menu_item(&editor.ui, "Unpack Prefab").expect("an instance's root unpacks");
    point_at(&mut editor.ui, unpack);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frames(&mut editor, &mut renderer, 2);
    let world = &editor.game.engine.world;
    assert!(!world.has::<PrefabInstance>(lamp) && !world.has::<FromPrefab>(glow), "no longer an instance");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert!(world.has::<PrefabInstance>(lamp) && world.get::<FromPrefab>(glow).is_some_and(|f| f.instance == lamp));
}

/// The Inspector's section titled `title`, by its header.
fn section(ui: &Ui, title: &str) -> NodeId {
    let mut headers = Vec::new();
    with_class(ui, ui.root(), "foldout-header", &mut headers);
    let found = headers.into_iter().find(|&h| ui.is_visible(h) && ui.children(h).iter().any(|&p| ui.text(p) == title));
    found.unwrap_or_else(|| panic!("no {title} section"))
}

/// Chooses `label` from the menu a right click on `node` opens.
fn context_choose(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId, label: &str) {
    point_at(&mut editor.ui, node);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(editor, renderer);
    let item = menu_item(&editor.ui, label).unwrap_or_else(|| panic!("the menu offers {label}"));
    point_at(&mut editor.ui, item);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frames(editor, renderer, 2);
}

#[test]
fn a_component_is_copied_and_pasted_onto_another_obj() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_lamp();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frames(&mut editor, &mut renderer, 2);
    let (lamp, ground) = (obj(&editor, "Far lamp"), obj(&editor, "Ground"));
    let light = |editor: &voxear_editor::Editor, obj| editor.game.engine.world.get::<voxear_render::Light>(obj).copied();

    editor.select(Some(lamp));
    frames(&mut editor, &mut renderer, 2);
    let header = section(&editor.ui, "Light");
    context_choose(&mut editor, &mut renderer, header, "Copy Component");
    editor.select(Some(ground));
    frames(&mut editor, &mut renderer, 2);
    assert!(light(&editor, ground).is_none());
    let header = section(&editor.ui, "Mesh Renderer");
    context_choose(&mut editor, &mut renderer, header, "Paste Component As New");
    assert_eq!(light(&editor, ground), light(&editor, lamp), "the lamp's light, on the ground");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(light(&editor, ground).is_none(), "a step to undo");
}
