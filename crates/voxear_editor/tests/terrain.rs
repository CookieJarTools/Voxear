//! A Terrain in the editor, on a copy of Hello: Add Component makes one on
//! a new asset of its own, the Inspector shows and changes it, each change a
//! step to undo, saving writes its asset, and the Scene view places and
//! picks on its ground. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Project, terrain::TerrainAsset};
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_terrain::Terrain;
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// An empty Obj under the Scene view, 256 m back and to the left and half
/// a metre up, so a new terrain's 512 m reach round the middle, over
/// Hello's ground.
const FIELD: &str = "\n[[obj]]\nid = 931\nname = \"Field\"\nposition = [-256.0, 0.5, -256.0]\n";

/// A copy of Hello, inside the repository, with the Obj in its Day scene.
fn hello_with_a_field() -> tempfile::TempDir {
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
    // Two materials for a terrain's surfaces.
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Mud.material"), "color = [0.35, 0.25, 0.15]\n").unwrap();
    std::fs::write(dir.path().join("Materials/Snow.material"), "color = [0.95, 0.95, 0.97]\n").unwrap();
    // No background build of the scripts outliving the test.
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

/// Types into a row's number field, as a user does, and leaves it.
fn type_number(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str, text: &str) {
    let row = prop_rows(&editor.ui, label)[0];
    let field = first_with(&editor.ui, row, "value-input").unwrap_or_else(|| panic!("{label} has a field"));
    editor.ui.focus(field);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input(text);
    frame(editor, renderer);
    editor.ui.blur();
    frame(editor, renderer);
}

/// Picks one of the Terrain's tools by its segment.
fn show_tool(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, tool: &str) {
    let strip = first_with(&editor.ui, editor.ui.root(), "terrain-tools").expect("the Terrain's tools");
    let segment = editor.ui.children(strip).into_iter().find(|&s| editor.ui.text(s) == tool).expect("the tool");
    click(&mut editor.ui, segment);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

/// A copy of Hello with the field, open in the editor with a Terrain added
/// to it, and its Obj.
fn editor_with_a_terrain() -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor, Entity)> {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Terrain");
    Some((dir, renderer, editor, field))
}

/// How high the field's ground is at `(x, z)` from its corner.
fn height(editor: &voxear_editor::Editor, field: Entity, x: f32, z: f32) -> f32 {
    editor.game.engine.world.get::<Terrain>(field).unwrap().heights.height_at(x, z)
}

#[test]
fn a_terrain_is_added_on_an_asset_of_its_own_changed_undone_and_saved() {
    let Some((dir, mut renderer, mut editor, ground)) = editor_with_a_terrain() else { return };
    // Add Component makes flat ground on a new asset named for its Obj; Settings shows the asset and its look.
    show_tool(&mut editor, &mut renderer, "Settings");
    let world = &editor.game.engine.world;
    let terrain = world.get::<Terrain>(ground).expect("a Terrain");
    assert_eq!(terrain.heights.size(), [512.0, 512.0]);
    let asset = world.get::<TerrainAsset>(ground).expect("on an asset").clone();
    assert!(dir.path().join("Terrains/Field.terrain").is_file());
    for label in
        ["Data", "Size", "Resolution", "Base height", "Material", "Texture size", "Detail distance", "Cast shadows"]
    {
        let rows: Vec<String> = prop_rows(&editor.ui, label).into_iter().map(|row| row_text(&editor.ui, row)).collect();
        assert_eq!(rows.len(), 1, "a {label} row: {rows:?}");
    }
    let data = row_text(&editor.ui, prop_rows(&editor.ui, "Data")[0]);
    assert!(data.contains("Field"), "the Data row names its asset: {data}");
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Resolution")[0]).contains("1025"));

    // The Scene view places on its ground, over Hello's own.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let below = [x + w / 2.0, y + h * 0.9];
    let at = voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, below);
    assert!(at.is_some_and(|at| (at.y - 0.5).abs() < 1e-3), "on the ground: {at:?}");

    // Smaller: the ground is made again at its new size; undo puts it back.
    type_number(&mut editor, &mut renderer, "Size", "128");
    let sized =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<Terrain>(ground).unwrap().heights.size();
    assert_eq!(sized(&editor), [128.0, 128.0]);
    assert_eq!(editor.game.engine.world.get::<TerrainAsset>(ground).unwrap().file.size, 128.0);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(sized(&editor), [512.0, 512.0]);
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(sized(&editor), [128.0, 128.0]);

    // Saving writes the asset as the level left it, its heights beside it.
    editor.save();
    let written = std::fs::read_to_string(dir.path().join("Terrains/Field.terrain")).unwrap();
    let file = voxear_terrain::file::TerrainFile::parse(&written).expect("a terrain file");
    assert_eq!(file.size, 128.0);
    let heights = std::fs::read(dir.path().join("Terrains/Field.heights")).expect("its heights");
    assert_eq!(voxear_terrain::file::read_heights(&heights).unwrap().0.size(), [128.0, 128.0]);
    let scene = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    assert!(scene.contains("[obj.terrain]") && scene.contains(&asset.guid.to_string()), "{scene}");
}

#[test]
fn layers_are_added_from_the_menu_switched_off_undone_and_taken_out() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    let layers = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.clone()
    };
    assert!(layers(&editor).is_empty(), "a new terrain is flat ground with no layers");

    // Add Layer offers each kind; a stamp starts in the middle, a hill an eighth of the terrain across.
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let stamp = menu_item(&editor.ui, "Stamp").expect("a Stamp to add");
    click(&mut editor.ui, stamp);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(layers(&editor).len(), 1);
    assert!((height(&editor, field, 256.0, 256.0) - 16.0).abs() < 0.01, "{}", height(&editor, field, 256.0, 256.0));
    shot(&mut renderer, "terrain-layers.png");
    for label in ["Name", "Blend", "Opacity", "Picture", "Height", "Falloff", "Height mask"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the stamp's {label}");
    }
    // Beside the Transform's own.
    for label in ["Position", "Rotation"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 2, "the stamp's {label}");
    }

    // A value changed reshapes it: twice as high.
    let row = prop_rows(&editor.ui, "Height")[0];
    let field_node = first_with(&editor.ui, row, "value-input").unwrap();
    editor.ui.focus(field_node);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("32");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    assert!((height(&editor, field, 256.0, 256.0) - 32.0).abs() < 0.01);

    // The stamp's middle dot drags it over the ground: with the field 200 m further on, its hill stands ahead.
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position.z += 200.0;
    frame(&mut editor, &mut renderer);
    let stamp_at = |editor: &voxear_editor::Editor| match &layers(editor)[0].kind {
        voxear_terrain::layers::LayerKind::Stamp { position, .. } => *position,
        _ => panic!("a stamp"),
    };
    let camera = editor.scene_camera().expect("the Scene view");
    let top = voxear_core::math::Vec3::new(0.0, 0.5 + 32.0, 200.0);
    let dot = camera.project(top).expect("the hill's top in view");
    pointer_to(&mut editor, dot);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "terrain-stamp-handles.png");
    left_button(&mut editor, true);
    frame(&mut editor, &mut renderer);
    pointer_to(&mut editor, [dot[0] + 80.0, dot[1]]);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    let moved = stamp_at(&editor);
    assert!(moved[0] > 260.0, "dragged to the right on the ground: {moved:?}");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(stamp_at(&editor), [256.0, 256.0], "one step to undo");

    // Its box switches it off, and undo on again. A moment later, as edits close together are one step.
    std::thread::sleep(std::time::Duration::from_millis(900));
    let on = first_with(&editor.ui, editor.ui.root(), "terrain-layer-on").expect("its box");
    click(&mut editor.ui, on);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!layers(&editor)[0].enabled);
    assert_eq!(height(&editor, field, 256.0, 256.0), 0.0, "flat with it off");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(layers(&editor)[0].enabled && height(&editor, field, 256.0, 256.0) > 30.0);

    // Its cross takes it out.
    let remove = first_with(&editor.ui, editor.ui.root(), "collection-remove").expect("its cross");
    click(&mut editor.ui, remove);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(layers(&editor).is_empty());
    assert_eq!(height(&editor, field, 256.0, 256.0), 0.0);
}

#[test]
fn a_brush_stroke_in_the_scene_view_raises_the_ground_as_one_step_to_undo() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    show_tool(&mut editor, &mut renderer, "Sculpt");
    for label in ["Brush", "Size", "Strength"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the brush's {label}");
    }
    // Over the ground below the middle of the Scene view, held for a few frames.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.85];
    let at =
        voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, point).expect("ground");
    let local = (at.x + 256.0, at.z + 256.0);
    pointer_to(&mut editor, point);
    frame(&mut editor, &mut renderer);
    left_button(&mut editor, true);
    for _ in 0..6 {
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    shot(&mut renderer, "terrain-sculpt.png");
    left_button(&mut editor, false);
    frame(&mut editor, &mut renderer);
    let raised = height(&editor, field, local.0, local.1);
    assert!(raised > 0.1, "raised under the brush: {raised}");
    let layers = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.clone()
    };
    assert!(matches!(layers(&editor).as_slice(), [layer] if layer.kind.title() == "Sculpt"), "into a new sculpt layer");
    assert!(editor.selected() == Some(field), "the stroke is not a click that selects");

    // One undo takes the stroke back, and the layer it made.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(height(&editor, field, local.0, local.1), 0.0);
    assert!(layers(&editor).is_empty());
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

/// Adds a surface of the material named `name` as a user does: Add
/// Surface, then the material in the picker.
fn add_surface(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Surface");
    click(&mut editor.ui, add);
    frame(editor, renderer);
    frame(editor, renderer);
    let offered = || {
        let list = editor.ui.find(editor.ui.root(), "picker-list");
        let rows = list.map_or(Vec::new(), |list| editor.ui.children(list));
        rows.into_iter().filter(|&r| editor.ui.is_visible(r)).map(|r| row_text(&editor.ui, r)).collect::<Vec<_>>()
    };
    let row = picker_row(&editor.ui, name).unwrap_or_else(|| panic!("{name} is offered: {:?}", offered()));
    click(&mut editor.ui, row);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// Holds the left button over `point` in the Scene view for a few frames.
fn stroke(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, point: [f32; 2], shift: bool) {
    stroke_held(editor, renderer, point, shift, std::time::Duration::ZERO);
}

/// As [`stroke`], held down for `at_least` that long however quickly frames come: a brush that lays its paint by how
/// long it is held lays as much on a fast machine as on a slow one.
fn stroke_held(
    editor: &mut voxear_editor::Editor,
    renderer: &mut Renderer,
    point: [f32; 2],
    shift: bool,
    at_least: std::time::Duration,
) {
    pointer_to(editor, point);
    frame(editor, renderer);
    if shift {
        shift_key(editor, true);
    }
    left_button(editor, true);
    let start = std::time::Instant::now();
    let mut frames = 0;
    while frames < 6 || start.elapsed() < at_least {
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
        frames += 1;
    }
    left_button(editor, false);
    if shift {
        shift_key(editor, false);
    }
    frame(editor, renderer);
}

fn shift_key(editor: &mut voxear_editor::Editor, held: bool) {
    let mut modifiers = winit::keyboard::ModifiersState::empty();
    modifiers.set(winit::keyboard::ModifiersState::SHIFT, held);
    editor.handle(&winit::event::WindowEvent::ModifiersChanged(modifiers.into()));
}

#[test]
fn surfaces_are_added_painted_and_holes_cut_in_the_scene_view_each_a_step_to_undo() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    show_tool(&mut editor, &mut renderer, "Paint");
    add_surface(&mut editor, &mut renderer, "Mud.material");
    add_surface(&mut editor, &mut renderer, "Snow.material");
    let asset = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<TerrainAsset>(field).unwrap().clone();
    let surfaces = asset(&editor).file.surfaces;
    assert_eq!(surfaces.len(), 2);
    assert_eq!(surfaces[0].cover, voxear_terrain::surfaces::Cover::Everywhere, "the first covers the ground");
    for label in ["Size", "Strength", "Material", "Tile size", "Height blend", "Triplanar", "Cover"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the Paint tool's {label}");
    }
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Material")[0]).contains("Snow"), "the new one is picked");
    let names: Vec<String> = {
        let mut found = Vec::new();
        with_class(&editor.ui, editor.ui.root(), "terrain-layer-name", &mut found);
        found.into_iter().map(|node| editor.ui.text(node)).collect()
    };
    assert_eq!(names, ["Snow", "Mud"], "the top one first");
    let look = editor.game.engine.world.get::<voxear_render::TerrainLook>(field).unwrap().clone();
    assert_eq!(look.surfaces.len(), 2, "drawn with both");

    // Snow painted where the pointer holds.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.85];
    let at =
        voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, point).expect("ground");
    let local = (at.x + 256.0, at.z + 256.0);
    let surface = |editor: &voxear_editor::Editor| {
        let terrain = editor.game.engine.world.get::<Terrain>(field).unwrap();
        terrain.weights.as_ref().expect("shares").surface_at(terrain.heights.spacing(), local.0, local.1)
    };
    assert_eq!(surface(&editor), 0, "mud before painting");
    // Held long enough to lay more snow than there is mud, however quickly frames come.
    stroke_held(&mut editor, &mut renderer, point, false, std::time::Duration::from_millis(300));
    shot(&mut renderer, "terrain-paint.png");
    assert_eq!(surface(&editor), 1, "snow painted");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(surface(&editor), 0, "the stroke undone");
    assert_eq!(asset(&editor).file.surfaces.len(), 2, "the surfaces stay");

    // The Holes brush, under Sculpt, cuts the ground away there; Shift fills it again.
    show_tool(&mut editor, &mut renderer, "Sculpt");
    let brushes = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brushes");
    choose(&mut editor, &mut renderer, brushes, 6);
    frame(&mut editor, &mut renderer);
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Brush")[0]), "Holes");
    assert_eq!(prop_rows(&editor.ui, "Strength").len(), 0, "a hole is cut whole");
    stroke(&mut editor, &mut renderer, point, false);
    let cut = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<Terrain>(field).unwrap().heights.cut_at(local.0, local.1)
    };
    assert!(cut(&editor), "a hole under the brush");
    assert!(asset(&editor).file.holes != 0);
    assert!(asset(&editor).file.layers.is_empty(), "with no sculpt layer");
    stroke(&mut editor, &mut renderer, point, true);
    assert!(!cut(&editor), "filled again");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(cut(&editor), "the filling undone");
}

#[test]
fn foliage_paints_plants_onto_the_terrain_and_shift_takes_them_away_each_a_step_to_undo() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    add_component(&mut editor, &mut renderer, "Foliage");
    let strip = first_with(&editor.ui, editor.ui.root(), "foliage-tools").expect("the Foliage's tools");
    let paint = editor.ui.children(strip).into_iter().find(|&s| editor.ui.text(s) == "Paint").expect("Paint");
    click(&mut editor.ui, paint);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for label in ["Density", "Sizes", "Lean with ground", "Steepest ground", "Spacing"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the Paint tool's {label}");
    }
    type_number(&mut editor, &mut renderer, "Density", "40");
    let plants = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<voxear_scene::Plants>(field).unwrap().plants.as_ref().clone()
    };
    let before = plants(&editor).len();
    assert!(before > 100, "Add Component's cubes scattered: {before}");

    // Painted where the pointer holds, on the terrain, which the field's Obj also is.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.85];
    let at =
        voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, point).expect("ground");
    stroke(&mut editor, &mut renderer, point, false);
    let painted = plants(&editor);
    let radius = 8.0;
    let corner = voxear_core::math::Vec3::new(-256.0, 0.5, -256.0);
    let in_brush: Vec<_> = painted
        .iter()
        .map(|plant| voxear_core::math::Vec3::from(plant.position) + corner)
        .filter(|place| (place.x - at.x).hypot(place.z - at.z) <= radius)
        .collect();
    assert!(painted.len() > before && !in_brush.is_empty(), "plants painted: {} then {}", before, painted.len());
    for place in &in_brush {
        assert!((place.y - 0.5).abs() < 0.05, "standing on the terrain: {place}");
    }
    let source = editor.game.engine.world.get::<voxear_engine::scene_file::FoliageSource>(field).unwrap().0.clone();
    assert!(source.scatter.is_none() && source.plants.len() == painted.len(), "written as placed plants");

    // Shift takes those under the brush away; each stroke undoes on its own.
    stroke(&mut editor, &mut renderer, point, true);
    let erased = plants(&editor);
    assert!(erased.len() < painted.len(), "taken away: {} then {}", painted.len(), erased.len());
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(plants(&editor).len(), painted.len(), "the erasing undone");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(plants(&editor).len(), before, "the painting undone");
}

#[test]
fn grass_is_added_from_the_mesh_picker_and_dug_up_in_the_scene_view_each_a_step_to_undo() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    show_tool(&mut editor, &mut renderer, "Grass");
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Grass");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Cube").expect("the cube among the meshes");
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for label in ["Mesh", "Density", "Sizes", "Draw distance", "Grows", "Steepest ground"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the Grass tool's {label}");
    }
    let look = editor.game.engine.world.get::<voxear_render::TerrainLook>(field).unwrap().clone();
    assert!(matches!(&look.grass[..], [Some(_)]), "drawn with the cube");

    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.85];
    let at =
        voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, point).expect("ground");
    let spacing = editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.spacing();
    let sample = (((at.x + 256.0) / spacing).round() as i64, ((at.z + 256.0) / spacing).round() as i64);
    let density = |editor: &voxear_editor::Editor| {
        let terrain = editor.game.engine.world.get::<Terrain>(field).unwrap();
        terrain.grass.as_ref().expect("grass").density_at(0, sample.0, sample.1)
    };
    assert_eq!(density(&editor), 255, "growing everywhere");
    stroke(&mut editor, &mut renderer, point, true);
    // A short stroke at half strength digs up about half.
    assert!(density(&editor) < 200, "dug up under the brush: {}", density(&editor));
    let far = editor.game.engine.world.get::<Terrain>(field).unwrap().grass.as_ref().unwrap().density_at(0, 10, 10);
    assert_eq!(far, 255, "and nowhere else");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(density(&editor), 255, "the digging undone");
}

