//! Fields the scripts' drawers draw (`ValueDrawer` and `AttributeDrawer` in
//! Voxear.Core), as the Inspector shows them in edit mode. A drawn field's
//! row starts with the Inspector's own control in a slot. Once the scripts
//! have started, its drawers are opened and their markup built in the slot,
//! each inside the "next" element of the one around it, with the
//! Inspector's own control in the innermost one's, or nowhere when a drawer
//! has none. A drawer built in code adds its `NextField` (an element with
//! the `field-drawer-next` class) as it first draws, so the drawers inside
//! it, and the control, wait for that draw and are placed after it
//! ([`FieldDrawers::place_waiting`]). They are drawn when they are built, when the value changes
//! elsewhere, as on undo, and when something in them is clicked or changed.
//! What they give back goes into the scene, as one step to undo with the
//! rest of the frame's edits. While the drawers draw, the scripts reach
//! their elements through [`DrawerUi`], by numbers of the Inspector's own.
//!
//! A behaviour's inspector (`BehaviourInspector`) is drawn the same way, as
//! a field with no path: its value is the table of fields the scene sets,
//! and what it gives back the table of those it changed.

use voxear_engine::{
    Content,
    game_ui::UiNumbers,
    script_types::{FieldKind, ScriptTypes, same_field},
};
use voxear_scripting::{DrawRequest, DrawerField, Drawn, FieldStep, ScriptServices, UiAnswer, UiAsk};
use voxear_ui::{ElementKind, LoadFile, NodeId, Style, Ui};

use crate::{fields::Step, props::Prop};

/// The first number the drawers' documents and elements are known by, far
/// from the game UI's, which start at 0.
const FIRST_NUMBER: u32 = 1 << 24;
/// How many numbers each Inspector's drawers have, so several never share one.
const NUMBERS_EACH: u32 = 1 << 20;

/// A field the Inspector draws with drawers.
struct DrawnRow {
    prop: Prop,
    /// Its place among the Inspector's rows, for its control; none for a
    /// whole behaviour.
    row: Option<usize>,
    slot: NodeId,
    drawers: Vec<String>,
    label: String,
    /// Whether what the drawers give back goes into the scene: not for a
    /// `[ReadOnly]` field, or one disabled.
    editable: bool,
    /// The document each drawer built became, outermost first, and the
    /// container of the one built last.
    documents: Vec<u32>,
    last: Option<NodeId>,
    /// The markup of each drawer, as the scripts gave it.
    markups: Vec<String>,
    /// How many drawers are built when the rest, and the field's own
    /// control, wait for the last one built to add a `NextField` as it
    /// draws; `None` once they have a place.
    waiting: Option<usize>,
    /// The outermost drawer's container, where clicks and changes are looked for.
    root: Option<NodeId>,
    /// The value it was last drawn with, or `None` to draw it again.
    drawn: Option<Option<toml::Value>>,
    /// Its drawers threw, which the Console has, so they are not drawn again
    /// until the Inspector is built again.
    failed: bool,
}

pub struct FieldDrawers {
    /// Which of several Inspectors these are, so the scripts keep each's
    /// drawers apart, and the numbers of each's elements too.
    owner: u32,
    rows: Vec<DrawnRow>,
    /// Whether the rows' drawers were opened since the Inspector was built.
    opened: bool,
    numbers: UiNumbers,
    /// The rows drawn by the last requests, in their order.
    asked: Vec<usize>,
}

/// A field whose drawers are to be opened, as the scripts are asked.
pub struct ToOpen {
    pub type_name: String,
    pub path: Vec<Step>,
    pub drawers: Vec<String>,
    pub label: String,
}

impl ToOpen {
    pub fn field(&self) -> DrawerField<'_> {
        let path = self
            .path
            .iter()
            .map(|step| match step {
                Step::Key(name) => FieldStep::Field(name),
                Step::Item(index) => FieldStep::Item(*index),
            })
            .collect();
        DrawerField { type_name: &self.type_name, path, drawers: &self.drawers, label: &self.label }
    }
}

impl FieldDrawers {
    pub fn new(owner: u32) -> Self {
        let numbers = UiNumbers::starting_at(FIRST_NUMBER + owner * NUMBERS_EACH);
        Self { owner, rows: Vec::new(), opened: false, numbers, asked: Vec::new() }
    }

    pub fn owner(&self) -> u32 {
        self.owner
    }

