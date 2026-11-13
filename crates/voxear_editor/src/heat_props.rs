//! Heat's components in the Inspector: the level's Climate and its clock,
//! a Heat Source, a Heat Zone and a Heat Body with its list of changes
//! (see `voxear_engine::heat`).
//!
//! A Heat Body's changes are a list whose length is a number, as Unity's
//! arrays show theirs: each change's name, when it counts, how hot or cold,
//! for how long, and the prefab it becomes. While the game plays, a Heat
//! Body also shows its temperature and how it feels.

use voxear_assets::AssetKind;
use voxear_ecs::{Component, Entity, World};
use voxear_engine::{
    Game,
    heat::{ChangeWhen, Climate, HeatBody, HeatSource, HeatState, HeatZone, ZoneMode, ZoneShape},
};

use crate::{
    fields::{self, Pick},
    props::{Part, Prop, Value},
};

/// The most changes a Heat Body's list is given from the Inspector.
const MOST_CHANGES: usize = 32;

/// A value one of heat's components shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeatField {
    TimeOfDay,
    DayLength,
    TurnsSun,
    Celsius,
    DaySwing,
    Humidity,
    SourceCelsius,
    SourceRadius,
    ZoneShape,
    ZoneSize,
    ZoneMode,
    ZoneCelsius,
    ZoneEdge,
    WarmsIn,
    Comfort,
    ColdResistance,
    HeatResistance,
    Changes,
    ChangeName(usize),
    ChangeWhen(usize),
    ChangeCelsius(usize),
    ChangeSeconds(usize),
    ChangeBecomes(usize),
    /// While playing: the body's temperature now, and how it feels it.
    Temperature,
    Feels,
}

use HeatField as F;

/// The component a value is of.
pub fn part(field: HeatField) -> Part {
    match field {
        F::TimeOfDay | F::DayLength | F::TurnsSun | F::Celsius | F::DaySwing | F::Humidity => Part::Climate,
        F::SourceCelsius | F::SourceRadius => Part::HeatSource,
        F::ZoneShape | F::ZoneSize | F::ZoneMode | F::ZoneCelsius | F::ZoneEdge => Part::HeatZone,
        _ => Part::HeatBody,
    }
}

/// The values a component shows, in order.
pub fn props(world: &World, entity: Entity, part: &Part) -> Vec<Prop> {
    let fields = match part {
        Part::Climate => vec![F::TimeOfDay, F::DayLength, F::TurnsSun, F::Celsius, F::DaySwing, F::Humidity],
        Part::HeatSource => vec![F::SourceCelsius, F::SourceRadius],
        Part::HeatZone => vec![F::ZoneShape, F::ZoneSize, F::ZoneMode, F::ZoneCelsius, F::ZoneEdge],
        Part::HeatBody => {
            let mut fields = Vec::new();
            if world.has::<HeatState>(entity) {
                fields.extend([F::Temperature, F::Feels]);
            }
            fields.extend([F::WarmsIn, F::Comfort, F::ColdResistance, F::HeatResistance, F::Changes]);
            let count = world.get::<HeatBody>(entity).map_or(0, |body| body.changes.len());
            for at in 0..count {
                fields.extend([F::ChangeName(at), F::ChangeWhen(at), F::ChangeCelsius(at), F::ChangeSeconds(at)]);
                fields.push(F::ChangeBecomes(at));
            }
            fields
        }
        _ => Vec::new(),
    };
    fields.into_iter().map(Prop::Heat).collect()
}

/// What a value's row says.
pub fn label(field: HeatField) -> String {
    match field {
        F::TimeOfDay => "Time of Day".into(),
        F::DayLength => "Day Length".into(),
        F::TurnsSun => "Turns Sun".into(),
        F::Celsius | F::SourceCelsius | F::ZoneCelsius | F::ChangeCelsius(_) | F::Temperature => "Temperature".into(),
        F::DaySwing => "Day Swing".into(),
        F::Humidity => "Humidity".into(),
        F::SourceRadius => "Radius".into(),
        F::ZoneShape => "Shape".into(),
        F::ZoneSize => "Size".into(),
        F::ZoneMode => "Mode".into(),
        F::ZoneEdge => "Edge".into(),
        F::WarmsIn => "Warms In".into(),
        F::Comfort => "Comfort".into(),
        F::ColdResistance => "Cold Resistance".into(),
        F::HeatResistance => "Heat Resistance".into(),
        F::Changes => "Changes".into(),
        F::ChangeName(at) => format!("Change {}", at + 1),
        F::ChangeWhen(_) => "When".into(),
        F::ChangeSeconds(_) => "For".into(),
        F::ChangeBecomes(_) => "Becomes".into(),
        F::Feels => "Feels".into(),
    }
}

