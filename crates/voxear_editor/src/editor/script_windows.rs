//! Windows written in the game's C# (`EditorWindow` in Voxear.Core), each a
//! panel of the dock opened from any window's menu, as many times as wanted,
//! beside the window the menu was for, each its own instance. A window is
//! opened in the scripts once they have started, which give the markup its
//! panel is built from, and is updated every frame its panel shows, while
//! editing and while playing, with the editor's UI lent to answer what it
//! asks of its elements, by numbers of its own from 2²⁶ up. One the scripts
//! let go of, as when they reload, is opened again. Closing one closes it
//! in the scripts too, and it goes for good. A layout that names one opens
//! it again with the layout, as the editor's memory keeps which window each
//! panel was.

use voxear_ecs::World;
use voxear_engine::{Game, game_ui::UiNumbers};
use voxear_scripting::{ScriptRuntime, WindowUpdate};
use voxear_ui::{NodeId, Ui};

use super::{
    Editor,
    play::{Mode, Play},
};
use crate::{field_drawers::DrawerUi, settings::KeptScriptWindow};

/// The first number the windows' documents and elements are known by, far
/// from the game UI's and the drawers'.
const FIRST_NUMBER: u32 = 1 << 26;

/// A game's window that is open: its panel, the window's type, the number
/// the scripts know it by, and what its panel holds.
pub(super) struct ScriptWindow {
    panel: String,
    type_name: String,
    number: u32,
    content: NodeId,
    /// The document its markup became, once opened in the scripts.
    document: Option<u32>,
    /// It threw while opening, so it waits for the scripts to change.
    failed: bool,
}

pub(super) struct ScriptWindows {
    open: Vec<ScriptWindow>,
    numbers: UiNumbers,
    next: u32,
}

impl Default for ScriptWindows {
    fn default() -> Self {
        Self { open: Vec::new(), numbers: UiNumbers::starting_at(FIRST_NUMBER), next: 1 }
    }
}

impl Editor {
    /// The game's windows there are, each by its type and its title.
    pub fn script_window_types(&self) -> Vec<(String, String)> {
        self.inspector.types().windows.iter().map(|window| (window.name.clone(), window.title())).collect()
    }

    /// Opens another of the game's windows of type `type_name` beside
    /// `beside`. Returns its panel, or none for a type the scripts have not.
    pub fn add_script_window(&mut self, type_name: &str, beside: &str) -> Option<String> {
        let taken = |n: u32| self.script_windows.open.iter().any(|window| window.panel == format!("window-{n}"));
        let panel = format!("window-{}", (1..).find(|&n| !taken(n)).expect("there is always a free number"));
        self.open_script_window(&panel, type_name, beside).then_some(panel)
    }

    /// Opens the game's window a layout names as `panel` (`window-2`), as
    /// the memory says it was, unless it is open.
    pub(super) fn reopen_script_window(&mut self, panel: &str) {
        if self.script_windows.open.iter().any(|window| window.panel == panel) {
            return;
        }
        let kept = self.memory.script_windows.iter().find(|kept| kept.panel == panel).cloned();
        if let Some(kept) = kept {
            self.open_script_window(panel, &kept.window, "inspector");
        }
    }

    fn open_script_window(&mut self, panel: &str, type_name: &str, beside: &str) -> bool {
        let types = self.inspector.types();
        let Some(window) = types.windows.iter().find(|window| window.name == type_name) else { return false };
        let (title, icon) = (window.title(), window.icon.clone());
        let content = self.ui.add_dock_panel_beside(self.nodes.dock, panel, &title, beside);
        self.ui.add_class(content, "dock-content");
        if let Some(icon) = icon {
            self.ui.set_dock_panel_icon(self.nodes.dock, panel, &icon);
        }
        say(&mut self.ui, content, "Waiting for the scripts to start…");
        let number = self.script_windows.next;
        self.script_windows.next += 1;
        let opened = ScriptWindow {
            panel: panel.to_string(),
            type_name: type_name.to_string(),
            number,
            content,
            document: None,
            failed: false,
        };
        self.script_windows.open.push(opened);
        true
    }

    /// Takes one of the game's windows away for good, closing it in the
    /// scripts too. False for a panel that is not one.
    pub fn remove_script_window(&mut self, panel: &str) -> bool {
        let windows = &mut self.script_windows;
        let Some(index) = windows.open.iter().position(|window| window.panel == panel) else { return false };
        let window = windows.open.remove(index);
        if let Some(document) = window.document {
            windows.numbers.close(document);
            if let Some((scripts, world)) = window_scripts(&mut self.play, &mut self.game)
                && let Err(error) = scripts.close_window(world, window.number)
            {
                tracing::error!(target: "script", "{error:#}");
            }
        }
        self.ui.remove_dock_panel(self.nodes.dock, panel);
        true
    }

    /// How many of the game's windows are open.
    pub fn script_window_count(&self) -> usize {
        self.script_windows.open.len()
    }

