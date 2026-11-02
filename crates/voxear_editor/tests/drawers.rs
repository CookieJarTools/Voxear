//! Drawers written in C#, on a copy of Hello whose Game Manager gains two
//! fields: one its type's `ValueDrawer` draws whole, with elements it
//! builds in code, and one an `AttributeDrawer` draws around the
//! Inspector's own control, from markup. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without a GPU or .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const FIELDS: &str = "
		[Stepper(5)]
		public int Bonus = 10;

		public Stat Health = new Stat { Value = 3, Max = 10 };
";

const SCRIPTS: [(&str, &str); 4] = [
    (
        "Stat.cs",
        "using System;

namespace Hello
{
	[Serializable]
	public struct Stat
	{
		public int Value;
		public int Max;
	}
}
",
    ),
    (
        "StatDrawer.cs",
        r#"using Voxear;

namespace Hello;

public sealed class StatDrawer : ValueDrawer<Stat>
{
	private Label text = null!;
	private Button fill = null!;

	protected override void Build(UiElement root)
	{
		text = root.Add(new Label());
		fill = root.Add(new Button("Fill"));
	}

	protected override Stat Draw(Stat value, UiDocument ui)
	{
		if (fill.WasClicked)
		{
			value.Value = value.Max;
		}
		text.Text = $"{value.Value} / {value.Max}";
		return value;
	}
}
"#,
    ),
    (
        "StepperAttribute.cs",
        "using System;

namespace Hello
{
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class StepperAttribute : Attribute
	{
		public StepperAttribute(int step)
		{
			Step = step;
		}

		public int Step { get; }
	}
}
",
    ),
    (
        "StepperDrawer.cs",
        r#"using Voxear;

namespace Hello
{
	public sealed class StepperDrawer : AttributeDrawer<StepperAttribute, int>
	{
		public override string Markup => "<vui><Button name=\"down\">-</Button><Panel name=\"next\" /><Button name=\"up\">+</Button></vui>";

		protected override int Draw(int value, UiDocument ui)
		{
			if (ui.Find("up")!.WasClicked)
			{
				value += Attribute.Step;
			}
			if (ui.Find("down")!.WasClicked)
			{
				value -= Attribute.Step;
			}
			return value;
		}
	}
}
"#,
    ),
];

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against, with the drawers and the fields they draw.
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
    let manager = scripts.join("GameManager.cs");
    let text = std::fs::read_to_string(&manager).unwrap();
    let cycle = "private LevelStop[] cycle = [];\n";
    assert!(text.contains(cycle), "the Game Manager's cycle is where the fields go");
    std::fs::write(&manager, text.replace(cycle, &format!("{cycle}{FIELDS}"))).unwrap();
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

/// The editor as it looks, saved when `VOXEAR_TEST_OUTPUT` names a folder.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
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
    assert_eq!(fields.len(), 1, "the Inspector's own field is inside the drawer");
    ui.text(fields[0])
}

#[test]
fn drawers_written_in_csharp_draw_fields_and_change_them() {
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
    let manager =
        world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == "Game Manager");
    editor.select(manager.map(|(entity, _)| entity));

    // The scripts start without playing, and the drawers are built once they have.
    let started = std::time::Instant::now();
    while all_with_class(&editor.ui, "field-drawer").len() < 2 {
        assert!(started.elapsed().as_secs() < 300, "the drawers were never built");
        assert!(!editor.is_playing(), "drawing does not play");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, &mut renderer);
    let drawers = all_with_class(&editor.ui, "field-drawer");
    let stat = *drawers.iter().find(|&&d| reading(&editor.ui, d, "Fill").is_some()).expect("the Stat's drawer");
    let stepper = *drawers.iter().find(|&&d| reading(&editor.ui, d, "+").is_some()).expect("the Stepper's drawer");

    // A ValueDrawer draws the whole value, so its fields have no rows of their own.
    assert!(reading(&editor.ui, stat, "3 / 10").is_some(), "the drawer wrote the value into its label");
    let labels: Vec<String> = all_with_class(&editor.ui, "prop-label").into_iter().map(|l| editor.ui.text(l)).collect();
    assert!(labels.contains(&"Health".to_string()) && labels.contains(&"Bonus".to_string()), "{labels:?}");
    assert!(!labels.contains(&"Max".to_string()), "the Stat's own fields are not shown: {labels:?}");

    // An AttributeDrawer around the Inspector's own field, whose buttons change it.
    assert_eq!(bonus(&editor.ui, stepper), "10");
    let up = reading(&editor.ui, stepper, "+").unwrap();
    click(&mut editor, &mut renderer, up);
    assert_eq!(bonus(&editor.ui, stepper), "15", "the attribute's step was added");

    let fill = reading(&editor.ui, stat, "Fill").unwrap();
    click(&mut editor, &mut renderer, fill);
    assert!(reading(&editor.ui, stat, "10 / 10").is_some(), "the drawer filled the Stat and showed it");
    shot(&mut renderer, "inspector-drawers.png");

    // A drawer's change is a step to undo, and the drawer shows the value undone.
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let drawers = all_with_class(&editor.ui, "field-drawer");
    let stat = *drawers.iter().find(|&&d| reading(&editor.ui, d, "Fill").is_some()).unwrap();
    assert!(reading(&editor.ui, stat, "3 / 10").is_some(), "undo brought the Stat back");
}
