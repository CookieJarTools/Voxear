//! A data asset changed in the Inspector while the game plays, on a copy of
//! Hello whose Spinner A has a Pacer holding a `Pace`: the instance the
//! script holds takes the new value at once, keeping what the game's code
//! set on its other fields, and the next run reads the file again. A file
//! of its own, so it runs in a process of its own, since .NET starts only
//! once per process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, Transform};
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const PACE: &str = r#"using Voxear;

namespace Hello;

public sealed class Pace : Definition
{
	public float Speed = 1f;
	public int Starts;
}
"#;

const PACER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Pacer : Behaviour
{
	public Pace? Pace;

	protected override void Start()
	{
		Pace!.Starts++;
	}

	protected override void Update()
	{
		Obj.Position = new Vector3(Pace!.Speed, Pace.Starts, 0f);
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
        assert!(started.elapsed().as_secs() < 60, "{what} never happened: {:?}", editor.play_error());
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

/// The text input of the Inspector row labelled `label`.
fn input_of(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    let row = rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))?;
    let mut inputs = Vec::new();
    with_class(ui, row, "value-input", &mut inputs);
    inputs.first().copied()
}

/// Where Spinner A is in the world shown, which the Pacer puts at its
/// Pace's speed and starts.
fn paced(editor: &voxear_editor::Editor) -> Option<Vec3> {
    let world = &editor.game.engine.world;
    let (spinner, _) =
        world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner A")?;
    world.get::<Transform>(spinner).map(|transform| transform.position)
}

fn at(editor: &voxear_editor::Editor, x: f32, y: f32) -> bool {
    paced(editor).is_some_and(|p| (p - Vec3::new(x, y, 0.0)).length() < 1e-4)
}

fn play(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.play(renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_data_asset_changed_in_the_inspector_while_playing_changes_the_instance_scripts_hold() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Pace.cs"), PACE).unwrap();
    std::fs::write(dir.path().join("Scripts/Pacer.cs"), PACER).unwrap();
    std::fs::create_dir_all(dir.path().join("Data")).unwrap();
    let file = dir.path().join("Data/Pace.asset");
    std::fs::write(&file, "type = \"Hello.Pace\"\n\n[fields]\nspeed = 2.0\n").unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let paced_by =
        "behaviours = [\"Hello.Spinner\", \"Hello.Pacer\"]\n[obj.fields.\"Hello.Pacer\"]\npace = \"Data/Pace.asset\"";
    std::fs::write(&scene, text.replacen("behaviours = [\"Hello.Spinner\"]", paced_by, 1)).unwrap();

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    play(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the Pacer pacing", |e| at(e, 2.0, 1.0));

    // The speed typed in goes to the file and to the instance the Pacer holds, its starts left as the game set them.
    assert!(editor.inspect_path("Data/Pace.asset"));
    until(&mut editor, &mut renderer, "the Inspector showing the asset", |e| input_of(&e.ui, "Speed").is_some());
    let speed = input_of(&editor.ui, "Speed").unwrap();
    editor.ui.focus(speed);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("3.5");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    until(&mut editor, &mut renderer, "the Pacer taking the new speed", |e| at(e, 3.5, 1.0));
    assert!(editor.is_playing());
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("speed = 3.5"), "{written}");

    // The next run reads the file again, with the game's starts gone with the last run.
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    play(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, "the Pacer started once, at the file's speed", |e| at(e, 3.5, 1.0));
}
