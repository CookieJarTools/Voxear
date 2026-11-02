//! A string table in the Inspector, as Unity's string table editor shows
//! one: its entries as a collection field, each its key, its text in every
//! language the project has, and a comment for translators, typed in place
//! and written to the `.strings` file as each field is let go of. A
//! language without the key's text says it is missing, and a text past the
//! key's limit is marked too long. Entries drag into a new order, and Add
//! Entry makes one.

use voxear_engine::localization::{StringEntry, StringTable};
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
}

/// What changed in a table.
pub(crate) enum StringsChange {
    /// A key, a text or a comment, already in [`StringsFields::table`].
    Typed,
    /// An entry added, taken out or moved, for the view to be built again.
    Entries,
    /// A key that cannot be had, as one another entry has, put back.
    Refused(String),
}

impl StringsFields {
    /// The table's entries under `root`, a field for each of `languages`
    /// and of any other language an entry has.
    pub fn build(ui: &mut Ui, root: NodeId, table: StringTable, languages: &[String]) -> Self {
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
        Self { table, collection, entries, focused: None }
    }

    /// The collection field's box of entries, whose slides a rebuild carries.
    pub fn items(&self) -> NodeId {
        self.collection.items
    }

    /// What changed this frame, written into [`table`](Self::table).
    pub fn update(&mut self, ui: &mut Ui) -> Option<StringsChange> {
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
