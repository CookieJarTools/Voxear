//! The Project Settings page, in the rail's drawer: settings kept in the
//! project's `voxear.toml` rather than the editor's, a tab down the left for
//! each section. Modules comes first: a row for each of the engine's
//! optional systems, switched on or off with its toggle. Quality holds what
//! the renderer may spend: the most simulated particles. World holds the
//! floating origin: whether the world's middle moves with the camera, how
//! far out it goes first, and how far it moves.

use voxear_engine::{
    modules::{Module, Modules},
    QualitySettings,
    origin::{OriginMode, OriginSettings},
};
use voxear_render::MAX_PARTICLE_BUDGET;
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

pub struct ProjectSettingsPanel {
    /// Each tab and the body it shows.
    tabs: Vec<(NodeId, NodeId)>,
    rows: Vec<ModuleRow>,
    origin: OriginRows,
    /// The most simulated particles, on the Quality tab.
    particles: NodeId,
}

impl ProjectSettingsPanel {
    /// Builds the tabs into `tab_bar` and their rows into `view`, showing
    /// `modules`, the floating origin's `origin` and the project's `quality`.
    pub fn new(
        ui: &mut Ui,
        (tab_bar, view): (NodeId, NodeId),
        modules: &Modules,
        origin: &OriginSettings,
        quality: &QualitySettings,
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
        let world_body = body;

        let (quality_tab, body) = section(ui, tab_bar, view, "Quality");
        ui.set_visible(body, false);
        let heading = ui.add_label(body, "Particles");
        ui.add_class(heading, "settings-group");
        let particles = number(ui, body, "Budget", quality.particles as f32, "");
        let tabs = vec![(modules_tab, modules_body), (quality_tab, body), (world_tab, world_body)];
        Self { tabs, rows, origin, particles }
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
