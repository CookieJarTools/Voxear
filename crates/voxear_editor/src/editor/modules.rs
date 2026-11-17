//! Project Settings: what the window changes, written into `voxear.toml`
//! and put into effect (see `project_settings` for what each setting is);
//! its parts of their own, the layers' names, the collision matrix and the
//! languages' fonts; and the setters tools and tests reach the same
//! settings by. Most changes take effect at once; the floating origin, the
//! network, time and gravity the next time the game plays.

use serde_json::{Value as Json, json};
use voxear_assets::AssetKind;
use voxear_engine::modules::Module;
use voxear_terrain::{WorldBudget, WorldQuality};
use voxear_ui::{NodeId, Style, Ui};

use super::Editor;
use crate::{
    fields::{self, Pick},
    object_picker::{self, PickFor},
    project_settings, setting_info,
    settings_window::Action,
};

/// What a button among the languages' fonts asks.
#[derive(Clone, Debug, PartialEq)]
pub enum FontAction {
    Pick { language: String, at: Option<usize>, anchor: NodeId },
    Remove { language: String, at: usize },
}

/// A button among the languages' fonts, and what it asks.
struct FontButton {
    node: NodeId,
    language: String,
    at: Option<usize>,
    remove: bool,
}

/// The languages' fonts in Project Settings ▸ Localization: a row for each
/// font of each language, which opens the picker, with one to take it out,
/// and Add Font.
#[derive(Default)]
pub struct FontRows {
    buttons: Vec<FontButton>,
}

impl FontRows {
    /// Shows each language's fonts in `holder`, `languages` giving each
    /// code with its fonts' names in order.
    pub fn show(&mut self, ui: &mut Ui, holder: NodeId, languages: &[(String, Vec<String>)]) {
        for child in ui.children(holder) {
            ui.remove(child);
        }
        self.buttons.clear();
        for (language, fonts) in languages {
            for (at, font) in fonts.iter().enumerate() {
                let row = font_row(ui, holder, if at == 0 { language } else { "" });
                let line = ui.panel(row, Style::row());
                ui.add_class(line, "prop-value");
                let picker = ui.button(line, font);
                ui.add_class(picker, "setting-folder");
                ui.set_name(picker, &format!("font-{language}-{at}"));
                let remove = ui.icon_button(line, "close");
                ui.add_class(remove, "setting-clear");
                ui.set_tooltip(remove, "Take this font out", None);
                let button = |node, remove| FontButton { node, language: language.clone(), at: Some(at), remove };
                self.buttons.extend([button(picker, false), button(remove, true)]);
            }
            let row = font_row(ui, holder, if fonts.is_empty() { language } else { "" });
            let add = ui.button(row, "Add Font");
            ui.add_class(add, "settings-button");
            ui.set_name(add, &format!("add-font-{language}"));
            self.buttons.push(FontButton { node: add, language: language.clone(), at: None, remove: false });
        }
    }

    /// A button clicked this frame.
    pub fn action(&self, ui: &Ui) -> Option<FontAction> {
        let button = self.buttons.iter().find(|button| ui.clicked(button.node))?;
        let language = button.language.clone();
        Some(match (button.remove, button.at) {
            (true, Some(at)) => FontAction::Remove { language, at },
            _ => FontAction::Pick { language, at: button.at, anchor: button.node },
        })
    }
}

/// A row of the fonts labelled `label`, for its fonts.
fn font_row(ui: &mut Ui, holder: NodeId, label: &str) -> NodeId {
    let row = ui.panel(holder, Style::default());
    ui.add_class(row, "prop");
    ui.add_class(row, "setting");
    let label = ui.add_label(row, label);
    ui.add_class(label, "prop-label");
    row
}

