//! Script fields in the Inspector: the values a scene sets on a behaviour,
//! each found by a path from its field (a list's item, a `[Serializable]`
//! table's field), typed by the scripts' description (`script_types`), and
//! read and written as the scene's TOML. A value the scene does not set
//! reads as the script starts it; writing one sets the whole field for the
//! scene, as it is now, with that value changed.

use std::sync::Arc;

use voxear_assets::AssetKind;
use voxear_core::math::{EulerRot, Quat};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours, Game,
    audio::AudioSource,
    physics::{CharacterController, Collider, Joint, Rigidbody, Vehicle, Wheel},
    physics2d::{Collider2D, Rigidbody2D},
    script_types::{FieldKind, ScriptTypes, nice_name, same_field},
};
use voxear_scene::{BehaviourSpec, Behaviours, Destroying, EditorOnly, Name};

use crate::props::{Value, specs};

/// One step into a field's value: a table's field, or a list's item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    Key(String),
    Item(usize),
}

/// What a reference is set to from its menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Obj(Option<Entity>),
    /// An asset by its project path.
    Asset(Option<String>),
    /// A key into the project's string tables.
    Key(Option<String>),
}

/// The kind of value at a path, as the scripts' description says.
pub fn kind_at(types: &ScriptTypes, behaviour: &str, path: &[Step]) -> Option<FieldKind> {
    let (Step::Key(first), rest) = path.split_first()? else { return None };
    let mut kind = types.behaviour(behaviour)?.field(first)?.kind.clone();
    for step in rest {
        kind = match (kind, step) {
            (FieldKind::List { element }, Step::Item(_)) => *element,
            (FieldKind::Struct { name }, Step::Key(key)) => {
                types.fields_of(&name).iter().find(|f| same_field(&f.name, key))?.kind.clone()
            }
            _ => return None,
        };
    }
    Some(kind)
}

/// The rows a field shows: its own, then each item of a list or each field
/// of a table, and theirs in turn.
pub fn expand(
    world: &World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: Vec<Step>,
    rows: &mut Vec<Vec<Step>>,
) {
    let kind = kind_at(types, behaviour, &path);
    rows.push(path.clone());
    match kind {
        Some(FieldKind::List { .. }) => {
            let items = value_at(world, types, entity, behaviour, &path).and_then(|v| v.as_array().map(Vec::len));
            for item in 0..items.unwrap_or_default() {
                expand(world, types, entity, behaviour, with(&path, Step::Item(item)), rows);
            }
        }
        Some(FieldKind::Struct { name }) => {
            for field in types.fields_of(&name) {
                expand(world, types, entity, behaviour, with(&path, Step::Key(field.name.clone())), rows);
            }
        }
        _ => {}
    }
}

fn with(path: &[Step], step: Step) -> Vec<Step> {
    let mut path = path.to_vec();
    path.push(step);
    path
}

/// What a reference points at: an Obj, or an asset by its project path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Obj(Entity),
    Asset(String),
}

/// What the reference at a path points at, if it is a reference set to
/// something that is there.
pub fn target(world: &World, types: &ScriptTypes, entity: Entity, behaviour: &str, path: &[Step]) -> Option<Target> {
    let value = value_at(world, types, entity, behaviour, path)?;
    match kind_at(types, behaviour, path)? {
        FieldKind::Obj | FieldKind::Behaviour { .. } | FieldKind::Component { .. } => {
            let bits = value.as_table()?.get("entity")?.as_integer().filter(|&bits| bits != 0)?;
            let target = Entity::from_bits(bits as u64);
            world.is_alive(target).then_some(Target::Obj(target))
        }
        FieldKind::Asset { .. } | FieldKind::Ref { .. } => {
            value.as_str().filter(|path| !path.is_empty()).map(|path| Target::Asset(path.to_string()))
        }
        _ => None,
    }
}

/// The fields the scene sets on an Obj's behaviour, as a table, for its
/// inspector; those left as the script starts them are not in it.
pub fn scene_fields(world: &World, entity: Entity, behaviour: &str) -> Option<toml::Value> {
    let spec = specs(world, entity).into_iter().find(|s| s.type_name == behaviour)?;
    Some(toml::Value::Table((*spec.fields).clone()))
}

