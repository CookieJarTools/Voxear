//! A World Generator's graph in the editor, on a copy of Hello with a
//! Terrain generated from it: the graph opens in the Generator panel, a node
//! picked there shows in the Inspector, nodes are added from the search and
//! wired by dragging, each change is a step of the graph's own to undo, a
//! value exposed becomes a row of the generator, and the terrain's ground
//! follows every change that makes other ground. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_terrain::{
    Terrain,
    generate::{NodeId as GraphNode, NodeKind, Preset, WorldGen, nodes::Output},
};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1600, 900);

/// An empty Obj for the terrain, as in the terrain's own tests.
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

fn middle(ui: &Ui, node: NodeId) -> (f32, f32) {
    let [x, y, w, h] = ui.rect(node);
    (x + w / 2.0, y + h / 2.0)
}

fn click(ui: &mut Ui, node: NodeId) {
    let (x, y) = middle(ui, node);
    ui.pointer_move(x, y);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// A press and release at a point of the window, a frame between them, as a user's click is.
fn click_at(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, point: (f32, f32)) {
    editor.ui.pointer_move(point.0, point.1);
    editor.ui.pointer_button(true);
    frame(editor, renderer);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
}

/// A drag from one point of the window to another, over a few frames.
fn drag(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, from: (f32, f32), to: (f32, f32)) {
    editor.ui.pointer_move(from.0, from.1);
    editor.ui.pointer_button(true);
    frame(editor, renderer);
    for step in 1..=4 {
        let t = step as f32 / 4.0;
        editor.ui.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        frame(editor, renderer);
    }
    editor.ui.pointer_button(false);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// Every Inspector row labelled `label`.
fn prop_rows(ui: &Ui, label: &str) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "inspector").expect("the Inspector"), "prop", &mut rows);
    rows.retain(|&row| ui.is_visible(row) && ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
    rows
}

/// The labels of the Inspector's rows, in order.
fn row_labels(ui: &Ui) -> Vec<String> {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "inspector").expect("the Inspector"), "prop", &mut rows);
    rows.into_iter().filter_map(|row| ui.children(row).first().map(|&label| ui.text(label))).collect()
}

/// The object picker's row that says `label`.
fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
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

/// Picks a kind of node in the open search, as a user does: its name typed, and a click on it.
fn pick_node(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    assert!(editor.picker_open(), "the search for a node to add");
    frame(editor, renderer);
    editor.ui.text_input(name);
    frame(editor, renderer);
    let row = picker_row(&editor.ui, name).unwrap_or_else(|| panic!("{name} is offered"));
    click(&mut editor.ui, row);
    frame(editor, renderer);
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

/// Runs frames until the open graph's previews are as fine as they get, when pictures are being kept: a
/// picture then shows them.
fn settle(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    if std::env::var("VOXEAR_TEST_OUTPUT").is_err() {
        return;
    }
    let waited = std::time::Instant::now();
    while !editor.generator().previews_settled() && waited.elapsed() < std::time::Duration::from_secs(120) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(editor, renderer);
    }
    for _ in 0..4 {
        frame(editor, renderer);
    }
}

fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

/// A copy of Hello open in the editor, with a Terrain on its field whose one layer generates its
/// ground from a World Generator that is one landmass, as a file from before generators were
/// graphs is: the project, the editor, the terrain's Obj and the generator's path.
fn editor_with_generated_ground() -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor, Entity, String)> {
    let (dir, mut renderer, mut editor, field, path) = editor_with_a_new_generator()?;
    // Written outside the editor, which reads it again when it has the focus back.
    std::fs::write(dir.path().join(&path), Preset::RollingHills.world().to_text()).unwrap();
    editor.handle(&winit::event::WindowEvent::Focused(true));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    Some((dir, renderer, editor, field, path))
}

