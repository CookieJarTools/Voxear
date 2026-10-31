//! The Inspector: the selected Obj's header (its kind, name, and the
//! active, tag, layer and static settings to come), then a section for its
//! Transform and for each component (see `props`), divided by lines. Values
//! apply as they are typed or picked, and values that change elsewhere show
//! here, except in the field being edited. When an Obj's components, or what
//! they show, change (a light of another kind, a component added or undone),
//! the Inspector is built again.
//!
//! Menus: a context click on a value's row offers Reset for the Transform
//! and script fields, and while playing, "Keep value after play", which
//! brings the value the played Obj has when play stops back into the edited
//! level, as one step to undo; kept rows are marked (`kept`). A context
//! click on a component's header offers Remove Component, and the
//! Transform's menu button Reset. Add Component lists the components and
//! the project's scripts (`script_types`) the Obj does not have yet.
//!
//! Script fields show as their kind says: a list as its items, each with a
//! button to take it out and the list's to add one; a `[Serializable]`
//! table as its fields, indented under it; and a reference to an Obj, a
//! behaviour or an asset as what it points at, picked from a menu.

use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{Game, script_types::ScriptTypes};
use voxear_render::{Camera, Light, MeshRenderer, Renderer};
use voxear_scene::{Name, Transform};
use voxear_ui::{DragSource, MenuItem, NodeId, Style, Ui};

use crate::{
    fields::Pick,
    history::{Edit, ObjState},
    props::{self, Part, Prop, Value},
};

/// The fields showing an Obj's name and Transform, by what they edit, and
/// the row each is on.
struct Fields {
    name: NodeId,
    position: [NodeId; 3],
    rotation: [NodeId; 3],
    scale: [NodeId; 3],
    rows: [(Field, NodeId); 4],
}

/// A value the menus can keep after play or reset: the name, a Transform
/// row, or a component's value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Position,
    Rotation,
    Scale,
    Prop(Prop),
}

impl Field {
    pub fn label(&self) -> String {
        match self {
            Field::Name => "Name".into(),
            Field::Position => "Position".into(),
            Field::Rotation => "Rotation".into(),
            Field::Scale => "Scale".into(),
            Field::Prop(prop) => props::label(prop),
        }
    }

    /// Copies the name or a Transform value from one state of an Obj into
    /// another. A component's value goes through `props` instead.
    pub fn copy(&self, from: &ObjState, to: &mut ObjState) {
        match self {
            Field::Name => to.name.clone_from(&from.name),
            Field::Position => to.transform.position = from.transform.position,
            Field::Rotation => to.transform.rotation = from.transform.rotation,
            Field::Scale => to.transform.scale = from.transform.scale,
            Field::Prop(_) => {}
        }
    }

    /// Sets a Transform value as Unity's Reset leaves it. Others are left as
    /// they are.
    pub fn reset(&self, state: &mut ObjState) {
        match self {
            Field::Position => state.transform.position = Vec3::ZERO,
            Field::Rotation => state.transform.rotation = Quat::IDENTITY,
            Field::Scale => state.transform.scale = Vec3::ONE,
            Field::Name | Field::Prop(_) => {}
        }
    }
}

/// What a menu in the Inspector asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Keep the value after play, or stop keeping it.
    Keep(Field),
    Reset(Field),
    ResetTransform,
    Add(Part),
    Remove(Part),
    /// Point a reference at what its menu picked.
    Pick(Prop, Pick),
    /// Point a reference at what was dragged onto it, from the Hierarchy or
    /// the Project, if it fits.
    Dropped(Prop, DragSource),
}

/// A component value's row and the control that edits it.
struct PropRow {
    prop: Prop,
    row: NodeId,
    control: Control,
    /// A list's button that adds an item, and an item's that takes it out.
    add: Option<NodeId>,
    remove: Option<NodeId>,
}

enum Control {
    /// A field typed in: a number, a whole number or text.
    Text(NodeId),
    Toggle(NodeId),
    /// A field for each number, for a vector or a colour, with the
    /// colour's swatch.
    Axes(Vec<NodeId>, Option<NodeId>),
    /// A reference: a button showing what it points at, which opens a menu.
    Picker(NodeId, NodeId),
    /// A dropdown, and the options it was last given.
    Choice(NodeId, Vec<String>),
    /// A label, for what is shown but not edited here.
    Fixed(NodeId),
}

