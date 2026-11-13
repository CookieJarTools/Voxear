//! Heat as scripts reach it: an Obj's Heat Body, Heat Source and Heat Zone
//! by their kinds (see `voxear_scripting::ComponentKind`), and `Heat` and
//! `Clock` through the hooks set here.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Transform, WorldTransform};
use voxear_scripting::{ComponentKind, HeatHooks};

use super::{Climate, Feels, HeatBody, HeatSource, HeatState, HeatZone, ZoneMode, ZoneShape};

/// Sets what answers `Heat`, `Clock`, `HeatBody`, `HeatSource` and
/// `HeatZone` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_heat_hooks(HeatHooks {
        sample: |world, at| super::sample(world, Vec3::from(at)).to_array(),
        add: |world, at, celsius, radius, seconds| super::add_source(world, Vec3::from(at), celsius, radius, seconds),
        clock: clock_value,
    });
    voxear_scripting::set_kind_values(ComponentKind::HeatBody, body_values);
    voxear_scripting::set_kind_texts(ComponentKind::HeatBody, body_texts);
    voxear_scripting::set_kind_values(ComponentKind::HeatSource, source_values);
    voxear_scripting::set_kind_values(ComponentKind::HeatZone, zone_values);
}

// What the values read or write, as `HeatBody`, `HeatSource` and `HeatZone` in Voxear.Core number them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const TEMPERATURE: i32 = 3;
const FEELS: i32 = 4;
const COLD_RESISTANCE: i32 = 5;
const HEAT_RESISTANCE: i32 = 6;
const WARMS_IN: i32 = 7;
const COMFORT: i32 = 8;
const BEYOND: i32 = 9;
const PLACE: i32 = 10;
const PROGRESS: i32 = 11;
const CHANGES: i32 = 12;

const CELSIUS: i32 = 3;
const RADIUS: i32 = 4;
const SHAPE: i32 = 4;
const SIZE: i32 = 5;
const MODE: i32 = 6;
const EDGE: i32 = 7;

// What the texts ask.
const INDEX_OF: i32 = 1;
const NAME_OF: i32 = 2;

// The clock's values.
const TIME_OF_DAY: i32 = 0;
const DAY_LENGTH: i32 = 1;
const TURNS_SUN: i32 = 2;
const EXISTS: i32 = 3;

const DONE: Option<[f32; 4]> = Some([0.0; 4]);

fn flag(on: bool) -> Option<[f32; 4]> {
    Some([f32::from(u8::from(on)), 0.0, 0.0, 0.0])
}

fn number(value: f32) -> Option<[f32; 4]> {
    Some([value, 0.0, 0.0, 0.0])
}

fn pick<T: Copy>(all: &[T], number: f32) -> T {
    all.get(number.round().max(0.0) as usize).copied().unwrap_or(all[0])
}

fn place_of<T: PartialEq>(all: &[T], one: &T) -> f32 {
    all.iter().position(|other| other == one).unwrap_or(0) as f32
}

