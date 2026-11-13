//! Blocks of parts in the editor, on a copy of Hello: the Build tool puts a
//! stair down rising away from the view and R turns the next on a quarter,
//! a door goes down with its upper block and is broken with it, the Use
//! brush opens it and undo shuts it again; and a block's Inspector offers
//! every shape, with a door's motion's keys to change; and the Paint brush
//! keeps a painted block's shape, generated ground's slabs and slabs and
//! stairs put down by hand; and what is put on slab ground stands on the
//! slab's top, half a block down: new Portals, an Obj stood on the ground,
//! and a block built on a slab. Skips itself without a GPU.

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
    // The brush's outline over the ground beside it is a door's panel, not a whole block.
    pointer_to(&mut editor, [door_point[0] + 120.0, door_point[1]]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "parts-door-outline.png");
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
    let brush = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brush's choice");
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
    // Each option's words, wherever in it they are.
    fn words(ui: &Ui, node: NodeId, out: &mut Vec<String>) {
        out.push(ui.text(node));
        ui.children(node).into_iter().for_each(|child| words(ui, child, out));
    }
    let names: Vec<String> = options
        .iter()
        .map(|&option| {
            let mut out = Vec::new();
            words(&editor.ui, option, &mut out);
            out.concat()
        })
        .collect();
    for wanted in ["Cube", "Slab", "Stairs", "Fence", "Wall", "Pane", "Door", "Trap Door", "Fence Gate", "Lever", "Button", "Chain", "Rope", "Boxes"] {
        assert!(names.iter().any(|name| name == wanted), "{wanted} offered: {names:?}");
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

const HILLS: &str = "\n[[obj]]\nid = 932\nname = \"Hills\"\n[obj.block_world]\ndata = \"Parts/Hills.blockworld\"\n";

/// The point on screen the Scene view's ray meets the block `at` of
/// `field` through, at the middle of it, where nothing stands before it.
fn seen(editor: &voxear_editor::Editor, field: Entity, at: [i32; 3]) -> Option<[f32; 2]> {
    let camera = editor.scene_camera()?;
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(field)?.position;
    let middle = corner + voxear_core::math::Vec3::new(at[0] as f32 + 0.5, at[1] as f32 + 0.5, at[2] as f32 + 0.5);
    let point = camera.project(middle)?;
    let [x, y, w, h] = camera.rect;
    if point[0] < x + 20.0 || point[0] > x + w - 20.0 || point[1] < y + 20.0 || point[1] > y + h - 20.0 {
        return None;
    }
    let (origin, direction) = camera.ray(point);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field)?;
    let met =
        voxear_terrain::blocks::raycast(&blocks.blocks, (origin - corner).to_array(), direction.to_array(), 500.0)?;
    (met.block == at).then_some(point)
}

/// Types `text` into the number of the Inspector row labelled `label`.
fn type_into(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str, text: &str) {
    let row = *prop_rows(&editor.ui, label).first().unwrap_or_else(|| panic!("no row says {label}"));
    let input = first_with(&editor.ui, row, "value-input").unwrap_or_else(|| panic!("{label} has a number"));
    editor.ui.focus(input);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input(text);
    frame(editor, renderer);
    editor.ui.blur();
    frame(editor, renderer);
}

/// A copy of Hello with generated hills of grass, their top laid in half
/// blocks, open in the editor with the hills selected and moved so a grass
/// slab in their middle shows `down` of the way down the middle of the
/// Scene view, 14 m off; Hello's own ground drawn no more.
struct Hills {
    dir: tempfile::TempDir,
    editor: voxear_editor::Editor,
    hills: Entity,
    grass_slab: u16,
    blocks: BlockWorld,
    slab: [i32; 3],
    point: [f32; 2],
}

