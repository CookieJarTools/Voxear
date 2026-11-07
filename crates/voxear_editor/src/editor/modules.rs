//! Switching the project's modules on and off from Project Settings: the
//! choice is written into `voxear.toml` and put into effect at once.

use voxear_engine::modules::Module;

use super::Editor;

impl Editor {
    pub(super) fn update_modules(&mut self) {
        if let Some((module, on)) = self.project_settings.changed(&self.ui) {
            self.set_module(module, on);
        }
        if let Some(module) = self.inspector.module_wanted(&self.ui) {
            self.set_module(module, true);
        }
    }

    /// Switches a module on or off for the project.
    pub fn set_module(&mut self, module: Module, on: bool) {
        if let Err(error) = self.game.project.set_module(module, on) {
            tracing::error!("{error:#}");
        }
        let modules = self.game.project.modules();
        self.game.engine.world.insert_resource(modules);
        self.project_settings.show(&mut self.ui, &modules);
        self.files_seen();
        // The scripts build against the modules' assemblies, so they build again with the new set.
        self.rebuild_scripts = true;
        // Components of a module switched off or on show it at once.
        let (shown, playing) = (self.inspector.shown(), self.is_playing());
        self.inspector.show(&mut self.ui, &self.game, shown, playing);
    }
}