    /// Forgets every drawn field, as the Inspector is built again.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.numbers.clear();
        self.opened = false;
    }

    /// Notes that a field is drawn by `drawers`, on the Inspector's row
    /// `row`, whose control is in `slot` until they are built there, or that
    /// a whole behaviour is, with no row.
    pub fn add(
        &mut self,
        prop: &Prop,
        row: Option<usize>,
        slot: NodeId,
        drawers: &[String],
        label: &str,
        editable: bool,
    ) {
        self.rows.push(DrawnRow {
            prop: prop.clone(),
            row,
            slot,
            drawers: drawers.to_vec(),
            label: label.to_string(),
            editable,
            documents: Vec::new(),
            last: None,
            markups: Vec::new(),
            waiting: None,
            root: None,
            drawn: None,
            failed: false,
        });
    }

    /// Has every field drawn again next frame, as when what the drawers
    /// read changed, though the fields did not.
    pub fn redraw(&mut self) {
        self.rows.iter_mut().for_each(|row| row.drawn = None);
    }

    /// The fields whose drawers are to be opened, when they have not been
    /// since the Inspector was built.
    pub fn to_open(&self) -> Option<Vec<ToOpen>> {
        if self.opened || self.rows.is_empty() {
            return None;
        }
        let fields = self.rows.iter().filter_map(|row| {
            let Prop::Field { behaviour, path } = &row.prop else { return None };
            let (type_name, path) = (behaviour.clone(), path.clone());
            Some(ToOpen { type_name, path, drawers: row.drawers.clone(), label: row.label.clone() })
        });
        Some(fields.collect())
    }

    /// Builds each field's drawers from the markups the scripts gave, in its
    /// slot. Returns each field whose drawers were built, with its row, the
    /// element its own control goes into, if one does yet, and whether it is
    /// editable. A field whose drawers could not be opened or built keeps
    /// its own control.
    pub fn place(
        &mut self,
        ui: &mut Ui,
        answers: Vec<Result<Vec<String>, String>>,
        load: &mut LoadFile,
    ) -> Vec<(Prop, Option<usize>, Option<NodeId>, bool)> {
        self.opened = true;
        let mut placed = Vec::new();
        for (row, answer) in self.rows.iter_mut().zip(answers) {
            let Ok(markups) = answer else {
                row.failed = true;
                continue;
            };
            for child in ui.children(row.slot) {
                ui.remove(child);
            }
            row.markups = markups;
            let next = row.build(ui, &mut self.numbers, 0, row.slot, load);
            for warning in ui.take_warnings() {
                tracing::warn!(target: "script", "{warning}");
            }
            placed.push((row.prop.clone(), row.row, next, row.editable));
        }
        placed
    }

    /// Places what waited for a drawer drawn last frame to add its
    /// `NextField`: the drawers inside it, drawn from the next frame on, and
    /// the field's own control, returned as [`place`](Self::place) returns
    /// it. A drawer that adds none draws the field alone.
    pub fn place_waiting(&mut self, ui: &mut Ui, load: &mut LoadFile) -> Vec<(Prop, Option<usize>, Option<NodeId>, bool)> {
        let mut placed = Vec::new();
        for &index in &self.asked {
            let row = &mut self.rows[index];
            let (Some(from), Some(last)) = (row.waiting, row.last) else { continue };
            let Some(next) = next_of(ui, last).filter(|_| !row.failed) else { continue };
            ui.add_class(next, "field-drawer-next");
            let control = row.build(ui, &mut self.numbers, from, next, load);
            // Drawn again with the drawers just built.
            row.drawn = None;
            placed.push((row.prop.clone(), row.row, control, row.editable));
        }
        for warning in ui.take_warnings() {
            tracing::warn!(target: "script", "{warning}");
        }
        placed
    }

    /// The fields to draw this frame, each with its value now as `value`
    /// reads it: those just built, those whose value changed elsewhere, and
    /// those with something in them clicked or changed.
    pub fn requests(&mut self, ui: &Ui, value: impl Fn(&Prop) -> Option<toml::Value>) -> Vec<DrawRequest> {
        self.asked.clear();
        let mut requests = Vec::new();
        for (index, row) in self.rows.iter_mut().enumerate() {
            let Some(root) = row.root.filter(|_| !row.failed) else { continue };
            let now = value(&row.prop);
            if row.drawn.as_ref() == Some(&now) && !touched(ui, root) {
                continue;
            }
            self.asked.push(index);
            row.drawn = Some(now.clone());
            requests.push(DrawRequest { field: index, documents: row.documents.clone(), value: now });
        }
        requests
    }

    /// What the scripts reach the drawers' elements through while they draw.
    pub fn services<'a>(&'a mut self, ui: &'a mut Ui) -> DrawerUi<'a> {
        DrawerUi::new(ui, &mut self.numbers)
    }

    /// Takes what the drawers made of each field asked for. Returns each
    /// field they changed, with its value as a scene writes it, and the
    /// errors of those that threw.
    pub fn take(
        &mut self,
        answers: Vec<Result<Drawn, String>>,
        types: &ScriptTypes,
    ) -> (Vec<(Prop, toml::Value)>, Vec<String>) {
        let mut changed = Vec::new();
        let mut errors = Vec::new();
        for (&index, answer) in self.asked.iter().zip(answers) {
            let row = &mut self.rows[index];
            match answer {
                Ok(Drawn { value: Some(toml::Value::Table(fields)), changed: true }) if row.whole() => {
                    let Prop::Field { behaviour, .. } = &row.prop else { continue };
                    // Drawn again next frame, with the scene's fields as they are now.
                    row.drawn = None;
                    for (name, value) in fields {
                        let path = vec![Step::Key(name)];
                        let kind = crate::fields::kind_at(types, behaviour, &path);
                        let value = kind.map_or(value.clone(), |kind| typed(types, &kind, value));
                        changed.push((Prop::Field { behaviour: behaviour.clone(), path }, value));
                    }
                }
                Ok(Drawn { value: Some(value), changed: true }) if row.editable => {
                    let Prop::Field { behaviour, path } = &row.prop else { continue };
                    let kind = crate::fields::kind_at(types, behaviour, path);
                    let value = kind.map_or(value.clone(), |kind| typed(types, &kind, value));
                    // Drawn again next frame when inner drawers saw the value before the outer ones changed it.
                    row.drawn = (row.documents.len() < 2).then(|| Some(value.clone()));
                    changed.push((row.prop.clone(), value));
                }
                Ok(_) => {}
                Err(error) => {
                    row.failed = true;
                    let drawer = if row.whole() { "inspector" } else { "drawer" };
                    errors.push(format!("{}'s {drawer} threw: {error}", row.label));
                }
            }
        }
        (changed, errors)
    }
}

