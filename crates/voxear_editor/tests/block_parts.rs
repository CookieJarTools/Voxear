//! Blocks of parts in the editor, on a copy of Hello: the Build tool puts a
//! stair down rising away from the view and R turns the next on a quarter,
//! a door goes down with its upper block and is broken with it, the Use
//! brush opens it and undo shuts it again; and a block's Inspector offers
//! every shape, with a door's motion's keys to change. Skips itself
//! without a GPU.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_terrain::blocks::{
    BlockWorld,
    shape::{BlockFacing, BlockHalf, BlockState},
    usable::state_of,
};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const FIELD: &str = "\n[[obj]]\nid = 931\nname = \"Field\"\nposition = [-256.0, 0.5, -256.0]\n";

/// A copy of Hello with an empty Obj in its Day scene, and a door, a stair
/// and a lever among its blocks.
fn hello_with_parts() -> tempfile::TempDir {
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
    std::fs::write(&day, scene + FIELD).unwrap();
    std::fs::create_dir_all(dir.path().join("Parts")).unwrap();
    std::fs::write(dir.path().join("Parts/Door.block"), "color = [0.5, 0.35, 0.2]\nshape = \"door\"\n").unwrap();
    std::fs::write(dir.path().join("Parts/Stairs.block"), "color = [0.6, 0.6, 0.6]\nshape = \"stairs\"\n").unwrap();
    std::fs::write(dir.path().join("Parts/Plain.block"), "color = [0.3, 0.3, 0.7]\n").unwrap();
    let toml = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, text.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
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

/// Every Inspector row labelled `label`.
fn prop_rows(ui: &Ui, label: &str) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "inspector").expect("the Inspector"), "prop", &mut rows);
    rows.retain(|&row| ui.is_visible(row) && ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
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

/// The object picker's row that says `label`.
fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
}

/// Adds a component as a user does: Add Component, its name typed into the
/// search, and a click on it.
fn add_component(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").expect("Add Component");
    click(&mut editor.ui, add);
    frame(editor, renderer);
    frame(editor, renderer);
    editor.ui.text_input(name);
    frame(editor, renderer);
    let row = picker_row(&editor.ui, name).unwrap_or_else(|| panic!("{name} is offered"));
    click(&mut editor.ui, row);
    frame(editor, renderer);
    frame(editor, renderer);
}

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

/// Writes what the renderer last drew to `name` in `VOXEAR_TEST_OUTPUT`, when set.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
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


/// Scrolls whatever holds `node` until it shows near the top, as a hand scrolls down to it.
fn scroll_to(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let mut at = editor.ui.parent(node);
    while let Some(parent) = at {
        if editor.ui.scroll_max(parent)[1] > 0.0 {
            let ([_, top, _, _], [_, y, _, _]) = (editor.ui.rect(parent), editor.ui.rect(node));
            let [x, now] = editor.ui.scroll_offset(parent);
            editor.ui.set_scroll(parent, [x, (now + y - top - 120.0).max(0.0)]);
            frame(editor, renderer);
            return;
        }
        at = editor.ui.parent(parent);
    }
}

