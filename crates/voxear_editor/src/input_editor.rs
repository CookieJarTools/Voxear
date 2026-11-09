//! The Input Actions window's Actions view, where the project's actions
//! are made and changed, as in Unity's Input Actions window: the action
//! maps on the left, the picked map's actions in the middle, each followed
//! by its bindings and a composite's parts, and the picked row's
//! properties on the right.
//!
//! - A map has its name and the maps it excludes, which are never on while
//!   it is, so their controls do not conflict with its.
//! - An action has its name, type, interaction, category, icon and whether
//!   it is hidden. Its icon is chosen from Voxear's in a picker.
//! - A binding has its control, chosen from a picker of every control by
//!   device, searched as Unity's binding picker is, or taken by Listen from
//!   the next key or button pressed; and its processors, each picked from
//!   a dropdown with a field for each of its values, as a deadzone's Min and
//!   Max.
//! - The plus over the actions adds an action, or a binding, 2D vector or
//!   1D axis to the action picked; the plus on an action's own row adds one
//!   of those to it. A new action is named so no other is,
//!   picked, and renamed in its row at once, its name selected to type
//!   over: Enter or a click elsewhere keeps what is typed, and Escape the
//!   name it was given.
//!
//! Each row shows an icon (an action's own, or a binding's device), a
//! binding's control by its name with its device after it, and a mark
//! where a control conflicts with another action's that can be on at once.
//! Rows alternate shades, to follow along.
//!
//! Each change is made to a copy of the file (see `input_edit`), checked as
//! the game would read it, and handed to the editor to write and read
//! again. A change the game would not read is not kept, and says why.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use voxear_input::{ActionKind, ActionsFile, Input, Processor, Rebound, Scheme, actions::MapData, controls};
use voxear_ui::{ItemFns, MenuItem, NodeId, Style, Ui};

use voxear_engine::keymaps;

use crate::{
    columns,
    input_edit::{self, Composite, Row, Shown},
};

const ROW: f32 = 22.0;
const KINDS: [&str; 3] = ["Button", "Value", "Vector 2"];
const INTERACTIONS: [&str; 3] = ["Press", "Hold", "Tap"];
/// What the plus over the actions adds.
const ADDS: [&str; 4] = ["Action", "Binding", "2D Vector", "1D Axis"];

/// A picker the window asks the editor to open, under `anchor`, with what
/// is chosen now.
pub enum Browse {
    Control { anchor: NodeId, now: String },
    Icon { anchor: NodeId, now: String },
}

/// What a picker the window asked for picked.
pub enum Picked {
    /// A binding's control, by its path.
    Control(String),
    /// An action's icon, or its type's when none.
    Icon(Option<String>),
}

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
    icon: Option<NodeId>,
    /// Each other map's toggle, for whether the picked map excludes it.
    excludes: Vec<(NodeId, usize)>,
    /// The binding's control, which opens the control picker.
    control: Option<NodeId>,
    /// The binding's control as a game's prompt draws it.
    glyph: Option<NodeId>,
    listen: Option<NodeId>,
    /// The plus that adds a processor, from a menu of their kinds.
    add_processor: Option<NodeId>,
    /// Each processor's dropdown, its values' fields and its remove button.
    processors: Vec<ProcessorControls>,
}

/// An action being renamed in its row: which, and its field once the row
/// shows it and it has the focus.
struct Renaming {
    action: usize,
    field: Option<NodeId>,
}

/// One processor's controls in the properties.
#[derive(Clone)]
struct ProcessorControls {
    kind: NodeId,
    values: Vec<NodeId>,
    remove: NodeId,
}