fn on_hills(renderer: &mut Renderer, down: f32) -> Hills {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{
        file::{BlockFile, BlockKind},
        generate::BlockGenerate,
    };
    // Generated hills of grass, their top laid in half blocks.
    let dir = hello_with_parts();
    let write = |path: &str, text: &str| std::fs::write(dir.path().join(path), text).unwrap();
    write("Parts/Grass.block", "color = [0.3, 0.55, 0.2]\n");
    write("Parts/Sand.block", "color = [0.85, 0.75, 0.45]\n");
    write("Parts/Slab.block", "color = [0.6, 0.6, 0.6]\nshape = \"slab\"\n");
    write("Parts/Hills.worldgen", &voxear_terrain::generate::Preset::RollingHills.world().to_text());
    let mut file = BlockFile::default();
    file.blocks[0] = BlockKind::of("Parts/Grass.block");
    file.ground.size = [48, 48];
    file.generate = Some(BlockGenerate {
        world: Some("Parts/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        slabs: true,
        ..BlockGenerate::default()
    });
    write("Parts/Hills.blockworld", &file.to_text());
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    std::fs::write(&day, scene + HILLS).unwrap();
    // Hello's ground is drawn no more, so the hills under where it lies show.
    let environment = dir.path().join("Scenes/Environment.scene");
    let scene = std::fs::read_to_string(&environment).unwrap();
    let plane = scene.find("[obj.renderer]\nmesh = \"builtin:plane\"").expect("Hello's ground");
    let end = scene[plane..].find("\n\n").map_or(scene.len(), |end| plane + end);
    std::fs::write(&environment, format!("{}{}", &scene[..plane], &scene[end..])).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let hills = obj(&editor, "Hills");
    editor.select(Some(hills));
    frame(&mut editor, renderer);
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(hills).expect("generated").clone();
    let grass_slab = asset.file.generate.as_ref().unwrap().halves.as_ref().expect("its slabs numbered")[1];
    // The hills moved so a grass slab in their middle shows where the view is asked to show it.
    let blocks = editor.game.engine.world.get::<BlockWorld>(hills).unwrap().clone();
    let top = |x: i32, z: i32| (-8..120).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0).map(|y| [x, y, z]);
    let mut slabs: Vec<[i32; 3]> = (8..40)
        .flat_map(|z| (8..40).map(move |x| (x, z)))
        .filter_map(|(x, z)| top(x, z))
        .filter(|&at| blocks.blocks.get(at) == grass_slab)
        .collect();
    slabs.sort_by_key(|at| (at[0] - 24).pow(2) + (at[2] - 24).pow(2));
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let (origin, direction) = camera.ray([x + w / 2.0, y + h * down]);
    let mut found = None;
    for &slab in &slabs {
        let middle = voxear_core::math::Vec3::new(slab[0] as f32 + 0.5, slab[1] as f32 + 0.5, slab[2] as f32 + 0.5);
        editor.game.engine.world.get_mut::<voxear_scene::Transform>(hills).unwrap().position =
            origin + direction * 14.0 - middle;
        if let Some(point) = seen(&editor, hills, slab) {
            found = Some((slab, point));
            break;
        }
    }
    let (slab, point) = found.expect("a grass slab in sight");
    frame(&mut editor, renderer);
    frame(&mut editor, renderer);
    Hills { dir, editor, hills, grass_slab, blocks, slab, point }
}

