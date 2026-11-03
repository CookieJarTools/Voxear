//! What the Localization Tables window shows and does, apart from how: every
//! key of every string table in the project, with its text in each of the
//! project's languages; where a key is used; and a key renamed in its table
//! and everywhere it is used.
//!
//! A key is used where a UI document binds text to it (`@menu.play`), and
//! where a scene, prefab, level or script names it in quotes, as a
//! `LocalizedString` field saves it and as `Localization.Get("menu.play")`
//! reads it.

use std::path::Path;

use anyhow::{Context, Result, bail};
use voxear_engine::localization::StringTable;

/// The files a key may be used in, by their endings.
const USED_IN: [&str; 5] = [".vui", ".scene", ".prefab", ".level", ".cs"];

/// One key of one table.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRow {
    /// The table's project path.
    pub table: String,
    pub key: String,
    /// Its text in each of the project's languages, in their order; empty
    /// where it has none.
    pub texts: Vec<String>,
}

impl KeyRow {
    /// How many of the languages it has no text in.
    pub fn missing(&self) -> usize {
        self.texts.iter().filter(|text| text.is_empty()).count()
    }

    /// Whether a search finds it: in its key, its table or any of its text,
    /// whatever the case.
    pub fn matches(&self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        let found = |text: &str| text.to_lowercase().contains(&search);
        search.is_empty() || found(&self.key) || found(&self.table) || self.texts.iter().any(|text| found(text))
    }
}

/// A place a key is used: the file, the line from 1, and the line's text.
#[derive(Clone, Debug, PartialEq)]
pub struct Usage {
    pub path: String,
    pub line: usize,
    pub text: String,
}

/// Every key of the tables at `tables` (project paths), in path order and
/// each table's own, with its text in `languages`.
pub fn gather(root: &Path, tables: &[String], languages: &[String]) -> Vec<KeyRow> {
    let mut sorted = tables.to_vec();
    sorted.sort();
    let mut rows = Vec::new();
    for path in sorted {
        let Some(table) = read(root, &path).ok() else { continue };
        for entry in table.entries {
            let texts = languages.iter().map(|language| entry.text(language).unwrap_or_default().to_string()).collect();
            rows.push(KeyRow { table: path.clone(), key: entry.key, texts });
        }
    }
    rows
}

/// Where `key` is used in the project's documents, scenes, prefabs, levels
/// and scripts, in path order.
pub fn usages(root: &Path, key: &str) -> Vec<Usage> {
    let mut found = Vec::new();
    for path in used_in_files(root) {
        let Ok(text) = std::fs::read_to_string(root.join(&path)) else { continue };
        for (at, line) in text.lines().enumerate() {
            if uses(&path, line, key) {
                found.push(Usage { path: path.clone(), line: at + 1, text: line.trim().to_string() });
            }
        }
    }
    found
}

/// Renames a key in its table and everywhere it is used, and gives the
/// files changed, the table first. Nothing changes when another key of the
/// table has the new name, or it is no name at all.
pub fn rename(root: &Path, table: &str, old: &str, new: &str) -> Result<Vec<String>> {
    let new = new.trim();
    if new.is_empty() || new.contains(char::is_whitespace) || new.contains('"') {
        bail!("`{new}` cannot be a key: keys have no spaces or quotes");
    }
    let mut strings = read(root, table)?;
    if strings.entries.iter().any(|entry| entry.key == new) {
        bail!("{table} has a key `{new}` already");
    }
    let entry =
        strings.entries.iter_mut().find(|entry| entry.key == old).with_context(|| format!("no `{old}` in {table}"))?;
    entry.key = new.to_string();
    std::fs::write(root.join(table), strings.write()).with_context(|| format!("writing {table}"))?;
    let mut changed = vec![table.to_string()];
    for path in used_in_files(root) {
        let file = root.join(&path);
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let renamed = rename_in(&path, &text, old, new);
        if renamed != text {
            std::fs::write(&file, renamed).with_context(|| format!("writing {path}"))?;
            changed.push(path);
        }
    }
    Ok(changed)
}

fn read(root: &Path, path: &str) -> Result<StringTable> {
    let text = std::fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))?;
    StringTable::parse(&text).with_context(|| format!("reading {path}"))
}

