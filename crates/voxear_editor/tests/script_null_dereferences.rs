//! A data asset's Inspector and a window written in C# that dereference
//! null throw a NullReferenceException each, which the Console shows, and
//! the editor goes on drawing them. Before, the fault took the editor down:
//! .NET catches a null dereference in its fault handler, which needs a
//! larger signal stack than Rust gives its threads. A file of its own, so it
//! runs in a process of its own, since .NET starts only once per process.
//! Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const GADGET: &str = r#"using Voxear;

namespace Hello;

public sealed class Gadget : Definition
{
	public float Size = 1f;
}
"#;

const GADGET_INSPECTOR: &str = r#"using Voxear;

namespace Hello;

public sealed class GadgetInspector : DefinitionInspector<Gadget>
{
	private static string? missing;
	private static int draws;

	public override string Markup => """
		<vui>
			<Label name="size" />
		</vui>
		""";

	protected override void Draw(Gadget gadget, UiDocument ui)
	{
		draws++;
		Log.Info($"gadget drawn {draws}");
		if (draws == 1)
		{
			Log.Info($"length {missing!.Length}");
		}
		ui.Find("size")!.Text = $"Size {gadget.Size}";
	}
}
"#;

const CARELESS_TOOLS: &str = r#"using Voxear;

namespace Hello;

[Window("Careless Tools")]
public sealed class CarelessTools : EditorWindow
{
	private static string? missing;
	private static int updates;

	protected override void Build(UiElement root)
	{
		root.Add(new Label("Careless"));
	}

	protected override void Update(UiDocument ui)
	{
		updates++;
		if (updates == 2)
		{
			Log.Info($"length {missing!.Length}");
		}
		Log.Info($"window updated {updates}");
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

/// Whether `node` or anything in it reads `text`.
fn shows(ui: &Ui, node: NodeId, text: &str) -> bool {
    ui.text(node) == text || ui.children(node).into_iter().any(|child| shows(ui, child, text))
}

/// What has been logged, line by line.
fn logged(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect()
}

/// Frames until a line `line` is logged, which must come within a minute.
fn until_logged(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, log: &LogCapture, line: &str) {
    let started = std::time::Instant::now();
    while !logged(log).iter().any(|logged| logged == line) {
        assert!(started.elapsed().as_secs() < 60, "{line:?} never logged: {:#?}", logged(log));
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn csharp_editor_code_that_dereferences_null_throws_and_the_editor_goes_on() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Gadget.cs"), GADGET).unwrap();
    std::fs::write(dir.path().join("Scripts/GadgetInspector.cs"), GADGET_INSPECTOR).unwrap();
    std::fs::write(dir.path().join("Scripts/CarelessTools.cs"), CARELESS_TOOLS).unwrap();
    std::fs::create_dir_all(dir.path().join("Gadgets")).unwrap();
    std::fs::write(dir.path().join("Gadgets/Box.asset"), "type = \"Hello.Gadget\"\n\n[fields]\nsize = 2.0\n").unwrap();
    std::fs::write(dir.path().join("Gadgets/Ball.asset"), "type = \"Hello.Gadget\"\n\n[fields]\nsize = 3.0\n").unwrap();

    let project = Project::open(dir.path()).unwrap();
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
    let nulls = |log: &LogCapture| logged(log).iter().filter(|line| line.starts_with("NullReferenceException")).count();

    // The Inspector throws on its first draw, and draws the next asset it is given.
    assert!(editor.inspect_path("Gadgets/Box.asset"));
    until_logged(&mut editor, &mut renderer, &log, "gadget drawn 1");
    for _ in 0..5 {
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(nulls(&log), 1, "{:#?}", logged(&log));
    assert!(editor.inspect_path("Gadgets/Ball.asset"));
    until_logged(&mut editor, &mut renderer, &log, "gadget drawn 2");
    let started = std::time::Instant::now();
    while !shows(&editor.ui, editor.ui.root(), "Size 3") {
        assert!(started.elapsed().as_secs() < 60, "the second asset never drawn");
        frame(&mut editor, &mut renderer);
    }
    assert_eq!(nulls(&log), 1, "{:#?}", logged(&log));

    // The window throws on its second update, and updates again after.
    let window = editor.add_script_window("Hello.CarelessTools", "inspector").expect("the window opens");
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.show_dock_panel(dock, &window);
    until_logged(&mut editor, &mut renderer, &log, "window updated 4");
    let lines = logged(&log);
    assert!(lines.iter().all(|line| line != "window updated 2"), "it stopped where it threw");
    assert_eq!(nulls(&log), 2, "{lines:#?}");
}
