//! The Settings panel: the editor's settings (see `settings`) in their
//! groups, each changed in place, with a filter that matches as the command
//! palette does. A setting the palette opens is scrolled to and marked
//! (`found`) for a moment.

use std::time::{Duration, Instant};

use voxear_ui::{NodeId, Style, Ui};

use crate::{
    palette::score,
    settings::{SETTINGS, SettingKind, SettingValue, Settings},
};

/// How long a setting the palette opened stays marked.
const MARKED: Duration = Duration::from_secs(2);

struct Row {
    key: &'static str,
    title: &'static str,
    row: NodeId,
    /// The heading of the setting's group, hidden when the filter leaves
    /// none of its settings.
    heading: NodeId,
    control: Control,
}

#[derive(Clone, Copy)]
enum Control {
    Toggle(NodeId),
    Choice(NodeId),
    Number(NodeId),
}

pub struct SettingsPanel {
    view: NodeId,
    filter: NodeId,
    rows: Vec<Row>,
    marked: Option<(NodeId, Instant)>,
    /// A setting to show once the panel is on screen, as it is not until its
    /// tab has been brought to the front and laid out.
    revealing: Option<&'static str>,
}

impl SettingsPanel {
    /// Builds every setting into `view`, showing `settings`.
    pub fn new(ui: &mut Ui, view: NodeId, filter: NodeId, settings: &Settings) -> Self {
        let body = ui.panel(view, Style::column());
        ui.add_class(body, "settings-body");
        let mut rows = Vec::new();
        let mut group: Option<(&str, NodeId)> = None;
        for info in &SETTINGS {
            let heading = match group {
                Some((name, heading)) if name == info.group => heading,
                _ => {
                    let heading = ui.add_label(body, info.group);
                    ui.add_class(heading, "settings-group");
                    group = Some((info.group, heading));
                    heading
                }
            };
            let row = ui.panel(body, Style::default());
            ui.add_class(row, "prop");
            ui.add_class(row, "setting");
            let label = ui.add_label(row, info.title);
            ui.add_class(label, "prop-label");
            let control = control(ui, row, info.kind, settings.get(info.key));
            rows.push(Row { key: info.key, title: info.title, row, heading, control });
        }
        Self { view, filter, rows, marked: None, revealing: None }
    }

    /// Applies what was changed in the panel to `settings`, and shows
    /// changes made elsewhere. Returns whether the settings changed.
    pub fn update(&mut self, ui: &mut Ui, settings: &mut Settings) -> bool {
        if ui.changed(self.filter) {
            self.apply_filter(ui);
        }
        if let Some(key) = self.revealing.filter(|_| ui.rect(self.view)[3] > 0.0) {
            self.revealing = None;
            self.show_setting(ui, key);
        }
        if let Some((row, at)) = self.marked
            && at.elapsed() > MARKED
        {
            ui.set_class(row, "found", false);
            self.marked = None;
        }
        let mut changed = false;
        for row in &self.rows {
            if let Some(value) = read(ui, &row.control) {
                changed |= settings.set(row.key, value);
            }
            if let Some(value) = settings.get(row.key) {
                show(ui, &row.control, value);
            }
        }
        changed
    }

    /// Shows a setting the palette opened, once the panel is on screen: the
    /// filter cleared, the setting scrolled to and marked, and a number's
    /// field focused to type in.
    pub fn reveal(&mut self, key: &'static str) {
        self.revealing = Some(key);
    }

    fn show_setting(&mut self, ui: &mut Ui, key: &str) {
        if !ui.text(self.filter).is_empty() {
            ui.set_text(self.filter, "");
            self.apply_filter(ui);
        }
        let Some(row) = self.rows.iter().find(|r| r.key == key) else { return };
        if let Some((old, _)) = self.marked.take() {
            ui.set_class(old, "found", false);
        }
        ui.set_class(row.row, "found", true);
        self.marked = Some((row.row, Instant::now()));
        let [_, top, ..] = ui.rect(self.view);
        let [_, y, _, height] = ui.rect(row.row);
        let [x, scrolled] = ui.scroll_offset(self.view);
        let [_, _, _, shown] = ui.viewport(self.view);
        let at = y - top + scrolled;
        if at < scrolled || at + height > scrolled + shown {
            ui.set_scroll(self.view, [x, (at - shown / 3.0).max(0.0)]);
        }
        if let Control::Number(field) = row.control {
            ui.focus(field);
        }
    }

    /// Shows only settings whose names match the filter, and the headings
    /// of their groups.
    fn apply_filter(&mut self, ui: &mut Ui) {
        let query = ui.text(self.filter);
        let query = query.trim();
        let mut headings: Vec<(NodeId, bool)> = Vec::new();
        for row in &self.rows {
            let shown = query.is_empty() || score(query, row.title).is_some();
            ui.set_visible(row.row, shown);
            match headings.iter_mut().find(|(h, _)| *h == row.heading) {
                Some((_, any)) => *any |= shown,
                None => headings.push((row.heading, shown)),
            }
        }
        for (heading, any) in headings {
            ui.set_visible(heading, any);
        }
    }
}

fn control(ui: &mut Ui, row: NodeId, kind: SettingKind, value: Option<SettingValue>) -> Control {
    match (kind, value) {
        (SettingKind::Toggle, value) => {
            let toggle = ui.toggle(row, "", value == Some(SettingValue::Toggle(true)));
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        (SettingKind::Choice(options), value) => {
            let index = match value {
                Some(SettingValue::Choice(i)) => i,
                _ => 0,
            };
            let dropdown = ui.dropdown(row, options, index);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        (SettingKind::Number { unit, .. }, value) => {
            let holder = ui.panel(row, Style::row());
            ui.add_class(holder, "prop-value");
            let number = match value {
                Some(SettingValue::Number(n)) => n,
                _ => 0.0,
            };
            let field = ui.text_field(holder, &format_number(number));
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            let unit = ui.add_label(holder, unit);
            ui.add_class(unit, "setting-unit");
            Control::Number(field)
        }
    }
}

/// What was changed in a control this frame.
fn read(ui: &Ui, control: &Control) -> Option<SettingValue> {
    match *control {
        Control::Toggle(toggle) if ui.changed(toggle) => Some(SettingValue::Toggle(ui.is_checked(toggle))),
        Control::Choice(dropdown) if ui.changed(dropdown) => Some(SettingValue::Choice(ui.selected_option(dropdown))),
        Control::Number(field) if ui.changed(field) => ui.text(field).trim().parse().ok().map(SettingValue::Number),
        _ => None,
    }
}

/// Shows a setting in its control, except in a field being typed in.
fn show(ui: &mut Ui, control: &Control, value: SettingValue) {
    match (*control, value) {
        (Control::Toggle(toggle), SettingValue::Toggle(on)) if ui.is_checked(toggle) != on => {
            ui.set_checked(toggle, on)
        }
        (Control::Choice(dropdown), SettingValue::Choice(i)) if ui.selected_option(dropdown) != i => {
            ui.set_selected_option(dropdown, i)
        }
        (Control::Number(field), SettingValue::Number(n)) => {
            let text = format_number(n);
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
        _ => {}
    }
}

fn format_number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
