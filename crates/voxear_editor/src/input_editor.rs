//! The Input Actions window's Actions view, where the project's actions
//! are made and changed, as in Unity's Input Actions window: the action
//! maps on the left, the picked map's actions in the middle, each followed
//! by its bindings and a composite's parts, and the picked row's
//! properties on the right. An action has its name, type, interaction,
//! category and whether it is hidden; a binding has its control, which
//! Listen takes from the next key or button pressed, and its processors.
//!
//! Each change is made to a copy of the file (see `input_edit`), checked as
//! the game would read it, and handed to the editor to write and read
//! again. A change the game would not read is not kept, and says why.

use std::{cell::RefCell, rc::Rc};

use voxear_input::{ActionKind, ActionsFile, Input, Rebound, Scheme, actions::MapData};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

use voxear_engine::keymaps;

use crate::{
    columns,
    input_edit::{self, Composite, Row},
};

const ROW: f32 = 22.0;
const KINDS: [&str; 3] = ["Button", "Value", "Vector 2"];
const INTERACTIONS: [&str; 3] = ["Press", "Hold", "Tap"];

/// The properties shown for the picked row, each control by what it sets.
#[derive(Default)]
struct Properties {
    map_name: Option<NodeId>,
    name: Option<NodeId>,
    kind: Option<NodeId>,
    interaction: Option<NodeId>,
    seconds: Option<NodeId>,
    category: Option<NodeId>,
    hidden: Option<NodeId>,
    path: Option<NodeId>,
    /// The binding's control as a game's prompt draws it.
    glyph: Option<NodeId>,
    listen: Option<NodeId>,
    processors: Option<NodeId>,
}

pub struct ActionsEditor {
    maps: NodeId,
    rows: NodeId,
    properties: NodeId,
    add_map: NodeId,
    remove_map: NodeId,
    add_action: NodeId,
    add_binding: NodeId,
    add_vector: NodeId,
    add_axis: NodeId,
    remove: NodeId,
    status: NodeId,
    shown_maps: Rc<RefCell<Vec<String>>>,
    shown_rows: Rc<RefCell<Vec<(String, usize)>>>,
    controls: Properties,
    /// The file being edited, and the file as the game last read it.
    file: ActionsFile,
    read: Option<ActionsFile>,
    map: usize,
    row: Option<Row>,
    /// The row waiting for a key or button pressed.
    listening: Option<Row>,
    /// The text field last focused, to take what was typed once it is left.
    editing: Option<NodeId>,
}

impl ActionsEditor {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let columns_row = ui.panel(parent, Style::row());
        ui.add_class(columns_row, "fill");
        ui.add_class(columns_row, "entities-columns");
        let panes = columns::panes(ui, columns_row, 3);
        let column = |ui: &mut Ui, pane: NodeId, title: &str| {
            let column = ui.panel(pane, Style::default());
            ui.add_class(column, "entities-column");
            let heading = ui.panel(column, Style::row());
            ui.add_class(heading, "input-heading");
            let label = ui.add_label(heading, title);
            ui.add_class(label, "profiler-heading");
            (column, heading)
        };

