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