/// The project's files a key may be used in, by project path, sorted; its
/// own build and settings folders left out.
fn used_in_files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" || name == "Builds" {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else if USED_IN.iter().any(|ending| name.ends_with(ending)) {
                let relative = path.strip_prefix(root).unwrap_or(&path);
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// Whether a line of a file uses `key`: bound to it in a document, or
/// named in quotes elsewhere.
fn uses(path: &str, line: &str, key: &str) -> bool {
    let quoted = format!("\"{key}\"");
    line.contains(&quoted) || (path.ends_with(".vui") && !bound_at(line, key).is_empty())
}

/// Where a document's line binds text to `key`: `@key` with no more of a
/// key after it.
fn bound_at(line: &str, key: &str) -> Vec<usize> {
    let bound = format!("@{key}");
    let ends = |at: usize| {
        let after = line[at + bound.len()..].chars().next();
        !after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    line.match_indices(bound.as_str()).map(|(at, _)| at).filter(|&at| ends(at)).collect()
}

/// A file's text with `old` renamed `new` where it is used.
fn rename_in(path: &str, text: &str, old: &str, new: &str) -> String {
    let mut renamed = text.replace(&format!("\"{old}\""), &format!("\"{new}\""));
    if path.ends_with(".vui") {
        let lines: Vec<String> = renamed.split('\n').map(|line| rebind(line, old, new)).collect();
        renamed = lines.join("\n");
    }
    renamed
}

fn rebind(line: &str, old: &str, new: &str) -> String {
    let places = bound_at(line, old);
    let mut out = line.to_string();
    for at in places.into_iter().rev() {
        out.replace_range(at + 1..at + 1 + old.len(), new);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "Text/Menu.strings",
            "format = 1\n\n[[entry]]\nkey = \"menu.play\"\nen = \"Play\"\nfr = \"Jouer\"\n\n\
             [[entry]]\nkey = \"menu.quit\"\nen = \"Quit\"\n",
        );
        write("UI/Menu.vui", "<vui>\n<Button text=\"@menu.play\" />\n<Button text=\"@menu.play.long\" />\n</vui>\n");
        write("Scenes/Main.scene", "[[obj]]\n[obj.fields]\ntitle = \"menu.play\"\n");
        write("Scripts/Menu.cs", "var text = Localization.Get(\"menu.play\");\n");
        dir
    }

    #[test]
    fn keys_are_gathered_with_their_text_in_each_language() {
        let dir = project();
        let languages = ["en".to_string(), "fr".to_string()];
        let rows = gather(dir.path(), &["Text/Menu.strings".to_string()], &languages);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].texts, ["Play", "Jouer"]);
        assert_eq!(rows[1].missing(), 1, "Quit has no French");
        assert!(rows[1].matches("QUI") && rows[0].matches("jouer") && !rows[0].matches("quit"));
    }

    #[test]
    fn usages_are_found_and_a_rename_follows_them() {
        let dir = project();
        let found: Vec<(String, usize)> =
            usages(dir.path(), "menu.play").into_iter().map(|u| (u.path, u.line)).collect();
        assert_eq!(found, [("Scenes/Main.scene".into(), 3), ("Scripts/Menu.cs".into(), 1), ("UI/Menu.vui".into(), 2)]);

        assert!(rename(dir.path(), "Text/Menu.strings", "menu.play", "menu.quit").is_err(), "taken");
        let changed = rename(dir.path(), "Text/Menu.strings", "menu.play", "menu.start").unwrap();
        assert_eq!(changed, ["Text/Menu.strings", "Scenes/Main.scene", "Scripts/Menu.cs", "UI/Menu.vui"]);
        let ui = std::fs::read_to_string(dir.path().join("UI/Menu.vui")).unwrap();
        assert!(ui.contains("@menu.start\"") && ui.contains("@menu.play.long"), "only the key itself: {ui}");
        assert!(usages(dir.path(), "menu.play").is_empty());
        let table = std::fs::read_to_string(dir.path().join("Text/Menu.strings")).unwrap();
        assert!(table.contains("key = \"menu.start\""));
    }
}
