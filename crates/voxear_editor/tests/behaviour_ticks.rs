//! A behaviour's tick in the Inspector, on a copy of Hello whose Spinner A
//! has a Switched that counts its callbacks: unticked while editing, it
//! saves so and the game starts with it disabled, Awake but no OnEnable,
//! Start or Update; ticked while playing, it gets OnEnable, Start and its
//! updates; unticked, OnDisable once and no more updates; ticked again,
//! OnEnable and its updates, but no second Start. Its Obj's tick does the
//! same, and leaves it off when its own tick is. A file of its own, so it
//! runs in a process of its own, since .NET starts only once per process.
//! Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SWITCHED: &str = r#"using Voxear;

namespace Hello;

public sealed class Switched : Behaviour
{
	public int Awakes;

	public int Enables;

	public int Disables;

	public int Starts;

	public int Ticks;

	protected override void Awake()
	{
		Awakes++;
	}

	protected override void OnEnable()
	{
		Enables++;
	}

	protected override void OnDisable()
	{
		Disables++;
	}

	protected override void Start()
	{
		Starts++;
	}

	protected override void Update()
	{
		Ticks++;
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

/// The editor on a copy of Hello, once its scripts are described.
fn open_described(renderer: &mut Renderer, dir: &tempfile::TempDir) -> voxear_editor::Editor {
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, renderer);
    editor
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

fn frames(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, count: usize) {
    for _ in 0..count {
        frame(editor, renderer);
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

/// The tick on the header of the section titled `title`.
fn tick(ui: &Ui, title: &str) -> Option<NodeId> {
    let mut ticks = Vec::new();
    with_class(ui, ui.root(), "component-switch", &mut ticks);
    ticks.into_iter().find(|&tick| {
        let header = ui.parent(tick).unwrap();
        ui.children(header).into_iter().any(|child| ui.has_class(child, "foldout-title") && ui.text(child) == title)
    })
}

/// Clicks Switched's tick.
fn click_tick(ui: &mut Ui) {
    let tick = tick(ui, "Switched").expect("the Switched section's tick");
    let [x, y, w, h] = ui.rect(tick);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// Clicks the tick by the Obj's name.
fn click_obj_tick(ui: &mut Ui) {
    let tick = ui.find(ui.root(), "obj-active").expect("the Obj's tick");
    let [x, y, w, h] = ui.rect(tick);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// Whether the tick by the Obj's name shows it on.
fn obj_ticked(editor: &voxear_editor::Editor) -> bool {
    editor.ui.is_checked(editor.ui.find(editor.ui.root(), "obj-active").expect("the Obj's tick"))
}

/// Whether Switched's tick shows it on.
fn ticked(editor: &voxear_editor::Editor) -> bool {
    editor.ui.is_checked(tick(&editor.ui, "Switched").expect("the Switched section's tick"))
}

/// The Inspector row labelled `label`.
fn prop_row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
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

/// The number the Inspector's field on the row labelled `label` shows.
fn shows(editor: &voxear_editor::Editor, label: &str) -> i64 {
    let Some(row) = prop_row(&editor.ui, label) else { return -1 };
    let mut inputs = Vec::new();
    with_class(&editor.ui, row, "value-input", &mut inputs);
    inputs.first().and_then(|&input| editor.ui.text(input).parse().ok()).unwrap_or(-1)
}

/// The counts Switched shows: Awake, OnEnable, OnDisable and Start.
fn counts(editor: &voxear_editor::Editor) -> [i64; 4] {
    ["Awakes", "Enables", "Disables", "Starts"].map(|label| shows(editor, label))
}

fn spinner(editor: &voxear_editor::Editor) -> voxear_ecs::Entity {
    let world = &editor.game.engine.world;
    let found = world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == "Spinner A");
    found.unwrap().0
}

/// Whether the edited Spinner A's Switched starts enabled.
fn starts_enabled(editor: &voxear_editor::Editor) -> bool {
    let kept = editor.game.engine.world.get::<voxear_engine::EditBehaviours>(spinner(editor)).unwrap();
    kept.0.iter().find(|spec| spec.type_name == "Hello.Switched").unwrap().enabled
}

#[test]
fn a_behaviours_tick_disables_and_enables_it_while_playing() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Switched.cs"), SWITCHED).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    let both = "behaviours = [\"Hello.Spinner\", \"Hello.Switched\"]";
    std::fs::write(&scene, text.replacen("behaviours = [\"Hello.Spinner\"]", both, 1)).unwrap();
    let mut editor = open_described(&mut renderer, &dir);
    editor.select(Some(spinner(&editor)));
    until(&mut editor, &mut renderer, "the Switched section", |e| tick(&e.ui, "Switched").is_some());
    // Laid out, to be clicked.
    frame(&mut editor, &mut renderer);

    // Unticked while editing, it is saved so, and undo ticks it again.
    assert!(ticked(&editor), "on, as the scene starts it");
    click_tick(&mut editor.ui);
    frame(&mut editor, &mut renderer);
    assert!(!starts_enabled(&editor));
    editor.undo(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(starts_enabled(&editor), "undone");
    assert!(ticked(&editor));
    click_tick(&mut editor.ui);
    frame(&mut editor, &mut renderer);
    assert!(!starts_enabled(&editor));
    editor.save();
    let saved = std::fs::read_to_string(&scene).unwrap();
    assert!(saved.contains("\"Hello.Switched\" = false"), "{saved}");

    // Playing, it wakes but is not enabled, started or updated.
    editor.play(&mut renderer);
    until(&mut editor, &mut renderer, "playing", |e| e.is_playing() && prop_row(&e.ui, "Ticks").is_some());
    until(&mut editor, &mut renderer, "its Awake", |e| shows(e, "Awakes") == 1);
    frames(&mut editor, &mut renderer, 10);
    assert_eq!(counts(&editor), [1, 0, 0, 0], "Awake, Enables, Disables, Starts");
    assert_eq!(shows(&editor, "Ticks"), 0);
    assert!(!ticked(&editor), "shown off");

    // Ticked, it is enabled, starts and updates.
    click_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its updates", |e| shows(e, "Ticks") > 2);
    assert_eq!(counts(&editor), [1, 1, 0, 1]);
    assert!(ticked(&editor));

    // Unticked, it gets OnDisable once and no more updates.
    click_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its OnDisable", |e| shows(e, "Disables") == 1);
    let stopped = shows(&editor, "Ticks");
    frames(&mut editor, &mut renderer, 10);
    assert_eq!(shows(&editor, "Ticks"), stopped, "no updates while it is off");
    assert_eq!(counts(&editor), [1, 1, 1, 1]);
    assert!(!ticked(&editor));

    // Ticked again, OnEnable and its updates again, but not Start.
    click_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its updates again", |e| shows(e, "Ticks") > stopped + 2);
    assert_eq!(counts(&editor), [1, 2, 1, 1]);

    // Its Obj switched off, it gets OnDisable and no updates, its own tick still on; back on, OnEnable.
    click_obj_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its OnDisable with its Obj", |e| shows(e, "Disables") == 2);
    let stopped = shows(&editor, "Ticks");
    frames(&mut editor, &mut renderer, 10);
    assert_eq!(shows(&editor, "Ticks"), stopped, "no updates while its Obj is off");
    assert_eq!(counts(&editor), [1, 2, 2, 1]);
    assert!(ticked(&editor) && !obj_ticked(&editor), "its own tick on, its Obj's off");
    click_obj_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its updates with its Obj back", |e| shows(e, "Ticks") > stopped + 2);
    assert_eq!(counts(&editor), [1, 3, 2, 1]);

    // Off by its own tick, its Obj going off and on leaves it off.
    click_tick(&mut editor.ui);
    until(&mut editor, &mut renderer, "its OnDisable again", |e| shows(e, "Disables") == 3);
    let stopped = shows(&editor, "Ticks");
    click_obj_tick(&mut editor.ui);
    frames(&mut editor, &mut renderer, 5);
    click_obj_tick(&mut editor.ui);
    frames(&mut editor, &mut renderer, 10);
    assert!(obj_ticked(&editor) && !ticked(&editor));
    assert_eq!(counts(&editor), [1, 3, 3, 1], "no OnEnable while its own tick is off");
    assert_eq!(shows(&editor, "Ticks"), stopped);

    // Stopping brings back the scene as it was edited, the behaviour unticked.
    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(!starts_enabled(&editor), "what play changed goes with it");
}