/// A copy of Hello open in the editor, with a Terrain on its field whose one layer generates its
/// ground from the World Generator that Add Layer made for it.
fn editor_with_a_new_generator() -> Option<(tempfile::TempDir, Renderer, voxear_editor::Editor, Entity, String)> {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let named = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, name)| name.0 == "Field");
    let field = named.map(|(entity, _)| entity).expect("the Field");
    editor.select(Some(field));
    frame(&mut editor, &mut renderer);
    // Add Component ▸ Terrain, then Add Layer ▸ Generate, which makes a generator beside the terrain's file.
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").expect("Add Component");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("Terrain");
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Terrain").expect("Terrain is offered");
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // At its coarsest, so the test does not wait on a million heights each time the ground is made again.
    show_tool(&mut editor, &mut renderer, "Settings");
    let resolution =
        first_with(&editor.ui, prop_rows(&editor.ui, "Resolution")[0], "value-choice").expect("its choices");
    click(&mut editor.ui, resolution);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    click(&mut editor.ui, options[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    show_tool(&mut editor, &mut renderer, "Layers");
    let add = first_with(&editor.ui, editor.ui.root(), "collection-add").expect("Add Layer");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let generate = menu_item(&editor.ui, "Generate").expect("a Generate layer to add");
    click(&mut editor.ui, generate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let db = editor.game.content.db();
    let made: Vec<String> = db
        .entries()
        .filter(|entry| entry.kind == voxear_assets::AssetKind::WorldGen)
        .map(|entry| entry.path.clone())
        .collect();
    let [path] = made.as_slice() else { panic!("one generator made: {made:?}") };
    let path = path.clone();
    Some((dir, renderer, editor, field, path))
}

/// How high the field's ground is at `(x, z)` from its corner.
fn height(editor: &voxear_editor::Editor, field: Entity, x: f32, z: f32) -> f32 {
    editor.game.engine.world.get::<Terrain>(field).unwrap().heights.height_at(x, z)
}

/// The generator as its file now has it.
fn saved(dir: &tempfile::TempDir, path: &str) -> WorldGen {
    WorldGen::parse(&std::fs::read_to_string(dir.path().join(path)).unwrap()).unwrap()
}

/// Whether the field's ground is the ground `world` makes, at a few places.
fn follows(editor: &voxear_editor::Editor, field: Entity, world: &WorldGen) -> bool {
    [(100.0, 100.0), (300.0, 50.0), (256.0, 400.0)]
        .into_iter()
        .all(|(x, z)| (height(editor, field, x, z) - world.height_at(x, z)).abs() < 1e-2)
}

/// The open graph's first node of this kind.
fn node(editor: &voxear_editor::Editor, title: &str) -> GraphNode {
    let world = editor.generator().world().expect("a graph is open");
    let found = world.graph().nodes.iter().find(|node| node.kind.title() == title);
    found.unwrap_or_else(|| panic!("a {title} node")).id
}

#[test]
fn a_generators_graph_is_opened_edited_undone_and_exposed_and_the_ground_follows() {
    let Some((dir, mut renderer, mut editor, field, path)) = editor_with_generated_ground() else { return };
    let rolling = saved(&dir, &path);
    assert_eq!(rolling.preset(), Some(Preset::RollingHills));
    assert!(follows(&editor, field, &rolling));

    // Opened as a double click on it in the Project does: its graph in the Generator panel, and its own rows
    // in the Inspector, with Open Graph under them.
    assert!(editor.generator().path().is_none());
    assert!(editor.open_path(&path, &mut renderer));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.generator().path(), Some(path.as_str()));
    let panel = editor.ui.find(editor.ui.root(), "generator-panel").expect("the Generator panel");
    assert!(editor.ui.rect(panel)[2] > 400.0, "showing, beside the Scene view: {:?}", editor.ui.rect(panel));
    for label in ["Preset", "Seed", "Scale", "Sea level", "Height", "Temperature", "Humidity"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "the generator's {label}");
    }
    assert!(first_with(&editor.ui, editor.ui.root(), "open-graph").is_some(), "Open Graph");
    let titles = |editor: &voxear_editor::Editor| -> Vec<String> {
        editor.generator().drawn().unwrap().nodes.iter().map(|node| node.title.clone()).collect()
    };
    assert_eq!(titles(&editor), ["Landmass", "Climate", "Output"]);
    let (land, output) = (node(&editor, "Landmass"), node(&editor, "Output"));

    // A press on a node picks it, and the Inspector shows its rows: those it seldom needs under Advanced.
    let on = editor.generator().node_point(&editor.ui, land).expect("the Landmass node");
    click_at(&mut editor, &mut renderer, on);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.generator().picked(), [land]);
    let labels = row_labels(&editor.ui);
    assert_eq!(labels[..2], ["Preset", "Height"], "{labels:?}");
    assert!(labels.contains(&"Sharpness".to_string()) && labels.contains(&"Coast".to_string()), "{labels:?}");
    assert!(!labels.contains(&"Seed".to_string()), "the node's rows, not the generator's");
    // A value typed is saved, and the terrain's ground is made again from it.
    type_number(&mut editor, &mut renderer, "Height", "300");
    frame(&mut editor, &mut renderer);
    let higher = saved(&dir, &path);
    assert_eq!(higher.as_landmass().map(|land| land.height), Some(300.0));
    assert!(higher.hash() != rolling.hash() && follows(&editor, field, &higher));

    // A context click on the canvas opens the search, with the kinds of node in their folders; one picked is
    // added where it was asked for, picked, and shown in the Inspector.
    let canvas = first_with(&editor.ui, panel, "graph-canvas").expect("the canvas");
    let [left, top, _, tall] = editor.ui.rect(canvas);
    let corner = (left + 60.0, top + tall - 50.0);
    editor.ui.pointer_move(corner.0, corner.1);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.picker_open());
    for folder in ["Sources", "Shape", "Climate", "Output"] {
        assert!(picker_row(&editor.ui, folder).is_some(), "{folder} in the search");
    }
    pick_node(&mut editor, &mut renderer, "Noise");
    assert_eq!(titles(&editor), ["Landmass", "Climate", "Output", "Noise"]);
    let noise = node(&editor, "Noise");
    assert_eq!(editor.generator().picked(), [noise]);
    assert_eq!(row_labels(&editor.ui), ["Style", "Size", "Octaves", "Low", "High"]);
    let placed = editor.generator().node_point(&editor.ui, noise).unwrap();
    assert!((placed.0 - corner.0).abs() < 120.0 && (placed.1 - corner.1).abs() < 40.0, "{placed:?} by {corner:?}");
    assert_eq!(saved(&dir, &path).graph().nodes.len(), 4, "saved, though the ground is the same");
    assert!(follows(&editor, field, &higher));
    // The keys are still the graph's after the search, which is outside its panel: Ctrl+Z takes the node
    // back, and Ctrl+Y adds it again.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(titles(&editor), ["Landmass", "Climate", "Output"]);
    assert_eq!(prop_rows(&editor.ui, "Seed").len(), 1, "its node gone, the Inspector shows the generator");
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(node(&editor, "Noise"), noise);

    // The toolbar's Add Node opens the same search. A Math node between the two, wired by dragging from port
    // to port: the noise into its A, and it into the Output's height in place of the landmass.
    let add = first_with(&editor.ui, panel, "console-button").expect("Add Node");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    pick_node(&mut editor, &mut renderer, "Math");
    let math = node(&editor, "Math");
    let port = |editor: &voxear_editor::Editor, node: GraphNode, output: bool, index: usize| {
        editor.generator().port_point(&editor.ui, node, output, index).expect("the port")
    };
    let (from, to) = (port(&editor, noise, true, 0), port(&editor, math, false, 0));
    drag(&mut editor, &mut renderer, from, to);
    let (from, to) = (port(&editor, math, true, 0), port(&editor, output, false, Output::HEIGHT));
    drag(&mut editor, &mut renderer, from, to);
    let wired = |editor: &voxear_editor::Editor, to: GraphNode, input: usize| {
        editor.generator().world().unwrap().graph().link_into(to, input).map(|link| link.from)
    };
    assert_eq!((wired(&editor, math, 0), wired(&editor, output, Output::HEIGHT)), (Some(noise), Some(math)));
    let hills = saved(&dir, &path);
    assert_eq!(hills.as_landmass(), None, "a graph of its own now");
    assert!(hills.faults().is_empty() && follows(&editor, field, &hills));
    assert!((0.0..=1.0).contains(&height(&editor, field, 100.0, 100.0)), "noise from 0 to 1");
    // A wire the graph refuses does not land: the Math's output into its own input would close a loop.
    let (from, to) = (port(&editor, math, true, 0), port(&editor, math, false, 1));
    drag(&mut editor, &mut renderer, from, to);
    assert_eq!(wired(&editor, math, 1), None);
    // With its A wired, the Math's A row is gone from the Inspector; B, with no wire, still shows.
    let on = editor.generator().node_point(&editor.ui, math).unwrap();
    click_at(&mut editor, &mut renderer, on);
    frame(&mut editor, &mut renderer);
    assert_eq!(row_labels(&editor.ui), ["Operation", "B"]);

    // Ctrl+Z is the graph's while its panel was the last pressed in: each wire a step, undone and made again,
    // the ground following. The level has nothing to undo, and is not touched.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((wired(&editor, math, 0), wired(&editor, output, Output::HEIGHT)), (Some(noise), Some(land)));
    assert!(follows(&editor, field, &higher), "the landmass's ground again");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(wired(&editor, math, 0), None);
    assert_eq!(row_labels(&editor.ui), ["Operation", "A", "B"], "and the row its wire stood in for");
    editor.redo(&mut renderer);
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!((wired(&editor, math, 0), wired(&editor, output, Output::HEIGHT)), (Some(noise), Some(math)));
    assert!(follows(&editor, field, &hills));

    // A context click on a row's label exposes its value: a row of the generator's own from then on.
    let on = editor.generator().node_point(&editor.ui, noise).unwrap();
    click_at(&mut editor, &mut renderer, on);
    frame(&mut editor, &mut renderer);
    let label = editor.ui.children(prop_rows(&editor.ui, "High")[0])[0];
    let (x, y) = middle(&editor.ui, label);
    editor.ui.pointer_move(x, y);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    let expose = menu_item(&editor.ui, "Expose").expect("Expose in the row's menu");
    click(&mut editor.ui, expose);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let exposed =
        |world: &WorldGen| -> Vec<String> { world.graph().exposed.iter().map(|row| row.label.clone()).collect() };
    assert_eq!(exposed(&saved(&dir, &path)), ["Height", "Temperature", "Humidity", "High"]);
    assert!(editor.ui.has_class(prop_rows(&editor.ui, "High")[0], "exposed"), "marked on the node's row");
    // A press on empty canvas picks nothing, and the Inspector shows the generator: no preset now that
    // its graph is its own, and High among its rows, which changes the node's value.
    click_at(&mut editor, &mut renderer, (left + 30.0, top + 30.0));
    frame(&mut editor, &mut renderer);
    assert!(editor.generator().picked().is_empty());
    assert_eq!(
        row_labels(&editor.ui)[4..],
        ["Seed", "Scale", "Sea level", "Height", "Temperature", "Humidity", "High"]
    );
    type_number(&mut editor, &mut renderer, "High", "40");
    frame(&mut editor, &mut renderer);
    let tall_hills = saved(&dir, &path);
    let noise_kind = |world: &WorldGen| world.graph().node(noise).map(|node| node.kind.clone());
    assert!(matches!(noise_kind(&tall_hills), Some(NodeKind::Noise(noise)) if noise.high == 40.0));
    assert!(follows(&editor, field, &tall_hills));
    let ground: Vec<f32> = (0..40).map(|i| height(&editor, field, 20.0 + i as f32 * 12.0, 200.0)).collect();
    let (low, high) = ground.iter().fold((f32::MAX, f32::MIN), |(low, high), &h| (low.min(h), high.max(h)));
    assert!(high - low > 2.0 && high <= 40.0, "hills up to 40 m now: {low} to {high}");
    // Undone from the generator's own rows, too: they are the graph's while it is open.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(matches!(noise_kind(&saved(&dir, &path)), Some(NodeKind::Noise(noise)) if noise.high == 1.0));
    editor.redo(&mut renderer);
    frame(&mut editor, &mut renderer);

    // Delete takes the picked node out of the graph, with its wires, and no Obj of the level.
    let on = editor.generator().node_point(&editor.ui, land).unwrap();
    click_at(&mut editor, &mut renderer, on);
    editor.delete_selection(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(titles(&editor), ["Climate", "Output", "Noise", "Math"]);
    assert_eq!(exposed(&saved(&dir, &path)), ["Temperature", "Humidity", "High"], "its row went with it");
    assert!(editor.game.engine.world.is_alive(field), "the field is still there");
    // Ctrl+D makes another of the picked node beside it, picked in its place.
    let on = editor.generator().node_point(&editor.ui, noise).unwrap();
    click_at(&mut editor, &mut renderer, on);
    editor.generator_key(voxear_editor::graph_canvas::CanvasKey::Duplicate);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(titles(&editor), ["Climate", "Output", "Noise", "Math", "Noise"]);
    assert_ne!(editor.generator().picked(), [noise]);

    // Its previews are worked out off the editor's thread: a picture on every node that makes something, and
    // the ground in 3D, coarse at first and then as fine as it gets. A drag on the ground turns it.
    let waited = std::time::Instant::now();
    while !editor.generator().previews_settled() && waited.elapsed() < std::time::Duration::from_secs(120) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut editor, &mut renderer);
    }
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.generator().ground_across(), 257);
    let drawn = editor.generator().drawn().unwrap();
    assert!(drawn.nodes.iter().all(|node| node.pictured == node.picture.is_some()), "a picture on each");
    assert!(drawn.nodes.iter().filter(|node| node.pictured).all(|node| node.detail.contains(" to ")));
    let ground = editor.generator().ground_frame();
    let from = middle(&editor.ui, ground);
    let before = editor.generator().ground_turn();
    drag(&mut editor, &mut renderer, from, (from.0 + 40.0, from.1 + 10.0));
    let turned = editor.generator().ground_turn();
    assert!((turned[0] - before[0] - 0.4).abs() < 1e-3 && turned[1] > before[1], "{before:?} to {turned:?}");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "generator-graph.png");

    // The area the previews show is kept in the file, and is no step to undo.
    type_number_in(&mut editor, &mut renderer, panel, "Size", "1200");
    frame(&mut editor, &mut renderer);
    assert_eq!(saved(&dir, &path).preview_area().size, 1200.0);
    assert_eq!(saved(&dir, &path).hash(), editor.generator().world().unwrap().hash());
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(titles(&editor), ["Climate", "Output", "Noise", "Math"], "the duplicate undone, not the area");
    assert_eq!(saved(&dir, &path).preview_area().size, 1200.0);

    // With the Hierarchy pressed in, the keys are the level's again.
    editor.select(Some(field));
    let hierarchy = editor.ui.find(editor.ui.root(), "hierarchy-panel").expect("the Hierarchy");
    let [x, y, w, h] = editor.ui.rect(hierarchy);
    click_at(&mut editor, &mut renderer, (x + w / 2.0, y + h - 12.0));
    let nodes = titles(&editor).len();
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(titles(&editor).len(), nodes, "the graph is left alone");
}

