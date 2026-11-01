//! Odin-style inspector attributes, as the Inspector applies them to script
//! fields. The attributes are C# (`Voxear.Core`'s `Inspector` folder), and
//! the scripts' description carries what each field asks for (see
//! `voxear_engine::script_types::FieldInspector`), so nothing here reads the
//! assembly.
//!
//! A row's own field gives its label, value controls and help; its top-level
//! field gives its group and order, so a list's items or a table's fields
//! stay with it. Conditions name another field of the same behaviour, or of
//! the same table, and read its value as the scene sets it, so they work
//! without the game running. When a condition changes, the Inspector builds
//! its sections again; [`key`] is what it compares.

use std::{collections::HashMap, sync::OnceLock};

use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    script_types::{
        FieldCondition, FieldGroup, FieldInspector, FieldKind, GroupKind, ListSettings, ScriptTypes, same_field,
    },
};
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    fields::{self, Step},
    props::{Part, Prop},
};

/// The attributes of the field a row shows, or none.
pub fn of<'t>(types: &'t ScriptTypes, prop: &Prop) -> &'t FieldInspector {
    let Prop::Field { behaviour, path } = prop else { return none() };
    field_at(types, behaviour, path).unwrap_or_else(|| none())
}

/// The attributes of the top-level field a row belongs to, which set its
/// group and order.
pub fn top<'t>(types: &'t ScriptTypes, prop: &Prop) -> &'t FieldInspector {
    let Prop::Field { behaviour, path } = prop else { return none() };
    field_at(types, behaviour, &path[..path.len().min(1)]).unwrap_or_else(|| none())
}

