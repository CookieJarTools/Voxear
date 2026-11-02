//! C# hot reload in the editor, on a copy of the Hello sample. A file of its
//! own, so it runs in a process of its own: .NET starts only once per
//! process, and the play test in `editor.rs` starts it too. Skips itself
//! without a GPU or .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};

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

/// The editor open on `project`, a frame in, its Console showing `log`, or
/// `None` without a GPU.
fn open_project(project: Project, log: LogCapture) -> Option<(Renderer, voxear_editor::Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log).unwrap();
    editor.update(&mut renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    Some((renderer, editor))
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// What has been logged, line by line.
fn logged(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect()
}

/// Runs frames until a logged line starts with `start`, for up to five
/// minutes, as the scripts build.
fn frames_until_logged(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, log: &LogCapture, start: &str) {
    let began = std::time::Instant::now();
    while !logged(log).iter().any(|line| line.starts_with(start)) {
        assert!(began.elapsed().as_secs() < 300, "nothing logged {start:?} in five minutes: {:?}", logged(log));
        let failed = logged(log).into_iter().any(|l| l.contains("failed to build") || l.contains("could not reload"));
        assert!(editor.play_error().is_none() && !failed, "{:?}", logged(log));
        frame(editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn scripts_changed_while_playing_reload_into_the_running_game() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    // A copy, since its scripts change.
    let dir = hello_copy();
    // For every thread, so the compiler's complaints from the build's are kept too.
    let log = LogCapture::default();
    let _ = tracing::subscriber::set_global_default(voxear_core::logging::capturing_only(log.clone()));
    let Some((mut renderer, mut editor)) = open_project(Project::open(dir.path()).unwrap(), log.clone()) else {
        return;
    };
    editor.play(&mut renderer);
    frames_until_logged(&mut editor, &mut renderer, &log, "Start    Spinner");
    for _ in 0..10 {
        frame(&mut editor, &mut renderer);
    }
    assert!(editor.is_playing(), "{:?}", editor.play_error());
    let score = logged(&log).iter().filter_map(|l| l.split("score now ").nth(1)?.parse::<u32>().ok()).max().unwrap();

    // Spinner says what it holds the first frame after it changes.
    let file = dir.path().join("Scripts/Spinner.cs");
    let source = std::fs::read_to_string(&file).unwrap();
    let told = "private bool told;\n\n\tprotected override void Update()\n\t{\n\t\tif (!told)\n\t\t{\n\t\t\ttold = true;\n\
                \t\t\tLog.Info($\"Reloaded {Obj.Name}: {degreesPerSecond} degrees, leader {leader?.Obj.Name ?? \"none\"}, \
                score {Instance<GameManager>.Current?.Score}\");\n\t\t}\n";
    let changed = source.replace("protected override void Update()\n\t{\n", told);
    assert_ne!(changed, source, "Spinner.cs has an Update to change");
    std::fs::write(&file, changed).unwrap();
    frames_until_logged(&mut editor, &mut renderer, &log, "Reloaded Spinner A");
    frame(&mut editor, &mut renderer);

    let lines = logged(&log);
    assert!(editor.is_playing(), "the game kept going");
    assert!(lines.iter().any(|l| l.starts_with("Reloaded the game scripts, carrying over")), "{lines:?}");
    for (spinner, leader) in [("Spinner A", "none"), ("Spinner B", "Spinner A"), ("Pyramid", "none")] {
        let expected = format!("Reloaded {spinner}: 90 degrees, leader {leader}, score {score}");
        assert!(lines.contains(&expected), "fields, references and the GameManager's score carry over: {lines:?}");
    }
    let count = |start: &str| lines.iter().filter(|l| l.starts_with(start)).count();
    assert_eq!(count("Awake    Spinner A"), 1, "Awake does not run again");
    assert_eq!(count("OnEnable Spinner A"), 2, "OnDisable and OnEnable do, as in Unity");

    // The old scripts leave memory once the reload settles, and the next
    // Play starts at once with the scripts as they are.
    for _ in 0..130 {
        frame(&mut editor, &mut renderer);
    }
    let held = logged(&log).into_iter().find(|l| l.contains("still in memory"));
    assert_eq!(held, None, "nothing holds the old scripts");
    editor.stop(&mut renderer);
    editor.play(&mut renderer);
    assert!(editor.is_playing(), "no build needed: the reloaded scripts are current");
    editor.stop(&mut renderer);
}