impl DrawnRow {
    /// Whether it is a whole behaviour, drawn by its inspector.
    fn whole(&self) -> bool {
        matches!(&self.prop, Prop::Field { path, .. } if path.is_empty())
    }

    /// Builds the drawers from `from` on, the first in `into` and each other
    /// in the next element of the one before. Returns where the field's own
    /// control goes: the last one's next, the slot when a markup could not
    /// be built, or nowhere yet when the drawer built last has no next, as
    /// one built in code until it draws (see `waiting`).
    fn build(
        &mut self,
        ui: &mut Ui,
        numbers: &mut UiNumbers,
        from: usize,
        into: NodeId,
        load: &mut LoadFile,
    ) -> Option<NodeId> {
        let mut parent = into;
        for index in from..self.markups.len().min(self.drawers.len()) {
            let drawer = &self.drawers[index];
            let built = match ui.load_markup(parent, drawer, &self.markups[index], load) {
                Ok(built) => built,
                Err(error) => {
                    // The field keeps its own control, as if it had no drawers.
                    tracing::error!("{drawer}'s markup: {error}");
                    for child in ui.children(self.slot) {
                        ui.remove(child);
                    }
                    (self.failed, self.root, self.last, self.waiting) = (true, None, None, None);
                    self.documents.clear();
                    return Some(self.slot);
                }
            };
            ui.set_style(built, Style::row());
            ui.add_class(built, "field-drawer");
            // A field's drawers sit beside its label; a behaviour's inspector stacks down its section.
            if self.whole() {
                ui.add_class(built, "behaviour-inspector");
            }
            self.root.get_or_insert(built);
            self.documents.push(numbers.add_document(built));
            self.last = Some(built);
            let Some(next) = next_of(ui, built) else {
                self.waiting = Some(index + 1);
                return None;
            };
            ui.add_class(next, "field-drawer-next");
            parent = next;
        }
        self.waiting = None;
        Some(parent)
    }
}

