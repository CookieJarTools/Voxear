//! Text from a project's string tables: every `.strings` file read when the
//! game starts, in the languages its `voxear.toml` lists, and read again
//! when one changes, the game staying in the language it is in; and UI text
//! bound to a key with `@menu.play`, following the language.

use voxear_assets::AssetKind;
use voxear_engine::{Engine, Game, Project, localization::Localization};

const PROJECT: &str = "[project]\nname = \"Spoken\"\n\n[localization]\nlanguages = [\"en\", \"fr\"]\n";
const MENU: &str = "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\nfr = \"Jouer\"\n";
const HUD: &str = "[[entry]]\nkey = \"hud.coins\"\nen = \"Coins\"\n";

fn text(game: &Game, key: &str) -> String {
    game.engine.world.resource::<Localization>().unwrap().text(key).into_owned()
}

#[test]
fn a_game_reads_every_string_table_in_its_languages() {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [("voxear.toml", PROJECT), ("Text/Menu.strings", MENU), ("Text/Hud.strings", HUD)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let localization = game.engine.world.resource_mut::<Localization>().unwrap();
    assert_eq!(localization.languages(), ["en", "fr"]);
    assert!(localization.set_language("fr-BE"));
    assert_eq!(text(&game, "menu.play"), "Jouer");
    assert_eq!(text(&game, "hud.coins"), "Coins", "French falls back to English, the project's own");

    // A table changed is read again, and the game stays in French.
    std::fs::write(dir.path().join("Text/Menu.strings"), MENU.replace("Jouer", "Lancer")).unwrap();
    game.content.forget_kind(AssetKind::Strings);
    game.load_strings();
    assert_eq!(
        (text(&game, "menu.play").as_str(), game.engine.world.resource::<Localization>().unwrap().language()),
        ("Lancer", "fr")
    );

    // One that cannot be read is left out, and the rest still are.
    std::fs::write(dir.path().join("Text/Hud.strings"), "[[entry]]\nen = \"Coins\"\n").unwrap();
    game.content.forget_kind(AssetKind::Strings);
    game.load_strings();
    assert_eq!((text(&game, "menu.play").as_str(), text(&game, "hud.coins").as_str()), ("Lancer", "⟦hud.coins⟧"));
}

#[test]
fn ui_text_bound_to_a_key_follows_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let menu = "<vui>\n  <Label name=\"title\" text=\"@menu.play\" />\n  <Label name=\"mail\">@@home</Label>\n</vui>\n";
    for (path, text) in [("voxear.toml", PROJECT), ("Text/Menu.strings", MENU), ("UI/Menu.vui", menu)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    let document = game.ui.load(&mut game.content, "UI/Menu.vui").unwrap();
    let (title, mail) = (game.ui.find(document, "title").unwrap(), game.ui.find(document, "mail").unwrap());
    game.tick(1.0 / 60.0, None);
    assert_eq!((game.ui.text(title).unwrap().as_str(), game.ui.text(mail).unwrap().as_str()), ("Play", "@home"));

    game.engine.world.resource_mut::<Localization>().unwrap().set_language("fr");
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(title).unwrap(), "Jouer", "shown again in French");

    // A script's text: a key binds it, plain text lets it go.
    game.ui.set_text(mail, "@menu.play");
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(mail).unwrap(), "Jouer");
    game.ui.set_text(title, "Start");
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.tick(1.0 / 60.0, None);
    assert_eq!((game.ui.text(title).unwrap().as_str(), game.ui.text(mail).unwrap().as_str()), ("Start", "Play"));
}