#[test]
fn the_paint_brush_keeps_a_slabs_shape_generated_or_put_down_and_a_stairs() {
    use voxear_engine::blocks::{BlockWorldAsset, block_number_state};
    use voxear_terrain::blocks::block::BlockShape;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let Hills { dir: _dir, mut editor, hills, grass_slab, blocks, slab, point } = on_hills(&mut renderer, 0.7);
    let top = |x: i32, z: i32| (-8..120).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0).map(|y| [x, y, z]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "paint-slabs-before.png");
    // Painted with sand, three blocks across: the slab is sand's slab, lying as it lay; whole blocks are sand.
    pick_asset(&mut editor, &mut renderer, "Block", "Sand.block");
    let brush = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brush's choice");
    choose(&mut editor, &mut renderer, brush, 2);
    type_into(&mut editor, &mut renderer, "Size", "3");
    let sand = editor.game.content.block("Parts/Sand.block").unwrap().0;
    let before: Vec<([i32; 3], u16)> = {
        let blocks = editor.game.engine.world.get::<BlockWorld>(hills).unwrap();
        let mut near = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let at = [slab[0] + dx, slab[1] + dy, slab[2] + dz];
                    near.push((at, blocks.blocks.get(at)));
                }
            }
        }
        near
    };
    click_scene(&mut editor, &mut renderer, point);
    frame(&mut editor, &mut renderer);
    let shape_of = |editor: &voxear_editor::Editor, at: [i32; 3]| {
        let blocks = editor.game.engine.world.get::<BlockWorld>(hills).unwrap();
        let id = blocks.blocks.get(at);
        let asset = editor.game.engine.world.get::<BlockWorldAsset>(hills).unwrap();
        (id, blocks.faces[usize::from(id)].shape, asset.drawn.guid(id))
    };
    let (painted, shape, guid) = shape_of(&editor, slab);
    assert_eq!((shape, guid), (BlockShape::Slab, Some(sand)), "the generated slab is sand's slab");
    assert_eq!(state_at(&editor, hills, slab).unwrap().facing, BlockFacing::Down, "lying low");
    let mut wholes = 0;
    for &(at, was) in &before {
        let (now, shape, guid) = shape_of(&editor, at);
        let was_slab = was == grass_slab;
        assert_eq!(was == 0, now == 0, "painting puts nothing down and takes nothing away at {at:?}");
        if was != 0 && (at[0] - slab[0]).pow(2) + (at[1] - slab[1]).pow(2) + (at[2] - slab[2]).pow(2) <= 2 {
            assert_eq!(guid, Some(sand), "painted at {at:?}");
            assert_eq!(shape == BlockShape::Slab, was_slab, "a slab stays a slab, a whole block whole, at {at:?}");
            wholes += usize::from(!was_slab);
        }
    }
    assert!(wholes > 0, "whole blocks painted too");
    shot(&mut renderer, "paint-slabs-after.png");
    // Undone, the grass slab is back.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(shape_of(&editor, slab).0, grass_slab, "undone");
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(shape_of(&editor, slab).0, painted, "done again");
    // An upright slab facing east and a stair, put down by hand on the hills, painted one block at a time: the slab
    // is sand's slab facing east; the stair, of which sand has no kind, is left a stair.
    let east = BlockState { facing: BlockFacing::East, ..BlockState::default() };
    let world = &mut editor.game.engine.world;
    let upright =
        block_number_state(world, &mut editor.game.content, hills, "Parts/Slab.block", east.to_byte(), true).unwrap();
    let stair = block_number_state(world, &mut editor.game.content, hills, "Parts/Stairs.block", 0, true).unwrap();
    frame(&mut editor, &mut renderer);
    let mut put = Vec::new();
    for (dx, dz) in (-6..=6).flat_map(|dx| (-6..=6).map(move |dz| (dx, dz))) {
        let Some(ground) = top(slab[0] + dx, slab[2] + dz) else { continue };
        let at = [ground[0], ground[1] + 1, ground[2]];
        let block = if put.is_empty() { upright } else { stair };
        if at == [slab[0], slab[1] + 1, slab[2]] || blocks.blocks.get(ground) == grass_slab {
            continue;
        }
        editor.game.engine.world.get_mut::<BlockWorld>(hills).unwrap().set_blocks([(at, block)]);
        match seen(&editor, hills, at) {
            Some(point) => put.push((at, point)),
            None => {
                editor.game.engine.world.get_mut::<BlockWorld>(hills).unwrap().set_blocks([(at, 0)]);
            }
        }
        if put.len() == 2 {
            break;
        }
    }
    let [(upright_at, upright_point), (stair_at, stair_point)] = put[..] else {
        panic!("two blocks in sight: {put:?}")
    };
    type_into(&mut editor, &mut renderer, "Size", "1");
    click_scene(&mut editor, &mut renderer, upright_point);
    click_scene(&mut editor, &mut renderer, stair_point);
    frame(&mut editor, &mut renderer);
    let (_, shape, guid) = shape_of(&editor, upright_at);
    assert_eq!((shape, guid, state_at(&editor, hills, upright_at).unwrap()), (BlockShape::Slab, Some(sand), east));
    assert_eq!(shape_of(&editor, stair_at).0, stair, "the stair is left as it was");
    shot(&mut renderer, "paint-slabs-by-hand.png");
}

