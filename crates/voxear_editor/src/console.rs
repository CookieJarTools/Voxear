//! The Console, after CookieJarTools' Sifter (docs/ARCHITECTURE.md): a log
//! worth reading.
//!
//! - **One line per row**, in columns: when (minutes and seconds since the
//!   editor started), the message, and where it came from. Monospaced
//!   throughout, as nearly all of it is code; warnings and errors take their
//!   own colour.
//! - **Categories nobody declared** (see `entry`), as chips in the toolbar
//!   with how many lines each has; a click hides or shows a category.
//! - **Search** (see `query`): words, `tag:` and `-tag:`, and `/regex/`; a bad
//!   expression says why under the toolbar.
//! - **Log, Warning and Error** toggles with their counts, **Collapse** to
//!   show identical lines once with how many there were, **Clear**, and
//!   **Error Pause**, which pauses play on an error.
//! - **The selected line's stack**, below, coloured part by part: namespace,
//!   class, method, parameters, file and line. Namespaces and parameters can
//!   be hidden, and **My code only** folds the frames that are not the
//!   project's own into one line to unfold. Any frame with a file opens it at
//!   its line in the code editor the settings name, as a double click on a
//!   line opens where it came from. Selecting a line about an Obj selects the
//!   Obj; Copy puts the line and its stack on the clipboard.
//! - A context click on a line copies it or mutes it: muted messages stay
//!   hidden until the toolbar's Muted button brings them back.

mod entry;
mod frame;
mod query;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};

use tracing::Level;
use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_ui::{Color, ItemFns, MenuItem, NodeId, RichText, Style, Ui};

use self::{entry::Entry, frame::Frame, query::Query};
use crate::settings::Settings;

/// A line's height, and the list's share of the Console over the trace pane.
const ROW: f32 = 20.0;
const LIST_SHARE: f32 = 0.6;
/// The most lines held; older ones go first.
const KEPT: usize = 10_000;
/// Each part of a frame's colour, tuned for the dark theme.
const NAMESPACE: u32 = 0x7d8597;
const CLASS: u32 = 0x7cc4fa;
const METHOD: u32 = 0xe8c37e;
const PARAMETERS: u32 = 0x6b7385;
const FILE: u32 = 0xc3a6ff;
const FOLDED: u32 = 0x8b93a7;
/// Categories' colours, given out by name so a category keeps its own.
const CATEGORY_COLORS: [u32; 8] = [0x7cc4fa, 0x4cb782, 0xe8c37e, 0xc3a6ff, 0xf2a65a, 0x5ec4c4, 0xeb7f9b, 0xa3c46b];

/// A row: the line it shows, and how many identical ones it stands for.
#[derive(Clone, Copy, Debug)]
struct Shown {
    entry: usize,
    count: usize,
}

/// What a frame's update asks of the editor.
#[derive(Debug, Default)]
pub struct ConsoleUpdate {
    /// A setting was changed from the Console, so the settings are saved.
    pub settings_changed: bool,
    /// Errors logged since the last frame, for Error Pause.
    pub errors: usize,
    /// A frame clicked: its file, and line, to open in the code editor.
    pub open: Option<(PathBuf, u32)>,
    /// The Obj the line selected is about, to select it.
    pub ping: Option<Entity>,
}

/// The toolbar's buttons, and the line under it that says what is wrong
/// with a search.
struct Toolbar {
    clear: NodeId,
    collapse: NodeId,
    pause: NodeId,
    chips: NodeId,
    muted: NodeId,
    search_box: NodeId,
    search: NodeId,
    problem: NodeId,
    /// Log, Warning and Error, each with its count's label.
    levels: [(NodeId, NodeId); 3],
}

/// The pane under the list: the selected line's details, message and stack.
struct Detail {
    info: NodeId,
    message: NodeId,
    trace: NodeId,
    mine_only: NodeId,
    namespaces: NodeId,
    parameters: NodeId,
    copy: NodeId,
}