/// Opens the object picker of the Inspector row labelled `label`, scrolled to where it shows.
fn open_picker(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str) {
    let row = *prop_rows(&editor.ui, label).first().unwrap_or_else(|| panic!("no row says {label}"));
    scroll_to(editor, renderer, row);
    let row = *prop_rows(&editor.ui, label).first().unwrap_or_else(|| panic!("no row says {label}"));
    let picker = first_with(&editor.ui, row, "value-picker").unwrap_or_else(|| panic!("{label} has a picker"));
    let arrow = first_with(&editor.ui, picker, "picker-arrow").expect("its arrow");
    click(&mut editor.ui, arrow);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// What the open object picker offers, by each row's name, in order.
fn picker_rows(ui: &Ui) -> Vec<String> {
    let list = ui.find(ui.root(), "picker-list");
    let rows = list.map_or(Vec::new(), |list| ui.children(list));
    // From the top: a list's rows are used again as it is filled, so they are not in the order they were made.
    let mut shown: Vec<NodeId> =
        rows.into_iter().filter(|&row| ui.has_class(row, "list-row") && ui.is_visible(row)).collect();
    shown.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    shown.into_iter().map(|row| ui.text(ui.children(row)[1])).collect()
}

/// Picks the asset whose file is `name` for the Inspector row labelled
/// `label`, as a user does: the row's picker opened, and its row clicked.
fn pick_asset(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str, name: &str) {
    open_picker(editor, renderer, label);
    let row = picker_row(&editor.ui, name);
    let row = row.unwrap_or_else(|| panic!("{name} is offered: {:?}", picker_rows(&editor.ui)));
    click(&mut editor.ui, row);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// Clicks the Scene view at `point`: pressed, a frame, released. The
/// pointer comes to it from beside it, so the editor sees it move there
/// after a click on the Inspector moved the UI's pointer alone.
fn click_scene(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, point: [f32; 2]) {
    pointer_to(editor, [point[0] + 1.0, point[1]]);
    pointer_to(editor, point);
    frame(editor, renderer);
    left_button(editor, true);
    frame(editor, renderer);
    left_button(editor, false);
    frame(editor, renderer);
}

/// A Block World added on the field, moved under the bottom middle of the
/// Scene view: the editor, the field, and where on screen to click and
/// which way the view looks there.
fn editor_with_blocks(renderer: &mut Renderer, dir: &tempfile::TempDir) -> (voxear_editor::Editor, Entity, [f32; 2], [f32; 3]) {
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, renderer);
    add_component(&mut editor, renderer, "Block World");
    frame(&mut editor, renderer);
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.8];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * ((0.5 - origin.y) / direction.y);
    let corner = voxear_core::math::Vec3::new(hit.x - 32.0, -7.5, hit.z - 32.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position = corner;
    frame(&mut editor, renderer);
    (editor, field, point, direction.to_array())
}

/// The state of the block at `at` in the field, and whether one is there.
fn state_at(editor: &voxear_editor::Editor, field: Entity, at: [i32; 3]) -> Option<BlockState> {
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap();
    let id = blocks.blocks.get(at);
    (id != 0).then(|| state_of(&blocks.faces, id).unwrap_or_default())
}

/// The block the Scene view's ray at `point` would put a block in.
fn placed_at(editor: &voxear_editor::Editor, field: Entity, point: [f32; 2]) -> [i32; 3] {
    let camera = editor.scene_camera().unwrap();
    let (origin, direction) = camera.ray(point);
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(field).unwrap().position;
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap();
    let met = voxear_terrain::blocks::raycast(&blocks.blocks, (origin - corner).to_array(), direction.to_array(), 500.0)
        .expect("a block");
    [0, 1, 2].map(|i| met.block[i] + met.normal[i])
}

#[test]
fn blocks_of_parts_are_put_down_turned_a_door_whole_and_the_use_brush_opens_one() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_parts();
    let (mut editor, field, point, look) = editor_with_blocks(&mut renderer, &dir);
    // A stair rises away from the view: its back the way the view looks, along the ground.
    pick_asset(&mut editor, &mut renderer, "Block", "Stairs.block");
    let at = placed_at(&editor, field, point);
    click_scene(&mut editor, &mut renderer, point);
    let stair = state_at(&editor, field, at).expect("a stair put down");
    assert_eq!(stair.facing, BlockFacing::nearest(look, true), "rising away from the view");
    shot(&mut renderer, "parts-stair.png");
    // R turns the next one on a quarter.
    editor.turn_brush_block();
    let next = placed_at(&editor, field, [point[0] + 60.0, point[1]]);
    click_scene(&mut editor, &mut renderer, [point[0] + 60.0, point[1]]);
    assert_eq!(state_at(&editor, field, next).expect("a second stair").facing, stair.facing.turned(1), "turned on");
    // A door goes down with its upper block over it, and breaking the upper takes both.
    pick_asset(&mut editor, &mut renderer, "Block", "Door.block");
    let door_point = [point[0] - 80.0, point[1]];
    let door = placed_at(&editor, field, door_point);
    editor.turn_brush_block();
    editor.turn_brush_block();
    editor.turn_brush_block();
    click_scene(&mut editor, &mut renderer, door_point);
    let lower = state_at(&editor, field, door).expect("a door put down");
    let upper = state_at(&editor, field, [door[0], door[1] + 1, door[2]]).expect("its upper block");
    assert_eq!((lower.half, upper.half, upper.facing), (BlockHalf::Lower, BlockHalf::Upper, lower.facing));
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "parts-door.png");
    // The Use brush opens it as a player would, and undo shuts it again.
    let brush = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brush's choice");
    choose(&mut editor, &mut renderer, brush, 4);
    click_scene(&mut editor, &mut renderer, door_point);
    assert!(state_at(&editor, field, door).unwrap().open, "opened");
    assert!(state_at(&editor, field, [door[0], door[1] + 1, door[2]]).unwrap().open, "whole");
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.game.engine.world.get::<BlockWorld>(field).unwrap().moving.is_empty(), "it swung and came to rest");
    shot(&mut renderer, "parts-door-open.png");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!state_at(&editor, field, door).unwrap().open, "shut again");
    // Broken from its upper block, the whole door goes.
    choose(&mut editor, &mut renderer, brush, 1);
    let camera = editor.scene_camera().unwrap();
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(field).unwrap().position;
    let top = corner + voxear_core::math::Vec3::new(door[0] as f32 + 0.5, door[1] as f32 + 1.5, door[2] as f32 + 0.5);
    let on_screen = camera.project(top).expect("the door's top on screen");
    click_scene(&mut editor, &mut renderer, on_screen);
    assert_eq!((state_at(&editor, field, door), state_at(&editor, field, [door[0], door[1] + 1, door[2]])), (None, None));
}