/// A call over the editor's link, answered once frames have run it, as a tool makes it.
fn link_call(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, call: &str, args: serde_json::Value) {
    let answer = editor.call(call, args);
    for _ in 0..30 {
        frame(editor, renderer);
        if let Ok(answered) = answer.try_recv() {
            answered.unwrap_or_else(|error| panic!("{call}: {error}"));
            return;
        }
    }
    panic!("{call} was not answered");
}

/// Writes the part of what the renderer last drew round `middle`, `half` pixels each way, four times as large, to
/// `name` in `VOXEAR_TEST_OUTPUT`, when set.
fn close_up(renderer: &mut Renderer, name: &str, middle: [f32; 2], half: u32) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let side = half * 2;
    let left = (middle[0] as u32).saturating_sub(half).min(width - side);
    let top = (middle[1] as u32).saturating_sub(half).min(height - side);
    let mut out = Vec::with_capacity((side * side * 16 * 4) as usize);
    for y in 0..side * 4 {
        for x in 0..side * 4 {
            let at = (((top + y / 4) * width + left + x / 4) * 4) as usize;
            out.extend_from_slice(&pixels[at..at + 4]);
        }
    }
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, side * 4, side * 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&out).unwrap();
}

#[test]
fn what_is_put_on_slab_ground_stands_on_the_slabs_top_half_a_block_down() {
    use voxear_core::math::Vec3;
    use voxear_terrain::blocks::ground_top;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let Hills { dir: _dir, mut editor, hills, grass_slab, blocks, slab, .. } = on_hills(&mut renderer, 0.5);
    // Hello's own Objs, out of the middle of the view.
    for name in ["Spinner A", "Spinner B", "Chrome Ball", "Lamp", "Glow", "Pyramid"] {
        let entity = obj(&editor, name);
        editor.game.engine.world.get_mut::<voxear_scene::Transform>(entity).unwrap().position.y -= 1000.0;
    }
    frame(&mut editor, &mut renderer);
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(hills).unwrap().position;
    let position = |editor: &voxear_editor::Editor, entity| {
        voxear_scene::world_matrix(&editor.game.engine.world, entity).w_axis.truncate()
    };
    // The ground under a point of the world, in metres, as the hills' blocks have it.
    let ground = |at: Vec3| {
        let local = at - corner;
        let cell = [local.x.floor() as i32, (local.y - 1e-3).floor() as i32, local.z.floor() as i32];
        (ground_top(&blocks.blocks, &blocks.faces, cell, (local.x, local.z)).map(|top| corner.y + top), cell)
    };
    // New Portals, made where nothing says, stand on the ground in the middle of the view: on the slab there, half
    // a block down.
    link_call(&mut editor, &mut renderer, "obj.create", serde_json::json!({ "kind": "Portals" }));
    let portals = obj(&editor, "Portals");
    let at = position(&editor, portals);
    let (under, cell) = ground(at);
    assert_eq!(blocks.blocks.get(cell), grass_slab, "the Portals stand over the slab: {cell:?}, slab {slab:?}");
    assert!(under.is_some_and(|under| (at.y - under).abs() < 1e-3), "on its top: {at} over {under:?}");
    assert!((at.y - corner.y - cell[1] as f32 - 0.5).abs() < 1e-3, "half a block down: {at}");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let camera = editor.scene_camera().unwrap();
    let base = camera.project(at + Vec3::new(1.5, 0.0, 0.0)).expect("the frame's foot on screen");
    shot(&mut renderer, "slab-portals.png");
    close_up(&mut renderer, "slab-portals-foot.png", base, 60);
    link_call(&mut editor, &mut renderer, "undo", serde_json::json!({}));
    // A cube made over the slab and stood on the ground stands on its top; moved over a whole block and stood
    // again, on that block's.
    let over = corner + Vec3::new(slab[0] as f32 + 0.5, slab[1] as f32 + 3.0, slab[2] as f32 + 0.5);
    let args = serde_json::json!({ "kind": "Cube", "position": over.to_array(), "on_ground": true, "name": "Stood" });
    link_call(&mut editor, &mut renderer, "obj.create", args);
    let cube = obj(&editor, "Stood");
    let at = position(&editor, cube);
    assert!((at.y - (corner.y + slab[1] as f32 + 0.5)).abs() < 1e-3, "on the slab's top: {at}");
    frame(&mut editor, &mut renderer);
    let on_screen = editor.scene_camera().unwrap().project(at).expect("the cube on screen");
    close_up(&mut renderer, "slab-cube.png", on_screen, 60);
    let whole = (-3..=3)
        .flat_map(|dx| (-3..=3).map(move |dz| [slab[0] + dx, slab[2] + dz]))
        .filter_map(|[x, z]| (-8..120).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0).map(|y| [x, y, z]))
        .find(|&at| blocks.blocks.get(at) != grass_slab)
        .expect("a whole block near");
    let over = corner + Vec3::new(whole[0] as f32 + 0.5, whole[1] as f32 + 3.0, whole[2] as f32 + 0.5);
    let set =
        serde_json::json!({ "obj": { "entity": cube.to_bits() }, "position": over.to_array(), "on_ground": true });
    link_call(&mut editor, &mut renderer, "obj.set", set);
    let at = position(&editor, cube);
    assert!((at.y - (corner.y + whole[1] as f32 + 1.0)).abs() < 1e-3, "on the whole block's top: {at}");
    // A block built on a slab, clicked where its top shows through the empty half of its block, goes on top of
    // it, not beside it.
    let camera = editor.scene_camera().unwrap();
    let eye = camera.position;
    let target = (-6..=6)
        .flat_map(|dx| (-6..=6).map(move |dz| [slab[0] + dx, slab[2] + dz]))
        .filter_map(|[x, z]| (-8..120).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0).map(|y| [x, y, z]))
        .filter(|&at| blocks.blocks.get(at) == grass_slab)
        .find_map(|at| {
            let middle = corner + Vec3::new(at[0] as f32 + 0.5, at[1] as f32 + 0.5, at[2] as f32 + 0.5);
            let away = Vec3::new(middle.x - eye.x, 0.0, middle.z - eye.z).normalize();
            let point = camera.project(middle + away * 0.4)?;
            let (origin, direction) = camera.ray(point);
            let from = (origin - corner).to_array();
            let cells = voxear_terrain::blocks::raycast(&blocks.blocks, from, direction.to_array(), 500.0)?;
            let any = |block: u16| block != 0;
            let shaped = voxear_terrain::blocks::raycast_shaped(
                &blocks.blocks,
                &blocks.faces,
                from,
                direction.to_array(),
                500.0,
                &any,
            )?;
            let sideways = cells.block == at && cells.normal[1] == 0;
            (sideways && shaped.block == at && shaped.normal == [0, 1, 0]).then_some((at, point, cells.normal))
        });
    let (at, point, side) = target.expect("a slab whose top shows through its block's side");
    editor.select(Some(hills));
    frame(&mut editor, &mut renderer);
    pick_asset(&mut editor, &mut renderer, "Block", "Plain.block");
    click_scene(&mut editor, &mut renderer, point);
    let world = editor.game.engine.world.get::<BlockWorld>(hills).unwrap();
    let plain = editor.game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(hills).unwrap().drawn.clone();
    let id = world.blocks.get([at[0], at[1] + 1, at[2]]);
    assert_eq!(plain.guid(id), editor.game.content.db().guid_of("Parts/Plain.block"), "on top of the slab");
    let beside = [at[0] + side[0], at[1], at[2] + side[2]];
    assert_eq!(world.blocks.get(beside), blocks.blocks.get(beside), "nothing put beside it");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let shown = corner + Vec3::new(at[0] as f32 + 0.5, at[1] as f32 + 1.0, at[2] as f32 + 0.5);
    let on_screen = editor.scene_camera().unwrap().project(shown).expect("the block on screen");
    close_up(&mut renderer, "slab-built.png", on_screen, 60);
}
