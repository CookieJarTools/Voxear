//! The console of a game window with its tools (`vox run`, and development
//! builds), on the key left of 1, as games have had since Quake: the log as
//! it comes, and a line to run commands from. A line runs a script's
//! `[Command]` method by name with its arguments, or reads or sets a
//! `[ConsoleVariable]`; `help` lists them, `clear` empties the log shown,
//! and `quit` closes the game. Up and Down go back through what was typed,
//! and Tab completes a name. What a line does is logged, so the console
//! shows it as it shows everything else.

use std::{cell::RefCell, rc::Rc};

use tracing::Level;
use voxear_core::logging;
use voxear_ui::{ItemFns, Key, NodeId, Ui};

use crate::Engine;

/// Lines the console keeps to show; older ones go first.
const KEPT: usize = 2_000;
/// What the console takes itself, before the scripts.
const OWN: [(&str, &str); 3] =
    [("help", "Lists what the console takes"), ("clear", "Empties the log shown here"), ("quit", "Closes the game")];

/// What a line asked of the game window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleAction {
    Quit,
}

/// A line shown: what it says, and how it looks.
#[derive(Clone, Debug)]
struct Shown {
    text: String,
    class: &'static str,
}

pub struct DevConsole {
    panel: NodeId,
    list: NodeId,
    input: NodeId,
    lines: Rc<RefCell<Vec<Shown>>>,
    /// How many lines the log had when last read.
    seen: u64,
    history: Vec<String>,
    /// Where Up has gone back to in the history, when it has.
    back: Option<usize>,
}

impl DevConsole {
    /// The console in a document with its markup: `console`, `console-lines`
    /// and `console-input`. It starts hidden, showing what was logged before.
    pub fn new(ui: &mut Ui, document: NodeId) -> Self {
        let find = |ui: &Ui, name: &str| ui.find(document, name).unwrap_or_else(|| panic!("no {name} in the markup"));
        let (panel, list, input) = (find(ui, "console"), find(ui, "console-lines"), find(ui, "console-input"));
        let lines: Rc<RefCell<Vec<Shown>>> = Rc::default();
        let rows = lines.clone();
        let make = |ui: &mut Ui, row: NodeId| {
            let label = ui.add_label(row, "");
            ui.add_class(label, "console-line");
        };
        let bind = move |ui: &mut Ui, row: NodeId, index: usize| {
            let rows: std::cell::Ref<Vec<Shown>> = rows.borrow();
            let (Some(line), Some(&label)) = (rows.get(index), ui.children(row).first()) else { return };
            ui.set_text(label, &line.text);
            for class in ["info", "warning", "error", "typed"] {
                ui.set_class(label, class, class == line.class);
            }
        };
        ui.bind_list(list, 0, ItemFns { make, bind });
        ui.set_visible(panel, false);
        Self { panel, list, input, lines, seen: 0, history: Vec::new(), back: None }
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        ui.is_visible(self.panel)
    }

    /// Opens it, ready to type, or closes it.
    pub fn toggle(&mut self, ui: &mut Ui) {
        let open = !self.is_open(ui);
        ui.set_visible(self.panel, open);
        if open {
            ui.focus(self.input);
        } else {
            ui.blur();
        }
    }

    /// Shows what was logged since the last frame, and runs a line when Enter
    /// is pressed on it, against `engine`'s scripts when there is one.
    pub fn update(&mut self, ui: &mut Ui, engine: Option<&mut Engine>) -> Option<ConsoleAction> {
        self.read_log(ui);
        if !self.is_open(ui) || !ui.submitted(self.input) {
            return None;
        }
        let line = ui.text(self.input).trim().to_string();
        ui.set_text(self.input, "");
        ui.focus(self.input);
        if line.is_empty() {
            return None;
        }
        self.history.retain(|typed| *typed != line);
        self.history.push(line.clone());
        self.back = None;
        self.run(ui, &line, engine)
    }

