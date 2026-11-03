//! String tables as CSV, for translators working in a spreadsheet: a row a
//! key, under a header naming the columns `key`, `comment`, `limit` and one
//! for each language, by its code.
//!
//! ```csv
//! key,comment,limit,en,fr
//! menu.play,The first button,12,Play,Jouer
//! loot.found,,,"{player} found {count} coins","{player} a trouvé {count} pièces"
//! ```
//!
//! Reading a sheet back gives the table as the sheet has it; [`changes`]
//! says what that adds, changes and takes away, to show before it is kept.
//!
//! A sheet can be read from a link too ([`fetch`], with the `fetch`
//! feature, as the editor and `vox` have): a CSV file's, or a Google
//! Sheet's, which [`sheet_link`] turns into its CSV export.

use anyhow::{Context, Result, bail, ensure};

use crate::table::{StringEntry, StringTable};

/// A sheet's link as the CSV it can be read from: a Google Sheet's, as its
/// address bar or Share gives it, is its export of that tab as CSV; any
/// other is as it is.
pub fn sheet_link(link: &str) -> String {
    let link = link.trim();
    let Some(rest) = link.split_once("docs.google.com/spreadsheets/d/").map(|(_, rest)| rest) else {
        return link.to_string();
    };
    if rest.contains("/export?") || rest.contains("/pub?") {
        return link.to_string();
    }
    let id = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let gid = link.split(['#', '?', '&']).find_map(|part| part.strip_prefix("gid=")).unwrap_or("0");
    format!("https://docs.google.com/spreadsheets/d/{id}/export?format=csv&gid={gid}")
}

/// A sheet's text, fetched from its link (see [`sheet_link`]).
#[cfg(feature = "fetch")]
pub fn fetch(link: &str) -> Result<String> {
    let url = sheet_link(link);
    let mut response = ureq::get(&url).call().with_context(|| format!("fetching {url}"))?;
    response.body_mut().read_to_string().with_context(|| format!("reading {url}"))
}

/// The columns every sheet has before its languages.
const FIRST: [&str; 3] = ["key", "comment", "limit"];

/// A table as a sheet: the languages in `languages` first, in that order,
/// then any others its entries have.
pub fn to_csv(table: &StringTable, languages: &[String]) -> String {
    let mut columns: Vec<String> = languages.to_vec();
    for entry in &table.entries {
        for (language, _) in &entry.texts {
            if !columns.contains(language) {
                columns.push(language.clone());
            }
        }
    }
    let header = FIRST.iter().map(|name| name.to_string()).chain(columns.iter().cloned());
    let mut out = line(header);
    for entry in &table.entries {
        let limit = if entry.limit > 0 { entry.limit.to_string() } else { String::new() };
        let texts = columns.iter().map(|language| entry.text(language).unwrap_or_default().to_string());
        out.push_str(&line([entry.key.clone(), entry.comment.clone(), limit].into_iter().chain(texts)));
    }
    out
}

/// A sheet as a table. A cell left empty is a language without that key's
/// text; a row without a key is skipped.
pub fn from_csv(text: &str) -> Result<StringTable> {
    let mut rows = rows(text.trim_start_matches('\u{feff}'))?.into_iter();
    let header = rows.next().context("the sheet is empty")?;
    let column = |name: &str| header.iter().position(|cell| cell.trim().eq_ignore_ascii_case(name));
    let key = column("key").context("the sheet has no `key` column")?;
    let (comment, limit) = (column("comment"), column("limit"));
    let languages: Vec<(usize, String)> = header
        .iter()
        .enumerate()
        .filter(|(_, name)| !FIRST.iter().any(|first| name.trim().eq_ignore_ascii_case(first)))
        .map(|(at, name)| (at, crate::normalize(name)))
        .filter(|(_, code)| !code.is_empty())
        .collect();
    let mut table = StringTable::default();
    for (at, row) in rows.enumerate() {
        let cell = |column: Option<usize>| column.and_then(|c| row.get(c)).map_or("", |cell| cell.as_str());
        let name = cell(Some(key)).trim();
        if name.is_empty() {
            continue;
        }
        ensure!(!table.entries.iter().any(|e| e.key == name), "row {}: `{name}` is in the sheet twice", at + 2);
        let limit = match cell(limit).trim() {
            "" => 0,
            number => number.parse().with_context(|| format!("row {}: a limit is a whole number", at + 2))?,
        };
        let texts = languages.iter().map(|(column, code)| (code.clone(), cell(Some(*column)).to_string()));
        table.entries.push(StringEntry {
            key: name.to_string(),
            comment: cell(comment).to_string(),
            limit,
            texts: texts.filter(|(_, text)| !text.is_empty()).collect(),
        });
    }
    Ok(table)
}

