//! The Build window, after Unity 6's Build Profiles: the project's build
//! profiles on the left (see `voxear_engine::build_profile`), the picked
//! one's settings beside them, and Build.
//!
//! The editor never builds inside itself. Build starts `vox build
//! --progress json` as a process of its own, as a terminal or CI would, so
//! the editor keeps working while it builds and a build that crashes cannot
//! take it down. The window follows the process's JSON lines: each stage
//! with its time, the current stage's progress, and the build's log. Cancel
//! asks the build to stop at its next step, and stops the process if it has
//! not within ten seconds. A finished build offers Run and Show in Files.
//! Run starts the built game, and its log comes into the Console as the
//! Player category, as Sifter shows a player's log.
//!
//! The picked profile's settings are edited in place, each written to the
//! profile's file as it changes (see `build_profile::set_setting`). Buttons
//! over the log show or hide its info, warnings and errors, and a build that
//! finishes while the editor is not the window in front says so on the
//! desktop.

use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    time::{Duration, Instant},
};

use serde_json::Value;
use tracing::Level;
use voxear_engine::{build_profile, player_build::sibling_program};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

/// How long a cancelled build has to stop before its process is stopped.
const CANCEL_WAIT: Duration = Duration::from_secs(10);
/// How often the profiles are read again while the window shows, to see
/// ones added or changed outside the editor.
const PROFILES_CHECK: Duration = Duration::from_secs(1);
const PROFILE_ROW: f32 = 26.0;
const LOG_ROW: f32 = 20.0;
/// How a setting is edited in the window.
#[derive(Clone, Copy)]
enum Edit {
    /// Words, with what is used when there are none.
    Text(&'static str),
    /// Words separated by commas, written as a list.
    List(&'static str),
    /// A whole number.
    Number,
    Toggle,
    /// One of several: what each shows, and what it writes, nothing for the first.
    Choice(&'static [(&'static str, &'static str)]),
}

/// How a profile can compress its bundles, the project's own way first.
const COMPRESSIONS: &[(&str, &str)] =
    &[("The project's", ""), ("LZ4, quick to load", "lz4"), ("Zstd, smaller", "zstd"), ("None", "none")];

/// The settings the window edits: each one's label, its table and key in a
/// profile, and how it is edited.
const SETTINGS: [(&str, Option<&str>, &str, Edit); 14] = [
    (
        "Platform",
        None,
        "platform",
        Edit::Choice(&[("This machine", ""), ("Linux", "linux-x64"), ("Windows", "windows-x64")]),
    ),
    ("Start level", Some("levels"), "start", Edit::Text("The project's")),
    ("Levels", Some("levels"), "include", Edit::List("Every level")),
    ("Development", Some("development"), "enabled", Edit::Toggle),
    ("Vulkan validation", Some("development"), "validation_layers", Edit::Toggle),
    ("Defines", Some("scripting"), "defines", Edit::List("None, or DEMO, STEAM")),
    ("Product name", Some("player"), "product_name", Edit::Text("The project's name")),
    ("Company", Some("player"), "company_name", Edit::Text("Not set")),
    ("Version", Some("version"), "version", Edit::Text("Not set")),
    ("Window", Some("player"), "window", Edit::Choice(&[("Windowed", "windowed"), ("Full screen", "fullscreen")])),
    ("Width", Some("player"), "width", Edit::Number),
    ("Height", Some("player"), "height", Edit::Number),
    ("Compression", Some("content"), "compression", Edit::Choice(COMPRESSIONS)),
    ("Output", Some("output"), "folder", Edit::Text("Builds/<profile>")),
];
/// The build's stages, as `vox build` numbers them.
const STAGES: [&str; 3] = ["Scripts", "Content", "Player"];

/// A line from the build's process.
enum Line {
    /// A stage started, counting from 0.
    Stage(usize),
    Progress {
        done: usize,
        total: usize,
        item: String,
    },
    Log {
        level: String,
        message: String,
    },
    Finished(Done),
}

/// How a build ended.
struct Done {
    result: Outcome,
    seconds: f64,
    message: String,
    output: Option<PathBuf>,
    program: Option<PathBuf>,
    size: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Succeeded,
    Failed,
    Cancelled,
}

/// A build in progress: its process, and what it has said.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Line>,
    profile: String,
    started: Instant,
    /// The stage running, and since when.
    stage: Option<(usize, Instant)>,
    /// When Cancel was pressed.
    cancelled: Option<Instant>,
}

/// What a finished build tells the editor, for its status bar.
pub struct Finished {
    pub succeeded: bool,
    pub text: String,
}

pub struct BuildWindow {
    list: NodeId,
    empty: NodeId,
    add_defaults: NodeId,
    title: NodeId,
    /// The profile's base, beside its name.
    base: NodeId,
    /// Each setting's control, in [`SETTINGS`] order, and the text setting
    /// being typed in, written once it is left or Enter is pressed.
    controls: Vec<NodeId>,
    typing: Option<usize>,
    problem: NodeId,
    build: NodeId,
    cancel: NodeId,
    run: NodeId,
    reveal: NodeId,
    /// Each stage's chip and time.
    stages: Vec<(NodeId, NodeId)>,
    progress: NodeId,
    progress_text: NodeId,
    progress_fill: NodeId,
    result: NodeId,
    log_list: NodeId,
    names: Rc<RefCell<Vec<String>>>,
    log: Rc<RefCell<Vec<(String, String)>>>,
    /// The log lines the list shows, by their place in the log, and how
    /// many lines the log had when they were picked.
    log_rows: Rc<RefCell<Vec<usize>>>,
    log_shown: usize,
    /// The buttons that show or hide info, warnings and errors, and which
    /// are hidden.
    log_levels: [NodeId; 3],
    log_hidden: [bool; 3],
    /// Whether the editor's window is in front, so a finished build is
    /// told on the desktop only when it is not.
    window_focused: bool,
    picked: Option<String>,
    checked: Option<Instant>,
    running: Option<Running>,
    /// Each stage's time in the build running or last run.
    times: [Option<Duration>; 3],
    /// The stage that failed, in the last build.
    failed: Option<usize>,
    /// The content stage's progress: done, of how many, and the asset.
    content: Option<(usize, usize, String)>,
    /// The last build that succeeded: its program and folder.
    built: Option<(PathBuf, PathBuf)>,
    /// `vox`, which builds.
    program: PathBuf,
}

impl BuildWindow {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let main = ui.panel(root, Style::row());
        ui.add_class(main, "fill");
        ui.add_class(main, "build-main");

        let side = ui.panel(main, Style::default());
        ui.add_class(side, "build-profiles");
        let heading = ui.add_label(side, "Profiles");
        ui.add_class(heading, "build-heading");
        let names: Rc<RefCell<Vec<String>>> = Rc::default();
        let bound = names.clone();
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let name = ui.add_label(row, "");
                ui.add_class(name, "build-profile-name");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let names = bound.borrow();
                let parts = ui.children(row);
                ui.set_text(parts[0], names.get(index).map_or("", String::as_str));
            },
        };
        let list = ui.list_view(side, PROFILE_ROW, 0, items);
        ui.add_class(list, "fill");
        ui.set_name(list, "build-profiles");
        let empty = ui.panel(side, Style::default());
        ui.add_class(empty, "build-empty");
        let hint = ui.add_label(empty, "This project has no build profiles yet.");
        ui.add_class(hint, "build-hint");
        let add_defaults = ui.button(empty, "Add the default profiles");
        ui.add_class(add_defaults, "build-button");
        ui.set_name(add_defaults, "build-add-defaults");

