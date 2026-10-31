//! The Scene view's header: the shading menu (Lit, Unlit or Wireframe), the
//! Gizmos button, which shows or hides lights, cameras and what the selected
//! one reaches (see `gizmos`), and full screen, which gives the dock to the
//! Scene view alone until it is pressed again. Shading and Gizmos are
//! settings, so they are kept.

use voxear_render::{DEBUG_VIEW_NONE, DEBUG_VIEW_UNLIT, DEBUG_VIEW_WIREFRAME};

use super::Editor;

impl Editor {
    /// The header's controls. Returns whether a setting changed.
    pub(super) fn update_scene_view(&mut self) -> bool {
        let settings = &mut self.memory.settings;
        let mut changed = false;
        if self.ui.changed(self.nodes.shading) {
            settings.scene_shading = self.ui.selected_option(self.nodes.shading);
            changed = true;
        }
        if self.ui.clicked(self.nodes.gizmos) {
            settings.show_gizmos = !settings.show_gizmos;
            changed = true;
        }
        if self.ui.clicked(self.nodes.scene_maximize) {
            self.toggle_maximized("scene");
        }
        self.ui.set_checked(self.nodes.scene_maximize, self.maximized.is_some());
        changed
    }

    /// The debug view the Scene view's shading asks for.
    pub(super) fn scene_debug_view(&self) -> u32 {
        match self.memory.settings.scene_shading {
            1 => DEBUG_VIEW_UNLIT,
            2 => DEBUG_VIEW_WIREFRAME,
            _ => DEBUG_VIEW_NONE,
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
}