/// The value at a path as it is now: what the scene set, or what the script
/// starts it as. `None` for nothing, as a reference starts.
pub fn value_at(
    world: &World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: &[Step],
) -> Option<toml::Value> {
    let (Step::Key(first), rest) = path.split_first()? else { return None };
    let spec = specs(world, entity).into_iter().find(|s| s.type_name == behaviour)?;
    let declared = types.behaviour(behaviour).and_then(|b| b.field(first));
    let set = spec.fields.iter().find(|(key, _)| same_field(key, first)).map(|(_, v)| v.clone());
    let top = set.or_else(|| declared.and_then(|f| f.default.clone()))?;
    descend(types, top, declared.map(|f| f.kind.clone()), rest)
}

fn descend(types: &ScriptTypes, value: toml::Value, kind: Option<FieldKind>, steps: &[Step]) -> Option<toml::Value> {
    let Some((step, rest)) = steps.split_first() else { return Some(value) };
    match step {
        Step::Item(index) => {
            let element = match kind {
                Some(FieldKind::List { element }) => Some(*element),
                _ => None,
            };
            descend(types, value.as_array()?.get(*index)?.clone(), element, rest)
        }
        Step::Key(key) => {
            let field = struct_field(types, kind.as_ref(), key);
            let set = value.as_table()?.iter().find(|(k, _)| same_field(k, key)).map(|(_, v)| v.clone());
            let found = set.or_else(|| field.and_then(|f| f.default.clone()))?;
            descend(types, found, field.map(|f| f.kind.clone()), rest)
        }
    }
}

fn struct_field<'a>(
    types: &'a ScriptTypes,
    kind: Option<&FieldKind>,
    key: &str,
) -> Option<&'a voxear_engine::script_types::ScriptField> {
    let Some(FieldKind::Struct { name }) = kind else { return None };
    types.fields_of(name).iter().find(|f| same_field(&f.name, key))
}

/// The value at a path as the Inspector shows it.
pub fn get(game: &Game, types: &ScriptTypes, entity: Entity, behaviour: &str, path: &[Step]) -> Value {
    let world = &game.engine.world;
    if let Some(live) = crate::live::get(world, types, entity, behaviour, path) {
        return live;
    }
    let value = value_at(world, types, entity, behaviour, path);
    match kind_at(types, behaviour, path) {
        Some(kind) => typed(world, &kind, value.as_ref()),
        None => untyped(world, value.as_ref()),
    }
}

/// A value shown as its field's kind says.
pub(crate) fn typed(world: &World, kind: &FieldKind, value: Option<&toml::Value>) -> Value {
    match kind {
        FieldKind::Number => Value::Number(value.and_then(number).unwrap_or_default() as f32),
        FieldKind::Whole => {
            Value::Whole(value.and_then(|v| v.as_integer().or_else(|| number(v).map(|f| f as i64))).unwrap_or_default())
        }
        FieldKind::Bool => Value::Toggle(value.and_then(toml::Value::as_bool).unwrap_or_default()),
        FieldKind::Text => Value::Text(value.and_then(toml::Value::as_str).unwrap_or_default().to_string()),
        FieldKind::Localized => Value::Asset(
            value.and_then(toml::Value::as_str).filter(|key| !key.is_empty()).unwrap_or("None").to_string(),
        ),
        FieldKind::Enum { names } => {
            let index = match value {
                Some(toml::Value::Integer(i)) => usize::try_from(*i).ok(),
                Some(v) => v.as_str().and_then(|name| names.iter().position(|n| same_field(n, name))),
                None => None,
            };
            Value::Choice(index.unwrap_or_default(), names.clone())
        }
        FieldKind::Vector { size } => {
            Value::Vector(floats(value).filter(|v| v.len() == *size).unwrap_or(vec![0.0; *size]))
        }
        FieldKind::Rotation => Value::Vector(euler(value)),
        FieldKind::Color => {
            let [r, g, b, _] = color(value);
            Value::Color([r, g, b].map(|c| (c * 1000.0).round() / 1000.0))
        }
        FieldKind::Obj | FieldKind::Behaviour { .. } | FieldKind::Component { .. } => {
            Value::Obj(reference(world, value))
        }
        FieldKind::Asset { .. } | FieldKind::Ref { .. } => {
            Value::Asset(value.and_then(toml::Value::as_str).filter(|p| !p.is_empty()).unwrap_or("None").to_string())
        }
        FieldKind::List { .. } => Value::List(value.and_then(toml::Value::as_array).map_or(0, Vec::len)),
        FieldKind::Struct { .. } => Value::Group,
        FieldKind::Unsupported { name } => Value::Fixed(value.map_or_else(|| name.clone(), |v| summary(world, v))),
    }
}

