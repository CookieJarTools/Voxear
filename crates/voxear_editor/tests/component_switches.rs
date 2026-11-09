//! The Inspector's ticks, on a copy of Hello with a neon tube added: a
//! Light's tick switches its light out of the frame the renderer draws, the
//! Obj's tick at the top does the same for everything on it, and each is a
//! step to undo. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, Switch};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A tube light in front of the Scene view's camera.
const NEON_TUBE: &str = r#"
[[obj]]
id = 931
name = "Neon Tube"
position = [0.0, 1.0, 0.0]
light = { type = "tube", color = [1.0, 0.2, 0.6], intensity = 20.0, range = 8.0 }
"#;

/// A copy of Hello, inside the repository, with the tube in its Day scene
/// and without its scripts, which the editor would otherwise build.
fn hello_with_a_tube() -> tempfile::TempDir {
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
    std::fs::write(&day, scene + NEON_TUBE).unwrap();
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, project.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
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

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// The tick in the header of the section titled `title`.
fn component_tick(ui: &Ui, title: &str) -> NodeId {
    let mut ticks = Vec::new();
    with_class(ui, ui.root(), "component-switch", &mut ticks);
    let titled = |tick: &NodeId| {
        let header = ui.parent(*tick).expect("a tick is in its header");
        let mut texts = vec![header];
        while let Some(node) = texts.pop() {
            if ui.text(node) == title {
                return true;
            }
            texts.extend(ui.children(node));
        }
        false
    };
    ticks.into_iter().find(titled).unwrap_or_else(|| panic!("no tick for {title}"))
}

/// The light the renderer shaded with across its clusters, a few frames
/// on, as the frames in flight catch up.
fn light_links(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) -> u32 {
    for _ in 0..4 {
        frame(editor, renderer);
    }
    renderer.stats().cluster_light_links
}

#[test]
fn unticking_a_light_takes_it_out_of_the_frame_and_the_objs_tick_takes_everything() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = hello_with_a_tube();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let tube = obj(&editor, "Neon Tube");
    editor.select(Some(tube));
    let lit = light_links(&mut editor, &mut renderer);
    assert!(lit > 0, "the tube lights what is around it");

    let tick = component_tick(&editor.ui, "Light");
    assert!(editor.ui.is_checked(tick), "the Light's tick shows it on");
    click(&mut editor.ui, tick);
    let unlit = light_links(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    assert!(!voxear_scene::is_component_on(world, tube, Switch::Light), "unticked, the Light is off");
    assert!(world.has::<voxear_render::Light>(tube), "and kept, with its values");
    assert!(unlit < lit, "and its light is out of the frame: {unlit} links, from {lit}");
    assert!(!editor.ui.is_checked(component_tick(&editor.ui, "Light")));

    editor.undo(&mut renderer);
    assert!(voxear_scene::is_component_on(&editor.game.engine.world, tube, Switch::Light), "undo ticks it again");
    assert_eq!(light_links(&mut editor, &mut renderer), lit, "and its light is back");
    assert!(editor.ui.is_checked(component_tick(&editor.ui, "Light")));

    let mut active = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "inspector-header", &mut active);
    let active = editor.ui.find(active[0], "obj-active").expect("the Obj's tick is at the top");
    click(&mut editor.ui, active);
    let off = light_links(&mut editor, &mut renderer);
    assert!(!voxear_scene::is_active_self(&editor.game.engine.world, tube), "the Obj's tick switches it off");
    assert!(voxear_scene::is_component_on(&editor.game.engine.world, tube, Switch::Light), "its Light's own stays");
    assert_eq!(off, unlit, "and nothing of it lights the frame");

    editor.save();
    let saved = voxear_engine::scene_file::read(&dir.path().join("Scenes/Day.scene")).unwrap();
    let written = saved.objs.iter().find(|o| o.name == "Neon Tube").unwrap();
    assert_eq!(written.active, Some(false), "and the scene saves it off");
}