#[test]
fn the_dig_brush_and_a_trench_layer_dig_into_the_ground_each_a_step_to_undo() {
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    show_tool(&mut editor, &mut renderer, "Sculpt");
    let brushes = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brushes");
    choose(&mut editor, &mut renderer, brushes, 7);
    frame(&mut editor, &mut renderer);
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Brush")[0]), "Dig");
    assert_eq!(prop_rows(&editor.ui, "Bares").len(), 1, "the surface it lays on what it bares");

    // A click digs a ball the brush's size round where the pointer meets the ground: 8 m across by default.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.85];
    let at =
        voxear_editor::scene_tools::surface_at(&editor.game.engine.world, &renderer, &camera, point).expect("ground");
    let local = (at.x + 256.0, at.z + 256.0);
    stroke(&mut editor, &mut renderer, point, false);
    shot(&mut renderer, "terrain-dig.png");
    let floor = |editor: &voxear_editor::Editor, x: f32, z: f32| {
        let terrain = editor.game.engine.world.get::<Terrain>(field).unwrap();
        terrain.dug.as_ref().and_then(|dug| dug.surface_at(&terrain.heights, x, z)).map(|(h, _)| h)
    };
    let dug = floor(&editor, local.0, local.1).expect("dug ground under the brush");
    assert!(dug < -5.0, "a pit under the brush: {dug}");
    let layers = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.clone()
    };
    let one_stroke = |layer: &voxear_terrain::layers::Layer| {
        matches!(&layer.kind, voxear_terrain::layers::LayerKind::Dig { strokes } if strokes.len() == 1)
    };
    assert!(
        matches!(layers(&editor).as_slice(), [layer] if one_stroke(layer)),
        "into a new Dig layer, a stroke: {:?}",
        layers(&editor)
    );

    // Smooth begun on the dug ground evens it out, into the same Dig layer, a step to undo of its own.
    let brushes = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("the brushes");
    choose(&mut editor, &mut renderer, brushes, 1);
    frame(&mut editor, &mut renderer);
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Brush")[0]), "Smooth");
    let dug_ground =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<Terrain>(field).unwrap().dug.clone();
    let before = dug_ground(&editor);
    stroke(&mut editor, &mut renderer, point, false);
    let strokes = |editor: &voxear_editor::Editor| match layers(editor).as_slice() {
        [voxear_terrain::layers::Layer { kind: voxear_terrain::layers::LayerKind::Dig { strokes }, .. }] => {
            strokes.clone()
        }
        _ => Vec::new(),
    };
    let smoothed = strokes(&editor);
    assert!(smoothed.len() == 2 && smoothed[1].smooth > 0.0, "a stroke that smooths: {smoothed:?}");
    assert!(dug_ground(&editor) != before, "the dug ground evened out");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(strokes(&editor).len(), 1, "the smoothing undone");
    assert!(dug_ground(&editor) == before, "back as it was dug");

    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(floor(&editor, local.0, local.1).is_none() && layers(&editor).is_empty(), "the dig undone");

    // A Trench from Add Layer: across the middle, its floor 2 m down.
    show_tool(&mut editor, &mut renderer, "Layers");
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let trench = menu_item(&editor.ui, "Trench").expect("a Trench to add");
    click(&mut editor.ui, trench);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for label in ["Depth", "Wall lean", "Step height", "Step width", "Parapet", "Front", "Surface"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the trench's {label}");
    }
    // Along x, its front on its left, +z: the floor at the back, the fire step at the front.
    let bottom = floor(&editor, 256.0, 255.7).expect("dug along it");
    assert!((bottom + 2.0).abs() < 0.2, "its floor 2 m down: {bottom}");
    let step = floor(&editor, 256.0, 256.3).expect("dug along it");
    assert!((step + 1.4).abs() < 0.2, "its fire step 0.6 m up: {step}");
    assert_eq!(height(&editor, field, 220.0, 300.0), 0.0, "and the ground beside it as it was");
    shot(&mut renderer, "terrain-trench.png");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(floor(&editor, 256.0, 255.7).is_none(), "the trench undone");
}

#[test]
fn a_generate_layer_lays_a_world_generators_ground_and_follows_it_as_it_changes() {
    use voxear_terrain::generate::{Preset, WorldGen};
    use voxear_terrain::layers::LayerKind;
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    let layers = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.clone()
    };
    // Add Layer ▸ Generate, with no generator in the project yet: one made beside the terrain's file.
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let generate = menu_item(&editor.ui, "Generate").expect("a Generate layer to add");
    click(&mut editor.ui, generate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let generating = |layer: &voxear_terrain::layers::Layer| {
        matches!(&layer.kind, LayerKind::Generate { world: Some(_), .. })
    };
    assert!(matches!(layers(&editor).as_slice(), [layer] if generating(layer)));
    for label in ["Generator", "Offset"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the layer's {label}");
    }
    let db = editor.game.content.db();
    let made: Vec<String> = db
        .entries()
        .filter(|entry| entry.kind == voxear_assets::AssetKind::WorldGen)
        .map(|entry| entry.path.clone())
        .collect();
    let [path] = made.as_slice() else { panic!("one generator made: {made:?}") };
    assert!(path.ends_with(".worldgen"));
    // A new generator is the Rolling Hills starter graph, with seed 1.
    let text = std::fs::read_to_string(dir.path().join(path)).unwrap();
    let rolling = WorldGen::parse(&text).unwrap();
    assert_eq!((rolling.starter(), rolling.seed(), rolling.as_landmass()), (Some("Rolling Hills"), 1, None));
    let starters = voxear_terrain::generate::starters();
    assert_eq!(rolling, starters[1].world(1, 1.0));
    // The ground is the generator's, from the corner of its world.
    for (x, z) in [(100.0, 100.0), (300.0, 50.0), (256.0, 400.0)] {
        let made = rolling.height_at(x, z);
        let laid = height(&editor, field, x, z);
        assert!((laid - made).abs() < 1e-2, "at {x}, {z}: {laid} not {made}");
    }
    shot(&mut renderer, "terrain-generate.png");

    // In its Inspector, Alpine picked: that starter's graph in its place, saved, its rows shown, and the
    // terrain's ground made again from it at once.
    assert!(editor.inspect_path(path));
    frame(&mut editor, &mut renderer);
    for label in ["Preset", "Seed", "Scale", "Sea level", "Height", "Gullies", "Temperature", "Humidity"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the generator's {label}");
    }
    let presets = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]), "Rolling Hills");
    choose(&mut editor, &mut renderer, presets, 2);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(dir.path().join(path)).unwrap();
    let alpine = WorldGen::parse(&text).unwrap();
    assert_eq!((alpine.starter(), alpine.seed()), (Some("Alpine"), 1), "saved as it was picked");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]), "Alpine");
    let shown = row_text(&editor.ui, prop_rows(&editor.ui, "Height")[0]);
    assert!(shown.starts_with("900"), "the rows are the Alpine starter's: {shown}");
    let made = alpine.height_at(100.0, 100.0);
    let laid = height(&editor, field, 100.0, 100.0);
    assert!((laid - made).abs() < 1e-2, "made again: {laid} not {made}");
    shot(&mut renderer, "terrain-generate-alpine.png");

    // A generator written before generators were graphs is one landmass: its Preset picks among a landmass's
    // presets, which make the ground they always made.
    std::fs::write(dir.path().join(path), Preset::RollingHills.world().to_text()).unwrap();
    assert!(editor.inspect_path(path));
    frame(&mut editor, &mut renderer);
    for label in ["Preset", "Seed", "Scale", "Sea level", "Height"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the landmass's {label}");
    }
    assert_eq!(prop_rows(&editor.ui, "Gullies").len(), 0);
    let presets = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]), "Rolling Hills");
    choose(&mut editor, &mut renderer, presets, 2);
    frame(&mut editor, &mut renderer);
    let land = WorldGen::parse(&std::fs::read_to_string(dir.path().join(path)).unwrap()).unwrap();
    assert_eq!((land.preset(), land.starter()), (Some(Preset::Alpine), None), "a landmass still");
    assert_eq!(land.hash(), Preset::Alpine.world().hash());
    let (made, laid) = (land.height_at(100.0, 100.0), height(&editor, field, 100.0, 100.0));
    assert!((laid - made).abs() < 1e-2, "made again from the landmass: {laid} not {made}");

    // A generator with a graph of its own shows its seed, scale and sea level, and a row for each value it exposes.
    use voxear_terrain::generate::{Graph, NodeKind, nodes};
    let mut graph = Graph::default();
    let noise = graph.add(NodeKind::Noise(nodes::Noise { size: 300.0, high: 40.0, ..Default::default() }), [0.0, 0.0]);
    let base = graph.add(NodeKind::Constant(nodes::Constant { value: 5.0 }), [0.0, 150.0]);
    let sum = graph.add(NodeKind::Math(nodes::Math::default()), [200.0, 0.0]);
    let output = graph.add(NodeKind::Output(nodes::Output::default()), [400.0, 0.0]);
    assert!(graph.connect(noise, 0, sum, 0) && graph.connect(base, 0, sum, 1) && graph.connect(sum, 0, output, 0));
    assert!(graph.expose(base, 0, "Base"));
    std::fs::write(dir.path().join(path), WorldGen::new(3, 1.0, 0.0, graph).to_text()).unwrap();
    assert!(editor.inspect_path(path));
    frame(&mut editor, &mut renderer);
    for (label, rows) in [("Seed", 1), ("Scale", 1), ("Sea level", 1), ("Base", 1), ("Preset", 0), ("Height", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), rows, "the graph's {label}");
    }
    // Its exposed row typed in: saved into the node it shows, and the terrain made again from the graph.
    type_number(&mut editor, &mut renderer, "Base", "30");
    frame(&mut editor, &mut renderer);
    let hills = WorldGen::parse(&std::fs::read_to_string(dir.path().join(path)).unwrap()).unwrap();
    assert_eq!(hills.graph().exposed_value(0).and_then(|value| value.number()), Some(30.0));
    assert_eq!((hills.seed(), hills.preset()), (3, None));
    let (made, laid) = (hills.height_at(100.0, 100.0), height(&editor, field, 100.0, 100.0));
    assert!((30.0..=70.0).contains(&made) && (laid - made).abs() < 1e-2, "the graph's ground: {laid} not {made}");
}

#[test]
fn an_erosion_layer_wears_the_ground_below_it_and_again_once_its_weather_changes() {
    use voxear_terrain::layers::LayerKind;
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    let add = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, kind: &str| {
        let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
        click(&mut editor.ui, add);
        frame(editor, renderer);
        let item = menu_item(&editor.ui, kind).unwrap_or_else(|| panic!("a {kind} layer to add"));
        click(&mut editor.ui, item);
        frame(editor, renderer);
        frame(editor, renderer);
    };
    // Hills generated to wear down.
    add(&mut editor, &mut renderer, "Generate");
    let ground = |editor: &voxear_editor::Editor| -> Vec<f32> {
        (0..400).map(|i| height(editor, field, 20.0 + (i % 20) as f32 * 24.0, 20.0 + (i / 20) as f32 * 24.0)).collect()
    };
    let generated = ground(&editor);
    let apart = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);

    // An Erosion layer wears it as it is added.
    add(&mut editor, &mut renderer, "Erosion");
    let worn = |editor: &voxear_editor::Editor| {
        match &editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.last().unwrap().kind {
            LayerKind::Erosion { made_from, weather, .. } => (*made_from, weather.rain),
            kind => panic!("an Erosion layer on top, not {kind:?}"),
        }
    };
    assert_ne!(worn(&editor).0, 0, "worn as it was added");
    let eroded = ground(&editor);
    assert!(apart(&generated, &eroded) > 0.01, "the ground worn: {}", apart(&generated, &eroded));
    for label in ["Rain", "Steps", "Wear", "Slump angle"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the layer's {label}");
    }
    let button = |editor: &voxear_editor::Editor| {
        let found = first_with(&editor.ui, editor.ui.root(), "terrain-erode").expect("its Erode button");
        (found, editor.ui.text(found))
    };
    assert_eq!(button(&editor).1, "Erode");
    shot(&mut renderer, "terrain-erosion.png");

    // Its rain changed, it asks to be worn again; pressed, the ground is worn anew.
    type_number(&mut editor, &mut renderer, "Rain", "6");
    frame(&mut editor, &mut renderer);
    assert_eq!(worn(&editor).1, 6.0);
    assert_eq!(button(&editor).1, "Erode Again");
    std::thread::sleep(std::time::Duration::from_millis(900));
    let erode = button(&editor).0;
    click(&mut editor.ui, erode);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(button(&editor).1, "Erode");
    let rained = ground(&editor);
    assert!(apart(&eroded, &rained) > 0.01, "worn again in more rain: {}", apart(&eroded, &rained));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(apart(&ground(&editor), &eroded) < 1e-4, "undone, as it was first worn");
}

#[test]
fn a_rivers_layer_cuts_rivers_where_water_gathers_and_adds_water_for_them_and_their_lakes() {
    use voxear_terrain::layers::LayerKind;
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    // The hills of a landmass, which the counts below were found on, for the Generate layer to pick: a new
    // generator's own hills have their valleys cut already, and take fewer beds.
    let hills = voxear_terrain::generate::Preset::RollingHills.world();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), hills.to_text()).unwrap();
    editor.handle(&winit::event::WindowEvent::Focused(true));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let add = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, kind: &str| {
        let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
        click(&mut editor.ui, add);
        frame(editor, renderer);
        let item = menu_item(&editor.ui, kind).unwrap_or_else(|| panic!("a {kind} layer to add"));
        click(&mut editor.ui, item);
        frame(editor, renderer);
        frame(editor, renderer);
    };
    add(&mut editor, &mut renderer, "Generate");
    let ground = |editor: &voxear_editor::Editor| -> Vec<f32> {
        (0..10_000).map(|i| height(editor, field, 6.0 + (i % 100) as f32 * 5.0, 6.0 + (i / 100) as f32 * 5.0)).collect()
    };
    let generated = ground(&editor);

    // A Rivers layer is cut as it is added; with less ground draining into a river before it starts, more rivers.
    add(&mut editor, &mut renderer, "Rivers");
    let carved_from = |editor: &voxear_editor::Editor| {
        match &editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.layers.last().unwrap().kind {
            LayerKind::Rivers { made_from, settings, .. } => (*made_from, settings.source),
            kind => panic!("a Rivers layer on top, not {kind:?}"),
        }
    };
    assert_ne!(carved_from(&editor).0, 0, "cut as it was added");
    for label in ["Source", "Width", "Depth"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the layer's {label}");
    }
    let button = |editor: &voxear_editor::Editor, class: &str| {
        let found = first_with(&editor.ui, editor.ui.root(), class).unwrap_or_else(|| panic!("its {class} button"));
        (found, editor.ui.text(found))
    };
    assert_eq!(button(&editor, "terrain-erode").1, "Carve");
    assert_eq!(button(&editor, "terrain-water").1, "Add Water");
    type_number(&mut editor, &mut renderer, "Source", "1");
    frame(&mut editor, &mut renderer);
    assert_eq!(carved_from(&editor).1, 1.0);
    assert_eq!(button(&editor, "terrain-erode").1, "Carve Again");
    let carve = button(&editor, "terrain-erode").0;
    click(&mut editor.ui, carve);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(button(&editor, "terrain-erode").1, "Carve");
    let cut = ground(&editor);
    let lowered = generated.iter().zip(&cut).filter(|(a, b)| **b < **a - 0.1).count();
    let raised = generated.iter().zip(&cut).filter(|(a, b)| **b > **a + 1e-3).count();
    assert!(lowered > 20 && raised == 0, "beds cut into the ground, none raised: {lowered} lowered, {raised} raised");
    shot(&mut renderer, "terrain-rivers.png");

    // Add Water: an Obj holding a Stream along each river and a Pool over each lake, one step to undo.
    std::thread::sleep(std::time::Duration::from_millis(900));
    let water = button(&editor, "terrain-water").0;
    click(&mut editor.ui, water);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let streams = world.query_ref::<&voxear_render::Stream>().into_iter().count();
    let pools = world.query_ref::<&voxear_render::Pool>().into_iter().count();
    assert!(streams > 0, "a Stream along each river: {streams}, and {pools} Pools");
    let root = obj(&editor, "Rivers and Lakes");
    let path = world.query_ref::<(&voxear_render::Stream, &voxear_scene::Path, &voxear_scene::Parent)>();
    assert!(path.into_iter().all(|(_, path, parent)| parent.0 == root && path.points.len() >= 2));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.query_ref::<&voxear_render::Stream>().into_iter().count(), 0, "undone");
}

