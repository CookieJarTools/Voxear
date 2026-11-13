//! Heat: how hot or cold it is anywhere, and what it does to what is there.
//!
//! [`sample`] gives a place's temperature, in °C, and what it is made of
//! (see `field`): the climate, the time of day on the level's clock,
//! shelter under blocks, water, Heat Zones, and hot and cold things near,
//! Heat Sources, hot and cold blocks, lava and short-lived heat.
//!
//! A Heat Body's temperature follows its place's, on each fixed step while
//! the game plays (the same on every machine). A character feels it, and
//! each of its changes counts while it is hot or cold enough: once one has
//! counted long enough, scripts hear of it and the Obj becomes its prefab.
//! The engine only notes what is due ([`HeatDue`]); the game makes the
//! prefabs, as only it has them, and tells scripts after the frame.
//!
//! The level's first Climate holds its clock: the time of day, which turns
//! while the game plays and may turn the sun.

mod field;
mod parts;
pub(crate) mod scripts;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Light, LightKind};
use voxear_scene::{Destroying, Transform, WorldTransform};

pub use field::{Burst, HeatSample, HeatWorld, LAVA, climate, day_curve};
pub use parts::{
    ChangeWhen, Climate, EXTREME, Feels, HeatBody, HeatChange, HeatSource, HeatState, HeatZone, ZoneMode, ZoneShape,
};

use crate::{Content, blocks::BlockWorldAsset};

/// How often a Heat Body looks at its place's temperature, in seconds.
const LOOK_EVERY: f32 = 0.2;
/// How far the sun's path leans from straight overhead, in radians.
const SUN_LEAN: f32 = 0.45;

/// How hot or cold it is at `at`, and what makes it so.
pub fn sample(world: &mut World, at: Vec3) -> HeatSample {
    field::sample(world, at)
}

/// How hot or cold it is at `at`, in °C.
pub fn at(world: &mut World, at: Vec3) -> f32 {
    sample(world, at).celsius
}

/// Sets off heat at `at` for `seconds` of the game's time: `celsius` there,
/// fading by `radius` metres, as a blast's or a spell's.
pub fn add_source(world: &mut World, at: Vec3, celsius: f32, radius: f32, seconds: f32) {
    let mut heat = world.remove_resource::<HeatWorld>().unwrap_or_default();
    let until = heat.now + f64::from(seconds.max(0.0));
    heat.bursts.push(Burst { at, celsius, radius: radius.max(0.0), until });
    world.insert_resource(heat);
}

/// What a Heat Body's change made due: which body, which change, its name
/// and the prefab it becomes, where it names one.
#[derive(Clone, Debug, PartialEq)]
pub struct DueChange {
    pub entity: Entity,
    pub index: usize,
    pub name: String,
    pub becomes: Option<String>,
}

/// What Heat Bodies made due since the game last took it: changes, and
/// characters that feel otherwise. A world resource.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeatDue {
    pub changes: Vec<DueChange>,
    pub feels: Vec<(Entity, Feels)>,
}

/// What Heat Bodies made due, taken.
pub fn take_due(world: &mut World) -> HeatDue {
    world.remove_resource::<HeatDue>().unwrap_or_default()
}

/// Gives heat each world of blocks' generator, for its climate.
pub fn prepare(world: &mut World, content: &mut Content) {
    let worlds: Vec<(Entity, BlockWorldAsset)> = world
        .query_ref::<(Entity, &BlockWorldAsset)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, asset)| (entity, asset.clone()))
        .collect();
    let mut heat = world.remove_resource::<HeatWorld>().unwrap_or_default();
    heat.generators.clear();
    for (entity, asset) in worlds {
        let Some(generator) = crate::blocks::generator(content, &asset.file) else { continue };
        let position = asset.file.generate.as_ref().map_or([0.0; 2], |generate| generate.position);
        heat.generators.insert(entity, (generator, position));
    }
    world.insert_resource(heat);
}

/// Turns the level's clock on by `delta` seconds while `playing`, and the
/// sun with it where its Climate says.
pub fn turn_clock(world: &mut World, delta: f32, playing: bool) {
    let mut found: Vec<Entity> =
        world.query_ref::<(Entity, &Climate)>().without::<Destroying>().into_iter().map(|(e, _)| e).collect();
    found.sort_by_key(|entity| entity.to_bits());
    let Some(climate) = found.first().and_then(|&entity| world.get_mut::<Climate>(entity)) else { return };
    if playing && climate.day_length > 0.0 {
        climate.time_of_day = (climate.time_of_day + delta * 24.0 / (climate.day_length * 60.0)).rem_euclid(24.0);
    }
    if !climate.turns_sun {
        return;
    }
    let rotation = sun_rotation(climate.time_of_day);
    let sun = world
        .query::<(&Light, &mut Transform)>()
        .without::<Destroying>()
        .into_iter()
        .find(|(light, _)| light.kind == LightKind::Directional);
    if let Some((_, transform)) = sun
        && transform.rotation.dot(rotation).abs() < 0.999_999
    {
        transform.rotation = rotation;
    }
}

