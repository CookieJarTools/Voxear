//! The guns package in the editor, loaded as a folder package from the
//! folder `VOXEAR_GUNS_PACKAGE` names, on a copy of Hello: Create Object's
//! Rifle made as a gun that plays, its Gun's Kind filling in the kind's
//! numbers, a Gun Handling drawn by its own Inspector writing what is picked
//! into its file, a Gun Handling Action picked from a menu of the handlings'
//! own actions, and New Data Asset… making a Gun Handling. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without the package, .NET or a GPU.

use std::path::PathBuf;

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

/// A behaviour holding an action of a handling's own, which says once, as
/// the game plays, what the Rifle is and what its parts are.
const ARMOURY: &str = r#"using System.Linq;
using Voxear;
using Voxear.Guns;

namespace Hello;

public sealed class Armoury : Behaviour
{
	public GunHandlingAction Inspect;

	protected override void Update()
	{
		var rifle = Obj.Find("Rifle");
		if (!rifle.IsValid || rifle.GetBehaviour<Gun>() is not { } gun)
		{
			return;
		}
		var parts = rifle.Children.Select(child => child.GetBehaviour<GunPart>()).OfType<GunPart>().Select(part => part.Role);
		Log.Info($"rifle: {gun.Kind} {gun.Rounds}/{gun.Capacity} {gun.Chambered} {string.Join(",", parts)}");
		Enabled = false;
	}
}
"#;

/// A Gun Handling for any kind with an action of its own.
const RIFLE_HANDLING: &str = "type = \"Voxear.Guns.GunHandling\"\n\n[[fields.action]]\nname = \"Inspect\"\n";

const SIZE: (u32, u32) = (1280, 800);

