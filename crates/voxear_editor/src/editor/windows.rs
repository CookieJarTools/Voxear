//! The dock's panels as windows. Each window's options button, at its tab
//! bar's right end, and a context click on a tab or its bar, offer Maximize
//! (or Restore), Close, Close Other Tabs, Float (or Dock, for one floating
//! in a window of its own), another of each window there can be more than
//! one of (as Unity's Add Tab: an Inspector or a Scene view), and Reset
//! Layout. The top bar's Window button lists every panel, ticked while open,
//! to open or close, and resets the layout. Any panel can close, and a
//! closed one keeps its state and opens again beside a panel it shared an
//! area with.
//!
//! Layouts are arrangements of the same panels, picked from the top bar's
//! Layout menu: Default for building levels, Profiling with the Game view
//! over the Profiler, the Entities panel, the Frame Debugger and the Memory
//! panel, and any saved with Save Layout As. Each keeps the arrangement it
//! was left in, as Blender's workspaces do, and Reset Layout puts it back as
//! it started. The editor's memory keeps them, and the one showing, between
//! sessions. A layout that names Scene views or Inspectors opened from a
//! window's menu opens them as it shows, each Scene view looking from where
//! it was left and shaded as it was, and those it does not name go for good,
//! as closing them does.

use std::time::{Duration, Instant};

use voxear_ui::{MenuItem, UiError};

use super::{Editor, LAYOUT};
use crate::settings::SavedLayout;

/// How often the layout is looked at, to keep it once it has changed.
const LAYOUT_CHECK: Duration = Duration::from_secs(1);

/// The layouts every project has, with the arrangements they start with.
const BUILT_IN: [(&str, &str); 2] = [
    ("Default", LAYOUT),
    (
        "Profiling",
        "row(240, [hierarchy], column(45%, row(-300, [game, scene], [inspector]), [profiler, entities, frames, memory, console]))",
    ),
];

/// A layout: its name, the arrangement it starts with (and goes back to on
/// Reset Layout), and the one it was left in while another shows.
pub(super) struct Layout {
    name: String,
    start: String,
    kept: Option<String>,
}

/// What a window's menu does to the panel it was opened for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WindowAction {
    /// Gives it the whole dock, or puts the layout back.
    Maximize,
    Close,
    CloseOthers,
    /// Floats it in a window of its own, or docks it again.
    Float,
    /// Opens another Inspector beside it.
    AddInspector,
    /// Opens another Scene view beside it.
    AddSceneView,
    ResetLayout,
}

/// The built-in layouts, as they start.
pub(super) fn built_in_layouts() -> Vec<Layout> {
    BUILT_IN.iter().map(|&(name, start)| Layout { name: name.into(), start: start.into(), kept: None }).collect()
}

impl Editor {
    pub(super) fn update_windows(&mut self) {
        let dock = self.nodes.dock;
        let asked = self.ui.dock_menu_clicked(dock).or_else(|| self.ui.dock_tab_context_clicked(dock));
        if let Some((panel, at)) = asked {
            self.open_window_menu(panel, at);
        }
        if let Some(choice) = self.ui.menu_choice(dock) {
            let (panel, actions) = self.window_menu.take().unwrap_or_default();
            if let Some(Some(action)) = actions.get(choice) {
                self.window_action(&panel, action.clone());
            }
        }

        let windows = self.nodes.windows;
        if self.ui.clicked(windows) {
            let [x, y, width, height] = self.ui.rect(windows);
            let mut items: Vec<MenuItem> =
                self.ui.dock_panels(dock).iter().map(|(_, title, open)| MenuItem::new(title).checked(*open)).collect();
            items.extend([MenuItem::separator(), MenuItem::new("Reset Layout")]);
            self.ui.open_menu(windows, (x + width, y + height + 4.0), &items);
        }
        if let Some(choice) = self.ui.menu_choice(windows) {
            match self.ui.dock_panels(dock).get(choice) {
                Some((id, _, true)) => self.ui.close_dock_panel(dock, id),
                Some((id, _, false)) => self.open_panel(&id.clone()),
                None => self.reset_layout(),
            }
        }
    }