/// A value whose script was not found, shown as what the scene wrote.
fn untyped(world: &World, value: Option<&toml::Value>) -> Value {
    match value {
        Some(toml::Value::Float(f)) => Value::Number(*f as f32),
        Some(toml::Value::Integer(i)) => Value::Whole(*i),
        Some(toml::Value::Boolean(b)) => Value::Toggle(*b),
        Some(toml::Value::String(s)) => Value::Text(s.clone()),
        Some(v) if v.as_table().is_some_and(|t| t.contains_key("entity")) => Value::Obj(reference(world, Some(v))),
        Some(v) => match floats(Some(v)).filter(|f| (2..=4).contains(&f.len())) {
            Some(values) => Value::Vector(values),
            None => Value::Fixed(summary(world, v)),
        },
        None => Value::Fixed("None".into()),
    }
}

/// Sets the value at a path. Returns whether it changed.
pub fn set(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: &[Step],
    value: &Value,
) -> bool {
    if crate::live::is_live(world, types, entity, behaviour, path) {
        return crate::live::set(world, types, entity, behaviour, path, value);
    }
    let new = match (value, kind_at(types, behaviour, path)) {
        (Value::Choice(index, names), _) => toml::Value::String(names.get(*index).cloned().unwrap_or_default()),
        (Value::Number(n), _) => toml::Value::Float(f64::from(*n)),
        (Value::Whole(n), _) => toml::Value::Integer(*n),
        (Value::Toggle(b), _) => toml::Value::Boolean(*b),
        (Value::Text(text), _) => toml::Value::String(text.clone()),
        (Value::Vector(values), _) => {
            toml::Value::Array(values.iter().map(|&v| toml::Value::Float(f64::from(v))).collect())
        }
        // The Inspector picks red, green and blue; alpha stays as it was.
        (Value::Color([r, g, b]), Some(FieldKind::Color)) => {
            let [.., a] = color(value_at(world, types, entity, behaviour, path).as_ref());
            let tidy = |c: f32| toml::Value::Float((f64::from(c) * 10_000.0).round() / 10_000.0);
            toml::Value::Array([*r, *g, *b, a].map(tidy).to_vec())
        }
        _ => return false,
    };
    if value_at(world, types, entity, behaviour, path).as_ref() == Some(&new) {
        return false;
    }
    change(world, types, entity, behaviour, path, |slot| *slot = new)
}

/// Sets the value at a path to one as a scene writes it. Returns whether it
/// changed.
pub fn set_raw_at(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: &[Step],
    value: toml::Value,
) -> bool {
    if value_at(world, types, entity, behaviour, path).as_ref() == Some(&value) {
        return false;
    }
    change(world, types, entity, behaviour, path, |slot| *slot = value)
}

/// Sets a top-level field for the scene to a value as a scene writes it,
/// under the name the scene already uses for it, if any.
pub fn set_raw(world: &mut World, entity: Entity, behaviour: &str, name: &str, value: toml::Value) -> bool {
    let Some(spec) = spec_mut(world, entity, behaviour) else { return false };
    let fields = Arc::make_mut(&mut spec.fields);
    let key = fields.keys().find(|key| same_field(key, name)).cloned().unwrap_or_else(|| name.to_string());
    fields.insert(key, value);
    true
}