        let detail = ui.panel(main, Style::default());
        ui.add_class(detail, "fill");
        ui.add_class(detail, "build-detail");
        let head = ui.panel(detail, Style::row());
        ui.add_class(head, "build-head");
        let title = ui.add_label(head, "");
        ui.add_class(title, "build-title");
        let base = ui.add_label(head, "");
        ui.add_class(base, "build-base");
        let button = |ui: &mut Ui, text: &str, name: &str| {
            let button = ui.button(head, text);
            ui.add_class(button, "build-button");
            ui.set_name(button, name);
            button
        };
        let run = button(ui, "Run", "build-run");
        let reveal = button(ui, "Show in Files", "build-reveal");
        let cancel = button(ui, "Cancel", "build-cancel");
        let build = button(ui, "Build", "build-start");
        ui.add_class(build, "primary");

        let settings = ui.panel(detail, Style::default());
        ui.add_class(settings, "build-settings");
        let controls = SETTINGS
            .iter()
            .map(|&(label, _, key, edit)| {
                let row = ui.panel(settings, Style::row());
                ui.add_class(row, "build-setting");
                let label = ui.add_label(row, label);
                ui.add_class(label, "build-key");
                let control = match edit {
                    Edit::Text(empty) | Edit::List(empty) => {
                        let field = ui.text_field(row, "");
                        ui.set_placeholder(field, empty);
                        field
                    }
                    Edit::Number => ui.text_field(row, ""),
                    Edit::Toggle => ui.toggle(row, "", false),
                    Edit::Choice(options) => {
                        let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
                        ui.dropdown(row, &labels, 0)
                    }
                };
                ui.add_class(control, "build-control");
                ui.set_name(control, &format!("build-{key}"));
                control
            })
            .collect();
        let problem = ui.add_label(settings, "");
        ui.add_class(problem, "build-problem");
        let line = ui.panel(detail, Style::default());
        ui.add_class(line, "divider");