#[test]
fn a_blocks_inspector_offers_every_shape_and_a_doors_motion_keys() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_parts();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    assert!(editor.inspect_path("Parts/Plain.block"));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(prop_rows(&editor.ui, "Time").is_empty(), "a cube has no motion");
    let shape = first_with(&editor.ui, prop_rows(&editor.ui, "Shape")[0], "value-choice").expect("its shape's choice");
    click(&mut editor.ui, shape);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let names: Vec<String> = options.iter().map(|&option| row_text(&editor.ui, option).trim().to_string()).collect();
    let texts: Vec<String> = options.iter().map(|&option| editor.ui.text(option)).collect();
    for wanted in ["Cube", "Slab", "Stairs", "Fence", "Wall", "Pane", "Door", "Trap Door", "Fence Gate", "Lever", "Button", "Chain", "Rope", "Boxes"] {
        assert!(names.iter().chain(&texts).any(|name| name == wanted), "{wanted} offered: {names:?} {texts:?}");
    }
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    let shape = first_with(&editor.ui, prop_rows(&editor.ui, "Shape")[0], "value-choice").unwrap();
    choose(&mut editor, &mut renderer, shape, 8);
    frame(&mut editor, &mut renderer);
    let written = std::fs::read_to_string(dir.path().join("Parts/Plain.block")).unwrap();
    assert!(written.contains("shape = \"door\""), "{written}");
    // A door's Use and Motion: its two keys, swinging a quarter turn.
    assert_eq!(prop_rows(&editor.ui, "Time").len(), 2, "a door's motion's keys");
    assert_eq!(prop_rows(&editor.ui, "Turn").len(), 2);
    shot(&mut renderer, "parts-inspector.png");
    // A key changed makes the motion the block's own.
    let turn = prop_rows(&editor.ui, "Turn")[1];
    let field = first_with(&editor.ui, turn, "value-input").expect("the key's turn");
    editor.ui.focus(field);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("100");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let written = std::fs::read_to_string(dir.path().join("Parts/Plain.block")).unwrap();
    let block = voxear_terrain::blocks::block::BlockDef::parse(&written).unwrap();
    assert_eq!(block.motion.expect("its own motion").keys[1].turn, 100.0, "{written}");
}