/// Points a reference at what its menu picked. A top-level asset set to
/// nothing goes back to what the script starts it as, which is nothing.
pub fn pick(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: &[Step],
    pick: &Pick,
) -> bool {
    let new = match pick {
        Pick::Obj(Some(target)) => toml::Value::Table(toml::Table::from_iter([(
            "entity".to_string(),
            toml::Value::Integer(target.to_bits() as i64),
        )])),
        Pick::Obj(None) => toml::Value::Table(toml::Table::new()),
        Pick::Asset(Some(asset)) | Pick::Key(Some(asset)) => toml::Value::String(asset.clone()),
        Pick::Asset(None) | Pick::Key(None) if path.len() == 1 => return reset(world, entity, behaviour, path),
        Pick::Asset(None) | Pick::Key(None) => toml::Value::String(String::new()),
    };
    change(world, types, entity, behaviour, path, |slot| *slot = new)
}

/// Adds an item to the end of a list, as a new one of its kind starts.
pub fn add_item(world: &mut World, types: &ScriptTypes, entity: Entity, behaviour: &str, list: &[Step]) -> bool {
    let Some(FieldKind::List { element }) = kind_at(types, behaviour, list) else { return false };
    let item = blank(types, &element);
    change(world, types, entity, behaviour, list, |slot| match slot.as_array_mut() {
        Some(items) => items.push(item),
        None => *slot = toml::Value::Array(vec![item]),
    })
}

/// Moves a list's item from one place to another.
pub fn move_item(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    list: &[Step],
    from: usize,
    to: usize,
) -> bool {
    change(world, types, entity, behaviour, list, |slot| {
        if let Some(items) = slot.as_array_mut().filter(|items| from < items.len() && to < items.len()) {
            let item = items.remove(from);
            items.insert(to, item);
        }
    })
}

/// Takes an item out of its list.
pub fn remove_item(world: &mut World, types: &ScriptTypes, entity: Entity, behaviour: &str, item: &[Step]) -> bool {
    let Some((Step::Item(index), list)) = item.split_last() else { return false };
    let index = *index;
    change(world, types, entity, behaviour, list, |slot| {
        if let Some(items) = slot.as_array_mut().filter(|items| index < items.len()) {
            items.remove(index);
        }
    })
}

/// Takes a field back to what the script starts it as: the scene no longer
/// sets it. Only whole fields can be, not values inside them.
pub fn reset(world: &mut World, entity: Entity, behaviour: &str, path: &[Step]) -> bool {
    let [Step::Key(name)] = path else { return false };
    let Some(spec) = spec_mut(world, entity, behaviour) else { return false };
    let Some(key) = spec.fields.keys().find(|key| same_field(key, name)).cloned() else { return false };
    Arc::make_mut(&mut spec.fields).remove(&key);
    true
}

/// Changes the value at a path in place, first setting the whole field for
/// the scene as it is now, if the scene does not set it yet.
fn change(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    behaviour: &str,
    path: &[Step],
    change: impl FnOnce(&mut toml::Value),
) -> bool {
    let Some((Step::Key(first), rest)) = path.split_first() else { return false };
    let now = value_at(world, types, entity, behaviour, &path[..1]);
    let kind = kind_at(types, behaviour, &path[..1]);
    let Some(spec) = spec_mut(world, entity, behaviour) else { return false };
    let fields = Arc::make_mut(&mut spec.fields);
    let key = fields.keys().find(|key| same_field(key, first)).cloned().unwrap_or_else(|| first.clone());
    let blank_top = || kind.as_ref().map_or_else(|| toml::Value::Table(toml::Table::new()), |k| blank(types, k));
    let top = fields.entry(key).or_insert_with(|| now.unwrap_or_else(blank_top));
    let Some(slot) = descend_mut(types, top, kind, rest) else { return false };
    change(slot);
    true
}

