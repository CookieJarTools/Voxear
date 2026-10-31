//! The dock's panels as windows. A context click on a panel's tab offers
//! Close, Close Other Tabs and Reset Layout; the top bar's Window button
//! lists every panel, ticked while open, to open or close, and resets the
//! layout. A closed panel keeps its state, and opens again beside a panel it
//! shared an area with.
//!
//! The top bar's workspaces are layouts of the same panels: World for
//! building levels, and Profiling, with the Game view over the Profiler.
//! Each keeps the layout it was left in, as Blender's workspaces do, and
//! the editor's memory keeps them and the workspace showing between
//! sessions.

use std::time::{Duration, Instant};

use voxear_ui::MenuItem;

use super::{Editor, LAYOUT};

/// How often the layout is looked at, to keep it once it has changed.
const LAYOUT_CHECK: Duration = Duration::from_secs(1);

/// The layout each workspace starts with, in the top bar's order.
const WORKSPACE_LAYOUTS: [&str; 2] =
    [LAYOUT, "row(240, [hierarchy], column(45%, row(-300, [game, scene], [inspector]), [profiler, console]))"];

impl Editor {
    pub(super) fn update_windows(&mut self) {
        let dock = self.nodes.dock;
        if let Some((panel, at)) = self.ui.dock_tab_context_clicked(dock) {
            let others = !self.ui.dock_panel_neighbours(dock, &panel).is_empty();
            let items = [
                MenuItem::new("Close"),
                MenuItem::new("Close Other Tabs").enabled(others),
                MenuItem::separator(),
                MenuItem::new("Reset Layout"),
            ];
            self.ui.open_menu(dock, at, &items);
            self.tab_menu = Some(panel);
        }
        if let Some(choice) = self.ui.menu_choice(dock) {
            let panel = self.tab_menu.take().unwrap_or_default();
            match choice {
                0 => self.ui.close_dock_panel(dock, &panel),
                1 => {
                    for other in self.ui.dock_panel_neighbours(dock, &panel) {
                        self.ui.close_dock_panel(dock, &other);
                    }
                }
                _ => self.reset_layout(),
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

    /// Opens a panel by id where the dock last had it. The Build window,
    /// which is wide, opens by the Scene view the first time rather than in
    /// a side column.
    pub(super) fn open_panel(&mut self, id: &str) {
        match id {
            "build" => self.ui.open_dock_panel_beside(self.nodes.dock, id, "scene"),
            _ => self.ui.open_dock_panel(self.nodes.dock, id),
        }
    }

    /// Puts every panel back where the workspace starts with them.
    pub(super) fn reset_layout(&mut self) {
        let layout = WORKSPACE_LAYOUTS[self.workspace];
        self.ui.set_dock_layout_closing_rest(self.nodes.dock, layout).expect("the workspaces' layouts are valid");
    }

    /// Changes to the workspace picked in the top bar, keeping the layout of
    /// the one left for when it is picked again.
    pub(super) fn update_workspaces(&mut self) {
        let picked = self.ui.segment(self.nodes.workspaces).min(WORKSPACE_LAYOUTS.len() - 1);
        if picked == self.workspace {
            return;
        }
        // Full screen belongs to the layout being left: it is put back first.
        if self.maximized.is_some() {
            self.toggle_maximized("");
        }
        let dock = self.nodes.dock;
        self.workspace_layouts[self.workspace] = Some(self.ui.dock_layout(dock));
        self.workspace = picked;
        let kept = self.workspace_layouts[picked].take();
        if kept.is_none_or(|layout| self.ui.set_dock_layout_closing_rest(dock, &layout).is_err()) {
            self.reset_layout();
        }
    }

    /// The workspace showing: 0 for World, 1 for Profiling.
    pub fn workspace(&self) -> usize {
        self.workspace
    }

    /// Puts back the workspace and layouts the editor was closed with. A
    /// layout naming panels this version does not have is left for the
    /// workspace's own.
    pub(super) fn restore_layout(&mut self) {
        let last = WORKSPACE_LAYOUTS.len() - 1;
        for (index, layout) in self.memory.layouts.iter().enumerate().take(last + 1) {
            self.workspace_layouts[index] = Some(layout.clone()).filter(|layout| !layout.is_empty());
        }
        self.workspace = self.memory.workspace.min(last);
        self.ui.set_segment(self.nodes.workspaces, self.workspace);
        let kept = self.workspace_layouts[self.workspace].take();
        if kept.is_none_or(|layout| self.ui.set_dock_layout_closing_rest(self.nodes.dock, &layout).is_err()) {
            self.reset_layout();
        }
    }

    /// Keeps the workspace and layouts in the editor's memory once they
    /// change, looking once a second. A panel shown full screen is not kept.
    pub(super) fn keep_layout(&mut self) {
        if self.maximized.is_some() || self.layout_checked.elapsed() < LAYOUT_CHECK {
            return;
        }
        self.layout_checked = Instant::now();
        let showing = self.ui.dock_layout(self.nodes.dock);
        let layouts: Vec<String> = (0..WORKSPACE_LAYOUTS.len())
            .map(|index| match index == self.workspace {
                true => showing.clone(),
                false => self.workspace_layouts[index].clone().unwrap_or_default(),
            })
            .collect();
        if self.memory.workspace != self.workspace || self.memory.layouts != layouts {
            (self.memory.workspace, self.memory.layouts) = (self.workspace, layouts);
            self.memory.save();
        }
    }
}
