//! Behaviours' gizmos, on a copy of Hello whose Spinner B gains a Beacon
//! that draws a sphere around itself, yellow while selected. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process, and debug draw keeps one list for the process. Skips itself
//! without a GPU or .NET.

use voxear_core::{debug_draw, logging::LogCapture};
use voxear_ecs::Entity;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Name;

const SIZE: (u32, u32) = (1280, 800);

const BEACON: &str = r#"using Voxear;

namespace Hello;

public sealed class Beacon : Behaviour
{
	public float Reach = 1.5f;

	protected override void DrawGizmos(Gizmo gizmo)
	{
		Draw.WireSphere(Obj.WorldPosition, Reach, gizmo.IsSelected ? Color.Yellow : Color.Cyan);
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against, with a Beacon on Spinner B.
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
    std::fs::write(dir.path().join("Scripts/Beacon.cs"), BEACON).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let spinner = "behaviours = [\"Hello.Spinner\"]\n# Follows Spinner A";
    assert!(text.contains(spinner), "Spinner B is where the Beacon goes");
    let beacon = "behaviours = [\"Hello.Spinner\", \"Hello.Beacon\"]\n# Follows Spinner A";
    std::fs::write(&scene, text.replace(spinner, beacon)).unwrap();
    dir
}

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

/// The gizmo lines drawn this frame in this colour.
fn gizmo_lines(color: [f32; 4]) -> usize {
    debug_draw::lines().iter().filter(|line| line.gizmo && line.color == color).count()
}

/// Frames until there are gizmo lines in this colour, failing after a while.
fn until_drawn(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, color: [f32; 4], what: &str) {
    let started = std::time::Instant::now();
    while gizmo_lines(color) == 0 {
        assert!(started.elapsed().as_secs() < 300, "{what} never drew");
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn behaviours_draw_their_gizmos_while_editing_and_while_playing() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let (cyan, yellow) = ([0.0, 1.0, 1.0, 1.0], [1.0, 0.92, 0.016, 1.0]);

    // While editing, from the scene's values, once the scripts have started.
    let world = &editor.game.engine.world;
    let beacon = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner B").unwrap().0;
    editor.select(Some(beacon));
    frame(&mut editor, &mut renderer);
    editor.select(None);
    until_drawn(&mut editor, &mut renderer, cyan, "the Beacon's gizmo");
    assert_eq!(gizmo_lines(cyan), 3 * 32, "a sphere's three circles, marked as gizmos");
    assert_eq!(gizmo_lines(yellow), 0);

    // Selected, it shows it.
    editor.select(Some(beacon));
    frame(&mut editor, &mut renderer);
    assert_eq!((gizmo_lines(yellow), gizmo_lines(cyan)), (3 * 32, 0), "yellow while selected");

    // While playing, the running Beacon draws its own.
    editor.select(None);
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "the scripts build and start");
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    until_drawn(&mut editor, &mut renderer, cyan, "the running Beacon's gizmo");
    editor.stop(&mut renderer);
}
