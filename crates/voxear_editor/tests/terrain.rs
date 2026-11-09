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

#[test]
fn a_terrain_is_added_on_an_asset_of_its_own_changed_undone_and_saved() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_with_a_field();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let ground = obj(&editor, "Field");
    editor.select(Some(ground));
    frame(&mut editor, &mut renderer);

    // Add Component makes flat ground on a new asset named for its Obj.
    add_component(&mut editor, &mut renderer, "Terrain");
    let world = &editor.game.engine.world;
    let terrain = world.get::<Terrain>(ground).expect("a Terrain");
    assert_eq!(terrain.heights.size(), [512.0, 512.0]);
    let asset = world.get::<TerrainAsset>(ground).expect("on an asset").clone();
    assert!(dir.path().join("Terrains/Field.terrain").is_file());
    for label in
        ["Data", "Size", "Resolution", "Heightmap", "Material", "Texture size", "Detail distance", "Cast shadows"]
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
    assert_eq!(voxear_terrain::file::read_heights(&heights).unwrap().size(), [128.0, 128.0]);
    let scene = std::fs::read_to_string(dir.path().join("Scenes/Day.scene")).unwrap();
    assert!(scene.contains("[obj.terrain]") && scene.contains(&asset.guid.to_string()), "{scene}");
}