#[test]
fn a_surface_covers_by_its_biome_and_climate_over_a_generated_world() {
    use voxear_terrain::surfaces::{Biome, Cover};
    let Some((_dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let generate = menu_item(&editor.ui, "Generate").expect("a Generate layer");
    click(&mut editor.ui, generate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    show_tool(&mut editor, &mut renderer, "Paint");
    add_surface(&mut editor, &mut renderer, "Mud.material");
    add_surface(&mut editor, &mut renderer, "Snow.material");
    let snow = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.surfaces[1].clone()
    };
    // Covering by rules: its biome and climate show beside its height and slope.
    let cover = first_with(&editor.ui, prop_rows(&editor.ui, "Cover")[0], "value-choice").expect("its cover");
    choose(&mut editor, &mut renderer, cover, Cover::ALL.iter().position(|&c| c == Cover::Rules).unwrap());
    frame(&mut editor, &mut renderer);
    for label in ["Biome", "Temperature", "Humidity"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the surface's {label}");
    }
    // Snow, where it is below freezing: nowhere on hills a hundred metres high in a mild world.
    let biomes = first_with(&editor.ui, prop_rows(&editor.ui, "Biome")[0], "value-choice").expect("its biomes");
    choose(&mut editor, &mut renderer, biomes, 1 + Biome::ALL.iter().position(|&b| b == Biome::Snow).unwrap());
    frame(&mut editor, &mut renderer);
    assert_eq!((snow(&editor).temperature, snow(&editor).humidity), Biome::Snow.ranges());
    let surface_at = |editor: &voxear_editor::Editor, x: f32, z: f32| {
        let terrain = editor.game.engine.world.get::<Terrain>(field).unwrap();
        terrain.weights.as_ref().unwrap().surface_at(terrain.heights.spacing(), x, z)
    };
    let points = [(50.0, 50.0), (250.0, 300.0), (450.0, 120.0)];
    assert!(points.iter().all(|&(x, z)| surface_at(&editor, x, z) == 0), "no snow in a mild world");
    // Its temperatures widened to every mild one: it covers everywhere, and its biome is its own.
    let row = prop_rows(&editor.ui, "Temperatures")[0];
    let mut fields = Vec::new();
    with_class(&editor.ui, row, "axis-input", &mut fields);
    // The highest first, so the range never turns round on the way.
    for (field_node, text) in fields.iter().zip(["0", "40"]).rev() {
        editor.ui.focus(*field_node);
        editor.ui.set_modifiers(Modifiers { shift: false, control: true });
        editor.ui.key_down(Key::Char('a'));
        editor.ui.set_modifiers(Modifiers::default());
        editor.ui.text_input(text);
        frame(&mut editor, &mut renderer);
        editor.ui.blur();
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(snow(&editor).temperature, Some([0.0, 40.0]));
    assert!(points.iter().all(|&(x, z)| surface_at(&editor, x, z) == 1), "covering wherever it is mild");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Biome")[0]), "Custom");
}

#[test]
fn an_endless_terrain_is_made_round_the_scene_view_wherever_it_looks_from() {
    use voxear_scene::{EditorOnly, Transform};
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_terrain() else { return };
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let generate = menu_item(&editor.ui, "Generate").expect("a Generate layer");
    click(&mut editor.ui, generate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    show_tool(&mut editor, &mut renderer, "Settings");
    // Endless: its spacing and view distance in place of its size and resolution.
    let endless = first_with(&editor.ui, prop_rows(&editor.ui, "Endless")[0], "checkbox").expect("Endless");
    click(&mut editor.ui, endless);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let file =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<TerrainAsset>(field).unwrap().file.clone();
    assert!(file(&editor).endless.is_some());
    for (label, shown) in [("Spacing", 1), ("View distance", 1), ("Size", 0), ("Resolution", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "{label}");
    }
    let spacing = first_with(&editor.ui, prop_rows(&editor.ui, "Spacing")[0], "value-choice").expect("its spacings");
    choose(&mut editor, &mut renderer, spacing, 4);
    type_number(&mut editor, &mut renderer, "View distance", "900");
    assert_eq!(file(&editor).endless.map(|e| (e.spacing, e.distance)), Some((4.0, 900.0)));
    // Made round where the Scene view looks from, wherever that is.
    let camera = editor
        .game
        .engine
        .world
        .query_ref::<(voxear_ecs::Entity, &voxear_render::Camera, &EditorOnly)>()
        .into_iter()
        .next()
        .expect("the Scene view's camera")
        .0;
    editor.game.engine.world.get_mut::<Transform>(camera).unwrap().position =
        voxear_core::math::Vec3::new(-6000.0, 200.0, 9000.0);
    for _ in 0..600 {
        frame(&mut editor, &mut renderer);
        if voxear_engine::terrain::streamed(&editor.game.engine.world)
            && voxear_terrain::ground_at(&editor.game.engine.world, -6000.0, 9000.0).is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ground = voxear_terrain::ground_at(&editor.game.engine.world, -6000.0, 9000.0).expect("ground under the view");
    assert_eq!(ground.terrain, field);
    assert_eq!(editor.game.engine.world.get::<Terrain>(field).unwrap().heights.tiles_x(), 3);
    // Only the layers that make ground without end are offered.
    show_tool(&mut editor, &mut renderer, "Layers");
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Noise").is_some() && menu_item(&editor.ui, "Stamp").is_some());
    for kind in ["Sculpt", "Dig", "Erosion", "Rivers", "Import", "Trench"] {
        assert!(menu_item(&editor.ui, kind).is_none(), "{kind} is not offered");
    }
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    // Saved, its file says so, and no heights are kept beside it.
    editor.save();
    let path = editor
        .game
        .content
        .db()
        .get(editor.game.engine.world.get::<TerrainAsset>(field).unwrap().guid)
        .unwrap()
        .path
        .clone();
    let text = std::fs::read_to_string(dir.path().join(&path)).unwrap();
    assert!(voxear_terrain::file::TerrainFile::parse(&text).unwrap().endless.is_some());
    assert!(!dir.path().join(voxear_terrain::file::heights_file(&path)).exists());
    // Undone, a terrain of a set size again, made whole from its Obj: three steps, or fewer where the changes came
    // within the history's merge time of each other, as on a fast machine.
    for _ in 0..3 {
        if file(&editor).endless.is_none() {
            break;
        }
        editor.undo(&mut renderer);
        frame(&mut editor, &mut renderer);
    }
    assert!(file(&editor).endless.is_none());
    let terrain = editor.game.engine.world.get::<Terrain>(field).unwrap();
    assert_eq!(terrain.origin, None);
    assert!(!terrain.heights.has_holes());
}

#[test]
fn a_block_world_is_added_on_an_asset_of_its_own_its_ground_changed_undone_and_saved() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockWorld, file::BlockFile};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    // On a new asset of its own, named for its Obj: grass over dirt over stone, 64 blocks a side and 8 up.
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).expect("its asset").clone();
    let path = editor.game.content.db().get(asset.guid).unwrap().path.clone();
    assert_eq!(path, "Blocks/Field.blockworld");
    // Its grass, dirt and stone are blocks of the project's, made beside it, which its file lists.
    assert!(asset.file.blocks.iter().all(|kind| kind.block.is_some()), "{:?}", asset.file.blocks);
    let names: Vec<&str> = asset.drawn.blocks.iter().map(|kind| kind.name.as_str()).collect();
    assert_eq!(names, ["Grass", "Dirt", "Stone"]);
    for name in names {
        assert!(dir.path().join(format!("Blocks/{name}.block")).is_file(), "{name}'s block");
    }
    let blocks = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!((blocks(&editor).blocks.get([0, 7, 0]), blocks(&editor).blocks.get([63, 0, 63])), (1, 3));
    show_block_tool(&mut editor, &mut renderer, "Settings");
    for label in ["Data", "Block size", "Ground size", "Ground height", "Cast shadows"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the Block World's {label}");
    }
    shot(&mut renderer, "block-world.png");
    // Its ground 3 blocks high: made again at once.
    type_number(&mut editor, &mut renderer, "Ground height", "3");
    assert_eq!((blocks(&editor).blocks.get([0, 2, 0]), blocks(&editor).blocks.get([0, 3, 0])), (1, 0));
    // Undone, 8 again; saved, its file says so.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(blocks(&editor).blocks.get([0, 7, 0]), 1);
    editor.save();
    let text = std::fs::read_to_string(dir.path().join(&path)).unwrap();
    assert_eq!(BlockFile::parse(&text).unwrap().ground.height, 8);
    assert_eq!(text.matches("block = ").count(), 3, "{text}");
    assert!(!text.contains("color"), "how a block looks is in its own file: {text}");
}

/// Picks one of the Block World's tools by its segment.
fn show_block_tool(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, tool: &str) {
    let strip = first_with(&editor.ui, editor.ui.root(), "block-tools").expect("the Block World's tools");
    let segment = editor.ui.children(strip).into_iter().find(|&s| editor.ui.text(s) == tool).expect("the tool");
    click(&mut editor.ui, segment);
    frame(editor, renderer);
    frame(editor, renderer);
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

/// The button saying `text` in `panel`, where it has one showing.
fn button_saying(ui: &Ui, panel: NodeId, text: &str) -> Option<NodeId> {
    let mut buttons = Vec::new();
    with_class(ui, panel, "console-button", &mut buttons);
    with_class(ui, panel, "collection-add", &mut buttons);
    buttons.into_iter().find(|&button| ui.is_visible(button) && ui.text(button) == text)
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

#[test]
fn blocks_are_placed_and_broken_in_the_scene_view_with_any_block_of_the_project_which_the_world_numbers_then() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockWorld, edits::BlockEdits};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    // Two blocks of the project no world holds yet.
    std::fs::create_dir_all(dir.path().join("Ores")).unwrap();
    std::fs::write(dir.path().join("Ores/Gold.block"), "color = [0.9, 0.75, 0.2]\n").unwrap();
    std::fs::write(dir.path().join("Ores/Ruby.block"), "color = [0.8, 0.1, 0.2]\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    frame(&mut editor, &mut renderer);
    // Build shows the brush and the block it builds with, and no list of blocks: nothing to add to, nothing to
    // take out of, and a block's own look is not the world's to show.
    for (label, shown) in [("Brush", 1), ("Size", 1), ("Block", 1), ("Color", 0), ("Material", 0), ("Name", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "Build's {label}");
    }
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    for gone in ["Add Block", "New Block", "Add Color", "Make Block Assets"] {
        assert!(button_saying(&editor.ui, inspector, gone).is_none(), "no {gone}");
    }
    assert!(first_with(&editor.ui, inspector, "terrain-layer").is_none(), "no list of blocks");
    // A new world's brush builds with its grass.
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Block")[0]).contains("Grass"));
    // The world moved under the bottom middle of the Scene view, its top half a metre up, under the camera.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.8];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * ((0.5 - origin.y) / direction.y);
    let corner = voxear_core::math::Vec3::new(hit.x - 32.0, -7.5, hit.z - 32.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position = corner;
    frame(&mut editor, &mut renderer);
    let under = [(hit.x - corner.x).floor() as i32, (hit.z - corner.z).floor() as i32];
    let block = |editor: &voxear_editor::Editor, y: i32| {
        editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.get([under[0], y, under[1]])
    };
    // A click puts grass on the grass under it; Shift and a click takes it away again.
    click_scene(&mut editor, &mut renderer, point);
    assert_eq!(block(&editor, 8), 1, "placed on the face clicked");
    shot(&mut renderer, "blocks-placed.png");
    shift_key(&mut editor, true);
    click_scene(&mut editor, &mut renderer, point);
    shift_key(&mut editor, false);
    assert_eq!(block(&editor, 8), 0, "broken");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(block(&editor, 8), 1, "the break undone, the block back");
    let file = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone()
    };
    let numbered = |editor: &voxear_editor::Editor| file(editor).blocks.len();
    let world_blocks =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.clone();
    let guid = |editor: &voxear_editor::Editor, path: &str| editor.game.content.db().guid_of(path).unwrap();

    // The picker offers every block of the project. Picking one numbers nothing: the world has not held it yet.
    open_picker(&mut editor, &mut renderer, "Block");
    let offered = picker_rows(&editor.ui);
    for name in ["Grass.block", "Dirt.block", "Stone.block", "Gold.block", "Ruby.block"] {
        assert!(offered.iter().any(|row| row == name), "{name} among {offered:?}");
    }
    assert!(!offered.iter().any(|row| row == "None"), "the brush builds with some block: {offered:?}");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    pick_asset(&mut editor, &mut renderer, "Block", "Gold.block");
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Block")[0]).contains("Gold"));
    assert_eq!(numbered(&editor), 3, "picked, not held");
    // Built with, the world numbers it after its three, and keeps that in its file: the block it names is gold.
    let blocks_now = world_blocks(&editor);
    let from = (origin - corner).to_array();
    let met = voxear_terrain::blocks::raycast(&blocks_now, from, direction.to_array(), 500.0).expect("a block");
    let against = [0, 1, 2].map(|i| met.block[i] + met.normal[i]);
    click_scene(&mut editor, &mut renderer, point);
    assert_eq!(world_blocks(&editor).get(against), 4, "gold placed against the face it meets, numbered 4");
    assert_eq!(numbered(&editor), 4);
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    assert_eq!(asset.drawn.guid(4), Some(guid(&editor, "Ores/Gold.block")));
    assert_eq!(asset.drawn.blocks[3].name, "Gold");
    assert!(file(&editor).blocks[3].cover.is_none() && file(&editor).blocks[3].block.is_some());
    // The stroke is one step: undone, the block and its number go together; done again, it is 4 as before.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((numbered(&editor), world_blocks(&editor).get(against)), (3, 0));
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((numbered(&editor), world_blocks(&editor).get(against)), (4, 4));
    // Another block takes the next number; one held already keeps the number it has.
    pick_asset(&mut editor, &mut renderer, "Block", "Ruby.block");
    click_scene(&mut editor, &mut renderer, point);
    assert_eq!(numbered(&editor), 5);
    pick_asset(&mut editor, &mut renderer, "Block", "Gold.block");
    click_scene(&mut editor, &mut renderer, point);
    assert_eq!(numbered(&editor), 5, "gold is held already");
    let placed: Vec<u16> = world_blocks(&editor).chunks().flat_map(|(_, chunk)| chunk.parts().0.to_vec()).collect();
    assert!(placed.contains(&4) && placed.contains(&5), "{placed:?}");
    // The picker offers the blocks built with last first, the most recent first, then the rest.
    open_picker(&mut editor, &mut renderer, "Block");
    let offered = picker_rows(&editor.ui);
    assert_eq!(offered[..3], ["Gold.block", "Ruby.block", "Grass.block"], "{offered:?}");
    assert_eq!(offered[3..], ["Dirt.block", "Stone.block"], "{offered:?}");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    // Saved, its changes from its ground are kept beside it, small, and its file keeps its numbering.
    editor.save();
    let path = editor
        .game
        .content
        .db()
        .get(editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().guid)
        .unwrap()
        .path
        .clone();
    let changes =
        std::fs::read(dir.path().join(voxear_terrain::blocks::edits::blocks_file(&path))).expect("its changes");
    let edits = BlockEdits::from_bytes(&changes).unwrap();
    assert!(!edits.is_empty() && changes.len() < 200, "{} bytes", changes.len());
    let text = std::fs::read_to_string(dir.path().join(&path)).unwrap();
    let read = voxear_terrain::blocks::file::BlockFile::parse(&text).unwrap();
    assert_eq!(read.blocks.len(), 5, "{text}");
    assert_eq!(read.blocks[3].block.as_deref(), Some(guid(&editor, "Ores/Gold.block").to_string().as_str()));
    assert_eq!(read.blocks[4].block.as_deref(), Some(guid(&editor, "Ores/Ruby.block").to_string().as_str()));
}

/// The tab of the docked panel titled `title`, where it shows.
fn dock_tab(ui: &Ui, title: &str) -> Option<NodeId> {
    let mut tabs = Vec::new();
    with_class(ui, ui.root(), "tab", &mut tabs);
    // A tab with an icon has its title in a label beside it.
    let says = |tab: NodeId| ui.text(tab) == title || ui.children(tab).iter().any(|&part| ui.text(part) == title);
    tabs.into_iter().find(|&tab| ui.is_visible(tab) && says(tab))
}

/// The Blocks panel, opened as a user opens it, from the top bar's Window
/// button: its panel.
fn open_blocks_panel(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) -> NodeId {
    let windows = editor.ui.find(editor.ui.root(), "windows").expect("the Window button");
    click(&mut editor.ui, windows);
    frame(editor, renderer);
    let blocks = menu_item(&editor.ui, "Blocks").expect("the Window menu lists Blocks");
    click(&mut editor.ui, blocks);
    frame(editor, renderer);
    frame(editor, renderer);
    assert!(dock_tab(&editor.ui, "Blocks").is_some(), "the Blocks panel is open");
    editor.ui.find(editor.ui.root(), "blocks-panel").expect("the Blocks panel")
}

/// Types `text` into the Blocks panel's search in place of what it says.
fn search_blocks(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, panel: NodeId, text: &str) {
    let search = first_with(&editor.ui, panel, "search-field").expect("its search");
    editor.ui.focus(search);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.key_down(Key::Backspace);
    editor.ui.text_input(text);
    frame(editor, renderer);
    editor.ui.blur();
    frame(editor, renderer);
    frame(editor, renderer);
}

/// Clicks the Blocks panel's tile of the block at `path`.
fn click_block_tile(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, path: &str) {
    let tile = editor.block_database().tile(path).unwrap_or_else(|| panic!("{path} has no tile"));
    click(&mut editor.ui, tile);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn the_blocks_panel_shows_every_block_of_the_project_and_makes_copies_and_deletes_them() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::block::BlockDef;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    // More blocks than a panel has room for at once.
    std::fs::create_dir_all(dir.path().join("Ores")).unwrap();
    for number in 0..300 {
        let shade = number as f32 / 300.0;
        std::fs::write(dir.path().join(format!("Ores/Ore {number:03}.block")), format!("color = [{shade}, 0.5, 0.5]\n"))
            .unwrap();
    }
    let project = Project::open(dir.path()).unwrap();
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    type Editor = voxear_editor::Editor;
    let asset = |editor: &Editor| editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    let read = |name: &str| BlockDef::parse(&std::fs::read_to_string(dir.path().join(name)).unwrap()).unwrap();

    // The panel is one of the Window menu's, closed until asked for.
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let listed = editor.ui.dock_panels(dock);
    assert!(listed.iter().any(|(id, title, open)| id == "blocks" && title == "Blocks" && !open), "{listed:?}");
    let panel = open_blocks_panel(&mut editor, &mut renderer);
    // Every block of the project, by name: the world's grass, dirt and stone among the ores.
    let names = editor.block_database().names();
    assert_eq!(names.len(), 303);
    assert_eq!(names[..3], ["Dirt", "Grass", "Ore 000"], "in the order of their names");
    assert!(names.contains(&"Stone".to_string()));
    // Only the rows on screen are made: far fewer tiles than blocks.
    let mut tiles = Vec::new();
    with_class(&editor.ui, panel, "tile", &mut tiles);
    assert!(!tiles.is_empty() && tiles.len() < 150, "{} tiles made for 303 blocks", tiles.len());
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "blocks-panel.png");
    // Typing narrows it, the best match first.
    search_blocks(&mut editor, &mut renderer, panel, "stone");
    assert_eq!(editor.block_database().names(), ["Stone"]);
    search_blocks(&mut editor, &mut renderer, panel, "ore 01");
    let found = editor.block_database().names();
    let nearest = found[..10].iter().all(|name| name.starts_with("Ore 01"));
    assert!(found.len() < 30 && nearest, "the nearest first: {found:?}");
    search_blocks(&mut editor, &mut renderer, panel, "");
    assert_eq!(editor.block_database().names().len(), 303);

    // A click picks a block, which the Inspector shows as it does one picked in the Project panel.
    click_block_tile(&mut editor, &mut renderer, "Blocks/Grass.block");
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Blocks/Grass.block"));
    assert_eq!(editor.block_database().selected().as_deref(), Some("Blocks/Grass.block"));
    assert_eq!(prop_rows(&editor.ui, "Liquid").len(), 1, "the block's own Inspector");
    // Its colour typed there is written to its file, and the world holding it draws it so.
    let color = first_with(&editor.ui, prop_rows(&editor.ui, "Color")[0], "axis-input").expect("its red");
    editor.ui.focus(color);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("0.25");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(read("Blocks/Grass.block").color[0], 0.25);
    assert_eq!(asset(&editor).drawn.blocks[0].color[0], 0.25, "the world holding it draws it so");

    // A double click has the brush build with the block, and the world being built in is selected again.
    assert_eq!(editor.selected(), None, "the block took the Inspector");
    let dirt = editor.block_database().tile("Blocks/Dirt.block").expect("dirt's tile");
    click(&mut editor.ui, dirt);
    frame(&mut editor, &mut renderer);
    click(&mut editor.ui, dirt);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.selected(), Some(field));
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Block")[0]).contains("Dirt"));

    // New Block makes one in the project's Blocks folder, picked and shown; no world holds it.
    let new = button_saying(&editor.ui, panel, "New Block").expect("New Block");
    click(&mut editor.ui, new);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Blocks/Block.block").is_file());
    assert_eq!(editor.block_database().selected().as_deref(), Some("Blocks/Block.block"));
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Blocks/Block.block"));
    assert_eq!(editor.block_database().names().len(), 304);
    assert_eq!(asset(&editor).file.blocks.len(), 3, "a block made is held by no world");
    // Duplicate copies the one picked beside it, as it is, and picks the copy.
    let made = read("Blocks/Block.block");
    let duplicate = button_saying(&editor.ui, panel, "Duplicate").expect("Duplicate");
    click(&mut editor.ui, duplicate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.block_database().selected().as_deref(), Some("Blocks/Block 1.block"));
    assert_eq!(read("Blocks/Block 1.block"), made);
    assert_eq!(editor.block_database().names().len(), 305);
    // Delete moves a block no world holds to the project's trash at once.
    let delete = button_saying(&editor.ui, panel, "Delete").expect("Delete");
    click(&mut editor.ui, delete);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!dir.path().join("Blocks/Block 1.block").exists());
    assert!(dir.path().join(".voxear/Trash/Block 1.block").is_file());
    assert_eq!((editor.block_database().names().len(), editor.block_database().selected()), (304, None));

    // One a world holds is not deleted at once: the panel says where it is, and asks again.
    search_blocks(&mut editor, &mut renderer, panel, "stone");
    click_block_tile(&mut editor, &mut renderer, "Blocks/Stone.block");
    let delete = button_saying(&editor.ui, panel, "Delete").expect("Delete");
    click(&mut editor.ui, delete);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Blocks/Stone.block").is_file(), "still there");
    assert!(menu_item(&editor.ui, "In Field").is_some(), "it says which world holds it");
    // Let be, it stays.
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Blocks/Stone.block").is_file());
    // Asked again and answered, it goes, and the world's blocks of it are missing, which the Console says.
    click(&mut editor.ui, delete);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let anyway = menu_item(&editor.ui, "Delete Anyway").expect("Delete Anyway");
    click(&mut editor.ui, anyway);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!dir.path().join("Blocks/Stone.block").exists());
    assert_eq!(asset(&editor).file.blocks.len(), 3, "its number is still its own");
    assert_eq!(asset(&editor).drawn.blocks[2].name, "Missing Block");
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let warned = lines.iter().filter(|line| line.contains("Stone was deleted while worlds hold it")).count();
    assert_eq!(warned, 1, "{lines:?}");
}