pub struct Inspector {
    root: NodeId,
    shown: Option<Entity>,
    fields: Option<Fields>,
    /// The Transform's menu button, and each section's header with its
    /// component (`None` for the Transform).
    transform_menu: Option<NodeId>,
    headers: Vec<(Option<Part>, NodeId)>,
    rows: Vec<PropRow>,
    add: Option<NodeId>,
    /// The project's scripts, as their last build describes them.
    types: ScriptTypes,
    /// The components and values shown, and whether the game was playing,
    /// to build again when either changes.
    built: (Vec<(Part, Vec<Prop>)>, bool),
    /// The element whose menu is open, and what each of its items does.
    menu: Option<(NodeId, Vec<Option<RowAction>>)>,
}

impl Inspector {
    pub fn new(root: NodeId) -> Self {
        Self {
            root,
            shown: None,
            fields: None,
            transform_menu: None,
            headers: Vec::new(),
            rows: Vec::new(),
            add: None,
            types: ScriptTypes::default(),
            built: (Vec::new(), false),
            menu: None,
        }
    }

    /// The Obj showing, if any.
    pub fn shown(&self) -> Option<Entity> {
        self.shown
    }

    /// The project's scripts as a new build describes them. What is showing
    /// is built again on the next update.
    pub fn set_types(&mut self, types: ScriptTypes) {
        self.types = types;
        self.built.0.clear();
    }

    pub fn types(&self) -> &ScriptTypes {
        &self.types
    }

    /// Shows `entity`, or a hint when nothing is selected. While `playing`,
    /// scripts show the values they started with, which cannot be edited.
    pub fn show(&mut self, ui: &mut Ui, game: &Game, entity: Option<Entity>, playing: bool) {
        for child in ui.children(self.root) {
            ui.remove(child);
        }
        let world = &game.engine.world;
        self.shown = entity.filter(|&e| world.get::<Transform>(e).is_some());
        (self.fields, self.transform_menu, self.add, self.menu) = (None, None, None, None);
        self.headers.clear();
        self.rows.clear();
        let Some(entity) = self.shown else {
            let hint = ui.add_label(self.root, "Select an Obj in the Hierarchy to see it here.");
            ui.add_class(hint, "placeholder");
            return;
        };
        let (name_row, name) = header(ui, self.root, game, entity);

        let (transform, rows, menu) = section(ui, self.root, "Transform", "transform", false);
        let (position_row, position) = vector_row(ui, rows, "Position");
        let (rotation_row, rotation) = vector_row(ui, rows, "Rotation");
        let (scale_row, scale) = vector_row(ui, rows, "Scale");
        let rows = [
            (Field::Name, name_row),
            (Field::Position, position_row),
            (Field::Rotation, rotation_row),
            (Field::Scale, scale_row),
        ];
        self.fields = Some(Fields { name, position, rotation, scale, rows });
        self.transform_menu = menu;
        self.headers.push((None, transform));

        let layout = layout(game, entity, &self.types);
        for (part, values) in &layout {
            let (header, rows, _) = section(ui, self.root, part.title(), part.icon(), true);
            self.headers.push((Some(part.clone()), header));
            if values.is_empty()
                && let Part::Behaviour(name) = part
            {
                let row = prop(ui, rows, "Script");
                let label = ui.add_label(row, name);
                ui.add_class(label, "value");
            }
            for value in values {
                let row = prop(ui, rows, &props::label(value));
                let depth = props::depth(value);
                if depth > 0 {
                    let label = ui.children(row)[0];
                    ui.set_inline_style(label, &format!("padding-left: {}px", depth * 12));
                }
                let shown = props::get(game, &self.types, entity, value).unwrap_or(Value::Fixed(String::new()));
                let editable = !(playing && matches!(value, Prop::Field { .. }));
                // An item of a list, or a list, gets its button beside its value.
                let item = editable && props::is_item(value);
                let list = editable && matches!(shown, Value::List(_));
                let holder = if item || list {
                    let holder = ui.panel(row, Style::row());
                    ui.add_class(holder, "prop-value");
                    holder
                } else {
                    row
                };
                let control = control(ui, holder, &shown, editable);
                let button = |ui: &mut Ui, icon: &str, class: &str| {
                    let button = ui.icon_button(holder, icon);
                    ui.add_class(button, class);
                    button
                };
                let add = list.then(|| button(ui, "plus", "list-add"));
                let remove = item.then(|| button(ui, "close", "list-remove"));
                self.rows.push(PropRow { prop: value.clone(), row, control, add, remove });
            }
        }
        let add = ui.button(self.root, "Add Component");
        ui.add_class(add, "add-component");
        self.add = Some(add);
        self.built = (layout, playing);
    }

