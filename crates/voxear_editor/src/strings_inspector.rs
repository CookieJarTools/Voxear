//! A string table in the Inspector, as Unity's string table editor shows
//! one: its entries as a collection field, each its key, its text in every
//! language the project has, and a comment for translators, typed in place
//! and written to the `.strings` file as each field is let go of. A
//! language without the key's text says it is missing, and a text past the
//! key's limit is marked too long. Entries drag into a new order, and Add
//! Entry makes one.
//!
//! Under the entries, the translators' sheet: Export CSV writes the table
//! as a spreadsheet beside it (`Menu.csv` for `Menu.strings`), and Import
//! CSV reads that sheet back, showing what it adds, changes and takes away
//! before Apply keeps it, as `vox loc import` does. Where the sheet is
//! online, as a Google Sheet, its link is kept with the table, and Import
//! from Link fetches it, away from the editor's frames, to show the same.

use std::{path::PathBuf, sync::mpsc};

use anyhow::Result;

use voxear_engine::localization::{StringEntry, StringTable, csv};
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    collection::{self, Collection},
    inspector::prop,
};

/// One entry's fields.
struct EntryFields {
    key: NodeId,
    /// Each language's code and field.
    texts: Vec<(String, NodeId)>,
    comment: NodeId,
    remove: Option<NodeId>,
}

pub(crate) struct StringsFields {
    pub table: StringTable,
    collection: Collection,
    entries: Vec<EntryFields>,
    /// The field typed in last frame, to tell when it is let go of.
    focused: Option<NodeId>,
    sheet: Sheet,
}

/// The translators' sheet beside the table, and the table it holds while
/// its changes wait to be applied.
struct Sheet {
    file: PathBuf,
    languages: Vec<String>,
    export: NodeId,
    import: NodeId,
    /// The sheet's link, and the button that fetches it.
    link: NodeId,
    fetch: NodeId,
    apply: NodeId,
    report: NodeId,
    pending: Option<StringTable>,
    /// The sheet being fetched from its link.
    fetching: Option<mpsc::Receiver<Result<String>>>,
}

/// What changed in a table.
pub(crate) enum StringsChange {
    /// A key, a text or a comment, already in [`StringsFields::table`].
    Typed,
    /// An entry added, taken out or moved, for the view to be built again.
    Entries,
    /// A key that cannot be had, as one another entry has, put back.
    Refused(String),
    /// Something to tell, as the sheet written.
    Notice(String),
}

impl StringsFields {
    /// The table's entries under `root`, a field for each of `languages`
    /// and of any other language an entry has.
    pub fn build(ui: &mut Ui, root: NodeId, table: StringTable, languages: &[String], file: PathBuf) -> Self {
        let mut columns = languages.to_vec();
        for (language, _) in table.entries.iter().flat_map(|entry| &entry.texts) {
            if !columns.contains(language) {
                columns.push(language.clone());
            }
        }
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");
        let foldout = ui.foldout(root, &format!("Entries · {}", table.entries.len()), true);
        ui.add_class(foldout, "section");
        ui.set_foldout_icon(foldout, "globe");
        let rows = ui.foldout_content(foldout).expect("a foldout has content");
        let collection = collection::collection(ui, rows, table.entries.len(), true, Some("Add Entry"));
        let entries = (table.entries.iter().enumerate())
            .map(|(place, entry)| entry_fields(ui, collection.items, place, entry, &columns))
            .collect();
        let sheet = Sheet::build(ui, root, file.with_extension("csv"), columns, &table.sheet);
        Self { table, collection, entries, focused: None, sheet }
    }

    /// The collection field's box of entries, whose slides a rebuild carries.
    pub fn items(&self) -> NodeId {
        self.collection.items
    }

