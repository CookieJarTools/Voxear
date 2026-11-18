//! The ready-made menu of shader packs from a script: `ShaderPackScreen.Open()`
//! lists every pack there is under None; a row clicked wears its pack,
//! compiled on a thread while the status line says so; and the pack worn's
//! options show under the list, a slider, a toggle and a dropdown, which set
//! them, kept beside the saves. On a copy of Hello. A file of its own, so it
//! runs in a process of its own, since .NET starts only once per process.
//! Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts, shader_packs};
use voxear_render::shader_packs::WornShaderPack;
use voxear_scripting::SaveSettings;
use voxear_ui::{NodeId, Ui};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Menu : SystemBase
{
	protected override void OnCreate()
	{
		var screen = ShaderPackScreen.Open("Shaders")!;
		screen.Picked += pack => Log.Info($"picked {pack}");
	}

	protected override void Update()
	{
	}
}
"#;

const GRADE: &str =
    "float4 run(PassInput input)\n{\n    return float4(sceneColor(input.uv).rgb * options().strength, 1.0);\n}\n";

const MANIFEST: &str = r#"
description = "A test grade."

[options]
strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength", about = "How bright" }
bright = { kind = "toggle", default = false, label = "Bright" }
look = { kind = "choice", choices = ["Soft", "Hard"], default = "Soft", label = "Look" }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color"]
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
        stack.extend(ui.children(node).into_iter().rev());
    }
    found
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

/// The text of the first of `row`'s labels with `class`.
fn said(ui: &Ui, row: NodeId, class: &str) -> String {
    let label = ui.children(row).into_iter().find(|&label| ui.has_class(label, class));
    label.map(|label| ui.text(label).to_string()).unwrap_or_default()
}

#[test]
fn players_pick_a_shader_pack_and_set_its_options_from_the_ready_made_screen() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Menu.cs"), SCRIPT).unwrap();
    for (path, text) in [("Packs/Warm.shaderpack/pack.toml", MANIFEST), ("Packs/Warm.shaderpack/grade.slang", GRADE)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let data = tempfile::tempdir().unwrap();
    let saves = data.path().join("Saves");
    let mine = data.path().join(shader_packs::SHADER_PACKS_FOLDER).join("Mine.shaderpack");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("pack.toml"), MANIFEST).unwrap();
    std::fs::write(mine.join("grade.slang"), GRADE).unwrap();

    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let frame = |game: &mut Game| {
        game.tick(1.0 / 60.0, None);
        game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    };
    frame(&mut game);
    frame(&mut game);

    // Down the left of the game: a row a pack under None, the player's own marked as theirs, None marked worn.
    let ui = game.ui.ui();
    let screen = find(ui, |n| ui.has_class(n, "shader-pack-screen"));
    assert_eq!(screen.len(), 1, "the screen is open");
    let [left, top, wide, tall] = ui.rect(screen[0]);
    assert!(left == 0.0 && top == 0.0 && tall == 720.0 && (300.0..=520.0).contains(&wide), "{:?}", ui.rect(screen[0]));
    let rows = find(ui, |n| ui.has_class(n, "sps-row"));
    let names: Vec<String> = rows.iter().map(|&row| said(ui, row, "sps-name")).collect();
    assert_eq!(names, ["None", "Evening", "Warm", "Mine"]);
    assert_eq!(said(ui, rows[2], "sps-description"), "A test grade.");
    assert_eq!(said(ui, rows[3], "sps-source"), "Yours");
    assert!(find(ui, |n| ui.has_class(n, "sps-option")).is_empty(), "no options with none worn");

    // The project's pack clicked: compiling, then drawn, its options under the list.
    click(game.ui.ui(), rows[2]);
    let started = std::time::Instant::now();
    while game.engine.world.resource::<WornShaderPack>().is_none() && started.elapsed().as_secs() < 300 {
        frame(&mut game);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    frame(&mut game);
    frame(&mut game);
    let ui = game.ui.ui();
    let status = find(ui, |n| ui.has_class(n, "sps-status"));
    assert_eq!(ui.text(status[0]), "", "drawn: nothing to say");
    let options = find(ui, |n| ui.has_class(n, "sps-option"));
    let labels: Vec<String> = options.iter().map(|&row| said(ui, row, "sps-label")).collect();
    assert_eq!(labels, ["Strength", "Bright", "Look"]);
    let controls: Vec<NodeId> = options
        .iter()
        .map(|&row| *ui.children(row).iter().find(|&&c| ui.has_class(c, "sps-control")).unwrap())
        .collect();
    let [x, y, w, h] = ui.rect(controls[0]);
    assert!(x > left && x + w <= left + wide + 0.5 && h > 0.0, "the slider beside its label: {:?}", [x, y, w, h]);

    // The toggle and the slider's far end clicked: the options set, given to the frame and kept.
    click(game.ui.ui(), controls[1]);
    let ui = game.ui.ui();
    let [x, y, w, h] = ui.rect(controls[0]);
    let ui = game.ui.ui();
    ui.pointer_move(x + w - 1.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    frame(&mut game);
    frame(&mut game);
    let worn = game.engine.world.resource::<WornShaderPack>().unwrap();
    assert!(worn.options[0] > 1.8, "the slider's far end: {}", worn.options[0]);
    assert_eq!(worn.options[1], 1.0, "the toggle on");
    let (reference, kept) = shader_packs::remembered(&saves).expect("kept beside the saves");
    assert_eq!(Some(reference.as_str()), shader_packs::worn(&game.engine.world));
    assert!(kept.iter().any(|(key, value)| key == "bright" && *value == 1.0), "{kept:?}");
    let script = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    assert!(script(&log).contains(&"picked Warm".to_string()), "{:#?}", script(&log));
}
