//! A VoxShade shader in the editor, on a copy of Hello: opened from the
//! Project, it shows on the VoxShade panel's canvas with each node's
//! cost; a node is added from the search and wired by a drag, a value is
//! typed into the side's rows, each change is saved and compiled into the
//! project's shaders, and Ctrl+Z takes it back. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{Key, Modifiers, NodeId, Ui};
use voxear_voxshade::{Graph, NodeKind, starters};

const SIZE: (u32, u32) = (1600, 900);
const PATH: &str = "Shaders/Glow.voxshade";

/// A copy of Hello, inside the repository, with a plain lit VoxShade shader named Glow.
fn hello_with_a_graph() -> tempfile::TempDir {
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
    let toml = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, text.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
    std::fs::create_dir_all(dir.path().join("Shaders")).unwrap();
    std::fs::write(dir.path().join(PATH), starters::lit("Glow").to_toml()).unwrap();
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

fn click_at(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, point: (f32, f32)) {
    editor.ui.pointer_move(point.0, point.1);
    editor.ui.pointer_button(true);
    frame(editor, renderer);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
}

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
    frames(editor, renderer, 2);
}

fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
}

/// The graph as its file now has it.
fn saved(dir: &tempfile::TempDir) -> Graph {
    Graph::from_toml(&std::fs::read_to_string(dir.path().join(PATH)).unwrap()).unwrap()
}

/// The open graph's first node of this kind.
fn node(editor: &voxear_editor::Editor, kind: &NodeKind) -> voxear_voxshade::NodeId {
    let graph = editor.voxshade().graph().expect("a graph is open");
    graph.nodes.iter().find(|node| node.kind == *kind).map(|node| node.id).expect("the node")
}

#[test]
fn a_voxshade_is_opened_wired_set_saved_compiled_and_undone() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_graph();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    // The project's shaders have it compiled already, with its properties, as a `.shader`'s.
    let compiled = |editor: &voxear_editor::Editor| {
        let shaders = editor.game.content.shaders();
        let found = shaders.shaders.iter().find(|shader| shader.path == PATH).expect("among the shaders");
        found.info.clone().map(|info| (info, found.source.clone()))
    };
    let (info, _) = compiled(&editor).expect("it compiles");
    let names: Vec<&str> = info.properties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!((info.name.as_str(), names), ("Glow", vec!["tint", "picture"]));

    // Opened as a double click on it in the Project does, on the panel beside the Scene view.
    assert!(editor.open_path(PATH, &mut renderer));
    frames(&mut editor, &mut renderer, 3);
    assert_eq!(editor.voxshade().path(), Some(PATH));
    let panel = editor.ui.find(editor.ui.root(), "voxshade-panel").expect("the VoxShade panel");
    assert!(editor.ui.rect(panel)[2] > 400.0, "showing: {:?}", editor.ui.rect(panel));
    let drawn = editor.voxshade().drawn().unwrap().clone();
    let titles: Vec<&str> = drawn.nodes.iter().map(|n| n.title.as_str()).collect();
    assert!(titles.contains(&"tint") && titles.contains(&"Sample picture") && titles.contains(&"Surface Output"));
    let sample = drawn.nodes.iter().find(|n| n.title == "Sample picture").unwrap();
    assert!(sample.detail.contains("1 picture"), "its cost on it: {}", sample.detail);

    // A context click on the canvas opens the search; Time picked is added where it was asked for.
    let canvas = first_with(&editor.ui, panel, "graph-canvas").expect("the canvas");
    let [left, top, _, tall] = editor.ui.rect(canvas);
    editor.ui.pointer_move(left + 80.0, top + tall - 60.0);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frames(&mut editor, &mut renderer, 2);
    assert!(editor.picker_open(), "the search for a node to add");
    for folder in ["Inputs", "Math", "Properties", "Outputs"] {
        assert!(picker_row(&editor.ui, folder).is_some(), "{folder} in the search");
    }
    editor.ui.text_input("Time");
    frame(&mut editor, &mut renderer);
    let row = picker_row(&editor.ui, "Time").expect("Time is offered");
    click(&mut editor.ui, row);
    frames(&mut editor, &mut renderer, 3);
    let time = node(&editor, &NodeKind::Input { input: voxear_voxshade::Input::Time });

    // Dragged from Time's output into the Surface Output's Emission, a wire, saved and compiled.
    let out = node(&editor, &NodeKind::SurfaceOutput);
    let from = editor.voxshade().port_point(&editor.ui, time, true, 0).expect("Time's output");
    let to = editor.voxshade().port_point(&editor.ui, out, false, 5).expect("the Emission input");
    drag(&mut editor, &mut renderer, from, to);
    frames(&mut editor, &mut renderer, 2);
    let wired = saved(&dir);
    assert!(wired.links.iter().any(|l| l.from == time && l.to == out && l.input == 5), "{:?}", wired.links);
    let summary = editor.voxshade().compiled().unwrap().as_ref().map(|c| c.source.clone()).unwrap();
    assert!(
        summary.contains("float3(input.time, input.time, input.time)") && summary.contains("surface.emission = "),
        "{summary}"
    );

    // The Surface Output picked, its Alpha, with no wire into it, typed in the side's rows.
    let on = editor.voxshade().node_point(&editor.ui, out).expect("the Surface Output");
    click_at(&mut editor, &mut renderer, on);
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(editor.voxshade().picked(), [out]);
    let field = editor.voxshade().row_control(|field| field == "Literal(1)").expect("the Alpha row");
    editor.ui.focus(field);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("0.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frames(&mut editor, &mut renderer, 3);
    let set = saved(&dir);
    shot(&mut renderer, "voxshade panel.png");
    let alpha = set.node(out).unwrap().literals.iter().find(|l| l.input == 1).map(|l| l.value[0]);
    assert_eq!(alpha, Some(0.5));
    // The project's shaders compiled again from the file, its Slang setting what was typed.
    let (_, source) = compiled(&editor).expect("it still compiles");
    assert!(source.contains("surface.alpha = 0.5;"), "{source}");

    // Ctrl+Z is the graph's while its panel was the last pressed in: the value, then the wire.
    editor.undo(&mut renderer);
    frames(&mut editor, &mut renderer, 3);
    assert!(!saved(&dir).node(out).unwrap().literals.iter().any(|l| l.input == 1), "the value taken back");
    editor.undo(&mut renderer);
    frames(&mut editor, &mut renderer, 3);
    assert!(!saved(&dir).links.iter().any(|l| l.to == out && l.input == 5), "the wire taken back");
    editor.redo(&mut renderer);
    frames(&mut editor, &mut renderer, 3);
    assert!(saved(&dir).links.iter().any(|l| l.to == out && l.input == 5), "and made again");
}