/// The values of a behaviour of an Obj, to change: the running one's while
/// the game plays, which the editor gives the running instance, or else the
/// scene's.
fn spec_mut<'w>(world: &'w mut World, entity: Entity, behaviour: &str) -> Option<&'w mut BehaviourSpec> {
    if world.has::<Behaviours>(entity) {
        let running = world.get_mut::<Behaviours>(entity)?;
        return running.0.iter_mut().find(|b| b.spec.type_name == behaviour).map(|b| &mut b.spec);
    }
    world.get_mut::<EditBehaviours>(entity)?.0.iter_mut().find(|s| s.type_name == behaviour)
}

/// [`descend`], for changing: a table's field the scene left out is set to
/// what the script starts it as on the way.
fn descend_mut<'a>(
    types: &ScriptTypes,
    value: &'a mut toml::Value,
    kind: Option<FieldKind>,
    steps: &[Step],
) -> Option<&'a mut toml::Value> {
    let Some((step, rest)) = steps.split_first() else { return Some(value) };
    match step {
        Step::Item(index) => {
            let element = match kind {
                Some(FieldKind::List { element }) => Some(*element),
                _ => None,
            };
            descend_mut(types, value.as_array_mut()?.get_mut(*index)?, element, rest)
        }
        Step::Key(key) => {
            let field = struct_field(types, kind.as_ref(), key).cloned();
            let table = value.as_table_mut()?;
            let name = table.keys().find(|k| same_field(k, key)).cloned().unwrap_or_else(|| key.clone());
            let starts = || {
                field.as_ref().map_or_else(
                    || toml::Value::Table(toml::Table::new()),
                    |f| f.default.clone().unwrap_or_else(|| blank(types, &f.kind)),
                )
            };
            let slot = table.entry(name).or_insert_with(starts);
            descend_mut(types, slot, field.map(|f| f.kind), rest)
        }
    }
}

/// A new value of a kind, as an item added to a list starts.
fn blank(types: &ScriptTypes, kind: &FieldKind) -> toml::Value {
    match kind {
        FieldKind::Number => toml::Value::Float(0.0),
        FieldKind::Whole => toml::Value::Integer(0),
        FieldKind::Bool => toml::Value::Boolean(false),
        FieldKind::Text | FieldKind::Localized | FieldKind::Asset { .. } | FieldKind::Ref { .. } => {
            toml::Value::String(String::new())
        }
        FieldKind::Enum { names } => toml::Value::String(names.first().cloned().unwrap_or_default()),
        FieldKind::Vector { size } => toml::Value::Array(vec![toml::Value::Float(0.0); *size]),
        FieldKind::Rotation => toml::Value::Array(vec![toml::Value::Float(0.0); 3]),
        FieldKind::Color => toml::Value::Array(vec![toml::Value::Float(1.0); 4]),
        FieldKind::List { .. } => toml::Value::Array(Vec::new()),
        FieldKind::Struct { name } => toml::Value::Table(
            types
                .fields_of(name)
                .iter()
                .map(|f| (f.name.clone(), f.default.clone().unwrap_or_else(|| blank(types, &f.kind))))
                .collect(),
        ),
        FieldKind::Obj | FieldKind::Behaviour { .. } | FieldKind::Component { .. } | FieldKind::Unsupported { .. } => {
            toml::Value::Table(toml::Table::new())
        }
    }
}

/// What an Obj, behaviour or component reference can point at: Objs in the
/// same scene as the Obj it is on, as a scene can only save those, and for
/// a behaviour or component, those that have one of its type. Nothing comes
/// first.
pub fn obj_choices(game: &Game, types: &ScriptTypes, entity: Entity, kind: &FieldKind) -> Vec<(String, Pick)> {
    let world = &game.engine.world;
    let scene = game.levels.scene_of(entity);
    let fits = |target: Entity| match kind {
        FieldKind::Behaviour { name } => specs(world, target)
            .iter()
            .any(|s| s.type_name == *name || types.behaviour(&s.type_name).is_some_and(|b| b.is_a(name))),
        FieldKind::Component { name } => has_component(world, target, name),
        _ => true,
    };
    let mut found: Vec<(String, Pick)> = world
        .query_ref::<(Entity, &Name)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(target, _)| game.levels.scene_of(*target) == scene && fits(*target))
        .map(|(target, name)| (name.0.clone(), Pick::Obj(Some(target))))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.insert(0, ("None".into(), Pick::Obj(None)));
    found
}