pub struct Console {
    list: NodeId,
    log: LogCapture,
    /// The project's folder, whose files are its own code, and the engine's,
    /// which a Rust line's file is relative to.
    project: PathBuf,
    engine: PathBuf,
    entries: Rc<RefCell<Vec<Entry>>>,
    shown: Rc<RefCell<Vec<Shown>>>,
    /// Lines logged so far, kept or not, so it knows which are new, and
    /// whether any came since the list was last filtered.
    seen: u64,
    new_lines_arrived: bool,
    toolbar: Toolbar,
    detail: Detail,
    query: Query,
    /// Levels hidden (Log, Warning, Error), categories hidden, and messages muted.
    hidden: [bool; 3],
    hidden_categories: HashSet<String>,
    muted: HashSet<String>,
    /// Each category's chip, as last built.
    chips: Vec<(String, NodeId)>,
    selected: Option<usize>,
    /// The selected line's frames that are not the project's own are shown.
    unfolded: bool,
    /// What the toggles showed last, to build the stack again when they change.
    drawn: Option<(usize, bool, bool, bool, bool)>,
}

impl Console {
    /// Builds the Console in `root`, showing what `log` holds.
    pub fn new(ui: &mut Ui, root: NodeId, log: LogCapture, project: &Path, settings: &Settings) -> Self {
        let toolbar = toolbar(ui, root);
        let split = ui.split_view_fraction(root, false, LIST_SHARE);
        ui.add_class(split, "fill");
        let (top, bottom) = ui.split_panes(split).expect("a split view has two panes");
        let entries: Rc<RefCell<Vec<Entry>>> = Rc::default();
        let shown: Rc<RefCell<Vec<Shown>>> = Rc::default();
        let (read, rows) = (entries.clone(), shown.clone());
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                for (part, class) in [
                    ("", "console-icon"),
                    ("", "console-time"),
                    ("", "console-message"),
                    ("", "console-count"),
                    ("", "console-source"),
                ] {
                    let made = if class == "console-icon" { ui.icon(row, "info") } else { ui.add_label(row, part) };
                    ui.add_class(made, class);
                }
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let (entries, rows) = (read.borrow(), rows.borrow());
                let Some(shown) = rows.get(index) else { return };
                let Some(entry) = entries.get(shown.entry) else { return };
                bind_row(ui, row, entry, shown.count);
            },
        };
        let list = ui.list_view(top, ROW, 0, items);
        ui.add_class(list, "fill");
        ui.add_class(list, "console");
        let detail = detail(ui, bottom, settings);
        let engine = voxear_engine::engine_root();
        // Stacks name files by their real paths, so the project's folder is
        // made real too, or a project opened as `../Game` owns none of them.
        let project = project.canonicalize().unwrap_or_else(|_| project.to_path_buf());
        let console = Self {
            list,
            log,
            project,
            engine,
            entries,
            shown,
            seen: 0,
            new_lines_arrived: false,
            toolbar,
            detail,
            query: Query::default(),
            hidden: [false; 3],
            hidden_categories: HashSet::new(),
            muted: HashSet::new(),
            chips: Vec::new(),
            selected: None,
            unfolded: false,
            drawn: None,
        };
        console.show_toggles(ui, settings);
        console
    }

    /// Takes in lines logged since the last frame and follows the toolbar,
    /// the list, its menu and the trace pane.
    pub fn update(&mut self, ui: &mut Ui, settings: &mut Settings) -> ConsoleUpdate {
        let mut update = ConsoleUpdate { errors: self.take_new_lines(), ..ConsoleUpdate::default() };
        let mut changed = update.errors > 0 || self.new_lines_arrived;
        self.new_lines_arrived = false;
        changed |= self.toolbar_clicks(ui, settings, &mut update);
        changed |= self.menu(ui);
        if changed {
            self.refresh(ui, settings);
        }
        self.follow_selection(ui, &mut update);
        self.open_on_double_click(ui, &mut update);
        update.settings_changed |= self.detail_clicks(ui, settings, &mut update);
        self.show_toggles(ui, settings);
        self.draw_detail(ui, settings);
        update
    }

    /// Hides every line logged so far, as Clear does.
    pub fn clear(&mut self, ui: &mut Ui) {
        self.entries.borrow_mut().clear();
        self.selected = None;
        self.drawn = None;
        self.shown.borrow_mut().clear();
        ui.set_list_count(self.list, 0);
        ui.refresh_list(self.list);
        self.rebuild_chips(ui);
    }

    /// Reads the lines logged since it last looked. Returns how many were
    /// errors.
    fn take_new_lines(&mut self) -> usize {
        let Ok(log) = self.log.lock() else { return 0 };
        let new = (log.total - self.seen).min(log.lines.len() as u64) as usize;
        self.seen = log.total;
        if new == 0 {
            return 0;
        }
        let lines: Vec<_> = log.lines.iter().skip(log.lines.len() - new).cloned().collect();
        drop(log);
        let mut entries = self.entries.borrow_mut();
        let errors = lines.iter().filter(|l| l.level == Level::ERROR).count();
        entries.extend(lines.iter().map(|line| Entry::new(line, &self.project)));
        let over = entries.len().saturating_sub(KEPT);
        if over > 0 {
            entries.drain(..over);
            self.selected = self.selected.and_then(|s| s.checked_sub(over));
        }
        self.new_lines_arrived = true;
        errors
    }

    /// The toolbar's buttons and search. Returns whether the list needs
    /// filtering again.
    fn toolbar_clicks(&mut self, ui: &mut Ui, settings: &mut Settings, update: &mut ConsoleUpdate) -> bool {
        let toolbar = &self.toolbar;
        let mut refilter = false;
        if ui.clicked(toolbar.clear) {
            self.clear(ui);
        }
        let toolbar = &self.toolbar;
        if ui.clicked(toolbar.collapse) {
            settings.console_collapse = !settings.console_collapse;
            update.settings_changed = true;
            refilter = true;
        }
        if ui.clicked(toolbar.pause) {
            settings.error_pause = !settings.error_pause;
            update.settings_changed = true;
        }
        for (slot, (level, _)) in toolbar.levels.iter().enumerate() {
            if ui.clicked(*level) {
                self.hidden[slot] = !self.hidden[slot];
                refilter = true;
            }
        }
        let clicked: Vec<String> =
            self.chips.iter().filter(|(_, chip)| ui.clicked(*chip)).map(|(n, _)| n.clone()).collect();
        for name in clicked {
            if !self.hidden_categories.remove(&name) {
                self.hidden_categories.insert(name);
            }
            refilter = true;
        }
        if ui.clicked(self.toolbar.muted) {
            self.muted.clear();
            refilter = true;
        }
        if ui.changed(self.toolbar.search) {
            self.query = Query::parse(&ui.text(self.toolbar.search));
            let problem = self.query.error.clone().unwrap_or_default();
            ui.set_text(self.toolbar.problem, &problem);
            ui.set_visible(self.toolbar.problem, !problem.is_empty());
            ui.set_class(self.toolbar.search_box, "bad", !problem.is_empty());
            refilter = true;
        }
        refilter
    }

    /// A context click on a line: Copy, or Mute this message. Returns
    /// whether the list needs filtering again.
    fn menu(&mut self, ui: &mut Ui) -> bool {
        if let Some(choice) = ui.menu_choice(self.list) {
            let entries = self.entries.borrow();
            let Some(entry) = self.selected.and_then(|at| entries.get(at)) else { return false };
            match choice {
                0 => ui.copy_text(&entry.as_text()),
                _ => {
                    self.muted.insert(entry.message.clone());
                    return true;
                }
            }
        }
        if ui.context_clicked(self.list) && !ui.selection(self.list).is_empty() {
            let at = ui.pointer_position().unwrap_or_default();
            ui.open_menu(self.list, at, &[MenuItem::new("Copy"), MenuItem::new("Mute this message")]);
        }
        false
    }

    /// Whether a line passes the categories, the muted messages and the
    /// search, whatever its level.
    fn passes(&self, entry: &Entry) -> bool {
        !entry.categories.iter().any(|c| self.hidden_categories.contains(c))
            && !self.muted.contains(&entry.message)
            && (self.query.is_empty() || self.query.matches(&entry.message, &entry.lower, &entry.categories))
    }

    /// Filters the lines into rows, collapsing identical ones when asked,
    /// counts each level, and follows the end while scrolled to it.
    fn refresh(&mut self, ui: &mut Ui, settings: &Settings) {
        let entries = self.entries.borrow();
        let mut shown: Vec<Shown> = Vec::new();
        let mut same: HashMap<(Level, &str, Option<&str>), usize> = HashMap::new();
        let mut counts = [0usize; 3];
        for (index, entry) in entries.iter().enumerate() {
            if !self.passes(entry) {
                continue;
            }
            let slot = level_slot(entry.level);
            counts[slot] += 1;
            if self.hidden[slot] {
                continue;
            }
            if settings.console_collapse {
                let key = (entry.level, entry.message.as_str(), entry.source().map(|f| f.raw.as_str()));
                if let Some(&at) = same.get(&key) {
                    shown[at] = Shown { entry: index, count: shown[at].count + 1 };
                    continue;
                }
                same.insert(key, shown.len());
            }
            shown.push(Shown { entry: index, count: 1 });
        }
        drop(entries);
        let before = self.shown.borrow().len();
        let [_, scrolled] = ui.scroll_offset(self.list);
        let [_, _, _, height] = ui.viewport(self.list);
        let following = scrolled + height >= before as f32 * ROW - ROW;
        let count = shown.len();
        let selected = self.selected.and_then(|e| shown.iter().position(|s| s.entry == e));
        *self.shown.borrow_mut() = shown;
        ui.set_list_count(self.list, count);
        ui.refresh_list(self.list);
        ui.set_selection(self.list, selected.as_slice());
        if following && count > 0 {
            ui.scroll_to_index(self.list, count - 1);
        }
        for ((_, label), count) in self.toolbar.levels.iter().zip(counts) {
            ui.set_text(*label, &count.to_string());
        }
        ui.set_visible(self.toolbar.muted, !self.muted.is_empty());
        ui.set_text(self.toolbar.muted, &format!("Muted {}", self.muted.len()));
        self.rebuild_chips(ui);
    }

    /// A chip for each category, with how many lines it has, built again
    /// when the categories change.
    fn rebuild_chips(&mut self, ui: &mut Ui) {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for category in self.entries.borrow().iter().flat_map(|e| e.categories.iter()) {
            match counts.iter_mut().find(|(name, _)| name == category) {
                Some((_, count)) => *count += 1,
                None => counts.push((category.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| a.0.cmp(&b.0));
        let same = counts.len() == self.chips.len() && counts.iter().zip(&self.chips).all(|((a, _), (b, _))| a == b);
        if !same {
            for (_, chip) in self.chips.drain(..) {
                ui.remove(chip);
            }
            for (name, _) in &counts {
                let chip = ui.panel(self.toolbar.chips, Style::row());
                ui.add_class(chip, "console-chip");
                let dot = ui.panel(chip, Style::default());
                ui.add_class(dot, "chip-dot");
                let [r, g, b] = category_color(name).to_be_bytes()[1..].try_into().unwrap_or([0; 3]);
                ui.set_inline_style(dot, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
                ui.add_label(chip, "");
                self.chips.push((name.clone(), chip));
            }
        }
        for ((name, count), (_, chip)) in counts.iter().zip(&self.chips) {
            let label = ui.children(*chip)[1];
            ui.set_text(label, &format!("{name} {count}"));
            ui.set_checked(*chip, !self.hidden_categories.contains(name));
        }
    }

    /// A line picked in the list becomes the one the trace pane shows; one
    /// about an Obj asks for the Obj to be selected.
    fn follow_selection(&mut self, ui: &Ui, update: &mut ConsoleUpdate) {
        if !ui.selection_changed(self.list) {
            return;
        }
        let picked = ui.selection(self.list).first().and_then(|&at| self.shown.borrow().get(at).map(|s| s.entry));
        if picked == self.selected {
            return;
        }
        self.selected = picked;
        self.unfolded = false;
        update.ping = picked.and_then(|at| self.entries.borrow().get(at).and_then(|e| e.context));
    }

    /// A double click on a line opens where it came from, as in Unity.
    fn open_on_double_click(&self, ui: &Ui, update: &mut ConsoleUpdate) {
        if !ui.double_clicked(self.list) {
            return;
        }
        let entries = self.entries.borrow();
        let source = self.selected.and_then(|at| entries.get(at)).and_then(Entry::source);
        update.open = source.and_then(|frame| self.place(frame));
    }

    /// The trace pane's toggles, Copy, and its links: a frame to open, or
    /// the folded frames to show. Returns whether a setting changed.
    fn detail_clicks(&mut self, ui: &mut Ui, settings: &mut Settings, update: &mut ConsoleUpdate) -> bool {
        let detail = &self.detail;
        let mut changed = false;
        for (button, setting) in [
            (detail.mine_only, &mut settings.console_mine_only),
            (detail.namespaces, &mut settings.console_namespaces),
            (detail.parameters, &mut settings.console_parameters),
        ] {
            if ui.clicked(button) {
                *setting = !*setting;
                changed = true;
            }
        }
        let entries = self.entries.borrow();
        let entry = self.selected.and_then(|at| entries.get(at));
        if let (true, Some(entry)) = (ui.clicked(detail.copy), entry) {
            ui.copy_text(&entry.as_text());
        }
        let link = ui.clicked_link(detail.trace).map(str::to_string);
        match (link.as_deref(), entry) {
            (Some("unfold"), _) => {
                drop(entries);
                self.unfolded = !self.unfolded;
            }
            (Some(link), Some(entry)) => {
                let frame = link
                    .strip_prefix("frame:")
                    .and_then(|at| at.parse::<usize>().ok())
                    .and_then(|at| entry.frames.get(at));
                update.open = frame.and_then(|frame| self.place(frame)).or(update.open.take());
            }
            _ => {}
        }
        changed
    }

    /// A frame's file on disk, and its line: the file as it is when
    /// absolute, or a Rust file from the engine's folder, or else the
    /// project's.
    fn place(&self, frame: &Frame) -> Option<(PathBuf, u32)> {
        let path = Path::new(frame.file.as_deref()?);
        let line = frame.line.unwrap_or(1);
        if path.is_absolute() {
            return Some((path.to_path_buf(), line));
        }
        let engine = self.engine.join(path);
        Some((if engine.exists() { engine } else { self.project.join(path) }, line))
    }

    /// Shows the toggles' states: on, or off.
    fn show_toggles(&self, ui: &mut Ui, settings: &Settings) {
        let (toolbar, detail) = (&self.toolbar, &self.detail);
        ui.set_checked(toolbar.collapse, settings.console_collapse);
        ui.set_checked(toolbar.pause, settings.error_pause);
        ui.set_checked(detail.mine_only, settings.console_mine_only);
        ui.set_checked(detail.namespaces, settings.console_namespaces);
        ui.set_checked(detail.parameters, settings.console_parameters);
        for (slot, (level, _)) in toolbar.levels.iter().enumerate() {
            ui.set_checked(*level, !self.hidden[slot]);
        }
    }

    /// The selected line in the trace pane: when and where it was logged,
    /// its whole message in its level's colour, and its stack, coloured part
    /// by part.
    fn draw_detail(&mut self, ui: &mut Ui, settings: &Settings) {
        let key = self.selected.map(|at| {
            (at, self.unfolded, settings.console_mine_only, settings.console_namespaces, settings.console_parameters)
        });
        if key == self.drawn && key.is_some() {
            return;
        }
        self.drawn = key;
        let entries = self.entries.borrow();
        let Some(entry) = self.selected.and_then(|at| entries.get(at)) else {
            ui.set_text(self.detail.info, "Select a line to see where it came from.");
            ui.set_text(self.detail.message, "");
            ui.set_rich_text(self.detail.trace, &RichText::new());
            return;
        };
        // The level is the message's colour, as in the list, which leaves
        // the bar room for the rest.
        let thread = if entry.worker { "worker thread" } else { "main thread" };
        let info =
            format!("{} · frame {} · {thread} · {}", time(entry.seconds), entry.frame, entry.categories.join(", "));
        ui.set_text(self.detail.info, &info);
        ui.set_text(self.detail.message, &entry.message);
        ui.set_class(self.detail.message, "log-warn", entry.level == Level::WARN);
        ui.set_class(self.detail.message, "log-error", entry.level == Level::ERROR);
        let trace = trace(entry, settings, self.unfolded);
        ui.set_rich_text(self.detail.trace, &trace);
    }
}

/// The toolbar: Clear, Collapse and Error Pause, the search, the levels with
/// their counts, Muted, and the categories' chips, last so they are what
/// wraps; under it, the line that says what is wrong with a search.
fn toolbar(ui: &mut Ui, root: NodeId) -> Toolbar {
    let bar = ui.panel(root, Style::row());
    ui.add_class(bar, "console-toolbar");
    let button = |ui: &mut Ui, text: &str| {
        let button = ui.button(bar, text);
        ui.add_class(button, "console-button");
        button
    };
    let (clear, collapse, pause) = (button(ui, "Clear"), button(ui, "Collapse"), button(ui, "Error Pause"));
    let search_box = ui.panel(bar, Style::row());
    ui.add_class(search_box, "search");
    ui.add_class(search_box, "console-search");
    let icon = ui.icon(search_box, "search");
    ui.add_class(icon, "search-icon");
    let search = ui.text_field(search_box, "");
    ui.add_class(search, "search-field");
    ui.set_placeholder(search, "Search · tag:name · /regex/");
    let levels = [("info", "level-log"), ("alert", "level-warning"), ("alert", "level-error")].map(|(icon, class)| {
        let level = ui.panel(bar, Style::row());
        ui.add_class(level, "console-level");
        ui.add_class(level, class);
        let icon = ui.icon(level, icon);
        ui.add_class(icon, "console-level-icon");
        (level, ui.add_label(level, "0"))
    });
    let muted = button(ui, "Muted");
    ui.set_visible(muted, false);
    let chips = ui.panel(bar, Style::row());
    ui.add_class(chips, "console-chips");
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let problem = ui.add_label(root, "");
    ui.add_class(problem, "console-problem");
    ui.set_visible(problem, false);
    Toolbar { clear, collapse, pause, chips, muted, search_box, search, problem, levels }
}

/// The trace pane: a bar of what the line is and the toggles, then the
/// message and the stack, which scroll.
fn detail(ui: &mut Ui, pane: NodeId, settings: &Settings) -> Detail {
    ui.add_class(pane, "console-detail");
    let line = ui.panel(pane, Style::default());
    ui.add_class(line, "divider");
    let bar = ui.panel(pane, Style::row());
    ui.add_class(bar, "console-detail-bar");
    let info = ui.add_label(bar, "");
    ui.add_class(info, "console-info");
    let button = |ui: &mut Ui, text: &str, on: bool| {
        let button = ui.button(bar, text);
        ui.add_class(button, "console-button");
        ui.set_checked(button, on);
        button
    };
    let mine_only = button(ui, "My code only", settings.console_mine_only);
    let namespaces = button(ui, "Namespaces", settings.console_namespaces);
    let parameters = button(ui, "Parameters", settings.console_parameters);
    let copy = button(ui, "Copy", false);
    let body = ui.panel(pane, Style::column().scroll());
    ui.add_class(body, "fill");
    ui.add_class(body, "console-detail-body");
    let message = ui.add_label(body, "");
    ui.add_class(message, "console-detail-message");
    let trace = ui.rich_label(body, &RichText::new());
    ui.add_class(trace, "console-trace");
    Detail { info, message, trace, mine_only, namespaces, parameters, copy }
}

/// Fills a row: the level's icon, when, the message's first line, how many
/// identical lines it stands for, and where it came from.
fn bind_row(ui: &mut Ui, row: NodeId, entry: &Entry, count: usize) {
    let parts = ui.children(row);
    let warn = entry.level == Level::WARN;
    let error = entry.level == Level::ERROR;
    ui.set_icon(parts[0], if warn || error { "alert" } else { "info" });
    ui.set_class(row, "log-warn", warn);
    ui.set_class(row, "log-error", error);
    ui.set_text(parts[1], &time(entry.seconds));
    ui.set_text(parts[2], entry.message.lines().next().unwrap_or_default());
    ui.set_text(parts[3], &if count > 1 { count.to_string() } else { String::new() });
    ui.set_visible(parts[3], count > 1);
    let source = entry.source().and_then(|f| Some(format!("{}:{}", f.file_name()?, f.line.unwrap_or_default())));
    ui.set_text(parts[4], &source.unwrap_or_default());
}

/// A line's stack as rich text: each frame's parts in their colours, the
/// file and line a link to open it. Frames not the project's own fold into
/// one line to unfold, when asked.
fn trace(entry: &Entry, settings: &Settings, unfolded: bool) -> RichText {
    let mut text = RichText::new();
    if entry.frames.is_empty() {
        text.push_colored("No stack: this line did not say where it came from.", Color::hex(FOLDED));
        return text;
    }
    let fold = settings.console_mine_only && !unfolded && entry.mine.iter().any(|m| *m);
    let mut index = 0;
    while index < entry.frames.len() {
        if fold && !entry.mine[index] {
            let run = entry.mine[index..].iter().take_while(|m| !**m).count();
            let frames = if run == 1 { "frame".to_string() } else { format!("{run} frames") };
            text.push_link_colored(&format!("… {frames} not the project's own\n"), "unfold", Color::hex(FOLDED));
            index += run;
            continue;
        }
        push_frame(&mut text, entry, index, settings);
        index += 1;
    }
    text
}

/// One frame: `Namespace.Class.Method(parameters)  File.cs:12`.
fn push_frame(text: &mut RichText, entry: &Entry, index: usize, settings: &Settings) {
    let frame = &entry.frames[index];
    if frame.has_call() {
        if settings.console_namespaces && !frame.namespace.is_empty() {
            text.push_colored(&format!("{}.", frame.namespace), Color::hex(NAMESPACE));
        }
        if !frame.class.is_empty() {
            text.push_colored(&format!("{}.", frame.class), Color::hex(CLASS));
        }
        text.push_colored(&frame.method, Color::hex(METHOD));
        let parameters = if settings.console_parameters { frame.parameters.as_str() } else { "" };
        text.push_colored(&format!("({parameters})"), Color::hex(PARAMETERS));
    } else if frame.file.is_none() {
        text.push_colored(&frame.raw, Color::hex(FOLDED));
    }
    if let (Some(name), Some(line)) = (frame.file_name(), frame.line) {
        let gap = if frame.has_call() { "  " } else { "" };
        text.push(gap, Default::default());
        text.push_link_colored(&format!("{name}:{line}"), &format!("frame:{index}"), Color::hex(FILE));
    }
    text.push("\n", Default::default());
}

/// Which of the toolbar's three levels a line counts under.
fn level_slot(level: Level) -> usize {
    match level {
        Level::ERROR => 2,
        Level::WARN => 1,
        _ => 0,
    }
}

/// Minutes and seconds since the editor started: `01:12.034`.
fn time(seconds: f64) -> String {
    let minutes = (seconds / 60.0).floor();
    format!("{:02}:{:06.3}", minutes as u64, seconds - minutes * 60.0)
}

/// A category's colour, the same every time for the same name.
fn category_color(name: &str) -> u32 {
    let hash = name.bytes().fold(0usize, |hash, b| hash.wrapping_mul(31).wrapping_add(b as usize));
    CATEGORY_COLORS[hash % CATEGORY_COLORS.len()]
}
