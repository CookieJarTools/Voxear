//! LOD Groups and a mesh's LODs in the Inspector, on a copy of Hello with a
//! rock of three levels added: Add Component makes a level of each Obj
//! inside, and the bar, as in Unity, selects a level, drags its edge and
//! deletes one from its menu, each a step to undo. Skips itself without a
//! GPU.

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{LodCamera, LodGroup, Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A rock in front of the Scene view's camera: a sphere close up, a cube
/// further away, and a flat plane further still.
const ROCK: &str = r#"
[[obj]]
id = 901
name = "Rock"
position = [0.0, 1.0, 0.0]

[[obj]]
id = 902
name = "Rock High"
parent = 901
[obj.renderer]
mesh = "builtin:sphere"

[[obj]]
id = 903
name = "Rock Low"
parent = 901
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 904
name = "Rock Card"
parent = 901
[obj.renderer]
mesh = "builtin:plane"
"#;

/// A copy of Hello, inside the repository, with the rock in its Day scene.
fn hello_with_a_rock() -> tempfile::TempDir {
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
    std::fs::write(&day, scene + ROCK).unwrap();
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

fn first_with(ui: &Ui, node: NodeId, class: &str) -> Option<NodeId> {
    let mut found = Vec::new();
    with_class(ui, node, class, &mut found);
    found.first().copied()
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

/// Every Inspector row labelled `label`.
fn prop_rows(ui: &Ui, label: &str) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.retain(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
    rows
}

/// What a row shows after its label.
fn row_text(ui: &Ui, row: NodeId) -> String {
    fn texts(ui: &Ui, node: NodeId, out: &mut Vec<String>) {
        out.push(ui.text(node));
        ui.children(node).into_iter().for_each(|child| texts(ui, child, out));
    }
    let mut out = Vec::new();
    ui.children(row).into_iter().skip(1).for_each(|part| texts(ui, part, &mut out));
    out.retain(|text| !text.is_empty());
    out.join(" ")
}

fn says(ui: &Ui, text: &str) -> bool {
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if ui.text(node).contains(text) {
            return true;
        }
        stack.extend(ui.children(node));
    }
    false
}

fn type_into(ui: &mut Ui, field: NodeId, text: &str) {
    ui.focus(field);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Char('a'));
    ui.set_modifiers(Modifiers::default());
    ui.text_input(text);
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

fn group(editor: &voxear_editor::Editor, rock: Entity) -> Option<LodGroup> {
    editor.game.engine.world.get::<LodGroup>(rock).cloned()
}

#[test]
fn a_lod_group_is_added_edited_and_shown_in_the_inspector() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_rock();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let (rock, high, low, card) =
        (obj(&editor, "Rock"), obj(&editor, "Rock High"), obj(&editor, "Rock Low"), obj(&editor, "Rock Card"));

    // A mesh's LODs show on its Mesh Renderer as a strip of cells, the one the Scene view draws lit.
    editor.select(Some(high));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "mesh-lods.png");
    let mesh = editor.game.engine.world.get::<voxear_render::MeshRenderer>(high).unwrap().mesh;
    let lods = renderer.mesh_lods(mesh).expect("the sphere's LODs");
    let mut cells = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "lod-cell", &mut cells);
    assert_eq!(cells.len(), lods.len(), "a cell for each LOD");
    assert_eq!(cells.iter().filter(|&&cell| editor.ui.has_class(cell, "lod-drawn")).count(), 1, "one drawn now");

    // Add Component makes a level of each Obj inside the rock, in the Hierarchy's order.
    editor.select(Some(rock));
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let item = menu_item(&editor.ui, "LOD Group").expect("Add Component offers a LOD Group");
    click(&mut editor.ui, item);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let made = group(&editor, rock).expect("the LOD Group was added");
    assert_eq!(made.levels.iter().map(|l| l.obj).collect::<Vec<_>>(), [Some(high), Some(low), Some(card)]);
    assert_eq!(made.levels.iter().map(|l| l.until).collect::<Vec<_>>(), [0.6, 0.3, 0.15]);

    // As in Unity, only the selected level's settings show under the bar, the first to begin with.
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "LOD 0")[0]), "Rock High");
    assert!(prop_rows(&editor.ui, "LOD 1").is_empty());
    let transition = prop_rows(&editor.ui, "Transition");
    assert_eq!(transition.len(), 1);
    assert_eq!(row_text(&editor.ui, transition[0]), "60 %");
    let bar = first_with(&editor.ui, editor.ui.root(), "lod-bar").expect("the bar of levels");
    assert!(says(&editor.ui, "Culled") && editor.ui.rect(bar)[2] > 100.0);
    shot(&mut renderer, "lod-group.png");
    let eye = first_with(&editor.ui, editor.ui.root(), "lod-eye-text").expect("the camera's mark");
    assert!(editor.ui.text(eye).ends_with('%'), "the mark reads a share of the screen: {}", editor.ui.text(eye));

    // Close up the Scene view sees the first level; far away, nothing.
    let world = &editor.game.engine.world;
    let near = renderer.lod_choices(world, LodCamera::new(Vec3::new(0.0, 1.0, -1.5), 60.0));
    assert_eq!(near.groups[&rock].1, Some(0));
    assert!(near.hidden.contains(&low) && near.hidden.contains(&card) && !near.hidden.contains(&high));
    let far = renderer.lod_choices(world, LodCamera::new(Vec3::new(0.0, 1.0, -500.0), 60.0));
    assert_eq!(far.groups[&rock].1, None);
    assert!([high, low, card].iter().all(|level| far.hidden.contains(level)));

    // A click on the bar selects a level, and its transition is typed in as a percentage of the screen.
    let mut segments = Vec::new();
    with_class(&editor.ui, bar, "lod-segment", &mut segments);
    click(&mut editor.ui, segments[1]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "LOD 1")[0]), "Rock Low");
    let transition = prop_rows(&editor.ui, "Transition")[0];
    let field = first_with(&editor.ui, transition, "value-input").unwrap();
    type_into(&mut editor.ui, field, "25");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels[1].until, 0.25);

    // Dragging the first level's edge to the middle of the bar ends it at half the screen.
    std::thread::sleep(std::time::Duration::from_millis(900));
    let bar = first_with(&editor.ui, editor.ui.root(), "lod-bar").unwrap();
    let edge = first_with(&editor.ui, bar, "lod-edge").unwrap();
    let ([ex, ey, ew, eh], [bx, _, bw, _]) = (editor.ui.rect(edge), editor.ui.rect(bar));
    editor.ui.pointer_move(ex + ew / 2.0, ey + eh / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(bx + bw / 2.0, ey + eh / 2.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let first = group(&editor, rock).unwrap().levels[0].until;
    assert!((first - 0.5).abs() < 0.02, "{first}");

    // A right-click on a level offers Delete, as in Unity; undo puts it back, then the drag, the typing
    // and the LOD Group itself go, each a step.
    let mut segments = Vec::new();
    with_class(&editor.ui, bar, "lod-segment", &mut segments);
    let [x, y, w, h] = editor.ui.rect(segments[2]);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    let delete = menu_item(&editor.ui, "Delete").expect("the level's menu");
    click(&mut editor.ui, delete);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels.len(), 2);
    editor.undo(&mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels.len(), 3);
    editor.undo(&mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels[0].until, 0.6, "the drag undone");
    editor.undo(&mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels[1].until, 0.3, "the typing undone");
    editor.undo(&mut renderer);
    assert!(group(&editor, rock).is_none(), "and then the LOD Group");
}
