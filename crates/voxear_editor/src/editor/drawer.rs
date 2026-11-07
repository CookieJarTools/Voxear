//! The rail's drawer: Levels, Lighting, Project Settings or Settings, which open over the left of the
//! dock from the rail's buttons, a third of the window wide. The same button,
//! or the drawer's close button, closes it.

use super::Editor;

/// What the drawer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawer {
    Levels,
    Lighting,
    Project,
    Settings,
}

impl Editor {
    pub(super) fn update_drawer(&mut self) {
        let nodes = &self.nodes;
        let clicked = if self.ui.clicked(nodes.rail_levels) {
            Some(Drawer::Levels)
        } else if self.ui.clicked(nodes.rail_lighting) {
            Some(Drawer::Lighting)
        } else if self.ui.clicked(nodes.rail_project) {
            Some(Drawer::Project)
        } else if self.ui.clicked(nodes.rail_settings) {
            Some(Drawer::Settings)
        } else {
            None
        };
        if let Some(drawer) = clicked {
            self.open_drawer((self.drawer != Some(drawer)).then_some(drawer));
        }
        if self.ui.clicked(self.nodes.drawer_close) {
            self.open_drawer(None);
        }
    }

    /// Opens the drawer on a page, or closes it with `None`.
    pub fn open_drawer(&mut self, drawer: Option<Drawer>) {
        self.drawer = drawer;
        let nodes = &self.nodes;
        self.ui.set_visible(nodes.drawer, drawer.is_some());
        self.ui.set_visible(nodes.drawer_levels, drawer == Some(Drawer::Levels));
        self.ui.set_visible(nodes.drawer_lighting, drawer == Some(Drawer::Lighting));
        self.ui.set_visible(nodes.drawer_project, drawer == Some(Drawer::Project));
        self.ui.set_visible(nodes.drawer_settings, drawer == Some(Drawer::Settings));
        self.ui.set_checked(nodes.rail_levels, drawer == Some(Drawer::Levels));
        self.ui.set_checked(nodes.rail_lighting, drawer == Some(Drawer::Lighting));
        self.ui.set_checked(nodes.rail_project, drawer == Some(Drawer::Project));
        self.ui.set_checked(nodes.rail_settings, drawer == Some(Drawer::Settings));
        let title = match drawer {
            Some(Drawer::Levels) => "Levels",
            Some(Drawer::Lighting) => "Lighting",
            Some(Drawer::Project) => "Project Settings",
            Some(Drawer::Settings) => "Settings",
            None => "",
        };
        self.ui.set_text(nodes.drawer_title, title);
        if drawer == Some(Drawer::Lighting) {
            self.refresh_lighting_page();
        }
    }

    /// What the drawer shows, if it is open.
    pub fn drawer(&self) -> Option<Drawer> {
        self.drawer
    }

    /// Shows a panel by id: Levels, Lighting, Project Settings and Settings in the drawer, and the rest
    /// in the dock, opened again if it was closed.
    pub(super) fn show_panel(&mut self, id: &str) {
        match id {
            "levels" => self.open_drawer(Some(Drawer::Levels)),
            "lighting" => self.open_drawer(Some(Drawer::Lighting)),
            "project" => self.open_drawer(Some(Drawer::Project)),
            "settings" => self.open_drawer(Some(Drawer::Settings)),
            _ => self.open_panel(id),
        }
    }
}