    /// A window's menu, from its options button or a context click on its
    /// tab or tab bar: Maximize (or Restore), Close, Close Other Tabs, Float
    /// (or Dock), another of each window there can be more than one of, and
    /// Reset Layout. Closed panels open from the top bar's Window button.
    fn open_window_menu(&mut self, panel: String, at: (f32, f32)) {
        let dock = self.nodes.dock;
        let others = !self.ui.dock_panel_neighbours(dock, &panel).is_empty();
        let floating = self.ui.dock_panel_floating(dock, &panel);
        let maximized = self.maximized.is_some();
        let mut items = vec![
            (MenuItem::new(if maximized { "Restore" } else { "Maximize" }), Some(WindowAction::Maximize)),
            (MenuItem::new("Close"), Some(WindowAction::Close)),
            (MenuItem::new("Close Other Tabs").enabled(others), Some(WindowAction::CloseOthers)),
            (MenuItem::new(if floating { "Dock" } else { "Float" }), Some(WindowAction::Float)),
            (MenuItem::separator(), None),
        ];
        items.extend([
            (MenuItem::new("Add Inspector"), Some(WindowAction::AddInspector)),
            (MenuItem::new("Add Scene View"), Some(WindowAction::AddSceneView)),
            (MenuItem::separator(), None),
        ]);
        items.push((MenuItem::new("Reset Layout"), Some(WindowAction::ResetLayout)));
        let (menu, actions): (Vec<MenuItem>, Vec<Option<WindowAction>>) = items.into_iter().unzip();
        self.ui.open_menu(dock, at, &menu);
        self.window_menu = Some((panel, actions));
    }

    /// Does what a window's menu picked, for the panel it was opened for.
    fn window_action(&mut self, panel: &str, action: WindowAction) {
        let dock = self.nodes.dock;
        match action {
            WindowAction::Maximize => self.toggle_maximized(panel),
            // An Inspector or Scene view opened from the menu goes for good; any other waits closed.
            WindowAction::Close if self.remove_inspector(panel) || self.remove_scene_view(panel) => {}
            WindowAction::Close => self.ui.close_dock_panel(dock, panel),
            WindowAction::CloseOthers => {
                for other in self.ui.dock_panel_neighbours(dock, panel) {
                    if !self.remove_inspector(&other) && !self.remove_scene_view(&other) {
                        self.ui.close_dock_panel(dock, &other);
                    }
                }
            }
            WindowAction::Float if self.ui.dock_panel_floating(dock, panel) => self.ui.dock_floating_panel(dock, panel),
            WindowAction::Float => self.ui.float_dock_panel(dock, panel, None),
            WindowAction::AddInspector => {
                self.add_inspector(panel);
            }
            WindowAction::AddSceneView => {
                self.add_scene_view(panel);
            }
            WindowAction::ResetLayout => self.reset_layout(),
        }
    }

    /// Gives the whole dock to one panel by closing the others, or puts back
    /// the layout from before.
    pub(super) fn toggle_maximized(&mut self, panel: &str) {
        let dock = self.nodes.dock;
        if let Some((layout, closed)) = self.maximized.take() {
            let _ = self.ui.set_dock_layout(dock, &layout);
            for id in closed {
                self.ui.close_dock_panel(dock, &id);
            }
            return;
        }
        let panels = self.ui.dock_panels(dock);
        let closed = panels.iter().filter(|(_, _, open)| !open).map(|(id, ..)| id.clone()).collect();
        self.maximized = Some((self.ui.dock_layout(dock), closed));
        for (id, _, open) in panels {
            if open && id != panel {
                self.ui.close_dock_panel(dock, &id);
            }
        }
    }

    /// Opens a panel by id where the dock last had it. The Build window,
    /// which is wide, opens by the Scene view the first time rather than in
    /// a side column.
    pub(super) fn open_panel(&mut self, id: &str) {
        match id {
            "build" => self.ui.open_dock_panel_beside(self.nodes.dock, id, "scene"),
            _ => self.ui.open_dock_panel(self.nodes.dock, id),
        }
    }

    /// Puts every panel back where the layout showing starts with them.
    pub(super) fn reset_layout(&mut self) {
        let start = self.layouts[self.layout].start.clone();
        if self.arrange(&start).is_err() {
            self.arrange(LAYOUT).expect("the default layout is valid");
        }
    }

