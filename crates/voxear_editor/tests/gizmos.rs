//! Behaviours' gizmos, on a copy of Hello whose Spinner B gains a Beacon
//! that draws a sphere around itself, yellow while selected, and while
//! selected shows handles for its reach, a target and a button, and has an
//! icon. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process, and debug draw keeps one list for the
//! process. Skips itself without a GPU or .NET.

use voxear_core::{debug_draw, logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::{EditBehaviours, Project};
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::{Name, WorldTransform};

const SIZE: (u32, u32) = (1280, 800);

const BEACON: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

[GizmoIcon(IconName.Audio)]
public sealed class Beacon : Behaviour
{
	public float Reach = 1.5f;
	[OnValueChanged(nameof(Moved))] public Vector3 Target = new(0f, 2f, 0f);
	public int Pings;
	public int Moves;

	protected override void DrawGizmos(Gizmo gizmo)
	{
		Draw.WireSphere(Obj.WorldPosition, Reach, gizmo.IsSelected ? Color.Yellow : Color.Cyan);
		if (!gizmo.IsSelected)
		{
			return;
		}
		Reach = Handles.Radius(Obj.WorldPosition, Reach, Color.Magenta);
		Target = Handles.Position(Target);
		if (Handles.Button(Obj.WorldPosition + new Vector3(0f, 2.5f, 0f)))
		{
			Pings++;
		}
	}

	private void Moved() => Moves++;
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

/// The Beacon's field as its scene now sets it, if it does.
fn beacon_field(editor: &voxear_editor::Editor, beacon: Entity, name: &str) -> Option<toml::Value> {
    let kept = editor.game.engine.world.get::<EditBehaviours>(beacon)?;
    let spec = kept.0.iter().find(|spec| spec.type_name == "Hello.Beacon")?;
    spec.fields.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.clone())
}

/// A number or vector as a scene writes it; whole numbers come as integers.
fn number(value: &toml::Value) -> f32 {
    value.as_float().or_else(|| value.as_integer().map(|n| n as f64)).unwrap() as f32
}

fn vector(value: &toml::Value) -> Vec3 {
    let items = value.as_array().unwrap();
    Vec3::new(number(&items[0]), number(&items[1]), number(&items[2]))
}

fn on_screen(editor: &voxear_editor::Editor, point: Vec3) -> [f32; 2] {
    editor.scene_camera().expect("the Scene view shows").project(point).expect("in front of the camera")
}

/// Clicks a point in the Scene view, a frame for the press and one for the release.
fn click_at(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, at: [f32; 2]) {
    pointer_to(editor, at);
    frame(editor, renderer);
    editor.ui.pointer_button(true);
    frame(editor, renderer);
    editor.ui.pointer_button(false);
    frame(editor, renderer);
}

fn with_class(ui: &voxear_ui::Ui, node: voxear_ui::NodeId, class: &str, found: &mut Vec<voxear_ui::NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// Opens the first Scene view's Gizmos menu and chooses the item that says `label`.
fn gizmos_menu(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, label: &str) {
    let button = editor.ui.find(editor.ui.root(), "gizmos-menu").expect("the Gizmos menu's button");
    let [x, y, w, h] = editor.ui.rect(button);
    click_at(editor, renderer, [x + w / 2.0, y + h / 2.0]);
    let mut items = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "menu-item", &mut items);
    let ui = &editor.ui;
    let item = items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label));
    let [x, y, w, h] = editor.ui.rect(item.unwrap_or_else(|| panic!("the Gizmos menu lists {label}")));
    click_at(editor, renderer, [x + w / 2.0, y + h / 2.0]);
}

/// How many of the Beacon's audio icons the Scene view shows.
fn icons(editor: &voxear_editor::Editor) -> usize {
    let handles = editor.ui.find(editor.ui.root(), "handles").expect("the Scene view's handles layer");
    let shapes = editor.ui.shapes(handles);
    shapes.iter().filter(|shape| matches!(shape, voxear_ui::Shape::Icon { name: "audio", .. })).count()
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

    // Its handles: the target is a point to drag, and the scene keeps where it went, as one step to undo.
    let tool = editor.ui.find(editor.ui.root(), "tool").expect("the tool strip");
    editor.ui.set_segment(tool, 0);
    frame(&mut editor, &mut renderer);
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join("script-handles.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    assert_eq!(beacon_field(&editor, beacon, "target"), None, "untouched, the script's own value");
    let from = on_screen(&editor, Vec3::new(0.0, 2.0, 0.0));
    drag(&mut editor, &mut renderer, from, [from[0], from[1] - 40.0]);
    let moved = vector(&beacon_field(&editor, beacon, "target").expect("the drag set the target"));
    assert!(moved.y > 2.1, "up the screen is up: {moved}");
    let moves = beacon_field(&editor, beacon, "moves").and_then(|value| value.as_integer());
    assert_eq!(moves, Some(1), "[OnValueChanged] ran once, for the one change");
    assert!(editor.has_unsaved_changes(), "a drag is an edit");
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    let back = beacon_field(&editor, beacon, "target").map(|value| vector(&value));
    assert!(back.is_none_or(|t| t.distance(Vec3::new(0.0, 2.0, 0.0)) < 1e-4), "one undo takes it back: {back:?}");

    // The reach's dot is on its rim, to the right on screen: out makes it bigger.
    let centre = editor.game.engine.world.get::<WorldTransform>(beacon).unwrap().0.to_scale_rotation_translation().2;
    let camera = editor.scene_camera().unwrap();
    let rim = on_screen(&editor, centre + camera.right * 1.5);
    drag(&mut editor, &mut renderer, rim, [rim[0] + 50.0, rim[1]]);
    let reach = number(&beacon_field(&editor, beacon, "reach").expect("the drag set the reach"));
    assert!(reach > 1.8, "dragged out, the reach grew: {reach}");

    // A click on the button is true for the one frame.
    let button = on_screen(&editor, centre + Vec3::new(0.0, 2.5, 0.0));
    drag(&mut editor, &mut renderer, button, button);
    let pings = beacon_field(&editor, beacon, "pings").and_then(|value| value.as_integer());
    assert_eq!(pings, Some(1), "one click, one ping");

    // A click on the gizmo's sphere selects the Obj it is drawn for, though nothing else is there.
    editor.select(None);
    frame(&mut editor, &mut renderer);
    assert_eq!(icons(&editor), 1, "the Beacon's icon shows where its Obj is");
    let reach = number(&beacon_field(&editor, beacon, "reach").unwrap());
    let top = on_screen(&editor, centre + Vec3::Y * reach);
    click_at(&mut editor, &mut renderer, top);
    assert_eq!(editor.selected(), Some(beacon), "a click on the top of the sphere selects Spinner B");

    // The Gizmos menu hides the Beacon's gizmos and icon, and shows them again.
    editor.select(None);
    gizmos_menu(&mut editor, &mut renderer, "Beacon");
    frame(&mut editor, &mut renderer);
    assert_eq!((gizmo_lines(cyan), icons(&editor)), (0, 0), "hidden from the Gizmos menu");
    assert!(editor.settings().hidden_gizmos.iter().any(|t| t == "Hello.Beacon"), "and the setting is kept");
    click_at(&mut editor, &mut renderer, top);
    assert_eq!(editor.selected(), None, "a hidden gizmo cannot be clicked");
    gizmos_menu(&mut editor, &mut renderer, "Beacon");
    until_drawn(&mut editor, &mut renderer, cyan, "the Beacon's gizmo, shown again");
    assert_eq!(icons(&editor), 1);

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