        let chips = ui.panel(detail, Style::row());
        ui.add_class(chips, "build-stages");
        let stages = STAGES
            .iter()
            .map(|title| {
                let chip = ui.panel(chips, Style::row());
                ui.add_class(chip, "build-stage");
                let dot = ui.panel(chip, Style::default());
                ui.add_class(dot, "build-stage-dot");
                ui.add_label(chip, title);
                let time = ui.add_label(chip, "");
                ui.add_class(time, "build-stage-time");
                (chip, time)
            })
            .collect();
        let result = ui.add_label(chips, "");
        ui.add_class(result, "build-result");
        ui.set_name(result, "build-result");
        let progress = ui.panel(detail, Style::default());
        ui.add_class(progress, "build-progress");
        let progress_text = ui.add_label(progress, "");
        ui.add_class(progress_text, "build-progress-text");
        let track = ui.panel(progress, Style::default());
        ui.add_class(track, "build-progress-track");
        let progress_fill = ui.panel(track, Style::default());
        ui.add_class(progress_fill, "build-progress-fill");

        let levels = ui.panel(detail, Style::row());
        ui.add_class(levels, "build-log-levels");
        let log_levels = ["info", "warnings", "errors"].map(|name| {
            let button = ui.button(levels, "");
            ui.add_class(button, "build-log-level");
            ui.set_name(button, &format!("build-log-{name}"));
            ui.set_checked(button, true);
            button
        });
        let log: Rc<RefCell<Vec<(String, String)>>> = Rc::default();
        let log_rows: Rc<RefCell<Vec<usize>>> = Rc::default();
        let (bound, rows) = (log.clone(), log_rows.clone());
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let message = ui.add_label(row, "");
                ui.add_class(message, "build-log-message");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let log = bound.borrow();
                let Some((level, message)) = rows.borrow().get(index).and_then(|&at| log.get(at)) else { return };
                ui.set_class(row, "log-warn", level == "warn");
                ui.set_class(row, "log-error", level == "error");
                ui.set_text(ui.children(row)[0], message);
            },
        };
        let log_list = ui.list_view(detail, LOG_ROW, 0, items);
        ui.add_class(log_list, "fill");
        ui.add_class(log_list, "build-log");
        ui.set_name(log_list, "build-log");

        Self {
            list,
            empty,
            add_defaults,
            title,
            base,
            controls,
            typing: None,
            problem,
            build,
            cancel,
            run,
            reveal,
            stages,
            progress,
            progress_text,
            progress_fill,
            result,
            log_list,
            names,
            log,
            log_rows,
            log_shown: usize::MAX,
            log_levels,
            log_hidden: [false; 3],
            window_focused: true,
            picked: None,
            checked: None,
            running: None,
            times: [None; 3],
            failed: None,
            content: None,
            built: None,
            program: sibling_program("vox"),
        }
    }

    /// Builds with another program than the `vox` beside the editor, which
    /// takes `vox build`'s arguments and speaks its JSON lines.
    pub fn set_program(&mut self, program: PathBuf) {
        self.program = program;
    }

    /// Whether a build is running.
    pub fn is_building(&self) -> bool {
        self.running.is_some()
    }

    /// The build's log so far, as level and message.
    pub fn log(&self) -> Vec<(String, String)> {
        self.log.borrow().clone()
    }

    /// Follows a running build, and, while the window shows, its profiles
    /// and buttons. Returns how a build ended, the frame it does.
    pub fn update(&mut self, ui: &mut Ui, project_root: &Path, shown: bool) -> Option<Finished> {
        let finished = self.follow(ui);
        if shown {
            self.read_profiles(ui, project_root, false);
            self.respond(ui, project_root);
        }
        self.show_state(ui);
        finished
    }

    /// Reads the profiles again, once a second or when `now`, and shows the
    /// picked one's settings.
    fn read_profiles(&mut self, ui: &mut Ui, root: &Path, now: bool) {
        if !now && self.checked.is_some_and(|at| at.elapsed() < PROFILES_CHECK) {
            return;
        }
        self.checked = Some(Instant::now());
        let names = build_profile::profile_names(root);
        if self.picked.as_ref().is_none_or(|picked| !names.contains(picked)) {
            self.picked = build_profile::default_profile(root);
        }
        let index = self.picked.as_ref().and_then(|picked| names.iter().position(|n| n == picked));
        if *self.names.borrow() != names {
            let count = names.len();
            *self.names.borrow_mut() = names;
            ui.set_list_count(self.list, count);
        }
        if ui.selection(self.list) != index.as_slice() {
            ui.set_selection(self.list, index.as_slice());
        }
        ui.set_visible(self.empty, self.names.borrow().is_empty());
        self.show_settings(ui, root);
    }

    /// The picked profile's settings in their controls, as its file and its
    /// bases make them; a field being typed in is left alone.
    fn show_settings(&mut self, ui: &mut Ui, root: &Path) {
        let Some(name) = self.picked.clone() else {
            ui.set_text(self.title, "No profile");
            ui.set_text(self.base, "");
            ui.set_text(self.problem, "");
            return;
        };
        ui.set_text(self.title, &name);
        let base = build_profile::base_of(root, &name).map(|base| format!("based on {base}"));
        ui.set_text(self.base, &base.unwrap_or_default());
        let profile = build_profile::load_profile(root, &name);
        ui.set_text(self.problem, &profile.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default());
        let Ok(profile) = profile else { return };
        let values = toml::Table::try_from(&profile).unwrap_or_default();
        for (index, &(_, table, key, edit)) in SETTINGS.iter().enumerate() {
            let control = self.controls[index];
            if ui.focused() == Some(control) || self.typing == Some(index) {
                continue;
            }
            let value = match table {
                Some(table) => values.get(table).and_then(|t| t.as_table()).and_then(|t| t.get(key)),
                None => values.get(key),
            };
            match edit {
                Edit::Text(_) => ui.set_text(control, value.and_then(|v| v.as_str()).unwrap_or_default()),
                Edit::List(_) => {
                    let items = value
                        .and_then(|v| v.as_array())
                        .map(|items| items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join(", "));
                    ui.set_text(control, &items.unwrap_or_default());
                }
                Edit::Number => {
                    ui.set_text(control, &value.and_then(|v| v.as_integer()).map(|n| n.to_string()).unwrap_or_default())
                }
                Edit::Toggle => ui.set_checked(control, value.and_then(|v| v.as_bool()).unwrap_or_default()),
                Edit::Choice(options) => {
                    let written = value.and_then(|v| v.as_str()).unwrap_or_default();
                    let index = options.iter().position(|(_, value)| *value == written).unwrap_or_default();
                    ui.set_selected_option(control, index);
                }
            }
        }
    }

    /// Writes setting `index` of the picked profile as its control has it,
    /// and shows what the profile now holds. A number that is not one is
    /// not written.
    fn write_setting(&mut self, ui: &mut Ui, root: &Path, index: usize) {
        let Some(name) = self.picked.clone() else { return };
        let (_, table, key, edit) = SETTINGS[index];
        let control = self.controls[index];
        let text = ui.text(control).trim().to_string();
        let value: Option<toml::Value> = match edit {
            Edit::Text(_) => (!text.is_empty()).then(|| text.clone().into()),
            Edit::List(_) => {
                let items: Vec<toml::Value> =
                    text.split(',').map(str::trim).filter(|item| !item.is_empty()).map(Into::into).collect();
                Some(toml::Value::Array(items)).filter(|items| items.as_array().is_some_and(|a| !a.is_empty()))
            }
            Edit::Number if text.is_empty() => None,
            Edit::Number => match text.parse::<i64>() {
                Ok(number) if number > 0 => Some(number.into()),
                _ => {
                    self.show_settings(ui, root);
                    return;
                }
            },
            Edit::Toggle => Some(ui.is_checked(control).into()),
            Edit::Choice(options) => {
                let written = options.get(ui.selected_option(control)).map_or("", |(_, value)| value);
                (!written.is_empty()).then(|| written.into())
            }
        };
        if let Err(error) = build_profile::set_setting(root, &name, table, key, value) {
            self.add_log("error", format!("could not change {name}: {error:#}"));
        }
        self.show_settings(ui, root);
    }

    /// Settings changed in the window: toggles and choices are written at
    /// once, and words once Enter is pressed or the field is left.
    fn respond_settings(&mut self, ui: &mut Ui, root: &Path) {
        for (index, &(_, _, _, edit)) in SETTINGS.iter().enumerate() {
            let control = self.controls[index];
            match edit {
                Edit::Toggle | Edit::Choice(_) if ui.changed(control) => self.write_setting(ui, root, index),
                Edit::Text(_) | Edit::List(_) | Edit::Number => {
                    if ui.changed(control) {
                        self.typing = Some(index);
                    }
                    if ui.submitted(control) && self.typing == Some(index) {
                        self.typing = None;
                        self.write_setting(ui, root, index);
                    }
                }
                _ => {}
            }
        }
        if let Some(index) = self.typing.filter(|&index| ui.focused() != Some(self.controls[index])) {
            self.typing = None;
            self.write_setting(ui, root, index);
        }
    }

    /// What the window's buttons and list ask for.
    fn respond(&mut self, ui: &mut Ui, root: &Path) {
        self.respond_settings(ui, root);
        if ui.selection_changed(self.list) {
            let picked = ui.selection(self.list).first().and_then(|&i| self.names.borrow().get(i).cloned());
            if picked.is_some() {
                self.picked = picked;
                self.show_settings(ui, root);
            }
        }
        if ui.clicked(self.add_defaults) {
            if let Err(error) = build_profile::write_default_profiles(root) {
                self.add_log("error", format!("could not add the default profiles: {error:#}"));
            }
            self.read_profiles(ui, root, true);
        }
        if ui.clicked(self.build) && self.running.is_none() {
            self.start(ui, root);
        }
        if ui.clicked(self.cancel) {
            self.cancel();
        }
        if ui.clicked(self.run)
            && let Some((program, output)) = &self.built
            && let Err(error) = run_player(program, output)
        {
            self.add_log("error", format!("could not run {}: {error}", program.display()));
        }
        if ui.clicked(self.reveal)
            && let Some((_, output)) = &self.built
        {
            crate::project_view::show_in_files(output);
        }
    }

    /// Starts `vox build` with the picked profile.
    fn start(&mut self, ui: &mut Ui, root: &Path) {
        let Some(profile) = self.picked.clone() else { return };
        self.log.borrow_mut().clear();
        // Drawn again from the empty log, counts and all.
        self.log_shown = usize::MAX;
        (self.times, self.failed, self.content, self.built) = ([None; 3], None, None, None);
        let mut command = Command::new(&self.program);
        command
            .arg("build")
            .arg(root)
            .args(["--profile", &profile, "--progress", "json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.add_log("error", format!("could not start {}: {error}", self.program.display()));
                ui.set_text(self.result, "Could not start the build");
                return;
            }
        };
        let (send, lines) = mpsc::channel();
        read_lines(child.stdout.take(), send.clone(), parse);
        read_lines(child.stderr.take(), send, |text| Line::Log { level: "error".into(), message: text.into() });
        let stdin = child.stdin.take();
        let started = Instant::now();
        self.running = Some(Running { child, stdin, lines, profile, started, stage: None, cancelled: None });
        ui.set_text(self.result, "");
    }

    /// Asks the build to stop at its next step.
    fn cancel(&mut self) {
        let Some(running) = &mut self.running else { return };
        if running.cancelled.is_none() {
            running.cancelled = Some(Instant::now());
            if let Some(stdin) = &mut running.stdin {
                let _ = stdin.write_all(b"cancel\n").and_then(|_| stdin.flush());
            }
        }
    }

    /// Takes in what the build has said since the last frame.
    fn follow(&mut self, ui: &mut Ui) -> Option<Finished> {
        let mut running = self.running.take()?;
        let mut done = None;
        loop {
            match running.lines.try_recv() {
                Ok(Line::Stage(index)) => {
                    if let Some((stage, since)) = running.stage {
                        self.times[stage] = Some(since.elapsed());
                    }
                    running.stage = Some((index.min(STAGES.len() - 1), Instant::now()));
                    self.content = None;
                }
                Ok(Line::Progress { done, total, item }) => self.content = Some((done, total, item)),
                Ok(Line::Log { level, message }) => self.add_log(&level, message),
                Ok(Line::Finished(finished)) => done = Some(finished),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Both pipes closed: the process has ended, having said how or not.
                    if done.is_none() {
                        let status = running.child.wait().map(|s| s.to_string()).unwrap_or_default();
                        let cancelled = running.cancelled.is_some();
                        let message = if cancelled { String::new() } else { format!("vox stopped ({status})") };
                        let result = if cancelled { Outcome::Cancelled } else { Outcome::Failed };
                        done = Some(Done { result, seconds: 0.0, message, output: None, program: None, size: 0 });
                    }
                    break;
                }
            }
        }
        if running.cancelled.is_some_and(|at| at.elapsed() > CANCEL_WAIT) {
            let _ = running.child.kill();
        }
        let Some(done) = done else {
            self.running = Some(running);
            return None;
        };
        Some(self.finish(ui, running, done))
    }

    fn finish(&mut self, ui: &mut Ui, mut running: Running, done: Done) -> Finished {
        // Waited on elsewhere, so it does not linger once it ends.
        std::thread::spawn(move || running.child.wait());
        if let Some((stage, since)) = running.stage {
            self.times[stage] = Some(since.elapsed());
            self.failed = (done.result != Outcome::Succeeded).then_some(stage);
        }
        let seconds = if done.seconds > 0.0 { done.seconds } else { running.started.elapsed().as_secs_f64() };
        let (result, text) = match done.result {
            Outcome::Succeeded => {
                let size = done.size as f64 / 1_048_576.0;
                self.built = done.program.zip(done.output);
                let built = format!("Built in {seconds:.1} s · {size:.1} MB");
                (built, format!("Built {} in {seconds:.1} s", running.profile))
            }
            Outcome::Cancelled => ("Cancelled".to_string(), format!("The {} build was cancelled", running.profile)),
            Outcome::Failed => {
                let failed = format!("Failed: {}", done.message);
                (failed, format!("The {} build failed: {}", running.profile, done.message))
            }
        };
        ui.set_text(self.result, &result);
        if !self.window_focused {
            tell_desktop(&text);
        }
        Finished { succeeded: done.result == Outcome::Succeeded, text }
    }

    fn add_log(&mut self, level: &str, message: String) {
        self.log.borrow_mut().push((level.to_string(), message));
    }

    /// Notes whether the editor's window is in front.
    pub fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }

    /// The log lines the level buttons let through, and each button's
    /// count, once the log or the buttons change.
    fn show_log(&mut self, ui: &mut Ui) {
        for (slot, &button) in self.log_levels.iter().enumerate() {
            if ui.clicked(button) {
                self.log_hidden[slot] = !self.log_hidden[slot];
                self.log_shown = usize::MAX;
            }
        }
        let log = self.log.borrow();
        if self.log_shown == log.len() {
            return;
        }
        self.log_shown = log.len();
        let mut counts = [0usize; 3];
        let mut rows = Vec::new();
        for (at, (level, _)) in log.iter().enumerate() {
            let slot = log_slot(level);
            counts[slot] += 1;
            if !self.log_hidden[slot] {
                rows.push(at);
            }
        }
        let count = rows.len();
        *self.log_rows.borrow_mut() = rows;
        for (slot, (&button, label)) in self.log_levels.iter().zip(["Info", "Warnings", "Errors"]).enumerate() {
            ui.set_text(button, &format!("{label} {}", counts[slot]));
            ui.set_checked(button, !self.log_hidden[slot]);
        }
        ui.set_list_count(self.log_list, count);
        ui.refresh_list(self.log_list);
        ui.scroll_to_index(self.log_list, count.saturating_sub(1));
    }

    /// Shows which buttons apply, each stage's state, and the progress.
    fn show_state(&mut self, ui: &mut Ui) {
        self.show_log(ui);
        let running = self.running.as_ref();
        ui.set_visible(self.build, running.is_none());
        ui.set_enabled(self.build, self.picked.is_some());
        ui.set_visible(self.cancel, running.is_some());
        ui.set_enabled(self.cancel, running.is_some_and(|r| r.cancelled.is_none()));
        let built = running.is_none() && self.built.is_some();
        ui.set_visible(self.run, built);
        ui.set_visible(self.reveal, built);
        let current = running.and_then(|r| r.stage);
        for (index, &(chip, time)) in self.stages.iter().enumerate() {
            let now = current.filter(|(stage, _)| *stage == index);
            ui.set_class(chip, "running", now.is_some());
            ui.set_class(chip, "done", self.times[index].is_some() && self.failed != Some(index));
            ui.set_class(chip, "failed", self.failed == Some(index));
            let taken = now.map(|(_, since)| since.elapsed()).or(self.times[index]);
            ui.set_text(time, &taken.map(|t| format!("{:.1} s", t.as_secs_f32())).unwrap_or_default());
        }
        let content = self.content.as_ref().filter(|_| current.is_some_and(|(stage, _)| stage == 1));
        ui.set_visible(self.progress, content.is_some());
        if let Some((done, total, item)) = content {
            ui.set_text(self.progress_text, &format!("{done} of {total} · {item}"));
            let percent = if *total == 0 { 0.0 } else { *done as f32 * 100.0 / *total as f32 };
            ui.set_inline_style(self.progress_fill, &format!("width: {percent:.1}%"));
        }
    }
}