/// Types into the number field of the row labelled `label` under `within`, as a user does, and leaves it.
fn type_number_in(
    editor: &mut voxear_editor::Editor,
    renderer: &mut Renderer,
    within: NodeId,
    label: &str,
    text: &str,
) {
    let mut rows = Vec::new();
    with_class(&editor.ui, within, "prop", &mut rows);
    rows.retain(|&row| editor.ui.children(row).first().is_some_and(|&first| editor.ui.text(first) == label));
    let field = first_with(&editor.ui, rows[0], "value-input").unwrap_or_else(|| panic!("{label} has a field"));
    editor.ui.focus(field);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input(text);
    frame(editor, renderer);
    editor.ui.blur();
    frame(editor, renderer);
}

#[test]
fn open_graph_in_a_generators_inspector_shows_its_graph_and_a_preset_reshapes_it() {
    let Some((dir, mut renderer, mut editor, field, path)) = editor_with_generated_ground() else { return };
    // Selected in the Project, the generator shows its rows; Open Graph opens its graph.
    assert!(editor.inspect_path(&path));
    frame(&mut editor, &mut renderer);
    let open = first_with(&editor.ui, editor.ui.root(), "open-graph").expect("Open Graph");
    click(&mut editor.ui, open);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.generator().path(), Some(path.as_str()));
    assert_eq!(prop_rows(&editor.ui, "Preset").len(), 1, "the Inspector still shows the generator");

    // A node moved on the canvas is saved where it is, and makes no ground again: the same generator still.
    let land = node(&editor, "Landmass");
    let before = editor.generator().world().unwrap().graph().node(land).unwrap().at;
    let held = editor.game.content.world_gen(&path);
    let on = editor.generator().node_point(&editor.ui, land).unwrap();
    drag(&mut editor, &mut renderer, on, (on.0 - 80.0, on.1 + 40.0));
    let moved = saved(&dir, &path);
    let now = moved.graph().node(land).unwrap().at;
    assert!(now[0] < before[0] && now[1] > before[1] && now[0] % 10.0 == 0.0, "{before:?} to {now:?}");
    assert_eq!(moved.hash(), Preset::RollingHills.world().hash());
    let still = editor.game.content.world_gen(&path);
    assert!(held.zip(still).is_some_and(|(held, still)| std::sync::Arc::ptr_eq(&held, &still)), "not read as another");
    // One step to undo, however many frames the drag took.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.generator().world().unwrap().graph().node(land).unwrap().at, before);

    // Its preset picked in the Inspector, with the graph open: the landmass and climate reshaped where they
    // stand, a step of the graph's to undo, and the terrain's ground made again.
    click_at(&mut editor, &mut renderer, on);
    let canvas = first_with(&editor.ui, editor.ui.root(), "graph-canvas").expect("the canvas");
    let [left, top, ..] = editor.ui.rect(canvas);
    click_at(&mut editor, &mut renderer, (left + 20.0, top + 20.0));
    frame(&mut editor, &mut renderer);
    let presets = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    click(&mut editor.ui, presets);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    click(&mut editor.ui, options[2]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let alpine = saved(&dir, &path);
    assert_eq!(alpine.preset(), Some(Preset::Alpine));
    assert_eq!(alpine, Preset::Alpine.world().with_seed(alpine.seed()), "as the preset is, where its nodes stood");
    assert!(follows(&editor, field, &alpine));
    let row = prop_rows(&editor.ui, "Height")[0];
    let shown = editor.ui.text(first_with(&editor.ui, row, "value-input").unwrap());
    assert_eq!(shown, "900", "its rows show the preset's values");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(saved(&dir, &path).preset(), Some(Preset::RollingHills));
    assert!(follows(&editor, field, &Preset::RollingHills.world()));

    // Written outside the editor, the open graph reads its file again.
    let outside = Preset::Canyons.world().with_seed(7);
    std::fs::write(dir.path().join(&path), outside.to_text()).unwrap();
    editor.handle(&winit::event::WindowEvent::Focused(true));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(
        editor.generator().world().map(|world| (world.seed(), world.preset())),
        Some((7, Some(Preset::Canyons)))
    );
}