/// Where a drawer's next drawer, or the field's own control, goes: its
/// markup's element named "next", or the `NextField` its code added.
fn next_of(ui: &Ui, built: NodeId) -> Option<NodeId> {
    fn marked(ui: &Ui, node: NodeId) -> Option<NodeId> {
        if ui.has_class(node, "field-drawer-next") {
            return Some(node);
        }
        ui.children(node).into_iter().find_map(|child| marked(ui, child))
    }
    ui.find(built, "next").or_else(|| ui.children(built).into_iter().find_map(|child| marked(ui, child)))
}

/// The editor's UI as scripts' drawers, and windows written in C#, reach
/// it, by the numbers their documents and elements were given.
pub struct DrawerUi<'a> {
    ui: &'a mut Ui,
    numbers: &'a mut UiNumbers,
    /// The project's content, for the data assets the scripts load and list.
    content: Option<&'a mut Content>,
}

impl<'a> DrawerUi<'a> {
    pub fn new(ui: &'a mut Ui, numbers: &'a mut UiNumbers) -> Self {
        Self { ui, numbers, content: None }
    }

    /// With the project's content, so the scripts' `Definition.Load` and
    /// `Definition.All` reach its data assets.
    pub fn with_content(mut self, content: &'a mut Content) -> Self {
        self.content = Some(content);
        self
    }
}

impl ScriptServices for DrawerUi<'_> {
    fn ui_find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.numbers.find(self.ui, document, name)
    }

    fn ui_ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        self.numbers.ask(self.ui, element, ask)
    }

    fn ui_create(&mut self, parent: u32, kind: i32, text: &str) -> Option<u32> {
        self.numbers.create(self.ui, parent, ElementKind::from_number(kind)?, text)
    }

    fn ui_remove(&mut self, element: u32) {
        self.numbers.remove(self.ui, element);
    }

    fn asset_fields(&mut self, reference: &str) -> anyhow::Result<String> {
        let Some(content) = self.content.as_deref_mut() else {
            anyhow::bail!("data assets load only with the project's content, not here ({reference})")
        };
        content.data_asset_json(reference)
    }

    fn data_assets(&mut self) -> anyhow::Result<String> {
        let Some(content) = self.content.as_deref_mut() else {
            anyhow::bail!("data assets are listed only with the project's content, not here")
        };
        Ok(content.data_assets_json())
    }
}

/// Whether anything in `node` was clicked, or changed, as typed in or
/// moved, since the last frame.
fn touched(ui: &Ui, node: NodeId) -> bool {
    fn changed(ui: &Ui, node: NodeId) -> bool {
        ui.changed(node) || ui.children(node).into_iter().any(|child| changed(ui, child))
    }
    ui.clicked(node) || changed(ui, node)
}

/// A value from the scripts as a scene writes one of its kind: JSON writes
/// 10.0 as 10, but a float stays a float in the scene.
fn typed(types: &ScriptTypes, kind: &FieldKind, value: toml::Value) -> toml::Value {
    use toml::Value as V;
    match (kind, value) {
        (FieldKind::Number, V::Integer(i)) => V::Float(i as f64),
        (FieldKind::Vector { .. } | FieldKind::Color | FieldKind::Rotation, V::Array(items)) => {
            V::Array(items.into_iter().map(|item| typed(types, &FieldKind::Number, item)).collect())
        }
        (FieldKind::List { element }, V::Array(items)) => {
            V::Array(items.into_iter().map(|item| typed(types, element, item)).collect())
        }
        (FieldKind::Struct { name }, V::Table(table)) => {
            let fields = types.fields_of(name);
            let typed_field = |(key, value): (String, V)| {
                let field = fields.iter().find(|f| same_field(&f.name, &key));
                let value = field.map_or(value.clone(), |f| typed(types, &f.kind, value));
                (key, value)
            };
            V::Table(table.into_iter().map(typed_field).collect())
        }
        (_, value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_from_the_scripts_stay_floats_where_the_field_is() {
        let types = ScriptTypes::default();
        let vector = typed(&types, &FieldKind::Vector { size: 2 }, toml::Value::Array(vec![1.into(), 2.5.into()]));
        assert_eq!(vector, toml::Value::Array(vec![1.0.into(), 2.5.into()]));
        let whole = typed(&types, &FieldKind::Whole, 3.into());
        assert_eq!(whole, toml::Value::Integer(3));
    }
}
