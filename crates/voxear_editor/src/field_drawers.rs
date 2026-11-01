//! Fields the scripts' drawers draw (`ValueDrawer` and `AttributeDrawer` in
//! Voxear.Core), as the Inspector shows them in edit mode. A drawn field's
//! row starts with the Inspector's own control in a slot. Once the scripts
//! have started, its drawers are opened and their markup built in the slot,
//! each inside the "next" element of the one around it, with the
//! Inspector's own control in the innermost one's, or nowhere when a drawer
//! has none. They are drawn when they are built, when the value changes
//! elsewhere, as on undo, and when something in them is clicked or changed.
//! What they give back goes into the scene, as one step to undo with the
//! rest of the frame's edits. While the drawers draw, the scripts reach
//! their elements through [`DrawerUi`], by numbers of the Inspector's own.

use voxear_engine::{
    game_ui::UiNumbers,
    script_types::{FieldKind, ScriptTypes, same_field},
};
use voxear_scripting::{DrawRequest, DrawerField, Drawn, FieldStep, ScriptServices, UiAnswer, UiAsk};
use voxear_ui::{LoadFile, NodeId, Style, Ui};

use crate::{fields::Step, props::Prop};

/// The first number the drawers' documents and elements are known by, far
/// from the game UI's, which start at 0.
const FIRST_NUMBER: u32 = 1 << 24;

/// A field the Inspector draws with drawers.
struct DrawnRow {
    prop: Prop,
    /// Its place among the Inspector's rows, for its control.
    row: usize,
    slot: NodeId,
    drawers: Vec<String>,
    label: String,
    /// Whether what the drawers give back goes into the scene: not for a
    /// `[ReadOnly]` field, or one disabled.
    editable: bool,
    /// The document each drawer built became, outermost first.
    documents: Vec<u32>,
    /// The outermost drawer's container, where clicks and changes are looked for.
    root: Option<NodeId>,
    /// The value it was last drawn with, or `None` to draw it again.
    drawn: Option<Option<toml::Value>>,
    /// Its drawers threw, which the Console has, so they are not drawn again
    /// until the Inspector is built again.
    failed: bool,
}

pub struct FieldDrawers {
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

impl Default for FieldDrawers {
    fn default() -> Self {
        Self { rows: Vec::new(), opened: false, numbers: UiNumbers::starting_at(FIRST_NUMBER), asked: Vec::new() }
    }
}

impl FieldDrawers {
    /// Forgets every drawn field, as the Inspector is built again.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.numbers.clear();
        self.opened = false;
    }

    /// Notes that a field is drawn by `drawers`, on the Inspector's row
    /// `row`, whose control is in `slot` until they are built there.
    pub fn add(&mut self, prop: &Prop, row: usize, slot: NodeId, drawers: &[String], label: &str, editable: bool) {
        self.rows.push(DrawnRow {
            prop: prop.clone(),
            row,
            slot,
            drawers: drawers.to_vec(),
            label: label.to_string(),
            editable,
            documents: Vec::new(),
            root: None,
            drawn: None,
            failed: false,
        });
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
    /// slot. Returns each row whose drawers were built, with the element its
    /// own control goes into, if one does, and whether it is editable. A
    /// field whose drawers could not be opened or built keeps its own
    /// control.
    pub fn place(
        &mut self,
        ui: &mut Ui,
        answers: Vec<Result<Vec<String>, String>>,
        load: &mut LoadFile,
    ) -> Vec<(usize, Option<NodeId>, bool)> {
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
            let mut parent = Some(row.slot);
            for (markup, drawer) in markups.iter().zip(&row.drawers) {
                let Some(into) = parent else { break };
                let built = match ui.load_markup(into, drawer, markup, load) {
                    Ok(built) => built,
                    Err(error) => {
                        // The field keeps its own control, as if it had no drawers.
                        tracing::error!("{drawer}'s markup: {error}");
                        for child in ui.children(row.slot) {
                            ui.remove(child);
                        }
                        (row.failed, row.root, parent) = (true, None, Some(row.slot));
                        row.documents.clear();
                        break;
                    }
                };
                ui.set_style(built, Style::row());
                ui.add_class(built, "field-drawer");
                row.root.get_or_insert(built);
                row.documents.push(self.numbers.add_document(built));
                parent = ui.find(built, "next");
                if let Some(next) = parent {
                    ui.add_class(next, "field-drawer-next");
                }
            }
            for warning in ui.take_warnings() {
                tracing::warn!(target: "script", "{warning}");
            }
            placed.push((row.row, parent, row.editable));
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
        DrawerUi { ui, numbers: &mut self.numbers }
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
                    errors.push(format!("{}'s drawer threw: {error}", row.label));
                }
            }
        }
        (changed, errors)
    }
}

/// The Inspector's UI as the drawers' code reaches it, by the numbers the
/// drawers' documents and elements were given.
pub struct DrawerUi<'a> {
    ui: &'a mut Ui,
    numbers: &'a mut UiNumbers,
}

impl ScriptServices for DrawerUi<'_> {
    fn ui_find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.numbers.find(self.ui, document, name)
    }

    fn ui_ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        self.numbers.ask(self.ui, element, ask)
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
