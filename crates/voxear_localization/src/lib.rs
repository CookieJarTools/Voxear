//! The game's text in each of its languages, as Unity's Localization
//! package keeps it. String tables (`.strings` files, see [`table`]) hold
//! each key's text in every language the project has, and
//! [`Localization`] finds a key's text in the language the game is in.
//!
//! A language without a key's text falls back as ICU says, `pt-BR` to
//! `pt`, and at last to the project's own language, the first it lists.
//! A key no language has shows as `⟦menu.play⟧` while the game is made,
//! with a warning, and as the key itself in a shipped game.
//!
//! A text may name values to put in it, as `{player} found {count} coins`,
//! and pick its words by them, as ICU MessageFormat does:
//! `{count, plural, one {# coin} other {# coins}}` follows each language's
//! plural rules. [`Localization::format`] fills them in (see [`message`]).
//!
//! Pseudo-localisation ([`Localization::set_pseudo`]) shows every text
//! accented, a third longer and in brackets, as `[Ƥļåý !!!]`, so text that
//! is not in the tables, and text that will not fit once translated, show
//! before translation starts.

pub mod csv;
pub mod message;
pub mod table;

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use icu_locale_core::Locale;
use icu_locale_fallback::LocaleFallbacker;

pub use table::{STRINGS_FORMAT, StringEntry, StringTable};

/// The project's text, in the language the game is in. Kept in the world
/// as a resource.
#[derive(Debug)]
pub struct Localization {
    /// The project's languages, its own first.
    languages: Vec<String>,
    language: String,
    /// Where a key's text is looked for, nearest first.
    chain: Vec<String>,
    /// Each key's text, by language.
    texts: HashMap<String, HashMap<String, String>>,
    /// Each asset table key's asset, as a reference, by language.
    assets: HashMap<String, HashMap<String, String>>,
    mark_missing: bool,
    pseudo: bool,
    /// Counts each change of language or text, for text shown from it to
    /// tell when to look again.
    version: u64,
    /// Keys warned of as missing, each once.
    warned: Mutex<HashSet<String>>,
}

impl Clone for Localization {
    fn clone(&self) -> Self {
        Self {
            languages: self.languages.clone(),
            language: self.language.clone(),
            chain: self.chain.clone(),
            texts: self.texts.clone(),
            assets: self.assets.clone(),
            mark_missing: self.mark_missing,
            pseudo: self.pseudo,
            version: self.version,
            warned: Mutex::default(),
        }
    }
}

impl Default for Localization {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl Localization {
    /// No text yet, in the project's own language: the first of
    /// `languages`, or English when it names none.
    pub fn new(languages: &[String]) -> Self {
        let mut languages: Vec<String> =
            languages.iter().map(|code| normalize(code)).filter(|c| !c.is_empty()).collect();
        if languages.is_empty() {
            languages.push("en".to_string());
        }
        let own = languages[0].clone();
        Self {
            chain: chain(&own, &own),
            language: own,
            languages,
            texts: HashMap::new(),
            assets: HashMap::new(),
            mark_missing: true,
            pseudo: false,
            version: 0,
            warned: Mutex::default(),
        }
    }

    /// Adds a table's text, over any the same keys had.
    pub fn add_table(&mut self, table: &StringTable) {
        for entry in &table.entries {
            let texts = self.texts.entry(entry.key.clone()).or_default();
            for (language, text) in &entry.texts {
                texts.insert(normalize(language), text.clone());
            }
        }
        self.version += 1;
    }

    /// Adds an asset table's assets, over any the same keys had. An asset
    /// table is written as a string table is (see [`StringTable`]), each
    /// language's value a reference to its asset, by GUID or project path.
    pub fn add_asset_table(&mut self, table: &StringTable) {
        for entry in &table.entries {
            let assets = self.assets.entry(entry.key.clone()).or_default();
            for (language, reference) in &entry.texts {
                assets.insert(normalize(language), reference.clone());
            }
        }
        self.version += 1;
    }

    /// The reference to a key's asset in the game's language, or the
    /// nearest it falls back to, as a voice line in English when it was
    /// not recorded in Portuguese.
    pub fn asset(&self, key: &str) -> Option<&str> {
        let assets = self.assets.get(key)?;
        self.chain.iter().find_map(|language| assets.get(language)).map(String::as_str)
    }

    /// Forgets every table's text and assets, as before reading them again.
    pub fn clear(&mut self) {
        self.texts.clear();
        self.assets.clear();
        self.version += 1;
    }

    /// Takes the project's languages again, as after its settings changed,
    /// staying in the language the game is in when it still has it, or the
    /// nearest it has.
    pub fn set_languages(&mut self, languages: &[String]) {
        let language = std::mem::take(&mut self.language);
        let fresh = Self::new(languages);
        (self.languages, self.chain, self.language) = (fresh.languages, fresh.chain, fresh.language);
        self.set_language(&language);
        self.version += 1;
    }

