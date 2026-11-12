//! Storms as scripts reach them: `Storms`, `Storm` and `Sea` in
//! Voxear.Core, through the one call set here. Places are given and told in
//! the world; storms keep theirs from the level's middle.

use voxear_core::math::{DVec2, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::sea::SeaLook;
use voxear_scene::{Destroying, WorldOrigin};
use voxear_scripting::STORM_NUMBERS;

use super::{SeaNow, Storm, StormEvent, StormKind, StormRules, Storms};

/// Sets what answers `Storms`, `Storm` and `Sea` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_storm_call(call);
}

// What a call asks, as `Storms` in Voxear.Core numbers them.
const START: i32 = 0;
const GET: i32 = 1;
const SET_PLACE: i32 = 2;
const SET_RADIUS: i32 = 3;
const SET_STRENGTH: i32 = 4;
const SET_KIND: i32 = 5;
const SET_VELOCITY: i32 = 6;
const SET_EYE: i32 = 7;
const SET_LIFE: i32 = 8;
const STOP: i32 = 9;
const PATH_CLEAR: i32 = 10;
const PATH_ADD: i32 = 11;
const COUNT: i32 = 12;
const ID_AT: i32 = 13;
const WEATHER_AT: i32 = 14;
const EVENT: i32 = 15;
const POOL_ADD: i32 = 16;
const POOL_REMOVE: i32 = 17;
const POOL_AT: i32 = 18;
const RULE: i32 = 19;
const SET_RULE: i32 = 20;
const CLOCK: i32 = 21;
const SET_CLOCK: i32 = 22;
const STRENGTH_AT: i32 = 23;

// The rules, as `Storms` and `Sea` number them.
const NATURAL: u32 = 0;
const FREQUENCY: u32 = 1;
const LARGEST: u32 = 2;
const STANDING: u32 = 3;
const SEA_LOOK: u32 = 4;
const SWELL: u32 = 5;
const WHIRLPOOLS: u32 = 6;
const MAELSTROMS: u32 = 7;
const GLOW: u32 = 8;

/// The level's first storm rules' Obj.
fn rules_obj(world: &World) -> Option<Entity> {
    let mut found: Vec<Entity> =
        world.query_ref::<(Entity, &StormRules)>().without::<Destroying>().into_iter().map(|(e, _)| e).collect();
    found.sort_by_key(|entity| entity.to_bits());
    found.first().copied()
}

/// The level's storm rules as they are: its first Obj's with any, else
/// what scripts last asked for, else the usual with no natural storms.
fn rules(world: &World) -> StormRules {
    let of_obj = rules_obj(world).and_then(|entity| world.get::<StormRules>(entity).copied());
    let asked = world.resource::<Storms>().and_then(|storms| storms.asked);
    of_obj.or(asked).unwrap_or(StormRules { natural: false, ..StormRules::default() })
}

fn rule(rules: &StormRules, which: u32) -> f32 {
    let flag = |on: bool| f32::from(u8::from(on));
    match which {
        NATURAL => flag(rules.natural),
        FREQUENCY => rules.frequency,
        LARGEST => rules.largest,
        STANDING => flag(rules.standing),
        SEA_LOOK => rules.sea_look.index() as f32,
        SWELL => rules.swell,
        WHIRLPOOLS => flag(rules.whirlpools),
        MAELSTROMS => flag(rules.maelstroms),
        GLOW => rules.glow,
        _ => f32::NAN,
    }
}

fn set_rule(rules: &mut StormRules, which: u32, value: f32) -> bool {
    let on = value > 0.5;
    match which {
        NATURAL => rules.natural = on,
        FREQUENCY => rules.frequency = value.clamp(0.0, 1.0),
        LARGEST => rules.largest = value.max(1.0),
        STANDING => rules.standing = on,
        SEA_LOOK => rules.sea_look = SeaLook::ALL.get(value.round().max(0.0) as usize).copied().unwrap_or_default(),
        SWELL => rules.swell = value.max(0.0),
        WHIRLPOOLS => rules.whirlpools = on,
        MAELSTROMS => rules.maelstroms = on,
        GLOW => rules.glow = value.max(0.0),
        _ => return false,
    }
    true
}

