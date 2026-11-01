//! The Console, after CookieJarTools' Sifter (docs/ARCHITECTURE.md): a log
//! worth reading.
//!
//! - **One line per row**, in columns: when (minutes and seconds since the
//!   editor started), the message, and where it came from. Monospaced
//!   throughout, as nearly all of it is code; warnings and errors take their
//!   own colour.
//! - **Categories nobody declared** (see `entry`), in the toolbar's
//!   Categories menu with how many lines each has; choosing one hides or
//!   shows it, and the menu stays open for the next.
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
//!   hidden until the toolbar's Muted button brings them back. It also
//!   colours the line's category, a colour kept with the project in
//!   `Settings/Console.toml`, since a team should agree on them; each line
//!   shows its category in it.
//! - **Filters** saves the search, levels and categories hidden under a
//!   name, to pick again, kept per user.
//! - **Player Log** opens the log a built game kept the last time it ran
//!   (or the time before), each line coming in at its level, in the Player
//!   category, as Sifter reads a player's log.
//! - **Hide…** in the trace pane folds away the frames of chosen assemblies
//!   (a frame's root namespace, such as `Voxear`, or an engine crate), for
//!   the one library that fills every trace.
//! - **Settings**, at the end of the toolbar, opens the Console's settings
//!   in its place, with a sample drawn as the Console draws its lines (see
//!   `settings_view`): row height, colour by kind, alternating rows, how a
//!   category shows, the stack's parts, colours, how many lines are held,
//!   and the category colours.

mod entry;
mod frame;
mod query;
mod settings_view;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};

use tracing::Level;
use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{build_profile, player};
use voxear_ui::{Color, ItemFns, MenuItem, NodeId, RichText, Style, Ui};

use self::{
    entry::Entry,
    frame::Frame,
    query::Query,
    settings_view::{Change, PARTS, SettingsView},
};
use crate::settings::{SavedFilter, Settings};

/// The list's share of the Console over the trace pane.
const LIST_SHARE: f32 = 0.6;
/// Each part of a frame's colour, tuned for the dark theme.
const NAMESPACE: u32 = 0x7d8597;
const CLASS: u32 = 0x7cc4fa;
const METHOD: u32 = 0xe8c37e;
const PARAMETERS: u32 = 0x6b7385;
const FILE: u32 = 0xc3a6ff;
const FOLDED: u32 = 0x8b93a7;
/// Where a project keeps its category colours.
const COLOURS_FILE: &str = "Settings/Console.toml";
/// The colours a category can take, by name.
const PALETTE: [(&str, u32); 9] = [
    ("Red", 0xf26d6d),
    ("Orange", 0xf0a35e),
    ("Yellow", 0xe2c94a),
    ("Green", 0x6fcf7f),
    ("Teal", 0x4fc8c0),
    ("Blue", 0x6aa8ff),
    ("Purple", 0xb48cff),
    ("Pink", 0xf27ac0),
    ("Grey", 0x9aa1ad),
];

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
    categories: NodeId,
    filters: NodeId,
    player_log: NodeId,
    /// Where a filter being saved is named.
    filter_name: NodeId,
    muted: NodeId,
    settings: NodeId,
    search_box: NodeId,
    search: NodeId,
    problem: NodeId,
    /// Log, Warning and Error, each with its count's label.
    levels: [(NodeId, NodeId); 3],
}

/// How the settings ask lines to look, which every row is bound with.
#[derive(Clone, Debug, Default)]
struct Look {
    tint: bool,
    stripes: bool,
    /// 0 no category, 1 a dot in its colour, 2 its name.
    category: usize,
    error: Option<u32>,
    warning: Option<u32>,
}

impl Look {
    fn of(settings: &Settings) -> Self {
        Self {
            tint: settings.console_tint,
            stripes: settings.console_stripes,
            category: settings.console_category,
            error: settings.console_colours.get("error").copied(),
            warning: settings.console_colours.get("warning").copied(),
        }
    }
}

/// The pane under the list: the selected line's details, message and stack.
struct Detail {
    info: NodeId,
    message: NodeId,
    trace: NodeId,
    mine_only: NodeId,
    namespaces: NodeId,
    parameters: NodeId,
    hide: NodeId,
    copy: NodeId,
}

