//! Drawers and an inspector written in C# only in `Build`, with handlers
//! that set `Value` or change `Target`, `Bind` for what they show and a
//! `NextField` where the usual control goes, on a copy of Hello with an Obj
//! that shows each field twice, without a drawer and with one, and a Tower
//! without an inspector and with one. With `VOXEAR_TEST_OUTPUT` set, each
//! pair is saved as the pictures the manual's drawer pages show. A file of
//! its own, so it runs in a process of its own, since .NET starts only once
//! per process. Skips itself without a GPU or .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const SCRIPTS: [(&str, &str); 9] = [
    (
        "Showcase.cs",
        "using Voxear;

namespace Hello;

public sealed class Showcase : Behaviour
{
	[LabelText(\"Bonus\")]
	public int PlainBonus = 10;

	[Stepper(5)]
	public int Bonus = 10;

	[LabelText(\"Health\")]
	public PlainStat PlainHealth = new PlainStat { Value = 3, Max = 10 };

	public Stat Health = new Stat { Value = 3, Max = 10 };
}
",
    ),
    (
        "PlainStat.cs",
        "using System;

namespace Hello;

[Serializable]
public struct PlainStat
{
	public int Value;
	public int Max;
}
",
    ),
    (
        "PlainTower.cs",
        "using Voxear;

namespace Hello.Plain;

public sealed class Tower : Behaviour
{
	public int Floors = 3;
}
",
    ),
    (
        "Stat.cs",
        "using System;

namespace Hello;

[Serializable]
public struct Stat
{
	public int Value;
	public int Max;
}
",
    ),
    (
        "StatDrawer.cs",
        r#"using Voxear;

namespace Hello;

public sealed class StatDrawer : ValueDrawer<Stat>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label().Bind(() => $"{Value.Value} / {Value.Max}"));
		root.Add(new Button("Fill", () => Value = Value with { Value = Value.Max }) { Tooltip = "Fills the bar" });
	}
}
"#,
    ),
    (
        "StepperAttribute.cs",
        "using System;

namespace Hello;

[AttributeUsage(AttributeTargets.Field)]
public sealed class StepperAttribute : Attribute
{
	public StepperAttribute(int step)
	{
		Step = step;
	}

	public int Step { get; }
}
",
    ),
    (
        "StepperDrawer.cs",
        r#"using Voxear;

namespace Hello;

public sealed class StepperDrawer : AttributeDrawer<StepperAttribute, int>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Button("-").OnClicked(() => Value -= Attribute.Step));
		root.Add(new NextField());
		root.Add(new Button("+").OnClicked(() => Value += Attribute.Step));
	}
}
"#,
    ),
    (
        "Tower.cs",
        "using Voxear;

namespace Hello;

public sealed class Tower : Behaviour
{
	public int Floors = 3;
}
",
    ),
    (
        "TowerInspector.cs",
        r#"using Voxear;

namespace Hello;

public sealed class TowerInspector : BehaviourInspector<Tower>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label().Bind(() => $"{Target.Floors} floors on {Target.Obj.Name}"));
		root.Add(new Button("Add a floor", () => Target.Floors++));
		root.Add(new NextField());
	}
}
"#,
    ),
];

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against, with the drawers, and an Obj with the fields
/// they draw and both Towers.
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
    let scripts = dir.path().join("Scripts");
    for (name, text) in SCRIPTS {
        std::fs::write(scripts.join(name), text).unwrap();
    }
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let showcase = "\n[[obj]]\nid = 900\nname = \"Showcase\"\nbehaviours = [\"Hello.Showcase\", \"Hello.Plain.Tower\", \"Hello.Tower\"]\n";
    std::fs::write(&scene, format!("{text}{showcase}")).unwrap();
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

fn all_with_class(ui: &Ui, class: &str) -> Vec<NodeId> {
    let mut found = Vec::new();
    with_class(ui, ui.root(), class, &mut found);
    found
}

/// The first element in `node`, or `node` itself, whose own text is `text`.
fn reading(ui: &Ui, node: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(node) == text && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| reading(ui, child, text))
}

fn says(editor: &voxear_editor::Editor, text: &str) -> bool {
    reading(&editor.ui, editor.ui.root(), text).is_some()
}

/// The drawer whose elements include one reading `text`.
fn drawer_with(ui: &Ui, text: &str) -> Option<NodeId> {
    all_with_class(ui, "field-drawer").into_iter().find(|&drawer| reading(ui, drawer, text).is_some())
}

fn click(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, node: NodeId) {
    let [x, y, w, h] = editor.ui.rect(node);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
    frame(editor, renderer);
}

/// The text of the Inspector's own field inside the Stepper's drawer.
fn bonus(ui: &Ui, stepper: NodeId) -> String {
    let mut fields = Vec::new();
    with_class(ui, stepper, "value-input", &mut fields);
    assert_eq!(fields.len(), 1, "the Inspector's own field is inside the drawer's NextField");
    ui.text(fields[0])
}

