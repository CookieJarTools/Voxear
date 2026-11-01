//! Single scenes and level changes from C#, through the handle-free API:
//! `Scene.LoadAsync`, `Scene.Unload`, and a held `Level.LoadAsync` driven by
//! `Level.IsReady`, `Level.Progress` and `Level.Activate()`, on a copy of
//! Hello with a script of its own. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class SceneSteps : SystemBase
{
	private int step;

	protected override void OnCreate()
	{
		Log.Info($"before: progress {Level.Progress}, next {Level.Next ?? "none"}");
		Scene.LoadAsync("Scenes/Night.scene", new() { SetActive = true });
	}

	protected override void Update()
	{
		switch (step)
		{
			case 0 when !Scene.IsLoading:
				Log.Info($"scene in: {string.Join(", ", Scene.Loaded)}; active {Scene.Active}; error {Scene.Current?.Error ?? "none"}");
				Scene.Unload("Scenes/Night.scene");
				step++;
				break;
			case 1:
				Log.Info($"after unload: {string.Join(", ", Scene.Loaded)}");
				Level.LoadAsync("Levels/Night.level", LevelLoadOptions.Held);
				step++;
				break;
			case 2 when Level.IsReady:
				Log.Info($"held at {Level.Progress:0.0} for {Level.Next}");
				Level.Activate();
				step++;
				break;
			case 3 when !Level.IsSwitching:
				Log.Info($"level now {Level.Active}; progress {Level.Progress}; error {Level.Error ?? "none"}");
				step++;
				break;
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

#[test]
fn scripts_load_single_scenes_and_drive_a_held_level_change_without_a_handle() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/SceneSteps.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    let started = std::time::Instant::now();
    while !said(&log).iter().any(|line| line.starts_with("level now")) {
        assert!(started.elapsed().as_secs() < 20, "the steps stopped: {:#?}", said(&log));
        game.tick(1.0 / 60.0, None);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let lines = said(&log);
    let line =
        |start: &str| lines.iter().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("{start}: {lines:#?}"));
    assert_eq!(line("before:"), "before: progress 1, next none", "with no change going on");
    let scene_in = line("scene in:");
    assert!(scene_in.contains("Scenes/Day.scene") && scene_in.contains("Scenes/Night.scene"), "{scene_in}");
    assert!(scene_in.contains("active Scenes/Night.scene; error none"), "{scene_in}");
    assert!(!line("after unload:").contains("Night"), "{lines:#?}");
    assert_eq!(line("held at"), "held at 0.9 for Levels/Night.level");
    assert_eq!(line("level now"), "level now Levels/Night.level; progress 1; error none");
}