    /// The project's languages, its own first.
    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    /// The language the game is in.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Puts the game in the project's language nearest `wanted`: itself,
    /// or the one it falls back to, as `pt` for `pt-BR`. Returns whether
    /// the project has one.
    pub fn set_language(&mut self, wanted: &str) -> bool {
        let Some(found) = self.nearest(wanted) else { return false };
        if found != self.language {
            self.chain = chain(&found, &self.languages[0]);
            self.language = found;
            self.version += 1;
        }
        true
    }

    /// The project's language nearest `wanted`, if any is.
    pub fn nearest(&self, wanted: &str) -> Option<String> {
        let near = near(&normalize(wanted));
        near.iter().find_map(|code| self.languages.iter().find(|language| language.eq_ignore_ascii_case(code)).cloned())
    }

    /// Whether a key that no language has shows as `⟦key⟧`, as while the
    /// game is made, rather than as the key, as in a shipped game.
    pub fn marks_missing(&self) -> bool {
        self.mark_missing
    }

    pub fn set_mark_missing(&mut self, mark: bool) {
        self.mark_missing = mark;
    }

    /// Whether every text shows pseudo-localised, as `[Ƥļåý !!!]`.
    pub fn pseudo(&self) -> bool {
        self.pseudo
    }

    pub fn set_pseudo(&mut self, pseudo: bool) {
        if pseudo != self.pseudo {
            self.pseudo = pseudo;
            self.version += 1;
        }
    }

    /// Counts each change of language or text.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// A key's text in the game's language, or the nearest it falls back to.
    pub fn find(&self, key: &str) -> Option<&str> {
        let texts = self.texts.get(key)?;
        self.chain.iter().find_map(|language| texts.get(language)).map(String::as_str)
    }

    /// Whether any table has the key.
    pub fn has(&self, key: &str) -> bool {
        self.texts.contains_key(key)
    }

    /// Every key, in no order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.texts.keys().map(String::as_str)
    }

    /// A key's text, as [`find`](Self::find) finds it, or the key marked
    /// as missing, warned of once.
    pub fn text(&self, key: &str) -> Cow<'_, str> {
        if let Some(text) = self.find(key) {
            return if self.pseudo { Cow::Owned(pseudo(text)) } else { Cow::Borrowed(text) };
        }
        let first = self.warned.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(key.to_string());
        if first {
            tracing::warn!(target: "localization", "no text for `{key}` in {}", self.language);
        }
        match self.mark_missing {
            true => Cow::Owned(format!("⟦{key}⟧")),
            false => Cow::Owned(key.to_string()),
        }
    }

    /// A key's text with the values it names filled in, in the game's
    /// language's ways: `{player}` takes the value named `player`, and
    /// plurals, selects and numbers are as [`message`] says. A name with no
    /// value is left as it is.
    pub fn format(&self, key: &str, values: &[(&str, &str)]) -> String {
        message::format(&self.text(key), values, &self.language)
    }
}

/// Text pseudo-localised: each letter accented, a third longer with `!`s,
/// and in brackets, the `{names}` it holds and how they are used left as
/// they are, so values still fill them and plurals still pick.
pub fn pseudo(text: &str) -> String {
    const PLAIN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const ACCENTED: &str = "åƀçđéƒĝĥîĵķļɱñöƥǫŕšŧûṽŵẋýžÅƁÇĐÉƑĜĤÎĴĶĻṀÑÖƤǪŔŠŦÛṼŴẊÝŽ";
    let mut out = String::from("[");
    // Inside one brace is a name and how it is used; inside two, a plural's or a select's words again.
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        let words = depth.is_multiple_of(2) && c != '{' && c != '}';
        let accented = words.then(|| PLAIN.find(c)).flatten().and_then(|at| ACCENTED.chars().nth(at));
        out.push(accented.unwrap_or(c));
    }
    let longer = (text.chars().count() as f32 * 0.3).round().max(1.0) as usize;
    out.push(' ');
    out.push_str(&"!".repeat(longer));
    out.push(']');
    out
}

