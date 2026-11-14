//! Project Settings: switching the project's modules on and off, its
//! quality settings, its floating origin and its block worlds' budget. Each
//! choice is written into `voxear.toml` at once; a module, a quality setting
//! and the budget are put into effect at once, the floating origin the next
//! time the game plays.

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
        let quality = self.project_settings.quality_changed(&self.ui, &self.game.project.file.quality);
        if let Some(quality) = quality {
            self.set_quality(&quality);
        }
        let in_use = voxear_terrain::WorldBudget::of(&self.game.engine.world);
        if let Some(budget) = self.project_settings.budget_changed(&self.ui, &in_use) {
            self.set_world_budget(&budget);
        }
        if let Some(keep) = self.project_settings.machine_changed(&self.ui) {
            self.keep_world_budget(keep);
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

    /// Sets what the project lets the renderer spend (see
    /// `voxear_engine::QualitySettings`), from now on.
    pub fn set_quality(&mut self, quality: &voxear_engine::QualitySettings) {
        if let Err(error) = self.game.project.set_quality(quality) {
            tracing::error!("{error:#}");
        }
        let quality = self.game.project.file.quality.clone();
        quality.put(&mut self.game.engine.world);
        self.project_settings.show_quality(&mut self.ui, &quality);
    }

    /// Sets what making worlds of blocks may spend (see
    /// `voxear_terrain::WorldBudget`), from now on: worlds streaming take it
    /// up as they go, with nothing made again. Into this machine's own where
    /// it keeps one, and else into the project's `voxear.toml`.
    pub fn set_world_budget(&mut self, budget: &voxear_terrain::WorldBudget) {
        use voxear_terrain::MachineBudget;
        let world = &mut self.game.engine.world;
        if MachineBudget::of(world).kept {
            budget.put(world);
            if let Err(error) = MachineBudget::keep(world, true) {
                tracing::error!("keeping this machine's world quality: {error}");
            }
        } else {
            if let Err(error) = self.game.project.set_world(budget) {
                tracing::error!("{error:#}");
            }
            let project = self.game.project.file.world;
            MachineBudget::open(world, project, MachineBudget::of(world).file);
        }
        self.show_world_budget();
    }

    /// Keeps the budget in use for this machine, beside the game's saves,
    /// in place of the project's; or forgets it and goes back to the
    /// project's.
    pub fn keep_world_budget(&mut self, keep: bool) {
        if let Err(error) = voxear_terrain::MachineBudget::keep(&mut self.game.engine.world, keep) {
            tracing::error!("keeping this machine's world quality: {error}");
        }
        self.show_world_budget();
    }

    /// Shows the budget in use, and whether it is this machine's own.
    pub(super) fn show_world_budget(&mut self) {
        let world = &self.game.engine.world;
        let (budget, machine) = (voxear_terrain::WorldBudget::of(world), voxear_terrain::MachineBudget::of(world));
        self.project_settings.show_budget(&mut self.ui, &budget);
        self.project_settings.show_machine(&mut self.ui, machine.kept);
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
