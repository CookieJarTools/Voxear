//! The Package Manager window, after Unity's: the project's packages on the
//! left, each with its version, where it comes from, what it holds and any
//! update, and the picked one's details on the right, with its description,
//! authors, docs pages, samples to import and README, and Update and Remove.
//!
//! The toolbar's + adds a package from a Git URL, a folder or the registry,
//! and Update All moves every package to the newest commit or release that
//! fits. Every change is worked out first as a [`Plan`](voxear_engine::packages::Plan)
//! (see `package_list`). A package from somewhere other than a folder of
//! the developer's own runs code once its scripts build with the project's,
//! so when a change brings one with scripts, a dialog lists what arrives,
//! goes and moves, and nothing is written until Add.
//!
//! Git can take seconds, so the window's work runs on a thread of its own,
//! one task after another, with a progress line under the list and any
//! error above it. The editor keeps working meanwhile; a change written
//! tells the editor (see [`PackagesEvent`]), which builds the scripts again
//! and looks at the project's files again.

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Instant,
};

use voxear_docs::{
    markdown,
    view::{self, Lookups, Picture},
};
use voxear_engine::packages::{Add, Cache, Update, contents::Sample};
use voxear_ui::{ItemFns, MenuItem, NodeId, Style, Ui};

use crate::package_list::{self, Asked, Available, Found, Planned, Reply, RevKind, Row, Task};

const ROW: f32 = 40.0;
const FOUND_ROW: f32 = 40.0;
/// The list's width as the window opens; the line beside it drags.
const LIST_WIDTH: f32 = 320.0;
/// How long the sliding bar takes to cross while the length of the work is
/// not known.
const SWEEP_SECONDS: f32 = 1.4;

/// What the window asks of the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackagesEvent {
    /// The project's packages changed: its scripts build again and its files
    /// are looked at again.
    Changed(String),
    /// A sample was copied into the project, at this project path.
    Imported(String),
}

/// The + menu's items, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddFrom {
    Git,
    Folder,
    Registry,
}

const ADD_FROM: [(AddFrom, &str); 3] = [
    (AddFrom::Git, "Add from Git URL…"),
    (AddFrom::Folder, "Add from Folder…"),
    (AddFrom::Registry, "Add from Registry…"),
];

/// A list row as it is drawn.
#[derive(Clone, Default)]
struct Shown {
    title: String,
    version: String,
    update: Option<String>,
    line: String,
}

/// The Git URL form, under the + button.
struct GitForm {
    panel: NodeId,
    url: NodeId,
    rev: NodeId,
    value: NodeId,
    path: NodeId,
    add: NodeId,
    cancel: NodeId,
}

/// The registry's search, under the + button.
struct RegistryForm {
    panel: NodeId,
    search: NodeId,
    list: NodeId,
    state: NodeId,
    add: NodeId,
    cancel: NodeId,
    found: Rc<RefCell<Vec<Found>>>,
}

/// The dialog shown before a change brings code, over the whole window.
struct Dialog {
    cover: NodeId,
    lines: NodeId,
    add: NodeId,
    cancel: NodeId,
}

/// The picked package's controls, made again as it changes.
#[derive(Default)]
struct Detail {
    update: Option<NodeId>,
    remove: Option<NodeId>,
    samples: Vec<(NodeId, Sample)>,
}

/// The task running on the window's thread.
struct Running {
    replies: Receiver<Reply>,
    doing: String,
    started: Instant,
}

pub struct PackageManager {
    add: NodeId,
    update_all: NodeId,
    refresh: NodeId,
    search: NodeId,
    summary: NodeId,
    list: NodeId,
    empty: NodeId,
    shown: Rc<RefCell<Vec<Shown>>>,
    body: NodeId,
    nothing_picked: NodeId,
    detail: Detail,
    error: NodeId,
    error_text: NodeId,
    error_close: NodeId,
    /// Under the list: what was last done, or the progress of what runs.
    foot: NodeId,
    status: NodeId,
    progress: NodeId,
    progress_text: NodeId,
    progress_fill: NodeId,
    git: GitForm,
    registry: RegistryForm,
    dialog: Dialog,
    rows: Vec<Row>,
    updates: BTreeMap<String, Available>,
    filtered: Vec<usize>,
    /// The picked package, by name, so it stays picked as the list changes.
    picked: Option<String>,
    /// Whether the packages have been read since the window opened.
    listed: bool,
    /// The project's folder, as the editor last gave it.
    project: PathBuf,
    cache: Cache,
    queue: VecDeque<Task>,
    running: Option<Running>,
    /// A change waiting on the dialog's Add.
    pending: Option<Planned>,
    /// The folder chooser, while it is open.
    choosing: Option<Receiver<std::io::Result<Option<PathBuf>>>>,
    events: Vec<PackagesEvent>,
    /// Whether the theme is dark, for the colours of a README's code.
    dark: bool,
}

