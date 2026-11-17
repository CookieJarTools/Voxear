//! The rail's drawer: Levels or Lighting, which open over the left of the
//! dock from the rail's buttons, a third of the window wide. The same button,
//! or the drawer's close button, closes it. The rail's Project Settings and
//! Preferences buttons open those windows, in the dock.

use super::Editor;

/// What the drawer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawer {
    Levels,
    Lighting,
}

impl Editor {
    pub(super) fn update_drawer(&mut self) {
        let nodes = &self.nodes;
        let clicked = if self.ui.clicked(nodes.rail_levels) {
            Some(Drawer::Levels)
        } else if self.ui.clicked(nodes.rail_lighting) {
            Some(Drawer::Lighting)
        } else {
            None
        };
        if self.ui.clicked(nodes.rail_project) {
            self.open_drawer(None);
            self.show_panel("project-settings");
        }
        if self.ui.clicked(self.nodes.rail_settings) {
            self.open_drawer(None);
            self.show_panel("preferences");
        }
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
        self.ui.set_checked(nodes.rail_levels, drawer == Some(Drawer::Levels));
        self.ui.set_checked(nodes.rail_lighting, drawer == Some(Drawer::Lighting));
        let title = match drawer {
            Some(Drawer::Levels) => "Levels",
            Some(Drawer::Lighting) => "Lighting",
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

    /// Shows a panel by id: Levels and Lighting in the drawer, and the rest
    /// in the dock, opened again if it was closed. "settings" is
    /// Preferences, by its older name.
    pub(super) fn show_panel(&mut self, id: &str) {
        match id {
            "levels" => self.open_drawer(Some(Drawer::Levels)),
            "lighting" => self.open_drawer(Some(Drawer::Lighting)),
            "settings" => self.open_panel("preferences"),
            _ => self.open_panel(id),
        }
    }
}
