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
use voxear_engine::{build_profile, player_build::sibling_program};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

/// How long a cancelled build has to stop before its process is stopped.
const CANCEL_WAIT: Duration = Duration::from_secs(10);
/// How often the profiles are read again while the window shows, to see
/// ones added or changed outside the editor.
const PROFILES_CHECK: Duration = Duration::from_secs(1);
const PROFILE_ROW: f32 = 26.0;
const LOG_ROW: f32 = 20.0;
/// What the window shows of the picked profile, in order.
const SETTINGS: [&str; 10] = [
    "Platform",
    "Based on",
    "Start level",
    "Levels",
    "Development",
    "Defines",
    "Product name",
    "Version",
    "Window",
    "Output",
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
    /// Each setting's value, in [`SETTINGS`] order.
    values: Vec<NodeId>,
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
    /// How many log lines the list shows.
    log_shown: usize,
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
        let values = SETTINGS
            .iter()
            .map(|key| {
                let row = ui.panel(settings, Style::row());
                ui.add_class(row, "build-setting");
                let key = ui.add_label(row, key);
                ui.add_class(key, "build-key");
                let value = ui.add_label(row, "");
                ui.add_class(value, "build-value");
                value
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

        let log: Rc<RefCell<Vec<(String, String)>>> = Rc::default();
        let bound = log.clone();
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let message = ui.add_label(row, "");
                ui.add_class(message, "build-log-message");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let log = bound.borrow();
                let Some((level, message)) = log.get(index) else { return };
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
            values,
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
            log_shown: 0,
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

    fn show_settings(&mut self, ui: &mut Ui, root: &Path) {
        let Some(name) = self.picked.clone() else {
            ui.set_text(self.title, "No profile");
            self.values.iter().for_each(|&value| ui.set_text(value, ""));
            ui.set_text(self.problem, "");
            return;
        };
        ui.set_text(self.title, &name);
        let (profile, problem) = match build_profile::load_profile(root, &name) {
            Ok(profile) => (Some(profile), String::new()),
            Err(error) => (None, format!("{error:#}")),
        };
        ui.set_text(self.problem, &problem);
        let Some(profile) = profile else {
            self.values.iter().for_each(|&value| ui.set_text(value, ""));
            return;
        };
        // The profile's own base, which loading folds away.
        let base = std::fs::read_to_string(build_profile::profile_path(root, &name))
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|table| table.get("base").and_then(|base| base.as_str()).map(str::to_string));
        let or = |value: &Option<String>, otherwise: &str| value.clone().unwrap_or_else(|| otherwise.to_string());
        let player = &profile.player;
        let window = match player.window.as_str() {
            "fullscreen" => "Full screen".to_string(),
            _ => format!("Windowed, {} × {}", player.width, player.height),
        };
        let development = match (profile.development.enabled, profile.development.validation_layers) {
            (false, _) => "Off".to_string(),
            (true, false) => "On: Debug scripts and the tools".to_string(),
            (true, true) => "On, with Vulkan validation".to_string(),
        };
        let output = profile.output_dir(Path::new(""), &name).display().to_string();
        let shown = [
            profile.platform().to_string(),
            or(&base, "None"),
            or(&profile.levels.start, "The project's"),
            if profile.levels.include.is_empty() { "Every level".into() } else { profile.levels.include.join(", ") },
            development,
            if profile.scripting.defines.is_empty() { "None".into() } else { profile.scripting.defines.join(", ") },
            or(&player.product_name, "The project's name"),
            or(&profile.version.version, "Not set"),
            window,
            output,
        ];
        for (&value, text) in self.values.iter().zip(shown) {
            ui.set_text(value, &text);
        }
    }

    /// What the window's buttons and list ask for.
    fn respond(&mut self, ui: &mut Ui, root: &Path) {
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
        {
            match Command::new(program).current_dir(output).spawn() {
                // Waited on elsewhere, so it does not linger once it ends.
                Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
                Err(error) => self.add_log("error", format!("could not run {}: {error}", program.display())),
            }
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
        self.log_shown = 0;
        ui.set_list_count(self.log_list, 0);
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
        Finished { succeeded: done.result == Outcome::Succeeded, text }
    }

    fn add_log(&mut self, level: &str, message: String) {
        self.log.borrow_mut().push((level.to_string(), message));
    }

    /// Shows which buttons apply, each stage's state, and the progress.
    fn show_state(&mut self, ui: &mut Ui) {
        let count = self.log.borrow().len();
        if self.log_shown != count {
            self.log_shown = count;
            ui.set_list_count(self.log_list, count);
            ui.scroll_to_index(self.log_list, count.saturating_sub(1));
        }
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