    /// Applies what was typed or picked, and shows changes made elsewhere.
    /// Returns what was edited this frame, for undo.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        game: &mut Game,
        mut renderer: Option<&mut Renderer>,
        playing: bool,
    ) -> Option<Edit> {
        let entity = self.shown?;
        let Some(&transform) = game.engine.world.get::<Transform>(entity) else {
            self.show(ui, game, None, playing);
            return None;
        };
        if layout(game, entity, &self.types) != self.built.0 || playing != self.built.1 {
            self.show(ui, game, Some(entity), playing);
            return None;
        }
        let before = ObjState::of(&game.engine.world, entity);
        self.update_transform(ui, game, entity, transform);
        for row in &self.rows {
            // Adding or taking out an item changes the rows, which are built again next frame.
            if row.add.is_some_and(|b| ui.clicked(b)) {
                props::add_item(&mut game.engine.world, &self.types, entity, &row.prop);
            }
            if row.remove.is_some_and(|b| ui.clicked(b)) {
                props::remove_item(&mut game.engine.world, &self.types, entity, &row.prop);
            }
        }
        for row in &mut self.rows {
            let Some(current) = props::get(game, &self.types, entity, &row.prop) else { continue };
            if let Some(picked) = read_control(ui, &row.control, &current) {
                props::set(game, renderer.as_deref_mut(), &self.types, entity, &row.prop, picked);
            }
            if let Some(now) = props::get(game, &self.types, entity, &row.prop) {
                refresh_control(ui, &mut row.control, &now);
            }
        }
        let (before, after) = (before?, ObjState::of(&game.engine.world, entity)?);
        (before != after).then_some(Edit::Changed { entity, before, after })
    }

    fn update_transform(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity, transform: Transform) {
        let Some(fields) = &self.fields else { return };
        let world = &mut game.engine.world;
        if ui.changed(fields.name) {
            let name = ui.text(fields.name);
            if let Some(current) = world.get_mut::<Name>(entity) {
                current.0 = name;
            }
        }
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        let euler = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
        let mut edited = transform;
        edited.position = read(ui, &fields.position, transform.position);
        let rotation = read(ui, &fields.rotation, euler);
        if rotation != euler {
            let r = rotation * (std::f32::consts::PI / 180.0);
            edited.rotation = Quat::from_euler(EulerRot::YXZ, r.y, r.x, r.z);
        }
        edited.scale = read(ui, &fields.scale, transform.scale);
        if edited != transform
            && let Some(current) = world.get_mut::<Transform>(entity)
        {
            *current = edited;
        }
        write(ui, &fields.position, edited.position);
        write(ui, &fields.rotation, if rotation != euler { rotation } else { euler });
        write(ui, &fields.scale, edited.scale);
    }

    /// Marks the rows whose values are `kept`, and opens and answers the
    /// Inspector's menus: a row's (while `playing`, whether to keep its
    /// value, offered when `can_keep` as the Obj is in the edited level),
    /// a section's, and Add Component's. Returns what was chosen.
    pub fn menus(
        &mut self,
        ui: &mut Ui,
        game: &Game,
        playing: bool,
        can_keep: bool,
        kept: &[Field],
    ) -> Option<RowAction> {
        let fields = self.fields.as_ref()?;
        let rows: Vec<(Field, NodeId)> =
            fields.rows.iter().cloned().chain(self.rows.iter().map(|r| (Field::Prop(r.prop.clone()), r.row))).collect();
        for (field, row) in &rows {
            ui.set_class(*row, "kept", kept.contains(field));
        }
        if let Some((owner, actions)) = &self.menu
            && let Some(index) = ui.menu_choice(*owner)
        {
            return actions.get(index).cloned().flatten();
        }
        let dropped = self.rows.iter().find_map(|row| match row.control {
            Control::Picker(picker, _) => ui.dropped_onto(picker).map(|source| (row.prop.clone(), source)),
            _ => None,
        });
        if let Some((prop, source)) = dropped {
            return Some(RowAction::Dropped(prop, source));
        }
        let at = ui.pointer_position().unwrap_or_default();
        let items: Vec<(MenuItem, Option<RowAction>)>;
        let (owner, at) = if let Some(add) = self.add.filter(|&add| ui.clicked(add)) {
            items = self.add_items(playing);
            let [x, y, _, height] = ui.rect(add);
            (add, (x, y + height + 4.0))
        } else if let Some(button) = self.transform_menu.filter(|&b| ui.clicked(b)) {
            items = vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))];
            let [x, y, _, height] = ui.rect(button);
            (button, (x, y + height + 4.0))
        } else if let Some((prop, button)) = self.picker_clicked(ui) {
            let entity = self.shown?;
            items = props::choices(game, &self.types, entity, &prop)
                .into_iter()
                .map(|(label, pick)| (MenuItem::new(&label), Some(RowAction::Pick(prop.clone(), pick))))
                .collect();
            let [x, y, _, height] = ui.rect(button);
            (button, (x, y + height + 4.0))
        } else if let Some((part, header)) = self.headers.iter().find(|(_, h)| ui.context_clicked(*h)) {
            items = match part {
                None => vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))],
                Some(part) => {
                    let removable = !(playing && matches!(part, Part::Behaviour(_)));
                    vec![(MenuItem::new("Remove Component").enabled(removable), Some(RowAction::Remove(part.clone())))]
                }
            };
            (*header, at)
        } else if let Some((field, row)) = rows.iter().find(|(_, row)| ui.context_clicked(*row)) {
            items = row_items(field, playing, can_keep, kept.contains(field));
            (*row, at)
        } else {
            return None;
        };
        if items.is_empty() {
            return None;
        }
        let (items, actions): (Vec<MenuItem>, Vec<Option<RowAction>>) = items.into_iter().unzip();
        ui.open_menu(owner, at, &items);
        self.menu = Some((owner, actions));
        None
    }

    /// The reference whose button was clicked, and the button.
    fn picker_clicked(&self, ui: &Ui) -> Option<(Prop, NodeId)> {
        self.rows.iter().find_map(|row| match row.control {
            Control::Picker(button, _) if ui.clicked(button) => Some((row.prop.clone(), button)),
            _ => None,
        })
    }

    /// What Add Component offers: the components the Obj has not got, then
    /// the project's scripts, which are added in edit mode.
    fn add_items(&self, playing: bool) -> Vec<(MenuItem, Option<RowAction>)> {
        let has = |part: &Part| self.built.0.iter().any(|(p, _)| p == part);
        let mut items: Vec<(MenuItem, Option<RowAction>)> = [Part::Renderer, Part::Light, Part::Camera]
            .into_iter()
            .map(|part| (MenuItem::new(part.title()).enabled(!has(&part)), Some(RowAction::Add(part))))
            .collect();
        if !self.types.behaviours.is_empty() {
            items.push((MenuItem::separator(), None));
        }
        for script in &self.types.behaviours {
            let part = Part::Behaviour(script.name.clone());
            let item = MenuItem::new(script.short_name()).enabled(!playing && !has(&part));
            items.push((item, Some(RowAction::Add(part))));
        }
        items
    }
}

