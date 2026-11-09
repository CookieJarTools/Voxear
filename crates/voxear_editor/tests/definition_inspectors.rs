//! A data asset drawn whole by a `DefinitionInspector` written in C#, on a
//! copy of Hello with a `WeaponStats` Definition and a rifle's stats: the
//! inspector's own elements, a field's rows in a `FieldRow` from markup and
//! another from code, the rest in its "next", a button whose change is
//! written to the file, and the data assets of its type found from it with
//! Definition.All. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET
//! or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const WEAPON_STATS: &str = r#"using System.Collections.Generic;
using Voxear;

namespace Hello;

public sealed class WeaponStats : Definition
{
	public float Damage = 10f;
	public List<int> Shots = [];
	public string Title = "";
}
"#;

const WEAPON_STATS_INSPECTOR: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class WeaponStatsInspector : DefinitionInspector<WeaponStats>
{
	public override string Markup => """
		<vui>
			<Label name="summary" />
			<Label name="listed" />
			<Button name="double">Double</Button>
			<FieldRow field="Shots" />
			<Panel name="next" />
		</vui>
		""";

	protected override void Build(UiElement root)
	{
		root.Add(new FieldRow(nameof(WeaponStats.Title)));
	}

	protected override void Draw(WeaponStats stats, UiDocument ui)
	{
		if (ui.Find("double")!.WasClicked)
		{
			stats.Damage *= 2f;
		}
		ui.Find("summary")!.Text = $"{stats.Name}: {stats.Damage} damage";
		ui.Find("listed")!.Text = string.Join(", ", Definition.All<WeaponStats>().Select(found => found.Path)) + " found";
	}
}
"#;

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

/// The first element in `node`, or `node` itself, whose own text is `text`.
fn reading(ui: &Ui, node: NodeId, text: &str) -> Option<NodeId> {
    if ui.text(node) == text && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| reading(ui, child, text))
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

/// The `FieldRow` naming `field`, as the editor finds it.
fn field_row(ui: &Ui, field: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "field-row", &mut rows);
    rows.into_iter().find(|&row| ui.has_class(row, &format!("field-row:{field}")))
}

#[test]
fn a_definition_inspector_draws_a_data_asset_whole_and_writes_what_it_changes() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/WeaponStats.cs"), WEAPON_STATS).unwrap();
    std::fs::write(dir.path().join("Scripts/WeaponStatsInspector.cs"), WEAPON_STATS_INSPECTOR).unwrap();
    std::fs::create_dir_all(dir.path().join("Weapons")).unwrap();
    let file = dir.path().join("Weapons/Rifle.asset");
    std::fs::write(&file, "type = \"Hello.WeaponStats\"\n\n[fields]\ndamage = 12.5\nshots = [1, 2]\n").unwrap();

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(editor.inspect_path("Weapons/Rifle.asset"));
    let says = |editor: &voxear_editor::Editor, text: &str| reading(&editor.ui, editor.ui.root(), text).is_some();

    // The inspector draws the file's values, named for it, with its fields' rows where it puts them.
    until(&mut editor, &mut renderer, "the inspector drawing", |e| says(e, "Rifle: 12.5 damage"));
    assert!(says(&editor, "Weapons/Rifle.asset found"), "the data assets of its type, read while editing");
    until(&mut editor, &mut renderer, "the fields placed", |e| {
        field_row(&e.ui, "Title").is_some_and(|row| prop_row(&e.ui, "Title").is_some_and(|t| e.ui.is_within(t, row)))
    });
    let shots = prop_row(&editor.ui, "Shots").expect("the usual Shots field");
    assert!(editor.ui.is_within(shots, field_row(&editor.ui, "Shots").unwrap()), "in the markup's FieldRow");
    let mut drawn = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "field-drawer", &mut drawn);
    let inspector = *drawn.iter().find(|&&d| reading(&editor.ui, d, "Double").is_some()).expect("its elements");
    let damage = prop_row(&editor.ui, "Damage").expect("the usual Damage field");
    assert!(editor.ui.is_within(damage, inspector) && shown(&editor.ui, damage), "the rest in its next");
    assert!(shown(&editor.ui, shots), "a FieldRow's rows show");

    // Its button doubles the damage, written to the file at once, the level left as it was.
    let double = reading(&editor.ui, inspector, "Double").unwrap();
    let [x, y, w, h] = editor.ui.rect(double);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    until(&mut editor, &mut renderer, "the damage doubled", |e| says(e, "Rifle: 25 damage"));
    frame(&mut editor, &mut renderer);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("damage = 25.0") && text.contains("shots = [1, 2]"), "{text}");
    assert!(!editor.has_unsaved_changes(), "the level has nothing to save");
}
