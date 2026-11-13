//! The ready-made menu of look packs from a script: `LookPackScreen.Open()`
//! lists every pack there is under None, each with what it looks like, down
//! the screen's left; a row clicked wears its pack and is marked; and Close
//! takes the screen away. On a copy of Hello. A file of its own, so it runs
//! in a process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, look_packs, scripts};
use voxear_scripting::SaveSettings;
use voxear_ui::{NodeId, Ui};

const SCRIPT: &str = r#"using Voxear;

namespace Hello;

public sealed class Wardrobe : SystemBase
{
	protected override void OnCreate()
	{
		var screen = LookPackScreen.Open("Looks")!;
		screen.Picked += pack => Log.Info($"picked {pack}");
		screen.Closed += () => Log.Info("closed the looks");
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
fn players_pick_a_look_pack_from_the_ready_made_screen() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Wardrobe.cs"), SCRIPT).unwrap();
    std::fs::create_dir_all(dir.path().join("Looks")).unwrap();
    let cosy = "description = \"Warm and close.\"\n[post.bloom]\nintensity = 0.33\n";
    std::fs::write(dir.path().join("Looks/Cosy.lookpack"), cosy).unwrap();
    let data = tempfile::tempdir().unwrap();
    let looks = data.path().join(look_packs::LOOKS_FOLDER);
    std::fs::create_dir_all(&looks).unwrap();
    std::fs::write(looks.join("Mine.lookpack"), "[post.vignette]\nintensity = 0.77\n").unwrap();

    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(data.path().join("Saves")), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let frame = |game: &mut Game| {
        game.tick(1.0 / 60.0, None);
        game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
    };
    frame(&mut game);
    frame(&mut game);
    let script = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };

    // The screen is a panel down the left of the game, as high as it, with the game showing beside it.
    let ui = game.ui.ui();
    let screen = find(ui, |n| ui.has_class(n, "look-pack-screen"));
    assert_eq!(screen.len(), 1, "the screen is open");
    let [left, top, wide, tall] = ui.rect(screen[0]);
    assert!(left == 0.0 && top == 0.0 && tall == 720.0, "down the left: {:?}", ui.rect(screen[0]));
    assert!((300.0..=520.0).contains(&wide), "a panel, not the whole screen: {wide}");
    assert!(!find(ui, |n| ui.has_class(n, "lps-title") && ui.text(n) == "Looks").is_empty(), "its title");

    // A row a pack under None: the engine's, the project's, then the player's own, marked as theirs.
    let rows = find(ui, |n| ui.has_class(n, "lps-row"));
    let names: Vec<String> = rows.iter().map(|&row| said(ui, row, "lps-name")).collect();
    let listed = ["None", "Sunlit", "Cube Clouds", "Little Cubes", "Painted", "Old Film", "Plain", "Cosy", "Mine"];
    assert_eq!(names, listed);
    assert_eq!(said(ui, rows[7], "lps-description"), "Warm and close.");
    assert!(rows[..7].iter().all(|&row| said(ui, row, "lps-description").len() > 20), "each says what it looks like");
    let yours: Vec<bool> = rows.iter().map(|&row| said(ui, row, "lps-source") == "Yours").collect();
    assert_eq!(yours, [false, false, false, false, false, false, false, false, true]);
    // None is marked, as nothing is worn yet.
    let marked = |ui: &Ui| -> Vec<String> {
        let rows = find(ui, |n| ui.has_class(n, "lps-row") && ui.has_class(n, "selected"));
        rows.iter().map(|&row| said(ui, row, "lps-name")).collect()
    };
    assert_eq!(marked(ui), ["None"]);

    // The rows are laid out one under another inside the panel, each tall enough for its name over its description,
    // which wraps within the row rather than running out of it.
    let mut under = top;
    for &row in &rows {
        let [x, y, w, h] = ui.rect(row);
        assert!(
            x >= left && x + w <= left + wide + 0.5,
            "{} is inside the panel: {:?}",
            said(ui, row, "lps-name"),
            [x, w]
        );
        assert!(y >= under && h >= 40.0, "{} is under the row before: {:?}", said(ui, row, "lps-name"), [y, h]);
        under = y + h;
        let labels = ui.children(row);
        let name = labels.iter().copied().find(|&l| ui.has_class(l, "lps-name")).expect("a name");
        let description = labels.iter().copied().find(|&l| ui.has_class(l, "lps-description")).expect("a description");
        let ([_, name_y, _, name_h], [dx, dy, dw, dh]) = (ui.rect(name), ui.rect(description));
        assert!(dy >= name_y + name_h - 0.5, "the description is under the name: {dy} against {name_y} + {name_h}");
        assert!(
            dx >= x && dx + dw <= x + w + 0.5 && dy + dh <= y + h + 0.5,
            "and inside its row: {:?}",
            [dx, dy, dw, dh]
        );
    }
    // Where packs of the player's own go is said under the list.
    let folder = find(ui, |n| ui.has_class(n, "lps-folder"));
    assert!(ui.text(folder[0]).ends_with(&looks.display().to_string()), "{}", ui.text(folder[0]));

    // Cube Clouds clicked: worn from the next frame, marked in None's place, kept for the next run, and told.
    click(game.ui.ui(), rows[2]);
    frame(&mut game);
    frame(&mut game);
    assert_eq!(look_packs::worn(&game.engine.world), Some("builtin:cube_clouds"));
    assert_eq!(look_packs::remembered().as_deref(), Some("builtin:cube_clouds"));
    assert_eq!(marked(game.ui.ui()), ["Cube Clouds"]);
    assert!(script(&log).contains(&"picked Cube Clouds".to_string()), "{:#?}", script(&log));

    // The player's own, then None again.
    let ui = game.ui.ui();
    let rows = find(ui, |n| ui.has_class(n, "lps-row"));
    click(game.ui.ui(), rows[8]);
    frame(&mut game);
    frame(&mut game);
    assert!(look_packs::worn(&game.engine.world).is_some_and(|worn| worn.ends_with("Mine.lookpack")));
    assert_eq!(marked(game.ui.ui()), ["Mine"]);
    let ui = game.ui.ui();
    let rows = find(ui, |n| ui.has_class(n, "lps-row"));
    click(game.ui.ui(), rows[0]);
    frame(&mut game);
    frame(&mut game);
    assert!(look_packs::worn(&game.engine.world).is_none() && look_packs::remembered().is_none());
    assert_eq!(marked(game.ui.ui()), ["None"]);

    // Close takes the screen away.
    let ui = game.ui.ui();
    let close = find(ui, |n| ui.text(n) == "Close");
    click(game.ui.ui(), close[0]);
    frame(&mut game);
    frame(&mut game);
    let ui = game.ui.ui();
    assert!(find(ui, |n| ui.has_class(n, "look-pack-screen")).is_empty(), "closed");
    assert!(script(&log).contains(&"closed the looks".to_string()), "{:#?}", script(&log));
}