/// A row's menu: while playing, keeping its value, for any but a script's
/// (those are the values it started with); in edit mode, Reset, for the
/// Transform and script fields.
fn row_items(field: &Field, playing: bool, can_keep: bool, kept: bool) -> Vec<(MenuItem, Option<RowAction>)> {
    let script = matches!(field, Field::Prop(Prop::Field { .. }));
    let mut items = Vec::new();
    if playing && !script {
        let keep = MenuItem::new("Keep value after play").checked(kept).enabled(can_keep);
        items.push((keep, Some(RowAction::Keep(field.clone()))));
    }
    let whole_field = matches!(field, Field::Prop(prop) if props::depth(prop) == 0);
    let resets =
        matches!(field, Field::Position | Field::Rotation | Field::Scale) || (script && whole_field && !playing);
    if resets {
        if !items.is_empty() {
            items.push((MenuItem::separator(), None));
        }
        let label = if script { "Reset to the script's value" } else { "Reset" };
        items.push((MenuItem::new(label), Some(RowAction::Reset(field.clone()))));
    }
    items
}

/// The components an Obj has and the values each shows.
fn layout(game: &Game, entity: Entity, types: &ScriptTypes) -> Vec<(Part, Vec<Prop>)> {
    let world = &game.engine.world;
    props::parts(world, entity)
        .into_iter()
        .map(|part| (part.clone(), props::props(world, entity, &part, types)))
        .collect()
}