/// The value as a prefab's locks name it.
pub fn key(field: HeatField) -> &'static str {
    match field {
        F::TimeOfDay => "time_of_day",
        F::DayLength => "day_length",
        F::TurnsSun => "turns_sun",
        F::Celsius | F::SourceCelsius | F::ZoneCelsius => "celsius",
        F::DaySwing => "day_swing",
        F::Humidity => "humidity",
        F::SourceRadius => "radius",
        F::ZoneShape => "shape",
        F::ZoneSize => "size",
        F::ZoneMode => "mode",
        F::ZoneEdge => "edge",
        F::WarmsIn => "warms_in",
        F::Comfort => "comfort",
        F::ColdResistance => "cold_resistance",
        F::HeatResistance => "heat_resistance",
        F::Changes
        | F::ChangeName(_)
        | F::ChangeWhen(_)
        | F::ChangeCelsius(_)
        | F::ChangeSeconds(_)
        | F::ChangeBecomes(_) => "changes",
        F::Temperature | F::Feels => "",
    }
}

pub fn unit(field: HeatField) -> Option<&'static str> {
    match field {
        F::TimeOfDay => Some("h"),
        F::DayLength => Some("min"),
        F::Celsius
        | F::DaySwing
        | F::SourceCelsius
        | F::ZoneCelsius
        | F::Comfort
        | F::ColdResistance
        | F::HeatResistance
        | F::ChangeCelsius(_)
        | F::Temperature => Some("°C"),
        F::SourceRadius | F::ZoneSize | F::ZoneEdge => Some("m"),
        F::WarmsIn | F::ChangeSeconds(_) => Some("s"),
        _ => None,
    }
}

/// The range a number is dragged within.
pub fn range(field: HeatField) -> Option<[f32; 2]> {
    match field {
        F::TimeOfDay => Some([0.0, 24.0]),
        F::Humidity => Some([0.0, 1.0]),
        _ => None,
    }
}

fn choice<T: PartialEq + Copy>(all: &[T], one: T, name: fn(T) -> &'static str) -> Value {
    let at = all.iter().position(|other| *other == one).unwrap_or_default();
    Value::Choice(at, all.iter().map(|each| name(*each).to_string()).collect())
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Whole(n) => Some(*n as f32),
        _ => None,
    }
}

fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// An asset's path, by the reference held, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

pub fn get(game: &Game, entity: Entity, field: HeatField) -> Option<Value> {
    let world = &game.engine.world;
    let climate = || world.get::<Climate>(entity);
    let source = || world.get::<HeatSource>(entity);
    let zone = || world.get::<HeatZone>(entity);
    let body = || world.get::<HeatBody>(entity);
    let change = |at: usize| body().and_then(|body| body.changes.get(at));
    Some(match field {
        F::TimeOfDay => Value::Number(round(climate()?.time_of_day)),
        F::DayLength => Value::Number(climate()?.day_length),
        F::TurnsSun => Value::Toggle(climate()?.turns_sun),
        F::Celsius => Value::Number(climate()?.celsius),
        F::DaySwing => Value::Number(climate()?.day_swing),
        F::Humidity => Value::Number(climate()?.humidity),
        F::SourceCelsius => Value::Number(source()?.celsius),
        F::SourceRadius => Value::Number(source()?.radius),
        F::ZoneShape => choice(&ZoneShape::ALL, zone()?.shape, ZoneShape::name),
        F::ZoneSize => match zone()?.shape {
            ZoneShape::Box => Value::Vector(zone()?.size.to_vec()),
            ZoneShape::Sphere => Value::Number(zone()?.size[0]),
        },
        F::ZoneMode => choice(&ZoneMode::ALL, zone()?.mode, ZoneMode::name),
        F::ZoneCelsius => Value::Number(zone()?.celsius),
        F::ZoneEdge => Value::Number(zone()?.edge),
        F::WarmsIn => Value::Number(body()?.warms_in),
        F::Comfort => Value::Vector(body()?.comfort.to_vec()),
        F::ColdResistance => Value::Number(body()?.cold_resistance),
        F::HeatResistance => Value::Number(body()?.heat_resistance),
        F::Changes => Value::Whole(body()?.changes.len() as i64),
        F::ChangeName(at) => Value::Text(change(at)?.name.clone()),
        F::ChangeWhen(at) => choice(&ChangeWhen::ALL, change(at)?.when, ChangeWhen::name),
        F::ChangeCelsius(at) => Value::Number(change(at)?.celsius),
        F::ChangeSeconds(at) => Value::Number(change(at)?.seconds),
        F::ChangeBecomes(at) => Value::Asset(path_of(game, change(at)?.becomes.as_deref())),
        F::Temperature => Value::Fixed(format!("{:.1} °C", world.get::<HeatState>(entity)?.temperature)),
        F::Feels => Value::Fixed(world.get::<HeatState>(entity)?.feels.name().into()),
    })
}

