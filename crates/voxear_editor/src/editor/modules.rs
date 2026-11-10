//! Project Settings: switching the project's modules on and off, and its
//! floating origin. Each choice is written into `voxear.toml` at once; a
//! module is put into effect at once, the floating origin the next time the
//! game plays.

use voxear_engine::modules::Module;

use super::Editor;

impl Editor {
    pub(super) fn update_modules(&mut self) {
        self.project_settings.switch_tabs(&mut self.ui);
        if let Some((module, on)) = self.project_settings.changed(&self.ui) {
            self.set_module(module, on);
        }
        let origin = self.project_settings.origin_changed(&self.ui, &self.game.project.file.floating_origin);
        if let Some(origin) = origin {
            self.set_floating_origin(&origin);
        }
        if let Some(module) = self.inspector.module_wanted(&self.ui) {
            self.set_module(module, true);
        }
    }

    /// Sets when and how far the world's middle moves with the camera while
    /// the game plays (see `voxear_engine::origin`).
    pub fn set_floating_origin(&mut self, origin: &voxear_engine::origin::OriginSettings) {
        if let Err(error) = self.game.project.set_floating_origin(origin) {
            tracing::error!("{error:#}");
        }
        self.project_settings.show_origin(&mut self.ui, &self.game.project.file.floating_origin);
    }

    /// Switches a module on or off for the project.
    pub fn set_module(&mut self, module: Module, on: bool) {
        if let Err(error) = self.game.project.set_module(module, on) {
            tracing::error!("{error:#}");
        }
        let modules = self.game.project.modules();
        voxear_engine::modules::put(&mut self.game.engine.world, modules);
        self.project_settings.show(&mut self.ui, &modules);
        self.files_seen();
        // The scripts build against the modules' assemblies, so they build again with the new set.
        self.rebuild_scripts = true;
        // Components of a module switched off or on show it at once.
        let (shown, playing) = (self.inspector.shown(), self.is_playing());
        self.inspector.show(&mut self.ui, &self.game, shown, playing);
    }
}