/// `text` with each `{name}` it holds replaced by the value of that name.
pub fn fill(text: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let named = after.find('}').map(|close| (&after[..close], &after[close + 1..]));
        match named.and_then(|(name, tail)| Some((values.iter().find(|(n, _)| *n == name.trim())?.1, tail))) {
            Some((value, tail)) => {
                out.push_str(value);
                rest = tail;
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A language code in its usual form: `pt_BR.UTF-8`, as Linux names one,
/// becomes `pt-BR`. Empty for none.
pub fn normalize(code: &str) -> String {
    let code = code.split(['.', '@']).next().unwrap_or_default().trim().replace('_', "-");
    if code.is_empty() || code == "C" || code == "POSIX" {
        return String::new();
    }
    code.parse::<Locale>().map_or(code, |locale| locale.to_string())
}

/// The languages to look in for `language`'s text, nearest first: itself,
/// those ICU falls back to, and at last `own`.
pub fn chain(language: &str, own: &str) -> Vec<String> {
    let mut chain = near(language);
    if !chain.iter().any(|code| code == own) {
        chain.push(own.to_string());
    }
    chain
}

/// A language and those ICU falls back to from it, nearest first: `pt-BR`,
/// then `pt`.
fn near(language: &str) -> Vec<String> {
    let mut near = Vec::new();
    if let Ok(locale) = language.parse::<Locale>() {
        let fallbacker = LocaleFallbacker::new();
        let mut step = fallbacker.for_config(Default::default()).fallback_for((&locale).into());
        while !step.get().is_unknown() {
            near.push(step.get().to_string());
            step.step();
        }
    }
    if !near.iter().any(|code| code == language) {
        near.insert(0, language.to_string());
    }
    near
}

/// The language the player's system is in: on Windows the first of the
/// languages the player prefers, as its settings list them; elsewhere
/// from `LC_ALL`, `LC_MESSAGES` or `LANG`, as Linux keeps it.
pub fn system_language() -> Option<String> {
    #[cfg(windows)]
    if let Some(preferred) = windows_language() {
        return Some(preferred);
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| normalize(&value))
        .find(|code| !code.is_empty())
}

/// The first of the languages Windows' settings list the player as
/// preferring for its own text, as `fr-FR`.
#[cfg(windows)]
fn windows_language() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let (mut count, mut length) = (0u32, 0u32);
    // SAFETY: asked first for the length, then given a buffer that long.
    let sized =
        unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut length) };
    if sized == 0 || length == 0 {
        return None;
    }
    let mut names = vec![0u16; length as usize];
    let read = unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, names.as_mut_ptr(), &mut length) };
    if read == 0 {
        return None;
    }
    // A list of names, each ended by a nought, the list by another.
    let first = names.split(|&unit| unit == 0).next()?;
    Some(normalize(&String::from_utf16_lossy(first))).filter(|code| !code.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> StringTable {
        StringTable::parse(
            "[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\npt = \"Jogar\"\n\n\
             [[entry]]\nkey = \"menu.quit\"\nen = \"Quit\"\npt-BR = \"Sair\"\n\n\
             [[entry]]\nkey = \"loot\"\nen = \"{player} found {count} coins\"\n",
        )
        .unwrap()
    }

    #[test]
    fn a_keys_asset_falls_back_as_its_text_does() {
        let mut texts = Localization::new(&["en".into(), "pt".into(), "pt-BR".into()]);
        let voices = "[[entry]]\nkey = \"voice.intro\"\nen = \"Sounds/Intro.wav\"\npt = \"Sounds/Intro_pt.wav\"\n";
        texts.add_asset_table(&StringTable::parse(voices).unwrap());
        assert_eq!(texts.asset("voice.intro"), Some("Sounds/Intro.wav"));
        texts.set_language("pt-BR");
        assert_eq!(texts.asset("voice.intro"), Some("Sounds/Intro_pt.wav"), "Brazil hears Portugal's recording");
        assert_eq!(texts.asset("menu.play"), None);
        assert!(!texts.has("voice.intro"), "an asset is not text");
    }

    #[test]
    fn a_key_is_found_in_the_game_language_or_the_ones_it_falls_back_to() {
        let mut texts = Localization::new(&["en".into(), "pt".into(), "pt-BR".into()]);
        texts.add_table(&menu());
        assert_eq!((texts.language(), texts.text("menu.play").as_ref()), ("en", "Play"));
        assert!(texts.set_language("pt_BR.UTF-8"));
        assert_eq!(texts.language(), "pt-BR");
        assert_eq!(texts.text("menu.quit"), "Sair");
        assert_eq!(texts.text("menu.play"), "Jogar", "pt-BR falls back to pt");
        assert_eq!(texts.text("loot"), "{player} found {count} coins", "and at last to the project's own");
        assert_eq!(texts.text("menu.nowhere"), "⟦menu.nowhere⟧");
        texts.set_mark_missing(false);
        assert_eq!(texts.text("menu.nowhere"), "menu.nowhere");

        // A language the project lacks falls back to the nearest it has, or none.
        assert!(texts.set_language("pt-PT") && texts.language() == "pt");
        let before = texts.version();
        assert!(!texts.set_language("ja") && texts.language() == "pt" && texts.version() == before);
        assert!(texts.set_language("en-GB") && texts.language() == "en");
    }

    #[test]
    fn values_fill_the_names_a_text_holds() {
        let mut texts = Localization::new(&[]);
        texts.add_table(&menu());
        assert_eq!(texts.format("loot", &[("player", "Ana"), ("count", "3")]), "Ana found 3 coins");
        assert_eq!(fill("{a} and {b} and {", &[("a", "x")]), "x and {b} and {");

        // Pseudo-localised, the names are kept for the values to fill.
        texts.set_pseudo(true);
        assert_eq!(texts.format("loot", &[("player", "Ana"), ("count", "3")]), "[Ana ƒöûñđ 3 çöîñš !!!!!!!!]");
        assert_eq!(pseudo("Play the game"), "[Ƥļåý ŧĥé ĝåɱé !!!!]");
        assert_eq!(
            (normalize("C.UTF-8"), normalize("fr_CA@euro"), normalize("zh-hant-tw")),
            ("".into(), "fr-CA".into(), "zh-Hant-TW".into())
        );
    }
}