/// Whether a row shows: neither it nor its top-level field is hidden, and
/// their conditions hold.
pub fn shows(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> bool {
    let Prop::Field { behaviour, path } = prop else { return true };
    let mut levels = vec![path.as_slice()];
    if path.len() > 1 {
        levels.insert(0, &path[..1]);
    }
    levels.into_iter().all(|at| {
        let Some(meta) = field_at(types, behaviour, at) else { return true };
        let holds = |condition: &FieldCondition| holds(game, types, entity, (behaviour, at), condition);
        !meta.hidden && meta.show_if.as_ref().is_none_or(holds) && !meta.hide_if.as_ref().is_some_and(holds)
    })
}

/// Whether a row's value can be edited, as `[ReadOnly]`, `[EnableIf]` and
/// `[DisableIf]` say.
pub fn enabled(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> bool {
    let Prop::Field { behaviour, path } = prop else { return true };
    let meta = of(types, prop);
    let holds = |condition: &FieldCondition| holds(game, types, entity, (behaviour, path), condition);
    !meta.read_only && meta.enable_if.as_ref().is_none_or(holds) && !meta.disable_if.as_ref().is_some_and(holds)
}

/// Whether a `[Required]` field is empty.
pub fn missing(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> bool {
    let Prop::Field { behaviour, path } = prop else { return false };
    of(types, prop).required.is_some()
        && !truthy(fields::value_at(&game.engine.world, types, entity, behaviour, path).as_ref())
}

/// Whether an info box shows: always, or while the field it names is set.
pub fn info_shows(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, visible_if: Option<&str>) -> bool {
    let (Prop::Field { behaviour, path }, Some(member)) = (prop, visible_if) else { return true };
    let condition = FieldCondition { member: member.to_string(), value: None };
    holds(game, types, entity, (behaviour, path), &condition)
}

/// Rows in the order `[PropertyOrder]` puts their top-level fields, each
/// field's own rows (a list's items, a table's fields) kept after it.
pub fn ordered(types: &ScriptTypes, values: &[Prop]) -> Vec<Prop> {
    let mut blocks: Vec<(i32, Vec<Prop>)> = Vec::new();
    for value in values {
        let starts = matches!(value, Prop::Field { path, .. } if path.len() == 1);
        match blocks.last_mut() {
            Some((_, block)) if !starts => block.push(value.clone()),
            _ => blocks.push((top(types, value).order, vec![value.clone()])),
        }
    }
    // A stable sort keeps fields of the same order as they were declared.
    blocks.sort_by_key(|(order, _)| *order);
    blocks.into_iter().flat_map(|(_, block)| block).collect()
}

/// What decides how an Obj's script fields show: each row's conditions,
/// whether it is missing a required value, and which info boxes show. The
/// Inspector builds its sections again when this changes.
pub fn key(game: &Game, types: &ScriptTypes, entity: Entity, layout: &[(Part, Vec<Prop>)]) -> Vec<bool> {
    let mut key = Vec::new();
    for value in layout.iter().flat_map(|(_, values)| values) {
        if !matches!(value, Prop::Field { .. }) {
            continue;
        }
        key.extend([shows(game, types, entity, value), enabled(game, types, entity, value)]);
        key.push(missing(game, types, entity, value));
        let infos = &of(types, value).info;
        key.extend(infos.iter().map(|info| info_shows(game, types, entity, value, info.visible_if.as_deref())));
    }
    key
}

/// The containers groups put rows in, made as each group is first used:
/// foldouts, boxes, rows side by side, and tabs.
pub struct Groups {
    rows: NodeId,
    made: HashMap<String, NodeId>,
    /// Each group of tabs made, by its name, for keeping the tab picked.
    pub tabs: Vec<(String, NodeId)>,
    /// Each foldout made, by its name, for keeping it open or closed.
    pub foldouts: Vec<(String, NodeId)>,
}

impl Groups {
    pub fn new(rows: NodeId) -> Self {
        Self { rows, made: HashMap::new(), tabs: Vec::new(), foldouts: Vec::new() }
    }

    /// Where a row of `group` goes; the section's rows for none. A name such
    /// as "Movement/Air" is a group inside another, of the same kind.
    pub fn container(&mut self, ui: &mut Ui, group: Option<&FieldGroup>) -> NodeId {
        let Some(group) = group else { return self.rows };
        let mut parent = self.rows;
        let mut path = String::new();
        for part in group.name.split('/').filter(|p| !p.is_empty()) {
            path = if path.is_empty() { part.to_string() } else { format!("{path}/{part}") };
            parent = self.group(ui, parent, &path, part, group.kind);
        }
        match (&group.kind, &group.tab) {
            (GroupKind::Tab, Some(tab)) => self.tab_page(ui, parent, &path, tab),
            _ => parent,
        }
    }

    fn group(&mut self, ui: &mut Ui, parent: NodeId, path: &str, title: &str, kind: GroupKind) -> NodeId {
        if let Some(&made) = self.made.get(path) {
            return made;
        }
        let made = match kind {
            GroupKind::Foldout => {
                let foldout = ui.foldout(parent, title, true);
                ui.add_class(foldout, "attr-foldout");
                self.foldouts.push((path.to_string(), foldout));
                ui.foldout_content(foldout).expect("a foldout has content")
            }
            GroupKind::Box => {
                let frame = ui.panel(parent, Style::column());
                ui.add_class(frame, "attr-box");
                let heading = ui.add_label(frame, title);
                ui.add_class(heading, "attr-box-title");
                frame
            }
            GroupKind::Horizontal => {
                let row = ui.panel(parent, Style::row());
                ui.add_class(row, "attr-horizontal");
                row
            }
            GroupKind::Tab => {
                let tabs = ui.tabs(parent);
                ui.add_class(tabs, "attr-tabs");
                self.tabs.push((path.to_string(), tabs));
                tabs
            }
        };
        self.made.insert(path.to_string(), made);
        made
    }

    fn tab_page(&mut self, ui: &mut Ui, tabs: NodeId, path: &str, tab: &str) -> NodeId {
        let key = format!("{path}#{tab}");
        if let Some(&page) = self.made.get(&key) {
            return page;
        }
        let page = ui.add_tab(tabs, tab);
        ui.add_class(page, "attr-tab-page");
        self.made.insert(key, page);
        page
    }
}

/// Whether a condition holds: the field it names, beside the one at `path`,
/// equals its value, or with none, is true, non-zero, not empty or set.
fn holds(
    game: &Game,
    types: &ScriptTypes,
    entity: Entity,
    (behaviour, path): (&str, &[Step]),
    condition: &FieldCondition,
) -> bool {
    let mut sibling = path.to_vec();
    sibling.pop();
    sibling.push(Step::Key(condition.member.clone()));
    let value = fields::value_at(&game.engine.world, types, entity, behaviour, &sibling);
    match &condition.value {
        Some(wanted) => equal(value.as_ref(), wanted),
        None => truthy(value.as_ref()),
    }
}

fn equal(value: Option<&toml::Value>, wanted: &toml::Value) -> bool {
    match (value, wanted) {
        (Some(toml::Value::Integer(a)), toml::Value::Float(b))
        | (Some(toml::Value::Float(b)), toml::Value::Integer(a)) => (*a as f64 - b).abs() < 1e-9,
        (Some(value), wanted) => value == wanted,
        (None, _) => false,
    }
}

/// True, non-zero, not empty, or a reference that is set.
fn truthy(value: Option<&toml::Value>) -> bool {
    match value {
        None => false,
        Some(toml::Value::Boolean(on)) => *on,
        Some(toml::Value::Integer(n)) => *n != 0,
        Some(toml::Value::Float(n)) => *n != 0.0,
        Some(toml::Value::String(text)) => !text.is_empty(),
        Some(toml::Value::Array(items)) => !items.is_empty(),
        Some(toml::Value::Table(table)) => !table.is_empty(),
        Some(toml::Value::Datetime(_)) => true,
    }
}

/// The description of the field at `path`, when the path ends at a field (a
/// behaviour's, or a table's) rather than a list's item.
/// Whether a row is drawn by its list rather than on its own: an item of a
/// `[TableList]` of `[Serializable]` values, or what is in one, which the
/// table shows, or an item on a `[ListDrawerSettings]` page not shown.
pub fn drawn_by_list(types: &ScriptTypes, prop: &Prop, pages: &HashMap<String, usize>) -> bool {
    let Prop::Field { behaviour, path } = prop else { return false };
    path.iter().enumerate().any(|(at, step)| {
        let Step::Item(index) = step else { return false };
        let list = &path[..at];
        let Some(meta) = field_at(types, behaviour, list) else { return false };
        let per_page = meta.list.map_or(0, |settings| settings.per_page);
        is_table(types, behaviour, list) || (per_page > 0 && index / per_page != page(pages, behaviour, list))
    })
}

/// Whether a list is shown as a table: `[TableList]` on a list of
/// `[Serializable]` values.
pub fn is_table(types: &ScriptTypes, behaviour: &str, list: &[Step]) -> bool {
    let table = field_at(types, behaviour, list).is_some_and(|meta| meta.table);
    let element = match fields::kind_at(types, behaviour, list) {
        Some(FieldKind::List { element }) => Some(*element),
        _ => None,
    };
    table && matches!(element, Some(FieldKind::Struct { .. }))
}

/// The page of a list showing, by the list's place.
pub fn page(pages: &HashMap<String, usize>, behaviour: &str, list: &[Step]) -> usize {
    pages.get(&page_key(behaviour, list)).copied().unwrap_or_default()
}

pub fn page_key(behaviour: &str, list: &[Step]) -> String {
    format!("{behaviour}/{list:?}")
}

/// The drawers scripts draw a row's value with, outermost first: its
/// field's, or for a list's item, those the list gives its items.
pub fn drawers<'t>(types: &'t ScriptTypes, prop: &Prop) -> &'t [String] {
    let Prop::Field { behaviour, path } = prop else { return &[] };
    match path.split_last() {
        Some((Step::Item(_), list)) => field_at(types, behaviour, list).map_or(&[], |meta| &meta.item_drawers),
        _ => &of(types, prop).drawers,
    }
}

/// Whether a row is inside a value drawers draw, which they draw whole, as
/// a table's fields.
pub fn drawn_by_drawer(types: &ScriptTypes, prop: &Prop) -> bool {
    let Prop::Field { behaviour, path } = prop else { return false };
    (1..path.len()).any(|at| {
        let outer = Prop::Field { behaviour: behaviour.clone(), path: path[..at].to_vec() };
        !drawers(types, &outer).is_empty()
    })
}

/// When a row is a list's item, the list's `[ListDrawerSettings]`, if any,
/// and the item's index.
pub fn item_of(types: &ScriptTypes, prop: &Prop) -> Option<(Option<ListSettings>, usize)> {
    let Prop::Field { behaviour, path } = prop else { return None };
    let (Step::Item(index), list) = path.split_last()? else { return None };
    Some((field_at(types, behaviour, list).and_then(|meta| meta.list), *index))
}

fn field_at<'t>(types: &'t ScriptTypes, behaviour: &str, path: &[Step]) -> Option<&'t FieldInspector> {
    let (Step::Key(first), rest) = path.split_first()? else { return None };
    let mut field = types.behaviour(behaviour)?.field(first)?;
    let mut kind = field.kind.clone();
    for (at, step) in rest.iter().enumerate() {
        match (kind, step) {
            // A list's item is no field of its own.
            (FieldKind::List { .. }, Step::Item(_)) if at + 1 == rest.len() => return None,
            (FieldKind::List { element }, Step::Item(_)) => kind = *element,
            (FieldKind::Struct { name }, Step::Key(key)) => {
                field = types.fields_of(&name).iter().find(|f| same_field(&f.name, key))?;
                kind = field.kind.clone();
            }
            _ => return None,
        }
    }
    Some(&field.inspector)
}

fn none() -> &'static FieldInspector {
    static NONE: OnceLock<FieldInspector> = OnceLock::new();
    NONE.get_or_init(FieldInspector::default)
}
