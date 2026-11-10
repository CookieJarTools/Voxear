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
    pointer_to(editor, point);
    frame(editor, renderer);
    if shift {
        shift_key(editor, true);
    }
    left_button(editor, true);
    for _ in 0..6 {
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
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
    stroke(&mut editor, &mut renderer, point, false);
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
    let text = std::fs::read_to_string(dir.path().join(path)).unwrap();
    assert_eq!(WorldGen::parse(&text).unwrap().preset(), Some(Preset::RollingHills));
    // The ground is the generator's, from the corner of its world.
    let rolling = WorldGen::default();
    for (x, z) in [(100.0, 100.0), (300.0, 50.0), (256.0, 400.0)] {
        let made = rolling.height_at(x, z);
        let laid = height(&editor, field, x, z);
        assert!((laid - made).abs() < 1e-2, "at {x}, {z}: {laid} not {made}");
    }
    shot(&mut renderer, "terrain-generate.png");

    // In its Inspector, Alpine picked: saved, and the terrain's ground made again from it at once.
    assert!(editor.inspect_path(path));
    frame(&mut editor, &mut renderer);
    for label in ["Preset", "Seed", "Scale", "Sea level", "Height"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the generator's {label}");
    }
    let presets = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Preset")[0]), "Rolling Hills");
    choose(&mut editor, &mut renderer, presets, 2);
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(dir.path().join(path)).unwrap();
    let alpine = WorldGen::parse(&text).unwrap();
    assert_eq!(alpine.preset(), Some(Preset::Alpine), "saved as it was picked");
    let made = alpine.height_at(100.0, 100.0);
    let laid = height(&editor, field, 100.0, 100.0);
    assert!((laid - made).abs() < 1e-2, "made again: {laid} not {made}");
    shot(&mut renderer, "terrain-generate-alpine.png");

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
    // Undone, a terrain of a set size again, made whole from its Obj.
    editor.undo(&mut renderer);
    editor.undo(&mut renderer);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
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
}

/// Picks one of the Block World's tools by its segment.
fn show_block_tool(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, tool: &str) {
    let strip = first_with(&editor.ui, editor.ui.root(), "block-tools").expect("the Block World's tools");
    let segment = editor.ui.children(strip).into_iter().find(|&s| editor.ui.text(s) == tool).expect("the tool");
    click(&mut editor.ui, segment);
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

#[test]
fn blocks_are_placed_broken_and_their_kinds_changed_in_the_scene_view_each_a_step_to_undo() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockWorld, edits::BlockEdits};
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
    for label in ["Brush", "Size", "Name", "Color", "Material", "Top", "Bottom"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "Build's {label}");
    }
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
    // A new kind, placed: number 4; then Grass taken out of the list, its blocks with it, the rest a number up.
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Block");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let kinds = |editor: &voxear_editor::Editor| {
        editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.blocks.len()
    };
    assert_eq!(kinds(&editor), 4);
    // Where the click lands now: against the face of the grass block it meets, likely its side.
    let blocks_now = editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.clone();
    let from = (origin - corner).to_array();
    let met = voxear_terrain::blocks::raycast(&blocks_now, from, direction.to_array(), 500.0).expect("a block");
    let against = [0, 1, 2].map(|i| met.block[i] + met.normal[i]);
    click_scene(&mut editor, &mut renderer, point);
    let world_blocks =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().blocks.clone();
    assert_eq!(world_blocks(&editor).get(against), 4, "the new kind placed against the face it meets");
    let mut removes = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "collection-remove", &mut removes);
    removes.retain(|&r| editor.ui.is_visible(r));
    click(&mut editor.ui, removes[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(kinds(&editor), 3);
    // Dirt is now 1 and stone 2; the ground's grass top is air, and the new kind 3.
    assert_eq!([8, 7, 5, 3].map(|y| block(&editor, y)), [0, 0, 1, 2]);
    assert_eq!(world_blocks(&editor).get(against), 3);
    // Another kind added, then dragged by its grip above the one placed: the ground has neither, and the placed
    // block keeps its kind by its new number.
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Block");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(kinds(&editor), 4);
    let mut grips = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "collection-grip", &mut grips);
    grips.retain(|&g| editor.ui.is_visible(g));
    let ([gx, gy, gw, gh], top) = (editor.ui.rect(grips[3]), editor.ui.rect(grips[2])[1]);
    let from = [gx + gw / 2.0, gy + gh / 2.0];
    editor.ui.pointer_move(from[0], from[1]);
    editor.ui.pointer_button(true);
    frame(&mut editor, &mut renderer);
    for step in 1..=4 {
        editor.ui.pointer_move(from[0], from[1] + (top - 2.0 - from[1]) * step as f32 / 4.0);
        frame(&mut editor, &mut renderer);
    }
    editor.ui.pointer_button(false);
    frame(&mut editor, &mut renderer);
    let names = |editor: &voxear_editor::Editor| {
        let file = &editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file;
        file.blocks.iter().map(|kind| kind.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&editor)[2..], ["Block 5", "Block 4"], "the second named past the first, and moved above it");
    assert_eq!(world_blocks(&editor).get(against), 4, "the placed block still the kind placed");
    // Saved, its changes from its ground are kept beside it, small.
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
}