/// Changes `entity`'s `T` by `how`: whether it had one.
fn with<T: Component>(world: &mut World, entity: Entity, how: impl FnOnce(&mut T)) -> bool {
    match world.get_mut::<T>(entity) {
        Some(component) => {
            how(component);
            true
        }
        None => false,
    }
}

pub fn set(game: &mut Game, entity: Entity, field: HeatField, value: Value) -> bool {
    let world = &mut game.engine.world;
    let n = number(&value);
    let at_choice = |value: &Value| match value {
        Value::Choice(at, _) => Some(*at),
        _ => None,
    };
    match part(field) {
        Part::Climate => with(world, entity, |climate: &mut Climate| match (field, &value) {
            (F::TimeOfDay, _) => climate.time_of_day = n.unwrap_or(climate.time_of_day).rem_euclid(24.0),
            (F::DayLength, _) => climate.day_length = n.unwrap_or(climate.day_length).max(0.0),
            (F::TurnsSun, Value::Toggle(on)) => climate.turns_sun = *on,
            (F::Celsius, _) => climate.celsius = n.unwrap_or(climate.celsius).clamp(-273.0, 5000.0),
            (F::DaySwing, _) => climate.day_swing = n.unwrap_or(climate.day_swing).max(0.0),
            (F::Humidity, _) => climate.humidity = n.unwrap_or(climate.humidity).clamp(0.0, 1.0),
            _ => {}
        }),
        Part::HeatSource => with(world, entity, |source: &mut HeatSource| match field {
            F::SourceCelsius => source.celsius = n.unwrap_or(source.celsius).clamp(-273.0, 5000.0),
            F::SourceRadius => source.radius = n.unwrap_or(source.radius).max(0.0),
            _ => {}
        }),
        Part::HeatZone => with(world, entity, |zone: &mut HeatZone| match (field, &value) {
            (F::ZoneShape, value) => zone.shape = ZoneShape::ALL[at_choice(value).unwrap_or(0).min(1)],
            (F::ZoneMode, value) => zone.mode = ZoneMode::ALL[at_choice(value).unwrap_or(0).min(1)],
            (F::ZoneSize, Value::Vector(size)) if size.len() == 3 => {
                zone.size = [size[0], size[1], size[2]].map(|s| s.max(0.0))
            }
            (F::ZoneSize, value) => {
                if let Some(width) = number(value) {
                    zone.size = [width.max(0.0); 3];
                }
            }
            (F::ZoneCelsius, _) => zone.celsius = n.unwrap_or(zone.celsius).clamp(-273.0, 5000.0),
            (F::ZoneEdge, _) => zone.edge = n.unwrap_or(zone.edge).max(0.0),
            _ => {}
        }),
        _ => with(world, entity, |body: &mut HeatBody| set_body(body, field, &value)),
    }
}

fn set_body(body: &mut HeatBody, field: HeatField, value: &Value) {
    let n = number(value);
    match (field, value) {
        (F::WarmsIn, _) => body.warms_in = n.unwrap_or(body.warms_in).max(0.0),
        (F::Comfort, Value::Vector(pair)) if pair.len() >= 2 => {
            body.comfort = [pair[0].min(pair[1]), pair[0].max(pair[1])]
        }
        (F::ColdResistance, _) => body.cold_resistance = n.unwrap_or(body.cold_resistance).max(0.0),
        (F::HeatResistance, _) => body.heat_resistance = n.unwrap_or(body.heat_resistance).max(0.0),
        (F::Changes, _) => {
            let count = n.unwrap_or(body.changes.len() as f32).max(0.0) as usize;
            // A new change starts as the last one does, as Unity's arrays grow.
            let last = body.changes.last().cloned().unwrap_or_default();
            body.changes.resize(count.min(MOST_CHANGES), last);
        }
        _ => {
            let Some(change) = body.changes.get_mut(index(field)) else { return };
            match (field, value) {
                (F::ChangeName(_), Value::Text(name)) => change.name.clone_from(name),
                (F::ChangeWhen(_), Value::Choice(at, _)) => change.when = ChangeWhen::ALL[(*at).min(1)],
                (F::ChangeCelsius(_), _) => change.celsius = n.unwrap_or(change.celsius).clamp(-273.0, 5000.0),
                (F::ChangeSeconds(_), _) => change.seconds = n.unwrap_or(change.seconds).max(0.0),
                _ => {}
            }
        }
    }
}