impl PackageManager {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let add = ui.button(bar, "");
        ui.add_class(add, "console-button");
        ui.add_class(add, "packages-add");
        ui.set_name(add, "packages-add");
        ui.icon(add, "plus");
        let arrow = ui.icon(add, "chevron-down");
        ui.add_class(arrow, "packages-add-arrow");
        let update_all = ui.button(bar, "Update All");
        ui.add_class(update_all, "console-button");
        ui.set_name(update_all, "packages-update-all");
        let search_box = ui.panel(bar, Style::row());
        ui.add_class(search_box, "search");
        ui.add_class(search_box, "packages-search");
        let icon = ui.icon(search_box, "search");
        ui.add_class(icon, "search-icon");
        let search = ui.text_field(search_box, "");
        ui.add_class(search, "search-field");
        ui.set_placeholder(search, "Search packages");
        ui.set_name(search, "packages-search");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        ui.set_name(summary, "packages-summary");
        let refresh = ui.button(bar, "Refresh");
        ui.add_class(refresh, "console-button");
        ui.set_name(refresh, "packages-refresh");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let split = ui.split_view(root, true, LIST_WIDTH);
        ui.add_class(split, "fill");
        let (left, right) = ui.split_panes(split).expect("a split view has two panes");
        ui.add_class(left, "packages-side");
        let empty = ui.add_label(left, "No packages");
        ui.add_class(empty, "placeholder");
        ui.set_name(empty, "packages-empty");
        let shown: Rc<RefCell<Vec<Shown>>> = Rc::default();
        let bound = shown.clone();
        let list = ui.list_view(
            left,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "packages-row");
                    let top = ui.panel(row, Style::row());
                    ui.add_class(top, "packages-row-top");
                    let title = ui.add_label(top, "");
                    ui.add_class(title, "packages-row-title");
                    let update = ui.panel(top, Style::row());
                    ui.add_class(update, "packages-row-update");
                    ui.icon(update, "arrow-up");
                    ui.add_label(update, "");
                    let version = ui.add_label(top, "");
                    ui.add_class(version, "packages-row-version");
                    let line = ui.add_label(row, "");
                    ui.add_class(line, "packages-row-line");
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = bound.borrow();
                    let Some(item) = shown.get(index) else { return };
                    let parts = ui.children(row);
                    let top = ui.children(parts[0]);
                    ui.set_text(top[0], &item.title);
                    ui.set_visible(top[1], item.update.is_some());
                    let update_label = ui.children(top[1])[1];
                    ui.set_text(update_label, item.update.as_deref().unwrap_or_default());
                    ui.set_text(top[2], &item.version);
                    ui.set_text(parts[1], &item.line);
                },
            },
        );
        ui.add_class(list, "fill");
        ui.set_name(list, "packages-list");
        let error = ui.panel(left, Style::row());
        ui.add_class(error, "packages-error");
        let error_text = ui.add_label(error, "");
        ui.add_class(error_text, "packages-error-text");
        ui.set_name(error_text, "packages-error");
        let error_close = ui.icon_button(error, "close");
        ui.set_visible(error, false);
        let foot = ui.panel(left, Style::column());
        ui.add_class(foot, "packages-foot");
        ui.set_visible(foot, false);
        let status = ui.add_label(foot, "");
        ui.add_class(status, "packages-status");
        ui.set_name(status, "packages-status");
        let progress = ui.panel(foot, Style::column());
        ui.add_class(progress, "build-progress");
        ui.add_class(progress, "packages-progress");
        let progress_text = ui.add_label(progress, "");
        ui.add_class(progress_text, "build-progress-text");
        ui.set_name(progress_text, "packages-progress");
        let track = ui.panel(progress, Style::default());
        ui.add_class(track, "build-progress-track");
        let progress_fill = ui.panel(track, Style::default());
        ui.add_class(progress_fill, "build-progress-fill");
        ui.set_visible(progress, false);

        ui.add_class(right, "packages-detail");
        let body = ui.panel(right, Style::column());
        ui.add_class(body, "fill");
        ui.add_class(body, "packages-detail-body");
        ui.set_name(body, "packages-detail");
        let nothing_picked = ui.add_label(right, "Nothing selected");
        ui.add_class(nothing_picked, "placeholder");

        let git = git_form(ui, root);
        let registry = registry_form(ui, root);
        let dialog = dialog(ui, root);
        Self {
            add,
            update_all,
            refresh,
            search,
            summary,
            list,
            empty,
            shown,
            body,
            nothing_picked,
            detail: Detail::default(),
            error,
            error_text,
            error_close,
            foot,
            status,
            progress,
            progress_text,
            progress_fill,
            git,
            registry,
            dialog,
            rows: Vec::new(),
            updates: BTreeMap::new(),
            filtered: Vec::new(),
            picked: None,
            listed: false,
            project: PathBuf::new(),
            cache: Cache::for_machine(),
            queue: VecDeque::new(),
            running: None,
            pending: None,
            choosing: None,
            events: Vec::new(),
            dark: true,
        }
    }

    /// Keeps packages fetched in another cache than the machine's, as tests
    /// and build machines do.
    pub fn set_cache(&mut self, cache: Cache) {
        self.cache = cache;
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
    }

    /// Adds a package, as the + menu's forms do: worked out on the window's
    /// thread, and written at once or once the dialog's Add is pressed.
    pub fn add(&mut self, add: Add) {
        self.queue.push_back(Task::Change(Asked::Add(add)));
    }

    /// The project's packages as last read, in the list's order.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The updates found, by package name.
    pub fn updates(&self) -> &BTreeMap<String, Available> {
        &self.updates
    }

    /// Whether the dialog asking before code arrives is open.
    pub fn is_asking(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether work is running or waiting to.
    pub fn is_busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }

    pub fn list(&self) -> NodeId {
        self.list
    }

    /// Follows the window's buttons and its thread. Work goes on while the
    /// window is hidden, but the packages are read first when it shows.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, project: &Path) -> Vec<PackagesEvent> {
        if self.project != project {
            self.project = project.to_path_buf();
        }
        self.take_replies(ui);
        self.take_folder(ui);
        if shown {
            if !self.listed {
                self.listed = true;
                self.queue.push_back(Task::List);
            }
            self.take_clicks(ui);
        }
        if self.pending.is_none() && self.running.is_none() {
            self.start_next();
        }
        self.show_progress(ui);
        std::mem::take(&mut self.events)
    }

    fn take_clicks(&mut self, ui: &mut Ui) {
        if ui.clicked(self.add) {
            let [x, y, _, height] = ui.rect(self.add);
            let items: Vec<MenuItem> = ADD_FROM.iter().map(|(_, label)| MenuItem::new(label)).collect();
            ui.open_menu(self.add, (x, y + height + 4.0), &items);
        }
        if let Some(choice) = ui.menu_choice(self.add) {
            match ADD_FROM.get(choice).map(|(from, _)| *from) {
                Some(AddFrom::Git) => {
                    ui.set_visible(self.registry.panel, false);
                    ui.set_visible(self.git.panel, true);
                    ui.focus(self.git.url);
                }
                Some(AddFrom::Folder) => self.choose_folder(ui),
                Some(AddFrom::Registry) => {
                    ui.set_visible(self.git.panel, false);
                    ui.set_visible(self.registry.panel, true);
                    ui.focus(self.registry.search);
                    ui.set_text(self.registry.state, "Searching…");
                    ui.set_visible(self.registry.state, true);
                    self.search_registry(ui, true);
                }
                None => {}
            }
        }
        if ui.clicked(self.update_all) {
            self.queue.push_back(Task::Change(Asked::Update(Update::All)));
        }
        if ui.clicked(self.refresh) {
            self.queue.push_back(Task::List);
        }
        if ui.changed(self.search) {
            self.filter(ui);
        }
        if ui.clicked(self.error_close) {
            ui.set_visible(self.error, false);
        }
        if ui.selection_changed(self.list) {
            self.picked = ui
                .selection(self.list)
                .first()
                .and_then(|&at| self.filtered.get(at))
                .map(|&at| self.rows[at].name.clone());
            self.show_detail(ui);
        }
        self.take_detail_clicks(ui);
        self.take_git_form(ui);
        self.take_registry_form(ui);
        self.take_dialog(ui);
    }

    fn take_detail_clicks(&mut self, ui: &mut Ui) {
        let Some(name) = self.picked.clone() else { return };
        if self.detail.update.is_some_and(|button| ui.clicked(button)) {
            self.queue.push_back(Task::Change(Asked::Update(Update::Only(vec![name.clone()]))));
        }
        if self.detail.remove.is_some_and(|button| ui.clicked(button)) {
            self.queue.push_back(Task::Change(Asked::Remove(name.clone())));
        }
        let imported = self.detail.samples.iter().find(|(button, _)| ui.clicked(*button));
        if let Some((button, sample)) = imported {
            ui.set_enabled(*button, false);
            self.queue.push_back(Task::Import { package: name, sample: sample.clone() });
        }
    }

    fn take_git_form(&mut self, ui: &mut Ui) {
        let form = &self.git;
        if ui.clicked(form.cancel) || ui.escaped(form.url) {
            ui.set_visible(form.panel, false);
            return;
        }
        let submitted = [form.url, form.value, form.path].into_iter().any(|field| ui.submitted(field));
        if !ui.clicked(form.add) && !submitted {
            return;
        }
        let url = ui.text(form.url).trim().to_string();
        if url.is_empty() {
            return ui.focus(form.url);
        }
        let kind = RevKind::ALL.get(ui.selected_option(form.rev)).copied().unwrap_or(RevKind::Default);
        let rev = kind.rev(&ui.text(form.value));
        let path = Some(ui.text(form.path).trim().to_string()).filter(|path| !path.is_empty());
        ui.set_visible(form.panel, false);
        ui.set_text(form.url, "");
        self.add(Add::Git { url, rev, path });
    }

    fn take_registry_form(&mut self, ui: &mut Ui) {
        let form = &self.registry;
        if ui.clicked(form.cancel) || ui.escaped(form.search) {
            ui.set_visible(form.panel, false);
            return;
        }
        if ui.changed(form.search) {
            self.search_registry(ui, false);
            return;
        }
        let form = &self.registry;
        if !ui.clicked(form.add) && !ui.double_clicked(form.list) {
            return;
        }
        let Some(&at) = ui.selection(form.list).first() else { return };
        let Some(name) = form.found.borrow().get(at).map(|found| found.name.clone()) else { return };
        ui.set_visible(form.panel, false);
        self.add(Add::Registry { name, version: None, registry: None });
    }

    /// Searches the registry for what its field holds: fetched first when
    /// the search opens, and from what was fetched as words are typed. Only
    /// the newest search waits.
    fn search_registry(&mut self, ui: &Ui, fetch: bool) {
        let words = ui.text(self.registry.search);
        self.queue.retain(|task| !matches!(task, Task::Search { .. }));
        self.queue.push_back(Task::Search { words, fetch });
    }

    fn take_dialog(&mut self, ui: &mut Ui) {
        if self.pending.is_none() {
            return;
        }
        if ui.clicked(self.dialog.cancel) {
            self.pending = None;
            ui.set_visible(self.dialog.cover, false);
            ui.set_text(self.status, "Nothing changed");
        } else if ui.clicked(self.dialog.add) {
            ui.set_visible(self.dialog.cover, false);
            if let Some(planned) = self.pending.take() {
                self.apply(ui, planned);
            }
        }
    }

    /// Opens the desktop's folder chooser on a thread, as Import New Asset's
    /// file chooser is, so the editor keeps drawing while it is open.
    fn choose_folder(&mut self, ui: &mut Ui) {
        if self.choosing.is_some() {
            return;
        }
        let (send, chosen) = mpsc::channel();
        let started = std::thread::Builder::new().name("folder chooser".into()).spawn(move || {
            let _ = send.send(choose_folder("Add from Folder"));
        });
        match started {
            Ok(_) => self.choosing = Some(chosen),
            Err(error) => self.show_error(ui, &format!("Could not open a folder chooser: {error}")),
        }
    }

    fn take_folder(&mut self, ui: &mut Ui) {
        let Some(chosen) = &self.choosing else { return };
        match chosen.try_recv() {
            Ok(Ok(Some(folder))) => {
                self.choosing = None;
                self.add(Add::Folder(folder));
            }
            Ok(Ok(None)) | Err(TryRecvError::Disconnected) => self.choosing = None,
            Ok(Err(error)) => {
                self.choosing = None;
                self.show_error(ui, &format!("Could not open a folder chooser: {error}"));
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn start_next(&mut self) {
        let Some(task) = self.queue.pop_front() else { return };
        let (send, replies) = mpsc::channel();
        let doing = task.doing();
        let (project, cache) = (self.project.clone(), self.cache.clone());
        let started = std::thread::Builder::new()
            .name("packages".into())
            .spawn(move || package_list::run(task, &project, &cache, &send));
        match started {
            Ok(_) => self.running = Some(Running { replies, doing, started: Instant::now() }),
            Err(error) => tracing::error!("could not start the Package Manager's thread: {error}"),
        }
    }

    fn take_replies(&mut self, ui: &mut Ui) {
        loop {
            let Some(running) = &mut self.running else { return };
            let reply = match running.replies.try_recv() {
                Ok(reply) => reply,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.running = None;
                    return;
                }
            };
            match reply {
                Reply::Doing(doing) => running.doing = doing,
                Reply::Listed(rows) => {
                    self.rows = rows;
                    self.updates.retain(|name, _| self.rows.iter().any(|row| row.name == *name));
                    self.listed_again(ui);
                }
                Reply::Updates(updates) => {
                    self.updates = updates;
                    self.listed_again(ui);
                }
                Reply::Planned(planned) => self.planned(ui, *planned),
                Reply::Imported(folder) => self.imported(ui, &folder),
                Reply::Found(found) => {
                    let state = if found.is_empty() { "Nothing matches" } else { "" };
                    ui.set_text(self.registry.state, state);
                    ui.set_visible(self.registry.state, !state.is_empty());
                    ui.set_list_count(self.registry.list, found.len());
                    *self.registry.found.borrow_mut() = found;
                    ui.refresh_list(self.registry.list);
                    if ui.selection(self.registry.list).is_empty() && ui.is_visible(self.registry.panel) {
                        ui.set_selection(self.registry.list, &[0]);
                    }
                }
                Reply::Failed(error) => {
                    tracing::warn!("Package Manager: {error}");
                    if ui.is_visible(self.registry.panel) {
                        ui.set_text(self.registry.state, "Could not read the registry");
                        ui.set_visible(self.registry.state, true);
                    }
                    self.show_error(ui, &error);
                    // A task that fails leaves the samples' Import buttons as they were.
                    self.show_detail(ui);
                }
            }
        }
    }

    /// A change worked out: written at once, unless it brings code from
    /// elsewhere, which waits on the dialog.
    fn planned(&mut self, ui: &mut Ui, planned: Planned) {
        if planned.changes_nothing() {
            let text = if planned.intent == package_list::Intent::Update { "Up to date" } else { "Nothing changes" };
            ui.set_text(self.status, text);
            return;
        }
        if planned.asks_first() {
            fill_dialog(ui, self.dialog.lines, &planned);
            ui.set_visible(self.dialog.cover, true);
            self.pending = Some(planned);
            return;
        }
        self.apply(ui, planned);
    }

    fn apply(&mut self, ui: &mut Ui, planned: Planned) {
        let done = planned.done();
        match planned.plan.apply() {
            Ok(_) => {
                ui.set_visible(self.error, false);
                ui.set_text(self.status, &done);
                self.events.push(PackagesEvent::Changed(done));
                self.queue.push_front(Task::List);
            }
            Err(error) => self.show_error(ui, &format!("{error:#}")),
        }
    }

    fn imported(&mut self, ui: &mut Ui, folder: &Path) {
        let shown = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        ui.set_text(self.status, &format!("Imported {shown}"));
        if let Ok(path) = folder.strip_prefix(&self.project) {
            self.events.push(PackagesEvent::Imported(path.to_string_lossy().replace('\\', "/")));
        }
        self.show_detail(ui);
    }

    fn show_error(&mut self, ui: &mut Ui, error: &str) {
        ui.set_text(self.error_text, error);
        ui.set_visible(self.error, true);
    }

    /// The progress line: what runs, for how long, and a bar sliding across,
    /// as Git says nothing of how far it has got.
    fn show_progress(&mut self, ui: &mut Ui) {
        let Some(running) = &self.running else {
            if ui.is_visible(self.progress) {
                ui.set_visible(self.progress, false);
                ui.set_visible(self.status, true);
            }
            let said = !ui.text(self.status).is_empty();
            if ui.is_visible(self.foot) != said {
                ui.set_visible(self.foot, said);
            }
            return;
        };
        ui.set_visible(self.foot, true);
        let seconds = running.started.elapsed().as_secs_f32();
        let text = match seconds {
            0.0..1.0 => format!("{}…", running.doing),
            _ => format!("{}…  {seconds:.0} s", running.doing),
        };
        ui.set_text(self.progress_text, &text);
        let at = (seconds / SWEEP_SECONDS).fract() * 130.0 - 30.0;
        ui.set_inline_style(self.progress_fill, &format!("width: 30%; margin-left: {at:.1}%"));
        ui.set_visible(self.progress, true);
        ui.set_visible(self.status, false);
    }

    /// The list and the details, after the packages or their updates were
    /// read again.
    fn listed_again(&mut self, ui: &mut Ui) {
        let mut summary = match self.rows.len() {
            0 => String::new(),
            1 => "1 package".to_string(),
            count => format!("{count} packages"),
        };
        match self.updates.len() {
            0 => {}
            1 => summary.push_str("   1 update"),
            count => summary.push_str(&format!("   {count} updates")),
        }
        ui.set_text(self.summary, &summary);
        ui.set_enabled(self.update_all, !self.updates.is_empty());
        self.filter(ui);
        self.show_detail(ui);
    }

    fn filter(&mut self, ui: &mut Ui) {
        let search = ui.text(self.search);
        self.filtered = (0..self.rows.len()).filter(|&at| self.rows[at].matches(&search)).collect();
        *self.shown.borrow_mut() = self
            .filtered
            .iter()
            .map(|&at| {
                let row = &self.rows[at];
                Shown {
                    title: row.title.clone(),
                    version: row.version.clone(),
                    update: self.updates.get(&row.name).map(Available::label),
                    line: format!("{}  ·  {}", row.source.label(), package_list::holds(&row.contents)),
                }
            })
            .collect();
        ui.set_list_count(self.list, self.filtered.len());
        ui.refresh_list(self.list);
        ui.set_visible(self.empty, self.rows.is_empty());
        let picked: Vec<usize> = self
            .picked
            .as_ref()
            .and_then(|name| self.filtered.iter().position(|&at| self.rows[at].name == *name))
            .into_iter()
            .collect();
        ui.set_selection(self.list, &picked);
        let _ = ui.selection_changed(self.list);
    }

    /// The picked package's details: its name, version and where it comes
    /// from with Update and Remove, then what it is, holds and needs, its
    /// docs pages, its samples and its README.
    fn show_detail(&mut self, ui: &mut Ui) {
        for child in ui.children(self.body) {
            ui.remove(child);
        }
        self.detail = Detail::default();
        let row = self.picked.as_ref().and_then(|name| self.rows.iter().find(|row| row.name == *name)).cloned();
        ui.set_visible(self.nothing_picked, row.is_none());
        ui.set_visible(self.body, row.is_some());
        let Some(row) = row else { return };
        let body = self.body;

        let head = ui.panel(body, Style::row());
        ui.add_class(head, "packages-head");
        let title = ui.add_label(head, &row.title);
        ui.add_class(title, "packages-title");
        ui.set_name(title, "packages-title");
        if let Some(update) = self.updates.get(&row.name) {
            let button = ui.button(head, &format!("Update to {}", update.label()));
            ui.add_class(button, "build-button");
            ui.add_class(button, "primary");
            ui.set_name(button, "packages-update");
            self.detail.update = Some(button);
        }
        let remove = ui.button(head, "Remove");
        ui.add_class(remove, "build-button");
        ui.set_name(remove, "packages-remove");
        if !row.direct {
            ui.set_enabled(remove, false);
            ui.set_tooltip(remove, "Other packages need it", None);
        }
        self.detail.remove = Some(remove);
        let sub = ui.add_label(body, &format!("{}   {}", row.version, row.name));
        ui.add_class(sub, "packages-subtitle");

        let fields = ui.panel(body, Style::column());
        ui.add_class(fields, "packages-fields");
        let field = |ui: &mut Ui, key: &str, value: &str| {
            let line = ui.panel(fields, Style::row());
            ui.add_class(line, "packages-field");
            let label = ui.add_label(line, key);
            ui.add_class(label, "build-key");
            let value = ui.add_label(line, value);
            ui.add_class(value, "packages-value");
            value
        };
        field(ui, "Source", row.source.label());
        let place = field(ui, "Location", &row.place);
        ui.add_class(place, "mono");
        if !row.authors.is_empty() {
            field(ui, "Authors", &row.authors.join(", "));
        }
        let holds = field(ui, "Holds", &package_list::holds(&row.contents));
        ui.set_name(holds, "packages-holds");
        if !row.dependencies.is_empty() {
            field(ui, "Needs", &row.dependencies.join(", "));
        }
        let needed_by: Vec<&str> = self
            .rows
            .iter()
            .filter(|other| other.dependencies.contains(&row.name))
            .map(|other| other.name.as_str())
            .collect();
        if !needed_by.is_empty() {
            field(ui, "Needed by", &needed_by.join(", "));
        }

        if !row.description.is_empty() {
            let description = ui.add_label(body, &row.description);
            ui.add_class(description, "packages-text");
            ui.set_name(description, "packages-description");
        }
        if !row.docs.is_empty() {
            heading(ui, body, "Docs");
            for title in &row.docs {
                let page = ui.add_label(body, title);
                ui.add_class(page, "packages-doc");
            }
        }
        if !row.contents.samples.is_empty() {
            heading(ui, body, "Samples");
            for sample in &row.contents.samples {
                let line = ui.panel(body, Style::row());
                ui.add_class(line, "packages-sample");
                let name = ui.add_label(line, &sample.name);
                ui.add_class(name, "packages-sample-name");
                let there = sample.destination(&self.project, &row.name).exists();
                let button = ui.button(line, if there { "Imported" } else { "Import" });
                ui.add_class(button, "build-button");
                ui.set_name(button, &format!("packages-import-{}", sample.name));
                ui.set_enabled(button, !there);
                self.detail.samples.push((button, sample.clone()));
            }
        }
        if let Some(readme) = &row.readme {
            heading(ui, body, "README");
            let page = ui.panel(body, Style::column());
            ui.add_class(page, "packages-readme");
            ui.set_name(page, "packages-readme");
            // Its links and pictures point into the package, which the window does not follow.
            let lookups = Lookups { works: &|_| true, dark: self.dark, pictures: &|_| Picture::Missing };
            view::render(ui, page, &markdown::parse(readme), &lookups);
        }
    }
}

fn heading(ui: &mut Ui, parent: NodeId, text: &str) {
    let heading = ui.add_label(parent, text);
    ui.add_class(heading, "profiler-heading");
    ui.add_class(heading, "packages-heading");
}

fn git_form(ui: &mut Ui, root: NodeId) -> GitForm {
    let panel = ui.panel(root, Style::column());
    ui.add_class(panel, "packages-popup");
    ui.set_name(panel, "packages-git");
    let title = ui.add_label(panel, "Add from Git URL");
    ui.add_class(title, "layout-prompt-title");
    let url = ui.text_field(panel, "");
    ui.add_class(url, "search-field");
    ui.add_class(url, "packages-input");
    ui.set_placeholder(url, "https://github.com/someone/orbit-tools");
    ui.set_name(url, "packages-git-url");
    let line = ui.panel(panel, Style::row());
    ui.add_class(line, "packages-popup-row");
    let labels: Vec<&str> = RevKind::ALL.iter().map(|kind| kind.label()).collect();
    let rev = ui.dropdown(line, &labels, 0);
    ui.add_class(rev, "packages-rev");
    ui.set_name(rev, "packages-git-rev");
    let value = ui.text_field(line, "");
    ui.add_class(value, "search-field");
    ui.add_class(value, "packages-input");
    ui.set_placeholder(value, "v1.2.0, main or a commit");
    ui.set_name(value, "packages-git-value");
    let path = ui.text_field(panel, "");
    ui.add_class(path, "search-field");
    ui.add_class(path, "packages-input");
    ui.set_placeholder(path, "Path in the repository");
    ui.set_name(path, "packages-git-path");
    let buttons = ui.panel(panel, Style::row());
    ui.add_class(buttons, "packages-buttons");
    let cancel = ui.button(buttons, "Cancel");
    ui.add_class(cancel, "build-button");
    let add = ui.button(buttons, "Add");
    ui.add_class(add, "build-button");
    ui.add_class(add, "primary");
    ui.set_name(add, "packages-git-add");
    ui.set_visible(panel, false);
    GitForm { panel, url, rev, value, path, add, cancel }
}

fn registry_form(ui: &mut Ui, root: NodeId) -> RegistryForm {
    let panel = ui.panel(root, Style::column());
    ui.add_class(panel, "packages-popup");
    ui.add_class(panel, "packages-registry");
    ui.set_name(panel, "packages-registry");
    let title = ui.add_label(panel, "Add from Registry");
    ui.add_class(title, "layout-prompt-title");
    let search_box = ui.panel(panel, Style::row());
    ui.add_class(search_box, "search");
    ui.add_class(search_box, "packages-registry-search");
    let icon = ui.icon(search_box, "search");
    ui.add_class(icon, "search-icon");
    let search = ui.text_field(search_box, "");
    ui.add_class(search, "search-field");
    ui.set_placeholder(search, "Search the registry");
    ui.set_name(search, "packages-registry-search");
    let found: Rc<RefCell<Vec<Found>>> = Rc::default();
    let bound = found.clone();
    let list = ui.list_view(
        panel,
        FOUND_ROW,
        0,
        ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                ui.add_class(row, "packages-row");
                let top = ui.panel(row, Style::row());
                ui.add_class(top, "packages-row-top");
                let name = ui.add_label(top, "");
                ui.add_class(name, "packages-row-title");
                let version = ui.add_label(top, "");
                ui.add_class(version, "packages-row-version");
                let description = ui.add_label(row, "");
                ui.add_class(description, "packages-row-line");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let found = bound.borrow();
                let Some(item) = found.get(index) else { return };
                let parts = ui.children(row);
                let top = ui.children(parts[0]);
                ui.set_text(top[0], &item.name);
                ui.set_text(top[1], item.latest.as_deref().unwrap_or("None for this engine"));
                ui.set_text(parts[1], &item.description);
            },
        },
    );
    ui.add_class(list, "packages-registry-list");
    ui.set_name(list, "packages-registry-list");
    let state = ui.add_label(panel, "");
    ui.add_class(state, "placeholder");
    ui.set_name(state, "packages-registry-state");
    let buttons = ui.panel(panel, Style::row());
    ui.add_class(buttons, "packages-buttons");
    let cancel = ui.button(buttons, "Cancel");
    ui.add_class(cancel, "build-button");
    let add = ui.button(buttons, "Add");
    ui.add_class(add, "build-button");
    ui.add_class(add, "primary");
    ui.set_name(add, "packages-registry-add");
    ui.set_visible(panel, false);
    RegistryForm { panel, search, list, state, add, cancel, found }
}

