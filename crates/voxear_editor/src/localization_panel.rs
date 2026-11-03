//! The Localization Tables window: every key of every string table, a row
//! each, with its table and its text in each of the project's languages,
//! text missing shown as a dash; a search over keys, tables and text; and
//! for the key picked, where it is used and a field to rename it there and
//! in its table (see `localization_tables`). Its columns are dragged by the
//! lines between them, as the Memory panel's are.

use std::{cell::RefCell, rc::Rc};

use voxear_assets::AssetKind;
use voxear_engine::Game;
use voxear_ui::{ItemFns, NodeId, Style, Ui};

use crate::{
    columns::{self, TableHeader},
    localization_tables::{self, KeyRow},
};

const ROW: f32 = 22.0;

/// What the window asks of the editor.
pub enum TablesAction {
    /// Renames a key: its table, the key, and its new name.
    Rename(String, String, String),
}

pub struct LocalizationPanel {
    search: NodeId,
    summary: NodeId,
    body: NodeId,
    list: Option<NodeId>,
    header: Option<TableHeader>,
    made: Rc<RefCell<Vec<NodeId>>>,
    shown: Rc<RefCell<Vec<Vec<String>>>>,
    details: NodeId,
    rows: Vec<KeyRow>,
    filtered: Vec<usize>,
    /// The languages the columns are for, and the tables' text as last read.
    languages: Vec<String>,
    read: Option<u64>,
    picked: Option<usize>,
    rename: Option<NodeId>,
    rename_button: Option<NodeId>,
}

