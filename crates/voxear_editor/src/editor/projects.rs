//! Several Project panels, as Unity has: the first, docked from the start,
//! and any more opened from a window's menu (Add Project), each a panel of
//! its own beside the one it was opened from, showing a folder of its own,
//! so two folders can be open side by side to drag between. Each runs the
//! same frame in turn: while one does, it is the editor's `project`, so what
//! it asks (an item shown in the Inspector, a file renamed, a model opened
//! out) goes to that one, and files that came, went or were renamed show in
//! every one. A ping, and showing where a file is, go to the first. One
//! opened from the menu goes for good when it closes. Each is built from
//! Project.vui.

use std::collections::HashMap;

use voxear_render::Renderer;
use voxear_ui::{DragSource, ImageRef, NodeId, Ui};

use super::{Editor, script_namespace};
use crate::project_view::ProjectView;

/// What every Project panel is built from.
const MARKUP: &str = include_str!("../../ui/Project.vui");

/// A Project panel opened from a window's menu: its panel's id, and itself.
pub(super) struct ExtraProject {
    panel: String,
    project: ProjectView,
}

/// The markup every Project panel's elements are found in, built in a dock
/// panel's `content`.
pub(super) fn build(ui: &mut Ui, content: NodeId) -> NodeId {
    let mut load = |path: &str| Err(format!("{path} is not built in"));
    ui.load_markup(content, "Project.vui", MARKUP, &mut load).expect("the Project panel's markup is built in")
}

impl Editor {
    /// Opens another Project panel beside `beside`. Returns its panel.
    pub fn add_project_panel(&mut self, beside: &str) -> String {
        let taken = |n: u32| self.extra_projects.iter().any(|extra| extra.panel == format!("project-{n}"));
        let panel = format!("project-{}", (2..).find(|&n| !taken(n)).expect("there is always a free number"));
        self.open_project_panel(&panel, beside);
        panel
    }

    /// Opens the Project panel a layout names as `panel` (`project-2`),
    /// unless it is open.
    pub(super) fn reopen_project_panel(&mut self, panel: &str) {
        if !self.extra_projects.iter().any(|extra| extra.panel == panel) {
            self.open_project_panel(panel, "project");
        }
    }

    fn open_project_panel(&mut self, panel: &str, beside: &str) {
        let content = self.ui.add_dock_panel_beside(self.nodes.dock, panel, "Project", beside);
        self.ui.add_class(content, "dock-content");
        let document = build(&mut self.ui, content);
        let (name, root) = (self.game.project.name().to_string(), self.game.project.root.clone());
        let namespace = script_namespace(&self.game.project);
        let mut project = ProjectView::new(&mut self.ui, document, &name, &root, self.game.content.db(), &namespace);
        project.set_prefab_parts(&mut self.ui, self.memory.settings.prefab_parts);
        self.extra_projects.push(ExtraProject { panel: panel.to_string(), project });
    }

    /// Takes a Project panel opened from the menu away for good, as closing
    /// it does. False for a panel that is not one.
    pub(super) fn remove_project_panel(&mut self, panel: &str) -> bool {
        let Some(index) = self.extra_projects.iter().position(|extra| extra.panel == panel) else { return false };
        self.extra_projects.remove(index);
        self.ui.remove_dock_panel(self.nodes.dock, panel);
        true
    }

    /// How many Project panels there are, the first included.
    pub fn project_panel_count(&self) -> usize {
        1 + self.extra_projects.len()
    }

    /// Runs every Project panel's frame: the first's, then each other's,
    /// swapped in as the editor's `project` while it runs.
    pub(super) fn update_projects(&mut self, renderer: &mut Renderer, delta: f32) {
        self.project_frame(renderer, delta);
        let mut index = 0;
        while index < self.extra_projects.len() {
            std::mem::swap(&mut self.project, &mut self.extra_projects[index].project);
            self.project_frame(renderer, delta);
            std::mem::swap(&mut self.project, &mut self.extra_projects[index].project);
            index += 1;
        }
    }

    /// One Project panel's frame: what it asks, and its folders taking an
    /// Obj dragged from a Hierarchy, to make a prefab in.
    fn project_frame(&mut self, renderer: &mut Renderer, delta: f32) {
        self.project.fade_ping(&mut self.ui, delta);
        for event in self.project.update(&mut self.ui) {
            self.project_event(event, renderer);
        }
        let making = self.ui.dragging().is_some_and(|source| self.dragged_obj(&source).is_some());
        self.project.take_drops(&mut self.ui, making);
    }

    /// Lists the project's files again in every Project panel but the
    /// editor's `project`, as they are after files came, went or were renamed.
    pub(super) fn refresh_other_projects(&mut self) {
        let (ui, db) = (&mut self.ui, self.game.content.db());
        for extra in &mut self.extra_projects {
            extra.project.refresh(ui, db, None);
        }
    }

    /// The file being dragged from any Project panel, by its path, when one is.
    pub(super) fn dragged_asset(&self, source: &DragSource) -> Option<String> {
        let extras = self.extra_projects.iter().map(|extra| &extra.project);
        std::iter::once(&self.project).chain(extras).find_map(|project| project.dragged(source))
    }

    /// The folder under a point in any Project panel, as a drop from
    /// outside lands in it.
    pub(super) fn folder_at(&self, point: [f32; 2]) -> Option<String> {
        let extras = self.extra_projects.iter().map(|extra| &extra.project);
        std::iter::once(&self.project).chain(extras).find_map(|project| project.folder_at(&self.ui, point))
    }

    /// Shows a prefab's parts in every Project panel, or not, as the setting says.
    pub(super) fn set_prefab_parts_shown(&mut self, on: bool) {
        let extras = self.extra_projects.iter_mut().map(|extra| &mut extra.project);
        for project in std::iter::once(&mut self.project).chain(extras) {
            project.set_prefab_parts(&mut self.ui, on);
        }
    }

    /// What each Project panel shows that has a preview, the editor's
    /// `project` first, with how big its tiles are.
    pub(super) fn project_previews_wanted(&self) -> Vec<(f32, Vec<String>)> {
        let extras = self.extra_projects.iter().map(|extra| &extra.project);
        let projects = std::iter::once(&self.project).chain(extras);
        projects.map(|project| (project.preview_size(), project.previewable(&self.ui))).collect()
    }

    /// Shows these previews in the Project panel at `index`, the editor's
    /// `project` being the first.
    pub(super) fn set_project_previews(&mut self, index: usize, shown: HashMap<String, ImageRef>) {
        let project = match index {
            0 => &mut self.project,
            _ => match self.extra_projects.get_mut(index - 1) {
                Some(extra) => &mut extra.project,
                None => return,
            },
        };
        project.set_previews(&mut self.ui, shown);
    }
}
