//! List and tree views the game's scripts make in code and fill: a
//! thousand names in a list that keeps rows only for those on screen, filled
//! in the frame they show in, even right after a scroll; a tree whose rows a
//! script makes its own way; and a selection the script hears of. On a copy
//! of Hello with a script of its own. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips itself
//! without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_ui::{Kind, NodeId, Ui};

const SCRIPT: &str = r#"using System.Collections.Generic;
using System.Linq;
using Voxear;

namespace Hello;

public sealed class Roster : SystemBase
{
	private readonly List<string> people = Enumerable.Range(0, 1000).Select(i => $"Player {i}").ToList();
	private readonly Dictionary<string, string[]> inside = new()
	{
		["World"] = ["Europe", "Asia"],
		["Europe"] = ["France", "Spain"],
	};

	protected override void OnCreate()
	{
		var hud = UiDocument.Create()!;
		var names = hud.Root.Add(new ListView<string>(people));
		names.SetStyle("height: 240px");
		names.SelectionChanged += () => Log.Info($"picked {names.SelectedItem}, at {names.SelectedIndex}");
		var places = hud.Root.Add(new TreeView<string>(["World"], place => inside.GetValueOrDefault(place, []))
		{
			MakeRow = row =>
			{
				row.Add(new Label("·"));
				row.Add(new Label());
			},
			BindRow = (row, place) => row.Children[1].Text = place.ToUpperInvariant(),
		});
		places.SetStyle("height: 240px");
	}

	protected override void Update()
	{
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

/// Every element in `node`, `node` first, depth first.
fn all(ui: &Ui, node: NodeId) -> Vec<NodeId> {
    let mut found = vec![node];
    let mut at = 0;
    while let Some(&next) = found.get(at) {
        found.extend(ui.children(next));
        at += 1;
    }
    found
}

/// The texts shown in `node`'s rows, top to bottom.
fn shown(ui: &Ui, node: NodeId) -> Vec<String> {
    let mut rows: Vec<NodeId> =
        all(ui, node).into_iter().filter(|&n| ui.has_class(n, "list-row") && ui.is_visible(n)).collect();
    rows.sort_by(|a, b| ui.rect(*a)[1].total_cmp(&ui.rect(*b)[1]));
    let texts = |row: NodeId| all(ui, row).into_iter().map(|n| ui.text(n)).filter(|t| !t.is_empty() && t != "·");
    rows.into_iter().filter_map(|row| texts(row).last()).collect()
}

const SIZE: (u32, u32) = (800, 600);

#[test]
fn scripts_fill_list_and_tree_views_with_the_rows_on_screen() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Roster.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let said: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {said:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // The first frame lays the lists out; from then on, each frame's rows are filled before it is drawn.
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, SIZE, 1.0);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, SIZE, 1.0);
    let errors: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    let ui = game.ui.ui();
    let find = |ui: &Ui, kind: Kind| all(ui, ui.root()).into_iter().find(|&n| ui.kind(n) == Some(kind)).unwrap();
    let (list, tree) = (find(ui, Kind::ListView), find(ui, Kind::TreeView));

    // Only the rows on screen exist, each showing its item.
    let names = shown(ui, list);
    assert_eq!(names.first().map(String::as_str), Some("Player 0"), "{names:?} {errors:#?}");
    assert!(names.len() < 20 && names.contains(&"Player 5".to_string()), "{names:?}");

    // Scrolled, the rows show the items there in the very next frame.
    ui.set_scroll(list, [0.0, 24.0 * 500.0]);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, SIZE, 1.0);
    let ui = game.ui.ui();
    let names = shown(ui, list);
    assert_eq!(names.first().map(String::as_str), Some("Player 500"), "{names:?}");

    // The tree's rows are made the script's way, closed until opened.
    assert_eq!(shown(ui, tree), ["WORLD"]);
    ui.set_expanded(tree, 1, true);
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, SIZE, 1.0);
    assert_eq!(shown(game.ui.ui(), tree), ["WORLD", "EUROPE", "ASIA"]);

    // A row clicked selects its item, which the script hears of.
    let ui = game.ui.ui();
    let row = all(ui, list).into_iter().find(|&n| {
        ui.has_class(n, "list-row") && ui.text(n).is_empty() && {
            all(ui, n).into_iter().any(|t| ui.text(t) == "Player 502")
        }
    });
    let [x, y, w, h] = ui.rect(row.expect("Player 502's row"));
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    game.tick(1.0 / 60.0, None);
    let said: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(said.contains(&"picked Player 502, at 502".to_string()), "{said:#?}");
}