#[test]
fn an_older_worlds_own_kinds_are_made_blocks_as_it_opens_in_the_editor_and_it_is_drawn_as_it_was() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{
        BlockWorld,
        block::{BlockDef, Liquid},
        file::BlockFile,
    };
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    // As worlds were written while they kept all their kinds themselves: three made by hand, and a colour a model
    // of small blocks brought.
    let older = r#"block_size = 1.0

[[blocks]]
name = "Turf"
color = [0.33, 0.52, 0.2]
material = "Materials/Mud.material"
top = "Materials/Snow.material"

[[blocks]]
name = "Sand"
color = [0.8, 0.7, 0.4]

[[blocks]]
name = "Pond"
color = [0.04, 0.46, 0.33]
water = true

[[blocks]]
name = "Color 7"
color = [0.8, 0.2, 0.2]

[ground]
size = [32, 32]
height = 6
top = 1
under = 2
under_depth = 2
bottom = 4
"#;
    std::fs::create_dir_all(dir.path().join("Voxels")).unwrap();
    std::fs::write(dir.path().join("Voxels/Old.blockworld"), older).unwrap();
    let as_written = BlockFile::parse(older).unwrap();
    let (looks_before, faces_before) = as_written.faces();
    let project = Project::open(dir.path()).unwrap();
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    // The older world's asset picked for the Obj's blocks: it is in the level now.
    show_block_tool(&mut editor, &mut renderer, "Settings");
    pick_asset(&mut editor, &mut renderer, "Data", "Old.blockworld");
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    assert_eq!(editor.game.content.db().get(asset.guid).unwrap().path, "Voxels/Old.blockworld");
    // Its three kinds made by hand are blocks, named after them, in a Blocks folder beside the world.
    let read = |name: &str| BlockDef::parse(&std::fs::read_to_string(dir.path().join(name)).unwrap()).unwrap();
    let turf = read("Voxels/Blocks/Turf.block");
    assert_eq!(turf.color, [0.33, 0.52, 0.2]);
    assert_eq!(turf.material.as_deref(), Some("Materials/Mud.material"));
    assert_eq!(turf.top.as_deref(), Some("Materials/Snow.material"));
    assert_eq!(read("Voxels/Blocks/Sand.block").color, [0.8, 0.7, 0.4]);
    assert_eq!(read("Voxels/Blocks/Pond.block").liquid, Liquid::Water);
    // The world holds those blocks where its kinds were, each by the number it had; the model's colour stays its own.
    let now = &asset.file.blocks;
    assert!(now[..3].iter().all(|kind| kind.block.is_some()) && now[3].is_inline(), "{now:?}");
    assert_eq!(now[3].name, "Color 7");
    for (at, path) in ["Turf", "Sand", "Pond"].into_iter().enumerate() {
        let guid = editor.game.content.db().guid_of(&format!("Voxels/Blocks/{path}.block")).unwrap();
        assert_eq!(asset.drawn.guid((at + 1) as u16), Some(guid), "{path} is number {}", at + 1);
    }
    assert_eq!(asset.file.ground, as_written.ground);
    // It is drawn as it was, and its blocks are what its file made before.
    let (looks, faces) = asset.drawn.faces();
    assert_eq!((looks, faces), (looks_before, faces_before));
    let world = editor.game.engine.world.get::<BlockWorld>(field).unwrap();
    assert_eq!(*world.blocks, as_written.make_ground(None));
    // Its file was written at once, and says so in the Console, once.
    let text = std::fs::read_to_string(dir.path().join("Voxels/Old.blockworld")).unwrap();
    let written = BlockFile::parse(&text).unwrap();
    assert_eq!(written, asset.file, "{text}");
    assert_eq!((text.matches("block = ").count(), text.matches("name = ").count()), (3, 1), "{text}");
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = "Voxels/Old.blockworld: 3 kinds are blocks now, in Voxels/Blocks";
    assert_eq!(lines.iter().filter(|line| line.as_str() == said).count(), 1, "{lines:?}");
    // The Inspector has nothing to make blocks with: it was done.
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    assert!(button_saying(&editor.ui, inspector, "Make Block Assets").is_none());
}

#[test]
fn a_block_that_is_water_is_placed_flows_out_and_a_generated_world_can_fill_its_sea_with_it() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockMap, BlockWorld, flow::Water, is_water};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Water.block"), "color = [0.04, 0.46, 0.33]\nliquid = \"water\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    // The brush builds with the project's water.
    pick_asset(&mut editor, &mut renderer, "Block", "Water.block");
    let file =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone();
    // The world moved under the bottom middle of the Scene view, its top half a metre up, under the camera.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.8];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * ((0.5 - origin.y) / direction.y);
    let corner = voxear_core::math::Vec3::new(hit.x - 32.0, -7.5, hit.z - 32.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position = corner;
    frame(&mut editor, &mut renderer);
    let under = [(hit.x - corner.x).floor() as i32, 8, (hit.z - corner.z).floor() as i32];
    // A click puts water on the grass under it: its own faces, drawn apart, and nothing to stand on.
    click_scene(&mut editor, &mut renderer, point);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!(blocks.blocks.get(under), 4, "water placed on the face clicked, the world's fourth block");
    assert!(is_water(&blocks.faces, 4));
    let mesh = &blocks.meshes[&BlockMap::chunk_of(under).0];
    assert!(mesh.water().1 >= 5, "its top and sides: {} quads", mesh.water().1);
    assert!(mesh.positions.iter().all(|p| p[1] <= 8.0), "nothing to stand on at its top, 9 blocks up");
    shot(&mut renderer, "blocks-water.png");
    // It flows while editing: out over the grass round it, a block a fifth of a second, an eighth lower each.
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    let beside = [under[0] + 1, 8, under[2]];
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!(blocks.blocks.get(beside), 4, "run out beside it");
    assert_eq!(Water::of(blocks.blocks.fill(beside)), Water::Flowing { level: 7, up: 2 });
    assert_eq!(Water::of(blocks.blocks.fill(under)), Water::Source, "what was placed stays full");
    shot(&mut renderer, "blocks-water-flowing.png");
    // Building with ground reaches through the water to the grass under it, and takes the water's place.
    pick_asset(&mut editor, &mut renderer, "Block", "Grass.block");
    click_scene(&mut editor, &mut renderer, point);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!(blocks.blocks.get(under), 1, "grass where the water was");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!(blocks.blocks.get(under), 4, "the water back");
    // Undone, the water it ran out into goes with it, and the number the world gave it.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!((blocks.blocks.get(under), blocks.blocks.get(beside)), (0, 0), "undone");
    assert_eq!(file(&editor).blocks.len(), 3);
    // Generated ground has a sea block to fill its sea with: none, or any block of the project, here its water,
    // which the world numbers as its sea is first made of it.
    show_block_tool(&mut editor, &mut renderer, "Settings");
    assert!(prop_rows(&editor.ui, "Sea block").is_empty(), "flat ground has no sea");
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 1);
    frame(&mut editor, &mut renderer);
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Sea block")[0]).contains("None"));
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Top block")[0]).contains("Grass"));
    open_picker(&mut editor, &mut renderer, "Sea block");
    assert_eq!(picker_rows(&editor.ui)[0], "None", "a sea of air");
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    pick_asset(&mut editor, &mut renderer, "Sea block", "Water.block");
    assert_eq!(file(&editor).generate.expect("generated").sea, 4);
    assert_eq!(file(&editor).blocks.len(), 4);
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Sea block")[0]).contains("Water"));
    // Its ground's top made stone, which it holds already: no new number.
    pick_asset(&mut editor, &mut renderer, "Top block", "Stone.block");
    assert_eq!((file(&editor).ground.top, file(&editor).blocks.len()), (3, 4));
    pick_asset(&mut editor, &mut renderer, "Sea block", "None");
    assert_eq!(file(&editor).generate.expect("generated").sea, 0);
}

#[test]
fn a_block_world_made_of_smaller_blocks_stays_as_large_with_what_was_built_where_it_was() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::BlockWorld;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    // A pillar of stone three blocks high on the grass, built as a script or a brush would.
    let pillar = [[20, 8, 30], [20, 9, 30], [20, 10, 30]];
    editor.game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(pillar.map(|at| (at, 3)));
    frame(&mut editor, &mut renderer);
    // Blocks half the size: the ground as large in metres, of twice as many blocks each way.
    show_block_tool(&mut editor, &mut renderer, "Settings");
    type_number(&mut editor, &mut renderer, "Block size", "0.5");
    let world = &editor.game.engine.world;
    let (file, blocks) =
        (world.get::<BlockWorldAsset>(field).unwrap().file.clone(), world.get::<BlockWorld>(field).unwrap());
    assert_eq!((file.block_size, file.ground.size, file.ground.height), (0.5, [128, 128], 16));
    assert_eq!(blocks.block_size, 0.5);
    // The pillar where it stood, a metre across and three high: two blocks across and six high.
    for (at, kind) in [([40, 16, 60], 3), ([41, 21, 61], 3), ([42, 16, 60], 0), ([40, 22, 60], 0), ([39, 15, 60], 1)] {
        assert_eq!(blocks.blocks.get(at), kind, "at {at:?}");
    }
    assert_eq!(blocks.blocks.get([127, 15, 127]), 1, "grass at its far corner, 64 m out");
    // One step to undo.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap();
    assert_eq!((blocks.block_size, blocks.blocks.get([20, 10, 30]), blocks.blocks.get([40, 16, 60])), (1.0, 3, 0));
}

#[test]
fn a_block_worlds_block_size_dragged_by_its_label_makes_it_again_once_when_let_go() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::BlockWorld;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let made =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.clone();
    let before = made(&editor);
    // Its label dragged left a step a frame: the field follows, the world is not made again yet.
    let row = prop_rows(&editor.ui, "Block size")[0];
    let label = editor.ui.children(row)[0];
    let [left, top, _, height] = editor.ui.rect(label);
    let y = top + height / 2.0;
    editor.ui.pointer_move(left + 20.0, y);
    editor.ui.pointer_button(true);
    for step in 1..=3 {
        editor.ui.pointer_move(left + 20.0 - 5.0 * step as f32, y);
        frame(&mut editor, &mut renderer);
        assert!(std::sync::Arc::ptr_eq(&made(&editor), &before), "made again while dragged, at step {step}");
    }
    let row = prop_rows(&editor.ui, "Block size")[0];
    assert_eq!(row_text(&editor.ui, row), "0.55 m", "the field shows where the drag is");
    assert_eq!(editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.block_size, 1.0);
    // Let go: made again once, of the blocks dragged to, as large in metres.
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let file = &world.get::<BlockWorldAsset>(field).unwrap().file;
    assert!(
        (file.block_size - 0.55).abs() < 1e-6 && world.get::<BlockWorld>(field).unwrap().block_size == file.block_size
    );
    assert_eq!(file.ground.size, [116, 116], "64 m across in blocks of 0.55 m");
    // One step to undo.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.game.engine.world.get::<BlockWorld>(field).unwrap().block_size, 1.0);
}

