//! The Project Settings page, in the rail's drawer: settings kept in the
//! project's `voxear.toml` rather than the editor's, a tab down the left for
//! each section. Modules comes first: a row for each of the engine's
//! optional systems, switched on or off with its toggle. Quality holds what
//! the renderer may spend: the most simulated particles, and how far off a
//! Portal without a view distance of its own still shows its far side.
//! World holds the
//! floating origin: whether the world's middle moves with the camera, how
//! far out it goes first, and how far it moves; and what making worlds of
//! blocks may spend: a Quality preset, and the threads, chunk columns at
//! once, megabytes uploaded a frame, distances and details under it, a
//! number changed by hand making it Custom. With This machine on, they are
//! this machine's own, kept beside the game's saves, and the project's are
//! left as they are. Localization holds each language's fonts, in the order
//! its text falls back to them, each picked from the project's `.ttf` and
//! `.otf` files.

use voxear_engine::{
    PortalSettings, QualitySettings,
    modules::{Module, Modules},
    origin::{OriginMode, OriginSettings},
};
use voxear_render::MAX_PARTICLE_BUDGET;
use voxear_terrain::{WorldBudget, WorldQuality};
use voxear_ui::{NodeId, Style, Ui};

struct ModuleRow {
    module: Module,
    toggle: NodeId,
}

/// The floating origin's controls on the World tab.
struct OriginRows {
    mode: NodeId,
    distance: NodeId,
    step: NodeId,
}

/// The block worlds' budget's controls on the World tab: its preset, and
/// each number under it.
struct BudgetRows {
    quality: NodeId,
    machine: NodeId,
    threads: NodeId,
    columns: NodeId,
    upload: NodeId,
    view: NodeId,
    far: NodeId,
    near_detail: NodeId,
    far_detail: NodeId,
    plant_detail: NodeId,
}

/// What the Localization tab asks of the editor: a font picked for a
/// language, in place of the one at `at` or added after the rest, or one
/// taken out.
#[derive(Clone, Debug, PartialEq)]
pub enum FontAction {
    Pick { language: String, at: Option<usize>, anchor: NodeId },
    Remove { language: String, at: usize },
}

/// A button on the Localization tab, and what it asks.
struct FontButton {
    node: NodeId,
    language: String,
    at: Option<usize>,
    remove: bool,
}

pub struct ProjectSettingsPanel {
    /// Each tab and the body it shows.
    tabs: Vec<(NodeId, NodeId)>,
    /// The Localization tab's body, built again as its fonts change, and
    /// its buttons.
    fonts_body: NodeId,
    font_buttons: Vec<FontButton>,
    rows: Vec<ModuleRow>,
    origin: OriginRows,
    budget: BudgetRows,
    /// The most simulated particles, on the Quality tab.
    particles: NodeId,
    /// The view distance of Portals without their own, on the Quality tab.
    portal_view: NodeId,
}

