//! `ShaderPacks` from C#: a system lists the packs there are, the engine's,
//! the project's and a player's own, wears the project's, waits for it to
//! compile and sets its options, on a copy of Hello; the choice and the
//! options are kept beside the saves, the compiled programs in the
//! ShaderPacks folder there, and the game started again wears the pack with
//! them. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET.

use voxear_engine::{Engine, Game, Project, scripts, shader_packs};
use voxear_render::shader_packs::WornShaderPack;
use voxear_scripting::SaveSettings;

const SCRIPT: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class Shaders : SystemBase
{
	private int frames;
	private bool set;

	protected override void Update()
	{
		frames++;
		if (frames == 1)
		{
			var all = ShaderPacks.All;
			var from = string.Join(", ", all.Select(pack => $"{pack.Name}:{pack.Source}"));
			Log.Info($"shaders has {from}");
			Log.Info($"shaders says {all.First(pack => pack.Name == "Warm").Description}");
			Log.Info($"shaders wears {ShaderPacks.Current} {ShaderPacks.Standing} folder {ShaderPacks.Folder != null}");
		}
		if (frames == 3)
		{
			ShaderPacks.Use(ShaderPack.Load("Packs/Warm.shaderpack/pack.toml"));
		}
		if (frames > 3 && !set && ShaderPacks.Standing == ShaderPackStanding.Drawn)
		{
			set = true;
			var options = string.Join(", ", ShaderPacks.Options.Select(option => option.ToString()));
			Log.Info($"shaders wears {ShaderPacks.Current.Name} from {ShaderPacks.Current.Source} with {options}");
			ShaderPacks.SetOption("strength", 1.5f);
			ShaderPacks.SetOption("look", "Hard");
			ShaderPacks.SetOption("glow", true);
		}
		if (set && frames % 10 == 0)
		{
			var options = string.Join(", ", ShaderPacks.Options.Select(option => option.ToString()));
			Log.Info($"shaders set {options}");
		}
	}
}
"#;

const GRADE: &str =
    "float4 run(PassInput input)\n{\n    return float4(sceneColor(input.uv).rgb * options().strength, 1.0);\n}\n";

const MANIFEST: &str = r#"
description = "A test grade."

[options]
strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength" }
look = { kind = "choice", choices = ["Soft", "Hard"], default = "Soft", label = "Look" }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color"]
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
fn a_script_lists_shader_packs_wears_one_sets_its_options_and_the_game_starts_in_them() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = voxear_core::logging::LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Shaders.cs"), SCRIPT).unwrap();
    // A pack of the project's, and one of the player's, in the ShaderPacks folder beside the saves.
    for (path, text) in [("Packs/Warm.shaderpack/pack.toml", MANIFEST), ("Packs/Warm.shaderpack/grade.slang", GRADE)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let data = tempfile::tempdir().unwrap();
    let saves = data.path().join("Saves");
    let mine = data.path().join(shader_packs::SHADER_PACKS_FOLDER).join("Mine.shaderpack");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("pack.toml"), MANIFEST.replace("A test grade.", "The player's own.")).unwrap();
    std::fs::write(mine.join("grade.slang"), GRADE).unwrap();

    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let lines = |log: &voxear_core::logging::LogCapture| -> Vec<String> {
        log.lock().unwrap().lines.iter().map(|l| format!("{} {}", l.target, l.message)).collect()
    };
    let said = |log: &voxear_core::logging::LogCapture, words: &str| lines(log).iter().any(|line| line.contains(words));
    let tick_until = |game: &mut Game, done: &dyn Fn(&Game) -> bool| {
        let started = std::time::Instant::now();
        while !done(game) && started.elapsed() < std::time::Duration::from_secs(300) {
            game.tick(1.0 / 60.0, None);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        done(game)
    };

    // The engine's own first, then the project's, then the player's; none worn to begin with.
    (0..2).for_each(|_| game.tick(1.0 / 60.0, None));
    assert!(said(&log, "shaders has Evening:BuiltIn, Warm:Game, Mine:Player"), "{:#?}", lines(&log));
    assert!(said(&log, "shaders says A test grade."), "{:#?}", lines(&log));
    assert!(said(&log, "shaders wears no shader pack None folder True"), "{:#?}", lines(&log));

    // The project's, compiled on a thread; its options set once it is drawn, and given to the frame.
    let set = |game: &Game| {
        game.engine.world.resource::<WornShaderPack>().is_some_and(|worn| (worn.options[0] - 1.5).abs() < 1e-4)
    };
    assert!(tick_until(&mut game, &set), "{:#?}", lines(&log));
    assert!(said(&log, "shaders wears Warm from Game with Strength: 1, Look: Soft"), "{:#?}", lines(&log));
    assert!(tick_until(&mut game, &|_| said(&log, "shaders set Strength: 1.5, Look: Hard")), "{:#?}", lines(&log));
    assert!(said(&log, "has no option `glow`"), "an option it has not is reported: {:#?}", lines(&log));
    let worn = game.engine.world.resource::<WornShaderPack>().unwrap();
    assert_eq!((worn.options[0], worn.options[1]), (1.5, 1.0));

    // Kept beside the saves: the pack and its options, and its programs compiled.
    let (reference, options) = shader_packs::remembered(&saves).expect("the choice is kept");
    assert_eq!(Some(reference.as_str()), shader_packs::worn(&game.engine.world));
    assert!(options.contains(&("strength".to_string(), 1.5)) && options.contains(&("look".to_string(), 1.0)));
    let compiled = shader_packs::compiled_folder(&saves).unwrap();
    let kept = std::fs::read_dir(&compiled).map(|files| files.count()).unwrap_or(0);
    assert!(kept >= 1, "its programs kept in {}", compiled.display());

    // The game started again wears it with its options, from what was compiled before.
    shader_packs::take_off(&mut game.engine.world);
    assert!(shader_packs::worn(&game.engine.world).is_none());
    game.wear_remembered_shader_pack();
    assert!(tick_until(&mut game, &|game| game.engine.world.resource::<WornShaderPack>().is_some()));
    let worn = game.engine.world.resource::<WornShaderPack>().unwrap();
    assert_eq!((worn.options[0], worn.options[1]), (1.5, 1.0), "the options set before");
    // One that is gone by then is forgotten.
    std::fs::remove_dir_all(dir.path().join("Packs")).unwrap();
    shader_packs::take_off(&mut game.engine.world);
    game.wear_remembered_shader_pack();
    assert!(shader_packs::worn(&game.engine.world).is_none() && shader_packs::remembered(&saves).is_none());
}