pub struct ActionsEditor {
    maps: NodeId,
    rows: NodeId,
    properties: NodeId,
    add_map: NodeId,
    remove_map: NodeId,
    add: NodeId,
    remove: NodeId,
    status: NodeId,
    shown_maps: Rc<RefCell<Vec<String>>>,
    shown_rows: Rc<RefCell<Vec<Shown>>>,
    /// Each row made and the place it shows, as last bound, to find the row
    /// being renamed.
    bound_rows: Rc<RefCell<Vec<(NodeId, usize)>>>,
    /// The place of the row renamed in place, which shows a field for it.
    renaming_row: Rc<Cell<Option<usize>>>,
    renaming: Option<Renaming>,
    /// The plus on an action's row whose menu is open, and the action.
    row_add: Option<(NodeId, usize)>,
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
    browse: Option<Browse>,
    picked: Vec<Picked>,
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
        let add = ui.icon_button(heading, "plus");
        ui.set_name(add, "input-add");
        let remove = ui.icon_button(heading, "close");
        let shown_rows: Rc<RefCell<Vec<Shown>>> = Rc::default();
        let bound_rows: Rc<RefCell<Vec<(NodeId, usize)>>> = Rc::default();
        let renaming_row: Rc<Cell<Option<usize>>> = Rc::default();
        let (texts, bound, renamed) = (shown_rows.clone(), bound_rows.clone(), renaming_row.clone());
        let rows = ui.list_view(
            middle,
            ROW,
            0,
            ItemFns {
                make: |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "entities-row");
                    ui.add_class(row, "input-row");
                    let icon = ui.icon(row, "play");
                    ui.add_class(icon, "input-row-icon");
                    let label = ui.add_label(row, "");
                    ui.add_class(label, "entities-row-name");
                    let rename = ui.text_field(row, "");
                    ui.add_class(rename, "input-row-rename");
                    ui.set_visible(rename, false);
                    let detail = ui.add_label(row, "");
                    ui.add_class(detail, "input-row-detail");
                    let conflict = ui.icon(row, "alert");
                    ui.add_class(conflict, "input-row-conflict");
                    let add = ui.icon_button(row, "plus");
                    ui.add_class(add, "input-row-add");
                    ui.set_tooltip(add, "Add Binding", None);
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let Some(shown) = texts.borrow().get(index).cloned() else { return };
                    let parts = ui.children(row);
                    let part = |class: &str| parts.iter().copied().find(|&p| ui.has_class(p, class));
                    let (Some(icon), Some(label), Some(rename), Some(detail), Some(conflict), Some(add)) = (
                        part("input-row-icon"),
                        part("entities-row-name"),
                        part("input-row-rename"),
                        part("input-row-detail"),
                        part("input-row-conflict"),
                        part("input-row-add"),
                    ) else {
                        return;
                    };
                    ui.set_icon(icon, shown.icon);
                    ui.set_inline_style(icon, &format!("margin-left: {}px", shown.depth * 16));
                    ui.set_text(label, &shown.text);
                    ui.set_text(detail, &shown.detail);
                    ui.set_visible(conflict, shown.conflict);
                    ui.set_visible(add, shown.action);
                    ui.set_class(row, "odd", index % 2 == 1);
                    ui.set_class(row, "conflict", shown.conflict);
                    ui.set_class(row, "unset", shown.unset);
                    let renaming = renamed.get() == Some(index);
                    ui.set_visible(rename, renaming);
                    ui.set_visible(label, !renaming);
                    let mut bound = bound.borrow_mut();
                    match bound.iter_mut().find(|(made, _)| *made == row) {
                        Some(found) => found.1 = index,
                        None => bound.push((row, index)),
                    }
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
            add,
            remove,
            status,
            shown_maps,
            shown_rows,
            bound_rows,
            renaming_row,
            renaming: None,
            row_add: None,
            controls: Properties::default(),
            file: ActionsFile::default(),
            read: None,
            map: 0,
            row: None,
            listening: None,
            editing: None,
            browse: None,
            picked: Vec::new(),
        }
    }

    /// The picker the window asks to open, once.
    pub fn take_browse(&mut self) -> Option<Browse> {
        self.browse.take()
    }

    /// What a picker the window asked for picked, taken on its next update.
    pub fn pick(&mut self, picked: Picked) {
        self.picked.push(picked);
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
        self.take_rename(ui);
        self.take_picked();
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
        // Each action is known by an id from when it is made, which a script's field or its Actions class names.
        self.file.fill_ids(|| voxear_assets::AssetGuid::new_random().to_string());
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
            input_edit::remove_map(&mut self.file, self.map);
            self.map = self.map.saturating_sub(1);
            self.row = None;
        }
        if self.file.maps.is_empty() {
            return;
        }
        let action = self.row.map(Row::action);
        if ui.clicked(self.add) {
            let [x, y, _, height] = ui.rect(self.add);
            let items: Vec<MenuItem> = ADDS
                .iter()
                .enumerate()
                .map(|(at, add)| MenuItem::new(add).enabled(at == 0 || action.is_some()))
                .collect();
            ui.open_menu(self.add, (x, y + height + 2.0), &items);
        }
        let composite = |at: usize| [None, None, Some(Composite::Vector), Some(Composite::Axis)][at.min(3)];
        self.open_row_add(ui);
        if let Some((add, action)) = self.row_add
            && let Some(at) = ui.menu_choice(add)
        {
            self.row_add = None;
            let added = self.map_data().and_then(|map| input_edit::add_binding(map, action, composite(at + 1)));
            if added.is_some() {
                self.row = added;
            }
        }
        let added = match ui.menu_choice(self.add) {
            Some(0) => {
                let added = self.map_data().map(input_edit::add_action);
                self.renaming = added.map(|row| Renaming { action: row.action(), field: None });
                added
            }
            Some(at) => {
                action.and_then(|a| self.map_data().and_then(|map| input_edit::add_binding(map, a, composite(at))))
            }
            None => None,
        };
        if added.is_some() {
            self.row = added;
        }
        if let Some(control) = self.controls.control.filter(|&button| ui.clicked(button)) {
            let now = self.row.and_then(|row| self.file.maps.get(self.map).and_then(|m| input_edit::path(m, row)));
            let now = now.map(|path| input_edit::control_name(&path).0).unwrap_or_default();
            self.browse = Some(Browse::Control { anchor: control, now });
        }
        if let Some(icon) = self.controls.icon.filter(|&button| ui.clicked(button)) {
            let action = self.row.and_then(|row| self.file.maps.get(self.map)?.actions.get(row.action()));
            let now = action.and_then(|a| a.icon.clone()).unwrap_or_default();
            self.browse = Some(Browse::Icon { anchor: icon, now });
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

    /// Opens the menu of what can be added to an action, under the plus on
    /// its row that was clicked: a binding, a 2D vector or a 1D axis.
    fn open_row_add(&mut self, ui: &mut Ui) {
        let rows = self.file.maps.get(self.map).map(input_edit::rows).unwrap_or_default();
        let bound = self.bound_rows.borrow().clone();
        for (row, index) in bound {
            let Some(add) = ui.children(row).into_iter().find(|&part| ui.has_class(part, "input-row-add")) else {
                continue;
            };
            let Some(Row::Action(action)) = rows.get(index).copied().filter(|_| ui.clicked(add)) else { continue };
            let [x, y, _, height] = ui.rect(add);
            let items: Vec<MenuItem> = ADDS[1..].iter().map(|added| MenuItem::new(added)).collect();
            ui.open_menu(add, (x, y + height + 2.0), &items);
            self.row_add = Some((add, action));
            self.row = Some(Row::Action(action));
        }
    }

    /// The action renamed in its row: its field shown, focused and its name
    /// selected once its row is there, then kept as typed when Enter is
    /// pressed or the field is left, or as it was when Escape is.
    fn take_rename(&mut self, ui: &mut Ui) {
        let Some(renaming) = &self.renaming else { return };
        let action = renaming.action;
        let rows = self.file.maps.get(self.map).map(input_edit::rows).unwrap_or_default();
        let Some(at) = rows.iter().position(|row| *row == Row::Action(action)) else {
            return self.end_rename(ui);
        };
        let Some(field) = renaming.field else {
            if self.renaming_row.get() != Some(at) {
                self.renaming_row.set(Some(at));
                ui.scroll_to_index(self.rows, at);
                ui.refresh_list(self.rows);
                return;
            }
            // The row is shown once the list binds it again, a frame after it is asked to, with the list's new
            // length, so it is scrolled to again.
            ui.scroll_to_index(self.rows, at);
            let bound = self.bound_rows.borrow().iter().find(|(_, index)| *index == at).map(|(row, _)| *row);
            let field =
                bound.and_then(|row| ui.children(row).into_iter().find(|&p| ui.has_class(p, "input-row-rename")));
            let Some(field) = field.filter(|&field| ui.is_visible(field)) else { return };
            let name = self.file.maps[self.map].actions.get(action).map(|a| a.name.clone()).unwrap_or_default();
            ui.set_text(field, &name);
            ui.focus(field);
            // A field can take focus only once it is laid out; until then this is asked again.
            if ui.focused() == Some(field) {
                ui.select_all_text(field);
                self.renaming = Some(Renaming { action, field: Some(field) });
            }
            return;
        };
        if ui.escaped(field) {
            return self.end_rename(ui);
        }
        if ui.submitted(field) || ui.focused() != Some(field) {
            let text = ui.text(field);
            if let Some(map) = self.file.maps.get_mut(self.map) {
                input_edit::rename_action(map, action, &text);
            }
            self.end_rename(ui);
        }
    }

    fn end_rename(&mut self, ui: &mut Ui) {
        self.renaming = None;
        self.renaming_row.set(None);
        ui.refresh_list(self.rows);
    }

    /// What the pickers the window asked for picked: the binding's control,
    /// or the action's icon.
    fn take_picked(&mut self) {
        let row = self.row;
        for picked in std::mem::take(&mut self.picked) {
            let Some((row, map)) = row.zip(self.file.maps.get_mut(self.map)) else { continue };
            match picked {
                Picked::Control(path) => input_edit::set_path(map, row, &path),
                Picked::Icon(icon) => {
                    if let Some(action) = map.actions.get_mut(row.action()) {
                        action.icon = icon;
                    }
                }
            }
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
        let (map_name, name, seconds) = (c.map_name, c.name, c.seconds);
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
        let fields_done: Vec<bool> =
            (self.controls.processors.iter()).map(|p| p.values.iter().any(|&field| done(ui, Some(field)))).collect();
        self.take_processors(ui, &fields_done);
        self.take_action_choices(ui, done(ui, seconds));
        let excludes = self.controls.excludes.clone();
        for (toggle, other) in excludes.into_iter().filter(|(toggle, _)| ui.changed(*toggle)) {
            input_edit::set_exclusive(&mut self.file, self.map, other, ui.is_checked(toggle));
        }
        let values = self.controls.processors.iter().flat_map(|p| p.values.iter().copied().map(Some));
        let fields: Vec<Option<NodeId>> = [map_name, name, seconds].into_iter().chain(values).collect();
        self.editing = focused.filter(|f| fields.contains(&Some(*f)));
    }

    /// The picked binding's processors: one added from the plus's menu, a
    /// kind picked, a value typed, or one taken away.
    fn take_processors(&mut self, ui: &mut Ui, fields_done: &[bool]) {
        let Some(row) = self.row.filter(|row| !matches!(row, Row::Action(_))) else { return };
        let Some(map) = self.file.maps.get(self.map) else { return };
        let before = input_edit::typed_processors(map, row);
        let mut list = before.clone();
        if let Some(add) = self.controls.add_processor {
            if ui.clicked(add) {
                let [x, y, _, height] = ui.rect(add);
                let items: Vec<MenuItem> = Processor::KINDS.iter().map(|(name, _)| MenuItem::new(name)).collect();
                ui.open_menu(add, (x, y + height + 2.0), &items);
            }
            if let Some(&(_, kind)) = ui.menu_choice(add).and_then(|at| Processor::KINDS.get(at)) {
                list.push(kind);
            }
        }
        let controls = self.controls.processors.clone();
        for (at, shown) in controls.iter().enumerate().rev() {
            if ui.changed(shown.kind) {
                list[at] = Processor::KINDS[ui.selected_option(shown.kind).min(4)].1;
            } else if fields_done.get(at).copied().unwrap_or(false) {
                let typed: Vec<f32> =
                    shown.values.iter().map(|&f| ui.text(f).trim().parse::<f32>().unwrap_or(f32::NAN)).collect();
                list[at] = list[at].with_values(&typed);
            }
            if ui.clicked(shown.remove) {
                list.remove(at);
            }
        }
        if list != before
            && let Some(map) = self.file.maps.get_mut(self.map)
        {
            input_edit::set_processors(map, row, list.iter().map(|p| p.to_text()).collect());
        }
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
        let conflicts = input_edit::conflicts(&self.file, self.map);
        let shown = |(row, conflict): (&Row, &bool)| {
            let shown = map.map(|m| input_edit::shown(m, *row)).unwrap_or_default();
            Shown { conflict: *conflict, ..shown }
        };
        *self.shown_rows.borrow_mut() = rows.iter().zip(&conflicts).map(shown).collect();
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
            None => self.show_map(ui, &map, &field),
            Some(Row::Action(index)) => self.show_action(ui, &map, index, &field),
            Some(row) => {
                let at = field(ui, "Control");
                let path = input_edit::path(&map, row).unwrap_or_default();
                let named = if path.is_empty() { "None".to_string() } else { controls::describe(&path) };
                let control = ui.button(at, &named);
                ui.add_class(control, "input-field");
                ui.add_class(control, "input-picker");
                ui.set_name(control, "input-control");
                self.controls.control = Some(control);
                let at = field(ui, "");
                let listen = ui.button(at, if self.listening == Some(row) { "Press a Key" } else { "Listen" });
                self.controls.listen = Some(listen);
                let at = field(ui, "Prompt");
                self.controls.glyph = Some(ui.prompt(at, ""));
                let at = field(ui, "Processors");
                let add = ui.icon_button(at, "plus");
                ui.set_name(add, "input-add-processor");
                self.controls.add_processor = Some(add);
                for processor in input_edit::typed_processors(&map, row) {
                    // The whole width, past the labels, so a deadzone's two values fit.
                    let at = ui.panel(self.properties, Style::row());
                    ui.add_class(at, "inspector-row");
                    ui.add_class(at, "input-processor-row");
                    self.show_processor(ui, at, processor);
                }
            }
        }
    }

    /// A processor's kind, picked from a dropdown, a field for each of its
    /// values with its name, and a button that takes it away.
    fn show_processor(&mut self, ui: &mut Ui, row: NodeId, processor: Processor) {
        let names: Vec<&str> = Processor::KINDS.iter().map(|(name, _)| *name).collect();
        let kind = ui.dropdown(row, &names, processor.kind());
        ui.add_class(kind, "input-processor");
        let mut values = Vec::new();
        for (name, value) in processor.values() {
            let label = ui.add_label(row, name);
            ui.add_class(label, "input-value-name");
            let field = ui.text_field(row, &value.to_string());
            ui.add_class(field, "input-value");
            values.push(field);
        }
        let remove = ui.icon_button(row, "close");
        self.controls.processors.push(ProcessorControls { kind, values, remove });
    }

    /// A map's name, and a toggle for each other map it may exclude.
    fn show_map(&mut self, ui: &mut Ui, map: &MapData, field: &dyn Fn(&mut Ui, &str) -> NodeId) {
        let row = field(ui, "Map");
        self.controls.map_name = Some(field_in(ui, row, &map.name));
        let others = self.file.maps.iter().enumerate().filter(|(at, _)| *at != self.map);
        for (shown, (at, other)) in others.enumerate() {
            let row = field(ui, if shown == 0 { "Excludes" } else { "" });
            let excluded = self.file.exclusive(&map.name, &other.name);
            let toggle = ui.toggle(row, &other.name, excluded);
            ui.add_class(toggle, "checkbox");
            self.controls.excludes.push((toggle, at));
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
        let row = field(ui, "Icon");
        let icon = ui.icon_button(row, input_edit::action_icon(action));
        ui.set_name(icon, "input-icon");
        self.controls.icon = Some(icon);
        let name = ui.add_label(row, action.icon.as_deref().unwrap_or("Default"));
        ui.add_class(name, "input-icon-name");
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