#[test]
fn a_large_block_world_is_made_again_in_the_background_showing_what_it_was_until_then() {
    use std::{sync::Arc, time::Instant};
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::BlockWorld;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let made = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let size = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.block_size
    };
    let before = made(&editor);
    // Blocks a fifth as large: 320 across, too many to make within a frame. The field shows the new size at once,
    // the world what it was.
    type_number(&mut editor, &mut renderer, "Block size", "0.2");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Block size")[0]), "0.2 m");
    assert!(Arc::ptr_eq(&made(&editor).blocks, &before.blocks), "shown as it was while made");
    assert_eq!(size(&editor), 1.0);
    // Put in once made, as large in metres.
    let start = Instant::now();
    while made(&editor).block_size != 0.2 {
        assert!(start.elapsed().as_secs() < 60, "made within a minute");
        frame(&mut editor, &mut renderer);
    }
    let file = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone();
    assert_eq!((file.block_size, file.ground.size), (0.2, [320, 320]));
    assert_eq!((made(&editor).blocks.get([300, 39, 300]), made(&editor).blocks.get([300, 40, 300])), (1, 0));
    let took = start.elapsed();
    // One step to undo, back to the world it was, and to do again.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(Arc::ptr_eq(&made(&editor).blocks, &before.blocks) && size(&editor) == 1.0);
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((made(&editor).block_size, size(&editor)), (0.2, 0.2));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    // Set back to the size it is while it is made, the change is let go, and the world stays as it was.
    type_number(&mut editor, &mut renderer, "Block size", "0.15");
    type_number(&mut editor, &mut renderer, "Block size", "1");
    let start = Instant::now();
    while start.elapsed() < took * 2 + std::time::Duration::from_secs(1) {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Block size")[0]), "1 m");
    assert!(Arc::ptr_eq(&made(&editor).blocks, &before.blocks) && size(&editor) == 1.0);
}

#[test]
fn a_block_worlds_ground_is_generated_by_a_picked_generator_and_covered_by_a_blocks_biome() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::{
        blocks::BlockWorld,
        generate::{Preset, WorldGen},
    };
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    // A cold world of hills, snow on its tops.
    let mut hills = Preset::RollingHills.landmass();
    hills.climate.temperature = -4.0;
    hills.climate.temperature_range = 1.0;
    let hills = WorldGen::from(hills);
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Cold.worldgen"), hills.to_text()).unwrap();
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Snow.block"), "color = [0.95, 0.95, 0.97]\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    show_block_tool(&mut editor, &mut renderer, "Settings");
    // Generated: its generator's rows in place of the flat ground's height.
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 1);
    frame(&mut editor, &mut renderer);
    for (label, shown) in [("Generator", 1), ("Offset", 1), ("Caves", 1), ("Overhangs", 1), ("Ground height", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "{label}");
    }
    let picker = first_with(&editor.ui, prop_rows(&editor.ui, "Generator")[0], "value-picker").expect("its picker");
    let arrow = first_with(&editor.ui, picker, "picker-arrow").expect("its arrow");
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered = || {
        let list = editor.ui.find(editor.ui.root(), "picker-list");
        let rows = list.map_or(Vec::new(), |list| editor.ui.children(list));
        rows.into_iter().filter(|&r| editor.ui.is_visible(r)).map(|r| row_text(&editor.ui, r)).collect::<Vec<_>>()
    };
    let row =
        picker_row(&editor.ui, "Cold.worldgen").unwrap_or_else(|| panic!("the generator is offered: {:?}", offered()));
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let world = WorldGen::parse(&hills.to_text()).unwrap();
    let blocks = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let top =
        |blocks: &BlockWorld, x: i32, z: i32| (-64..300).rev().find(|&y| blocks.blocks.get([x, y, z]) != 0).unwrap();
    for (x, z) in [(3, 3), (40, 20), (60, 60)] {
        let height = world.height_at(x as f32 + 0.5, z as f32 + 0.5);
        assert!(((top(&blocks(&editor), x, z) + 1) as f32 - height).abs() <= 1.0, "column ({x}, {z}) at {height} m");
    }
    // A block that covers where it is below freezing, as Snow: everywhere in this cold world. The world numbers it
    // as it first covers.
    show_block_tool(&mut editor, &mut renderer, "Build");
    pick_asset(&mut editor, &mut renderer, "Block", "Snow.block");
    assert_eq!(editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.blocks.len(), 3);
    let cover = first_with(&editor.ui, prop_rows(&editor.ui, "Cover")[0], "checkbox").expect("its cover");
    click(&mut editor.ui, cover);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let biome = first_with(&editor.ui, prop_rows(&editor.ui, "Biome")[0], "value-choice").expect("its biomes");
    choose(
        &mut editor,
        &mut renderer,
        biome,
        1 + voxear_terrain::surfaces::Biome::ALL
            .iter()
            .position(|&b| b == voxear_terrain::surfaces::Biome::Snow)
            .unwrap(),
    );
    frame(&mut editor, &mut renderer);
    let file = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone();
    assert!(file.blocks[3].cover.is_some_and(|cover| cover.temperature.is_some()));
    let now = blocks(&editor);
    assert_eq!(now.blocks.get([10, top(&now, 10, 10), 10]), 4, "snow on top");
}

/// A copy of Hello open in the editor, its field a Block World made by `Worlds/Steep.worldgen`, a
/// generator from before graphs with hills steep enough for overhangs to push, and with Overhangs
/// at 9: the project, the renderer, the editor and the field.
fn editor_with_a_steep_block_world() -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor, Entity)> {
    use voxear_terrain::generate::{Preset, WorldGen};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let dir = hello_with_a_field();
    // Steep hills, so overhangs have ground to push.
    let mut hills = Preset::RollingHills.landmass();
    hills.height = 400.0;
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Steep.worldgen"), WorldGen::from(hills).to_text()).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let button = |editor: &voxear_editor::Editor| first_with(&editor.ui, editor.ui.root(), "block-move-to-generator");
    assert!(button(&editor).is_none(), "flat ground has no generator to move anything into");
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 1);
    frame(&mut editor, &mut renderer);
    let picker = first_with(&editor.ui, prop_rows(&editor.ui, "Generator")[0], "value-picker").expect("its picker");
    let arrow = first_with(&editor.ui, picker, "picker-arrow").expect("its arrow");
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let offered = picker_row(&editor.ui, "Steep.worldgen").expect("the generator is offered");
    click(&mut editor.ui, offered);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    type_number(&mut editor, &mut renderer, "Overhangs", "9");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    Some((dir, renderer, editor, field))
}

#[test]
fn a_block_worlds_overhangs_and_caves_move_into_its_generator_as_nodes_and_leave_its_ground_as_it_was() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::{
        blocks::BlockWorld,
        generate::{Value, WorldGen},
    };
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_steep_block_world() else { return };
    let button = |editor: &voxear_editor::Editor| first_with(&editor.ui, editor.ui.root(), "block-move-to-generator");
    let file =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone();
    let blocks =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.clone();
    let generate = file(&editor).generate.expect("generated");
    assert_eq!((generate.overhangs, generate.caves), (9.0, 0.3));
    let before = blocks(&editor);
    // Moved: the numbers are nodes of the generator's graph, shown as its rows, and the world keeps none.
    let move_out = button(&editor).expect("Move to Generator");
    click(&mut editor.ui, move_out);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(dir.path().join("Worlds/Steep.worldgen")).unwrap();
    let world = WorldGen::parse(&text).unwrap();
    assert!(world.volume().is_some() && world.faults().is_empty(), "{text}");
    let rows: Vec<(String, Option<Value>)> = (0..world.graph().exposed.len())
        .map(|row| (world.graph().exposed[row].label.clone(), world.graph().exposed_value(row)))
        .collect();
    let amounts: Vec<(&str, Option<Value>)> =
        rows.iter().skip(3).map(|(label, value)| (label.as_str(), *value)).collect();
    assert_eq!(
        amounts,
        [
            ("Overhangs", Some(Value::Number(9.0))),
            ("Tunnels", Some(Value::Number(0.3))),
            ("Chambers", Some(Value::Number(0.3)))
        ]
    );
    let generate = file(&editor).generate.expect("generated");
    assert_eq!((generate.overhangs, generate.caves), (0.0, 0.0));
    for label in ["Overhangs", "Caves"] {
        assert!(prop_rows(&editor.ui, label).is_empty(), "{label} left the world");
    }
    assert!(button(&editor).is_none(), "nothing left to move");
    assert_eq!(prop_rows(&editor.ui, "Generator").len(), 1);
    // The ground is what it was, block for block.
    assert!(*blocks(&editor) == *before, "the ground moved");
    assert!(before.chunks().count() > 4);
}

#[test]
fn overhangs_moved_into_a_generator_whose_graph_is_open_show_there_and_its_nodes_stay_where_they_were_put() {
    use voxear_terrain::generate::WorldGen;
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_steep_block_world() else { return };
    let path = "Worlds/Steep.worldgen";
    let saved = || WorldGen::parse(&std::fs::read_to_string(dir.path().join(path)).unwrap()).unwrap();
    // A node moved on the canvas makes no other ground, so the level keeps the generator it had read before.
    let world = saved();
    let mut graph = world.graph().clone();
    let land = graph.nodes.iter().find(|node| node.kind.title() == "Landmass").expect("its Landmass").id;
    graph.node_mut(land).unwrap().at = [-640.0, 96.0];
    std::fs::write(dir.path().join(path), world.with_graph(graph).to_text()).unwrap();
    editor.handle(&winit::event::WindowEvent::Focused(true));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    // Its graph is open in the Generator panel while the world's numbers move into it.
    assert!(editor.open_path(path, &mut renderer));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.generator().world().is_some_and(|open| open.volume().is_none()), "open, with no Volume yet");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let move_out = first_with(&editor.ui, editor.ui.root(), "block-move-to-generator").expect("Move to Generator");
    click(&mut editor.ui, move_out);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    // The file has the nodes moved in, and the Landmass where it was put.
    let moved = saved();
    assert!(moved.volume().is_some() && moved.faults().is_empty());
    assert_eq!(moved.graph().node(land).map(|node| node.at), Some([-640.0, 96.0]));
    // The open graph is the file's, so what it saves next keeps the Volume.
    let open = editor.generator().world().expect("the graph is still open");
    assert!(open.volume().is_some(), "the open graph has the nodes moved in");
    assert!(*open == moved, "and is the generator its file has");
    let titles: Vec<&str> = open.graph().nodes.iter().map(|node| node.kind.title()).collect();
    for title in ["Solid", "Overhangs", "Tunnels", "Chambers", "Combine"] {
        assert!(titles.contains(&title), "a {title} node: {titles:?}");
    }
    // When pictures are kept: the whole graph in view, once its previews are as fine as they get.
    if std::env::var("VOXEAR_TEST_OUTPUT").is_err() {
        return;
    }
    let panel = editor.ui.find(editor.ui.root(), "generator-panel").expect("the Generator panel");
    let mut buttons = Vec::new();
    with_class(&editor.ui, panel, "console-button", &mut buttons);
    let named = |ui: &Ui, node: NodeId| {
        ui.text(node) == "Frame" || ui.children(node).iter().any(|&part| ui.text(part) == "Frame")
    };
    let frame_all = buttons.into_iter().find(|&button| named(&editor.ui, button)).expect("the panel's Frame button");
    click(&mut editor.ui, frame_all);
    let waited = std::time::Instant::now();
    while !editor.generator().previews_settled() && waited.elapsed() < std::time::Duration::from_secs(120) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut editor, &mut renderer);
    }
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "generator-voxels-moved-in.png");
}

#[test]
fn a_generated_block_world_made_endless_is_made_round_the_scene_view() {
    use voxear_terrain::{blocks::BlockWorld, generate::Preset};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Hills.worldgen"), Preset::RollingHills.world().to_text()).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 1);
    frame(&mut editor, &mut renderer);
    let picker = first_with(&editor.ui, prop_rows(&editor.ui, "Generator")[0], "value-picker").expect("its picker");
    let arrow = first_with(&editor.ui, picker, "picker-arrow").expect("its arrow");
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Hills.worldgen").expect("the generator");
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let endless = first_with(&editor.ui, prop_rows(&editor.ui, "Endless")[0], "checkbox").expect("Endless");
    click(&mut editor.ui, endless);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for (label, shown) in [("View distance", 1), ("Far distance", 1), ("Ground size", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "{label}");
    }
    // Made round where the Scene view looks from, the field's Obj at (-256, 0.5, -256).
    let camera = editor.scene_camera().expect("the Scene view").position;
    let local = [(camera.x + 256.0).floor() as i32, (camera.z + 256.0).floor() as i32];
    for _ in 0..600 {
        frame(&mut editor, &mut renderer);
        let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap();
        if voxear_engine::blocks::streamed(&editor.game.engine.world) && !blocks.far.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert!((-16..200).any(|y| blocks.blocks.get([local[0], y, local[1]]) != 0), "ground under the view");
    assert!(!blocks.far.is_empty(), "coarser chunks beyond");
}

/// A `.vox` file of one model `size` blocks across, z up, of `blocks`, each
/// a place and a colour, and a palette whose colour 1 is red and colour 2
/// white.
fn vox_file(size: [i32; 3], blocks: &[[u8; 4]]) -> Vec<u8> {
    let chunk = |id: &[u8; 4], content: &[u8], children: &[u8]| {
        let mut bytes = id.to_vec();
        bytes.extend((content.len() as u32).to_le_bytes());
        bytes.extend((children.len() as u32).to_le_bytes());
        bytes.extend(content);
        bytes.extend(children);
        bytes
    };
    let mut xyzi = (blocks.len() as u32).to_le_bytes().to_vec();
    blocks.iter().for_each(|b| xyzi.extend(b));
    let mut palette = vec![255u8, 0, 0, 255, 255, 255, 255, 255];
    palette.resize(256 * 4, 128);
    let mut children = chunk(b"SIZE", &size.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(), &[]);
    children.extend(chunk(b"XYZI", &xyzi, &[]));
    children.extend(chunk(b"RGBA", &palette, &[]));
    let mut bytes = b"VOX ".to_vec();
    bytes.extend(200u32.to_le_bytes());
    bytes.extend(chunk(b"MAIN", &[], &children));
    bytes
}

#[test]
fn a_vox_model_is_built_where_the_model_brush_is_clicked_as_one_step_to_undo() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::BlockWorld;
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    // A tower 2 by 2 and 3 high: two layers of red, a white top.
    let mut tower = Vec::new();
    for z in 0..3u8 {
        for y in 0..2u8 {
            for x in 0..2u8 {
                tower.push([x, y, z, if z < 2 { 1 } else { 2 }]);
            }
        }
    }
    std::fs::create_dir_all(dir.path().join("Models")).unwrap();
    std::fs::write(dir.path().join("Models/Tower.vox"), vox_file([2, 2, 3], &tower)).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    // The world moved under the bottom middle of the Scene view, its top half a metre up, under the camera.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.8];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * ((0.5 - origin.y) / direction.y);
    let corner = voxear_core::math::Vec3::new(hit.x - 32.0, -7.5, hit.z - 32.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position = corner;
    frame(&mut editor, &mut renderer);
    // The Model brush: its model in place of its size.
    let brush = first_with(&editor.ui, prop_rows(&editor.ui, "Brush")[0], "value-choice").expect("its brush");
    choose(&mut editor, &mut renderer, brush, 3);
    frame(&mut editor, &mut renderer);
    assert_eq!((prop_rows(&editor.ui, "Size").len(), prop_rows(&editor.ui, "Model").len()), (0, 1));
    let picker = first_with(&editor.ui, prop_rows(&editor.ui, "Model")[0], "value-picker").expect("its picker");
    let arrow = first_with(&editor.ui, picker, "picker-arrow").expect("its arrow");
    click(&mut editor.ui, arrow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Tower.vox").expect("the model is offered");
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // Built standing on the top it is clicked on, its middle over the block clicked: a kind for each colour.
    let blocks = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let from = (origin - corner).to_array();
    let met = voxear_terrain::blocks::raycast(&blocks(&editor).blocks, from, direction.to_array(), 500.0).unwrap();
    assert_eq!(met.normal, [0, 1, 0], "the ground's top under the click");
    let at = [met.block[0], met.block[1] + 1, met.block[2]];
    click_scene(&mut editor, &mut renderer, point);
    shot(&mut renderer, "blocks-model.png");
    let kinds = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.blocks.clone()
    };
    assert_eq!(kinds(&editor).len(), 5, "red and white added");
    assert_eq!(kinds(&editor)[3].color, [1.0, 0.0, 0.0]);
    let built = blocks(&editor);
    let tower_at = |dx: i32, dy: i32, dz: i32| built.blocks.get([at[0] + dx, at[1] + dy, at[2] + dz]);
    assert_eq!([tower_at(0, 0, 0), tower_at(-1, 1, 1), tower_at(0, 2, 0), tower_at(-1, 2, 1)], [4, 4, 5, 5]);
    assert_eq!([tower_at(1, 0, 0), tower_at(0, 3, 0), tower_at(-2, 0, 0)], [0, 0, 0], "no wider and no higher");
    // The click is one step to undo, its kinds with it.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(kinds(&editor).len(), 3);
    assert_eq!(blocks(&editor).blocks.get(at), 0, "the tower gone");
}

#[test]
fn a_voxel_terrain_is_added_drawn_smooth_and_built_on_in_balls_whose_edges_lie_between_blocks() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockWorld, chunk::AIR};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Voxel Terrain");
    // A Block World drawn smooth, on a new asset in the Voxels folder, its section named for what it is.
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).expect("its asset").clone();
    assert_eq!(editor.game.content.db().get(asset.guid).unwrap().path, "Voxels/Field.blockworld");
    assert!(asset.file.smooth);
    let blocks = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert!(blocks(&editor).smooth);
    assert!(blocks(&editor).meshes.values().all(|mesh| mesh.buckets[1] as usize == mesh.indices.len() * 2));
    fn texts(ui: &Ui, node: NodeId, out: &mut Vec<String>) {
        out.push(ui.text(node));
        ui.children(node).into_iter().for_each(|child| texts(ui, child, out));
    }
    let mut shown = Vec::new();
    texts(&editor.ui, editor.ui.find(editor.ui.root(), "inspector").unwrap(), &mut shown);
    assert!(shown.iter().any(|text| text == "Voxel Terrain"), "its section's title");
    // The world moved under the bottom middle of the Scene view, its top half a metre up, under the camera.
    let camera = editor.scene_camera().expect("the Scene view");
    let [x, y, w, h] = camera.rect;
    let point = [x + w / 2.0, y + h * 0.8];
    let (origin, direction) = camera.ray(point);
    let hit = origin + direction * ((0.5 - origin.y) / direction.y);
    let corner = voxear_core::math::Vec3::new(hit.x - 32.0, -7.5, hit.z - 32.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(field).unwrap().position = corner;
    frame(&mut editor, &mut renderer);
    let from = (origin - corner).to_array();
    let met = voxear_terrain::blocks::raycast(&blocks(&editor).blocks, from, direction.to_array(), 500.0).unwrap();
    assert_eq!(met.normal, [0, 1, 0]);
    let middle = [met.block[0], met.block[1] + 1, met.block[2]];
    // A brush 3 blocks across: a ball of ground on the top clicked, its edge falling between blocks.
    type_number(&mut editor, &mut renderer, "Size", "3");
    click_scene(&mut editor, &mut renderer, point);
    shot(&mut renderer, "voxel-terrain-ball.png");
    let now = blocks(&editor);
    assert_eq!((now.blocks.get(middle), now.blocks.fill(middle)), (1, 255), "its middle wholly ground");
    let above = [middle[0], middle[1] + 2, middle[2]];
    assert_eq!(now.blocks.get(above), AIR);
    assert!((50..80).contains(&now.blocks.fill(above)), "half a block past its edge: {}", now.blocks.fill(above));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((blocks(&editor).blocks.get(middle), blocks(&editor).blocks.fill(above)), (AIR, 0), "undone");
    // Smooth off: a Block World of cubes again, its quads facing six ways.
    show_block_tool(&mut editor, &mut renderer, "Settings");
    assert!(prop_rows(&editor.ui, "Voxels").is_empty(), "a smooth surface has no faces to cut into small cubes");
    let smooth = first_with(&editor.ui, prop_rows(&editor.ui, "Smooth")[0], "checkbox").expect("its Smooth");
    click(&mut editor.ui, smooth);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!blocks(&editor).smooth);
    assert!(blocks(&editor).meshes.values().all(|mesh| mesh.quads.len() * 3 != mesh.indices.len() * 2));
    // Its cubes' faces cut into small cubes: Voxels to a block's side, and then how deep they sink.
    let voxels = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<voxear_render::BlockLook>(field).expect("its look").voxels
    };
    assert_eq!(voxels(&editor).count, 0, "none to begin with");
    assert!(prop_rows(&editor.ui, "Relief").is_empty(), "nothing to sink until there are some");
    let quads: usize = blocks(&editor).meshes.values().map(|mesh| mesh.quads.len()).sum();
    type_number(&mut editor, &mut renderer, "Voxels", "8");
    frame(&mut editor, &mut renderer);
    assert_eq!(voxels(&editor).count, 8);
    type_number(&mut editor, &mut renderer, "Relief", "3");
    frame(&mut editor, &mut renderer);
    assert_eq!(voxels(&editor), voxear_render::BlockVoxels { count: 8, relief: 3.0 });
    let after: usize = blocks(&editor).meshes.values().map(|mesh| mesh.quads.len()).sum();
    assert_eq!(after, quads, "the small cubes are drawn on the faces it has, not built");
    shot(&mut renderer, "block-world-small-cubes.png");
}

