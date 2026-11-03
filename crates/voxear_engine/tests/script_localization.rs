//! A script's text from the string tables: a `Localized` text that follows
//! the language and its values, and dates and money written as the
//! language writes them. Builds a copy of Hello's scripts with one more;
//! skips itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, localization::Localization, scripts};

const SCRIPT: &str = r#"using System;
using Voxear;

namespace Hello;

public sealed class Texts : SystemBase
{
	private Localized coins = null!;

	protected override void OnCreate()
	{
		coins = new Localized("hud.coins", ("count", 1));
		coins.Changed += text => Log.Info($"coins now {text}");
		Log.Info($"coins {coins.Text}");
		coins.Set("count", 3);
		Log.Info(Localization.Format("save.when", ("when", new DateTime(2026, 11, 3, 14, 30, 0))));
		Log.Info(Localization.Format("shop.price", ("price", new Money(4.99m, Currency.GBP))));
	}

	protected override void Update()
	{
	}
}
"#;

const HUD: &str = "[[entry]]\nkey = \"hud.coins\"\nen = \"{count, plural, one {# coin} other {# coins}}\"\n\
                   fr = \"{count, plural, one {# pièce} other {# pièces}}\"\n\n[[entry]]\nkey = \"save.when\"\n\
                   en = \"Saved {when, date}\"\n\n[[entry]]\nkey = \"shop.price\"\nen = \"{price, currency}\"\n";

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against, without input actions of its own.
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
fn a_scripts_text_follows_the_language_and_writes_dates_and_money() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let toml = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, format!("{project}\n\n[localization]\nlanguages = [\"en\", \"fr\"]\n")).unwrap();
    std::fs::create_dir_all(dir.path().join("Text")).unwrap();
    std::fs::write(dir.path().join("Text/Hud.strings"), HUD).unwrap();
    std::fs::write(dir.path().join("Scripts/Texts.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let lines = || log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect::<Vec<_>>();
    let said = |text: &str| lines().iter().any(|line| line == text);
    assert!(said("coins 1 coin") && said("coins now 3 coins"), "{:?}", lines());
    assert!(said("Saved Nov 3, 2026"), "a DateTime, as English writes it: {:?}", lines());
    assert!(said("£4.99"), "Money, with its currency's symbol: {:?}", lines());

    game.engine.world.resource_mut::<Localization>().unwrap().set_language("fr");
    game.tick(1.0 / 60.0, None);
    assert!(said("coins now 3 pièces"), "shown again in French by itself: {:?}", lines());
}