#[test]
fn drawers_built_in_code_change_values_from_their_handlers() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let world = &editor.game.engine.world;
    let showcase = world
        .query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>()
        .into_iter()
        .find(|(_, n)| n.0 == "Showcase")
        .unwrap()
        .0;
    editor.select(Some(showcase));

    // Each drawer builds once the scripts have started; the Stepper's NextField takes the usual field as it draws.
    until(&mut editor, &mut renderer, "the drawers drawing", |e| {
        drawer_with(&e.ui, "+").is_some_and(|stepper| all_with_class_in(&e.ui, stepper, "value-input") == 1)
    });
    assert!(!editor.is_playing(), "drawing does not play");
    assert!(says(&editor, "3 / 10"), "the bound label shows the Stat");
    let stepper = drawer_with(&editor.ui, "+").unwrap();
    assert_eq!(bonus(&editor.ui, stepper), "10");
    until(&mut editor, &mut renderer, "the Tower's inspector drawing", |e| says(e, "3 floors on Showcase"));
    pictures(&editor, &mut renderer);

    // A handler that sets Value changes the field, and the bound label follows.
    let up = reading(&editor.ui, stepper, "+").unwrap();
    click(&mut editor, &mut renderer, up);
    until(&mut editor, &mut renderer, "the step added", |e| drawer_with(&e.ui, "+").is_some_and(|s| bonus(&e.ui, s) == "15"));
    let stat = drawer_with(&editor.ui, "Fill").unwrap();
    let fill = reading(&editor.ui, stat, "Fill").unwrap();
    click(&mut editor, &mut renderer, fill);
    until(&mut editor, &mut renderer, "the Stat filled", |e| says(e, "10 / 10"));

    // The Fill button's tooltip is the element's own, for the editor to show where the pointer rests.
    let fill = reading(&editor.ui, drawer_with(&editor.ui, "Fill").unwrap(), "Fill").unwrap();
    let [x, y, w, h] = editor.ui.rect(fill);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.hovered_tooltip().map(|(text, _)| text).as_deref(), Some("Fills the bar"));

    // A change from outside, as undo, shows in the bound label without the drawer doing anything.
    editor.undo(&mut renderer);
    until(&mut editor, &mut renderer, "the undo shown", |e| says(e, "3 / 10"));

    // The Tower's inspector, built in code, holds its usual fields in its NextField and changes them through Target.
    let raise = reading(&editor.ui, editor.ui.root(), "Add a floor").unwrap();
    let inspector = drawer_with(&editor.ui, "Add a floor").unwrap();
    until(&mut editor, &mut renderer, "the Tower's fields in its NextField", |e| {
        all_with_class(&e.ui, "prop").into_iter().any(|row| {
            e.ui.is_within(row, inspector) && e.ui.children(row).first().is_some_and(|&l| e.ui.text(l) == "Floors")
        })
    });
    click(&mut editor, &mut renderer, raise);
    until(&mut editor, &mut renderer, "the floor added", |e| says(e, "4 floors on Showcase"));
}

/// Each field and Tower without its drawer and with it, as the manual's
/// pictures, when `VOXEAR_TEST_OUTPUT` names a folder.
fn pictures(editor: &voxear_editor::Editor, renderer: &mut Renderer) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let ui = &editor.ui;
    let rows = |label: &str| {
        let mut rows: Vec<NodeId> = all_with_class(ui, "prop")
            .into_iter()
            .filter(|&row| ui.children(row).first().is_some_and(|&l| ui.text(l) == label) && shown(ui, row))
            .collect();
        rows.sort_by(|&a, &b| ui.rect(a)[1].total_cmp(&ui.rect(b)[1]));
        rows
    };
    let (bonus, health) = (rows("Bonus"), rows("Health"));
    let mut towers: Vec<NodeId> = all_with_class(ui, "section")
        .into_iter()
        .filter(|&section| reading(ui, section, "Tower").is_some() && shown(ui, section))
        .collect();
    towers.sort_by(|&a, &b| ui.rect(a)[1].total_cmp(&ui.rect(b)[1]));
    assert!(bonus.len() == 2 && health.len() == 2 && towers.len() == 2, "each without its drawer and with it");
    let between = |top: NodeId, below: NodeId| {
        let ([x, y, w, _], [_, next, _, _]) = (ui.rect(top), ui.rect(below));
        [x, y, w, next - y]
    };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
    let save = |name: &str, [x, y, w, h]: [f32; 4]| {
        let (left, top) = ((x - 4.0).max(0.0) as u32, (y - 2.0).max(0.0) as u32);
        let (right, bottom) = (((x + w + 4.0) as u32).min(width), ((y + h + 2.0) as u32).min(height));
        let part = image::imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image();
        part.save(std::path::Path::new(&dir).join(name)).unwrap();
    };
    save("step-before.png", between(bonus[0], bonus[1]));
    save("step-after.png", between(bonus[1], health[0]));
    save("health-before.png", between(health[0], health[1]));
    save("health-after.png", ui.rect(health[1]));
    save("tower-before.png", ui.rect(towers[0]));
    save("tower-after.png", ui.rect(towers[1]));
}

/// Shown when it and everything it is in are.
fn shown(ui: &Ui, node: NodeId) -> bool {
    std::iter::successors(Some(node), |&n| ui.parent(n)).all(|n| ui.is_visible(n))
}

fn all_with_class_in(ui: &Ui, node: NodeId, class: &str) -> usize {
    let mut found = Vec::new();
    with_class(ui, node, class, &mut found);
    found.len()
}