impl LocalizationPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let search = ui.text_field(bar, "");
        ui.add_class(search, "localization-search");
        ui.set_name(search, "localization-search");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");
        let body = ui.panel(root, Style::default());
        ui.add_class(body, "fill");
        ui.add_class(body, "entities-column");
        let details = ui.panel(root, Style::default());
        ui.add_class(details, "localization-details");
        Self {
            search,
            summary,
            body,
            list: None,
            header: None,
            made: Rc::default(),
            shown: Rc::default(),
            details,
            rows: Vec::new(),
            filtered: Vec::new(),
            languages: Vec::new(),
            read: None,
            picked: None,
            rename: None,
            rename_button: None,
        }
    }

    /// Reads the tables again when they changed, follows the search and
    /// what is picked, while the window shows.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, game: &Game) -> Option<TablesAction> {
        if !shown {
            return None;
        }
        let version = game.engine.world.resource::<voxear_engine::localization::Localization>().map(|l| l.version());
        let languages = game.project.file.localization.languages.clone();
        if self.read != version || self.languages != languages || self.list.is_none() {
            self.read = version;
            self.read_tables(ui, game, languages);
        }
        if let Some(header) = &mut self.header
            && header.update(ui)
        {
            let widths = header.widths();
            for &row in self.made.borrow().iter() {
                columns::fit(ui, row, &widths.borrow());
            }
        }
        if ui.changed(self.search) {
            self.filter(ui);
        }
        if let Some(list) = self.list
            && ui.selection_changed(list)
        {
            self.picked = ui.selection(list).first().and_then(|&at| self.filtered.get(at).copied());
            self.show_details(ui, game);
        }
        self.take_rename(ui)
    }

    pub fn list(&self) -> Option<NodeId> {
        self.list
    }

    pub fn details(&self) -> NodeId {
        self.details
    }

    fn take_rename(&mut self, ui: &mut Ui) -> Option<TablesAction> {
        let (field, button) = (self.rename?, self.rename_button?);
        if !ui.clicked(button) && !ui.submitted(field) {
            return None;
        }
        let row = self.rows.get(self.picked?)?;
        let new = ui.text(field).trim().to_string();
        (new != row.key).then(|| TablesAction::Rename(row.table.clone(), row.key.clone(), new))
    }

    /// Reads every table, and makes the columns again for `languages`.
    fn read_tables(&mut self, ui: &mut Ui, game: &Game, languages: Vec<String>) {
        let db = game.content.db();
        let tables: Vec<String> =
            db.entries().filter(|entry| entry.kind == AssetKind::Strings).map(|entry| entry.path.clone()).collect();
        let picked = self.picked.and_then(|at| self.rows.get(at)).map(|row| (row.table.clone(), row.key.clone()));
        self.rows = localization_tables::gather(&game.project.root, &tables, &languages);
        self.picked = picked.and_then(|(table, key)| self.rows.iter().position(|r| r.table == table && r.key == key));
        if self.languages != languages || self.list.is_none() {
            self.languages = languages;
            self.make_columns(ui);
        }
        self.filter(ui);
        self.show_details(ui, game);
    }

    fn make_columns(&mut self, ui: &mut Ui) {
        for child in ui.children(self.body) {
            ui.remove(child);
        }
        self.made.borrow_mut().clear();
        let mut headings: Vec<(String, f32)> = vec![("Key".into(), 0.25), ("Table".into(), 0.2)];
        let share = 0.55 / self.languages.len().max(1) as f32;
        headings.extend(self.languages.iter().map(|language| (language.clone(), share)));
        if let Some(last) = headings.last_mut() {
            last.1 = 0.0;
        }
        let titles: Vec<(&str, f32)> = headings.iter().map(|(title, share)| (title.as_str(), *share)).collect();
        let header = TableHeader::new(ui, self.body, &titles);
        let (widths, made, shown) = (header.widths(), self.made.clone(), self.shown.clone());
        let count = headings.len();
        let list = ui.list_view(
            self.body,
            ROW,
            0,
            ItemFns {
                make: move |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "entities-row");
                    ui.add_class(row, "table-row");
                    for at in 0..count {
                        let label = ui.add_label(row, "");
                        ui.add_class(label, if at == 0 { "entities-row-name" } else { "localization-cell" });
                    }
                    columns::fit(ui, row, &widths.borrow());
                    made.borrow_mut().push(row);
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = shown.borrow();
                    let Some(cells) = shown.get(index) else { return };
                    for (label, text) in ui.children(row).into_iter().zip(cells) {
                        ui.set_text(label, text);
                    }
                },
            },
        );
        ui.add_class(list, "fill");
        ui.set_name(list, "localization-list");
        self.header = Some(header);
        self.list = Some(list);
    }

    fn filter(&mut self, ui: &mut Ui) {
        let search = ui.text(self.search);
        self.filtered = (0..self.rows.len()).filter(|&at| self.rows[at].matches(&search)).collect();
        let cells = |row: &KeyRow| {
            let mut cells = vec![row.key.clone(), row.table.rsplit('/').next().unwrap_or_default().to_string()];
            cells.extend(row.texts.iter().map(|text| if text.is_empty() { "—".to_string() } else { text.clone() }));
            cells
        };
        *self.shown.borrow_mut() = self.filtered.iter().map(|&at| cells(&self.rows[at])).collect();
        let missing: usize = self.rows.iter().map(KeyRow::missing).sum();
        let keys = match self.rows.len() {
            1 => "1 key".to_string(),
            count => format!("{count} keys"),
        };
        let summary = if missing == 0 { keys } else { format!("{keys}   {missing} missing") };
        ui.set_text(self.summary, &summary);
        let Some(list) = self.list else { return };
        ui.set_list_count(list, self.filtered.len());
        ui.refresh_list(list);
        let picked: Vec<usize> =
            self.picked.and_then(|picked| self.filtered.iter().position(|&at| at == picked)).into_iter().collect();
        ui.set_selection(list, &picked);
        let _ = ui.selection_changed(list);
    }

    /// The picked key's usages and its rename field.
    fn show_details(&mut self, ui: &mut Ui, game: &Game) {
        for child in ui.children(self.details) {
            ui.remove(child);
        }
        self.rename = None;
        self.rename_button = None;
        let Some(row) = self.picked.and_then(|at| self.rows.get(at)) else { return };
        let bar = ui.panel(self.details, Style::row());
        ui.add_class(bar, "localization-rename");
        let label = ui.add_label(bar, "Rename");
        ui.add_class(label, "input-property");
        let field = ui.text_field(bar, &row.key);
        ui.add_class(field, "input-field");
        ui.set_name(field, "localization-rename");
        let button = ui.button(bar, "Rename");
        self.rename = Some(field);
        self.rename_button = Some(button);
        let usages = localization_tables::usages(&game.project.root, &row.key);
        let heading = match usages.len() {
            0 => "Not used".to_string(),
            1 => "Used once".to_string(),
            count => format!("Used {count} times"),
        };
        let heading = ui.add_label(self.details, &heading);
        ui.add_class(heading, "profiler-heading");
        for usage in usages.iter().take(12) {
            let line = ui.add_label(self.details, &format!("{}:{}   {}", usage.path, usage.line, usage.text));
            ui.add_class(line, "localization-usage");
        }
    }
}