/// The control for a value: a field, a toggle, three fields, a dropdown, or
/// a label when it is not edited here.
fn control(ui: &mut Ui, row: NodeId, value: &Value, editable: bool) -> Control {
    if !editable {
        let label = ui.add_label(row, &display(value));
        ui.add_class(label, "value");
        return Control::Fixed(label);
    }
    match value {
        Value::Number(_) | Value::Whole(_) | Value::Text(_) => {
            let field = ui.text_field(row, &display(value));
            ui.add_class(field, "value-input");
            ui.set_class(field, "number-input", !matches!(value, Value::Text(_)));
            Control::Text(field)
        }
        Value::Toggle(on) => {
            let toggle = ui.toggle(row, "", *on);
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        Value::Vector(_) | Value::Color(_) => {
            let colour = matches!(value, Value::Color(_));
            let count = numbers(value).len();
            let letters = if colour { &["r", "g", "b"][..] } else { &["x", "y", "z", "w"][..count.min(4)] };
            let vector = ui.panel(row, Style::default());
            ui.add_class(vector, "vector");
            ui.set_class(vector, "color", colour);
            ui.set_class(vector, &format!("vector-{count}"), !colour);
            let swatch = colour.then(|| {
                let swatch = ui.panel(vector, Style::default());
                ui.add_class(swatch, "swatch");
                swatch
            });
            let fields = axis_fields(ui, vector, letters);
            let mut control = Control::Axes(fields, swatch);
            refresh_control(ui, &mut control, value);
            control
        }
        Value::Obj(_) | Value::Asset(_) => {
            let picker = ui.panel(row, Style::row());
            ui.add_class(picker, "value-picker");
            ui.accept_drops(picker, true);
            let label = ui.add_label(picker, &display(value));
            ui.add_class(label, "picker-label");
            let arrow = ui.icon(picker, "chevron-down");
            ui.add_class(arrow, "dropdown-arrow");
            Control::Picker(picker, label)
        }
        Value::Choice(index, options) => {
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            let dropdown = ui.dropdown(row, &labels, *index);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown, options.clone())
        }
        Value::Fixed(_) | Value::List(_) | Value::Group => {
            let label = ui.add_label(row, &display(value));
            ui.add_class(label, "value");
            Control::Fixed(label)
        }
    }
}