/// What keeping `new` over `old` would do: the keys it adds and takes
/// away, and for each key both have, the languages whose text changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub added: Vec<String>,
    pub changed: Vec<(String, Vec<String>)>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

pub fn changes(old: &StringTable, new: &StringTable) -> Changes {
    let find = |table: &StringTable, key: &str| table.entries.iter().find(|e| e.key == key).cloned();
    let mut changes = Changes::default();
    for entry in &new.entries {
        let Some(before) = find(old, &entry.key) else {
            changes.added.push(entry.key.clone());
            continue;
        };
        let mut languages: Vec<String> =
            entry.texts.iter().chain(&before.texts).map(|(code, _)| code.clone()).collect();
        languages.sort();
        languages.dedup();
        let differ: Vec<String> = languages.into_iter().filter(|code| entry.text(code) != before.text(code)).collect();
        if !differ.is_empty() || entry.comment != before.comment || entry.limit != before.limit {
            changes.changed.push((entry.key.clone(), differ));
        }
    }
    changes.removed = old.entries.iter().filter(|e| find(new, &e.key).is_none()).map(|e| e.key.clone()).collect();
    changes
}

/// A row, each cell quoted when it holds a comma, a quote or a line break,
/// or starts or ends with a space.
fn line(cells: impl IntoIterator<Item = String>) -> String {
    let quoted = |cell: String| {
        let plain = !cell.contains([',', '"', '\n', '\r']) && cell.trim() == cell;
        if plain { cell } else { format!("\"{}\"", cell.replace('"', "\"\"")) }
    };
    let mut out = cells.into_iter().map(quoted).collect::<Vec<_>>().join(",");
    out.push('\n');
    out
}

/// A sheet's rows, as RFC 4180 has them: cells split by commas, rows by
/// line breaks, and a quoted cell keeping its commas and line breaks, `""`
/// a quote within it.
fn rows(text: &str) -> Result<Vec<Vec<String>>> {
    let (mut rows, mut row, mut cell) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => cell.push(c),
            (false, '"') if cell.is_empty() => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n' | '\r') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => cell.push(c),
        }
    }
    if quoted {
        bail!("a quoted cell is never closed");
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> StringTable {
        StringTable::parse(
            "[[entry]]\nkey = \"menu.play\"\ncomment = \"The first button\"\nlimit = 12\nen = \"Play\"\nfr = \"Jouer\"\n\n\
             [[entry]]\nkey = \"loot\"\nen = \"{a}, \\\"{b}\\\"\\nand more\"\npt-BR = \" Saque \"\n",
        )
        .unwrap()
    }

    #[test]
    fn a_table_goes_to_a_sheet_and_back() {
        let sheet = to_csv(&table(), &["en".into(), "fr".into()]);
        assert_eq!(
            sheet,
            "key,comment,limit,en,fr,pt-BR\nmenu.play,The first button,12,Play,Jouer,\n\
             loot,,,\"{a}, \"\"{b}\"\"\nand more\",,\" Saque \"\n"
        );
        assert_eq!(from_csv(&sheet).unwrap(), table(), "read back as it was");
        let spreadsheet = format!("\u{feff}{}", sheet.replace('\n', "\r\n"));
        assert_eq!(from_csv(&spreadsheet).unwrap().entries.len(), 2, "a spreadsheet's mark and line ends");
        assert!(from_csv("key,en\na,\"open\n").is_err());
        assert!(from_csv("en\nPlay\n").is_err(), "a sheet needs its keys");
    }

    #[test]
    fn changes_say_what_a_sheet_adds_changes_and_takes_away() {
        let sheet = "key,en,fr\nmenu.play,Play,Lancer\nmenu.quit,Quit,\n";
        let changes = changes(&table(), &from_csv(sheet).unwrap());
        assert_eq!(changes.added, ["menu.quit"]);
        assert_eq!(changes.changed, [("menu.play".into(), vec!["fr".to_string()])], "and its comment and limit went");
        assert_eq!(changes.removed, ["loot"]);
        assert!(super::changes(&table(), &table()).is_empty());
    }

    #[test]
    fn a_google_sheets_link_is_read_as_its_csv() {
        let shared = "https://docs.google.com/spreadsheets/d/1AbC_d-9/edit?usp=sharing#gid=42";
        assert_eq!(sheet_link(shared), "https://docs.google.com/spreadsheets/d/1AbC_d-9/export?format=csv&gid=42");
        let first = "https://docs.google.com/spreadsheets/d/1AbC_d-9/edit";
        assert!(sheet_link(first).ends_with("/1AbC_d-9/export?format=csv&gid=0"), "the first tab");
        assert_eq!(sheet_link(" https://example.com/menu.csv "), "https://example.com/menu.csv");
    }
}
