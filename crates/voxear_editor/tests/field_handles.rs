//! Handles that edit a behaviour's fields in the Scene view, on a copy of
//! Hello whose Spinner B gains a Guard with a `[PositionHandle]` point, a
//! `[RadiusHandle]` range, a world-space patrol path and the other handles.
//! A file of its own, so it runs in a process of its own, since .NET starts
//! only once per process. Skips itself without a GPU or .NET.

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::{EditBehaviours, Project};
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, WorldTransform};

const SIZE: (u32, u32) = (1280, 800);

const GUARD: &str = r##"using System.Collections.Generic;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Guard : Behaviour
{
	[PositionHandle] public Vector3 Post = new(1f, 0f, 0f);
	[RadiusHandle] public float Range = 2f;
	[PositionHandle(Space = HandleSpace.World)] public List<Vector3> Patrol = [new(-2f, 0.5f, -1f), new(-1f, 0.5f, -1f)];
	[BoundsHandle] public Vector3 Area = new(2f, 2f, 2f);
	[ArcHandle(Radius = 2.5f)] public float View = 90f;
	[SliderHandle] public float Height = 1f;
	[DirectionHandle(Color = "#ff8800")] public Vector3 Facing = new(0f, 0f, 1.5f);
}
"##;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against, with Spinner B guarding.
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
    std::fs::write(dir.path().join("Scripts/Guard.cs"), GUARD).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let spinner_b = "name = \"Spinner B\"\nposition = [2.2, 0.75, 0.0]\nrotation = [0.0, 30.0, 0.0]\nscale = [1.5, 1.5, 1.5]\nbehaviours = [\"Hello.Spinner\"]";
    assert!(text.contains(spinner_b), "Spinner B is where the Guard goes");
    std::fs::write(
        &scene,
        text.replace(spinner_b, &spinner_b.replace("[\"Hello.Spinner\"]", "[\"Hello.Spinner\", \"Hello.Guard\"]")),
    )
    .unwrap();
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

fn pointer_to(editor: &mut voxear_editor::Editor, [x, y]: [f32; 2]) {
    let device_id = winit::event::DeviceId::dummy();
    let position = winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y));
    editor.handle(&winit::event::WindowEvent::CursorMoved { device_id, position });
}

fn left_button(editor: &mut voxear_editor::Editor, pressed: bool) {
    use winit::event::{ElementState, MouseButton, WindowEvent};
    let device_id = winit::event::DeviceId::dummy();
    let state = if pressed { ElementState::Pressed } else { ElementState::Released };
    editor.handle(&WindowEvent::MouseInput { device_id, state, button: MouseButton::Left });
}

/// Presses at `from`, moves to `to` and lets go, a frame between each.
fn drag(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, from: [f32; 2], to: [f32; 2]) {
    pointer_to(editor, from);
    frame(editor, renderer);
    left_button(editor, true);
    frame(editor, renderer);
    pointer_to(editor, to);
    frame(editor, renderer);
    left_button(editor, false);
    frame(editor, renderer);
}

/// The Guard's field as its scene now sets it, if it does.
fn guard_field(editor: &voxear_editor::Editor, guard: Entity, name: &str) -> Option<toml::Value> {
    let kept = editor.game.engine.world.get::<EditBehaviours>(guard)?;
    let spec = kept.0.iter().find(|spec| spec.type_name == "Hello.Guard")?;
    spec.fields.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.clone())
}

/// A vector as a scene writes it; whole numbers from the script's own values come as integers.
fn vector(value: &toml::Value) -> Vec3 {
    let items = value.as_array().unwrap();
    let number = |i: usize| items[i].as_float().or_else(|| items[i].as_integer().map(|n| n as f64)).unwrap() as f32;
    Vec3::new(number(0), number(1), number(2))
}

/// The editor's view of the Scene camera, as the handles see it.
fn on_screen(editor: &voxear_editor::Editor, point: Vec3) -> [f32; 2] {
    editor.scene_camera().expect("the Scene view shows").project(point).expect("in front of the camera")
}

#[test]
fn handles_on_fields_drag_their_values_in_the_scene_view() {
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
    let guard = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Spinner B").unwrap().0;
    let matrix = world.get::<WorldTransform>(guard).unwrap().0;
    let centre = matrix.to_scale_rotation_translation().2;
    editor.select(Some(guard));
    // Select only: the move tool's handles would sit over the Obj's own dots.
    let tool = editor.ui.find(editor.ui.root(), "tool").expect("the tool strip");
    editor.ui.set_segment(tool, 0);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("field-handles.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    // The post is the Obj's own point: dragging its dot moves it, and the field follows, in the Obj's space.
    assert_eq!(guard_field(&editor, guard, "post"), None, "untouched, the script's own value");
    let post = matrix.transform_point3(Vec3::new(1.0, 0.0, 0.0));
    let from = on_screen(&editor, post);
    drag(&mut editor, &mut renderer, from, [from[0], from[1] - 40.0]);
    let moved = vector(&guard_field(&editor, guard, "post").expect("the drag set the post"));
    assert!(moved.y > 0.1, "up the screen is up the Obj: {moved}");
    assert!(editor.has_unsaved_changes(), "a drag is an edit");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let back = guard_field(&editor, guard, "post").map(|value| vector(&value));
    assert!(back.is_none_or(|post| post.distance(Vec3::new(1.0, 0.0, 0.0)) < 1e-4), "one undo takes it back: {back:?}");

    // The range's dot is on its circle, to the right on screen: out makes it bigger.
    let camera = editor.scene_camera().unwrap();
    let rim = on_screen(&editor, centre + camera.right * 2.0);
    drag(&mut editor, &mut renderer, rim, [rim[0] + 50.0, rim[1]]);
    let range = guard_field(&editor, guard, "range").and_then(|v| v.as_float()).expect("the drag set the range");
    assert!(range > 2.3, "dragged out, the range grew: {range}");

    // The patrol is in the world: each point drags on its own.
    let second = on_screen(&editor, Vec3::new(-1.0, 0.5, -1.0));
    drag(&mut editor, &mut renderer, second, [second[0] + 30.0, second[1]]);
    let patrol = guard_field(&editor, guard, "patrol").expect("the drag set the patrol");
    let points: Vec<Vec3> = patrol.as_array().unwrap().iter().map(vector).collect();
    assert_eq!(points.len(), 2);
    assert!(points[0].distance(Vec3::new(-2.0, 0.5, -1.0)) < 1e-4, "the first point stays: {points:?}");
    assert!(points[1].x > -0.9, "the second moved right: {points:?}");
}
