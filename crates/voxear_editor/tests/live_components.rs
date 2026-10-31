//! A running game's C# component values in the Inspector, read from and
//! written to the component's own bytes. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips
//! itself without a GPU or .NET.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_ui::{Key, Modifiers, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A component the scene gives Chrome Ball, and a system that counts the
/// frames in it.
const WOBBLE: &str = r#"using Voxear;

namespace Hello
{
	public struct Wobble : IComponent
	{
		public float Speed;
		public int Frames;
		public bool Steady;
	}

	public sealed class WobbleSystem : SystemBase
	{
		protected override void Update()
		{
			foreach (var chunk in Query<Wobble>())
			{
				var wobbles = chunk.Write<Wobble>();
				for (var i = 0; i < chunk.Count; i++)
				{
					wobbles[i].Frames++;
				}
			}
		}
	}
}
"#;

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

/// The value field in the Inspector row labelled `label`.
fn input(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    let row = rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))?;
    let mut inputs = Vec::new();
    with_class(ui, row, "value-input", &mut inputs);
    inputs.first().copied()
}

fn named(editor: &voxear_editor::Editor, name: &str) -> Entity {
    let world = &editor.game.engine.world;
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

/// Frames until `done`, for up to five minutes, as the scripts build.
fn until(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, done: impl Fn(&voxear_editor::Editor) -> bool) {
    let started = std::time::Instant::now();
    while !done(editor) {
        assert_eq!(editor.play_error(), None);
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_running_games_component_values_show_and_edit_in_the_inspector() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Wobble.cs"), WOBBLE).unwrap();
    let day = dir.path().join("Scenes/Day.scene");
    let scene = std::fs::read_to_string(&day).unwrap();
    let given = "name = \"Chrome Ball\"\ncomponents = [\"Hello.Wobble\"]\nfields.\"Hello.Wobble\" = { speed = 2.5 }";
    std::fs::write(&day, scene.replace("name = \"Chrome Ball\"", given)).unwrap();

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    until(&mut editor, &mut renderer, |editor| editor.scripts_described());
    editor.select(Some(named(&editor, "Chrome Ball")));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let speed = input(&editor.ui, "Speed").expect("the scene's component shows in edit mode");
    assert_eq!(editor.ui.text(speed), "2.5");

    editor.play(&mut renderer);
    until(&mut editor, &mut renderer, |editor| editor.is_playing());
    let ball = named(&editor, "Chrome Ball");
    editor.select(Some(ball));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let counted = |editor: &voxear_editor::Editor| {
        editor.ui.text(input(&editor.ui, "Frames").expect("its fields show while playing")).parse::<i64>().unwrap()
    };
    let before = counted(&editor);
    for _ in 0..5 {
        frame(&mut editor, &mut renderer);
    }
    assert!(counted(&editor) >= before + 5, "the running count shows: {before}, then {}", counted(&editor));

    // Typed while playing, it is written into the game.
    let speed = input(&editor.ui, "Speed").unwrap();
    editor.ui.focus(speed);
    editor.ui.set_modifiers(Modifiers { shift: false, control: true });
    editor.ui.key_down(Key::Char('a'));
    editor.ui.set_modifiers(Modifiers::default());
    editor.ui.text_input("7");
    frame(&mut editor, &mut renderer);
    editor.ui.blur();
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let id = world.components().id_by_name("Hello.Wobble").unwrap();
    let bytes = world.get_by_id(ball, id).unwrap();
    let written = unsafe { bytes.cast::<f32>().read_unaligned() };
    assert_eq!(written, 7.0, "the game's own value changed");

    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    editor.select(Some(named(&editor, "Chrome Ball")));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert_eq!(editor.ui.text(input(&editor.ui, "Speed").unwrap()), "2.5", "the edited level is as it was");
}
