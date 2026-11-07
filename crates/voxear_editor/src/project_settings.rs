//! The Project Settings page, in the rail's drawer: settings kept in the
//! project's `voxear.toml` rather than the editor's, a tab down the left for
//! each section. Modules comes first: a row for each of the engine's
//! optional systems, switched on or off with its toggle.

use voxear_engine::modules::{Module, Modules};
use voxear_ui::{NodeId, Style, Ui};

struct ModuleRow {
    module: Module,
    toggle: NodeId,
}

pub struct ProjectSettingsPanel {
    rows: Vec<ModuleRow>,
}

impl ProjectSettingsPanel {
    /// Builds the Modules tab into `tab_bar` and its rows into `view`,
    /// showing `modules`.
    pub fn new(ui: &mut Ui, tab_bar: NodeId, view: NodeId, modules: &Modules) -> Self {
        let tab = ui.button(tab_bar, "Modules");
        ui.add_class(tab, "settings-tab");
        ui.set_checked(tab, true);
        let body = ui.panel(view, Style::column());
        ui.add_class(body, "settings-body");
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
        Self { rows }
    }

    /// A module switched on or off on the page this frame.
    pub fn changed(&self, ui: &Ui) -> Option<(Module, bool)> {
        self.rows.iter().find(|row| ui.changed(row.toggle)).map(|row| (row.module, ui.is_checked(row.toggle)))
    }

    /// Shows the project's modules, as changes are made or refused.
    pub fn show(&self, ui: &mut Ui, modules: &Modules) {
        for row in &self.rows {
            if ui.is_checked(row.toggle) != modules.has(row.module) {
                ui.set_checked(row.toggle, modules.has(row.module));
            }
        }
    }
}
