//! String tables: `.strings` files, each holding some of the game's text,
//! a key's text in every language the project has, as Unity's string table
//! collections do. A table is TOML, an entry a key at a time:
//!
//! ```toml
//! format = 1
//!
//! [[entry]]
//! key = "menu.play"
//! comment = "The main menu's first button"
//! limit = 12
//! en = "Play"
//! fr = "Jouer"
//! pt-BR = "Jogar"
//! ```
//!
//! Besides `key`, an entry may have a `comment` for translators and a
//! `limit`, the most characters its text has room for. Every other value
//! is its text in a language, named by its code.
//!
//! A table may say where its translators' sheet is online, `sheet = "…"`
//! before its entries, as a Google Sheet's link, to import it again from.

use anyhow::{Context, Result, bail, ensure};

pub const STRINGS_FORMAT: i64 = 1;

/// A string table, its entries in the file's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringTable {
    /// Where its translators' sheet is online; empty for nowhere.
    pub sheet: String,
    pub entries: Vec<StringEntry>,
}

/// A key's text in each language.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringEntry {
    pub key: String,
    /// What a translator should know about it, such as where it shows.
    pub comment: String,
    /// The most characters its text has room for; 0 for no limit.
    pub limit: u32,
    /// Its text in each language, by the language's code.
    pub texts: Vec<(String, String)>,
}

impl StringEntry {
    /// Its text in a language, when it has one.
    pub fn text(&self, language: &str) -> Option<&str> {
        self.texts.iter().find(|(code, _)| code == language).map(|(_, text)| text.as_str())
    }
}

impl StringTable {
    pub fn parse(text: &str) -> Result<Self> {
        let file: toml::Table = toml::from_str(text)?;
        let format = file.get("format").and_then(toml::Value::as_integer).unwrap_or(STRINGS_FORMAT);
        ensure!(format == STRINGS_FORMAT, "format {format} is newer than this Voxear reads ({STRINGS_FORMAT})");
        if let Some(other) = file.keys().find(|key| !matches!(key.as_str(), "format" | "sheet" | "entry")) {
            bail!("`{other}` is not part of a string table; each text goes in an [[entry]]");
        }
        let entries = match file.get("entry") {
            None => Vec::new(),
            Some(toml::Value::Array(entries)) => entries.iter().enumerate().map(entry).collect::<Result<_>>()?,
            Some(_) => bail!("`entry` must be a list of [[entry]] tables"),
        };
        let sheet = file.get("sheet").and_then(toml::Value::as_str).unwrap_or_default().to_string();
        Ok(Self { sheet, entries })
    }

    /// The table as its file has it, which [`parse`](Self::parse) reads back.
    pub fn write(&self) -> String {
        let mut out = format!("format = {STRINGS_FORMAT}\n");
        if !self.sheet.is_empty() {
            out.push_str(&format!("sheet = {}\n", quoted(&self.sheet)));
        }
        for entry in &self.entries {
            out.push_str("\n[[entry]]\n");
            out.push_str(&format!("key = {}\n", quoted(&entry.key)));
            if !entry.comment.is_empty() {
                out.push_str(&format!("comment = {}\n", quoted(&entry.comment)));
            }
            if entry.limit > 0 {
                out.push_str(&format!("limit = {}\n", entry.limit));
            }
            for (language, text) in &entry.texts {
                out.push_str(&format!("{} = {}\n", bare_or_quoted(language), quoted(text)));
            }
        }
        out
    }
}

fn entry((at, value): (usize, &toml::Value)) -> Result<StringEntry> {
    let table = value.as_table().with_context(|| format!("entry {} is not a table", at + 1))?;
    let key = table.get("key").and_then(toml::Value::as_str);
    let key = key.with_context(|| format!("entry {} has no key", at + 1))?.trim().to_string();
    ensure!(!key.is_empty(), "entry {} has an empty key", at + 1);
    let mut entry = StringEntry { key, ..StringEntry::default() };
    for (name, value) in table {
        match (name.as_str(), value) {
            ("key", _) => {}
            ("comment", toml::Value::String(comment)) => entry.comment = comment.clone(),
            ("limit", toml::Value::Integer(limit)) => entry.limit = (*limit).clamp(0, u32::MAX.into()) as u32,
            (language, toml::Value::String(text)) => entry.texts.push((language.to_string(), text.clone())),
            (name, _) => bail!("{}: `{name}` should be text", entry.key),
        }
    }
    Ok(entry)
}

fn quoted(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// A key bare when TOML allows it, as `pt-BR`, or quoted.
fn bare_or_quoted(key: &str) -> String {
    let bare = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if bare { key.to_string() } else { quoted(key) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_reads_each_keys_text_in_every_language_and_writes_it_back() {
        let text = "format = 1\n\n[[entry]]\nkey = \"menu.play\"\ncomment = \"The first button\"\nlimit = 12\n\
                    en = \"Play\"\npt-BR = \"Jogar\"\n\n[[entry]]\nkey = \"menu.quit\"\nen = 'Say \"bye\"'\n";
        let table = StringTable::parse(text).unwrap();
        let play = &table.entries[0];
        assert_eq!((play.key.as_str(), play.comment.as_str(), play.limit), ("menu.play", "The first button", 12));
        assert_eq!((play.text("pt-BR"), play.text("fr")), (Some("Jogar"), None));
        assert_eq!(table.entries[1].text("en"), Some("Say \"bye\""));
        assert_eq!(table.write(), text, "written back as it was read");

        let wrong = StringTable::parse("[[entry]]\nkey = \"a\"\nen = 3\n").unwrap_err();
        assert!(format!("{wrong:#}").contains("`en` should be text"), "{wrong:#}");
        assert!(StringTable::parse("[[entry]]\nen = \"Play\"\n").is_err(), "an entry needs its key");
        assert!(StringTable::parse("format = 2\n").is_err());
        let online = "format = 1\nsheet = \"https://example.com/menu.csv\"\n";
        assert_eq!(StringTable::parse(online).unwrap().write(), online, "where its sheet is, kept");
    }
}