#[test]
fn plants_and_what_is_dropped_find_the_ground_of_a_block_world() {
    use voxear_scene::{Transform, WorldTransform};
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    add_component(&mut editor, &mut renderer, "Block World");
    frame(&mut editor, &mut renderer);
    // Its ground is 8 blocks of a metre deep: straight down onto it, from well over it, is its top.
    let world = &editor.game.engine.world;
    let corner = voxear_terrain::corner(world.get::<Transform>(field).unwrap(), world.get::<WorldTransform>(field));
    let over = (corner.x + 40.5, corner.z + 40.5, corner.y + 200.0);
    let (at, facing_up) = voxear_editor::scene_tools::ground_below(world, &renderer, over, &[]).expect("its blocks");
    assert!((at.y - (corner.y + 8.0)).abs() < 1e-3, "on top of its blocks: {at}");
    assert!(facing_up.y > 0.99, "flat ground faces up: {facing_up}");
}

#[test]
fn a_block_world_takes_every_starter_plant_at_once_and_the_starter_trees_of_blocks_from_its_add_plant_row() {
    use voxear_engine::{blocks::BlockWorldAsset, scene_file::FoliageSource};
    use voxear_render::StarterPlant;
    use voxear_terrain::blocks::{BlockWorld, block::BlockDef, block::BlockShape, plant::PlantDef};
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_steep_block_world() else { return };
    reveal(&mut editor, &mut renderer, "Add plant");
    open_picker(&mut editor, &mut renderer, "Add plant");
    let offered = picker_rows(&editor.ui);
    let place = |name: &str| offered.iter().position(|row| row == name);
    // Both sets come first, before the starter plants one by one.
    let first = [place("All Starter Plants"), place("Starter Trees of Blocks"), place("New Grass Tuft")];
    assert_eq!(first, [Some(1), Some(2), Some(3)], "{offered:?}");
    let all = picker_row(&editor.ui, "All Starter Plants").unwrap();
    click(&mut editor.ui, all);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // Every starter plant is a model in the project and a Foliage under the world; the pebbles a second time.
    let grown = |editor: &voxear_editor::Editor, name: &str| {
        let world = &editor.game.engine.world;
        let found = obj(editor, name);
        assert_eq!(world.get::<voxear_scene::Parent>(found).map(|parent| parent.0), Some(field), "{name}");
        world.get::<FoliageSource>(found).and_then(|source| source.0.grow.clone()).expect("where it grows")
    };
    for plant in StarterPlant::ALL {
        assert!(dir.path().join(format!("Plants/{}.boxmodel", plant.name())).is_file(), "{}", plant.name());
        grown(&editor, plant.name());
    }
    assert_eq!(grown(&editor, "Shore Pebbles").height, Some([-1.0, 2.5]));
    // Sticks keep near the three trees and the leaves of the trees of blocks; pebbles near the two stones.
    let sticks = grown(&editor, "Stick");
    let trees = ["Plants/Tree.boxmodel", "Plants/Pine.boxmodel", "Plants/Birch.boxmodel"];
    assert_eq!(sticks.near_plants, trees);
    assert_eq!(sticks.near_blocks.len(), 3, "{:?}", sticks.near_blocks);
    assert!(sticks.near_blocks[0].ends_with("Oak Leaves.block") && !sticks.aligned);
    let pebbles = grown(&editor, "Pebbles");
    assert_eq!((pebbles.near_plants.len(), pebbles.near_blocks.len(), pebbles.within), (2, 0, 5.0));
    assert!(grown(&editor, "Grass Tuft").near_plants.is_empty());
    // Its Grow tool shows what a stick keeps near to.
    editor.select(Some(obj(&editor, "Stick")));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let strip = first_with(&editor.ui, editor.ui.root(), "foliage-tools").expect("the Foliage's tools");
    let grow = editor.ui.children(strip).into_iter().find(|&s| editor.ui.text(s) == "Grow").expect("Grow");
    click(&mut editor.ui, grow);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for row in ["Near plant", "Near block", "Within"] {
        assert_eq!(prop_rows(&editor.ui, row).len(), 1, "{row}");
    }
    // One step to undo, and back again; picked a second time, there is nothing left to add.
    let foliages = |editor: &voxear_editor::Editor| {
        let world = &editor.game.engine.world;
        let grown = world.query_ref::<(&FoliageSource, &Name)>().without::<voxear_scene::Destroying>().into_iter();
        grown.filter(|(source, _)| source.0.grow.is_some()).count()
    };
    assert_eq!(foliages(&editor), StarterPlant::ALL.len() + 1);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(foliages(&editor), 0, "all of them gone in one step");
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(foliages(&editor), StarterPlant::ALL.len() + 1);
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    reveal(&mut editor, &mut renderer, "Add plant");
    open_picker(&mut editor, &mut renderer, "Add plant");
    let all = picker_row(&editor.ui, "All Starter Plants").unwrap();
    click(&mut editor.ui, all);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(foliages(&editor), StarterPlant::ALL.len() + 1);

    // The starter trees of blocks: three plants, a structure each, and blocks for their wood, leaves and vines.
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    reveal(&mut editor, &mut renderer, "Add plant");
    open_picker(&mut editor, &mut renderer, "Add plant");
    let trees = picker_row(&editor.ui, "Starter Trees of Blocks").unwrap();
    click(&mut editor.ui, trees);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let read = |path: &str| std::fs::read_to_string(dir.path().join(path)).unwrap_or_else(|_| panic!("{path} was made"));
    for tree in ["Oak", "Birch", "Pine"] {
        let plant = PlantDef::parse(&read(&format!("Plants/{tree}.voxplant"))).unwrap();
        assert!(plant.wood.is_some() && plant.leaves.is_some(), "{tree}");
        assert_eq!(plant.vine.is_some(), plant.vines > 0.0, "{tree}");
        assert!(read(&format!("Structures/{tree}.structure")).contains("plant = "), "{tree}");
    }
    let world_path = {
        let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap();
        editor.game.content.db().get(asset.guid).unwrap().path.clone()
    };
    let blocks_folder = world_path.rsplit_once('/').map_or("Blocks".to_string(), |(folder, _)| folder.to_string());
    let leaves = BlockDef::parse(&read(&format!("{blocks_folder}/Oak Leaves.block"))).unwrap();
    assert!(leaves.shape == BlockShape::Bushy && leaves.sways);
    assert_eq!(BlockDef::parse(&read(&format!("{blocks_folder}/Vine.block"))).unwrap().shape, BlockShape::Vine);
    // The world lists the structures, numbers their blocks, and its ground is made again with trees on it:
    // blocks of leaves, and their tufts among its chunks' faces.
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    assert_eq!(asset.file.generate.as_ref().map(|generate| generate.structures.len()), Some(3));
    let oak = editor.game.content.parse(&format!("{blocks_folder}/Oak Leaves.block")).unwrap().guid().unwrap();
    let birch = editor.game.content.parse(&format!("{blocks_folder}/Birch Leaves.block")).unwrap().guid().unwrap();
    let numbers = [oak, birch].map(|guid| asset.drawn.number_of(guid));
    assert!(numbers.iter().all(Option::is_some), "the world numbers the trees' leaves: {numbers:?}");
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let leafy = blocks.blocks.chunks().filter(|(_, chunk)| chunk.may_hold(|block| numbers.contains(&Some(block)))).count();
    assert!(leafy > 0, "trees stand on its ground");
    let tufts: u32 = blocks.meshes.values().map(|mesh| mesh.loose().1).sum();
    assert!(tufts > 30, "{tufts} sheets of leaves and vines");
    // Seen from a little over the ground, with the plants grown round the Scene view.
    let top = (0..500).rev().find(|&y| blocks.blocks.get([24, y, 24]) != 0).expect("ground at 24, 24") as f32;
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(field).unwrap().position;
    let camera = editor
        .game
        .engine
        .world
        .query_ref::<(Entity, &voxear_render::Camera, &voxear_scene::EditorOnly)>()
        .into_iter()
        .next()
        .expect("the Scene view's camera")
        .0;
    let look = voxear_core::math::Quat::from_euler(voxear_core::math::EulerRot::YXZ, 0.8, 0.3, 0.0);
    let over = corner + voxear_core::math::Vec3::new(24.0, top + 7.0, 24.0);
    *editor.game.engine.world.get_mut::<voxear_scene::Transform>(camera).unwrap() =
        voxear_scene::Transform::new(over, look, voxear_core::math::Vec3::ONE);
    for _ in 0..90 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "block-world-starter-trees-and-plants.png");
    // Picked again, nothing changes; undone, the world builds none.
    editor.undo(&mut renderer);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap();
    assert_eq!(asset.file.generate.as_ref().map(|generate| generate.structures.len()), Some(0));

    // The starter biomes hang vines where it is wet: blocks of the Vine under the world's ledges and cave roofs.
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let starters = button_saying(&editor.ui, inspector, "Add Starter Biomes").expect("Add Starter Biomes");
    click(&mut editor.ui, starters);
    for _ in 0..8 {
        frame(&mut editor, &mut renderer);
    }
    let vine = editor.game.content.parse(&format!("{blocks_folder}/Vine.block")).unwrap().guid().unwrap();
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    let vine = asset.drawn.number_of(vine).expect("the world numbers its biomes' vine");
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let mut vines: Vec<[i32; 3]> = Vec::new();
    for (key, chunk) in blocks.blocks.chunks().filter(|(_, chunk)| chunk.may_hold(|block| block == vine)) {
        for i in 0..32 * 32 * 32u32 {
            let (x, y, z) = (i % 32, i / 1024, i / 32 % 32);
            if chunk.get(x, y, z) == vine {
                vines.push([key[0] * 32 + x as i32, key[1] * 32 + y as i32, key[2] * 32 + z as i32]);
            }
        }
    }
    assert!(vines.len() > 10, "{} blocks of vine hang in the world", vines.len());
    let sheets: u32 = blocks.meshes.values().map(|mesh| mesh.loose().1).sum();
    assert!(sheets as usize >= vines.len(), "each a sheet or two among its chunk's faces: {sheets}");
    // Seen from a few blocks off through open air, where there is such a place to see one from.
    let open = |at: [i32; 3]| [0, vine].contains(&blocks.blocks.get(at));
    let ways = [[1, 0], [0, 1], [-1, 0], [0, -1], [1, 1], [-1, 1], [1, -1], [-1, -1]];
    let seen_from = vines.iter().find_map(|&[x, y, z]| {
        ways.iter().find_map(|[dx, dz]| {
            let clear = (1..=5).all(|out| (-1..=1).all(|up| open([x + dx * out, y + up, z + dz * out])));
            clear.then_some(([x, y, z], [x + dx * 5, y, z + dz * 5]))
        })
    });
    if let Some((hangs, from)) = seen_from {
        let middle = |at: [i32; 3]| corner + voxear_core::math::Vec3::new(at[0] as f32 + 0.5, at[1] as f32 + 0.5, at[2] as f32 + 0.5);
        let towards = (middle(hangs) - middle(from)).normalize();
        let look = voxear_core::math::Quat::from_rotation_y(towards.x.atan2(towards.z));
        *editor.game.engine.world.get_mut::<voxear_scene::Transform>(camera).unwrap() =
            voxear_scene::Transform::new(middle(from), look, voxear_core::math::Vec3::ONE);
        for _ in 0..30 {
            frame(&mut editor, &mut renderer);
        }
        shot(&mut renderer, "block-world-vines.png");
    }
}