/// The project's assets an asset field can take, by path, with nothing
/// first.
pub fn asset_choices(game: &Game, kind: &FieldKind) -> Vec<(String, Pick)> {
    let (FieldKind::Asset { asset } | FieldKind::Ref { asset }) = kind else { return Vec::new() };
    let wanted = match asset.rsplit('.').next() {
        Some("Prefab") => Some(AssetKind::Prefab),
        Some("Material") => Some(AssetKind::Material),
        Some("Texture") => Some(AssetKind::Texture),
        Some("Model") => Some(AssetKind::Model),
        Some("Scene" | "SceneAsset") => Some(AssetKind::Scene),
        Some("Level" | "LevelAsset") => Some(AssetKind::Level),
        Some("Sound") => Some(AssetKind::Sound),
        Some("LightingSettings") => Some(AssetKind::Lighting),
        _ => None,
    };
    assets_of(game, wanted)
}

/// The project's assets of a kind, or of every kind, by path, with nothing
/// first.
pub fn assets_of(game: &Game, wanted: Option<AssetKind>) -> Vec<(String, Pick)> {
    let mut paths: Vec<String> = game
        .content
        .db()
        .entries()
        .filter(|e| wanted.is_none_or(|kind| e.kind == kind))
        .map(|e| e.path.clone())
        .collect();
    paths.sort();
    let mut found = vec![("None".to_string(), Pick::Asset(None))];
    found.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
    found
}

/// The keys of the project's string tables a `LocalizedString` field can
/// take, in order, with nothing first.
pub fn key_choices(game: &Game) -> Vec<(String, Pick)> {
    let localization = game.engine.world.resource::<voxear_engine::localization::Localization>();
    let mut keys: Vec<String> = localization.map(|l| l.keys().map(str::to_string).collect()).unwrap_or_default();
    keys.sort();
    let mut found = vec![("None".to_string(), Pick::Key(None))];
    found.extend(keys.into_iter().map(|key| (key.clone(), Pick::Key(Some(key)))));
    found
}

/// Why a reference cannot take what was dropped on it, for the status bar.
pub fn why_not(game: &Game, entity: Entity, kind: &FieldKind, pick: &Pick) -> String {
    let world = &game.engine.world;
    match (pick, kind) {
        (Pick::Obj(Some(target)), FieldKind::Asset { asset } | FieldKind::Ref { asset }) => {
            format!("{} is an Obj; the field takes a {}", name_of(world, *target), short(asset))
        }
        (Pick::Obj(Some(target)), _) if game.levels.scene_of(*target) != game.levels.scene_of(entity) => {
            format!("{} is in another scene, which this scene cannot point into", name_of(world, *target))
        }
        (Pick::Obj(Some(target)), FieldKind::Behaviour { name }) => {
            format!("{} has no {}", name_of(world, *target), short(name))
        }
        (Pick::Obj(Some(target)), FieldKind::Component { name }) => {
            format!("{} has no {}", name_of(world, *target), nice_name(short(name)))
        }
        (Pick::Asset(Some(path)), FieldKind::Asset { asset } | FieldKind::Ref { asset }) => {
            format!("{} is not a {}", path.rsplit('/').next().unwrap_or(path), short(asset))
        }
        (Pick::Asset(Some(path)), _) => {
            format!("{} is an asset; the field takes an Obj", path.rsplit('/').next().unwrap_or(path))
        }
        _ => "That does not fit the field".into(),
    }
}

/// What a reference field takes, by the short name of its type: "Prefab",
/// "Spinner", or "Obj".
pub fn takes(kind: &FieldKind) -> &str {
    match kind {
        FieldKind::Asset { asset } | FieldKind::Ref { asset } => short(asset),
        FieldKind::Behaviour { name } | FieldKind::Component { name } => short(name),
        FieldKind::Localized => "Text",
        _ => "Obj",
    }
}