    /// Arranges the dock as `layout` says, closing the panels it leaves out.
    /// The Scene views and Inspectors opened from a window's menu that it
    /// names open first, and those it leaves out go for good, as closing
    /// them from the menu does.
    fn arrange(&mut self, layout: &str) -> Result<(), UiError> {
        let named = opened_from_menu(layout);
        for &panel in &named {
            match panel.starts_with("scene-") {
                true => self.reopen_scene_view(panel),
                false => self.reopen_inspector(panel),
            }
        }
        let dock = self.nodes.dock;
        self.ui.set_dock_layout_closing_rest(dock, layout)?;
        let panels = self.ui.dock_panels(dock).into_iter().map(|(id, ..)| id);
        let left_out: Vec<String> =
            panels.filter(|id| is_opened_from_menu(id) && !named.contains(&id.as_str())).collect();
        for panel in left_out {
            let _ = self.remove_inspector(&panel) || self.remove_scene_view(&panel);
        }
        Ok(())
    }

    /// The top bar's Layout menu: the layouts to pick from, ticked for the
    /// one showing, then Save Layout As, Delete Layout for one saved, and
    /// Reset Layout. Save Layout As asks for a name in a box under the menu's
    /// button; Enter or Save saves, and Escape or a click elsewhere does not.
    pub(super) fn update_layouts(&mut self) {
        let button = self.nodes.layouts;
        if self.ui.clicked(button) {
            let [x, y, _, height] = self.ui.rect(button);
            let saved = self.layout >= BUILT_IN.len();
            let mut items: Vec<MenuItem> = self
                .layouts
                .iter()
                .enumerate()
                .map(|(index, layout)| MenuItem::new(&layout.name).checked(index == self.layout))
                .collect();
            items.extend([
                MenuItem::separator(),
                MenuItem::new("Save Layout As…"),
                MenuItem::new("Delete Layout").enabled(saved),
                MenuItem::new("Reset Layout"),
            ]);
            self.ui.open_menu(button, (x, y + height + 4.0), &items);
        }
        if let Some(choice) = self.ui.menu_choice(button) {
            let count = self.layouts.len();
            match choice {
                index if index < count => self.switch_layout(index),
                // The separator is counted among the items.
                index if index == count + 1 => self.ask_layout_name(),
                index if index == count + 2 => self.delete_layout(),
                _ => self.reset_layout(),
            }
        }
        self.update_layout_prompt();
    }

    /// Shows the layout called `name`, keeping the one left as it is. False
    /// when there is none by that name.
    pub fn use_layout(&mut self, name: &str) -> bool {
        let Some(index) = self.layouts.iter().position(|layout| layout.name == name) else { return false };
        self.switch_layout(index);
        true
    }

    /// The name of the layout showing.
    pub fn layout_name(&self) -> &str {
        &self.layouts[self.layout].name
    }

    /// Keeps the layout showing as it now is under `name`, as a new layout
    /// or in place of a saved one by that name, and shows it.
    pub fn save_layout_as(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if BUILT_IN.iter().any(|(built_in, _)| built_in.eq_ignore_ascii_case(name)) {
            return self.notify(&format!("{name} is built in; save the layout under another name"));
        }
        let now = self.ui.dock_layout(self.nodes.dock);
        match self.layouts.iter().position(|layout| layout.name == name) {
            Some(index) => {
                self.layouts[index].start = now;
                self.layout = index;
            }
            None => {
                self.layouts.push(Layout { name: name.to_string(), start: now, kept: None });
                self.layout = self.layouts.len() - 1;
            }
        }
        self.ui.set_text(self.nodes.layout_name, name);
        self.notify(&format!("Saved the layout as {name}"));
    }

    fn switch_layout(&mut self, index: usize) {
        if index == self.layout || index >= self.layouts.len() {
            return;
        }
        // Full screen belongs to the layout being left: it is put back first.
        if self.maximized.is_some() {
            self.toggle_maximized("");
        }
        let dock = self.nodes.dock;
        self.layouts[self.layout].kept = Some(self.ui.dock_layout(dock));
        self.layout = index;
        let kept = self.layouts[index].kept.take();
        if kept.is_none_or(|layout| self.arrange(&layout).is_err()) {
            self.reset_layout();
        }
        let name = self.layouts[index].name.clone();
        self.ui.set_text(self.nodes.layout_name, &name);
    }

    /// Forgets the saved layout showing, and shows Default.
    fn delete_layout(&mut self) {
        if self.layout < BUILT_IN.len() {
            return;
        }
        let gone = self.layouts.remove(self.layout).name;
        self.layout = 0;
        let kept = self.layouts[0].kept.take();
        if kept.is_none_or(|layout| self.arrange(&layout).is_err()) {
            self.reset_layout();
        }
        let name = self.layouts[0].name.clone();
        self.ui.set_text(self.nodes.layout_name, &name);
        self.notify(&format!("Deleted the layout {gone}"));
    }

