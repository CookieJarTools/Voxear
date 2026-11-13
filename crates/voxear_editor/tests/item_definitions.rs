//! Items, recipes and a block's rules in the Inspector, on a copy of Hello
//! with a tool kind, two items, a recipe and a block: the engine's own
//! Definitions drawn as a game's are, a recipe's grid of items, a block's
//! Breaking, Drops and Sounds, its Tool picked from the project's tool
//! kinds alone, and the item that places it made for it. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_terrain::blocks::block::BlockDef;
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
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
    dir
}

/// A renderer, or why the test cannot run.
fn renderer() -> Option<Renderer> {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return None;
    }
    let renderer = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok();
    if renderer.is_none() {
        eprintln!("skipping: no usable GPU");
    }
    renderer
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Frames until `done`, which must come within a minute.
fn until(
    editor: &mut voxear_editor::Editor,
    renderer: &mut Renderer,
    what: &str,
    done: impl Fn(&voxear_editor::Editor) -> bool,
) {
    let started = std::time::Instant::now();
    while !done(editor) {
        assert!(started.elapsed().as_secs() < 60, "{what} never happened");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
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

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// The Inspector's visible rows labelled `label`.
fn prop_rows(ui: &Ui, label: &str) -> Vec<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.find(ui.root(), "inspector").expect("the Inspector"), "prop", &mut rows);
    let shown = |node: NodeId| std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n));
    rows.retain(|&row| shown(row) && ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
    rows
}

/// What the object picker offers, by its rows' names.
fn offered(ui: &Ui) -> Vec<(String, NodeId)> {
    let Some(list) = ui.find(ui.root(), "picker-list") else { return Vec::new() };
    let rows = ui.children(list).into_iter().filter(|&row| ui.has_class(row, "list-row") && ui.is_visible(row));
    rows.map(|row| (ui.text(ui.children(row)[1]), row)).collect()
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

#[test]
fn items_recipes_and_a_blocks_rules_are_edited_in_the_inspector_and_a_block_is_given_its_item() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Tools/Pickaxe.asset", "type = \"Voxear.ToolKind\"\n");
    write("Items/Stick.asset", "type = \"Voxear.Item\"\n");
    write("Items/Pick.asset", "type = \"Voxear.Item\"\n\n[fields]\ntool = \"Tools/Pickaxe.asset\"\nspeed = 4.0\n");
    let recipe = "type = \"Voxear.Recipe\"\n\n[fields]\nresult = \"Items/Pick.asset\"\n\
                  top = \"Items/Stick.asset\"\nmiddle = \"Items/Stick.asset\"\n";
    write("Recipes/Pick.asset", recipe);
    write("Blocks/Stone.block", "color = [0.5, 0.5, 0.52]\nhardness = 1.5\n");

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // An item is the engine's own Definition, drawn as a game's are: its fields under their labels.
    assert!(editor.inspect_path("Items/Pick.asset"));
    until(&mut editor, &mut renderer, "the item's fields", |e| !prop_rows(&e.ui, "Stack").is_empty());
    for label in ["Icon", "Stack", "Places", "Leaves", "Tool", "Tier", "Speed", "Durability"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "an item's {label}");
    }
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "item-inspector.png");

    // A recipe: its shape, its grid a row at a time, and what it makes.
    assert!(editor.inspect_path("Recipes/Pick.asset"));
    until(&mut editor, &mut renderer, "the recipe's fields", |e| !prop_rows(&e.ui, "Result").is_empty());
    for label in ["Shape", "Result", "Count", "Station", "Time"] {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "a recipe's {label}");
    }
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "recipe-inspector.png");
    // Its grid is three rows of three places, side by side, each a picker of items with no label.
    let mut grid = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "attr-horizontal", &mut grid);
    grid.retain(|&row| editor.ui.is_visible(row));
    assert_eq!(grid.len(), 3, "three rows");
    let across: Vec<f32> = editor.ui.children(grid[0]).iter().map(|&cell| editor.ui.rect(cell)[0]).collect();
    assert!(across.len() == 3 && across[0] < across[1] && across[1] < across[2], "three places across: {across:?}");

    // A block's own Inspector: its look, then its rules.
    assert!(editor.inspect_path("Blocks/Stone.block"));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    for label in
        ["Color", "Material", "Overlap", "Liquid", "Hardness", "Unbreakable", "Tool", "Tier", "Break", "Place", "Step", "Data"]
    {
        assert_eq!(prop_rows(&editor.ui, label).len(), 1, "a block's {label}");
    }
    shot(&mut renderer, "block-inspector.png");
    // Its Tool is picked from the project's tool kinds: the pickaxe is offered, the items are not.
    let mut arrows = Vec::new();
    with_class(&editor.ui, prop_rows(&editor.ui, "Tool")[0], "picker-arrow", &mut arrows);
    click(&mut editor.ui, arrows[0]);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let names: Vec<String> = offered(&editor.ui).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["None", "Pickaxe.asset"], "tool kinds alone");
    let pickaxe = offered(&editor.ui).into_iter().find(|(name, _)| name == "Pickaxe.asset").unwrap().1;
    click(&mut editor.ui, pickaxe);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let read = || BlockDef::parse(&std::fs::read_to_string(dir.path().join("Blocks/Stone.block")).unwrap()).unwrap();
    let guid_of =
        |editor: &voxear_editor::Editor, path: &str| editor.game.content.db().guid_of(path).unwrap().to_string();
    assert_eq!(read().tool, Some(guid_of(&editor, "Tools/Pickaxe.asset")));
    assert_eq!(read().hardness, 1.5, "the rest as it was");

    // Make Item makes the item it is carried as, which places it, and the block drops it.
    let mut buttons = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "collection-add", &mut buttons);
    let said = |editor: &voxear_editor::Editor, text: &str| {
        let mut buttons = Vec::new();
        with_class(&editor.ui, editor.ui.root(), "collection-add", &mut buttons);
        buttons.into_iter().find(|&button| editor.ui.is_visible(button) && editor.ui.text(button) == text)
    };
    let make = said(&editor, "Make Item").expect("Make Item, while no item places the block");
    click(&mut editor.ui, make);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let item = std::fs::read_to_string(dir.path().join("Items/Stone.asset")).expect("the item made");
    let stone = guid_of(&editor, "Blocks/Stone.block");
    assert!(item.starts_with("type = \"Voxear.Item\"") && item.contains(&format!("places = \"{stone}\"")), "{item}");
    let drops = read().drops;
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].item, Some(guid_of(&editor, "Items/Stone.asset")));
    assert_eq!((drops[0].count, drops[0].chance), ([1, 1], 1.0));
    assert!(said(&editor, "Make Item").is_none(), "it has its item now");
    assert_eq!(prop_rows(&editor.ui, "Chance").len(), 1, "its drop's rows");
    shot(&mut renderer, "block-inspector-drops.png");
}