#[test]
fn the_guns_package_makes_inspects_and_picks_its_guns_in_the_editor() {
    let Some(package) = std::env::var_os("VOXEAR_GUNS_PACKAGE").map(PathBuf::from) else {
        return eprintln!("skipping: VOXEAR_GUNS_PACKAGE names no guns package");
    };
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let package = package.canonicalize().expect("the guns package's folder");
    let packages = format!("[packages]\nvoxear-guns = {{ path = {:?} }}\n", package.display().to_string());
    std::fs::write(dir.path().join("packages.toml"), packages).unwrap();
    std::fs::write(dir.path().join("Scripts/Armoury.cs"), ARMOURY).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let armed =
        text.replacen("behaviours = [\"Hello.Spinner\"]", "behaviours = [\"Hello.Spinner\", \"Hello.Armoury\"]", 1);
    std::fs::write(&scene, armed).unwrap();
    std::fs::create_dir_all(dir.path().join("Guns")).unwrap();
    let handling = dir.path().join("Guns/Rifle.asset");
    std::fs::write(&handling, RIFLE_HANDLING).unwrap();
    let mut project = Project::open(dir.path()).unwrap();
    project.set_module(voxear_engine::modules::Module::Guns, false).unwrap();
    // The scripts' own lines too, which come through tracing from C#.
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, &mut renderer);

    // Create Object's Rifle, from the palette's New Rifle: a Gun with its bolt, clip and case parts.
    run_command(&mut editor, &mut renderer, "New Rifle");
    let rifle = obj(&editor, "Rifle").expect("the Rifle was made");
    assert_eq!(editor.selected(), Some(rifle));
    assert!(kept(&editor, rifle, "Voxear.Guns.Gun").is_some(), "a Gun");
    for part in ["Bolt", "Clip", "Case"] {
        let made = obj(&editor, part).unwrap_or_else(|| panic!("its {part}"));
        assert!(kept(&editor, made, "Voxear.Guns.GunPart").is_some(), "the {part} is a Gun Part");
    }

    // It plays: full, a round chambered, its parts found. Play waits while the scripts build for the Inspector.
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        editor.play(&mut renderer);
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    until(&mut editor, &mut renderer, "the rifle playing", |_| said(&log, "rifle: ").is_some());
    let line = said(&log, "rifle: ").unwrap();
    assert_eq!(line, "rifle: BoltActionRifle 5/5 True Bolt,Clip,Case");
    editor.stop(&mut renderer);
    until(&mut editor, &mut renderer, "stopping", |e| !e.is_playing());

    // Its Kind, from a menu of the kinds' names, fills in a Slide Pistol's numbers.
    editor.select(Some(rifle));
    until(&mut editor, &mut renderer, "the Kind menu", |e| dropdown_in(&e.ui, "Kind", "attr-dropdown").is_some());
    pick(&mut editor, &mut renderer, ("Kind", "attr-dropdown"), "Slide Pistol");
    until(&mut editor, &mut renderer, "the preset", |e| {
        let mass = kept(e, rifle, "Voxear.Guns.Gun").and_then(|f| f.get("mass").and_then(toml::Value::as_float));
        mass.is_some_and(|mass| (mass - 1.1).abs() < 1e-4)
    });
    let gun = kept(&editor, rifle, "Voxear.Guns.Gun").unwrap();
    assert!(gun.get("kind").and_then(toml::Value::as_str).is_some_and(|k| same(k, "SlidePistol")), "{gun:?}");

    // A Gun Handling opens in its own Inspector: its kind, its carrier from the arms clips' bones, its sections.
    assert!(editor.inspect_path("Guns/Rifle.asset"));
    until(&mut editor, &mut renderer, "the handling's inspector", |e| {
        prop_row(&e.ui, "Carrier").is_some() && dropdown_in(&e.ui, "Gun Kind", "attr-dropdown").is_some()
    });
    for section in ["Holding", "Clips", "Reload", "Cycle", "Check Magazine", "Check Chamber"] {
        assert!(label(&editor.ui, editor.ui.root(), section).is_some_and(|l| shown(&editor.ui, l)), "{section}");
    }
    let carrier = dropdown_in(&editor.ui, "Carrier", "value-choice").expect("the carrier's menu");
    assert_eq!(editor.ui.selected_option(carrier), 0, "the right hand, with no carrier named");

    // The kind picked is written to the file.
    pick(&mut editor, &mut renderer, ("Gun Kind", "attr-dropdown"), "Bolt-Action Rifle");
    let written = || std::fs::read_to_string(&handling).unwrap();
    until(&mut editor, &mut renderer, "the kind written", |_| key(&fields(&written()), "kind").is_some());
    let kind = key(&fields(&written()), "kind").and_then(|k| k.as_str().map(str::to_string)).unwrap();
    assert!(same(&kind, "BoltActionRifle"), "{}", written());
    assert!(written().contains("Inspect"), "its own action kept: {}", written());

    // Reload played from clips starts with a part, which shows in its section.
    pick(&mut editor, &mut renderer, ("Mode", "value-choice"), "Clips");
    let reload = |text: &str| key(&fields(text), "reload").and_then(|r| r.as_table().cloned()).unwrap_or_default();
    until(&mut editor, &mut renderer, "a part to start with", |_| {
        key(&reload(&written()), "part").and_then(|parts| parts.as_array().map(Vec::len)) == Some(1)
    });
    assert!(key(&reload(&written()), "mode").is_some_and(|mode| mode.as_str().is_some_and(|m| same(m, "Clips"))));
    until(&mut editor, &mut renderer, "the parts shown", |e| {
        prop_row(&e.ui, "Parts").is_some_and(|row| shown(&e.ui, row))
    });

    // A Gun Handling Action's menu lists the handling's own action by name and handling.
    let spinner = obj(&editor, "Spinner A").unwrap();
    editor.select(Some(spinner));
    until(&mut editor, &mut renderer, "the action's menu", |e| dropdown_in(&e.ui, "Inspect", "value-choice").is_some());
    pick(&mut editor, &mut renderer, ("Inspect", "value-choice"), "Inspect · Rifle");
    until(&mut editor, &mut renderer, "the action picked", |e| {
        kept(e, spinner, "Hello.Armoury").and_then(|f| key(&f, "inspect")).is_some()
    });
    let picked = key(&kept(&editor, spinner, "Hello.Armoury").unwrap(), "inspect").unwrap();
    let picked = picked.as_table().cloned().unwrap_or_default();
    assert_eq!(
        key(&picked, "name").and_then(|n| n.as_str().map(str::to_string)).as_deref(),
        Some("Inspect"),
        "{picked:?}"
    );
    assert!(
        key(&picked, "handling").and_then(|h| h.as_str().map(str::to_string)).is_some_and(|h| !h.is_empty()),
        "{picked:?}"
    );

    // New Data Asset… offers the Gun Handling under Guns, and makes one, which opens in its own Inspector.
    open_folder(&mut editor, &mut renderer, "Guns");
    let files = editor.ui.find(editor.ui.root(), "project-files").unwrap();
    let [x, y, w, h] = editor.ui.rect(files);
    editor.ui.pointer_move(x + w / 2.0, y + h * 0.8);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    let item = menu_item(&editor.ui, "New Data Asset…").expect("the menu makes data assets");
    click(&mut editor.ui, item);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let guns = picker_row(&editor.ui, "Guns").expect("the Guns folder its menu puts it in");
    click(&mut editor.ui, guns);
    frame(&mut editor, &mut renderer);
    let made = picker_row(&editor.ui, "Gun Handling").expect("named as its menu names it");
    click_across_frames(&mut editor, &mut renderer, made);
    frame(&mut editor, &mut renderer);
    let file = dir.path().join("Guns/New Gun Handling.asset");
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(text.starts_with("type = \"Voxear.Guns.GunHandling\""), "{text}");
    assert_eq!(editor.inspected_asset().map(|item| item.path.as_str()), Some("Guns/New Gun Handling.asset"));
    until(&mut editor, &mut renderer, "its inspector", |e| prop_row(&e.ui, "Carrier").is_some());
}

/// The fields a behaviour on `entity` keeps in the scene.
fn kept(editor: &voxear_editor::Editor, entity: voxear_ecs::Entity, behaviour: &str) -> Option<toml::Table> {
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(entity)?;
    kept.0.iter().find(|spec| spec.type_name == behaviour).map(|spec| (*spec.fields).clone())
}