#[test]
fn a_new_generator_is_a_starter_graph_whose_preset_picks_another_and_an_angle_is_typed_in_degrees() {
    use voxear_terrain::generate::nodes::{STEEPEST, SlopeMask};
    let Some((dir, mut renderer, mut editor, field, path)) = editor_with_a_new_generator() else { return };
    let rolling = saved(&dir, &path);
    assert_eq!((rolling.starter(), rolling.as_landmass()), (Some("Rolling Hills"), None));
    assert!(follows(&editor, field, &rolling));

    // Its graph opens whole: many nodes, wired, none at fault, with Gullies among them.
    assert!(editor.open_path(&path, &mut renderer));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let titles = |editor: &voxear_editor::Editor| -> Vec<String> {
        editor.generator().drawn().unwrap().nodes.iter().map(|node| node.title.clone()).collect()
    };
    let nodes = titles(&editor);
    assert!(nodes.len() > 8 && nodes.contains(&"Gullies".to_string()) && nodes.contains(&"Output".to_string()));
    assert!(editor.generator().drawn().unwrap().nodes.iter().all(|node| node.fault.is_empty()));
    assert_eq!(
        row_labels(&editor.ui)[4..],
        ["Preset", "Seed", "Scale", "Sea level", "Height", "Gullies", "Temperature", "Humidity"]
    );

    // Canyons picked in its Preset row, with the graph open: that starter's graph on the canvas, its rows in
    // the Inspector, the terrain's ground made again, and one step of the graph's to undo.
    let presets = first_with(&editor.ui, prop_rows(&editor.ui, "Preset")[0], "value-choice").expect("its presets");
    click(&mut editor.ui, presets);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    assert_eq!(options.len(), 7, "five kinds of ground, the Caves and the Cave Level, and no Custom");
    click(&mut editor.ui, options[4]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let canyons = saved(&dir, &path);
    assert_eq!((canyons.starter(), canyons.seed()), (Some("Canyons"), 1));
    assert_ne!(titles(&editor), nodes);
    assert!(titles(&editor).contains(&"Terrace".to_string()));
    assert_eq!(prop_rows(&editor.ui, "Ravines").len(), 1, "Canyons' own row");
    assert!(follows(&editor, field, &canyons));
    settle(&mut editor, &mut renderer);
    shot(&mut renderer, "generator-starter.png");
    // Its Height typed is the starter's row still, so it is Canyons still.
    type_number(&mut editor, &mut renderer, "Height", "150");
    frame(&mut editor, &mut renderer);
    assert_eq!(saved(&dir, &path).starter(), Some("Canyons"));
    assert_eq!(prop_rows(&editor.ui, "Preset").len(), 1);

    // A Slope Mask added from the search, in the Masks folder: its angles are rows in degrees, kept as the
    // slope's rise over run. With a node more the graph is its own, and has no preset to pick.
    let panel = editor.ui.find(editor.ui.root(), "generator-panel").expect("the Generator panel");
    let add = first_with(&editor.ui, panel, "console-button").expect("Add Node");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(picker_row(&editor.ui, "Masks").is_some() && picker_row(&editor.ui, "Erosion").is_some());
    pick_node(&mut editor, &mut renderer, "Slope Mask");
    let mask = node(&editor, "Slope Mask");
    assert_eq!(editor.generator().picked(), [mask]);
    assert_eq!(row_labels(&editor.ui), ["Min", "Max", "Feather", "Step"]);
    let shown = |editor: &voxear_editor::Editor, label: &str| {
        let field = first_with(&editor.ui, prop_rows(&editor.ui, label)[0], "value-input").expect("its field");
        editor.ui.text(field)
    };
    let slope = |world: &WorldGen| match world.graph().node(mask).map(|node| node.kind.clone()) {
        Some(NodeKind::SlopeMask(slope)) => slope,
        other => panic!("a Slope Mask, not {other:?}"),
    };
    let SlopeMask { min, max, .. } = slope(&saved(&dir, &path));
    let degrees = |rise: f32| voxear_terrain::generate::nodes::degrees_of(rise);
    assert_eq!(shown(&editor, "Min").parse::<f32>().ok(), Some(degrees(min).round()), "its least, in degrees");
    assert_eq!(max, STEEPEST, "to straight up as it is made");
    assert_eq!(shown(&editor, "Max"), "90");
    type_number(&mut editor, &mut renderer, "Min", "45");
    frame(&mut editor, &mut renderer);
    let kept = slope(&saved(&dir, &path)).min;
    assert!((kept - 1.0).abs() < 1e-4, "45 degrees is a rise of 1 in 1, kept as {kept}");
    assert_eq!(shown(&editor, "Min"), "45");
    settle(&mut editor, &mut renderer);
    shot(&mut renderer, "generator-angles.png");
    assert_eq!(saved(&dir, &path).starter(), None, "a graph of its own now");
    // The canvas left for empty ground shows the generator, with no Preset row.
    let canvas = first_with(&editor.ui, panel, "graph-canvas").expect("the canvas");
    let [left, top, ..] = editor.ui.rect(canvas);
    click_at(&mut editor, &mut renderer, (left + 12.0, top + 12.0));
    frame(&mut editor, &mut renderer);
    assert!(editor.generator().picked().is_empty());
    assert_eq!((prop_rows(&editor.ui, "Preset").len(), prop_rows(&editor.ui, "Height").len()), (0, 1));

    // Undone back to the starter it was, and then to the one before: each a step of the graph's.
    editor.undo(&mut renderer);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(saved(&dir, &path).starter(), Some("Canyons"));
    assert_eq!(prop_rows(&editor.ui, "Preset").len(), 1, "its preset to pick again");
    editor.undo(&mut renderer);
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(saved(&dir, &path), rolling);
    assert!(follows(&editor, field, &rolling));
}

/// A cave room: a hall with a way out at its end.
const HALL: &str = r#"format = 1

[[obj]]
id = 1
name = "Hall"
[obj.cave_shape]
shape = "box"
size = [16.0, 5.0, 6.0]

[[obj]]
id = 2
name = "Way Out"
parent = 1
position = [8.0, 1.5, 0.0]
rotation = [0.0, 90.0, 0.0]
cave_exit = {}
"#;

/// What a row says after its label.
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

/// Everything a node and what is inside it says.
fn all_text(ui: &Ui, node: NodeId) -> String {
    let mut said = vec![ui.text(node)];
    said.extend(ui.children(node).into_iter().map(|child| all_text(ui, child)));
    said.retain(|text| !text.is_empty());
    said.join(" ")
}

/// The rooms the first Cave Network of a generator lists.
fn rooms_of(world: &WorldGen) -> Vec<voxear_terrain::generate::Room> {
    world.graph().nodes.iter().find_map(|node| node.kind.rooms()).expect("a Cave Network").to_vec()
}

/// Picks option `option` of a dropdown, as a user does.
fn choose(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, dropdown: NodeId, option: usize) {
    click(&mut editor.ui, dropdown);
    frame(editor, renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    click(&mut editor.ui, options[option]);
    frame(editor, renderer);
    frame(editor, renderer);
}

#[test]
fn a_cave_networks_rooms_are_picked_from_prefabs_and_read_again_when_a_prefab_changes() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_field();
    std::fs::write(dir.path().join("Prefabs/Hall.prefab"), HALL).unwrap();
    // Two generators with a level of caves: one to open, and one left closed that lists the hall already,
    // by its path, with nothing read from it yet.
    let level = voxear_terrain::generate::starters().pop().unwrap().world(1, 1.0);
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Cave.worldgen"), level.to_text()).unwrap();
    let mut listed = level.graph().clone();
    for node in &mut listed.nodes {
        if let Some(rooms) = node.kind.rooms_mut() {
            let prefab = Some("Prefabs/Hall.prefab".to_string());
            rooms.push(voxear_terrain::generate::Room { prefab, ..Default::default() });
        }
    }
    std::fs::write(dir.path().join("Worlds/Closed.worldgen"), level.with_graph(listed).to_text()).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    // The Cave Network picked in the open graph: its rows, and under them its rooms, none yet.
    assert!(editor.open_path("Worlds/Cave.worldgen", &mut renderer));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let network = node(&editor, "Cave Network");
    let on = editor.generator().node_point(&editor.ui, network).expect("the Cave Network node");
    click_at(&mut editor, &mut renderer, on);
    frame(&mut editor, &mut renderer);
    let labels = row_labels(&editor.ui);
    assert_eq!(labels[..4], ["Layout", "Spacing", "Depth", "Tunnel Width"], "{labels:?}");
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    assert!(first_with(&editor.ui, inspector, "room-row").is_none());
    // Add Room lists the project's prefabs; the hall picked is read at once, and saved with its numbers.
    let add = first_with(&editor.ui, inspector, "room-add").expect("Add Room");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    let mut options = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
    let offered: Vec<String> = options.iter().map(|&option| all_text(&editor.ui, option)).collect();
    let hall = offered.iter().position(|option| option == "Prefabs/Hall.prefab");
    let hall = hall.unwrap_or_else(|| panic!("{offered:?}"));
    assert_eq!(offered[0], "Add Room");
    click(&mut editor.ui, options[hall]);
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let rooms = rooms_of(&saved(&dir, "Worlds/Cave.worldgen"));
    assert_eq!(rooms.len(), 1);
    let guid = editor.game.content.db().guid_of("Prefabs/Hall.prefab").unwrap().to_string();
    assert_eq!(rooms[0].prefab.as_deref(), Some(guid.as_str()), "kept by its GUID");
    assert_eq!((rooms[0].shapes.len(), rooms[0].exits.len(), rooms[0].shapes[0].size), (1, 1, [16.0, 5.0, 6.0]));
    assert_eq!(rooms[0].exits[0].way, [1.0, 0.0, 0.0]);
    // Its row: the prefab, its weight, and what removes it. A weight typed is saved.
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let row = first_with(&editor.ui, inspector, "room-row").expect("the room's row");
    let prefab = first_with(&editor.ui, row, "room-prefab").unwrap();
    assert_eq!(all_text(&editor.ui, prefab), "Prefabs/Hall.prefab");
    let weight = first_with(&editor.ui, row, "room-weight").unwrap();
    assert_eq!(editor.ui.text(weight), "1");
    editor.ui.focus(weight);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("3");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(rooms_of(&saved(&dir, "Worlds/Cave.worldgen"))[0].weight, 3.0);
    settle(&mut editor, &mut renderer);
    shot(&mut renderer, "generator-cave-network.png");

    // The prefab changed outside the editor: both generators read the hall again, the open graph as a step
    // of its own, the closed one from its file.
    let longer = HALL.replace("[16.0, 5.0, 6.0]", "[20.0, 5.0, 6.0]");
    std::fs::write(dir.path().join("Prefabs/Hall.prefab"), longer).unwrap();
    editor.handle(&winit::event::WindowEvent::Focused(true));
    for _ in 0..5 {
        frame(&mut editor, &mut renderer);
    }
    for file in ["Worlds/Cave.worldgen", "Worlds/Closed.worldgen"] {
        let rooms = rooms_of(&saved(&dir, file));
        assert_eq!(rooms[0].shapes[0].size, [20.0, 5.0, 6.0], "{file} follows the prefab");
    }
    assert_eq!(rooms_of(&saved(&dir, "Worlds/Cave.worldgen"))[0].weight, 3.0, "and keeps what its graph had");
    assert_eq!(rooms_of(editor.generator().world().unwrap())[0].shapes[0].size, [20.0, 5.0, 6.0]);

    // The prefab opened and its hall made higher with the Inspector's Size, then saved: read again.
    assert!(editor.open_path("Prefabs/Hall.prefab", &mut renderer));
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let named = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, name)| name.0 == "Hall");
    let hall = named.map(|(entity, _)| entity).expect("the prefab's root");
    assert!(world.has::<voxear_terrain::generate::CaveShape>(hall));
    editor.select(Some(hall));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for label in ["Shape", "Mode", "Size", "Chance", "Inner Margin"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "a Cave Shape's {label}");
    }
    let mode = first_with(&editor.ui, prop_rows(&editor.ui, "Mode")[0], "value-choice").expect("Carve or Fill");
    assert_eq!(row_text(&editor.ui, prop_rows(&editor.ui, "Mode")[0]), "Carve");
    type_number(&mut editor, &mut renderer, "Chance", "0.5");
    shot(&mut renderer, "generator-cave-room-prefab.png");
    editor.save();
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    for file in ["Worlds/Cave.worldgen", "Worlds/Closed.worldgen"] {
        assert_eq!(rooms_of(&saved(&dir, file))[0].shapes[0].chance, 0.5, "{file} follows the prefab saved");
    }
    // Fill picked, and saved: the shape puts rock back.
    choose(&mut editor, &mut renderer, mode, 1);
    editor.save();
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    assert!(rooms_of(&saved(&dir, "Worlds/Closed.worldgen"))[0].shapes[0].fill);
    // Add Component offers both, under Terrain.
    let world = &editor.game.engine.world;
    let named = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, name)| name.0 == "Way Out");
    let exit = named.map(|(entity, _)| entity).expect("the exit");
    assert!(world.has::<voxear_terrain::generate::CaveExit>(exit));
    editor.select(Some(exit));
    frame(&mut editor, &mut renderer);
    let add = first_with(&editor.ui, editor.ui.root(), "add-component").expect("Add Component");
    click(&mut editor.ui, add);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("Cave");
    frame(&mut editor, &mut renderer);
    assert!(picker_row(&editor.ui, "Cave Shape").is_some(), "Cave Shape is offered");
    let row = picker_row(&editor.ui, "Cave Shape").unwrap();
    click(&mut editor.ui, row);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.game.engine.world.has::<voxear_terrain::generate::CaveShape>(exit));
}

