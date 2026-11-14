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
    let voices = "[[entry]]\nkey = \"voice.hello\"\nen = \"Voices/Hello.wav\"\nfr = \"Voices/Bonjour.wav\"\n";
    let files = [
        ("voxear.toml", PROJECT),
        ("Text/Menu.strings", MENU),
        ("Text/Hud.strings", HUD),
        ("Text/Voices.assettable", voices),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let localization = game.engine.world.resource_mut::<Localization>().unwrap();
    assert_eq!(localization.languages(), ["en", "fr"]);
    assert!(localization.set_language("fr-BE"));
    assert_eq!(localization.asset("voice.hello"), Some("Voices/Bonjour.wav"), "asset tables are read too");
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

#[test]
fn a_game_in_a_language_written_right_to_left_lays_its_ui_out_that_way() {
    let dir = tempfile::tempdir().unwrap();
    let project = "[project]\nname = \"Spoken\"\n\n[localization]\nlanguages = [\"en\", \"ar\"]\n";
    let menu = "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\nar = \"العب\"\n";
    for (path, text) in [("voxear.toml", project), ("Text/Menu.strings", menu)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.tick(1.0 / 60.0, None);
    assert!(!game.ui.ui().is_right_to_left());
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("ar");
    game.tick(1.0 / 60.0, None);
    assert!(game.ui.ui().is_right_to_left(), "Arabic reads right to left");
}

#[test]
fn a_languages_fonts_are_read_and_its_text_falls_back_to_them() {
    let dir = tempfile::tempdir().unwrap();
    let project = "[project]\nname = \"Spoken\"\n\n[localization]\nlanguages = [\"en\", \"ja\"]\n\n\
                   [localization.fonts]\nja = [\"Fonts/Wide Japanese.ttf\"]\n";
    let menu = "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\nja = \"日\"\n";
    for (path, text) in [("voxear.toml", project), ("Text/Menu.strings", menu)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(dir.path().join("Fonts")).unwrap();
    std::fs::write(dir.path().join("Fonts/Wide Japanese.ttf"), include_bytes!("../../voxear_ui/tests/fonts/Wide.ttf")).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    let face = voxear_ui::fonts::face("Fonts/Wide Japanese.ttf");
    assert!(face.is_loaded(), "the project's fonts are read as the game starts");
    let guid = game.content.db().guid_of("Fonts/Wide Japanese.ttf").unwrap();
    assert!(voxear_ui::fonts::face(&guid.to_string()).is_loaded(), "named by its GUID too");
    assert_eq!(game.content.db().get(guid).unwrap().kind, AssetKind::Font);

    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.tick(1.0 / 60.0, None);
    assert!(game.ui.ui().language_fonts().is_empty(), "English names none");
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("ja");
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.ui().language_fonts(), &[face], "Japanese text falls back to its font");
}

#[test]
fn a_localized_text_shows_its_keys_text_in_its_documents_element_and_follows_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let sign = "<vui>\n  <Label name=\"heading\" text=\"Heading\" />\n  <Label name=\"title\" />\n</vui>\n";
    let table = "[[entry]]\nkey = \"sign.tavern\"\nen = \"{owner}'s inn\"\nfr = \"L'auberge de {owner}\"\n";
    for (path, text) in [("voxear.toml", PROJECT), ("Text/Signs.strings", table), ("UI/Sign.vui", sign)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.engine.world.resource_mut::<Localization>().unwrap().set_language("en");
    game.engine.set_edit_mode(false);
    let document = game.content.db().guid_of("UI/Sign.vui").unwrap();
    let obj = game.engine.world.spawn_empty();
    game.engine.world.insert(obj, voxear_engine::screen_ui::ScreenUi { document: Some(document), order: 0 });
    let mut text = voxear_engine::localized_text::LocalizedText { key: "sign.tavern".into(), element: "title".into(), ..Default::default() };
    text.values.insert("owner".into(), "Mara".into());
    game.engine.world.insert(obj, text);
    game.tick(1.0 / 60.0, None);
    game.tick(1.0 / 60.0, None);
    let shown = game.engine.world.get::<voxear_engine::screen_ui::ScreenDocument>(obj).expect("its document shows").0;
    let (title, heading) = (game.ui.find(shown, "title").unwrap(), game.ui.find(shown, "heading").unwrap());
    assert_eq!(game.ui.text(title).unwrap(), "Mara's inn");

    game.engine.world.resource_mut::<Localization>().unwrap().set_language("fr");
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(title).unwrap(), "L'auberge de Mara", "shown again in French");

    // With no element named, the document's first label.
    game.engine.world.get_mut::<voxear_engine::localized_text::LocalizedText>(obj).unwrap().element.clear();
    game.tick(1.0 / 60.0, None);
    assert_eq!(game.ui.text(heading).unwrap(), "L'auberge de Mara");

    // And it saves with its scene, read back the same.
    let data: voxear_engine::localized_text::LocalizedText =
        toml::from_str("key = \"sign.tavern\"\nelement = \"title\"\nvalues = { owner = \"Mara\" }\n").unwrap();
    assert_eq!(data.values["owner"], "Mara");
}