/// What the user changed in a control this frame, as a value of the same
/// kind as `current`.
fn read_control(ui: &Ui, control: &Control, current: &Value) -> Option<Value> {
    match (control, current) {
        (Control::Text(field), _) if !ui.changed(*field) => None,
        (Control::Text(field), Value::Number(_)) => ui.text(*field).trim().parse().ok().map(Value::Number),
        (Control::Text(field), Value::Whole(_)) => ui.text(*field).trim().parse().ok().map(Value::Whole),
        (Control::Text(field), Value::Text(_)) => Some(Value::Text(ui.text(*field))),
        (Control::Toggle(toggle), _) if ui.changed(*toggle) => Some(Value::Toggle(ui.is_checked(*toggle))),
        (Control::Axes(fields, _), Value::Vector(values)) if fields.iter().any(|&f| ui.changed(f)) => {
            Some(Value::Vector(read_numbers(ui, fields, values)))
        }
        (Control::Axes(fields, _), Value::Color(rgb)) if fields.iter().any(|&f| ui.changed(f)) => {
            let rgb = read_numbers(ui, fields, rgb);
            Some(Value::Color([rgb[0], rgb[1], rgb[2]]))
        }
        (Control::Choice(dropdown, options), _) if ui.changed(*dropdown) => {
            Some(Value::Choice(ui.selected_option(*dropdown), options.clone()))
        }
        _ => None,
    }
}

/// Shows a value in its control, except in a field being typed in.
fn refresh_control(ui: &mut Ui, control: &mut Control, value: &Value) {
    match (control, value) {
        (Control::Text(field), value) => {
            let text = display(value);
            if ui.focused() != Some(*field) && ui.text(*field) != text {
                ui.set_text(*field, &text);
            }
        }
        (Control::Toggle(toggle), Value::Toggle(on)) => {
            if ui.is_checked(*toggle) != *on {
                ui.set_checked(*toggle, *on);
            }
        }
        (Control::Axes(fields, swatch), Value::Vector(_) | Value::Color(_)) => {
            write_numbers(ui, fields, &numbers(value));
            if let (Some(swatch), Value::Color(rgb)) = (swatch, value) {
                let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                ui.set_inline_style(*swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            }
        }
        (Control::Picker(_, label), value) => {
            let text = display(value);
            if ui.text(*label) != text {
                ui.set_text(*label, &text);
            }
        }
        (Control::Choice(dropdown, shown), Value::Choice(index, options)) => {
            if shown != options {
                let labels: Vec<&str> = options.iter().map(String::as_str).collect();
                ui.set_options(*dropdown, &labels);
                shown.clone_from(options);
            }
            if ui.selected_option(*dropdown) != *index {
                ui.set_selected_option(*dropdown, *index);
            }
        }
        (Control::Fixed(label), value) => {
            let text = display(value);
            if ui.text(*label) != text {
                ui.set_text(*label, &text);
            }
        }
        _ => {}
    }
}

/// A value as text, for a field or a label.
fn display(value: &Value) -> String {
    match value {
        Value::Number(n) => number(*n),
        Value::Whole(n) => n.to_string(),
        Value::Toggle(on) => if *on { "On" } else { "Off" }.into(),
        Value::Text(text) | Value::Fixed(text) | Value::Obj(text) => text.clone(),
        Value::Asset(path) => path.rsplit('/').next().unwrap_or(path).to_string(),
        Value::Vector(_) | Value::Color(_) => numbers(value).into_iter().map(number).collect::<Vec<_>>().join(", "),
        Value::Choice(index, options) => options.get(*index).cloned().unwrap_or_default(),
        Value::List(1) => "1 item".into(),
        Value::List(count) => format!("{count} items"),
        Value::Group => String::new(),
    }
}

/// A vector's or colour's numbers.
fn numbers(value: &Value) -> Vec<f32> {
    match value {
        Value::Vector(values) => values.clone(),
        Value::Color(rgb) => rgb.to_vec(),
        _ => Vec::new(),
    }
}

/// The Obj's icon, its name field and whether it is active, then rows for
/// its tag, layer and whether it is static. Returns the top row and the
/// name field. Active, tag, layer and static show their defaults until
/// Objs have them.
fn header(ui: &mut Ui, root: NodeId, game: &Game, entity: Entity) -> (NodeId, NodeId) {
    let world = &game.engine.world;
    let header = ui.panel(root, Style::column());
    ui.add_class(header, "inspector-header");
    let top = ui.panel(header, Style::row());
    ui.add_class(top, "header-row");
    let tile = ui.panel(top, Style::row());
    ui.add_class(tile, "obj-tile");
    let kind = crate::hierarchy::icon(
        world.get::<Camera>(entity).is_some(),
        world.get::<Light>(entity).is_some(),
        world.get::<MeshRenderer>(entity).is_some(),
    );
    ui.icon(tile, kind);
    let name = ui.text_field(top, &world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_default());
    ui.add_class(name, "obj-name");
    ui.toggle(top, "", true);
    for (label, choice) in [("Tag", "Untagged"), ("Layer", "Default")] {
        let row = prop(ui, header, label);
        ui.dropdown(row, &[choice], 0);
    }
    let row = prop(ui, header, "Static");
    let fixed = ui.toggle(row, "", false);
    ui.add_class(fixed, "checkbox");
    (top, name)
}

/// A row of a label and three axis fields, X, Y and Z. Returns the row and
/// the fields.
fn vector_row(ui: &mut Ui, rows: NodeId, label: &str) -> (NodeId, [NodeId; 3]) {
    let row = prop(ui, rows, label);
    let vector = ui.panel(row, Style::default());
    ui.add_class(vector, "vector");
    let fields = axis_fields(ui, vector, &["x", "y", "z"]);
    (row, [fields[0], fields[1], fields[2]])
}

/// A field for each letter, each with its coloured letter inside.
fn axis_fields(ui: &mut Ui, vector: NodeId, letters: &[&str]) -> Vec<NodeId> {
    letters
        .iter()
        .map(|axis| {
            let cell = ui.panel(vector, Style::row());
            ui.add_class(cell, "axis-field");
            let letter = ui.add_label(cell, &axis.to_uppercase());
            ui.add_class(letter, "axis-letter");
            ui.add_class(letter, &format!("axis-{axis}"));
            let field = ui.text_field(cell, "");
            ui.add_class(field, "axis-input");
            field
        })
        .collect()
}

/// A component's section: a line above it, then a foldout headed by its
/// icon and title, and at the right a switch for whether it is enabled
/// (shown on, until components can be turned off) or, for the Transform,
/// which cannot be, a menu button. Returns its header, where its rows go,
/// and the menu button.
fn section(ui: &mut Ui, root: NodeId, title: &str, icon: &str, switch: bool) -> (NodeId, NodeId, Option<NodeId>) {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    let header = ui.foldout_header(foldout).expect("a foldout has a header");
    let menu = if switch {
        ui.toggle(header, "", true);
        None
    } else {
        Some(ui.icon_button(header, "more"))
    };
    (header, ui.foldout_content(foldout).expect("a foldout has content"), menu)
}

/// A property's row: its label in the first 88 pixels, the value after.
fn prop(ui: &mut Ui, rows: NodeId, label: &str) -> NodeId {
    let row = ui.panel(rows, Style::default());
    ui.add_class(row, "prop");
    let name = ui.add_label(row, label);
    ui.add_class(name, "prop-label");
    row
}

/// A number as the Inspector writes it: up to three decimals, no trailing zeros.
fn number(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.to_string() }
}

