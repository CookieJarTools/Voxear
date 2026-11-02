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

/// A gate of three parts of one name and a card for far away, and a tree
/// with LODs named as a model's.
const GATE_AND_TREE: &str = r#"
[[obj]]
id = 911
name = "Gate"
position = [3.0, 1.0, 0.0]

[[obj]]
id = 912
name = "Gate Part"
parent = 911
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 913
name = "Gate Part"
parent = 911
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 914
name = "Gate Part"
parent = 911
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 915
name = "Gate Card"
parent = 911
[obj.renderer]
mesh = "builtin:plane"

[[obj]]
id = 921
name = "Tree"
position = [-3.0, 1.0, 0.0]

[[obj]]
id = 922
name = "Tree_LOD1"
parent = 921

[[obj]]
id = 923
name = "Tree_LOD0"
parent = 921
"#;

/// A copy of Hello, inside the repository, with the rock in its Day scene.
fn hello_with_a_rock() -> tempfile::TempDir {
    hello_with(ROCK)
}

/// A copy of Hello, inside the repository, with `objs` added to its Day scene.
fn hello_with(objs: &str) -> tempfile::TempDir {
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
    std::fs::write(&day, scene + objs).unwrap();
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

    // A mesh's LODs show on its Mesh Renderer as a bar, a part for each, the one the Scene view draws outlined.
    editor.select(Some(high));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "mesh-lods.png");
    let mesh = editor.game.engine.world.get::<voxear_render::MeshRenderer>(high).unwrap().mesh;
    let lods = renderer.mesh_lods(mesh).expect("the sphere's LODs");
    let parts = |editor: &voxear_editor::Editor| {
        let mut parts = Vec::new();
        with_class(&editor.ui, editor.ui.root(), "lod-segment", &mut parts);
        parts
    };
    assert_eq!(parts(&editor).len(), lods.len(), "a part for each LOD");
    assert_eq!(parts(&editor).iter().filter(|&&part| editor.ui.has_class(part, "lod-drawn")).count(), 1, "one drawn now");
    let renderer_of = |editor: &voxear_editor::Editor| *editor.game.engine.world.get::<voxear_render::MeshRenderer>(high).unwrap();
    let bias_row = |editor: &voxear_editor::Editor| prop_rows(&editor.ui, "LOD Bias")[0];
    assert!(renderer_of(&editor).lod_transitions.is_none() && editor.ui.is_visible(bias_row(&editor)));

    // Dragging an edge gives the renderer where its LODs end, as a LOD Group's, and LOD Bias goes, having no use.
    let bar = first_with(&editor.ui, editor.ui.root(), "lod-bar").unwrap();
    let edge = first_with(&editor.ui, bar, "lod-edge").unwrap();
    let ([ex, ey, ew, eh], [bx, _, bw, _]) = (editor.ui.rect(edge), editor.ui.rect(bar));
    editor.ui.pointer_move(ex + ew / 2.0, ey + eh / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(bx + bw * 0.3, ey + eh / 2.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let ends = renderer_of(&editor).lod_transitions.expect("ends of its own");
    assert!((ends[0] - 0.7).abs() < 0.02, "{ends:?}");
    assert!(!editor.ui.is_visible(bias_row(&editor)));

    // Even ends, so each part is wide enough to click: a click moves the Scene view to where that LOD is drawn.
    editor.game.engine.world.get_mut::<voxear_render::MeshRenderer>(high).unwrap().lod_transitions =
        Some([0.8, 0.6, 0.4, 0.2, 0.0]);
    frame(&mut editor, &mut renderer);
    for level in [2, 0] {
        let part = parts(&editor)[level];
        click(&mut editor.ui, part);
        frame(&mut editor, &mut renderer);
        frame(&mut editor, &mut renderer);
        assert!(editor.ui.has_class(parts(&editor)[level], "lod-drawn"), "LOD {level} is drawn after its click");
        let camera = editor.scene_camera().unwrap();
        let towards = (Vec3::new(0.0, 1.0, 0.0) - camera.position).normalize();
        assert!(camera.forward.dot(towards) > 0.99, "looking at the rock");
    }

    // A right-click forces a LOD at every distance, as Unity's Forced LOD does; undo lets it go.
    let right_click = |editor: &mut voxear_editor::Editor, node: NodeId| {
        let [x, y, w, h] = editor.ui.rect(node);
        editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
        editor.ui.pointer_secondary(true);
        editor.ui.pointer_secondary(false);
    };
    let part = parts(&editor)[3];
    right_click(&mut editor, part);
    frame(&mut editor, &mut renderer);
    let force = menu_item(&editor.ui, "Force LOD 3").expect("the part's menu");
    click(&mut editor.ui, force);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(renderer_of(&editor).forced_lod, Some(3));
    let forced = parts(&editor)[3];
    assert!(editor.ui.has_class(forced, "lod-forced") && editor.ui.has_class(forced, "lod-drawn"), "drawn up close");
    editor.undo(&mut renderer);
    assert_eq!(renderer_of(&editor).forced_lod, None);

    // Reset Transitions lets the engine switch them again, and LOD Bias comes back.
    frame(&mut editor, &mut renderer);
    let part = parts(&editor)[1];
    right_click(&mut editor, part);
    frame(&mut editor, &mut renderer);
    let reset = menu_item(&editor.ui, "Reset Transitions").expect("offered while the renderer has ends");
    click(&mut editor.ui, reset);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(renderer_of(&editor).lod_transitions.is_none() && editor.ui.is_visible(bias_row(&editor)));
    let bias = first_with(&editor.ui, bias_row(&editor), "value-input").unwrap();
    type_into(&mut editor.ui, bias, "1.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert_eq!(renderer_of(&editor).lod_bias, 1.5);

    // Add Component makes a level of each Obj inside the rock, in the Hierarchy's order.
    editor.select(Some(rock));
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("LOD Group");
    frame(&mut editor, &mut renderer);
    let list = editor.ui.find(editor.ui.root(), "picker-list").unwrap();
    let mut rows = Vec::new();
    with_class(&editor.ui, list, "list-row", &mut rows);
    let item = rows.into_iter().find(|&row| editor.ui.text(editor.ui.children(row)[1]) == "LOD Group");
    click(&mut editor.ui, item.expect("Add Component offers a LOD Group"));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let made = group(&editor, rock).expect("the LOD Group was added");
    let objs: Vec<Vec<Entity>> = made.levels.iter().map(|l| l.objs.clone()).collect();
    assert_eq!(objs, [vec![high], vec![low], vec![card]]);
    assert_eq!(made.levels.iter().map(|l| l.until).collect::<Vec<_>>(), [0.6, 0.3, 0.15]);

    // As in Unity, only the selected level's settings show under the bar, the first to begin with.
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Objs")[0]), "Rock High");
    assert_eq!(prop_rows(&editor.ui, "Objs").len(), 1, "only the selected level's");
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
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Objs")[0]), "Rock Low");

    // A level shows several Objs, as Unity's LOD lists its renderers: one picked into Add Obj joins it.
    let add = first_with(&editor.ui, prop_rows(&editor.ui, "Add Obj")[0], "picker-arrow").unwrap();
    click(&mut editor.ui, add);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let list = editor.ui.find(editor.ui.root(), "picker-list").unwrap();
    let mut rows = Vec::new();
    with_class(&editor.ui, list, "list-row", &mut rows);
    // The card is in the last level, so it says so, and moves from there.
    let card_row = rows.into_iter().find(|&row| editor.ui.text(editor.ui.children(row)[1]) == "Rock Card · LOD 2");
    click(&mut editor.ui, card_row.unwrap());
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(group(&editor, rock).unwrap().levels[1].objs, [low, card]);
    assert!(group(&editor, rock).unwrap().levels[2].objs.is_empty(), "it left the level it was in");
    let shown = renderer.lod_choices(&editor.game.engine.world, LodCamera::new(Vec3::new(0.0, 1.0, -2.5), 60.0));
    if shown.groups[&rock].1 == Some(1) {
        assert!(!shown.hidden.contains(&low) && !shown.hidden.contains(&card), "both of the level's Objs show");
    }
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(group(&editor, rock).unwrap().levels[1].objs, [low]);
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
    // The Scene view followed the edge, as Unity's does: just past it, where the next level takes over.
    frame(&mut editor, &mut renderer);
    let segments = {
        let mut found = Vec::new();
        with_class(&editor.ui, bar, "lod-segment", &mut found);
        found
    };
    assert!(editor.ui.has_class(segments[1], "lod-shown"), "the second level shows at the edge's distance");

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

fn objs_named(editor: &voxear_editor::Editor, name: &str) -> Vec<Entity> {
    let world = &editor.game.engine.world;
    let mut found: Vec<Entity> =
        world.query_ref::<(Entity, &Name)>().into_iter().filter(|(_, n)| n.0 == name).map(|(e, _)| e).collect();
    found.sort_by_key(|e| e.index());
    found
}

/// The Hierarchy's row showing an Obj called `name`.
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

/// Adds a LOD Group to the Obj selected, through Add Component's search.
fn add_lod_group(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").unwrap();
    click(&mut editor.ui, add);
    frame(editor, renderer);
    frame(editor, renderer);
    editor.ui.text_input("LOD Group");
    frame(editor, renderer);
    let list = editor.ui.find(editor.ui.root(), "picker-list").unwrap();
    let mut rows = Vec::new();
    with_class(&editor.ui, list, "list-row", &mut rows);
    let item = rows.into_iter().find(|&row| editor.ui.text(editor.ui.children(row)[1]) == "LOD Group").unwrap();
    click(&mut editor.ui, item);
    for _ in 0..3 {
        frame(editor, renderer);
    }
}

#[test]
fn a_lod_groups_levels_hold_several_objs_grouped_by_name_and_dropped_on() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with(GATE_AND_TREE);
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let (gate, tree, card) = (obj(&editor, "Gate"), obj(&editor, "Tree"), obj(&editor, "Gate Card"));
    let parts = objs_named(&editor, "Gate Part");
    let levels = |editor: &voxear_editor::Editor, of: Entity| -> Vec<Vec<Entity>> {
        group(editor, of).expect("a LOD Group").levels.iter().map(|level| level.objs.clone()).collect()
    };

    // Parts of one name share a level, as a model's often do; LODs named as a model's go by their number.
    editor.select(Some(gate));
    frame(&mut editor, &mut renderer);
    add_lod_group(&mut editor, &mut renderer);
    assert_eq!(levels(&editor, gate), [parts.clone(), vec![card]]);
    editor.select(Some(tree));
    frame(&mut editor, &mut renderer);
    add_lod_group(&mut editor, &mut renderer);
    assert_eq!(levels(&editor, tree), [vec![obj(&editor, "Tree_LOD0")], vec![obj(&editor, "Tree_LOD1")]]);

    // An Obj dragged from the Hierarchy onto a level of the bar goes in it, leaving the level it was in.
    editor.select(Some(card));
    frame(&mut editor, &mut renderer);
    editor.select(Some(gate));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let bar = first_with(&editor.ui, editor.ui.root(), "lod-bar").unwrap();
    let mut segments = Vec::new();
    with_class(&editor.ui, bar, "lod-segment", &mut segments);
    let row = hierarchy_row(&editor.ui, "Gate Card").expect("the card's row");
    let ([rx, ry, rw, rh], [sx, sy, sw, sh]) = (editor.ui.rect(row), editor.ui.rect(segments[0]));
    editor.ui.pointer_move(rx + rw / 3.0, ry + rh / 2.0);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(rx + rw / 3.0 + 30.0, ry + rh / 2.0 + 30.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_move(sx + sw / 3.0, sy + sh / 2.0);
    frame(&mut editor, &mut renderer);
    editor.ui.pointer_button(false);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let mut first = parts.clone();
    first.push(card);
    assert_eq!(levels(&editor, gate), [first, vec![]]);

    // Add Obj lists the Objs not in the level, each in another saying which, and picked, moves it here.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(levels(&editor, gate), [parts.clone(), vec![card]]);
    let bar = first_with(&editor.ui, editor.ui.root(), "lod-bar").unwrap();
    let mut segments = Vec::new();
    with_class(&editor.ui, bar, "lod-segment", &mut segments);
    let [x, y, w, h] = editor.ui.rect(segments[1]);
    editor.ui.pointer_move(x + w / 3.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, prop_rows(&editor.ui, "Add Obj")[0], "picker-arrow").unwrap();
    click(&mut editor.ui, add);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let list = editor.ui.find(editor.ui.root(), "picker-list").unwrap();
    let mut rows = Vec::new();
    with_class(&editor.ui, list, "list-row", &mut rows);
    let labels: Vec<String> = rows.iter().map(|&row| editor.ui.text(editor.ui.children(row)[1])).collect();
    assert!(labels.contains(&"Gate Part · LOD 0".to_string()) && !labels.iter().any(|l| l.starts_with("Gate Card")), "{labels:?}");
    let part = rows.into_iter().find(|&row| editor.ui.text(editor.ui.children(row)[1]) == "Gate Part · LOD 0").unwrap();
    click(&mut editor.ui, part);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let after = levels(&editor, gate);
    assert_eq!((after[0].len(), after[1].len()), (2, 2), "one part moved to the card's level: {after:?}");
}