/// A number's bits as two numbers, low half first, as scripts put them
/// back together.
fn bits(of: u64) -> [f32; 2] {
    [f32::from_bits(of as u32), f32::from_bits((of >> 32) as u32)]
}

fn call(world: &mut World, what: i32, id: u32, v: &mut [f32; STORM_NUMBERS]) -> i32 {
    let origin = WorldOrigin::of(world);
    let to_level = |x: f32, z: f32| {
        let level = origin.to_level(Vec3::new(x, 0.0, z));
        [level.x, level.z]
    };
    let to_world = |at: [f64; 2]| [(at[0] - origin.0.x as f64) as f32, (at[1] - origin.0.z as f64) as f32];
    if !v.iter().all(|number| number.is_finite()) && what != EVENT {
        return 0;
    }
    match what {
        START => {
            let kind = StormKind::ALL.get(v[5].round().max(0.0) as usize).copied().unwrap_or_default();
            super::start(world, Vec3::new(v[0], v[1], v[2]), v[3], v[4], kind) as i32
        }
        WEATHER_AT => {
            let at = super::weather_at(world, Vec3::new(v[0], v[1], v[2]));
            v[..8].copy_from_slice(&at.amounts.to_array());
            v[8] = at.falling.index() as f32;
            (v[9], v[10], v[11]) = (at.strength, at.celsius, at.humidity);
            at.storm as i32
        }
        POOL_ADD => super::add_whirlpool(world, Vec3::new(v[0], v[1], v[2]), v[3], v[4]) as i32,
        POOL_AT => {
            let point = world.resource::<SeaNow>().map(|sea| sea.0.at(v[0], v[2])).unwrap_or_default();
            *v = [0.0; STORM_NUMBERS];
            (v[0], v[1], v[2], v[3], v[4]) = (point.whirlpool, point.flow.x, point.flow.y, point.flow.z, point.sunk);
            i32::from(point.whirlpool > 0.0)
        }
        RULE => {
            v[0] = rule(&rules(world), id);
            i32::from(v[0].is_finite())
        }
        SET_RULE => {
            let mut changed = rules(world);
            if !set_rule(&mut changed, id, v[0]) {
                return 0;
            }
            match rules_obj(world) {
                Some(entity) => drop(world.insert(entity, changed)),
                None => {
                    let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
                    storms.asked = Some(changed);
                    world.insert_resource(storms);
                }
            }
            1
        }
        _ => {
            let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
            let answer = of_storms(&mut storms, what, id, v, (&to_level, &to_world));
            world.insert_resource(storms);
            answer
        }
    }
}

type Places<'a> = (&'a dyn Fn(f32, f32) -> [f64; 2], &'a dyn Fn([f64; 2]) -> [f32; 2]);