        let (left, heading) = column(ui, panes[0], "Action Maps");
        let add_map = ui.icon_button(heading, "plus");
        let remove_map = ui.icon_button(heading, "close");
        let shown_maps: Rc<RefCell<Vec<String>>> = Rc::default();
        let names = shown_maps.clone();
        let maps = ui.list_view(
            left,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "entities-row");
                    let label = ui.add_label(row, "");
                    ui.add_class(label, "entities-row-name");
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let Some(name) = names.borrow().get(index).cloned() else { return };
                    if let Some(&label) = ui.children(row).first() {
                        ui.set_text(label, &name);
                    }
                },
            },
        );
        ui.add_class(maps, "fill");

        let (middle, heading) = column(ui, panes[1], "Actions");
        let remove = ui.icon_button(heading, "close");
        let adds = ui.panel(middle, Style::row());
        ui.add_class(adds, "input-adds");
        let add_action = ui.button(adds, "Action");
        let add_binding = ui.button(adds, "Binding");
        let add_vector = ui.button(adds, "2D Vector");
        let add_axis = ui.button(adds, "1D Axis");
        for button in [add_action, add_binding, add_vector, add_axis] {
            ui.add_class(button, "input-add");
        }
        let shown_rows: Rc<RefCell<Vec<(String, usize)>>> = Rc::default();
        let texts = shown_rows.clone();
        let rows = ui.list_view(
            middle,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "entities-row");
                    let label = ui.add_label(row, "");
                    ui.add_class(label, "entities-row-name");
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let Some((text, depth)) = texts.borrow().get(index).cloned() else { return };
                    let Some(&label) = ui.children(row).first() else { return };
                    ui.set_text(label, &text);
                    ui.set_inline_style(label, &format!("padding-left: {}px", 6 + depth * 16));
                },
            },
        );
        ui.add_class(rows, "fill");

        let (right, _) = column(ui, panes[2], "Properties");
        let properties = ui.panel(right, Style::default());
        ui.add_class(properties, "input-properties");
        let status = ui.add_label(right, "");
        ui.add_class(status, "entities-hint");
        Self {
            maps,
            rows,
            properties,
            add_map,
            remove_map,
            add_action,
            add_binding,
            add_vector,
            add_axis,
            remove,
            status,
            shown_maps,
            shown_rows,
            controls: Properties::default(),
            file: ActionsFile::default(),
            read: None,
            map: 0,
            row: None,
            listening: None,
            editing: None,
        }
    }

    /// Whether a binding waits for a key or button, which the editor then
    /// hands the game's input whatever the focus.
    pub fn is_listening(&self) -> bool {
        self.listening.is_some()
    }

    /// Follows the game's actions and what is done to them here, once a
    /// frame while the view shows. Gives the file's new text after a
    /// change, for the editor to write.
    pub fn update(&mut self, ui: &mut Ui, input: &mut Input) -> Option<String> {
        if self.read.as_ref() != Some(input.authored_file()) {
            let fresh = input.authored_file().clone();
            self.read = Some(fresh.clone());
            // Shown again only when it is not what was written from here.
            if fresh != self.file {
                self.file = fresh;
                self.map = self.map.min(self.file.maps.len().saturating_sub(1));
                self.show(ui);
            }
        }
        let before = self.file.clone();
        self.listen(ui, input);
        // Picks first, so that a row a button adds is the one picked after.
        self.take_picks(ui);
        self.take_buttons(ui, input);
        self.take_properties(ui);
        if self.file == before {
            self.show_glyph(ui, input);
            return None;
        }
        if let Err(error) = self.file.check() {
            ui.set_text(self.status, &format!("{error:#}"));
            self.file = before;
            self.show(ui);
            return None;
        }
        ui.set_text(self.status, "");
        self.show(ui);
        self.show_glyph(ui, input);
        self.file.to_text().ok()
    }

    fn map_data(&mut self) -> Option<&mut MapData> {
        self.file.maps.get_mut(self.map)
    }

    fn listen(&mut self, ui: &mut Ui, input: &mut Input) {
        let Some(row) = self.listening else { return };
        match input.take_rebound() {
            Some(Rebound::To(path)) => {
                self.listening = None;
                if let Some(map) = self.map_data() {
                    input_edit::set_path(map, row, &path);
                }
            }
            Some(Rebound::Cancelled) => {
                self.listening = None;
                self.show_properties(ui);
            }
            None => {}
        }
    }

    fn take_buttons(&mut self, ui: &mut Ui, input: &mut Input) {
        if ui.clicked(self.add_map) {
            self.map = input_edit::add_map(&mut self.file);
            self.row = None;
        }
        if ui.clicked(self.remove_map) && self.map < self.file.maps.len() {
            self.file.maps.remove(self.map);
            self.map = self.map.saturating_sub(1);
            self.row = None;
        }
        if self.file.maps.is_empty() {
            return;
        }
        let action = self.row.map(Row::action);
        let added = if ui.clicked(self.add_action) {
            self.map_data().map(input_edit::add_action)
        } else if ui.clicked(self.add_binding) {
            action.and_then(|a| self.map_data().and_then(|map| input_edit::add_binding(map, a, None)))
        } else if ui.clicked(self.add_vector) {
            action
                .and_then(|a| self.map_data().and_then(|map| input_edit::add_binding(map, a, Some(Composite::Vector))))
        } else if ui.clicked(self.add_axis) {
            action.and_then(|a| self.map_data().and_then(|map| input_edit::add_binding(map, a, Some(Composite::Axis))))
        } else {
            None
        };
        if added.is_some() {
            self.row = added;
        }
        if ui.clicked(self.remove)
            && let (Some(row), Some(map)) = (self.row, self.file.maps.get_mut(self.map))
        {
            input_edit::remove(map, row);
            self.row = None;
        }
        if let Some(listen) = self.controls.listen.filter(|&button| ui.clicked(button)) {
            let row = self.row.filter(|row| !matches!(row, Row::Action(_)));
            let scheme = row.and_then(|row| self.file.maps.get(self.map).and_then(|m| input_edit::path(m, row)));
            input.listen_for_control(scheme.filter(|path| !path.is_empty()).map(|path| Scheme::of(&path)));
            self.listening = row;
            ui.set_text(listen, "Press a Key");
        }
    }

    fn take_picks(&mut self, ui: &mut Ui) {
        if ui.selection_changed(self.maps)
            && let Some(&picked) = ui.selection(self.maps).first()
        {
            self.map = picked;
            self.row = None;
            self.show(ui);
        }
        if ui.selection_changed(self.rows) {
            let rows = self.file.maps.get(self.map).map(input_edit::rows).unwrap_or_default();
            self.row = ui.selection(self.rows).first().and_then(|&at| rows.get(at).copied());
            self.show_properties(ui);
        }
    }

    /// Takes what the properties' controls changed: a dropdown or toggle at
    /// once, a text field once it is left or Enter is pressed.
    fn take_properties(&mut self, ui: &mut Ui) {
        let focused = ui.focused();
        let left = self.editing.filter(|&field| Some(field) != focused);
        let done = |ui: &Ui, field: Option<NodeId>| field.is_some_and(|f| ui.submitted(f) || left == Some(f));
        let c = &self.controls;
        let (map_name, name, seconds, path, processors) = (c.map_name, c.name, c.seconds, c.path, c.processors);
        if done(ui, map_name) {
            let text = ui.text(map_name.expect("shown"));
            input_edit::rename_map(&mut self.file, self.map, &text);
        }
        if done(ui, name) {
            let text = ui.text(name.expect("shown"));
            let action = self.row.map(Row::action);
            if let (Some(map), Some(action)) = (self.file.maps.get_mut(self.map), action) {
                input_edit::rename_action(map, action, &text);
            }
        }
        if done(ui, path) {
            let text = ui.text(path.expect("shown"));
            if let (Some(map), Some(row)) = (self.file.maps.get_mut(self.map), self.row) {
                input_edit::set_path(map, row, text.trim());
            }
        }
        if done(ui, processors) {
            let text = ui.text(processors.expect("shown"));
            let list = split_calls(&text);
            if let (Some(map), Some(row)) = (self.file.maps.get_mut(self.map), self.row) {
                input_edit::set_processors(map, row, list);
            }
        }
        self.take_action_choices(ui, done(ui, seconds));
        self.editing = focused.filter(|f| [map_name, name, seconds, path, processors].contains(&Some(*f)));
    }

    /// The picked action's type, interaction, category and hidden.
    fn take_action_choices(&mut self, ui: &mut Ui, seconds_done: bool) {
        let Some(Row::Action(index)) = self.row else { return };
        let c = &self.controls;
        let changed = |node: Option<NodeId>| node.is_some_and(|n| ui.changed(n));
        let (kind, interaction, seconds, category, hidden) = (c.kind, c.interaction, c.seconds, c.category, c.hidden);
        let categories: Vec<String> = self.file.categories.iter().map(|c| c.name.clone()).collect();
        let Some(action) = self.file.maps.get_mut(self.map).and_then(|m| m.actions.get_mut(index)) else { return };
        if let Some(kind) = kind.filter(|&k| changed(Some(k))) {
            action.kind = [ActionKind::Button, ActionKind::Value, ActionKind::Vector2][ui.selected_option(kind).min(2)];
        }
        if changed(interaction) || seconds_done {
            let picked = interaction.map_or(0, |i| ui.selected_option(i));
            let typed = seconds.and_then(|s| ui.text(s).trim().parse::<f32>().ok()).filter(|s| *s > 0.0);
            input_edit::set_interaction(action, picked, typed);
        }
        if let Some(category) = category.filter(|&c| changed(Some(c))) {
            action.category = ui.selected_option(category).checked_sub(1).and_then(|at| categories.get(at).cloned());
        }
        if let Some(hidden) = hidden.filter(|&h| changed(Some(h))) {
            action.hidden = ui.is_checked(hidden);
        }
    }

    /// The picked binding's control as the game's prompts draw it on the
    /// device the window last heard.
    fn show_glyph(&self, ui: &mut Ui, input: &Input) {
        let Some(glyph) = self.controls.glyph else { return };
        let path = self.row.and_then(|row| self.file.maps.get(self.map).and_then(|map| input_edit::path(map, row)));
        let glyphs = path.filter(|path| !path.is_empty()).map(|path| keymaps::control_prompt_glyph(input, &path));
        ui.set_prompt(glyph, glyphs.into_iter().collect());
    }

    /// Shows the maps, the picked map's rows and the picked row's properties.
    fn show(&mut self, ui: &mut Ui) {
        *self.shown_maps.borrow_mut() = self.file.maps.iter().map(|m| m.name.clone()).collect();
        ui.set_list_count(self.maps, self.file.maps.len());
        ui.refresh_list(self.maps);
        if !self.file.maps.is_empty() {
            ui.set_selection(self.maps, &[self.map]);
            let _ = ui.selection_changed(self.maps);
        }
        let map = self.file.maps.get(self.map);
        let rows = map.map(input_edit::rows).unwrap_or_default();
        let depth = |row: &Row| match row {
            Row::Action(_) => 0,
            Row::Binding(..) => 1,
            Row::Part(..) => 2,
        };
        *self.shown_rows.borrow_mut() =
            rows.iter().map(|row| (map.map(|m| input_edit::label(m, *row)).unwrap_or_default(), depth(row))).collect();
        ui.set_list_count(self.rows, rows.len());
        ui.refresh_list(self.rows);
        let picked: Vec<usize> = self.row.and_then(|row| rows.iter().position(|r| *r == row)).into_iter().collect();
        ui.set_selection(self.rows, &picked);
        let _ = ui.selection_changed(self.rows);
        self.show_properties(ui);
    }

    fn show_properties(&mut self, ui: &mut Ui) {
        for child in ui.children(self.properties) {
            ui.remove(child);
        }
        self.controls = Properties::default();
        let Some(map) = self.file.maps.get(self.map).cloned() else { return };
        let properties = self.properties;
        let field = |ui: &mut Ui, label: &str| {
            let row = ui.panel(properties, Style::row());
            ui.add_class(row, "inspector-row");
            let label = ui.add_label(row, label);
            ui.add_class(label, "input-property");
            row
        };
        match self.row {
            None => {
                let row = field(ui, "Map");
                self.controls.map_name = Some(field_in(ui, row, &map.name));
            }
            Some(Row::Action(index)) => self.show_action(ui, &map, index, &field),
            Some(row) => {
                let at = field(ui, "Path");
                let path = field_in(ui, at, &input_edit::path(&map, row).unwrap_or_default());
                ui.set_name(path, "input-path");
                self.controls.path = Some(path);
                let listen = ui.button(at, if self.listening == Some(row) { "Press a Key" } else { "Listen" });
                self.controls.listen = Some(listen);
                let at = field(ui, "Prompt");
                self.controls.glyph = Some(ui.prompt(at, ""));
                let processors = input_edit::processors(&map, row).join(", ");
                let at = field(ui, "Processors");
                self.controls.processors = Some(field_in(ui, at, &processors));
            }
        }
    }

    fn show_action(&mut self, ui: &mut Ui, map: &MapData, index: usize, field: &dyn Fn(&mut Ui, &str) -> NodeId) {
        let Some(action) = map.actions.get(index) else { return };
        let row = field(ui, "Name");
        self.controls.name = Some(field_in(ui, row, &action.name));
        let row = field(ui, "Type");
        let kind = [ActionKind::Button, ActionKind::Value, ActionKind::Vector2].iter().position(|k| *k == action.kind);
        self.controls.kind = Some(ui.dropdown(row, &KINDS, kind.unwrap_or(0)));
        let (interaction, seconds) = input_edit::interaction(action);
        let row = field(ui, "Interaction");
        self.controls.interaction = Some(ui.dropdown(row, &INTERACTIONS, interaction));
        if interaction > 0 {
            let row = field(ui, "Seconds");
            let default = if interaction == 1 { 0.4 } else { 0.2 };
            self.controls.seconds = Some(field_in(ui, row, &seconds.unwrap_or(default).to_string()));
        }
        let mut categories = vec!["None".to_string()];
        categories.extend(self.file.categories.iter().map(|c| c.name.clone()));
        let picked = action.category.as_ref().and_then(|name| categories.iter().position(|c| c == name)).unwrap_or(0);
        let labels: Vec<&str> = categories.iter().map(String::as_str).collect();
        let row = field(ui, "Category");
        self.controls.category = Some(ui.dropdown(row, &labels, picked));
        let row = field(ui, "Hidden");
        let hidden = ui.toggle(row, "", action.hidden);
        ui.add_class(hidden, "checkbox");
        self.controls.hidden = Some(hidden);
    }
}

/// A property's text field, taking the rest of its row.
fn field_in(ui: &mut Ui, row: NodeId, text: &str) -> NodeId {
    let field = ui.text_field(row, text);
    ui.add_class(field, "input-field");
    field
}

/// Processors as typed, `deadzone(0.1, 0.9), invert`: split at the commas
/// between calls, not those inside one.
fn split_calls(text: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                calls.push(text[start..at].trim().to_string());
                start = at + 1;
            }
            _ => {}
        }
    }
    calls.push(text[start..].trim().to_string());
    calls.retain(|call| !call.is_empty());
    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processors_split_between_calls() {
        assert_eq!(split_calls("deadzone(0.1, 0.9), invert"), ["deadzone(0.1, 0.9)", "invert"]);
        assert!(split_calls("  ").is_empty());
    }
}