    fn ask_layout_name(&mut self) {
        let (prompt, field) = (self.nodes.layout_prompt, self.nodes.layout_prompt_name);
        let [x, y, _, height] = self.ui.rect(self.nodes.layouts);
        self.ui.set_inline_style(prompt, &format!("left: {x}px; top: {}px", y + height + 4.0));
        self.ui.set_visible(prompt, true);
        self.ui.set_text(field, "");
        // Focused once it is laid out, which it is not while hidden.
        self.naming_layout = true;
    }

    /// Saves under the name typed when Enter or Save is pressed, and closes
    /// the box when its field loses the focus, as to a click elsewhere.
    fn update_layout_prompt(&mut self) {
        let (prompt, field) = (self.nodes.layout_prompt, self.nodes.layout_prompt_name);
        if !self.ui.is_visible(prompt) {
            return;
        }
        if self.ui.submitted(field) || self.ui.clicked(self.nodes.layout_prompt_save) {
            let name = self.ui.text(field);
            self.ui.set_visible(prompt, false);
            return self.save_layout_as(&name);
        }
        if self.naming_layout {
            self.ui.focus(field);
            self.naming_layout = self.ui.focused() != Some(field);
        } else if self.ui.focused() != Some(field) && !self.ui.is_pressed(prompt) {
            self.ui.set_visible(prompt, false);
        }
    }

    /// Puts back the layouts and the one showing that the editor was closed
    /// with, opening the Scene views and Inspectors it names that were
    /// opened from a window's menu. A layout that cannot be read starts
    /// again.
    pub(super) fn restore_layout(&mut self) {
        self.layouts = built_in_layouts();
        let saved = self.memory.saved_layouts.iter().filter(|saved| !saved.name.is_empty());
        self.layouts.extend(saved.map(|saved| Layout {
            name: saved.name.clone(),
            start: saved.start.clone(),
            kept: None,
        }));
        for (layout, kept) in self.layouts.iter_mut().zip(&self.memory.layouts) {
            layout.kept = Some(kept.clone()).filter(|kept| !kept.is_empty());
        }
        self.layout = self.memory.workspace.min(self.layouts.len() - 1);
        let name = self.layouts[self.layout].name.clone();
        self.ui.set_text(self.nodes.layout_name, &name);
        let kept = self.layouts[self.layout].kept.take();
        if kept.is_none_or(|layout| self.arrange(&layout).is_err()) {
            self.reset_layout();
        }
    }

    /// Keeps the layouts, the one showing, and the Scene views opened from a
    /// window's menu that they name, in the editor's memory once they
    /// change, looking once a second. A panel shown full screen is not kept.
    pub(super) fn keep_layout(&mut self) {
        if self.maximized.is_some() || self.layout_checked.elapsed() < LAYOUT_CHECK {
            return;
        }
        self.layout_checked = Instant::now();
        let showing = self.ui.dock_layout(self.nodes.dock);
        let arrangements: Vec<String> = self
            .layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| match index == self.layout {
                true => showing.clone(),
                false => layout.kept.clone().unwrap_or_default(),
            })
            .collect();
        let saved: Vec<SavedLayout> = self.layouts[BUILT_IN.len()..]
            .iter()
            .map(|layout| SavedLayout { name: layout.name.clone(), start: layout.start.clone() })
            .collect();
        let starts = saved.iter().map(|layout| layout.start.as_str());
        let named: Vec<&str> =
            arrangements.iter().map(String::as_str).chain(starts).flat_map(opened_from_menu).collect();
        let scene_views = self.kept_scene_views(&named);
        let memory = &self.memory;
        let changed = memory.workspace != self.layout || memory.layouts != arrangements;
        if changed || memory.saved_layouts != saved || memory.scene_views != scene_views {
            (self.memory.workspace, self.memory.layouts) = (self.layout, arrangements);
            (self.memory.saved_layouts, self.memory.scene_views) = (saved, scene_views);
            self.memory.save();
        }
    }
}

/// The panels a layout names that were opened from a window's menu: Scene
/// views and Inspectors past the first, as `scene-2` and `inspector-2`.
fn opened_from_menu(layout: &str) -> Vec<&str> {
    layout.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').filter(|id| is_opened_from_menu(id)).collect()
}

fn is_opened_from_menu(panel: &str) -> bool {
    let number = panel.strip_prefix("scene-").or_else(|| panel.strip_prefix("inspector-"));
    number.and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| n >= 2)
}