/// Adds, takes away or tells whether `entity` has a `T`, for the three
/// values every kind shares; `None` for any other.
fn shared<T: voxear_ecs::Component + Default>(
    world: &mut World,
    entity: Entity,
    what: i32,
    written: Option<[f32; 4]>,
) -> Option<Option<[f32; 4]>> {
    match (what, written) {
        (HAS, None) => Some(flag(world.has::<T>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<T>(entity) {
                world.insert(entity, T::default());
            }
            Some(DONE)
        }
        (REMOVE, Some(_)) => Some(world.remove::<T>(entity).map(|_| [0.0; 4])),
        _ => None,
    }
}

fn position(world: &World, entity: Entity) -> Vec3 {
    match (world.get::<WorldTransform>(entity), world.get::<Transform>(entity)) {
        (Some(placed), _) => placed.0.w_axis.truncate(),
        (None, Some(local)) => local.position,
        (None, None) => Vec3::ZERO,
    }
}

/// `entity`'s Heat Body's state, made now from its place where it has none.
fn state(world: &mut World, entity: Entity) -> Option<HeatState> {
    let body = world.get::<HeatBody>(entity)?.clone();
    if let Some(state) = world.get::<HeatState>(entity) {
        return Some(state.clone());
    }
    let at = position(world, entity);
    let state = HeatState::new(&body, super::at(world, at));
    world.insert(entity, state.clone());
    Some(state)
}

/// Reads value `what` of `entity`'s Heat Body, or with `written`, writes it.
fn body_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    if let Some(answer) = shared::<HeatBody>(world, entity, what, written) {
        if what == REMOVE {
            world.remove::<HeatState>(entity);
        }
        return answer;
    }
    match (what, written) {
        (TEMPERATURE, None) => return number(state(world, entity)?.temperature),
        (PLACE, None) => return number(state(world, entity)?.place),
        (TEMPERATURE, Some([temperature, ..])) => {
            let mut now = state(world, entity)?;
            now.temperature = temperature;
            now.feels = world.get::<HeatBody>(entity)?.feels(temperature);
            world.insert(entity, now);
            return DONE;
        }
        (FEELS, None) => {
            let body = world.get::<HeatBody>(entity)?.clone();
            return number(place_of(&Feels::ALL, &body.feels(state(world, entity)?.temperature)));
        }
        (BEYOND, None) => {
            let body = world.get::<HeatBody>(entity)?.clone();
            return number(body.beyond(state(world, entity)?.temperature));
        }
        (PROGRESS, None) | (PROGRESS, Some(_)) => {
            let index = written.map_or(0, |[index, ..]| index.max(0.0) as usize);
            let body = world.get::<HeatBody>(entity)?.clone();
            let change = body.changes.get(index)?;
            let now = state(world, entity)?;
            let held = now.held.get(index).copied().unwrap_or(0.0);
            let done = now.done.get(index).copied().unwrap_or(false);
            let share =
                if change.seconds <= 0.0 { f32::from(u8::from(done)) } else { (held / change.seconds).min(1.0) };
            return Some([held, change.seconds, share, f32::from(u8::from(done))]);
        }
        _ => {}
    }
    let body = world.get_mut::<HeatBody>(entity)?;
    match (what, written) {
        (COLD_RESISTANCE, None) => number(body.cold_resistance),
        (HEAT_RESISTANCE, None) => number(body.heat_resistance),
        (WARMS_IN, None) => number(body.warms_in),
        (COMFORT, None) => Some([body.comfort[0], body.comfort[1], 0.0, 0.0]),
        (CHANGES, None) => number(body.changes.len() as f32),
        (COLD_RESISTANCE, Some([resistance, ..])) => {
            body.cold_resistance = resistance.max(0.0);
            DONE
        }
        (HEAT_RESISTANCE, Some([resistance, ..])) => {
            body.heat_resistance = resistance.max(0.0);
            DONE
        }
        (WARMS_IN, Some([seconds, ..])) => {
            body.warms_in = seconds.max(0.0);
            DONE
        }
        (COMFORT, Some([from, to, ..])) => {
            body.comfort = [from.min(to), from.max(to)];
            DONE
        }
        _ => None,
    }
}

/// A Heat Body's change by name: its place in the list, or its name by its
/// place, as text.
fn body_texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    let body = world.get::<HeatBody>(entity)?;
    match what {
        INDEX_OF => {
            Some(body.changes.iter().position(|change| change.name == text).map_or(-1, |at| at as i64).to_string())
        }
        NAME_OF => Some(body.changes.get(text.trim().parse::<usize>().ok()?)?.name.clone()),
        _ => None,
    }
}

fn source_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    if let Some(answer) = shared::<HeatSource>(world, entity, what, written) {
        return answer;
    }
    let source = world.get_mut::<HeatSource>(entity)?;
    match (what, written) {
        (CELSIUS, None) => number(source.celsius),
        (RADIUS, None) => number(source.radius),
        (CELSIUS, Some([celsius, ..])) => {
            source.celsius = celsius;
            DONE
        }
        (RADIUS, Some([radius, ..])) => {
            source.radius = radius.max(0.0);
            DONE
        }
        _ => None,
    }
}