/// Whether an Obj has the engine component a component field names by its
/// C# type, as `Voxear.AudioSource`.
fn has_component(world: &World, entity: Entity, name: &str) -> bool {
    match short(name) {
        "AudioSource" => world.has::<AudioSource>(entity),
        "Collider" => world.has::<Collider>(entity),
        "Rigidbody" => world.has::<Rigidbody>(entity),
        "Buoyancy" => world.has::<voxear_engine::water::Buoyancy>(entity),
        "Light" => world.has::<voxear_render::Light>(entity),
        "ParticleSystem" => world.has::<voxear_render::ParticleSystem>(entity),
        "Smoke" => world.has::<voxear_render::Smoke>(entity),
        "Mist" => world.has::<voxear_render::Mist>(entity),
        "CharacterController" => world.has::<CharacterController>(entity),
        "Joint" => world.has::<Joint>(entity),
        "Collider2D" => world.has::<Collider2D>(entity),
        "Rigidbody2D" => world.has::<Rigidbody2D>(entity),
        "Vehicle" => world.has::<Vehicle>(entity),
        "Wheel" => world.has::<Wheel>(entity),
        _ => false,
    }
}

fn name_of(world: &World, entity: Entity) -> String {
    world.get::<Name>(entity).map_or_else(|| "That Obj".into(), |n| n.0.clone())
}

fn short(name: &str) -> &str {
    name.rsplit(['.', '+']).next().unwrap_or(name)
}

fn number(value: &toml::Value) -> Option<f64> {
    value.as_float().or_else(|| value.as_integer().map(|i| i as f64))
}

fn floats(value: Option<&toml::Value>) -> Option<Vec<f32>> {
    let items = value?.as_array()?;
    let numbers: Vec<f32> = items.iter().filter_map(number).map(|f| f as f32).collect();
    (numbers.len() == items.len()).then_some(numbers)
}

/// A colour's red, green, blue and alpha, as scenes write one: `"#ff8800"`
/// or `"#ff880080"`, or three or four numbers. White when there is none.
fn color(value: Option<&toml::Value>) -> [f32; 4] {
    if let Some(hex) = value.and_then(toml::Value::as_str) {
        let digits = hex.trim_start_matches('#');
        let Ok(bits) = u32::from_str_radix(digits, 16) else { return [1.0; 4] };
        let bits = match digits.len() {
            6 => bits << 8 | 0xff,
            8 => bits,
            _ => return [1.0; 4],
        };
        return bits.to_be_bytes().map(|b| f32::from(b) / 255.0);
    }
    match floats(value).as_deref() {
        Some(&[r, g, b]) => [r, g, b, 1.0],
        Some(&[r, g, b, a]) => [r, g, b, a],
        _ => [1.0; 4],
    }
}

/// A rotation as Euler degrees, as scenes write one; four numbers are a
/// quaternion, turned into degrees in the same order.
fn euler(value: Option<&toml::Value>) -> Vec<f32> {
    match floats(value).as_deref() {
        Some([x, y, z]) => vec![*x, *y, *z],
        Some([x, y, z, w]) => {
            let (y, x, z) = Quat::from_xyzw(*x, *y, *z, *w).normalize().to_euler(EulerRot::YXZ);
            [x, y, z].iter().map(|a| a.to_degrees()).collect()
        }
        _ => vec![0.0; 3],
    }
}

/// A reference by the name of the Obj it points at.
fn reference(world: &World, value: Option<&toml::Value>) -> String {
    match value.and_then(|v| v.as_table()).and_then(|t| t.get("entity")).and_then(toml::Value::as_integer) {
        None | Some(0) => "None".into(),
        Some(bits) => world.get::<Name>(Entity::from_bits(bits as u64)).map_or("Missing".into(), |n| n.0.clone()),
    }
}

/// A value shown rather than edited, as the scene would write it.
fn summary(world: &World, value: &toml::Value) -> String {
    match value {
        toml::Value::Table(table) if table.contains_key("entity") || table.is_empty() => reference(world, Some(value)),
        toml::Value::Array(items) => format!("{} items", items.len()),
        other => other.to_string(),
    }
}