    /// The game's windows open, as the editor's memory keeps them.
    pub(super) fn kept_script_windows(&self, named: &[&str]) -> Vec<KeptScriptWindow> {
        let open = self.script_windows.open.iter();
        let mut kept: Vec<KeptScriptWindow> = open
            .map(|window| KeptScriptWindow { panel: window.panel.clone(), window: window.type_name.clone() })
            .collect();
        let left = self.memory.script_windows.iter().filter(|old| !kept.iter().any(|window| window.panel == old.panel));
        let left: Vec<KeptScriptWindow> = left.cloned().collect();
        kept.extend(left);
        kept.retain(|window| named.contains(&window.panel.as_str()));
        kept
    }

    /// Opens each window waiting to be in the scripts, and updates those
    /// that show.
    pub(super) fn update_script_windows(&mut self) {
        if self.script_windows.open.is_empty() {
            return;
        }
        let unopened = self.script_windows.open.iter().any(|window| window.document.is_none() && !window.failed);
        if unopened && matches!(self.play.mode, Mode::Editing) {
            // Starts the scripts while editing, as drawers do, when they have not started.
            self.with_edit_scripts(|_, _| ());
        }
        self.open_waiting_windows();
        let shown: Vec<(u32, u32)> = self
            .script_windows
            .open
            .iter()
            .filter(|window| self.shown(window.content))
            .filter_map(|window| Some((window.number, window.document?)))
            .collect();
        if shown.is_empty() {
            return;
        }
        let Some((scripts, world)) = window_scripts(&mut self.play, &mut self.game) else { return };
        let mut services = DrawerUi::new(&mut self.ui, &mut self.script_windows.numbers);
        let updates = match scripts.update_windows(world, &mut services, &shown) {
            Ok(updates) => updates,
            Err(error) => return tracing::error!(target: "script", "{error:#}"),
        };
        for (&(number, _), update) in shown.iter().zip(updates) {
            let Some(window) = self.script_windows.open.iter_mut().find(|window| window.number == number) else {
                continue;
            };
            match update {
                WindowUpdate::Updated => {}
                // Opened again next frame, from the scripts as they are now.
                WindowUpdate::Gone => {
                    if let Some(document) = window.document.take() {
                        self.script_windows.numbers.close(document);
                    }
                    say(&mut self.ui, window.content, "Waiting for the scripts to start…");
                }
                WindowUpdate::Threw(error) => {
                    let said = format!("{} threw: {error}", window.type_name);
                    self.notify(&said);
                }
            }
        }
    }

    /// Opens in the scripts each window not yet opened there, and builds
    /// its markup in its panel.
    fn open_waiting_windows(&mut self) {
        for index in 0..self.script_windows.open.len() {
            let window = &self.script_windows.open[index];
            if window.document.is_some() || window.failed {
                continue;
            }
            let (number, type_name) = (window.number, window.type_name.clone());
            let Some((scripts, world)) = window_scripts(&mut self.play, &mut self.game) else { return };
            let opened = scripts.open_window(world, number, &type_name);
            let content = self.script_windows.open[index].content;
            for child in self.ui.children(content) {
                self.ui.remove(child);
            }
            let store = &mut self.game.content;
            let mut load = |src: &str| store.read_text(src).map_err(|error| format!("{error:#}"));
            let built = opened
                .map_err(|error| format!("{error:#}"))
                .and_then(|markup| self.ui.load_markup(content, &type_name, &markup, &mut load).map_err(|e| e.0));
            for warning in self.ui.take_warnings() {
                tracing::warn!(target: "script", "{warning}");
            }
            let window = &mut self.script_windows.open[index];
            match built {
                Ok(root) => {
                    self.ui.set_style(root, voxear_ui::Style::column());
                    window.document = Some(self.script_windows.numbers.add_document(root));
                }
                Err(error) => {
                    tracing::error!(target: "script", "{type_name}: {error}");
                    window.failed = true;
                    say(&mut self.ui, content, &format!("{type_name} could not open: {error}"));
                }
            }
        }
    }

    /// Lets each window that failed to open try again, as when the scripts
    /// changed.
    pub(super) fn retry_script_windows(&mut self) {
        for window in &mut self.script_windows.open {
            window.failed = false;
        }
    }
}

/// The scripts the game's windows run in, with their world: those of the
/// edit mode while editing, once started, and the game's while it plays.
fn window_scripts<'a>(play: &'a mut Play, game: &'a mut Game) -> Option<(&'a mut ScriptRuntime, &'a mut World)> {
    match play.mode {
        Mode::Editing => Some((play.edit_scripts()?, &mut game.engine.world)),
        Mode::Playing(_) => game.engine.scripts_and_world(),
        Mode::Building(_) => None,
    }
}

/// A panel's only line, while its window waits or after it failed.
fn say(ui: &mut Ui, content: NodeId, text: &str) {
    for child in ui.children(content) {
        ui.remove(child);
    }
    let label = ui.add_label(content, text);
    ui.add_class(label, "placeholder");
}