    /// What changed this frame, written into [`table`](Self::table).
    pub fn update(&mut self, ui: &mut Ui) -> Option<StringsChange> {
        if let Some(change) = self.update_sheet(ui) {
            return Some(change);
        }
        if let Some((from, to)) = collection::moved(ui, self.collection.items) {
            let entry = self.table.entries.remove(from);
            self.table.entries.insert(to.min(self.table.entries.len()), entry);
            return Some(StringsChange::Entries);
        }
        if self.collection.add.is_some_and(|add| ui.clicked(add)) {
            let key = (1..).map(|n| if n == 1 { "new.key".to_string() } else { format!("new.key.{n}") });
            let key =
                key.into_iter().find(|key| !self.table.entries.iter().any(|e| e.key == *key)).expect("one is free");
            self.table.entries.push(StringEntry { key, ..StringEntry::default() });
            return Some(StringsChange::Entries);
        }
        if let Some(at) = self.entries.iter().position(|entry| entry.remove.is_some_and(|remove| ui.clicked(remove))) {
            self.table.entries.remove(at);
            return Some(StringsChange::Entries);
        }
        let focused = ui.focused();
        let left = self.focused.filter(|&field| focused != Some(field));
        self.focused = focused.filter(|focused| self.fields().any(|field| field == *focused));
        let typed = |ui: &Ui, field: NodeId| ui.submitted(field) || left == Some(field);
        for (at, fields) in self.entries.iter().enumerate() {
            if typed(ui, fields.key) {
                let key = ui.text(fields.key).trim().to_string();
                if key == self.table.entries[at].key {
                    continue;
                }
                if key.is_empty() || self.table.entries.iter().any(|other| other.key == key) {
                    ui.set_text(fields.key, &self.table.entries[at].key);
                    return Some(StringsChange::Refused(key));
                }
                self.table.entries[at].key = key;
                return Some(StringsChange::Typed);
            }
            let entry = &mut self.table.entries[at];
            if typed(ui, fields.comment) && ui.text(fields.comment) != entry.comment {
                entry.comment = ui.text(fields.comment);
                return Some(StringsChange::Typed);
            }
            for (language, field) in &fields.texts {
                let text = ui.text(*field);
                if !typed(ui, *field) || text == entry.text(language).unwrap_or_default() {
                    continue;
                }
                entry.texts.retain(|(code, _)| code != language);
                if !text.is_empty() {
                    entry.texts.push((language.clone(), text));
                }
                mark(ui, *field, entry, language);
                return Some(StringsChange::Typed);
            }
        }
        None
    }

    /// Exports the sheet, reads it back to show what it would change, or
    /// applies it, taking its table for this one.
    fn update_sheet(&mut self, ui: &mut Ui) -> Option<StringsChange> {
        let sheet = &mut self.sheet;
        let name = sheet.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if ui.clicked(sheet.export) {
            let written = std::fs::write(&sheet.file, csv::to_csv(&self.table, &sheet.languages));
            return Some(StringsChange::Notice(match written {
                Ok(()) => format!("Written to {name}"),
                Err(error) => format!("Could not write {name}: {error}"),
            }));
        }
        // The link is kept once it is let go of, before a click on Import from Link fetches it.
        let link = ui.text(sheet.link).trim().to_string();
        let linked = ui.focused() != Some(sheet.link) && link != self.table.sheet;
        if linked {
            self.table.sheet = link;
        }
        if ui.clicked(sheet.import) {
            let read = std::fs::read_to_string(&sheet.file).map_err(anyhow::Error::from);
            self.read_sheet(ui, read, &name);
        }
        if ui.clicked(self.sheet.fetch) && !self.table.sheet.is_empty() {
            let (send, receive) = mpsc::channel();
            let link = self.table.sheet.clone();
            std::thread::spawn(move || send.send(voxear_localization::csv::fetch(&link)));
            self.sheet.fetching = Some(receive);
            ui.set_text(self.sheet.report, "Fetching…");
            ui.set_visible(self.sheet.report, true);
        }
        let fetched = self.sheet.fetching.as_ref().and_then(|receive| receive.try_recv().ok());
        if let Some(fetched) = fetched {
            self.sheet.fetching = None;
            self.read_sheet(ui, fetched, "The sheet's link");
        }
        let sheet = &mut self.sheet;
        if ui.clicked(sheet.apply)
            && let Some(new) = sheet.pending.take()
        {
            self.table = new;
            return Some(StringsChange::Entries);
        }
        linked.then_some(StringsChange::Typed)
    }

