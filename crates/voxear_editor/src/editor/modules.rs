//! Project Settings: switching the project's modules on and off, its
//! quality settings, its Portals' view distance, its floating origin, its
//! block worlds' budget and its languages' fonts. Each choice is written
//! into `voxear.toml` at once; a module, a quality setting, the view
//! distance, the budget and the fonts are put into effect at once, the
//! floating origin the next time the game plays.

use voxear_assets::AssetKind;
use voxear_engine::modules::Module;
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    fields::{self, Pick},
    object_picker::{self, PickFor},
    project_settings::FontAction,
};

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
        let view = self.project_settings.portal_view_changed(&self.ui, self.game.project.file.portals.view_distance);
        if let Some(distance) = view {
            self.set_portal_view_distance(distance);
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
        match self.project_settings.font_action(&self.ui) {
            Some(FontAction::Pick { language, at, anchor }) => self.browse_language_font(&language, at, anchor),
            Some(FontAction::Remove { language, at }) => {
                let mut fonts = self.language_fonts(&language);
                if at < fonts.len() {
                    fonts.remove(at);
                    self.set_language_fonts(&language, &fonts);
                }
            }
            None => {}
        }
    }

    /// The fonts a language's text falls back to, by GUID or path, in order.
    fn language_fonts(&self, language: &str) -> Vec<String> {
        self.game.project.file.localization.fonts.get(language).cloned().unwrap_or_default()
    }

    /// Sets the fonts a language's text falls back to, by GUID or path, in
    /// order, saved in `voxear.toml` and drawn in at once.
    pub fn set_language_fonts(&mut self, language: &str, fonts: &[String]) {
        if let Err(error) = self.game.project.set_language_fonts(language, fonts) {
            return self.notify(&format!("Could not save the fonts: {error:#}"));
        }
        self.files_seen();
        self.game.load_strings();
        self.show_language_fonts();
    }

    /// Shows each of the project's languages with its fonts on the
    /// Localization tab of Project Settings, each by its file's name.
    pub(super) fn show_language_fonts(&mut self) {
        let localization = &self.game.project.file.localization;
        let mut languages = localization.languages.clone();
        if languages.is_empty() {
            languages.push("en".to_string());
        }
        // A language with fonts that is no longer listed still shows, so its fonts can be taken out.
        languages.extend(localization.fonts.keys().filter(|code| !localization.languages.contains(code)).cloned());
        let db = self.game.content.db();
        let name = |reference: &String| {
            let path = reference.parse().ok().and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
            let path = path.unwrap_or_else(|| reference.clone());
            path.rsplit('/').next().unwrap_or(&path).to_string()
        };
        let rows: Vec<(String, Vec<String>)> = languages
            .into_iter()
            .map(|code| {
                let fonts = localization.fonts.get(&code).map(|fonts| fonts.iter().map(name).collect());
                (code, fonts.unwrap_or_default())
            })
            .collect();
        self.project_settings.show_language_fonts(&mut self.ui, &rows);
    }

    /// Opens the object picker under `anchor` for a language's font, in
    /// place of the one at `at`, or to add.
    fn browse_language_font(&mut self, language: &str, at: Option<usize>, anchor: NodeId) {
        let choices = object_picker::decorate(&self.game, fields::assets_of(&self.game, Some(AssetKind::Font)));
        let target = PickFor::LanguageFont { language: language.to_string(), at };
        self.picker.open(&mut self.ui, (target, anchor), "Select Font", choices, "");
    }

    /// Puts the font picked among a language's, by its GUID: in place of
    /// the one at `at`, or after the rest; None takes the one at `at` out.
    pub(super) fn pick_language_font(&mut self, language: &str, at: Option<usize>, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let guid = choice.as_deref().and_then(|path| self.game.content.db().guid_of(path)).map(|guid| guid.to_string());
        let mut fonts = self.language_fonts(language);
        match (guid, at) {
            (Some(guid), Some(at)) if at < fonts.len() => fonts[at] = guid,
            (Some(guid), _) if !fonts.contains(&guid) => fonts.push(guid),
            (None, Some(at)) if at < fonts.len() => drop(fonts.remove(at)),
            _ => return,
        }
        self.set_language_fonts(language, &fonts);
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

    /// Sets how far off a Portal without a view distance of its own still
    /// shows its far side, saved in `voxear.toml` and from now on, in the
    /// game playing too.
    pub fn set_portal_view_distance(&mut self, distance: f32) {
        if let Err(error) = self.game.project.set_portal_view_distance(distance) {
            tracing::error!("{error:#}");
        }
        let portals = self.game.project.file.portals;
        portals.put(&mut self.game.engine.world);
        self.project_settings.show_portals(&mut self.ui, &portals);
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