fn zone_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    if let Some(answer) = shared::<HeatZone>(world, entity, what, written) {
        return answer;
    }
    let zone = world.get_mut::<HeatZone>(entity)?;
    match (what, written) {
        (CELSIUS, None) => number(zone.celsius),
        (SHAPE, None) => number(place_of(&ZoneShape::ALL, &zone.shape)),
        (SIZE, None) => Some([zone.size[0], zone.size[1], zone.size[2], 0.0]),
        (MODE, None) => number(place_of(&ZoneMode::ALL, &zone.mode)),
        (EDGE, None) => number(zone.edge),
        (CELSIUS, Some([celsius, ..])) => {
            zone.celsius = celsius;
            DONE
        }
        (SHAPE, Some([shape, ..])) => {
            zone.shape = pick(&ZoneShape::ALL, shape);
            DONE
        }
        (SIZE, Some([x, y, z, _])) => {
            zone.size = [x, y, z].map(|side| side.max(0.0));
            DONE
        }
        (MODE, Some([mode, ..])) => {
            zone.mode = pick(&ZoneMode::ALL, mode);
            DONE
        }
        (EDGE, Some([edge, ..])) => {
            zone.edge = edge.max(0.0);
            DONE
        }
        _ => None,
    }
}

/// The level's clock's value `what`, set first when `written`: as it
/// stands still at noon without a Climate, which nothing can set.
fn clock_value(world: &mut World, what: i32, written: Option<f32>) -> f32 {
    let mut found: Vec<Entity> = world.query_ref::<(Entity, &Climate)>().into_iter().map(|(e, _)| e).collect();
    found.sort_by_key(|entity| entity.to_bits());
    let climate = found.first().and_then(|&entity| world.get_mut::<Climate>(entity));
    let Some(climate) = climate else {
        if written.is_some() {
            tracing::warn!(target: "script", "Clock: the level has no Climate to keep its time");
        }
        return match what {
            TIME_OF_DAY => 12.0,
            _ => 0.0,
        };
    };
    match (what, written) {
        (TIME_OF_DAY, Some(hours)) => climate.time_of_day = hours.rem_euclid(24.0),
        (DAY_LENGTH, Some(minutes)) => climate.day_length = minutes.max(0.0),
        (TURNS_SUN, Some(on)) => climate.turns_sun = on != 0.0,
        _ => {}
    }
    match what {
        TIME_OF_DAY => climate.time_of_day,
        DAY_LENGTH => climate.day_length,
        TURNS_SUN => f32::from(u8::from(climate.turns_sun)),
        EXISTS => 1.0,
        _ => f32::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heat::HeatChange;

    #[test]
    fn scripts_read_and_write_a_heat_body_by_its_numbers() {
        let mut world = World::new();
        world.spawn((Climate { celsius: 20.0, day_swing: 0.0, ..Climate::default() }, Transform::default()));
        let changes = vec![HeatChange::default(), HeatChange { name: "Frozen".into(), ..HeatChange::default() }];
        let meat = world.spawn((HeatBody { changes, ..HeatBody::default() }, Transform::default()));
        assert_eq!(body_values(&mut world, meat, TEMPERATURE, None), number(20.0), "where it is, at first");
        assert_eq!(body_values(&mut world, meat, FEELS, None), number(2.0));
        body_values(&mut world, meat, TEMPERATURE, Some([-30.0, 0.0, 0.0, 0.0]));
        assert_eq!(body_values(&mut world, meat, FEELS, None), number(0.0), "freezing");
        body_values(&mut world, meat, COLD_RESISTANCE, Some([40.0, 0.0, 0.0, 0.0]));
        assert_eq!(body_values(&mut world, meat, FEELS, None), number(2.0), "comfortable in its coat");
        assert_eq!(body_texts(&mut world, meat, INDEX_OF, "Frozen").as_deref(), Some("1"));
        assert_eq!(body_texts(&mut world, meat, INDEX_OF, "Burnt").as_deref(), Some("-1"));
        assert_eq!(body_values(&mut world, meat, PROGRESS, Some([1.0, 0.0, 0.0, 0.0])), Some([0.0, 10.0, 0.0, 0.0]));

        let fire = world.spawn(Transform::default());
        source_values(&mut world, fire, ADD, DONE);
        source_values(&mut world, fire, CELSIUS, Some([700.0, 0.0, 0.0, 0.0]));
        assert_eq!(world.get::<HeatSource>(fire).unwrap().celsius, 700.0);

        assert_eq!(clock_value(&mut world, TIME_OF_DAY, Some(30.0)), 6.0);
        assert_eq!(clock_value(&mut world, EXISTS, None), 1.0);
    }
}