#[test]
fn a_block_world_grows_a_starter_plant_picked_on_its_add_plant_row_as_one_step_to_undo() {
    use voxear_engine::{grown_plants, scene_file::FoliageSource};
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_steep_block_world() else { return };
    // Its Add plant row, the last of its settings, offers the starter plants, new to the project, before the
    // project's models.
    reveal(&mut editor, &mut renderer, "Add plant");
    open_picker(&mut editor, &mut renderer, "Add plant");
    let offered = picker_rows(&editor.ui);
    let has = |name: &str| offered.iter().any(|row| row == name);
    assert!(has("New Grass Tuft") && has("New Tree"), "{offered:?}");
    let tuft = picker_row(&editor.ui, "New Grass Tuft").expect("a tuft of grass to start from");
    click(&mut editor.ui, tuft);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // The plant is a model in the project's Plants folder, and a Foliage under the world that grows it.
    assert!(dir.path().join("Plants/Grass Tuft.boxmodel").is_file());
    let grass = obj(&editor, "Grass Tuft");
    let world = &editor.game.engine.world;
    assert_eq!(world.get::<voxear_scene::Parent>(grass).map(|parent| parent.0), Some(field));
    let grow = world.get::<FoliageSource>(grass).and_then(|source| source.0.grow.clone()).expect("where it grows");
    assert_eq!((grow.spacing, grow.on.len()), (1.0, 1), "thick, on the world's top block");
    assert!(world.get::<voxear_render::Foliage>(grass).is_some(), "drawn with the model's mesh");
    assert_eq!(editor.selected(), Some(grass));
    // Its plants grow only as near as they are drawn: none while the Scene view looks from far off.
    let standing = |editor: &voxear_editor::Editor| grown_plants::standing(&editor.game.engine.world, grass).len();
    for _ in 0..10 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(standing(&editor), 0, "the world is 300 m from the Scene view");
    // The Scene view over the world's ground: they grow over the next ticks, a hidden Obj a chunk column.
    let blocks = editor.game.engine.world.get::<voxear_terrain::blocks::BlockWorld>(field).unwrap().clone();
    let top = (0..500).rev().find(|&y| blocks.blocks.get([20, y, 20]) != 0).expect("ground at 20, 20") as f32;
    let corner = editor.game.engine.world.get::<voxear_scene::Transform>(field).unwrap().position;
    let camera = editor
        .game
        .engine
        .world
        .query_ref::<(Entity, &voxear_render::Camera, &voxear_scene::EditorOnly)>()
        .into_iter()
        .next()
        .expect("the Scene view's camera")
        .0;
    let look = voxear_core::math::Quat::from_euler(voxear_core::math::EulerRot::YXZ, 0.8, 0.45, 0.0);
    let over = corner + voxear_core::math::Vec3::new(20.0, top + 8.0, 20.0);
    *editor.game.engine.world.get_mut::<voxear_scene::Transform>(camera).unwrap() =
        voxear_scene::Transform::new(over, look, voxear_core::math::Vec3::ONE);
    for _ in 0..60 {
        frame(&mut editor, &mut renderer);
    }
    let grown = standing(&editor);
    assert!(grown > 300, "{grown} tufts on 64 by 64 blocks");
    assert!(grown_plants::settled(&editor.game.engine.world));
    assert!(obj_named(&editor, "Grass Tuft") == 1, "the columns' Objs have no names to show in the Hierarchy");
    shot(&mut renderer, "block-world-grown-tufts.png");
    // A click on a grown plant picks its Foliage, never the Obj that holds its column; between plants, the world.
    let view = editor.scene_camera().expect("the Scene view's camera");
    let [left, top_edge, wide, tall] = view.rect;
    let points = (1..12).flat_map(|row| (1..20).map(move |column| (column as f32 / 20.0, row as f32 / 12.0)));
    let picked: Vec<Entity> = points
        .filter_map(|(across, down)| {
            let point = [left + wide * across, top_edge + tall * down];
            let markers = voxear_editor::scene_tools::Markers::NONE;
            voxear_editor::scene_tools::pick(&editor.game.engine.world, &renderer, &view, point, markers)
        })
        .collect();
    assert!(picked.iter().filter(|&&obj| obj == grass).count() > 10, "clicks on tufts pick the Foliage");
    assert!(picked.iter().any(|&obj| obj == field), "and between them, the world");
    assert!(picked.iter().all(|obj| editor.game.engine.world.has::<Name>(*obj)), "never a column's own Obj");
    // Its Inspector opens on Grow, where it grows: no chance, none; undone, as many as before.
    assert_eq!(prop_rows(&editor.ui, "Chance").len(), 1, "the Grow tool's rows");
    type_number(&mut editor, &mut renderer, "Chance", "0");
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(standing(&editor), 0);
    editor.undo(&mut renderer);
    for _ in 0..60 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(standing(&editor), grown, "the same plants again");
    // Picked again, the tuft is one of the project's models now, and grows a second Foliage; undone, gone with
    // its plants.
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    reveal(&mut editor, &mut renderer, "Add plant");
    open_picker(&mut editor, &mut renderer, "Add plant");
    editor.ui.text_input("Grass Tuft");
    frame(&mut editor, &mut renderer);
    assert!(!picker_rows(&editor.ui).iter().any(|row| row == "New Grass Tuft"));
    let model = picker_row(&editor.ui, "Grass Tuft.boxmodel").expect("the model it made");
    click(&mut editor.ui, model);
    for _ in 0..30 {
        frame(&mut editor, &mut renderer);
    }
    let holders = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.query_ref::<&grown_plants::GrownColumn>().into_iter().count()
    };
    let (both, with_both) = (obj_named(&editor, "Grass Tuft") + obj_named(&editor, "Grass Tuft (1)"), holders(&editor));
    assert_eq!(both, 2, "a second Foliage");
    editor.undo(&mut renderer);
    for _ in 0..5 {
        frame(&mut editor, &mut renderer);
    }
    assert!(holders(&editor) < with_both && holders(&editor) > 0, "the second's plants went with it");
}

/// Scrolls the Inspector until its row labelled `label` shows, as the wheel would.
fn reveal(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str) {
    let row = *prop_rows(&editor.ui, label).first().unwrap_or_else(|| panic!("no row says {label}"));
    let mut at = editor.ui.parent(row);
    while let Some(node) = at {
        let most = editor.ui.scroll_max(node)[1];
        if most > 0.0 {
            let ([_, top, _, tall], [_, row_top, _, row_tall]) = (editor.ui.rect(node), editor.ui.rect(row));
            let below = (row_top + row_tall) - (top + tall) + 8.0;
            let now = editor.ui.scroll_offset(node);
            editor.ui.set_scroll(node, [now[0], (now[1] + below.max(0.0)).min(most)]);
            frame(editor, renderer);
            frame(editor, renderer);
        }
        at = editor.ui.parent(node);
    }
}

/// How many Objs are named `name`.
fn obj_named(editor: &voxear_editor::Editor, name: &str) -> usize {
    let world = &editor.game.engine.world;
    let objs = world.query_ref::<(Entity, &Name)>().without::<voxear_scene::Destroying>();
    objs.into_iter().filter(|(_, n)| n.0 == name).count()
}

/// A copy of Hello whose field is a world of blocks 48 blocks a side, generated by rolling hills, with moss and
/// sand among its blocks and two biomes that between them take in every climate: Cool, of moss, and Warm, of sand.
/// The world lists `listed` of them. Open in the editor, its Obj selected and its Settings showing.
fn editor_with_a_generated_world(
    listed: &[&str],
) -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor, Entity)> {
    use voxear_terrain::{
        blocks::{
            file::{BlockFile, BlockKind},
            generate::BlockGenerate,
        },
        generate::Preset,
    };
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let dir = hello_with_a_field();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let hills = Preset::RollingHills.world();
    write("Worlds/Hills.worldgen", &hills.to_text());
    for (name, color) in [("Grass", "0.33, 0.52, 0.2"), ("Dirt", "0.45, 0.32, 0.2"), ("Stone", "0.5, 0.5, 0.52")] {
        write(&format!("Blocks/{name}.block"), &format!("color = [{color}]\n"));
    }
    write("Blocks/Moss.block", "color = [0.2, 0.5, 0.2]\n");
    write("Blocks/Sand.block", "color = [0.9, 0.8, 0.5]\n");
    // As warm as the middle of the world is the border between them.
    let split = hills.climate(24.0, 24.0, hills.height_at(24.0, 24.0)).0;
    let cool =
        format!("temperature = [-60.0, {split}]\nhumidity = [0.0, 1.0]\n\n[ground]\ntop = \"Blocks/Moss.block\"\n");
    let warm =
        format!("temperature = [{split}, 60.0]\nhumidity = [0.0, 1.0]\n\n[ground]\ntop = \"Blocks/Sand.block\"\n");
    write("Biomes/Cool.biome", &cool);
    write("Biomes/Warm.biome", &warm);
    let mut file = BlockFile::default();
    file.blocks = ["Grass", "Dirt", "Stone"].map(|name| BlockKind::of(&format!("Blocks/{name}.block"))).to_vec();
    file.ground.size = [48, 48];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        biomes: listed.iter().map(|name| format!("Biomes/{name}.biome")).collect(),
        ..BlockGenerate::default()
    });
    write("Blocks/Field.blockworld", &file.to_text());
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    std::fs::write(&day, scene + "[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    Some((dir, renderer, editor, field))
}

/// Writes a picture of the editor with its Inspector scrolled `down`, and puts it back.
fn shot_scrolled(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str, down: f32) {
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    editor.ui.set_scroll(inspector, [0.0, down]);
    frame(editor, renderer);
    frame(editor, renderer);
    shot(renderer, name);
    editor.ui.set_scroll(inspector, [0.0, 0.0]);
    frame(editor, renderer);
}

/// The names its list of biomes shows, in its order.
fn listed_biomes(ui: &Ui) -> Vec<String> {
    let Some(list) = first_with(ui, ui.root(), "block-biomes") else { return Vec::new() };
    let mut names = Vec::new();
    with_class(ui, list, "terrain-layer-name", &mut names);
    names.into_iter().filter(|&name| ui.is_visible(name)).map(|name| ui.text(name)).collect()
}

/// How many of the 48 by 48 columns of `field`'s world have each block on top, by the block's number.
fn tops(editor: &voxear_editor::Editor, field: Entity) -> std::collections::BTreeMap<u16, usize> {
    let blocks = editor.game.engine.world.get::<voxear_terrain::blocks::BlockWorld>(field).unwrap();
    let mut tops = std::collections::BTreeMap::new();
    for z in 0..48 {
        for x in 0..48 {
            let top = (-8..300).rev().map(|y| blocks.blocks.get([x, y, z])).find(|&block| block != 0).unwrap_or(0);
            *tops.entry(top).or_default() += 1;
        }
    }
    tops
}

/// Types `words` into the command palette and runs what it finds first.
fn run_command(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, words: &str) {
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input(words);
    frame(editor, renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn a_generated_worlds_biomes_are_listed_added_moved_and_taken_out_each_making_its_ground_again() {
    use voxear_engine::blocks::BlockWorldAsset;
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_generated_world(&[]) else { return };
    let file = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone()
    };
    let number = |editor: &voxear_editor::Editor, path: &str| {
        let guid = editor.game.content.db().guid_of(path).unwrap();
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().drawn.number_of(guid)
    };
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    // Under its generator's rows, the biomes its ground has: none yet, so its own grass everywhere.
    assert_eq!(prop_rows(&editor.ui, "Biomes").len(), 1);
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Biomes")[0]).contains('0'));
    let labels: Vec<String> = {
        let mut rows = Vec::new();
        with_class(&editor.ui, inspector, "prop", &mut rows);
        let shown = rows.into_iter().filter(|&row| editor.ui.is_visible(row));
        shown.filter_map(|row| editor.ui.children(row).first().map(|&label| editor.ui.text(label))).collect()
    };
    let at = |label: &str| labels.iter().position(|shown| shown == label).unwrap_or_else(|| panic!("{labels:?}"));
    assert!(at("Generator") < at("Biomes") && at("Biomes") < at("Top block"), "{labels:?}");
    assert!(listed_biomes(&editor.ui).is_empty());
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [1]);
    for button in ["Add Biome", "Add Starter Biomes"] {
        assert!(button_saying(&editor.ui, inspector, button).is_some(), "{button}");
    }

    // Add Biome offers the project's biomes and nothing else. One picked is listed, and the ground made again from
    // it: with one biome every place is that one, so moss everywhere, numbered as the world first holds it.
    let add = button_saying(&editor.ui, inspector, "Add Biome").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(picker_rows(&editor.ui), ["Cool.biome", "Warm.biome"]);
    let row = picker_row(&editor.ui, "Cool.biome").unwrap();
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool"]);
    assert_eq!(file(&editor).generate.unwrap().biomes.len(), 1);
    let moss = number(&editor, "Blocks/Moss.block").expect("the world holds moss now");
    assert_eq!((moss, file(&editor).blocks.len()), (4, 4));
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [moss]);
    // A second: the warm places are sand, the cool ones moss still.
    let add = button_saying(&editor.ui, inspector, "Add Biome").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Warm.biome").unwrap();
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool", "Warm"]);
    let sand = number(&editor, "Blocks/Sand.block").expect("and sand");
    let both = tops(&editor, field);
    assert!(both[&moss] > 100 && both[&sand] > 100 && both.len() == 2, "{both:?}");
    shot_scrolled(&mut editor, &mut renderer, "biomes-list.png", 420.0);
    // One listed already is not listed twice.
    let add = button_saying(&editor.ui, inspector, "Add Biome").unwrap();
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Warm.biome").unwrap();
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool", "Warm"]);

    // Each change is one step: undone, the warm biome is off the list and the ground all moss again.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool"]);
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [moss]);
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool", "Warm"]);
    assert_eq!(tops(&editor, field), both);

    // A world that lists biomes takes its ground's blocks from them: Build shows no Cover to set for its block.
    show_block_tool(&mut editor, &mut renderer, "Build");
    pick_asset(&mut editor, &mut renderer, "Block", "Sand.block");
    assert!(prop_rows(&editor.ui, "Block").len() == 1 && prop_rows(&editor.ui, "Cover").is_empty());
    show_block_tool(&mut editor, &mut renderer, "Settings");

    // Dragged by its grip above the first, the second goes first; the ground is what it was, as no place is as
    // much within both.
    let list = first_with(&editor.ui, editor.ui.root(), "block-biomes").unwrap();
    let items = first_with(&editor.ui, list, "collection-items").unwrap();
    let rows = editor.ui.children(items);
    let grip = first_with(&editor.ui, rows[1], "collection-grip").unwrap();
    let [x, y, w, h] = editor.ui.rect(grip);
    let (from, top) = ([x + w / 2.0, y + h / 2.0], editor.ui.rect(rows[0])[1]);
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=4 {
        editor.ui.pointer_move(from[0], from[1] + (top - from[1]) * step as f32 / 4.0);
        frame(&mut editor, &mut renderer);
    }
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Warm", "Cool"]);
    assert_eq!(tops(&editor, field), both);

    // Saved, its file lists them by reference, in its order, and keeps the numbers it gave their blocks.
    editor.save();
    let text = std::fs::read_to_string(dir.path().join("Blocks/Field.blockworld")).unwrap();
    let read = voxear_terrain::blocks::file::BlockFile::parse(&text).unwrap();
    let guid = |path: &str| editor.game.content.db().guid_of(path).unwrap().to_string();
    assert_eq!(read.generate.unwrap().biomes, [guid("Biomes/Warm.biome"), guid("Biomes/Cool.biome")], "{text}");
    assert_eq!(read.blocks.len(), 5);

    // Its cross takes a biome out: with the cool one alone, moss everywhere again. The last out, its own grass.
    let cross = |editor: &voxear_editor::Editor, at: usize| {
        let list = first_with(&editor.ui, editor.ui.root(), "block-biomes").unwrap();
        let mut crosses = Vec::new();
        with_class(&editor.ui, list, "collection-remove", &mut crosses);
        crosses[at]
    };
    let warm = cross(&editor, 0);
    click(&mut editor.ui, warm);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(listed_biomes(&editor.ui), ["Cool"]);
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [moss]);
    let cool = cross(&editor, 0);
    click(&mut editor.ui, cool);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(listed_biomes(&editor.ui).is_empty());
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(file(&editor).blocks.len(), 5, "the blocks they brought keep their numbers");
    // With none listed, a block's Cover is the world's to set again.
    show_block_tool(&mut editor, &mut renderer, "Build");
    assert_eq!(prop_rows(&editor.ui, "Cover").len(), 1);
}