pub struct Console {
    list: NodeId,
    /// The list and trace pane, which the settings take the place of.
    split: NodeId,
    root: NodeId,
    look: Rc<RefCell<Look>>,
    row_height: f32,
    settings_view: Option<SettingsView>,
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
    /// Each category with how many lines it has, by name, and where its
    /// menu opens.
    categories: Vec<(String, usize)>,
    categories_at: (f32, f32),
    selected: Option<usize>,
    /// The selected line's frames that are not the project's own are shown.
    unfolded: bool,
    /// What the toggles showed last, to build the stack again when they change.
    drawn: Option<(usize, bool, bool, bool, bool)>,
    /// A filter is being named, and its field is to take focus.
    naming: bool,
    focus_name: bool,
    /// The assemblies the Hide menu lists, in its order, and the logs the
    /// Player Log menu does.
    hide_menu: Vec<String>,
    log_menu: Vec<(String, PathBuf)>,
    /// Each category's colour, kept with the project, and the category whose
    /// colour is being picked from a line's menu.
    colours: Rc<RefCell<HashMap<String, u32>>>,
    colouring: Option<String>,
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
        let colours = Rc::new(RefCell::new(read_colours(project)));
        let look = Rc::new(RefCell::new(Look::of(settings)));
        let (read, rows, painted, looks) = (entries.clone(), shown.clone(), colours.clone(), look.clone());
        let items = ItemFns {
            make: make_row,
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let (entries, rows) = (read.borrow(), rows.borrow());
                let Some(shown) = rows.get(index) else { return };
                let Some(entry) = entries.get(shown.entry) else { return };
                bind_row(ui, row, (entry, shown.count, index), &painted.borrow(), &looks.borrow());
            },
        };
        let list = ui.list_view(top, settings.console_row_height, 0, items);
        ui.add_class(list, "fill");
        ui.add_class(list, "console");
        let detail = detail(ui, bottom, settings);
        let engine = voxear_engine::engine_root();
        // Stacks name files by their real paths, so the project's folder is
        // made real too, or a project opened as `../Game` owns none of them.
        let project = project.canonicalize().unwrap_or_else(|_| project.to_path_buf());
        let console = Self {
            list,
            split,
            root,
            look,
            row_height: settings.console_row_height,
            settings_view: None,
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
            categories: Vec::new(),
            categories_at: (0.0, 0.0),
            selected: None,
            unfolded: false,
            drawn: None,
            naming: false,
            focus_name: false,
            hide_menu: Vec::new(),
            log_menu: Vec::new(),
            colours,
            colouring: None,
        };
        console.show_toggles(ui, settings);
        console
    }

    /// Takes in lines logged since the last frame and follows the toolbar,
    /// the list, its menu and the trace pane.
    pub fn update(&mut self, ui: &mut Ui, settings: &mut Settings) -> ConsoleUpdate {
        let mut update =
            ConsoleUpdate { errors: self.take_new_lines(settings.console_kept), ..ConsoleUpdate::default() };
        if self.settings_view.is_some() {
            update.settings_changed = self.update_settings(ui, settings);
            return update;
        }
        let mut changed = update.errors > 0 || self.new_lines_arrived;
        self.new_lines_arrived = false;
        if ui.clicked(self.toolbar.settings) {
            self.open_settings(ui, settings);
            return update;
        }
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
        self.count_categories(ui);
    }

    /// Reads the lines logged since it last looked, holding no more than
    /// `kept`. Returns how many were errors.
    fn take_new_lines(&mut self, kept: usize) -> usize {
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
        let over = entries.len().saturating_sub(kept);
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
        if ui.clicked(self.toolbar.categories) {
            let [x, y, _, height] = ui.rect(self.toolbar.categories);
            self.categories_at = (x, y + height + 4.0);
            self.open_categories(ui);
        }
        refilter |= self.category_choice(ui);
        if ui.clicked(self.toolbar.filters) {
            self.open_filters(ui, settings);
        }
        refilter |= self.filter_choice(ui, settings, update);
        if ui.clicked(self.toolbar.player_log) {
            self.open_player_logs(ui);
        }
        if let Some((name, path)) = ui.menu_choice(self.toolbar.player_log).and_then(|i| self.log_menu.get(i).cloned())
        {
            read_player_log(&name, &path);
        }
        self.name_filter(ui, settings, update);
        if ui.clicked(self.toolbar.muted) {
            self.muted.clear();
            refilter = true;
        }
        if ui.changed(self.toolbar.search) {
            self.read_search(ui);
            refilter = true;
        }
        refilter
    }

    /// Reads the search box into the query, saying what is wrong with it.
    fn read_search(&mut self, ui: &mut Ui) {
        self.query = Query::parse(&ui.text(self.toolbar.search));
        let problem = self.query.error.clone().unwrap_or_default();
        ui.set_text(self.toolbar.problem, &problem);
        ui.set_visible(self.toolbar.problem, !problem.is_empty());
        ui.set_class(self.toolbar.search_box, "bad", !problem.is_empty());
    }

    /// The search, levels and categories as they are now, as a filter to
    /// save or compare.
    fn current_filter(&self, ui: &Ui) -> SavedFilter {
        let mut hidden_categories: Vec<String> = self.hidden_categories.iter().cloned().collect();
        hidden_categories.sort();
        SavedFilter {
            name: String::new(),
            search: ui.text(self.toolbar.search),
            hidden_levels: self.hidden,
            hidden_categories,
        }
    }

    /// The Filters menu: each saved filter, ticked while it is the one in
    /// use, then Save this filter and Forget.
    fn open_filters(&self, ui: &mut Ui, settings: &Settings) {
        let now = self.current_filter(ui);
        let mut items: Vec<MenuItem> =
            settings.console_filters.iter().map(|f| MenuItem::new(&f.name).checked(same_filter(f, &now))).collect();
        if items.is_empty() {
            items.push(MenuItem::new("No saved filters yet").enabled(false));
        }
        items.push(MenuItem::separator());
        items.push(MenuItem::new("Save this filter…"));
        let active = settings.console_filters.iter().find(|f| same_filter(f, &now));
        let forget = active.map_or("Forget this filter".to_string(), |f| format!("Forget “{}”", f.name));
        items.push(MenuItem::new(&forget).enabled(active.is_some()));
        let [x, y, _, height] = ui.rect(self.toolbar.filters);
        ui.open_menu(self.toolbar.filters, (x, y + height + 4.0), &items);
    }

    /// A choice from the Filters menu: a filter to use, or to save the one
    /// in use, or to forget it. Returns whether the list needs filtering again.
    fn filter_choice(&mut self, ui: &mut Ui, settings: &mut Settings, update: &mut ConsoleUpdate) -> bool {
        let Some(choice) = ui.menu_choice(self.toolbar.filters) else { return false };
        let saved = settings.console_filters.len().max(1);
        if let Some(filter) = settings.console_filters.get(choice).filter(|_| choice < saved).cloned() {
            ui.set_text(self.toolbar.search, &filter.search);
            self.read_search(ui);
            self.hidden = filter.hidden_levels;
            self.hidden_categories = filter.hidden_categories.into_iter().collect();
            self.count_categories(ui);
            return true;
        }
        if choice == saved + 1 {
            self.naming = true;
            self.focus_name = true;
            ui.set_text(self.toolbar.filter_name, "");
            ui.set_visible(self.toolbar.filter_name, true);
        } else if choice == saved + 2 {
            let now = self.current_filter(ui);
            settings.console_filters.retain(|f| !same_filter(f, &now));
            update.settings_changed = true;
        }
        false
    }

    /// The field naming a filter being saved: Enter saves it, replacing one
    /// of the same name, and leaving it, or Escape, puts it away.
    fn name_filter(&mut self, ui: &mut Ui, settings: &mut Settings, update: &mut ConsoleUpdate) {
        if !self.naming {
            return;
        }
        let field = self.toolbar.filter_name;
        if self.focus_name {
            ui.focus(field);
            self.focus_name = ui.focused() != Some(field);
            return;
        }
        let name = ui.text(field).trim().to_string();
        if ui.submitted(field) && !name.is_empty() {
            let filter = SavedFilter { name: name.clone(), ..self.current_filter(ui) };
            match settings.console_filters.iter_mut().find(|f| f.name == name) {
                Some(kept) => *kept = filter,
                None => settings.console_filters.push(filter),
            }
            update.settings_changed = true;
            ui.blur();
        }
        if ui.focused() != Some(field) {
            self.naming = false;
            ui.set_visible(field, false);
        }
    }

    /// A context click on a line: Copy, or Mute this message. Returns
    /// whether the list needs filtering again.
    fn menu(&mut self, ui: &mut Ui) -> bool {
        if let Some(choice) = ui.menu_choice(self.list) {
            if let Some(category) = self.colouring.take() {
                self.set_colour(ui, &category, PALETTE.get(choice).map(|(_, colour)| *colour));
                return false;
            }
            let entries = self.entries.borrow();
            let Some(entry) = self.selected.and_then(|at| entries.get(at)) else { return false };
            match choice {
                0 => ui.copy_text(&entry.as_text()),
                1 => {
                    self.muted.insert(entry.message.clone());
                    return true;
                }
                _ => {
                    // Its category's colour, from a second menu where the first was.
                    let category = entry.categories.first().cloned();
                    drop(entries);
                    if let Some(category) = category {
                        let mut items: Vec<MenuItem> = PALETTE.iter().map(|(name, _)| MenuItem::new(name)).collect();
                        items.extend([MenuItem::separator(), MenuItem::new("No colour")]);
                        let at = ui.pointer_position().unwrap_or_default();
                        ui.open_menu(self.list, at, &items);
                        self.colouring = Some(category);
                    }
                }
            }
        }
        if ui.context_clicked(self.list) && !ui.selection(self.list).is_empty() {
            let at = ui.pointer_position().unwrap_or_default();
            let entries = self.entries.borrow();
            let category = self.selected.and_then(|at| entries.get(at)).and_then(|e| e.categories.first().cloned());
            let mut items = vec![MenuItem::new("Copy"), MenuItem::new("Mute this message")];
            if let Some(category) = category {
                items.push(MenuItem::new(&format!("Colour “{category}”…")));
            }
            drop(entries);
            self.colouring = None;
            ui.open_menu(self.list, at, &items);
        }
        false
    }

    /// The Player Log menu: each of the project's builds, its log from the
    /// last run and the one before, where the game has kept them.
    fn open_player_logs(&mut self, ui: &mut Ui) {
        self.log_menu = player_logs(&self.project, player::log_path);
        let mut items: Vec<MenuItem> = self.log_menu.iter().map(|(label, _)| MenuItem::new(label)).collect();
        if items.is_empty() {
            items.push(MenuItem::new("No build has run yet").enabled(false));
        }
        let [x, y, _, height] = ui.rect(self.toolbar.player_log);
        ui.open_menu(self.toolbar.player_log, (x, y + height + 4.0), &items);
    }

    /// Gives a category a colour, or takes it away, and keeps it with the
    /// project.
    fn set_colour(&mut self, ui: &mut Ui, category: &str, colour: Option<u32>) {
        let mut colours = self.colours.borrow_mut();
        match colour {
            Some(colour) => colours.insert(category.to_string(), colour),
            None => colours.remove(category),
        };
        if let Err(error) = write_colours(&self.project, &colours) {
            tracing::warn!("could not keep the Console's colours in {COLOURS_FILE}: {error:#}");
        }
        drop(colours);
        ui.refresh_list(self.list);
    }

    /// Shows the settings in the Console's place, with the categories seen
    /// so far to colour.
    fn open_settings(&mut self, ui: &mut Ui, settings: &Settings) {
        let colours = self.category_colours();
        let view = SettingsView::new(ui, self.root, &self.project, settings, &colours);
        ui.set_visible(self.split, false);
        self.settings_view = Some(view);
        self.show_settings(ui, settings);
    }

    /// Every category seen, and those given a colour, with their colours.
    fn category_colours(&self) -> Vec<(String, Option<u32>)> {
        let colours = self.colours.borrow();
        let mut names: Vec<&String> = self.categories.iter().map(|(name, _)| name).chain(colours.keys()).collect();
        names.sort();
        names.dedup();
        names.into_iter().map(|name| (name.clone(), colours.get(name).copied())).collect()
    }

    /// The settings screen: what was picked is applied to the Console at
    /// once, and the sample drawn again. Returns whether a setting changed.
    fn update_settings(&mut self, ui: &mut Ui, settings: &mut Settings) -> bool {
        let Some(change) = self.settings_view.as_ref().and_then(|view| view.update(ui, settings)) else {
            return false;
        };
        let settings_changed = matches!(change, Change::Settings);
        match change {
            Change::Done => {
                if let Some(view) = self.settings_view.take() {
                    ui.remove(view.screen);
                }
                ui.set_visible(self.split, true);
                return false;
            }
            Change::Category(name, colour) => self.set_colour(ui, &name, colour),
            Change::ClearCategories => {
                self.colours.borrow_mut().clear();
                if let Err(error) = write_colours(&self.project, &self.colours.borrow()) {
                    tracing::warn!("could not keep the Console's colours in {COLOURS_FILE}: {error:#}");
                }
            }
            Change::Settings => {}
        }
        self.apply_settings(ui, settings);
        self.show_settings(ui, settings);
        settings_changed
    }

    /// Draws every line again as the settings now ask.
    fn apply_settings(&mut self, ui: &mut Ui, settings: &Settings) {
        *self.look.borrow_mut() = Look::of(settings);
        if self.row_height != settings.console_row_height {
            self.row_height = settings.console_row_height;
            ui.set_item_height(self.list, self.row_height);
        }
        self.drawn = None;
        self.refresh(ui, settings);
        self.show_toggles(ui, settings);
    }

    /// The settings screen as the settings are now, and its sample.
    fn show_settings(&self, ui: &mut Ui, settings: &Settings) {
        let Some(view) = &self.settings_view else { return };
        view.show(ui, settings, &self.category_colours());
        let (colours, look) = (self.colours.borrow(), self.look.borrow());
        for (index, (&row, entry)) in view.sample_rows.iter().zip(&view.samples).enumerate() {
            ui.set_inline_style(row, &format!("height: {}px", settings.console_row_height));
            bind_row(ui, row, (entry, 1, index), &colours, &look);
        }
        let traced = view.samples.first().map(|entry| trace(entry, settings, false)).unwrap_or_default();
        ui.set_rich_text(view.sample_trace, &traced);
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
        let row = self.row_height;
        let following = scrolled + height >= before as f32 * row - row;
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
        self.count_categories(ui);
    }

    /// Counts each category's lines, for its menu, and says on the
    /// Categories button how many are hidden.
    fn count_categories(&mut self, ui: &mut Ui) {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for category in self.entries.borrow().iter().flat_map(|e| e.categories.iter()) {
            match counts.iter_mut().find(|(name, _)| name == category) {
                Some((_, count)) => *count += 1,
                None => counts.push((category.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| a.0.cmp(&b.0));
        self.categories = counts;
        let hidden = self.categories.iter().filter(|(name, _)| self.hidden_categories.contains(name)).count();
        let label = if hidden == 0 { "Categories".to_string() } else { format!("Categories · {hidden} hidden") };
        ui.set_text(self.toolbar.categories, &label);
        ui.set_checked(self.toolbar.categories, hidden > 0);
    }

    /// The Categories menu: Show all categories, then each category, ticked while
    /// shown, with how many lines it has.
    fn open_categories(&self, ui: &mut Ui) {
        let mut items = vec![
            MenuItem::new("Show all categories").enabled(!self.hidden_categories.is_empty()),
            MenuItem::separator(),
        ];
        for (name, count) in &self.categories {
            let shown = !self.hidden_categories.contains(name);
            items.push(MenuItem::new(name).checked(shown).shortcut(&count.to_string()));
        }
        if self.categories.is_empty() {
            items.push(MenuItem::new("No categories yet").enabled(false));
        }
        ui.open_menu(self.toolbar.categories, self.categories_at, &items);
    }

    /// A category chosen in its menu is shown or hidden, and the menu opens
    /// again where it was, so several can be changed in a row. Returns whether
    /// the list needs filtering again.
    fn category_choice(&mut self, ui: &mut Ui) -> bool {
        let Some(choice) = ui.menu_choice(self.toolbar.categories) else { return false };
        match choice.checked_sub(2).and_then(|at| self.categories.get(at)) {
            Some((name, _)) if !self.hidden_categories.remove(name) => {
                self.hidden_categories.insert(name.clone());
            }
            Some(_) => {}
            None => self.hidden_categories.clear(),
        }
        self.count_categories(ui);
        self.open_categories(ui);
        true
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
        if let Some(choice) = ui.menu_choice(detail.hide)
            && let Some(assembly) = self.hide_menu.get(choice).cloned()
        {
            let hidden = &mut settings.console_hidden_assemblies;
            match hidden.iter().position(|h| *h == assembly) {
                Some(at) => drop(hidden.remove(at)),
                None => hidden.push(assembly),
            }
            changed = true;
            self.drawn = None;
            self.open_hide(ui, settings);
        } else if ui.clicked(self.detail.hide) {
            self.open_hide(ui, settings);
        }
        let detail = &self.detail;
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

    /// The Hide menu: the assemblies in the selected line's stack, ticked
    /// while their frames are hidden.
    fn open_hide(&mut self, ui: &mut Ui, settings: &Settings) {
        let entries = self.entries.borrow();
        let mut assemblies: Vec<String> = Vec::new();
        for frame in self.selected.and_then(|at| entries.get(at)).iter().flat_map(|e| e.frames.iter()) {
            if let Some(assembly) = frame.assembly().filter(|a| !assemblies.iter().any(|known| known == a)) {
                assemblies.push(assembly.to_string());
            }
        }
        drop(entries);
        // Hidden ones not in this stack still show, so they can be let go of.
        for hidden in &settings.console_hidden_assemblies {
            if !assemblies.contains(hidden) {
                assemblies.push(hidden.clone());
            }
        }
        let mut items: Vec<MenuItem> = assemblies
            .iter()
            .map(|a| MenuItem::new(a).checked(settings.console_hidden_assemblies.contains(a)))
            .collect();
        if items.is_empty() {
            items.push(MenuItem::new("No assemblies in this stack").enabled(false));
        }
        self.hide_menu = assemblies;
        let [x, y, _, height] = ui.rect(self.detail.hide);
        ui.open_menu(self.detail.hide, (x, y + height + 4.0), &items);
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
/// their counts, the Categories menu and Muted; under it, the line that says
/// what is wrong with a search.
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
    let categories = button(ui, "Categories");
    let filters = button(ui, "Filters");
    let player_log = button(ui, "Player Log");
    let filter_name = ui.text_field(bar, "");
    ui.add_class(filter_name, "console-filter-name");
    ui.set_placeholder(filter_name, "Name this filter, then Enter");
    ui.set_visible(filter_name, false);
    let muted = button(ui, "Muted");
    ui.set_visible(muted, false);
    let settings = ui.icon_button(bar, "settings");
    ui.add_class(settings, "console-settings-button");
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let problem = ui.add_label(root, "");
    ui.add_class(problem, "console-problem");
    ui.set_visible(problem, false);
    Toolbar {
        clear,
        collapse,
        pause,
        categories,
        filters,
        player_log,
        filter_name,
        muted,
        settings,
        search_box,
        search,
        problem,
        levels,
    }
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
    let hide = button(ui, "Hide libraries…", false);
    let copy = button(ui, "Copy", false);
    let body = ui.panel(pane, Style::column().scroll());
    ui.add_class(body, "fill");
    ui.add_class(body, "console-detail-body");
    let message = ui.add_label(body, "");
    ui.add_class(message, "console-detail-message");
    let trace = ui.rich_label(body, &RichText::new());
    ui.add_class(trace, "console-trace");
    Detail { info, message, trace, mine_only, namespaces, parameters, hide, copy }
}

/// A row's parts, which [`bind_row`] fills: the level's icon, when, the
/// category, the message, how many lines it stands for, and where from.
fn make_row(ui: &mut Ui, row: NodeId) {
    let icon = ui.icon(row, "info");
    ui.add_class(icon, "console-icon");
    for class in ["console-time", "console-category", "console-message", "console-count", "console-source"] {
        let label = ui.add_label(row, "");
        ui.add_class(label, class);
    }
}

/// Fills a row with a line, the `count` identical ones it stands for, and
/// its `index` in the list: the level's icon, when, its category in its
/// colour, the message's first line, how many, and where it came from.
fn bind_row(
    ui: &mut Ui,
    row: NodeId,
    (entry, count, index): (&Entry, usize, usize),
    colours: &HashMap<String, u32>,
    look: &Look,
) {
    let parts = ui.children(row);
    let warn = entry.level == Level::WARN;
    let error = entry.level == Level::ERROR;
    ui.set_icon(parts[0], if warn || error { "alert" } else { "info" });
    ui.set_class(parts[0], "warn", warn);
    ui.set_class(parts[0], "error", error);
    ui.set_class(row, "log-warn", warn && look.tint);
    ui.set_class(row, "log-error", error && look.tint);
    ui.set_class(row, "odd", look.stripes && index % 2 == 1);
    // A colour picked for the level takes the theme's place, on the mark and, tinted, the line.
    let picked = if error {
        look.error
    } else if warn {
        look.warning
    } else {
        None
    };
    let painted = picked.map(|c| format!("color: #{c:06x}")).unwrap_or_default();
    ui.set_inline_style(parts[0], &painted);
    ui.set_inline_style(parts[3], if look.tint { &painted } else { "" });
    ui.set_text(parts[1], &time(entry.seconds));
    let category = entry.categories.first().map_or("", String::as_str);
    ui.set_text(parts[2], if look.category == 1 && !category.is_empty() { "●" } else { category });
    ui.set_visible(parts[2], look.category > 0);
    ui.set_class(parts[2], "dot", look.category == 1);
    let colour = colours.get(category).map(|c| format!("color: #{c:06x}")).unwrap_or_default();
    ui.set_inline_style(parts[2], &colour);
    ui.set_text(parts[3], entry.message.lines().next().unwrap_or_default());
    ui.set_text(parts[4], &if count > 1 { count.to_string() } else { String::new() });
    ui.set_visible(parts[4], count > 1);
    let source = entry.source().and_then(|f| Some(format!("{}:{}", f.file_name()?, f.line.unwrap_or_default())));
    ui.set_text(parts[5], &source.unwrap_or_default());
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
    let fold_mine = settings.console_mine_only && !unfolded && entry.mine.iter().any(|m| *m);
    let hidden = |at: usize| {
        let assembly = entry.frames[at].assembly();
        !unfolded && assembly.is_some_and(|a| settings.console_hidden_assemblies.iter().any(|h| h == a))
    };
    let folded = |at: usize| (fold_mine && !entry.mine[at]) || hidden(at);
    let mut index = 0;
    while index < entry.frames.len() {
        if folded(index) {
            let run = (index..entry.frames.len()).take_while(|&at| folded(at)).count();
            let frames = if run == 1 { "frame".to_string() } else { format!("{run} frames") };
            let why = if (index..index + run).all(hidden) { "from hidden assemblies" } else { "not the project's own" };
            text.push_link_colored(&format!("… {frames} {why}\n"), "unfold", Color::hex(FOLDED));
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
    let colour = |part: &str| Color::hex(part_colour(settings, part));
    if frame.has_call() {
        if settings.console_namespaces && !frame.namespace.is_empty() {
            text.push_colored(&format!("{}.", frame.namespace), colour("namespace"));
        }
        if !frame.class.is_empty() {
            text.push_colored(&format!("{}.", frame.class), colour("class"));
        }
        text.push_colored(&frame.method, colour("method"));
        let parameters = if settings.console_parameters { frame.parameters.as_str() } else { "" };
        text.push_colored(&format!("({parameters})"), colour("parameters"));
    } else if frame.file.is_none() {
        text.push_colored(&frame.raw, Color::hex(FOLDED));
    }
    if let (Some(name), Some(line)) = (frame.file_name(), frame.line) {
        let gap = if frame.has_call() { "  " } else { "" };
        text.push(gap, Default::default());
        text.push_link_colored(&format!("{name}:{line}"), &format!("frame:{index}"), colour("file"));
    }
    text.push("\n", Default::default());
}

/// A frame part's colour: the one picked in the settings, or its own.
fn part_colour(settings: &Settings, part: &str) -> u32 {
    let own = PARTS.iter().find(|(key, ..)| *key == part).and_then(|(_, _, own)| *own).unwrap_or(FOLDED);
    settings.console_colours.get(part).copied().unwrap_or(own)
}

/// The logs the project's builds have kept: each profile's build that has
/// run, its last run's log and the one before, as labels and files.
/// `log_of` finds a game's log by its company and name.
fn player_logs(project: &Path, log_of: impl Fn(Option<&str>, &str) -> Option<PathBuf>) -> Vec<(String, PathBuf)> {
    let mut logs = Vec::new();
    for profile in build_profile::profile_names(project) {
        let Ok(settings) = build_profile::load_profile(project, &profile) else { continue };
        let data = settings.output_dir(project, &profile).join(player::DATA_FOLDER);
        let Ok(built) = player::PlayerSettings::read(&data) else { continue };
        let Some(log) = log_of(built.company_name.as_deref(), &built.product_name) else { continue };
        let before = log.with_file_name(player::PREVIOUS_LOG);
        for (label, path) in [(profile.clone(), log), (format!("{profile}, the run before"), before)] {
            if path.is_file() {
                logs.push((label, path));
            }
        }
    }
    logs
}

/// Reads a built game's log into the editor's, each line at its level in
/// the Player category, after a line saying whose and where it is.
fn read_player_log(name: &str, path: &Path) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            tracing::info!(target: "player", "The player's log of {name}, from {}", path.display());
            text.lines().for_each(|line| crate::build_window::log_player_line(line, false));
        }
        Err(error) => tracing::warn!(target: "player", "could not read {}: {error}", path.display()),
    }
}

/// Whether two filters show the same: their search, levels and categories.
fn same_filter(a: &SavedFilter, b: &SavedFilter) -> bool {
    let sorted = |f: &SavedFilter| {
        let mut categories = f.hidden_categories.clone();
        categories.sort();
        categories
    };
    a.search == b.search && a.hidden_levels == b.hidden_levels && sorted(a) == sorted(b)
}

/// The project's category colours, from `Settings/Console.toml`: a table of
/// category names and `#rrggbb` colours.
fn read_colours(project: &Path) -> HashMap<String, u32> {
    let text = std::fs::read_to_string(project.join(COLOURS_FILE)).unwrap_or_default();
    let table: toml::Table = toml::from_str(&text).unwrap_or_default();
    let colours = table.get("colours").and_then(|c| c.as_table()).cloned().unwrap_or_default();
    colours
        .into_iter()
        .filter_map(|(name, colour)| {
            let hex = colour.as_str()?.strip_prefix('#')?;
            Some((name, u32::from_str_radix(hex, 16).ok()?))
        })
        .collect()
}

fn write_colours(project: &Path, colours: &HashMap<String, u32>) -> anyhow::Result<()> {
    let mut names: Vec<&String> = colours.keys().collect();
    names.sort();
    let mut text = String::from(
        "# The Console's category colours, kept with the project so the whole team sees the same.\n[colours]\n",
    );
    for name in names {
        text.push_str(&format!("{} = \"#{:06x}\"\n", toml::Value::String(name.clone()), colours[name]));
    }
    let path = project.join(COLOURS_FILE);
    std::fs::create_dir_all(path.parent().unwrap_or(project))?;
    std::fs::write(&path, text)?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_log_menu_lists_the_builds_that_ran() {
        let project = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        build_profile::write_default_profiles(project.path()).unwrap();
        let data = project.path().join("Builds/Linux Release").join(player::DATA_FOLDER);
        std::fs::create_dir_all(&data).unwrap();
        let built = "format = 1\nproduct_name = \"Orbit\"\nstart_level = \"Levels/Main.level\"\ndevelopment = false\n\
                     validation_layers = false\nfullscreen = false\nwidth = 1600\nheight = 900\nengine = \"0.1.0\"\n\
                     profile = \"Linux Release\"\n";
        std::fs::write(data.join(player::PLAYER_FILE), built).unwrap();
        let log_of = |_: Option<&str>, product: &str| Some(logs.path().join(product).join(player::PLAYER_LOG));
        assert!(player_logs(project.path(), log_of).is_empty(), "built, but not run yet");
        std::fs::create_dir_all(logs.path().join("Orbit")).unwrap();
        std::fs::write(logs.path().join("Orbit").join(player::PLAYER_LOG), "a line").unwrap();
        std::fs::write(logs.path().join("Orbit").join(player::PREVIOUS_LOG), "an older line").unwrap();
        let labels: Vec<String> = player_logs(project.path(), log_of).into_iter().map(|(label, _)| label).collect();
        assert_eq!(labels, ["Linux Release", "Linux Release, the run before"]);
    }
}