    /// Keys the console takes while open: Up and Down for what was typed,
    /// Tab to complete a name, and Escape to close. Returns whether it took
    /// the key.
    pub fn key(&mut self, ui: &mut Ui, key: Key, engine: Option<&Engine>) -> bool {
        if !self.is_open(ui) {
            return false;
        }
        match key {
            Key::Up | Key::Down => {
                let back = match (key, self.back) {
                    (Key::Up, None) => self.history.len().checked_sub(1),
                    (Key::Up, Some(at)) => Some(at.saturating_sub(1)),
                    (_, Some(at)) if at + 1 < self.history.len() => Some(at + 1),
                    _ => None,
                };
                self.back = back;
                let text = back.map_or_else(String::new, |at| self.history[at].clone());
                ui.set_text(self.input, &text);
                true
            }
            Key::Tab => {
                self.complete(ui, engine);
                true
            }
            Key::Escape => {
                self.toggle(ui);
                true
            }
            _ => false,
        }
    }

    /// Runs a line from outside the window, as `vox console` sends one, and
    /// returns what it logged, warnings and errors marked, with what it asks
    /// of the window.
    pub fn run_remote(&mut self, ui: &mut Ui, line: &str, engine: &mut Engine) -> (Vec<String>, Option<ConsoleAction>) {
        let total = || logging::recent().lock().map_or(0, |log| log.total);
        let before = total();
        let action = self.run(ui, line, Some(engine));
        let recent = logging::recent();
        let Ok(log) = recent.lock() else { return (Vec::new(), action) };
        let new = (log.total - before).min(log.lines.len() as u64) as usize;
        let echo = format!("› {line}");
        let said =
            log.lines.iter().skip(log.lines.len() - new).filter(|l| !(l.target == "console" && l.message == echo));
        let lines = said
            .flat_map(|logged| {
                let mark = match logged.level {
                    Level::ERROR => "error: ",
                    Level::WARN => "warning: ",
                    _ => "",
                };
                logged.message.lines().map(move |text| format!("{mark}{text}")).collect::<Vec<_>>()
            })
            .collect();
        (lines, action)
    }

    fn run(&mut self, ui: &mut Ui, line: &str, engine: Option<&mut Engine>) -> Option<ConsoleAction> {
        let word = line.split_whitespace().next().unwrap_or_default().to_lowercase();
        // Cleared is empty, so clear is not echoed.
        if word == "clear" {
            self.lines.borrow_mut().clear();
            ui.set_list_count(self.list, 0);
            return None;
        }
        tracing::info!(target: "console", "› {line}");
        match word.as_str() {
            "quit" => return Some(ConsoleAction::Quit),
            "help" => OWN.iter().for_each(|(name, help)| tracing::info!(target: "console", "{name}  {help}")),
            _ => {}
        }
        let ran = match engine {
            Some(engine) => engine.run_console(line),
            None => Ok(false),
        };
        match ran {
            Ok(true) => {}
            Ok(false) if word == "help" => {}
            Ok(false) => tracing::warn!(target: "console", "nothing is called {word}; help lists what there is"),
            Err(error) => tracing::error!(target: "console", "{error:#}"),
        }
        None
    }

    /// Completes the first word to the name it can only be, or to as much as
    /// the names it could be share, which it lists.
    fn complete(&mut self, ui: &mut Ui, engine: Option<&Engine>) {
        let typed = ui.text(self.input);
        if typed.contains(char::is_whitespace) {
            return;
        }
        let scripts = engine.and_then(Engine::scripts).map(|s| s.console_names()).unwrap_or_default();
        let names = OWN.iter().map(|(name, _)| name.to_string()).chain(scripts.into_iter().map(|n| n.name));
        let typed_lower = typed.to_lowercase();
        let mut matching: Vec<String> = names.filter(|name| name.to_lowercase().starts_with(&typed_lower)).collect();
        matching.sort();
        matching.dedup();
        let Some(first) = matching.first().cloned() else { return };
        if matching.len() == 1 {
            ui.set_text(self.input, &format!("{first} "));
            return;
        }
        let shared = matching.iter().fold(first, |shared, name| common_prefix(&shared, name));
        if shared.len() > typed.len() {
            ui.set_text(self.input, &shared);
        }
        tracing::info!(target: "console", "{}", matching.join("  "));
    }