impl ProjectSettingsPanel {
    /// Builds the tabs into `tab_bar` and their rows into `view`, showing
    /// `modules`, the floating origin's `origin`, the project's `quality`
    /// and `portals`, and its block worlds' `budget`.
    pub fn new(
        ui: &mut Ui,
        (tab_bar, view): (NodeId, NodeId),
        modules: &Modules,
        (origin, budget): (&OriginSettings, &WorldBudget),
        (quality, portals): (&QualitySettings, &PortalSettings),
    ) -> Self {
        let (modules_tab, body) = section(ui, tab_bar, view, "Modules");
        ui.set_checked(modules_tab, true);
        let heading = ui.add_label(body, "Modules");
        ui.add_class(heading, "settings-group");
        let rows = Module::ALL
            .into_iter()
            .map(|module| {
                let row = ui.panel(body, Style::default());
                ui.add_class(row, "prop");
                ui.add_class(row, "setting");
                ui.add_class(row, "module-row");
                let words = ui.panel(row, Style::column());
                ui.add_class(words, "module-words");
                let title = ui.add_label(words, module.title());
                ui.add_class(title, "prop-label");
                let summary = ui.add_label(words, module.summary());
                ui.add_class(summary, "module-summary");
                let toggle = ui.toggle(row, "", modules.has(module));
                ui.add_class(toggle, "checkbox");
                ModuleRow { module, toggle }
            })
            .collect();
        let modules_body = body;

        let (world_tab, body) = section(ui, tab_bar, view, "World");
        ui.set_visible(body, false);
        let heading = ui.add_label(body, "Floating Origin");
        ui.add_class(heading, "settings-group");
        let labels: Vec<&str> = OriginMode::ALL.iter().map(|mode| mode.label()).collect();
        let selected = OriginMode::ALL.iter().position(|mode| *mode == origin.mode).unwrap_or(0);
        let row = setting(ui, body, "Mode");
        let mode = ui.dropdown(row, &labels, selected);
        ui.add_class(mode, "value-choice");
        let distance = number(ui, body, "Distance", origin.distance, "m");
        let step = number(ui, body, "Step", origin.step as f32, "m");
        let origin = OriginRows { mode, distance, step };
        let heading = ui.add_label(body, "Block Worlds");
        ui.add_class(heading, "settings-group");
        let labels: Vec<&str> = WorldQuality::ALL.iter().map(|quality| quality.label()).collect();
        let selected = WorldQuality::ALL.iter().position(|quality| *quality == budget.quality).unwrap_or(0);
        let row = setting(ui, body, "Quality");
        let quality_choice = ui.dropdown(row, &labels, selected);
        ui.add_class(quality_choice, "value-choice");
        let row = setting(ui, body, "This machine");
        let machine = ui.toggle(row, "", false);
        ui.add_class(machine, "checkbox");
        let budget = BudgetRows {
            quality: quality_choice,
            machine,
            threads: number(ui, body, "Threads", budget.threads as f32, ""),
            columns: number(ui, body, "Columns at once", budget.columns as f32, ""),
            upload: number(ui, body, "Upload a frame", budget.upload as f32, "MB"),
            view: number(ui, body, "View distance", budget.view_distance, "m"),
            far: number(ui, body, "Far distance", budget.far_distance, "m"),
            near_detail: number(ui, body, "Near detail", budget.near_detail, "×"),
            far_detail: number(ui, body, "Far detail", budget.far_detail, "×"),
            plant_detail: number(ui, body, "Plant detail", budget.plant_detail, "×"),
        };
        let world_body = body;

        let (quality_tab, body) = section(ui, tab_bar, view, "Quality");
        ui.set_visible(body, false);
        let heading = ui.add_label(body, "Particles");
        ui.add_class(heading, "settings-group");
        let particles = number(ui, body, "Budget", quality.particles as f32, "");
        let heading = ui.add_label(body, "Portals");
        ui.add_class(heading, "settings-group");
        let portal_view = number(ui, body, "View Distance", portals.view_distance, "m");
        let quality_body = body;

        let (localization_tab, fonts_body) = section(ui, tab_bar, view, "Localization");
        ui.set_visible(fonts_body, false);
        let tabs = vec![
            (modules_tab, modules_body),
            (quality_tab, quality_body),
            (world_tab, world_body),
            (localization_tab, fonts_body),
        ];
        Self { tabs, fonts_body, font_buttons: Vec::new(), rows, origin, budget, particles, portal_view }
    }

    /// Shows each language's fonts on the Localization tab, `languages`
    /// giving each code with its fonts' names in order: a button for each,
    /// which opens the picker, with one to take it out, and Add Font.
    pub fn show_language_fonts(&mut self, ui: &mut Ui, languages: &[(String, Vec<String>)]) {
        for child in ui.children(self.fonts_body) {
            ui.remove(child);
        }
        self.font_buttons.clear();
        let heading = ui.add_label(self.fonts_body, "Fonts");
        ui.add_class(heading, "settings-group");
        for (language, fonts) in languages {
            for (at, font) in fonts.iter().enumerate() {
                let row = setting(ui, self.fonts_body, if at == 0 { language } else { "" });
                let holder = ui.panel(row, Style::row());
                ui.add_class(holder, "prop-value");
                let picker = ui.button(holder, font);
                ui.add_class(picker, "value-picker");
                ui.set_name(picker, &format!("font-{language}-{at}"));
                let remove = ui.icon_button(holder, "close");
                ui.add_class(remove, "icon-button");
                let button = |node, remove| FontButton { node, language: language.clone(), at: Some(at), remove };
                self.font_buttons.extend([button(picker, false), button(remove, true)]);
            }
            let row = setting(ui, self.fonts_body, if fonts.is_empty() { language } else { "" });
            let add = ui.button(row, "Add Font");
            ui.add_class(add, "settings-button");
            ui.set_name(add, &format!("add-font-{language}"));
            self.font_buttons.push(FontButton { node: add, language: language.clone(), at: None, remove: false });
        }
    }

    /// A font button clicked on the Localization tab this frame.
    pub fn font_action(&self, ui: &Ui) -> Option<FontAction> {
        let button = self.font_buttons.iter().find(|button| ui.clicked(button.node))?;
        let language = button.language.clone();
        Some(match (button.remove, button.at) {
            (true, Some(at)) => FontAction::Remove { language, at },
            _ => FontAction::Pick { language, at: button.at, anchor: button.node },
        })
    }

