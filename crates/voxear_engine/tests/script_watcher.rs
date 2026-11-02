//! Scripts changed while a game runs outside the editor, as in `vox run`,
//! on a copy of Hello. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

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

fn logged(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect()
}

#[test]
fn a_running_game_reloads_its_scripts_when_one_changes() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.watch_scripts();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);

    // Spinner says so the first frame after it changes.
    let file = dir.path().join("Scripts/Spinner.cs");
    let source = std::fs::read_to_string(&file).unwrap();
    let told = "private bool told;\n\n\tprotected override void Update()\n\t{\n\t\tif (!told)\n\t\t{\n\t\t\t\
                told = true;\n\t\t\tLog.Info($\"Changed {Obj.Name}\");\n\t\t}\n";
    let changed = source.replace("protected override void Update()\n\t{\n", told);
    assert_ne!(changed, source, "Spinner.cs has an Update to change");
    std::fs::write(&file, changed).unwrap();

    let started = std::time::Instant::now();
    while !logged(&log).iter().any(|line| line == "Changed Spinner A") {
        assert!(started.elapsed().as_secs() < 300, "never reloaded: {:?}", logged(&log));
        game.tick(1.0 / 60.0, None);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let lines = logged(&log);
    assert!(lines.iter().any(|l| l == "reloaded the changed scripts into the running game"), "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.starts_with("Awake    Spinner A")).count(), 1, "the game kept going");
}