/// What three axis fields say, keeping `current` for any that is not a number
/// or that the user has not changed.
fn read(ui: &Ui, fields: &[NodeId; 3], current: Vec3) -> Vec3 {
    let read = read_numbers(ui, fields, &current.to_array());
    Vec3::new(read[0], read[1], read[2])
}

/// Shows `value` in fields the user is not editing.
fn write(ui: &mut Ui, fields: &[NodeId; 3], value: Vec3) {
    write_numbers(ui, fields, &value.to_array());
}

/// What a row of number fields says, keeping `current` for any that is not
/// a number or that the user has not changed.
fn read_numbers(ui: &Ui, fields: &[NodeId], current: &[f32]) -> Vec<f32> {
    let mut values = current.to_vec();
    for (value, field) in values.iter_mut().zip(fields) {
        if ui.changed(*field)
            && let Ok(number) = ui.text(*field).trim().parse::<f32>()
        {
            *value = number;
        }
    }
    values
}

/// Shows numbers in fields the user is not editing.
fn write_numbers(ui: &mut Ui, fields: &[NodeId], values: &[f32]) {
    for (field, value) in fields.iter().zip(values) {
        let text = number(*value);
        if ui.focused() != Some(*field) && ui.text(*field) != text {
            ui.set_text(*field, &text);
        }
    }
}