/// What a call asks of the storms themselves.
fn of_storms(storms: &mut Storms, what: i32, id: u32, v: &mut [f32; STORM_NUMBERS], (to_level, to_world): Places) -> i32 {
    let clock = storms.clock;
    let change = |storms: &mut Storms, with: &mut dyn FnMut(&mut Storm)| match storms.made_mut(id) {
        Some(storm) => {
            with(storm);
            1
        }
        None => 0,
    };
    match what {
        GET => {
            let Some(storm) = storms.get(id) else { return 0 };
            let [x, z] = to_world(storm.middle);
            let left = if storm.lasts.is_finite() { storm.lasts - (clock - storm.born) as f32 } else { -1.0 };
            let numbers = [
                x,
                0.0,
                z,
                storm.radius,
                storm.strength,
                storm.kind.index() as f32,
                storm.velocity[0],
                storm.velocity[1],
                storm.eye,
                storm.strength_at(clock),
                left,
                f32::from(u8::from(storm.is_natural())),
                storm.radius_at(clock),
                storm.path.len() as f32,
            ];
            v[..numbers.len()].copy_from_slice(&numbers);
            1
        }
        STRENGTH_AT => {
            let Some(storm) = storms.get(id) else { return 0 };
            let from = DVec2::from(storm.middle).distance(DVec2::from(to_level(v[0], v[2]))) as f32;
            v[0] = storm.strength_over(clock, from);
            1
        }
        SET_PLACE => change(storms, &mut |storm| storm.middle = to_level(v[0], v[2])),
        SET_RADIUS => change(storms, &mut |storm| storm.radius = v[0].max(1.0)),
        SET_STRENGTH => change(storms, &mut |storm| storm.strength = v[0].clamp(0.0, 1.0)),
        SET_KIND => {
            let kind = StormKind::ALL.get(v[0].round().max(0.0) as usize).copied().unwrap_or_default();
            change(storms, &mut |storm| storm.kind = kind)
        }
        SET_VELOCITY => change(storms, &mut |storm| storm.velocity = [v[0], v[2]]),
        SET_EYE => change(storms, &mut |storm| storm.eye = v[0].clamp(0.0, 0.8)),
        // How long it lives from now, dying away at its end; without end below nothing.
        SET_LIFE => change(storms, &mut |storm| {
            storm.lasts = if v[0] < 0.0 { f32::INFINITY } else { (clock - storm.born) as f32 + v[0] };
            storm.fades = storm.fades.min(v[0].max(0.0));
        }),
        STOP => i32::from(storms.stop(id, v[0])),
        PATH_CLEAR => change(storms, &mut |storm| storm.path.clear()),
        PATH_ADD => {
            let to = to_level(v[0], v[2]);
            let speed = v[3];
            change(storms, &mut |storm| {
                storm.path.push(to);
                // A storm standing still sets off at the speed given.
                if Vec2::from(storm.velocity).length() <= 0.0 && speed > 0.0 {
                    storm.velocity = [speed, 0.0];
                }
            })
        }
        COUNT => storms.all().len() as i32,
        ID_AT => storms.all().get(v[0].round().max(0.0) as usize).map_or(0, |storm| storm.id as i32),
        EVENT => {
            let Some(event) = storms.next_event() else { return 0 };
            let (kind, about) = match event {
                StormEvent::Arrived(id) => (1, u64::from(id)),
                StormEvent::Left(id) => (2, u64::from(id)),
                StormEvent::Swallowed(entity) => (3, entity.to_bits()),
            };
            [v[0], v[1]] = bits(about);
            kind
        }
        POOL_REMOVE => i32::from(storms.remove_whirlpool(id)),
        CLOCK => {
            // Whole seconds and the part over, as a number alone cannot hold a long clock.
            (v[0], v[1]) = (clock.floor() as f32, clock.fract() as f32);
            1
        }
        SET_CLOCK => {
            storms.clock = f64::from(v[0]) + f64::from(v[1]);
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::Camera;
    use voxear_scene::Transform;

    #[test]
    fn scripts_start_a_storm_change_it_and_ask_the_weather_under_it() {
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 2.0, 0.0))));
        let mut v = [0.0; STORM_NUMBERS];
        v[..6].copy_from_slice(&[100.0, 0.0, -50.0, 300.0, 0.9, StormKind::Blizzard.index() as f32]);
        let id = call(&mut world, START, 0, &mut v) as u32;
        assert_eq!(id, 1);
        assert_eq!(call(&mut world, GET, id, &mut v), 1);
        assert_eq!(v[..6], [100.0, 0.0, -50.0, 300.0, 0.9, 3.0]);
        assert_eq!((v[10], v[11]), (-1.0, 0.0), "without end, and made");
        // Moved over the viewer, at full strength at once.
        v = [0.0; STORM_NUMBERS];
        assert_eq!(call(&mut world, SET_PLACE, id, &mut v), 1);
        world.resource_mut::<Storms>().unwrap().made_mut(id).unwrap().grows = 0.0;
        super::super::update(&mut world, 0.1, true);
        v = [0.0; STORM_NUMBERS];
        v[1] = 2.0;
        assert_eq!(call(&mut world, WEATHER_AT, 0, &mut v) as u32, id);
        assert_eq!((v[5], v[8], v[9]), (0.9, super::super::Falling::Blizzard.index() as f32, 0.9));
        v = [0.0; STORM_NUMBERS];
        assert_eq!((call(&mut world, STRENGTH_AT, id, &mut v), v[0]), (1, 0.9));
        // Scripts hear it arrive, once.
        v = [0.0; STORM_NUMBERS];
        assert_eq!(call(&mut world, EVENT, 0, &mut v), 1);
        assert_eq!((v[0].to_bits(), v[1].to_bits()), (id, 0));
        assert_eq!(call(&mut world, EVENT, 0, &mut v), 0);
        assert_eq!((call(&mut world, COUNT, 0, &mut v), call(&mut world, ID_AT, 0, &mut [0.0; STORM_NUMBERS]) as u32), (1, id));
        // Sent along a path, it sets off; stopped, it is gone in time.
        v = [0.0; STORM_NUMBERS];
        (v[0], v[2], v[3]) = (500.0, 0.0, 10.0);
        assert_eq!(call(&mut world, PATH_ADD, id, &mut v), 1);
        super::super::update(&mut world, 10.0, true);
        call(&mut world, GET, id, &mut v);
        assert!((v[0] - 100.0).abs() < 0.01 && v[13] == 1.0, "a hundred metres on: {}", v[0]);
        v = [0.0; STORM_NUMBERS];
        v[0] = 5.0;
        assert_eq!(call(&mut world, STOP, id, &mut v), 1);
        super::super::update(&mut world, 6.0, true);
        assert_eq!(call(&mut world, GET, id, &mut v), 0);
        assert_eq!(call(&mut world, SET_RADIUS, id, &mut v), 0, "nothing to change");
    }

    #[test]
    fn scripts_read_and_set_the_rules_with_no_obj_holding_them_and_on_the_obj_that_does() {
        let mut world = World::new();
        let mut v = [0.0; STORM_NUMBERS];
        assert_eq!((call(&mut world, RULE, NATURAL, &mut v), v[0]), (1, 0.0), "none by themselves unless asked");
        v[0] = 1.0;
        assert_eq!(call(&mut world, SET_RULE, NATURAL, &mut v), 1);
        v[0] = SeaLook::Steps.index() as f32;
        call(&mut world, SET_RULE, SEA_LOOK, &mut v);
        assert_eq!(rules(&world), StormRules { sea_look: SeaLook::Steps, ..StormRules::default() });
        // An Obj's rules are the level's, and what scripts set after is set on it.
        let obj = world.spawn((StormRules { swell: 5.0, ..StormRules::default() },));
        assert_eq!((call(&mut world, RULE, SWELL, &mut v), v[0]), (1, 5.0));
        v[0] = 0.2;
        call(&mut world, SET_RULE, FREQUENCY, &mut v);
        assert_eq!(world.get::<StormRules>(obj).unwrap().frequency, 0.2);
        assert_eq!(call(&mut world, SET_RULE, 99, &mut v), 0);
        // The clock, and whirlpools.
        (v[0], v[1]) = (86_400.0, 0.5);
        call(&mut world, SET_CLOCK, 0, &mut v);
        assert_eq!(world.resource::<Storms>().unwrap().clock, 86_400.5);
        v = [0.0; STORM_NUMBERS];
        (v[3], v[4]) = (20.0, 6.0);
        let pool = call(&mut world, POOL_ADD, 0, &mut v) as u32;
        assert!(pool > 0 && call(&mut world, POOL_REMOVE, pool, &mut v) == 1 && call(&mut world, POOL_REMOVE, pool, &mut v) == 0);
    }
}