#[test]
fn a_biome_in_the_inspector_is_saved_as_it_is_edited_and_every_world_listing_it_is_made_again() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::generate::BiomeDef;
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_generated_world(&["Cool"]) else { return };
    let read = || BiomeDef::parse(&std::fs::read_to_string(dir.path().join("Biomes/Cool.biome")).unwrap()).unwrap();
    let number = |editor: &voxear_editor::Editor, path: &str| {
        let guid = editor.game.content.db().guid_of(path).unwrap();
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().drawn.number_of(guid)
    };
    // The world lists Cool as it loads: moss everywhere, its fourth block.
    let moss = number(&editor, "Blocks/Moss.block").expect("moss, numbered as the world loaded");
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [moss]);
    // Something built in it, to be kept as its ground is made again: grass at its lowest block, deep in its stone.
    let built = [3, -8, 3];
    editor.game.engine.world.get_mut::<voxear_terrain::blocks::BlockWorld>(field).unwrap().set_blocks([(built, 1)]);

    // The biome picked in the Project panel shows in the Inspector: its colour, its climate and its ground.
    assert!(editor.inspect_path("Biomes/Cool.biome"));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let labels = ["Color", "Preset", "Temperature", "Humidity", "Height"];
    let ground = ["Top", "Under", "Under depth", "Deep", "Steep", "Steep slope", "Shore", "Shore height"];
    for label in labels.into_iter().chain(ground) {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "a biome's {label}");
    }
    assert!(prop_rows(&editor.ui, "Heights").is_empty(), "it keeps to no height yet");
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Top")[0]).contains("Moss"));
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Under")[0]).contains("None (Block)"));
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]).contains("Custom"));
    shot(&mut renderer, "biome-inspector.png");

    // Top picks from the project's blocks. Sand picked, the file names it, and the world listing the biome is made
    // again at once: sand everywhere, with what was built on it kept.
    open_picker(&mut editor, &mut renderer, "Top");
    let offered = picker_rows(&editor.ui);
    assert_eq!(offered[0], "None");
    assert!(offered.iter().any(|row| row == "Sand.block") && !offered.iter().any(|row| row.ends_with(".biome")));
    let row = picker_row(&editor.ui, "Sand.block").unwrap();
    click(&mut editor.ui, row);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let sand_guid = editor.game.content.db().guid_of("Blocks/Sand.block").unwrap().to_string();
    assert_eq!(read().ground.top.as_deref(), Some(sand_guid.as_str()));
    let sand = number(&editor, "Blocks/Sand.block").expect("the world holds sand now");
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [sand]);
    let blocks = editor.game.engine.world.get::<voxear_terrain::blocks::BlockWorld>(field).unwrap();
    assert_eq!(blocks.blocks.get(built), 1, "what was built is kept");
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Top")[0]).contains("Sand"));

    // The cross beside a block takes it out: the top is the world's own grass again.
    let clear = first_with(&editor.ui, prop_rows(&editor.ui, "Top")[0], "slot-clear").expect("its cross");
    assert!(editor.ui.is_visible(clear));
    click(&mut editor.ui, clear);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(read().ground.top, None);
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [1]);
    let clear = first_with(&editor.ui, prop_rows(&editor.ui, "Top")[0], "slot-clear").expect("its cross");
    assert!(!editor.ui.is_visible(clear), "nothing to take out");

    // A preset sets how warm and how wet at once; a number typed after is its own, and the Preset says Custom.
    let preset = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    choose(&mut editor, &mut renderer, preset, 5);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((read().temperature, read().humidity), ([18.0, 60.0], [0.0, 0.25]), "the Desert's");
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]).contains("Desert"));
    let fields = |editor: &voxear_editor::Editor, label: &str| {
        let mut fields = Vec::new();
        with_class(&editor.ui, prop_rows(&editor.ui, label)[0], "value-input", &mut fields);
        fields
    };
    let type_into = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, field: NodeId, text: &str| {
        editor.ui.focus(field);
        editor.ui.set_modifiers(Modifiers { shift: false, control: true });
        editor.ui.key_down(Key::Char('a'));
        editor.ui.set_modifiers(Modifiers::default());
        editor.ui.text_input(text);
        frame(editor, renderer);
        editor.ui.blur();
        frame(editor, renderer);
        frame(editor, renderer);
    };
    let warmest = fields(&editor, "Temperature")[1];
    type_into(&mut editor, &mut renderer, warmest, "40");
    assert_eq!(read().temperature, [18.0, 40.0]);
    assert!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]).contains("Custom"));
    // Height switched on, it keeps to heights, which are typed in metres.
    let height = first_with(&editor.ui, prop_rows(&editor.ui, "Height")[0], "checkbox").expect("its switch");
    click(&mut editor.ui, height);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(read().height, Some([200.0, 10_000.0]));
    let lowest = fields(&editor, "Heights")[0];
    type_into(&mut editor, &mut renderer, lowest, "350");
    assert_eq!(read().height, Some([350.0, 10_000.0]));
    let depth = fields(&editor, "Under depth")[0];
    type_into(&mut editor, &mut renderer, depth, "5");
    let slope = fields(&editor, "Steep slope")[0];
    type_into(&mut editor, &mut renderer, slope, "55");
    assert_eq!((read().ground.under_depth, read().ground.steep_slope), (5, 55.0));
    // Its colour typed is its colour in the world's list of biomes.
    let red = first_with(&editor.ui, prop_rows(&editor.ui, "Color")[0], "axis-input").expect("its red");
    type_into(&mut editor, &mut renderer, red, "0.25");
    assert_eq!(read().color[0], 0.25);
    shot(&mut renderer, "biome-inspector-edited.png");
    shot_scrolled(&mut editor, &mut renderer, "biome-inspector-ground.png", 400.0);

    // New Biome in the Project panel's menu makes one in the folder it shows, as new as any: no blocks.
    editor.ping_asset("Biomes/Cool.biome");
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let mut tiles = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "tile", &mut tiles);
    let named = |tile: NodeId| editor.ui.children(tile).get(1).map(|&name| editor.ui.text(name)).unwrap_or_default();
    let cool = tiles.into_iter().find(|&tile| editor.ui.is_visible(tile) && named(tile) == "Cool.biome");
    let cool = cool.expect("the Project panel shows the Biomes folder");
    context_click(&mut editor.ui, cool);
    frame(&mut editor, &mut renderer);
    // The menu has more to make than a small window has room for: scrolled to, as the wheel does.
    let new = menu_item(&editor.ui, "New Biome").expect("the menu makes biomes");
    scroll_to(&mut editor, &mut renderer, new);
    let new = menu_item(&editor.ui, "New Biome").expect("the menu makes biomes");
    click(&mut editor.ui, new);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let made = std::fs::read_to_string(dir.path().join("Biomes/New Biome.biome")).expect("a new biome");
    assert_eq!(BiomeDef::parse(&made).unwrap(), BiomeDef::default());
    // Its name let be. New Biome from the command palette makes another, in the project's Biomes folder, and
    // shows it.
    editor.ui.key_down(Key::Escape);
    frame(&mut editor, &mut renderer);
    run_command(&mut editor, &mut renderer, "new biome");
    frame(&mut editor, &mut renderer);
    assert!(dir.path().join("Biomes/New Biome 1.biome").is_file());
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Biomes/New Biome 1.biome"));
}

#[test]
fn starter_biomes_are_made_beside_a_generated_world_with_their_blocks_and_listed_on_it() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::{blocks::block::BlockDef, generate::BiomeDef};
    let Some((dir, mut renderer, mut editor, field)) = editor_with_a_generated_world(&[]) else { return };
    // A block of a starter's name elsewhere in the project, of a colour of the project's own.
    std::fs::create_dir_all(dir.path().join("Mine")).unwrap();
    std::fs::write(dir.path().join("Mine/Gravel.block"), "color = [0.1, 0.2, 0.3]\n").unwrap();
    editor.game.content.rescan().unwrap();
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let starters = button_saying(&editor.ui, inspector, "Add Starter Biomes").expect("Add Starter Biomes");
    click(&mut editor.ui, starters);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    // Eight biomes, in a Biomes folder beside the world's Blocks folder, listed on the world.
    let names = ["Snow", "Tundra", "Taiga", "Grassland", "Forest", "Desert", "Savanna", "Rainforest"];
    assert_eq!(listed_biomes(&editor.ui), names);
    let biome = |name: &str| {
        let text = std::fs::read_to_string(dir.path().join(format!("Biomes/{name}.biome")));
        BiomeDef::parse(&text.unwrap_or_else(|_| panic!("{name}'s file"))).unwrap()
    };
    // Each names its ground's blocks, with rock on steep ground and something along its shore; the heights are
    // the Snow's.
    let db = editor.game.content.db();
    let path_of = |reference: &Option<String>| {
        let guid = editor.game.content.parse(reference.as_deref().expect("a block")).unwrap().guid().unwrap();
        db.get(guid).expect("a block of the project").path.clone()
    };
    for name in names {
        let ground = biome(name).ground;
        let named = [&ground.top, &ground.under, &ground.deep, &ground.steep, &ground.shore].map(path_of);
        assert!(named.iter().all(|path| path.ends_with(".block")), "{name}: {named:?}");
        assert!(["Stone.block", "Sandstone.block"].iter().any(|rock| named[3].ends_with(rock)), "{name}: {named:?}");
        assert!(["Sand.block", "Gravel.block"].iter().any(|shore| named[4].ends_with(shore)), "{name}: {named:?}");
    }
    assert!(biome("Snow").height.is_some() && biome("Grassland").height.is_none());
    assert_eq!(path_of(&biome("Desert").ground.top), "Blocks/Sand.block");
    // Their blocks are beside the world, each a colour. The world's own grass, dirt and stone are theirs too, the
    // sand the project had there is used as it was, and the gravel it had elsewhere is used where it is.
    let block = |path: &str| BlockDef::parse(&std::fs::read_to_string(dir.path().join(path)).unwrap()).unwrap();
    assert_eq!(block("Blocks/Snow.block").color, [0.95, 0.96, 0.98]);
    assert_eq!(block("Blocks/Sand.block").color, [0.9, 0.8, 0.5], "the project's own sand");
    assert!(!dir.path().join("Blocks/Gravel.block").exists());
    assert_eq!(path_of(&biome("Tundra").ground.shore), "Mine/Gravel.block");
    let made = std::fs::read_dir(dir.path().join("Blocks")).unwrap().flatten();
    let blocks = made.filter(|entry| entry.file_name().to_string_lossy().ends_with(".block")).count();
    assert_eq!(blocks, 15, "the fifteen of their ground and the vine, but the gravel, which the project has elsewhere");
    // The wet ones hang vines of that block; the dry and the frozen none.
    assert_eq!(block("Blocks/Vine.block").shape, voxear_terrain::blocks::block::BlockShape::Vine);
    assert_eq!(path_of(&biome("Rainforest").vines.block), "Blocks/Vine.block");
    assert!(biome("Rainforest").vines.amount > biome("Forest").vines.amount && biome("Desert").vines.is_none());
    // The ground is made again from them: rolling hills in a mild climate, so more than one kind of country and
    // none of it snow.
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    assert_eq!(asset.file.generate.as_ref().unwrap().biomes.len(), 8);
    let snow = asset.drawn.number_of(db.guid_of("Blocks/Snow.block").unwrap()).expect("snow is numbered");
    let now = tops(&editor, field);
    assert!(!now.contains_key(&snow) && !now.contains_key(&1), "{now:?}");
    shot_scrolled(&mut editor, &mut renderer, "biomes-starters.png", 420.0);
    // Asked for again, nothing is made twice or listed twice.
    let starters = button_saying(&editor.ui, inspector, "Add Starter Biomes").unwrap();
    click(&mut editor.ui, starters);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(listed_biomes(&editor.ui), names);
    assert!(!dir.path().join("Biomes/Snow 1.biome").exists());
    // One step to undo: the list is empty again and the ground the world's own; the files stay.
    editor.undo(&mut renderer);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert!(listed_biomes(&editor.ui).is_empty());
    assert_eq!(tops(&editor, field).keys().copied().collect::<Vec<_>>(), [1]);
    assert!(dir.path().join("Biomes/Snow.biome").is_file());
}

fn context_click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_secondary(true);
    ui.pointer_secondary(false);
}

/// What Add Starter Biomes makes of a world of blocks `columns` blocks a side, each `block` metres, generated by
/// `preset` from where `fits` says its ground suits (given the least and greatest heights over the sea of a
/// square that large): for each 50 m of height over the sea, how many columns are there and how many of them have
/// snow on top. Clicked in the editor as a user does; with `VOXEAR_TEST_OUTPUT` set, a picture of it from above
/// one corner goes there as `picture`.
fn starter_biomes_on(
    preset: voxear_terrain::generate::Preset,
    (block, columns): (f32, u32),
    fits: impl Fn(f32, f32) -> bool,
    picture: &str,
) -> Option<Vec<(f32, usize, usize)>> {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{
        BlockWorld,
        file::{BlockFile, BlockKind},
        generate::BlockGenerate,
    };
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let dir = hello_with_a_field();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let world = preset.world();
    write("Worlds/World.worldgen", &world.to_text());
    for (name, color) in [("Grass", "0.33, 0.52, 0.2"), ("Dirt", "0.45, 0.32, 0.2"), ("Stone", "0.5, 0.5, 0.52")] {
        write(&format!("Blocks/{name}.block"), &format!("color = [{color}]\n"));
    }
    write("Blocks/Water.block", "color = [0.05, 0.3, 0.45]\nliquid = \"water\"\n");
    // The first place, looked for half a kilometre at a time, whose ground suits.
    let (side, sea) = (block * columns as f32, world.sea_level());
    let heights = |corner: [f32; 2]| {
        let step = side / 8.0;
        let at = |i: usize| world.height_at(corner[0] + (i % 9) as f32 * step, corner[1] + (i / 9) as f32 * step);
        let heights: Vec<f32> = (0..81).map(at).collect();
        let least = heights.iter().copied().fold(f32::MAX, f32::min);
        (least - sea, heights.iter().copied().fold(f32::MIN, f32::max) - sea)
    };
    let position = (0..6400)
        .map(|i| [(i % 80) as f32 * 500.0 - 20_000.0, (i / 80) as f32 * 500.0 - 20_000.0])
        .find(|&corner| {
            let (least, greatest) = heights(corner);
            fits(least, greatest)
        })
        .expect("ground that suits somewhere within 20 km");
    let (least, greatest) = heights(position);
    let mut file = BlockFile { block_size: block, ..BlockFile::default() };
    let held = ["Grass", "Dirt", "Stone", "Water"];
    file.blocks = held.map(|name| BlockKind::of(&format!("Blocks/{name}.block"))).to_vec();
    file.ground.size = [columns, columns];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/World.worldgen".into()),
        bottom: ((least + sea) / block).floor() as i32 - 24,
        caves: 0.0,
        position,
        sea: 4,
        ..BlockGenerate::default()
    });
    write("Blocks/Field.blockworld", &file.to_text());
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    std::fs::write(&day, scene + "[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let field = obj(&editor, "Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    show_block_tool(&mut editor, &mut renderer, "Settings");
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let starters = button_saying(&editor.ui, inspector, "Add Starter Biomes").expect("Add Starter Biomes");
    click(&mut editor.ui, starters);
    // A world this large is made again on other threads: until it is.
    let listed = |editor: &voxear_editor::Editor| {
        let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap();
        asset.file.generate.as_ref().is_some_and(|generate| generate.placed.is_some())
    };
    for _ in 0..3000 {
        frame(&mut editor, &mut renderer);
        if listed(&editor) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(listed(&editor), "its ground made again with its biomes");
    let asset = editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().clone();
    let snow = asset.drawn.number_of(editor.game.content.db().guid_of("Blocks/Snow.block").unwrap()).expect("snow");
    let water = 4;
    // Each column's top block, by how high over the sea its ground is.
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    let bottom = asset.file.generate.as_ref().unwrap().bottom;
    let mut bands: std::collections::BTreeMap<i32, (usize, usize)> = Default::default();
    let step = (columns / 96).max(1) as usize;
    for z in (0..columns as i32).step_by(step) {
        for x in (0..columns as i32).step_by(step) {
            let top = (bottom..bottom + 2000).rev().find(|&y| !matches!(blocks.blocks.get([x, y, z]), 0 | 4));
            let Some(top) = top else { continue };
            let above_sea = (top + 1) as f32 * block - sea;
            if above_sea < 0.0 || blocks.blocks.get([x, top + 1, z]) == water {
                continue;
            }
            let band = bands.entry((above_sea / 50.0).floor() as i32).or_default();
            band.0 += 1;
            band.1 += usize::from(blocks.blocks.get([x, top, z]) == snow);
        }
    }
    if std::env::var("VOXEAR_TEST_OUTPUT").is_ok() {
        // Looked at from high over its corner, across it and down, with nothing selected and the view to itself.
        editor.select(None);
        let tab = dock_tab(&editor.ui, "Scene").expect("the Scene view's tab");
        context_click(&mut editor.ui, tab);
        frame(&mut editor, &mut renderer);
        let maximize = menu_item(&editor.ui, "Maximize").expect("Maximize");
        click(&mut editor.ui, maximize);
        frame(&mut editor, &mut renderer);
        let camera = editor.scene_view_cameras()[0].1;
        let corner = voxear_core::math::Vec3::new(-256.0, 0.5, -256.0);
        let (low, high) = (least.max(0.0) + sea, greatest + sea);
        let from = corner + voxear_core::math::Vec3::new(-side * 0.12, high + side * 0.32, -side * 0.12);
        let towards = corner + voxear_core::math::Vec3::new(side * 0.5, (low + high) * 0.5, side * 0.5) - from;
        let (yaw, pitch) = (towards.x.atan2(towards.z), (-towards.y).atan2(towards.x.hypot(towards.z)));
        let turn = voxear_core::math::Quat::from_rotation_y(yaw) * voxear_core::math::Quat::from_rotation_x(pitch);
        let view = editor.game.engine.world.get_mut::<voxear_scene::Transform>(camera).unwrap();
        (view.position, view.rotation) = (from, turn);
        // A world this large comes to the GPU over many frames.
        for _ in 0..240 {
            frame(&mut editor, &mut renderer);
        }
        shot(&mut renderer, picture);
    }
    Some(bands.into_iter().map(|(band, (count, snowy))| (band as f32 * 50.0, count, snowy)).collect())
}

#[test]
fn the_starter_biomes_put_snow_on_a_world_of_mountains_heights_and_none_on_its_low_ground() {
    use voxear_terrain::generate::Preset;
    // Mountains 2 km a side in blocks of 8 m, from the sea to well over the snow line.
    let alps = |least: f32, greatest: f32| least < 30.0 && greatest > 620.0;
    let Some(bands) = starter_biomes_on(Preset::Alpine, (8.0, 256), alps, "starter-biomes-alpine-8m.png") else {
        return;
    };
    let said: Vec<String> = bands.iter().map(|(from, count, snowy)| format!("{from} m: {snowy} of {count}")).collect();
    println!("{said:#?}");
    let share = |from: f32, to: f32| {
        let within = bands.iter().filter(|(band, ..)| (from..to).contains(band));
        let (count, snowy) = within.fold((0, 0), |(count, snowy), (_, c, s)| (count + c, snowy + s));
        (count, snowy as f32 / count.max(1) as f32)
    };
    let (low, low_snow) = share(0.0, 200.0);
    let (high, high_snow) = share(350.0, 10_000.0);
    assert!(low > 100 && high > 100, "{said:?}");
    assert_eq!(low_snow, 0.0, "no snow on its low ground: {said:?}");
    // Its heights are snow but where they are steep, which is rock.
    assert!(high_snow > 0.3, "snow on its heights: {high_snow} of them, {said:?}");
    // The other worlds this was tuned by, for their pictures alone.
    if std::env::var("VOXEAR_TEST_OUTPUT").is_err() {
        return;
    }
    let slope = |least: f32, greatest: f32| least < 190.0 && greatest > 400.0 && least > 60.0;
    starter_biomes_on(Preset::Alpine, (1.0, 448), slope, "starter-biomes-alpine-1m.png");
    let anywhere = |_: f32, _: f32| true;
    starter_biomes_on(Preset::RollingHills, (8.0, 256), anywhere, "starter-biomes-hills-8m.png");
    starter_biomes_on(Preset::RollingHills, (1.0, 448), anywhere, "starter-biomes-hills-1m.png");
}