impl Editor {
    pub(super) fn update_project_settings(&mut self) {
        for action in self.project_settings.update(&mut self.ui) {
            self.project_settings_action(action);
        }
        if let Some(module) = self.inspector.module_wanted(&self.ui) {
            self.set_module(module, true);
        }
        if let Some(edit) = self.settings_layers.update(&mut self.ui) {
            self.edit_layers(edit);
        }
        match self.font_rows.action(&self.ui) {
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
        if self.shown(self.nodes.project_settings_panel) {
            let values = project_settings::values(&self.game);
            self.project_settings.show(&mut self.ui, &|key| values.get(key).cloned());
        }
    }

    fn project_settings_action(&mut self, action: Action) {
        let outcome = match action {
            Action::Set(key, value) => self.set_project_setting(key, &value).map(drop),
            Action::Reset(key) => self.set_project_setting(key, &Json::Null).map(drop),
            Action::ResetCategory(category) => {
                // Modules are the project's choices, with no default to go back to.
                let keys: Vec<&str> = (project_settings::settings().iter())
                    .filter(|s| s.category == category && s.settable() && !s.key.starts_with("modules."))
                    .map(|s| s.key)
                    .collect();
                keys.into_iter().try_for_each(|key| self.set_project_setting(key, &Json::Null).map(drop))
            }
            Action::PickAsset { key, kind, anchor } => {
                let choices = object_picker::decorate(&self.game, fields::assets_of(&self.game, Some(kind)));
                let title = project_settings::find(key).map_or("Select", |setting| setting.label);
                self.picker.open(&mut self.ui, (PickFor::ProjectSetting(key), anchor), title, choices, "");
                Ok(())
            }
            Action::Press("input.open") => Ok(self.show_panel("input")),
            Action::Press("player.build") => Ok(self.show_panel("build")),
            Action::Press("scripts.open") => Ok(self.open_project_in_code_editor()),
            Action::OpenManual(page) => {
                self.show_panel("docs");
                self.docs.open_reference(&mut self.ui, page);
                Ok(())
            }
            Action::Press(_) | Action::PickColor { .. } | Action::PickFolder(_) => Ok(()),
        };
        if let Err(error) = outcome {
            self.notify(&error);
        }
    }

    /// Puts the asset picked for one of Project Settings' settings.
    pub(super) fn pick_project_setting(&mut self, key: &str, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let value = choice.as_ref().map_or(Json::Null, |path| json!(path));
        if let Err(error) = self.set_project_setting(key, &value) {
            self.notify(&error);
        }
    }

    /// Changes one of Project Settings' settings by its key, as the window
    /// and `vox editor call` do; null sets it back to its default. It is
    /// written into `voxear.toml`, or this machine's own file for the block
    /// worlds' budget it keeps, and put into effect. Gives the value it has
    /// now, or why it cannot be that.
    pub fn set_project_setting(&mut self, key: &str, value: &Json) -> Result<Json, String> {
        let setting = project_settings::find(key)
            .ok_or_else(|| format!("no project setting has the key `{key}`; `project_settings.get` lists them"))?;
        if !setting.settable() {
            return Err(format!("{} is not changed by a value", setting.label));
        }
        let value = if value.is_null() { None } else { Some(setting_info::check(setting, value)?) };
        let (table, name) = key.split_once('.').unwrap_or((key, ""));
        match (table, name) {
            ("modules", name) => {
                let module = Module::from_key(name).ok_or_else(|| format!("no module is `{name}`"))?;
                if let Some(on) = value.as_ref().and_then(Json::as_bool) {
                    self.set_module(module, on);
                }
            }
            // The product name has no default to go back to, and is never empty.
            ("project", "name") => {
                let name = value.as_ref().and_then(Json::as_str).unwrap_or_default();
                if !name.is_empty() {
                    self.game
                        .project
                        .set_setting(key, Some(toml::Value::String(name.to_string())))
                        .map_err(|error| format!("{error:#}"))?;
                    self.files_seen();
                } else if value.is_some() {
                    return Err("The product name cannot be empty".to_string());
                }
            }
            ("world", "machine") => self.keep_world_budget(value.and_then(|v| v.as_bool()).unwrap_or(false)),
            ("world", "quality") => {
                let word = value.as_ref().and_then(Json::as_str).unwrap_or("High");
                let quality = WorldQuality::ALL.into_iter().find(|q| q.label() == word).unwrap_or_default();
                let now = WorldBudget::of(&self.game.engine.world);
                self.set_world_budget(&quality.budget().unwrap_or(WorldBudget { quality, ..now }));
            }
            ("world", field) => {
                let default = serde_json::to_value(WorldBudget::default()).unwrap_or_default();
                let number = value.as_ref().or(default.get(field)).and_then(Json::as_f64).unwrap_or_default();
                let now = WorldBudget::of(&self.game.engine.world);
                let changed =
                    project_settings::budget_with(now, field, number).ok_or("no such number of the budget")?;
                self.set_world_budget(&changed);
            }
            _ => {
                let written = match &value {
                    Some(value) => project_settings::to_toml(setting, value, self.game.content.db())?,
                    None => None,
                };
                self.game.project.set_setting(key, written).map_err(|error| format!("{error:#}"))?;
                self.files_seen();
                self.put_project_setting(table);
            }
        }
        tracing::info!("[Project Settings] {key}");
        let values = project_settings::values(&self.game);
        Ok(values.get(setting.key).map_or(Json::Null, |(value, _)| value.clone()))
    }

    /// Puts a table of `voxear.toml` just written into effect, where it
    /// takes effect before the game next plays.
    fn put_project_setting(&mut self, table: &str) {
        let file = &self.game.project.file;
        let world = &mut self.game.engine.world;
        match table {
            "quality" => file.quality.put(world),
            "portals" => file.portals.put(world),
            "foliage" => world.insert_resource(file.foliage.bounded()),
            "physics" => file.physics.put(world),
            "audio" => {
                let audio = &mut self.game.audio;
                (audio.headphones, audio.reverb) = (file.audio.headphones, file.audio.reverb);
                audio.reverb_mode = file.audio.reverb_mode;
                audio.tracer.project = file.audio.sound_tracing();
            }
            "input" => self.game.load_input_actions(),
            "localization" => {
                self.game.load_strings();
                self.show_language_fonts();
            }
            _ => {}
        }
    }

    /// Takes a change to the project's layers from Project Settings.
    fn edit_layers(&mut self, edit: crate::layers_view::LayersEdit) {
        use crate::layers_view::LayersEdit;
        let mut layers = self.game.project.layers();
        match edit {
            LayersEdit::Name(layer, name) => {
                if let Some(other) = layers.find(&name).filter(|&other| other != layer) {
                    self.notify(&format!("Layer {other} is already named {name}"));
                    self.settings_layers.reset_name(&mut self.ui, layer);
                    return;
                }
                layers.set_name(layer, &name);
            }
            LayersEdit::Collide(a, b, on) => layers.set_collide(a, b, on),
        }
        self.set_layers(&layers);
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

    /// Shows each of the project's languages with its fonts in Project
    /// Settings ▸ Localization, each by its file's name.
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
        if let Some(holder) = self.project_settings.part("localization.fonts") {
            self.font_rows.show(&mut self.ui, holder, &rows);
        }
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
        self.files_seen();
    }

    /// Sets what the project lets the renderer spend (see
    /// `voxear_engine::QualitySettings`), from now on.
    pub fn set_quality(&mut self, quality: &voxear_engine::QualitySettings) {
        if let Err(error) = self.game.project.set_quality(quality) {
            tracing::error!("{error:#}");
        }
        self.files_seen();
        self.game.project.file.quality.put(&mut self.game.engine.world);
    }

    /// Sets how far off a Portal without a view distance of its own still
    /// shows its far side, saved in `voxear.toml` and from now on, in the
    /// game playing too.
    pub fn set_portal_view_distance(&mut self, distance: f32) {
        if let Err(error) = self.game.project.set_portal_view_distance(distance) {
            tracing::error!("{error:#}");
        }
        self.files_seen();
        self.game.project.file.portals.put(&mut self.game.engine.world);
    }

    /// Sets what making worlds of blocks may spend (see
    /// `voxear_terrain::WorldBudget`), from now on: worlds streaming take it
    /// up as they go, with nothing made again. Into this machine's own where
    /// it keeps one, and else into the project's `voxear.toml`.
    pub fn set_world_budget(&mut self, budget: &voxear_terrain::WorldBudget) {
        use voxear_terrain::MachineBudget;
        if MachineBudget::of(&self.game.engine.world).kept {
            let world = &mut self.game.engine.world;
            budget.put(world);
            if let Err(error) = MachineBudget::keep(world, true) {
                tracing::error!("keeping this machine's world quality: {error}");
            }
        } else {
            if let Err(error) = self.game.project.set_world(budget) {
                tracing::error!("{error:#}");
            }
            self.files_seen();
            let (project, world) = (self.game.project.file.world, &mut self.game.engine.world);
            MachineBudget::open(world, project, MachineBudget::of(world).file);
        }
    }

    /// Keeps the budget in use for this machine, beside the game's saves,
    /// in place of the project's; or forgets it and goes back to the
    /// project's.
    pub fn keep_world_budget(&mut self, keep: bool) {
        if let Err(error) = voxear_terrain::MachineBudget::keep(&mut self.game.engine.world, keep) {
            tracing::error!("keeping this machine's world quality: {error}");
        }
    }

    /// Switches a module on or off for the project.
    pub fn set_module(&mut self, module: Module, on: bool) {
        if let Err(error) = self.game.project.set_module(module, on) {
            tracing::error!("{error:#}");
        }
        let modules = self.game.project.modules();
        voxear_engine::modules::put(&mut self.game.engine.world, modules);
        self.files_seen();
        // The scripts build against the modules' assemblies, so they build again with the new set.
        self.rebuild_scripts = true;
        // Components of a module switched off or on show it at once.
        let (shown, playing) = (self.inspector.shown(), self.is_playing());
        self.inspector.show(&mut self.ui, &self.game, shown, playing);
    }

    /// Opens Project Settings at one of its settings, or Preferences at one
    /// of its own, as the palette and `settings.show` do. False for a key
    /// neither has.
    pub fn show_setting(&mut self, key: &str) -> bool {
        if let Some(setting) = project_settings::find(key) {
            self.show_panel("project-settings");
            self.project_settings.reveal(&mut self.ui, setting.key);
        } else if let Some(setting) = crate::settings::info(key) {
            self.show_panel("preferences");
            self.preferences.reveal(&mut self.ui, setting.key);
        } else {
            return false;
        }
        true
    }

    /// Opens Project Settings on a category, as Add Layer… opens Tags and
    /// Layers.
    pub fn show_project_settings(&mut self, category: &str) {
        self.show_panel("project-settings");
        self.project_settings.select(&mut self.ui, category);
    }
}