    /// Reads a sheet's text, from its file or its link, and shows what it
    /// would add, change and take away, for Apply to keep.
    fn read_sheet(&mut self, ui: &mut Ui, read: Result<String>, from: &str) {
        let sheet = &mut self.sheet;
        sheet.pending = None;
        let report = match read.and_then(|text| csv::from_csv(&text)) {
            Ok(mut new) => {
                new.sheet.clone_from(&self.table.sheet);
                let report = describe(&csv::changes(&self.table, &new));
                sheet.pending = (report != NOTHING).then_some(new);
                report
            }
            Err(error) => format!("{from}: {error:#}"),
        };
        ui.set_text(sheet.report, &report);
        ui.set_visible(sheet.report, true);
        ui.set_visible(sheet.apply, sheet.pending.is_some());
    }

    fn fields(&self) -> impl Iterator<Item = NodeId> + '_ {
        let each = |entry: &EntryFields| {
            [entry.key, entry.comment]
                .into_iter()
                .chain(entry.texts.iter().map(|(_, field)| *field))
                .collect::<Vec<_>>()
        };
        self.entries.iter().flat_map(each)
    }
}

/// What a sheet read back says when it changes nothing.
const NOTHING: &str = "Nothing changes";

impl Sheet {
    fn build(ui: &mut Ui, root: NodeId, file: PathBuf, languages: Vec<String>, link: &str) -> Self {
        let foldout = ui.foldout(root, "Translators' Sheet", true);
        ui.add_class(foldout, "section");
        ui.set_foldout_icon(foldout, "file");
        let rows = ui.foldout_content(foldout).expect("a foldout has content");
        let row = prop(ui, rows, "CSV");
        let export = ui.button(row, "Export CSV");
        let import = ui.button(row, "Import CSV");
        let row = prop(ui, rows, "Link");
        let link_field = ui.text_field(row, link);
        ui.add_class(link_field, "value-input");
        ui.set_placeholder(link_field, "A Google Sheet or CSV link");
        ui.set_name(link_field, "strings-link");
        let fetch = ui.button(row, "Import from Link");
        let row = prop(ui, rows, "");
        let apply = ui.button(row, "Apply");
        ui.set_visible(apply, false);
        let report = ui.add_label(rows, "");
        ui.add_class(report, "strings-report");
        ui.set_visible(report, false);
        Self { file, languages, export, import, link: link_field, fetch, apply, report, pending: None, fetching: None }
    }
}

/// What a sheet read back would do, a line a key: `+` added, `~` changed
/// with its languages, `-` taken away, under a count of each.
fn describe(changes: &csv::Changes) -> String {
    if changes.is_empty() {
        return NOTHING.to_string();
    }
    let (added, changed, removed) = (changes.added.len(), changes.changed.len(), changes.removed.len());
    let mut lines = vec![format!("{added} added, {changed} changed, {removed} removed")];
    lines.extend(changes.added.iter().map(|key| format!("+ {key}")));
    lines.extend(changes.changed.iter().map(|(key, languages)| match languages.is_empty() {
        true => format!("~ {key}"),
        false => format!("~ {key} ({})", languages.join(", ")),
    }));
    lines.extend(changes.removed.iter().map(|key| format!("- {key}")));
    lines.join("\n")
}

/// An entry's item: its key, a field for each language, and its comment.
fn entry_fields(ui: &mut Ui, items: NodeId, place: usize, entry: &StringEntry, languages: &[String]) -> EntryFields {
    let item = collection::item(ui, items, place, true);
    let field = |ui: &mut Ui, label: &str, text: &str| {
        let row = prop(ui, item.body, label);
        let field = ui.text_field(row, text);
        ui.add_class(field, "value-input");
        field
    };
    let key = field(ui, "Key", &entry.key);
    ui.add_class(key, "strings-key");
    let texts = languages
        .iter()
        .map(|language| {
            let text = field(ui, language, entry.text(language).unwrap_or_default());
            mark(ui, text, entry, language);
            (language.clone(), text)
        })
        .collect();
    let comment = field(ui, "Comment", &entry.comment);
    ui.set_placeholder(comment, "For translators");
    EntryFields { key, texts, comment, remove: item.remove }
}

/// Marks a language's field missing when the entry has no text in it, and
/// too long when its text runs past the entry's limit.
fn mark(ui: &mut Ui, field: NodeId, entry: &StringEntry, language: &str) {
    let text = entry.text(language).unwrap_or_default();
    ui.set_placeholder(field, "Missing");
    ui.set_class(field, "missing", text.is_empty());
    ui.set_class(field, "too-long", entry.limit > 0 && text.chars().count() > entry.limit as usize);
}