fn dialog(ui: &mut Ui, root: NodeId) -> Dialog {
    let cover = ui.panel(root, Style::column());
    ui.add_class(cover, "packages-cover");
    ui.set_name(cover, "packages-dialog");
    let card = ui.panel(cover, Style::column());
    ui.add_class(card, "packages-dialog");
    let title = ui.add_label(card, "Packages with scripts");
    ui.add_class(title, "packages-dialog-title");
    let hint = ui.add_label(card, "Their scripts run with the project's.");
    ui.add_class(hint, "packages-dialog-hint");
    let lines = ui.panel(card, Style::column());
    ui.add_class(lines, "packages-dialog-lines");
    ui.set_name(lines, "packages-dialog-lines");
    let buttons = ui.panel(card, Style::row());
    ui.add_class(buttons, "packages-buttons");
    let cancel = ui.button(buttons, "Cancel");
    ui.add_class(cancel, "build-button");
    ui.set_name(cancel, "packages-dialog-cancel");
    let add = ui.button(buttons, "Add");
    ui.add_class(add, "build-button");
    ui.add_class(add, "primary");
    ui.set_name(add, "packages-dialog-add");
    ui.set_visible(cover, false);
    Dialog { cover, lines, add, cancel }
}

/// The dialog's lines: each package arriving, with where from and what it
/// holds, then those going and those moving to another version.
fn fill_dialog(ui: &mut Ui, lines: NodeId, planned: &Planned) {
    for child in ui.children(lines) {
        ui.remove(child);
    }
    let line = |ui: &mut Ui, mark: &str, class: &str, title: &str, version: &str| {
        let line = ui.panel(lines, Style::row());
        ui.add_class(line, "packages-dialog-line");
        let sign = ui.add_label(line, mark);
        ui.add_class(sign, "packages-dialog-mark");
        ui.add_class(sign, class);
        let name = ui.add_label(line, title);
        ui.add_class(name, "packages-dialog-name");
        let version = ui.add_label(line, version);
        ui.add_class(version, "packages-row-version");
    };
    for arrival in &planned.arriving {
        line(ui, "+", "packages-added", &arrival.name, &arrival.version);
        let from = ui.add_label(lines, &format!("{}  ·  {}", arrival.source.label(), arrival.place));
        ui.add_class(from, "packages-dialog-detail");
        ui.add_class(from, "mono");
        let holds = ui.add_label(lines, &arrival.holds);
        ui.add_class(holds, "packages-dialog-detail");
        ui.set_class(holds, "packages-runs-code", arrival.runs_code && !arrival.local);
        if !arrival.description.is_empty() {
            let description = ui.add_label(lines, &arrival.description);
            ui.add_class(description, "packages-dialog-detail");
        }
    }
    for (name, version) in &planned.removed {
        line(ui, "−", "packages-removed", name, version);
    }
    for (name, from, to) in &planned.moved {
        let version = if from == to { format!("{to}, new commit") } else { format!("{from} → {to}") };
        line(ui, "~", "packages-moved", name, &version);
    }
}

/// A folder picked in the desktop's chooser, or `None` when it is closed
/// without one: zenity's, or else KDE's kdialog.
fn choose_folder(title: &str) -> std::io::Result<Option<PathBuf>> {
    let choosers: [(&str, &[&str]); 2] = [
        ("zenity", &["--file-selection", "--directory", "--title", title]),
        ("kdialog", &["--getexistingdirectory", ".", "--title", title]),
    ];
    for (program, args) in choosers {
        match std::process::Command::new(program).args(args).output() {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                return Ok(text.lines().find(|line| !line.is_empty()).map(PathBuf::from));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    let missing = "no folder chooser was found; install zenity, or run vox package add with the folder";
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, missing))
}
