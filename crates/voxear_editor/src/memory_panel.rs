//! The Memory panel, the memory profiler: what each part of the engine holds
//! now, a line kept up to date while the panel shows, and snapshots taken by
//! hand, each a list of amounts by part and name (see `voxear_engine::memory`).
//! A snapshot picked shows its amounts, the largest first; compared with
//! another, only what changed, the largest change first, as before and after
//! a level change to find what was not let go.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_core::memory::{self, Change, Snapshot};
use voxear_engine::Game;
use voxear_render::Renderer;
use voxear_ui::{ItemFns, NodeId, Style, Ui};

/// How often the line of totals is read again while the panel shows.
const REFRESH: Duration = Duration::from_secs(1);
const ROW: f32 = 22.0;
/// The classes of an amount's columns: its part, its name, its size and its
/// change.
const AMOUNT_COLUMNS: [&str; 4] = ["memory-part", "entities-row-name", "entities-row-count", "memory-change"];

/// A row of the amounts' list: part, name, bytes, and the change in bytes
/// when comparing.
#[derive(Clone, Debug, PartialEq)]
struct Amount {
    category: String,
    name: String,
    bytes: u64,
    change: Option<i64>,
}

#[derive(Default)]
struct Shown {
    snapshots: Vec<(Snapshot, String)>,
    amounts: Vec<Amount>,
}

pub struct MemoryPanel {
    summary: NodeId,
    take: NodeId,
    compare: NodeId,
    snapshots: NodeId,
    amounts: NodeId,
    hint: NodeId,
    shown: Rc<RefCell<Shown>>,
    picked: Option<usize>,
    read: Option<Instant>,
}