impl Drop for BuildWindow {
    /// A build does not outlive the editor that started it.
    fn drop(&mut self) {
        if let Some(running) = &mut self.running {
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }
}

/// Which level button a log line counts under: info, warnings or errors.
fn log_slot(level: &str) -> usize {
    match level {
        "error" => 2,
        "warn" => 1,
        _ => 0,
    }
}

/// Puts a notification on the desktop, where there is a way to: on Linux,
/// `notify-send`. Nothing happens where there is none.
fn tell_desktop(text: &str) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let shown = Command::new("notify-send").args(["--app-name=Voxear", "Build finished", text]).spawn();
    if let Ok(mut child) = shown {
        // Waited on elsewhere, so it does not linger once it ends.
        std::thread::spawn(move || child.wait());
    }
}

/// Starts a built game, its log going into the editor's own, as the Player
/// category in the Console, until it closes.
fn run_player(program: &Path, folder: &Path) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .current_dir(folder)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let name = program.file_name().unwrap_or_default().to_string_lossy().into_owned();
    tracing::info!(target: "player", "Running {name}");
    let pipes = [(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), false)]
        .into_iter()
        .chain([(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>), true)]);
    for (pipe, errors) in pipes {
        let Some(pipe) = pipe else { continue };
        std::thread::spawn(move || {
            for text in BufReader::new(pipe).lines().map_while(Result::ok) {
                log_player_line(&text, errors);
            }
        });
    }
    std::thread::spawn(move || {
        let status = child.wait().map(|s| s.to_string()).unwrap_or_default();
        tracing::info!(target: "player", "{name} closed ({status})");
    });
    Ok(())
}

