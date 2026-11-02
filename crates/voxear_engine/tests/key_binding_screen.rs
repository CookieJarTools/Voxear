//! The ready-made controls screen from a script: `KeyBindingScreen.Open()`
//! draws the keyboard with the game's actions, a key clicked lists its
//! actions, and Rebind takes the next key pressed. On a copy of Hello with
//! Voxear's own input actions. A file of its own, so it runs in a process of
//! its own, since .NET starts only once per process. Skips itself without
//! .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_ui::{NodeId, Ui};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Controls : SystemBase
{
	protected override void OnCreate()
	{
		var screen = KeyBindingScreen.Open("Controls")!;
		screen.Scheme = ControlScheme.KeyboardMouse;
		screen.Rebound += (action, control) => Log.Info($"rebound {action.Name} to {control}");
		var uses = Input.ControlUses("Keyboard/Space");
		Log.Info($"space: {uses.Count} {uses[0].Label} binding {uses[0].Binding}");
		Log.Info($"W: {Input.ControlUses("Keyboard/W")[0].Label}, south is {Input.ControlLabel("Gamepad/South")}");
	}

	protected override void Update()
	{
	}
}
"#;

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

fn find(ui: &Ui, test: impl Fn(NodeId) -> bool) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![ui.root()];
    while let Some(node) = stack.pop() {
        if test(node) {
            found.push(node);
        }
        stack.extend(ui.children(node));
    }
    found
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

#[test]
fn players_see_their_controls_and_rebind_them() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    // Hello's own actions, if a copy of the user's has any, give way to Voxear's.
    for entry in std::fs::read_dir(dir.path()).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "inputactions") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
    std::fs::write(dir.path().join("Scripts/Controls.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    let lines = said(&log);
    assert!(lines.contains(&"space: 1 Jump binding 0".to_string()), "{lines:#?}");
    assert!(lines.contains(&"W: Move (Up), south is A".to_string()), "{lines:#?}");

    // The keyboard shows with its bound keys lit and labelled.
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    let ui = game.ui.ui();
    let lit =
        find(ui, |n| ui.has_class(n, "keymap-key") && ui.has_class(n, "assigned") && !ui.has_class(n, "kbs-swatch"));
    assert!(lit.len() >= 10, "Voxear's own actions light WASD, Space and the rest: {}", lit.len());
    let space =
        find(ui, |n| ui.has_class(n, "keymap-key") && ui.children(n).first().is_some_and(|&l| ui.text(l) == "Space"));

    // Space clicked lists Jump; Rebind waits for a key, and J takes it.
    click(game.ui.ui(), space[0]);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    let ui = game.ui.ui();
    let rebind = find(ui, |n| ui.text(n) == "Rebind");
    assert_eq!(rebind.len(), 1, "Jump's Rebind button");
    assert!(!find(ui, |n| ui.has_class(n, "kbs-action") && ui.text(n) == "Jump").is_empty());
    click(game.ui.ui(), rebind[0]);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    assert!(game.input.is_listening(), "listening for the new key");
    game.input.devices_mut().set_key("J", true);
    game.tick(1.0 / 60.0, None);
    game.input.devices_mut().set_key("J", false);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    let lines = said(&log);
    assert!(lines.contains(&"rebound Jump to Keyboard/J".to_string()), "{lines:#?}");
    let jump = game.input.action("Jump").unwrap();
    assert_eq!(
        game.input.control_uses().iter().find(|u| u.action == jump).map(|u| u.path.clone()),
        Some("Keyboard/J".to_string())
    );

    // The List tab rebinds by row: Crouch's C, pressed E, is taken by Interact too, and the row says so.
    let ui = game.ui.ui();
    let list_tab = find(ui, |n| ui.has_class(n, "kbs-tab") && ui.text(n) == "List");
    click(game.ui.ui(), list_tab[0]);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    let ui = game.ui.ui();
    let crouch = find(ui, |n| ui.has_class(n, "controls-binding") && ui.text(n) == "C");
    assert_eq!(crouch.len(), 1, "Crouch's key in the list");
    click(game.ui.ui(), crouch[0]);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    assert!(game.input.is_listening());
    game.input.devices_mut().set_key("E", true);
    game.tick(1.0 / 60.0, None);
    game.input.devices_mut().set_key("E", false);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    let lines = said(&log);
    assert!(lines.contains(&"rebound Crouch to Keyboard/E".to_string()), "{lines:#?}");
    let ui = game.ui.ui();
    let conflicts: Vec<String> =
        find(ui, |n| ui.has_class(n, "controls-conflict")).into_iter().map(|n| ui.text(n)).collect();
    assert!(conflicts.contains(&"Also Interact".to_string()), "{conflicts:?}");
}