impl MemoryPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let take = ui.button(bar, "Take Snapshot");
        ui.add_class(take, "memory-take");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        let compare_label = ui.add_label(bar, "Compare with");
        ui.add_class(compare_label, "entities-hint");
        let compare = ui.dropdown(bar, &["Nothing"], 0);
        ui.add_class(compare, "memory-compare");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let columns = ui.panel(root, Style::row());
        ui.add_class(columns, "fill");
        ui.add_class(columns, "entities-columns");
        let shown: Rc<RefCell<Shown>> = Rc::default();
        let column = |ui: &mut Ui, title: &str, class: &str| {
            let column = ui.panel(columns, Style::default());
            ui.add_class(column, "entities-column");
            ui.add_class(column, class);
            let heading = ui.add_label(column, title);
            ui.add_class(heading, "profiler-heading");
            column
        };

        let left = column(ui, "Snapshots", "memory-snapshots");
        let rows = shown.clone();
        let snapshots = ui.list_view(
            left,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| make_row(ui, row, &["entities-row-name", "entities-row-count"]),
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = rows.borrow();
                    let Some((snapshot, when)) = shown.snapshots.get(index) else { return };
                    fill_row(ui, row, &[&snapshot.label, when]);
                },
            },
        );
        ui.add_class(snapshots, "fill");

        let right = column(ui, "Amounts", "memory-amounts");
        let hint = ui.add_label(right, "Take a snapshot to see what each part of the engine holds.");
        ui.add_class(hint, "entities-hint");
        let rows = shown.clone();
        let amounts = ui.list_view(
            right,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| make_row(ui, row, &AMOUNT_COLUMNS),
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = rows.borrow();
                    let Some(amount) = shown.amounts.get(index) else { return };
                    let change = amount.change.map_or_else(String::new, signed);
                    fill_row(ui, row, &[&amount.category, &amount.name, &memory::describe(amount.bytes), &change]);
                },
            },
        );
        ui.add_class(amounts, "fill");
        Self { summary, take, compare, snapshots, amounts, hint, shown, picked: None, read: None }
    }

    /// Takes a snapshot when asked, follows what is picked, and keeps the
    /// line of totals up to date while the panel shows.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, game: &Game, renderer: &Renderer) {
        if !shown {
            return;
        }
        let mut changed = false;
        if ui.clicked(self.take) {
            self.take_snapshot(ui, game, renderer);
            changed = true;
        }
        if ui.selection_changed(self.snapshots) {
            self.picked = ui.selection(self.snapshots).first().copied();
            changed = true;
        }
        changed |= ui.changed(self.compare);
        if changed {
            self.show_amounts(ui);
        }
        if self.read.is_none_or(|read| read.elapsed() >= REFRESH) {
            self.read = Some(Instant::now());
            let now = voxear_engine::memory::snapshot("Now", &game.engine, Some(renderer));
            let parts: Vec<String> =
                now.totals().iter().map(|(part, bytes)| format!("{part} {}", memory::describe(*bytes))).collect();
            ui.set_text(self.summary, &parts.join("   "));
        }
    }

    /// The snapshots taken, oldest first.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        self.shown.borrow().snapshots.iter().map(|(snapshot, _)| snapshot.clone()).collect()
    }

    pub fn take_button(&self) -> NodeId {
        self.take
    }

    pub fn compare(&self) -> NodeId {
        self.compare
    }

    pub fn amounts(&self) -> NodeId {
        self.amounts
    }

    fn take_snapshot(&mut self, ui: &mut Ui, game: &Game, renderer: &Renderer) {
        let count = self.shown.borrow().snapshots.len() + 1;
        let level = game.levels.current().map(|path| path.rsplit('/').next().unwrap_or(path).to_string());
        let label = match level {
            Some(level) => format!("Snapshot {count} · {level}"),
            None => format!("Snapshot {count}"),
        };
        let taken = voxear_engine::memory::snapshot(&label, &game.engine, Some(renderer));
        let total = taken.totals().iter().filter(|(part, _)| part != "Process").map(|(_, bytes)| bytes).sum();
        self.shown.borrow_mut().snapshots.push((taken, memory::describe(total)));
        let snapshots = self.snapshots();
        let mut options = vec!["Nothing".to_string()];
        options.extend(snapshots.iter().map(|s| s.label.clone()));
        let labels: Vec<&str> = options.iter().map(String::as_str).collect();
        let chosen = ui.selected_option(self.compare);
        ui.set_options(self.compare, &labels);
        ui.set_selected_option(self.compare, chosen);
        ui.set_list_count(self.snapshots, snapshots.len());
        ui.refresh_list(self.snapshots);
        self.picked = Some(snapshots.len() - 1);
        ui.set_selection(self.snapshots, &[snapshots.len() - 1]);
        let _ = ui.selection_changed(self.snapshots);
    }

    /// The picked snapshot's amounts, the largest first, or what changed
    /// since the one compared with.
    fn show_amounts(&mut self, ui: &mut Ui) {
        let snapshots = self.snapshots();
        let picked = self.picked.and_then(|index| snapshots.get(index));
        let baseline = ui.selected_option(self.compare).checked_sub(1).and_then(|index| snapshots.get(index));
        let amounts: Vec<Amount> = match (picked, baseline) {
            (Some(picked), Some(baseline)) => memory::diff(baseline, picked).into_iter().map(changed).collect(),
            (Some(picked), None) => {
                let mut amounts: Vec<Amount> = picked
                    .entries
                    .iter()
                    .map(|e| Amount {
                        category: e.category.clone(),
                        name: e.name.clone(),
                        bytes: e.bytes,
                        change: None,
                    })
                    .collect();
                amounts.sort_by_key(|amount| std::cmp::Reverse(amount.bytes));
                amounts
            }
            (None, _) => Vec::new(),
        };
        let hint = match (picked, baseline) {
            (None, _) => "Take a snapshot to see what each part of the engine holds.",
            (Some(_), Some(_)) if amounts.is_empty() => "Nothing changed.",
            _ => "",
        };
        ui.set_text(self.hint, hint);
        ui.set_visible(self.hint, !hint.is_empty());
        let count = amounts.len();
        self.shown.borrow_mut().amounts = amounts;
        ui.set_list_count(self.amounts, count);
        ui.refresh_list(self.amounts);
    }
}

fn changed(change: Change) -> Amount {
    let delta = change.delta();
    Amount { category: change.category, name: change.name, bytes: change.after, change: Some(delta) }
}

/// A change in bytes, with its sign.
fn signed(delta: i64) -> String {
    let sign = if delta < 0 { "−" } else { "+" };
    format!("{sign}{}", memory::describe(delta.unsigned_abs()))
}

/// A row of labels, one for each column, each with its column's class.
fn make_row(ui: &mut Ui, row: NodeId, columns: &[&str]) {
    ui.add_class(row, "entities-row");
    for class in columns {
        let label = ui.add_label(row, "");
        ui.add_class(label, class);
    }
}

fn fill_row(ui: &mut Ui, row: NodeId, texts: &[&str]) {
    for (label, text) in ui.children(row).into_iter().zip(texts) {
        ui.set_text(label, text);
    }
}