    /// Adds what was logged since it last looked, keeping the newest in view.
    fn read_log(&mut self, ui: &mut Ui) {
        let recent = logging::recent();
        let Ok(log) = recent.lock() else { return };
        if log.total == self.seen {
            return;
        }
        let new = (log.total - self.seen).min(log.lines.len() as u64) as usize;
        self.seen = log.total;
        let mut lines = self.lines.borrow_mut();
        for line in log.lines.iter().skip(log.lines.len() - new) {
            let class = match line.level {
                Level::ERROR => "error",
                Level::WARN => "warning",
                _ if line.target == "console" && line.message.starts_with('›') => "typed",
                _ => "info",
            };
            lines.extend(line.message.lines().map(|text| Shown { text: text.to_string(), class }));
        }
        let over = lines.len().saturating_sub(KEPT);
        lines.drain(..over);
        let count = lines.len();
        drop(lines);
        ui.set_list_count(self.list, count);
        ui.scroll_to_index(self.list, count.saturating_sub(1));
    }
}

fn common_prefix(a: &str, b: &str) -> String {
    a.chars().zip(b.chars()).take_while(|(x, y)| x.eq_ignore_ascii_case(y)).map(|(x, _)| x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats_overlay::{MARKUP, SHEET};

    /// The console reads the game's one log, so its tests take turns.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn console() -> (Ui, DevConsole) {
        let mut ui = Ui::new();
        let root = ui.root();
        let mut load = |_: &str| Ok(SHEET.to_string());
        let document = ui.load_markup(root, "StatsOverlay.vui", MARKUP, &mut load).unwrap();
        let console = DevConsole::new(&mut ui, document);
        (ui, console)
    }

    /// Types a line and presses Enter, then lets the console read the log.
    fn enter(ui: &mut Ui, console: &mut DevConsole, line: &str) -> Option<ConsoleAction> {
        ui.focus(console.input);
        ui.set_text(console.input, line);
        ui.key_down(Key::Enter);
        let action = console.update(ui, None);
        console.update(ui, None);
        action
    }

    fn shown(console: &DevConsole) -> Vec<String> {
        console.lines.borrow().iter().map(|line| line.text.clone()).collect()
    }

    #[test]
    fn lines_run_and_what_they_did_shows() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // The game's log keeps its lines where the console reads them.
        let _log = tracing::subscriber::set_default(logging::capturing_only(logging::recent()));
        let (mut ui, mut console) = console();
        console.toggle(&mut ui);
        assert!(console.is_open(&ui));

        enter(&mut ui, &mut console, "help");
        let lines = shown(&console);
        assert!(lines.contains(&"› help".to_string()), "what was typed shows: {lines:?}");
        assert!(lines.contains(&"clear  Empties the log shown here".to_string()), "{lines:?}");
        enter(&mut ui, &mut console, "jump");
        assert!(shown(&console).iter().any(|l| l.contains("nothing is called jump")));
        assert_eq!(console.lines.borrow().last().map(|l| l.class), Some("warning"));

        // Up goes back through what was typed; Tab completes a name.
        assert!(console.key(&mut ui, Key::Up, None));
        assert_eq!(ui.text(console.input), "jump");
        console.key(&mut ui, Key::Up, None);
        assert_eq!(ui.text(console.input), "help");
        console.key(&mut ui, Key::Down, None);
        assert_eq!(ui.text(console.input), "jump");
        ui.set_text(console.input, "cl");
        console.key(&mut ui, Key::Tab, None);
        assert_eq!(ui.text(console.input), "clear ");

        enter(&mut ui, &mut console, "clear");
        assert!(shown(&console).is_empty(), "clear empties what shows");
        assert_eq!(enter(&mut ui, &mut console, "quit"), Some(ConsoleAction::Quit));
        assert!(console.key(&mut ui, Key::Escape, None));
        assert!(!console.is_open(&ui), "Escape closes it");
    }

    #[test]
    fn a_line_from_vox_gives_back_what_it_logged() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _log = tracing::subscriber::set_default(logging::capturing_only(logging::recent()));
        let (mut ui, mut console) = console();
        let mut engine = Engine::new();
        let (said, action) = console.run_remote(&mut ui, "help", &mut engine);
        assert!(said.contains(&"quit  Closes the game".to_string()), "{said:?}");
        assert!(!said.iter().any(|line| line.starts_with('›')), "not the line itself: {said:?}");
        assert_eq!(action, None);
        let (said, _) = console.run_remote(&mut ui, "jump", &mut engine);
        assert!(said.contains(&"warning: nothing is called jump; help lists what there is".to_string()), "{said:?}");
        assert_eq!(console.run_remote(&mut ui, "quit", &mut engine).1, Some(ConsoleAction::Quit));
    }
}