/// A line the game printed, into the editor's log at its level.
fn log_player_line(text: &str, errors: bool) {
    let Some((level, message)) = player_line(text, errors) else { return };
    match level {
        Level::ERROR => tracing::error!(target: "player", "{message}"),
        Level::WARN => tracing::warn!(target: "player", "{message}"),
        Level::INFO => tracing::info!(target: "player", "{message}"),
        _ => tracing::debug!(target: "player", "{message}"),
    }
}

/// A line the game printed, as its level and message. The player's log lines
/// start with a time and a level, which are read off, as are any terminal
/// colours; what the game wrote to its errors is an error unless it says
/// otherwise. `None` for a blank line.
fn player_line(text: &str, errors: bool) -> Option<(Level, String)> {
    let plain = without_colours(text);
    let plain = plain.trim();
    if plain.is_empty() {
        return None;
    }
    let mut words = plain.split_whitespace();
    let stamp = words.next().unwrap_or_default();
    let named = words.next().and_then(|level| level.parse::<Level>().ok().map(|parsed| (level, parsed)));
    if let Some((word, level)) = named.filter(|_| stamp.ends_with('Z'))
        && let Some((_, message)) = plain.split_once(word)
    {
        return Some((level, message.trim().to_string()));
    }
    Some((if errors { Level::ERROR } else { Level::INFO }, plain.to_string()))
}