#[test]
fn a_kind_of_block_made_water_is_placed_flows_out_and_a_generated_world_can_fill_its_sea_with_it() {
    use voxear_engine::blocks::BlockWorldAsset;
    use voxear_terrain::blocks::{BlockMap, BlockWorld, flow::Water, is_water};
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
    // A new kind, made water: its materials' rows go, as water is drawn with its colour alone.
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Block");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let water = first_with(&editor.ui, prop_rows(&editor.ui, "Water")[0], "checkbox").expect("Water");
    click(&mut editor.ui, water);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for (label, shown) in [("Water", 1), ("Color", 1), ("Material", 0), ("Top", 0), ("Bottom", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "{label}");
    }
    let file =
        |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorldAsset>(field).unwrap().file.clone();
    assert!(file(&editor).blocks[3].water && !file(&editor).blocks[0].water);
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
    assert_eq!(blocks.blocks.get(under), 4, "water placed on the face clicked");
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
    // Undone, the water it ran out into goes with it.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let blocks = editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!((blocks.blocks.get(under), blocks.blocks.get(beside)), (0, 0), "undone");
    // Generated ground has a sea block to fill its sea with: air, or the water.
    show_block_tool(&mut editor, &mut renderer, "Settings");
    assert!(prop_rows(&editor.ui, "Sea block").is_empty(), "flat ground has no sea");
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 1);
    frame(&mut editor, &mut renderer);
    let sea = first_with(&editor.ui, prop_rows(&editor.ui, "Sea block")[0], "value-choice").expect("its sea");
    choose(&mut editor, &mut renderer, sea, 4);
    frame(&mut editor, &mut renderer);
    assert_eq!(file(&editor).generate.expect("generated").sea, 4);
}

#[test]
fn a_block_worlds_ground_is_generated_by_a_picked_generator_and_covered_by_a_kinds_biome() {
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
    // A kind that covers where it is below freezing, as Snow: everywhere in this cold world.
    show_block_tool(&mut editor, &mut renderer, "Build");
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Block");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
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
    assert!(blocks(&editor).meshes.values().all(|mesh| mesh.quads.len() == mesh.indices.len() * 2));
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
    let smooth = first_with(&editor.ui, prop_rows(&editor.ui, "Smooth")[0], "checkbox").expect("its Smooth");
    click(&mut editor.ui, smooth);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!blocks(&editor).smooth);
    assert!(blocks(&editor).meshes.values().all(|mesh| mesh.quads.len() * 3 != mesh.indices.len() * 2));
}

#[test]
fn a_block_worlds_ground_made_a_planet_lies_all_round_its_middle_as_large_as_its_radius() {
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
    show_block_tool(&mut editor, &mut renderer, "Settings");
    // A planet: its radius in place of its ground's size, place, lowest block and end.
    let ground = first_with(&editor.ui, prop_rows(&editor.ui, "Ground")[0], "value-choice").expect("its ground");
    choose(&mut editor, &mut renderer, ground, 2);
    frame(&mut editor, &mut renderer);
    for (label, shown) in [("Radius", 1), ("Generator", 1), ("Offset", 0), ("Endless", 0), ("Ground size", 0)] {
        assert_eq!(prop_rows(&editor.ui, label).len(), shown, "{label}");
    }
    type_number(&mut editor, &mut renderer, "Radius", "20");
    let blocks = |editor: &voxear_editor::Editor| editor.game.engine.world.get::<BlockWorld>(field).unwrap().clone();
    assert_eq!(blocks(&editor).planet, Some(20.0), "physics pulls things towards it");
    // With no generator, level with its sea all round: ground inside, hollowed here and there by caves, and grass
    // 19 blocks out every way.
    let now = blocks(&editor);
    let inside: Vec<[i32; 3]> = (-14..14)
        .flat_map(|z| (-14..14).flat_map(move |y| (-14..14).map(move |x| [x, y, z])))
        .filter(|at| at.iter().map(|v| (*v as f32 + 0.5).powi(2)).sum::<f32>() < 14.0 * 14.0)
        .collect();
    let ground = inside.iter().filter(|&&at| now.blocks.get(at) != AIR).count();
    assert!(ground * 10 > inside.len() * 7, "{ground} of {} blocks inside are ground", inside.len());
    for at in [[19, 0, 0], [-20, 0, 0], [0, 19, 0], [0, -20, 0], [0, 0, 19]] {
        assert_eq!(now.blocks.get(at), 1, "grass at {at:?}");
    }
    assert_eq!(now.blocks.get([0, 21, 0]), AIR);
    // Undone, flat ground again: the two changes are one step when they came within the history's merge time, else
    // two, which a busy machine making the first planet slowly can take.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    if blocks(&editor).planet.is_some() {
        assert_eq!(blocks(&editor).planet, Some(100.0), "the radius's own step undone, the first planet left");
        editor.undo(&mut renderer);
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(blocks(&editor).planet, None);
    assert_eq!(blocks(&editor).blocks.get([0, 7, 0]), 1, "its flat ground's grass");
}