#[test]
fn a_cave_network_of_rooms_made_by_rules_shows_their_rules_and_one_room_alone_in_the_preview() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_field();
    let starters = voxear_terrain::generate::starters();
    let caves = starters.iter().find(|starter| starter.name == "Caves").expect("the Caves starter").world(1, 1.0);
    std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
    std::fs::write(dir.path().join("Worlds/Caves.worldgen"), caves.to_text()).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    assert!(editor.open_path("Worlds/Caves.worldgen", &mut renderer));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let panel = editor.ui.find(editor.ui.root(), "generator-panel").expect("the Generator panel");
    let note = |editor: &voxear_editor::Editor| {
        let note = first_with(&editor.ui, panel, "asset-preview-hint").expect("the preview's note");
        editor.ui.text(note)
    };
    // The Cave Network picked: its rooms, each Generated with its rules as rows under it.
    let network = node(&editor, "Cave Network");
    let on = editor.generator().node_point(&editor.ui, network).expect("the Cave Network node");
    click_at(&mut editor, &mut renderer, on);
    frame(&mut editor, &mut renderer);
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let mut rows = Vec::new();
    with_class(&editor.ui, inspector, "room-row", &mut rows);
    assert_eq!(rows.len(), 2, "a hall and a chamber");
    let kind = first_with(&editor.ui, rows[0], "room-prefab").unwrap();
    assert_eq!(all_text(&editor.ui, kind), "Generated");
    let labels = row_labels(&editor.ui);
    for rule in ["Size", "Largest", "Shapes", "Height", "Floor", "Pillars", "Hanging Rock", "Ledges", "Exits Apart"] {
        assert_eq!(labels.iter().filter(|label| *label == rule).count(), 2, "{rule} for each room: {labels:?}");
    }
    // The preview shows a room by itself while the network is picked, and the ground again once it is not.
    for _ in 0..600 {
        if note(&editor) == "Room" && editor.generator().previews_settled() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(note(&editor), "Room");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    shot(&mut renderer, "generator-cave-room.png");
    // A rule typed is saved in the generator's file.
    type_number(&mut editor, &mut renderer, "Pillars", "7");
    for _ in 0..4 {
        frame(&mut editor, &mut renderer);
    }
    let rules: Vec<_> = rooms_of(&saved(&dir, "Worlds/Caves.worldgen")).iter().filter_map(|room| room.rules).collect();
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().any(|rules| rules.pillars == 7), "{rules:?}");
    let output = node(&editor, "Output");
    let on = editor.generator().node_point(&editor.ui, output).expect("the Output node");
    click_at(&mut editor, &mut renderer, on);
    for _ in 0..400 {
        if note(&editor) != "Room" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut editor, &mut renderer);
    }
    assert_ne!(note(&editor), "Room", "the ground again");
}