    /// Shows the tab clicked this frame, its body in place of the others'.
    pub fn switch_tabs(&self, ui: &mut Ui) {
        let Some(&(clicked, _)) = self.tabs.iter().find(|(tab, _)| ui.clicked(*tab)) else { return };
        for &(tab, body) in &self.tabs {
            ui.set_checked(tab, tab == clicked);
            ui.set_visible(body, tab == clicked);
        }
    }

    /// A module switched on or off on the page this frame.
    pub fn changed(&self, ui: &Ui) -> Option<(Module, bool)> {
        self.rows.iter().find(|row| ui.changed(row.toggle)).map(|row| (row.module, ui.is_checked(row.toggle)))
    }

    /// The floating origin as changed on the page this frame, from `now`:
    /// a distance or step of a metre at least, whole metres for the step.
    pub fn origin_changed(&self, ui: &Ui, now: &OriginSettings) -> Option<OriginSettings> {
        let OriginRows { mode, distance, step } = self.origin;
        let metres = |field: NodeId| ui.text(field).trim().parse::<f32>().ok().filter(|m| m.is_finite() && *m >= 1.0);
        let changed = if ui.changed(mode) {
            let mode = OriginMode::ALL.get(ui.selected_option(mode)).copied().unwrap_or_default();
            OriginSettings { mode, ..now.clone() }
        } else if ui.changed(distance) {
            OriginSettings { distance: metres(distance)?, ..now.clone() }
        } else if ui.changed(step) {
            OriginSettings { step: metres(step)?.round() as u32, ..now.clone() }
        } else {
            return None;
        };
        (changed != *now).then_some(changed)
    }