/// Which way the sun shines at `hours`: up in the east at six, highest at
/// noon, down in the west at six, and under the ground at night. Only
/// drawn, so it may use the sine.
pub fn sun_rotation(hours: f32) -> Quat {
    let angle = (hours - 6.0) / 12.0 * std::f32::consts::PI;
    let (sin, cos) = angle.sin_cos();
    let towards_sun = Vec3::new(-cos, sin * SUN_LEAN.cos(), sin * SUN_LEAN.sin());
    Quat::from_rotation_arc(Vec3::Z, -towards_sun.normalize())
}

/// Moves every Heat Body on by `delta` seconds of the game's time, `now`:
/// towards its place's temperature, feeling it, and counting its changes.
pub fn update(world: &mut World, delta: f32, now: f64) {
    let mut heat = world.remove_resource::<HeatWorld>().unwrap_or_default();
    heat.now = now;
    heat.bursts.retain(|burst| burst.until > now);
    let mut bodies: Vec<(Entity, HeatBody, Vec3)> = world
        .query_ref::<(Entity, &HeatBody, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, body, local, placed)| {
            (entity, body.clone(), placed.map_or(local.position, |w| w.0.w_axis.truncate()))
        })
        .collect();
    bodies.sort_by_key(|(entity, ..)| entity.to_bits());
    let inactive = if bodies.is_empty() { Default::default() } else { voxear_scene::inactive_objs(world) };
    let mut due = world.remove_resource::<HeatDue>().unwrap_or_default();
    for (entity, body, at) in bodies {
        if inactive.contains(&entity) {
            continue;
        }
        let mut state = match world.get::<HeatState>(entity).cloned() {
            Some(state) => state,
            None => HeatState::new(&body, field::sample_with(world, &mut heat, at).celsius),
        };
        if state.going {
            continue;
        }
        // A change added or taken away while playing starts afresh.
        if state.held.len() != body.changes.len() {
            state.held.resize(body.changes.len(), 0.0);
            state.done.resize(body.changes.len(), false);
        }
        state.since += delta;
        if state.since >= LOOK_EVERY {
            state.place = field::sample_with(world, &mut heat, at).celsius;
            state.since = 0.0;
        }
        let share = if body.warms_in <= 0.0 { 1.0 } else { delta / (body.warms_in + delta) };
        state.temperature += (state.place - state.temperature) * share;
        let feels = body.feels(state.temperature);
        if feels != state.feels {
            state.feels = feels;
            due.feels.push((entity, feels));
        }
        for (index, change) in body.changes.iter().enumerate() {
            if state.done[index] || !change.when.holds(state.temperature, change.celsius) {
                continue;
            }
            state.held[index] += delta;
            if state.held[index] < change.seconds.max(0.0) {
                continue;
            }
            state.done[index] = true;
            due.changes.push(DueChange { entity, index, name: change.name.clone(), becomes: change.becomes.clone() });
            if change.becomes.is_some() {
                state.going = true;
                break;
            }
        }
        world.insert(entity, state);
    }
    world.insert_resource(heat);
    if !due.changes.is_empty() || !due.feels.is_empty() {
        world.insert_resource(due);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(climate: Climate) -> World {
        let mut world = World::new();
        world.spawn((climate, Transform::default()));
        world
    }

    #[test]
    fn a_dry_place_is_hotter_by_day_than_by_night_and_swings_more_than_a_wet_one() {
        let dry = Climate { celsius: 30.0, humidity: 0.0, day_swing: 10.0, ..Climate::default() };
        let mut world = level(Climate { time_of_day: 15.0, ..dry });
        let day = at(&mut world, Vec3::ZERO);
        let mut world = level(Climate { time_of_day: 4.0, ..dry });
        let night = at(&mut world, Vec3::ZERO);
        assert!(day > 40.0 && night < 20.0, "{day} by day, {night} by night");
        let mut world = level(Climate { time_of_day: 15.0, humidity: 1.0, ..dry });
        assert!(at(&mut world, Vec3::ZERO) < day - 5.0, "a wet place swings less");
    }

    #[test]
    fn the_clock_turns_while_playing_and_turns_the_sun() {
        let mut world = level(Climate { time_of_day: 23.0, day_length: 24.0, turns_sun: true, ..Climate::default() });
        let sun = world.spawn((Light::directional(Vec3::ONE, 1.0), Transform::default()));
        turn_clock(&mut world, 60.0, false);
        assert_eq!(climate(&world).unwrap().time_of_day, 23.0, "the clock stands still in the editor");
        turn_clock(&mut world, 120.0, true);
        assert_eq!(climate(&world).unwrap().time_of_day, 1.0, "two minutes of a day of 24 are two hours");
        let noon = sun_rotation(12.0) * Vec3::Z;
        assert!(noon.y < -0.85, "the sun shines down at noon: {noon}");
        assert!((sun_rotation(18.0) * Vec3::Z).y.abs() < 1e-4, "and sets at six");
        let shining = world.get::<Transform>(sun).unwrap().rotation * Vec3::Z;
        assert!(shining.y > 0.0, "under the ground at one in the morning: {shining}");
    }

    #[test]
    fn heat_sources_warm_what_is_near_them_and_bursts_go_out() {
        let mut world = level(Climate::default());
        let open = at(&mut world, Vec3::ZERO);
        world.spawn((HeatSource { celsius: 400.0, radius: 4.0 }, Transform::from_position(Vec3::new(10.0, 0.0, 0.0))));
        assert_eq!(at(&mut world, Vec3::ZERO), open, "too far to feel");
        let near = at(&mut world, Vec3::new(8.0, 0.0, 0.0));
        assert!(near > open + 50.0 && near < 400.0, "{near}");
        assert_eq!(at(&mut world, Vec3::new(10.0, 0.0, 0.0)), 400.0);
        world.spawn((HeatSource { celsius: -40.0, radius: 4.0 }, Transform::default()));
        assert_eq!(at(&mut world, Vec3::ZERO), -40.0, "an ice block's cold");

        add_source(&mut world, Vec3::new(0.0, 0.0, 20.0), 900.0, 2.0, 1.0);
        assert_eq!(at(&mut world, Vec3::new(0.0, 0.0, 20.0)), 900.0);
        update(&mut world, 0.02, 2.0);
        assert_eq!(at(&mut world, Vec3::new(0.0, 0.0, 20.0)), open, "out once its second is up");
    }

    #[test]
    fn a_zone_sets_its_temperature_or_adds_to_it() {
        let mut world = level(Climate { celsius: 10.0, day_swing: 0.0, ..Climate::default() });
        world.spawn((HeatZone { celsius: 40.0, edge: 0.0, ..HeatZone::default() }, Transform::default()));
        let sample = sample(&mut world, Vec3::ZERO);
        assert_eq!((sample.celsius, sample.zones), (40.0, 30.0));
        let added = HeatZone { mode: ZoneMode::Add, celsius: 5.0, edge: 0.0, ..HeatZone::default() };
        world.spawn((added, Transform::from_position(Vec3::new(0.0, 0.0, 3.0))));
        assert_eq!(at(&mut world, Vec3::new(0.0, 0.0, 3.0)), 45.0);
    }

    #[test]
    fn a_body_warms_feels_and_changes_once_held_hot_for_long_enough() {
        let mut world = level(Climate { celsius: 20.0, day_swing: 0.0, ..Climate::default() });
        let seared = HeatChange { name: "Seared".into(), celsius: 90.0, seconds: 2.0, ..HeatChange::default() };
        let body = HeatBody { warms_in: 0.5, changes: vec![seared], ..HeatBody::default() };
        let meat = world.spawn((body, Transform::default()));
        update(&mut world, 0.1, 0.0);
        assert_eq!(world.get::<HeatState>(meat).unwrap().temperature, 20.0);
        assert!(take_due(&mut world).changes.is_empty());

        let fire = world.spawn((HeatSource { celsius: 200.0, radius: 2.0 }, Transform::default()));
        let mut time = 0.0;
        let step = 1.0 / 50.0;
        while time < 1.0 {
            update(&mut world, step, f64::from(time));
            time += step;
        }
        let state = world.get::<HeatState>(meat).unwrap().clone();
        assert!(state.temperature > 150.0, "warmed: {}", state.temperature);
        assert_eq!(state.feels, Feels::Scorching);
        assert!(state.held[0] > 0.0 && state.held[0] < 2.0);
        let due = take_due(&mut world);
        assert!(due.feels.contains(&(meat, Feels::Scorching)) && due.changes.is_empty());

        // Taken off the fire, its time holds still once it has cooled.
        world.despawn(fire);
        for _ in 0..100 {
            update(&mut world, step, 0.0);
        }
        let cooled = world.get::<HeatState>(meat).unwrap().clone();
        assert!(cooled.temperature < 90.0 && cooled.held[0] < 2.0, "{cooled:?}");
        for _ in 0..200 {
            update(&mut world, step, 0.0);
        }
        assert_eq!(world.get::<HeatState>(meat).unwrap().held[0], cooled.held[0]);
        world.spawn((HeatSource { celsius: 200.0, radius: 2.0 }, Transform::default()));
        for _ in 0..200 {
            update(&mut world, step, 0.0);
        }
        let due = take_due(&mut world);
        assert_eq!(due.changes.len(), 1, "once");
        assert_eq!((due.changes[0].entity, due.changes[0].name.as_str()), (meat, "Seared"));
        assert!(world.get::<HeatState>(meat).unwrap().done[0]);
    }
}