/// A data asset file's fields.
fn fields(text: &str) -> toml::Table {
    let table: toml::Table = toml::from_str(text).unwrap();
    table.get("fields").and_then(toml::Value::as_table).cloned().unwrap_or_default()
}

/// The value of a table's key as a file writes it, case and underscores aside.
fn key(table: &toml::Table, name: &str) -> Option<toml::Value> {
    table.iter().find(|(written, _)| same(written, name)).map(|(_, value)| value.clone())
}

/// Whether two names are one, as a scene's keys and values match: case and underscores aside.
fn same(a: &str, b: &str) -> bool {
    let plain = |name: &str| name.replace('_', "").to_lowercase();
    plain(a) == plain(b)
}

/// The latest line a script logged that starts with `words`.
fn said(log: &LogCapture, words: &str) -> Option<String> {
    let lines = log.lock().unwrap();
    lines.lines.iter().rev().map(|line| line.message.clone()).find(|message| message.starts_with(words))
}

/// The first shown dropdown of `class` in an Inspector row labelled `label`.
fn dropdown_in(ui: &Ui, label: &str, class: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    let labelled =
        rows.into_iter().filter(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label));
    labelled.into_iter().find_map(|row| {
        let mut found = Vec::new();
        with_class(ui, row, class, &mut found);
        found.into_iter().find(|&node| shown(ui, node))
    })
}

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

/// Runs a command from the command bar, as typed and entered.
fn run_command(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, words: &str) {
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input(words);
    frame(editor, renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(editor, renderer);
    frame(editor, renderer);
}

fn obj(editor: &voxear_editor::Editor, name: &str) -> Option<voxear_ecs::Entity> {
    let world = &editor.game.engine.world;
    let objs = world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().without::<voxear_scene::Destroying>();
    objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e)
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The first label anywhere in `node` whose text is `text`.
fn label(ui: &Ui, node: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(node) == text && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label(ui, child, text))
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
}

/// Shown when it and everything it is in are.
fn shown(ui: &Ui, node: NodeId) -> bool {
    std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n))
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// Opens the dropdown of `class` in the Inspector row labelled `row`, and
/// picks `option` from its menu, opening it again while the Inspector is
/// built again under it.
fn pick(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, (row, class): (&str, &str), option: &str) {
    let started = std::time::Instant::now();
    loop {
        assert!(started.elapsed().as_secs() < 60, "no {option} in the {row} menu");
        frame(editor, renderer);
        let Some(dropdown) = dropdown_in(&editor.ui, row, class) else { continue };
        scroll_to(editor, renderer, dropdown);
        let Some(dropdown) = dropdown_in(&editor.ui, row, class) else { continue };
        click(&mut editor.ui, dropdown);
        frame(editor, renderer);
        let mut options = Vec::new();
        with_class(&editor.ui, editor.ui.root(), "dropdown-option", &mut options);
        let found = options
            .into_iter()
            .filter(|&node| shown(&editor.ui, node))
            .find_map(|node| label(&editor.ui, node, option));
        if let Some(found) = found {
            click(&mut editor.ui, found);
            frame(editor, renderer);
            frame(editor, renderer);
            return;
        }
    }
}

/// Scrolls what `node` is in so that it shows.
fn scroll_to(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let scrolled = std::iter::successors(editor.ui.parent(node), |&n| editor.ui.parent(n))
        .find(|&n| editor.ui.scroll_max(n)[1] > 0.0);
    if let Some(parent) = scrolled {
        let ([_, top, _, _], [_, y, _, _]) = (editor.ui.rect(parent), editor.ui.rect(node));
        let [x, now] = editor.ui.scroll_offset(parent);
        editor.ui.set_scroll(parent, [x, (now + y - top - 20.0).max(0.0)]);
        frame(editor, renderer);
    }
}

/// Opens a folder of the Project panel, as a double click on its tile does.
fn open_folder(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str) {
    let mut tiles = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "tile", &mut tiles);
    let folder = tiles
        .into_iter()
        .find(|&t| editor.ui.is_visible(t) && editor.ui.children(t).get(1).is_some_and(|&n| editor.ui.text(n) == name))
        .unwrap_or_else(|| panic!("a tile for {name}"));
    let [x, y, w, h] = editor.ui.rect(folder);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    for _ in 0..2 {
        editor.ui.pointer_button(true);
        editor.ui.pointer_button(false);
    }
    frame(editor, renderer);
}

/// The open menu's item that says `label`.
fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

/// The object picker's row that says `label`.
fn picker_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let list = ui.find(ui.root(), "picker-list")?;
    let rows = ui.children(list).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r));
    rows.into_iter().find(|&row| ui.text(ui.children(row)[1]) == label)
}

/// A click whose press and release come frames apart, as a hand's do.
fn click_across_frames(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    for _ in 0..3 {
        frame(editor, renderer);
    }
    editor.ui.pointer_button(false);
}