/// Text with a terminal's colour codes (`ESC [ ... m`) taken out.
fn without_colours(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Reads a pipe line by line on a thread of its own, sending each as
/// `line` makes it.
fn read_lines(pipe: Option<impl Read + Send + 'static>, send: Sender<Line>, line: fn(&str) -> Line) {
    let Some(pipe) = pipe else { return };
    std::thread::spawn(move || {
        for text in BufReader::new(pipe).lines().map_while(Result::ok) {
            if !text.trim().is_empty() && send.send(line(&text)).is_err() {
                break;
            }
        }
    });
}

/// One of `vox build --progress json`'s lines; anything else is a log line.
fn parse(text: &str) -> Line {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Line::Log { level: "info".into(), message: text.to_string() };
    };
    let text_of = |key: &str| value[key].as_str().unwrap_or_default().to_string();
    let number = |key: &str| value[key].as_u64().unwrap_or_default() as usize;
    match value["event"].as_str() {
        Some("stage") => Line::Stage(number("index").saturating_sub(1)),
        Some("progress") => Line::Progress { done: number("done"), total: number("total"), item: text_of("item") },
        Some("log") => Line::Log { level: text_of("level"), message: text_of("message") },
        Some("finished") => Line::Finished(Done {
            result: match value["result"].as_str() {
                Some("succeeded") => Outcome::Succeeded,
                Some("cancelled") => Outcome::Cancelled,
                _ => Outcome::Failed,
            },
            seconds: value["seconds"].as_f64().unwrap_or_default(),
            message: text_of("message"),
            output: value["output"].as_str().map(PathBuf::from),
            program: value["program"].as_str().map(PathBuf::from),
            size: value["size"].as_u64().unwrap_or_default(),
        }),
        _ => Line::Log { level: "info".into(), message: text.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_players_lines_keep_their_level() {
        let info =
            "\u{1b}[2m2026-10-31T22:04:40.486699Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m level Levels/Day.level: loaded";
        assert_eq!(player_line(info, false), Some((Level::INFO, "level Levels/Day.level: loaded".into())));
        let warn = "2026-10-31T22:04:41Z  WARN no camera renders this level";
        assert_eq!(player_line(warn, false), Some((Level::WARN, "no camera renders this level".into())));
        assert_eq!(player_line("Unhandled exception.", true), Some((Level::ERROR, "Unhandled exception.".into())));
        assert_eq!(player_line("plain words", false), Some((Level::INFO, "plain words".into())));
        assert_eq!(player_line("   ", false), None);
    }
}