/// Which change a change's value is of.
fn index(field: HeatField) -> usize {
    match field {
        F::ChangeName(at) | F::ChangeWhen(at) | F::ChangeCelsius(at) | F::ChangeSeconds(at) | F::ChangeBecomes(at) => {
            at
        }
        _ => usize::MAX,
    }
}

pub fn is_picked(field: HeatField) -> bool {
    matches!(field, F::ChangeBecomes(_))
}

pub fn takes_name(_field: HeatField) -> &'static str {
    "Prefab"
}

pub fn choices(game: &Game, field: HeatField) -> Vec<(String, Pick)> {
    if !is_picked(field) {
        return Vec::new();
    }
    let mut found = vec![("None".to_string(), Pick::Asset(None))];
    found.extend(fields::assets_of(game, Some(AssetKind::Prefab)));
    found
}

/// Takes the prefab picked for what a change becomes, or none.
pub fn pick(game: &mut Game, entity: Entity, field: HeatField, choice: &Pick) -> bool {
    let (F::ChangeBecomes(at), Pick::Asset(path)) = (field, choice) else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference.to_string()),
            Err(_) => return false,
        },
        None => None,
    };
    with(&mut game.engine.world, entity, |body: &mut HeatBody| {
        if let Some(change) = body.changes.get_mut(at) {
            change.becomes = reference;
        }
    })
}

/// Gives `entity` the component `part` is, as Add Component does.
pub fn add(world: &mut World, entity: Entity, part: &Part) {
    match part {
        Part::Climate => drop(world.insert(entity, Climate::default())),
        Part::HeatSource => drop(world.insert(entity, HeatSource::default())),
        Part::HeatZone => drop(world.insert(entity, HeatZone::default())),
        Part::HeatBody => drop(world.insert(entity, HeatBody::default())),
        _ => {}
    }
}

/// Takes the component `part` is off `entity`.
pub fn remove(world: &mut World, entity: Entity, part: &Part) {
    match part {
        Part::Climate => drop(world.remove::<Climate>(entity)),
        Part::HeatSource => drop(world.remove::<HeatSource>(entity)),
        Part::HeatZone => drop(world.remove::<HeatZone>(entity)),
        Part::HeatBody => {
            world.remove::<HeatBody>(entity);
            world.remove::<HeatState>(entity);
        }
        _ => {}
    }
}

/// The heat components `entity` has, in the order the Inspector shows them.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<Climate>(entity) {
        parts.push(Part::Climate);
    }
    if world.has::<HeatSource>(entity) {
        parts.push(Part::HeatSource);
    }
    if world.has::<HeatZone>(entity) {
        parts.push(Part::HeatZone);
    }
    if world.has::<HeatBody>(entity) {
        parts.push(Part::HeatBody);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_engine::heat::HeatChange;

    #[test]
    fn a_heat_bodys_changes_grow_as_the_last_and_take_their_values() {
        let mut body = HeatBody::default();
        set_body(&mut body, F::Changes, &Value::Whole(1));
        assert_eq!(body.changes, vec![HeatChange::default()]);
        set_body(&mut body, F::ChangeName(0), &Value::Text("Seared".into()));
        set_body(&mut body, F::ChangeSeconds(0), &Value::Number(-3.0));
        set_body(&mut body, F::Changes, &Value::Whole(2));
        assert_eq!(body.changes[1].name, "Seared", "a new one starts as the last");
        set_body(&mut body, F::ChangeWhen(1), &Value::Choice(1, Vec::new()));
        assert_eq!((body.changes[0].seconds, body.changes[1].when), (0.0, ChangeWhen::Below));
        set_body(&mut body, F::Comfort, &Value::Vector(vec![30.0, 5.0]));
        assert_eq!(body.comfort, [5.0, 30.0]);
        set_body(&mut body, F::Changes, &Value::Whole(0));
        assert!(body.changes.is_empty());
    }
}