    /// The block worlds' budget as changed on the page this frame, from
    /// `now`: a preset picked sets every number, and a number typed makes it
    /// Custom.
    pub fn budget_changed(&self, ui: &Ui, now: &WorldBudget) -> Option<WorldBudget> {
        let rows = &self.budget;
        let typed = |field: NodeId| ui.text(field).trim().parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0);
        let custom = WorldBudget { quality: WorldQuality::Custom, ..*now };
        let changed = if ui.changed(rows.quality) {
            let quality = WorldQuality::ALL.get(ui.selected_option(rows.quality)).copied().unwrap_or_default();
            quality.budget().unwrap_or(custom)
        } else if ui.changed(rows.threads) {
            WorldBudget { threads: typed(rows.threads)?.round() as u32, ..custom }
        } else if ui.changed(rows.columns) {
            WorldBudget { columns: typed(rows.columns)?.round() as u32, ..custom }
        } else if ui.changed(rows.upload) {
            WorldBudget { upload: typed(rows.upload)?.round() as u32, ..custom }
        } else if ui.changed(rows.view) {
            WorldBudget { view_distance: typed(rows.view)?, ..custom }
        } else if ui.changed(rows.far) {
            WorldBudget { far_distance: typed(rows.far)?, ..custom }
        } else if ui.changed(rows.near_detail) {
            WorldBudget { near_detail: typed(rows.near_detail)?, ..custom }
        } else if ui.changed(rows.far_detail) {
            WorldBudget { far_detail: typed(rows.far_detail)?, ..custom }
        } else if ui.changed(rows.plant_detail) {
            WorldBudget { plant_detail: typed(rows.plant_detail)?, ..custom }
        } else {
            return None;
        };
        let changed = changed.checked();
        (changed != *now).then_some(changed)
    }

    /// This machine toggled on or off this frame: whether it now keeps its
    /// own budget.
    pub fn machine_changed(&self, ui: &Ui) -> Option<bool> {
        let machine = self.budget.machine;
        ui.changed(machine).then(|| ui.is_checked(machine))
    }

    /// Shows whether this machine keeps its own budget.
    pub fn show_machine(&self, ui: &mut Ui, kept: bool) {
        if ui.is_checked(self.budget.machine) != kept {
            ui.set_checked(self.budget.machine, kept);
        }
    }

    /// Shows the block worlds' budget, but in a field being typed in.
    pub fn show_budget(&self, ui: &mut Ui, budget: &WorldBudget) {
        let rows = &self.budget;
        let selected = WorldQuality::ALL.iter().position(|quality| *quality == budget.quality).unwrap_or(0);
        if ui.selected_option(rows.quality) != selected {
            ui.set_selected_option(rows.quality, selected);
        }
        let numbers = [
            (rows.threads, budget.threads as f32),
            (rows.columns, budget.columns as f32),
            (rows.upload, budget.upload as f32),
            (rows.view, budget.view_distance),
            (rows.far, budget.far_distance),
            (rows.near_detail, budget.near_detail),
            (rows.far_detail, budget.far_detail),
            (rows.plant_detail, budget.plant_detail),
        ];
        for (field, value) in numbers {
            let text = format_metres(value);
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
    }

    /// The quality settings as changed on the page this frame, from `now`:
    /// a whole number of particles, no more than the renderer can keep.
    pub fn quality_changed(&self, ui: &Ui, now: &QualitySettings) -> Option<QualitySettings> {
        if !ui.changed(self.particles) {
            return None;
        }
        let typed = ui.text(self.particles).trim().replace([',', ' ', '_'], "").parse::<f64>().ok()?;
        let most = f64::from(MAX_PARTICLE_BUDGET);
        let particles = (typed.is_finite() && typed >= 0.0).then(|| typed.round().min(most) as u32)?;
        let changed = QualitySettings { particles };
        (changed != *now).then_some(changed)
    }

    /// Shows the project's quality settings, but in a field being typed in.
    pub fn show_quality(&self, ui: &mut Ui, quality: &QualitySettings) {
        let text = format_metres(quality.particles as f32);
        if ui.focused() != Some(self.particles) && ui.text(self.particles) != text {
            ui.set_text(self.particles, &text);
        }
    }

    /// The view distance of Portals without their own as typed on the page
    /// this frame, where it differs from `now`.
    pub fn portal_view_changed(&self, ui: &Ui, now: f32) -> Option<f32> {
        if !ui.changed(self.portal_view) {
            return None;
        }
        let typed = ui.text(self.portal_view).trim().replace([',', ' ', '_'], "").parse::<f32>().ok()?;
        let distance = (typed.is_finite() && typed >= 0.0).then_some(typed)?;
        (distance != now).then_some(distance)
    }

    /// Shows the project's view distance for Portals, but in a field being
    /// typed in.
    pub fn show_portals(&self, ui: &mut Ui, portals: &PortalSettings) {
        let text = format_metres(portals.view_distance);
        if ui.focused() != Some(self.portal_view) && ui.text(self.portal_view) != text {
            ui.set_text(self.portal_view, &text);
        }
    }

    /// Shows the project's modules, as changes are made or refused.
    pub fn show(&self, ui: &mut Ui, modules: &Modules) {
        for row in &self.rows {
            if ui.is_checked(row.toggle) != modules.has(row.module) {
                ui.set_checked(row.toggle, modules.has(row.module));
            }
        }
    }

    /// Shows the project's floating origin, but in a field being typed in.
    pub fn show_origin(&self, ui: &mut Ui, origin: &OriginSettings) {
        let selected = OriginMode::ALL.iter().position(|mode| *mode == origin.mode).unwrap_or(0);
        if ui.selected_option(self.origin.mode) != selected {
            ui.set_selected_option(self.origin.mode, selected);
        }
        for (field, value) in [(self.origin.distance, origin.distance), (self.origin.step, origin.step as f32)] {
            let text = format_metres(value);
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
    }
}

/// A tab named `name` in `tab_bar`, and its body in `view`.
fn section(ui: &mut Ui, tab_bar: NodeId, view: NodeId, name: &str) -> (NodeId, NodeId) {
    let tab = ui.button(tab_bar, name);
    ui.add_class(tab, "settings-tab");
    let body = ui.panel(view, Style::column());
    ui.add_class(body, "settings-body");
    (tab, body)
}

/// A row labelled `label`, for its control.
fn setting(ui: &mut Ui, body: NodeId, label: &str) -> NodeId {
    let row = ui.panel(body, Style::default());
    ui.add_class(row, "prop");
    ui.add_class(row, "setting");
    let label = ui.add_label(row, label);
    ui.add_class(label, "prop-label");
    row
}

/// A row with a number to type, and what it is a number of after it, if anything.
fn number(ui: &mut Ui, body: NodeId, label: &str, value: f32, unit: &str) -> NodeId {
    let row = setting(ui, body, label);
    let holder = ui.panel(row, Style::row());
    ui.add_class(holder, "prop-value");
    let field = ui.text_field(holder, &format_metres(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    if !unit.is_empty() {
        let unit = ui.add_label(holder, unit);
        ui.add_class(unit, "setting-unit");
    }
    field
}

fn format_metres(value: f32) -> String {
    if value.fract() == 0.0 { format!("{value:.0}") } else { format!("{value}") }
}
