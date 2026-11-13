//! `TexturePacks` from C#: a system lists the packs there are, the engine's,
//! the project's and a player's own, wears three in turn, makes a pack to
//! start from and takes the last off, on a copy of Hello; then it opens the
//! ready-made `TexturePackScreen`, whose rows wear a pack at a click and
//! whose New Pack makes one; and the game starts again wearing the one it
//! was told to remember. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts, texture_packs};
use voxear_scripting::SaveSettings;
use voxear_terrain::blocks::block::BlockDef;
use voxear_ui::{NodeId, Ui};

const SCRIPT: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class Painter : SystemBase
{
	private int frames;

	protected override void Update()
	{
		frames++;
		if (frames == 1)
		{
			TexturePacks.Changed += pack => Log.Info($"painter changed to {pack}");
			var all = TexturePacks.All;
			var from = string.Join(", ", all.Select(pack => $"{pack.Name}:{pack.Source}"));
			Log.Info($"painter has {from}");
			Log.Info($"painter says {all.First(pack => pack.Name == "Old").Description}");
			Log.Info($"painter wears {TexturePacks.Current} folder {TexturePacks.Folder != null}");
		}
		if (frames == 3)
		{
			TexturePacks.Use(TexturePack.PixelGrain);
		}
		if (frames == 6)
		{
			Log.Info($"painter wears {TexturePacks.Current.Name} from {TexturePacks.Current.Source}");
			TexturePacks.Use(TexturePack.Load("Packs/Old/Old.texturepack"));
		}
		if (frames == 9)
		{
			Log.Info($"painter wears {TexturePacks.Current.Name} from {TexturePacks.Current.Source}");
			TexturePacks.Use(TexturePacks.All.First(pack => pack.Source == TexturePackSource.Player), remember: false);
		}
		if (frames == 12)
		{
			Log.Info($"painter wears {TexturePacks.Current.Name} from {TexturePacks.Current.Source}");
			var fresh = TexturePacks.WriteStarter("Fresh");
			var again = TexturePacks.WriteStarter("Fresh");
			Log.Info($"painter started {fresh} from {fresh.Source}, again {again}");
			TexturePacks.UseNone(remember: false);
		}
		if (frames == 15)
		{
			Log.Info($"painter wears {TexturePacks.Current}");
			var screen = TexturePackScreen.Open("Textures")!;
			screen.Picked += pack => Log.Info($"picked {pack}");
			screen.Closed += () => Log.Info("closed the textures");
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

fn picture(file: &std::path::Path, color: [u8; 4]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(file).unwrap(), 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&color.repeat(4)).unwrap();
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
fn label(ui: &Ui, row: NodeId, class: &str) -> String {
    let label = ui.children(row).into_iter().find(|&label| ui.has_class(label, class));
    label.map(|label| ui.text(label).to_string()).unwrap_or_default()
}

#[test]
fn a_script_lists_texture_packs_wears_them_and_players_pick_one_from_the_ready_made_screen() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Painter.cs"), SCRIPT).unwrap();
    // Two blocks, a pack of the project's for one of them, and a pack of the player's in the Textures folder
    // beside the saves.
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Grass.block"), BlockDef::colored([0.2, 0.6, 0.1]).to_text()).unwrap();
    std::fs::write(dir.path().join("Blocks/Stone.block"), BlockDef::colored([0.5, 0.5, 0.52]).to_text()).unwrap();
    picture(&dir.path().join("Packs/Old/grass.png"), [40, 160, 40, 255]);
    let old = "description = \"Older than the hills.\"\n[[block]]\nblock = \"Blocks/Grass.block\"\n\
               all = \"Packs/Old/grass.png\"\n";
    std::fs::write(dir.path().join("Packs/Old/Old.texturepack"), old).unwrap();
    let data = tempfile::tempdir().unwrap();
    let saves = data.path().join("Saves");
    let textures = data.path().join(texture_packs::TEXTURES_FOLDER);
    picture(&textures.join("Mine/stone.png"), [90, 90, 96, 255]);
    let mine = "pixels = true\n[[block]]\nblock = \"Blocks/Stone.block\"\nall = \"stone.png\"\n";
    std::fs::write(textures.join("Mine/Mine.texturepack"), mine).unwrap();

    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // The project's pack is named by its GUID, which a new name for its file would leave as it is.
    let old = game.content.parse("Packs/Old/Old.texturepack").unwrap().guid().expect("an asset").to_string();
    let lines = |log: &LogCapture| -> Vec<String> {
        log.lock().unwrap().lines.iter().map(|l| format!("{} {}", l.target, l.message)).collect()
    };
    let said = |log: &LogCapture, words: &str| lines(log).iter().any(|line| line.contains(words));
    let ticks = |game: &mut Game, frames: u32| {
        for _ in 0..frames {
            game.tick(1.0 / 60.0, None);
            game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
        }
    };

    // The engine's own first, then the project's, then the player's; none worn to begin with.
    ticks(&mut game, 2);
    assert!(
        said(&log, "painter has Pixel Grain:BuiltIn, Soft Grain:BuiltIn, Old:Game, Mine:Player"),
        "{:#?}",
        lines(&log)
    );
    assert!(said(&log, "painter says Older than the hills."), "{:#?}", lines(&log));
    assert!(said(&log, "painter wears no texture pack folder True"), "{:#?}", lines(&log));
    assert!(texture_packs::worn(&game.content).is_none());

    // A built-in pack, kept beside the saves.
    ticks(&mut game, 3);
    assert_eq!(texture_packs::worn(&game.content), Some("builtin:pixel_grain"));
    assert_eq!(texture_packs::remembered().as_deref(), Some("builtin:pixel_grain"));

    // The project's, by its path.
    ticks(&mut game, 3);
    assert!(said(&log, "painter wears Pixel Grain from BuiltIn"), "{:#?}", lines(&log));
    assert_eq!(texture_packs::worn(&game.content), Some(old.as_str()));
    assert_eq!(texture_packs::remembered().as_deref(), Some(old.as_str()));

    // The player's own folder, to try: worn, and not kept.
    ticks(&mut game, 3);
    assert!(said(&log, "painter wears Old from Game"), "{:#?}", lines(&log));
    assert!(texture_packs::worn(&game.content).is_some_and(|worn| worn.ends_with("Mine.texturepack")));
    assert_eq!(texture_packs::remembered().as_deref(), Some(old.as_str()), "a pack tried is not remembered");

    // A pack to start from, in the player's folder: every block listed, each with a picture; and none of that
    // name twice. Then none worn.
    ticks(&mut game, 3);
    assert!(said(&log, "painter wears Mine from Player"), "{:#?}", lines(&log));
    assert!(said(&log, "painter started Fresh from Player, again no texture pack"), "{:#?}", lines(&log));
    let fresh = std::fs::read_to_string(textures.join("Fresh/Fresh.texturepack")).unwrap();
    assert!(fresh.contains("block = \"Blocks/Grass.block\"") && fresh.contains("all = \"Stone.png\""), "{fresh}");
    assert!(textures.join("Fresh/Grass.png").is_file());
    assert!(texture_packs::worn(&game.content).is_none());
    for changed in ["Pixel Grain", "Old", "Mine", "no texture pack"] {
        assert!(said(&log, &format!("painter changed to {changed}")), "{changed}: {:#?}", lines(&log));
    }

    // The ready-made screen: a panel down the left, a row a pack under None, the player's marked as theirs.
    ticks(&mut game, 4);
    assert!(said(&log, "painter wears no texture pack"), "{:#?}", lines(&log));
    let ui = game.ui.ui();
    let screen = find(ui, |n| ui.has_class(n, "texture-pack-screen"));
    assert_eq!(screen.len(), 1, "the screen is open");
    let [left, top, wide, tall] = ui.rect(screen[0]);
    assert!(left == 0.0 && top == 0.0 && tall == 720.0, "down the left: {:?}", ui.rect(screen[0]));
    assert!((300.0..=520.0).contains(&wide), "a panel, not the whole screen: {wide}");
    let names = |ui: &Ui| -> Vec<String> {
        find(ui, |n| ui.has_class(n, "tps-row")).iter().map(|&row| label(ui, row, "tps-name")).collect()
    };
    assert_eq!(names(ui), ["None", "Pixel Grain", "Soft Grain", "Old", "Fresh", "Mine"]);
    let rows = find(ui, |n| ui.has_class(n, "tps-row"));
    assert_eq!(label(ui, rows[3], "tps-description"), "Older than the hills.");
    let yours: Vec<bool> = rows.iter().map(|&row| label(ui, row, "tps-source") == "Yours").collect();
    assert_eq!(yours, [false, false, false, false, true, true]);
    let marked = |ui: &Ui| -> Vec<String> {
        let rows = find(ui, |n| ui.has_class(n, "tps-row") && ui.has_class(n, "selected"));
        rows.iter().map(|&row| label(ui, row, "tps-name")).collect()
    };
    assert_eq!(marked(ui), ["None"]);
    // Each row is inside the panel, under the one before, its description under its name.
    let mut under = top;
    for &row in &rows {
        let [x, y, w, h] = ui.rect(row);
        assert!(x >= left && x + w <= left + wide + 0.5 && y >= under && h >= 40.0, "{:?}", ui.rect(row));
        under = y + h;
    }
    let folder = find(ui, |n| ui.has_class(n, "tps-folder"));
    assert!(ui.text(folder[0]).contains(&textures.display().to_string()), "{}", ui.text(folder[0]));

    // Soft Grain clicked: worn from the next frame, marked, kept for the next run, and told.
    click(game.ui.ui(), rows[2]);
    ticks(&mut game, 2);
    assert_eq!(texture_packs::worn(&game.content), Some("builtin:soft_grain"));
    assert_eq!(texture_packs::remembered().as_deref(), Some("builtin:soft_grain"));
    assert_eq!(marked(game.ui.ui()), ["Soft Grain"]);
    assert!(said(&log, "picked Soft Grain"), "{:#?}", lines(&log));

    // New Pack makes one of the player's own and lists it, the pack worn still marked.
    let ui = game.ui.ui();
    let new = find(ui, |n| ui.text(n) == "New Pack");
    click(game.ui.ui(), new[0]);
    ticks(&mut game, 2);
    assert_eq!(names(game.ui.ui()), ["None", "Pixel Grain", "Soft Grain", "Old", "Fresh", "Mine", "My Pack"]);
    assert!(textures.join("My Pack/My Pack.texturepack").is_file());
    assert_eq!(marked(game.ui.ui()), ["Soft Grain"]);

    // The player's own, then Close.
    let ui = game.ui.ui();
    let rows = find(ui, |n| ui.has_class(n, "tps-row"));
    click(game.ui.ui(), rows[5]);
    ticks(&mut game, 2);
    assert!(texture_packs::worn(&game.content).is_some_and(|worn| worn.ends_with("Mine.texturepack")));
    assert_eq!(marked(game.ui.ui()), ["Mine"]);
    let ui = game.ui.ui();
    let close = find(ui, |n| ui.text(n) == "Close");
    click(game.ui.ui(), close[0]);
    ticks(&mut game, 2);
    let ui = game.ui.ui();
    assert!(find(ui, |n| ui.has_class(n, "texture-pack-screen")).is_empty(), "closed");
    assert!(said(&log, "closed the textures"), "{:#?}", lines(&log));

    // The game started again wears the one remembered, from its first frame.
    let mine = texture_packs::remembered().expect("the player's, picked on the screen");
    texture_packs::wear(&mut game.content, "").unwrap();
    game.wear_remembered_textures();
    assert_eq!(texture_packs::worn(&game.content), Some(mine.as_str()));
    // One that is gone by then is forgotten, and the game starts as it was made.
    std::fs::remove_dir_all(textures.join("Mine")).unwrap();
    texture_packs::wear(&mut game.content, "").unwrap();
    game.wear_remembered_textures();
    assert!(texture_packs::worn(&game.content).is_none() && texture_packs::remembered().is_none());
}
