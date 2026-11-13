//! `LookPacks` from C#: a system lists the packs there are, the engine's,
//! the project's and a player's own, wears three in turn and takes the last
//! off, on a copy of Hello; then the game starts again wearing the one it
//! was told to remember. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET.

use voxear_engine::{Engine, Game, Project, look_packs, scripts};
use voxear_render::{CloudKind, WornLook};
use voxear_scripting::SaveSettings;

const SCRIPT: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class Wardrobe : SystemBase
{
	private int frames;

	protected override void Update()
	{
		frames++;
		if (frames == 1)
		{
			LookPacks.Changed += pack => Log.Info($"wardrobe changed to {pack}");
			var all = LookPacks.All;
			var from = string.Join(", ", all.Select(pack => $"{pack.Name}:{pack.Source}"));
			Log.Info($"wardrobe has {from}");
			Log.Info($"wardrobe says {all.First(pack => pack.Name == "Cosy").Description}");
			Log.Info($"wardrobe wears {LookPacks.Current} folder {LookPacks.Folder != null}");
		}
		if (frames == 3)
		{
			LookPacks.Use(LookPack.CubeClouds);
		}
		if (frames == 6)
		{
			Log.Info($"wardrobe wears {LookPacks.Current.Name} from {LookPacks.Current.Source}");
			LookPacks.Use(LookPack.Load("Looks/Cosy.lookpack"));
		}
		if (frames == 9)
		{
			Log.Info($"wardrobe wears {LookPacks.Current.Name} from {LookPacks.Current.Source}");
			LookPacks.Use(LookPacks.All.First(pack => pack.Source == LookPackSource.Player), remember: false);
		}
		if (frames == 12)
		{
			Log.Info($"wardrobe wears {LookPacks.Current.Name} from {LookPacks.Current.Source}");
			LookPacks.UseNone(remember: false);
		}
		if (frames == 15)
		{
			Log.Info($"wardrobe wears {LookPacks.Current}");
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
fn a_script_lists_look_packs_wears_them_and_the_game_starts_in_the_one_remembered() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Wardrobe.cs"), SCRIPT).unwrap();
    // A pack of the project's, and one of the player's, in the Looks folder beside the saves.
    std::fs::create_dir_all(dir.path().join("Looks")).unwrap();
    let cosy = "description = \"Warm and close.\"\n[post.bloom]\nintensity = 0.33\n";
    std::fs::write(dir.path().join("Looks/Cosy.lookpack"), cosy).unwrap();
    let data = tempfile::tempdir().unwrap();
    let saves = data.path().join("Saves");
    let looks = data.path().join(look_packs::LOOKS_FOLDER);
    std::fs::create_dir_all(&looks).unwrap();
    std::fs::write(looks.join("Mine.lookpack"), "[post.vignette]\nintensity = 0.77\n").unwrap();

    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // The project's pack is named by its GUID, which a new name for its file would leave as it is.
    let cosy = game.content.parse("Looks/Cosy.lookpack").unwrap().guid().expect("an asset").to_string();
    let lines = |log: &voxear_core::logging::LogCapture| -> Vec<String> {
        log.lock().unwrap().lines.iter().map(|l| format!("{} {}", l.target, l.message)).collect()
    };
    let said = |log: &voxear_core::logging::LogCapture, words: &str| lines(log).iter().any(|line| line.contains(words));
    let ticks = |game: &mut Game, frames: u32| (0..frames).for_each(|_| game.tick(1.0 / 60.0, None));

    // The engine's own first, then the project's, then the player's; none worn to begin with.
    ticks(&mut game, 2);
    let listed = "wardrobe has Sunlit:BuiltIn, Cube Clouds:BuiltIn, Little Cubes:BuiltIn, Painted:BuiltIn, \
                  Old Film:BuiltIn, Plain:BuiltIn, Cosy:Game, Mine:Player";
    assert!(said(&log, listed), "{:#?}", lines(&log));
    assert!(said(&log, "wardrobe says Warm and close."), "{:#?}", lines(&log));
    assert!(said(&log, "wardrobe wears no look pack folder True"), "{:#?}", lines(&log));
    assert!(game.engine.world.resource::<WornLook>().is_none());

    // A built-in pack: the level's clouds, whatever they are, are built of cubes.
    ticks(&mut game, 3);
    assert_eq!(look_packs::worn(&game.engine.world), Some("builtin:cube_clouds"));
    assert_eq!(game.engine.world.resource::<WornLook>().map(|worn| worn.sky.clouds.kind), Some(CloudKind::Blocks));
    assert_eq!(look_packs::remembered().as_deref(), Some("builtin:cube_clouds"), "kept beside the saves");

    // The project's, by its path: its glow over the level's own post-processing.
    ticks(&mut game, 3);
    assert!(said(&log, "wardrobe wears Cube Clouds from BuiltIn"), "{:#?}", lines(&log));
    let own = game.engine.world.resource::<voxear_render::PostProcess>().copied().unwrap_or_default();
    let worn = *game.engine.world.resource::<WornLook>().expect("the project's pack is worn");
    assert_eq!((worn.post.bloom.intensity, worn.post.tonemap), (0.33, own.tonemap));
    assert_ne!(own.bloom.intensity, 0.33, "the level's own is as it was");
    assert_eq!(look_packs::remembered().as_deref(), Some(cosy.as_str()));

    // The player's own file, to try: worn, and not kept.
    ticks(&mut game, 3);
    assert!(said(&log, "wardrobe wears Cosy from Game"), "{:#?}", lines(&log));
    assert_eq!(game.engine.world.resource::<WornLook>().map(|worn| worn.post.vignette.intensity), Some(0.77));
    assert_eq!(look_packs::remembered().as_deref(), Some(cosy.as_str()), "a pack tried is not remembered");

    // None: the level as it was made.
    ticks(&mut game, 6);
    assert!(said(&log, "wardrobe wears Mine from Player"), "{:#?}", lines(&log));
    assert!(said(&log, "wardrobe wears no look pack"), "{:#?}", lines(&log));
    assert!(game.engine.world.resource::<WornLook>().is_none() && look_packs::worn(&game.engine.world).is_none());
    for changed in ["Cube Clouds", "Cosy", "Mine", "no look pack"] {
        assert!(said(&log, &format!("wardrobe changed to {changed}")), "{changed}: {:#?}", lines(&log));
    }

    // The game started again wears the one remembered, from its first frame.
    game.wear_remembered_look();
    game.tick(1.0 / 60.0, None);
    assert_eq!(look_packs::worn(&game.engine.world), Some(cosy.as_str()));
    assert_eq!(game.engine.world.resource::<WornLook>().map(|worn| worn.post.bloom.intensity), Some(0.33));
    // One that is gone by then is forgotten, and the game starts as it was made.
    std::fs::remove_file(dir.path().join("Looks/Cosy.lookpack")).unwrap();
    look_packs::wear(&mut game.engine.world, &mut game.content, "").unwrap();
    game.wear_remembered_look();
    assert!(look_packs::worn(&game.engine.world).is_none() && look_packs::remembered().is_none());
}
