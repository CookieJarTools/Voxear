//! Guns for scripts: a Gun's and a Gun Holder's values, as `Gun` and
//! `GunHolder` in Voxear.Guns number them. A game without the Guns module
//! has no Voxear.Guns to ask.

use voxear_ecs::{Entity, World};
use voxear_scripting::ComponentKind;

use super::actions::ActionKind;
use super::gun::{Gun, GunKind};
use super::holder::{GunHolder, GunHolderState};
use super::mechanism::{Chamber, GunState, Known};
use crate::cameras::{obj_value, value_obj};

const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;

// A Gun's values.
const KIND: i32 = 3;
const MASS: i32 = 4;
const LENGTH: i32 = 5;
const FIRE_RATE: i32 = 6;
const RECOIL: i32 = 7;
const GRIP: i32 = 8;
const SUPPORT: i32 = 9;
const REAR_SIGHT: i32 = 10;
const FRONT_SIGHT: i32 = 11;
const EYE_RELIEF: i32 = 12;
const AIM_FOV: i32 = 13;
const MUZZLE: i32 = 14;
const ROUNDS: i32 = 15;
const CHAMBER: i32 = 16;
const FEED_KNOWN: i32 = 17;
const CHAMBER_KNOWN: i32 = 18;
const CAPACITY: i32 = 19;
const OPEN: i32 = 20;

// A Gun Holder's values.
const GUN: i32 = 3;
const AIMING: i32 = 4;
const AIMED: i32 = 5;
const HIP: i32 = 6;
const AIM_TIME: i32 = 7;
const SWAY: i32 = 8;
const BOB: i32 = 9;
const BREATHING: i32 = 10;
const WALL_CHECK: i32 = 11;
const FIRE: i32 = 12;
const SHOT_ORIGIN: i32 = 13;
const SHOT_DIRECTION: i32 = 14;
const REQUEST: i32 = 15;
const CANCEL: i32 = 16;
const ACTING: i32 = 17;
const SPARE: i32 = 18;
const AUTO_CYCLE: i32 = 19;

/// The actions in the order Voxear.Guns numbers them.
const ACTIONS: [ActionKind; 4] =
    [ActionKind::Reload, ActionKind::Cycle, ActionKind::CheckMagazine, ActionKind::CheckChamber];

fn known(known: Known) -> f32 {
    match known {
        Known::Unknown => 0.0,
        Known::Rough => 1.0,
        Known::Exact => 2.0,
    }
}

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Gun, gun_values);
    voxear_scripting::set_kind_values(ComponentKind::GunHolder, holder_values);
}

fn number(n: f32) -> [f32; 4] {
    [n, 0.0, 0.0, 0.0]
}

fn flag(on: bool) -> [f32; 4] {
    number(f32::from(u8::from(on)))
}

fn point(p: [f32; 3]) -> [f32; 4] {
    [p[0], p[1], p[2], 0.0]
}

/// Answers whether an Obj has a `T`, gives it one or takes it away, for
/// the values every kind has; `None` for the rest.
fn presence<T: voxear_ecs::Component + Default>(
    world: &mut World,
    entity: Entity,
    what: i32,
    written: Option<[f32; 4]>,
) -> Option<Option<[f32; 4]>> {
    if !world.is_alive(entity) || !crate::modules::has(world, crate::modules::Module::Guns) {
        return Some(None);
    }
    match (what, written) {
        (HAS, None) => Some(Some(flag(world.has::<T>(entity)))),
        (ADD, Some(_)) => {
            if !world.has::<T>(entity) {
                world.insert(entity, T::default());
            }
            Some(Some([0.0; 4]))
        }
        (REMOVE, Some(_)) => Some(world.remove::<T>(entity).map(|_| [0.0; 4])),
        _ => None,
    }
}

fn gun_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if let Some(answer) = presence::<Gun>(world, entity, what, written) {
        return answer;
    }
    if (CHAMBER..=OPEN).contains(&what) || what == ROUNDS {
        let state = super::mechanism::state_of(world, entity)?;
        let kind = world.get::<Gun>(entity)?.kind;
        return match (what, written) {
            (ROUNDS, None) => Some(number(state.rounds as f32)),
            (ROUNDS, Some(v)) => {
                let rounds = (v[0].max(0.0) as u32).min(kind.capacity());
                world.get_mut::<GunState>(entity)?.rounds = rounds;
                Some([0.0; 4])
            }
            (CHAMBER, None) => Some(number(match state.chamber {
                Chamber::Empty => 0.0,
                Chamber::Live => 1.0,
                Chamber::Spent => 2.0,
            })),
            (FEED_KNOWN, None) => Some(number(known(state.known_feed))),
            (CHAMBER_KNOWN, None) => Some(number(known(state.known_chamber))),
            (CAPACITY, None) => Some(number(kind.capacity() as f32)),
            (OPEN, None) => Some(flag(state.open)),
            _ => None,
        };
    }
    let gun = world.get_mut::<Gun>(entity)?;
    let Some(v) = written else {
        return match what {
            KIND => Some(number(GunKind::ALL.iter().position(|k| *k == gun.kind).unwrap_or(0) as f32)),
            MASS => Some(number(gun.mass)),
            LENGTH => Some(number(gun.length)),
            FIRE_RATE => Some(number(gun.fire_rate)),
            RECOIL => Some(number(gun.recoil)),
            GRIP => Some(point(gun.grip.position)),
            SUPPORT => Some(point(gun.support.position)),
            REAR_SIGHT => Some(point(gun.rear_sight)),
            FRONT_SIGHT => Some(point(gun.front_sight)),
            EYE_RELIEF => Some(number(gun.eye_relief)),
            AIM_FOV => Some(number(gun.aim_fov)),
            MUZZLE => Some(point(gun.muzzle)),
            _ => None,
        };
    };
    let at = [v[0], v[1], v[2]];
    match what {
        // A kind fills in its typical numbers, as in the Inspector.
        KIND => {
            let kind = GunKind::ALL.get(v[0].max(0.0) as usize).copied().unwrap_or_default();
            *gun = Gun { handling: std::mem::take(&mut gun.handling), ..Gun::preset(kind) };
        }
        MASS => gun.mass = v[0].max(0.1),
        LENGTH => gun.length = v[0].max(0.05),
        FIRE_RATE => gun.fire_rate = v[0].max(1.0),
        RECOIL => gun.recoil = v[0].max(0.0),
        GRIP => gun.grip.position = at,
        SUPPORT => gun.support.position = at,
        REAR_SIGHT => gun.rear_sight = at,
        FRONT_SIGHT => gun.front_sight = at,
        EYE_RELIEF => gun.eye_relief = v[0].max(0.0),
        AIM_FOV => gun.aim_fov = v[0].clamp(0.0, 120.0),
        MUZZLE => gun.muzzle = at,
        _ => return None,
    }
    Some([0.0; 4])
}

fn holder_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if let Some(answer) = presence::<GunHolder>(world, entity, what, written) {
        return answer;
    }
    match (what, written) {
        (FIRE, Some(_)) => return super::holder::fire(world, entity).map(|_| [1.0, 0.0, 0.0, 0.0]),
        (REQUEST, Some(v)) => {
            let action = ACTIONS.get(v[0].max(0.0) as usize).copied()?;
            world.get_mut::<GunHolder>(entity)?.request = Some(action);
            return Some([0.0; 4]);
        }
        (CANCEL, Some(_)) => {
            world.get_mut::<GunHolderState>(entity)?.cancel();
            return Some([0.0; 4]);
        }
        (ACTING, None) => {
            let acting = world.get::<GunHolderState>(entity).and_then(GunHolderState::acting);
            return Some(number(acting.and_then(|a| ACTIONS.iter().position(|k| *k == a)).map_or(-1.0, |i| i as f32)));
        }
        _ => {}
    }
    let shot = world.get::<GunHolderState>(entity).and_then(|state| state.shot);
    let aimed = world.get::<GunHolderState>(entity).map_or(0.0, GunHolderState::aimed);
    let held = world.get::<GunHolder>(entity)?.gun;
    let held = held.or_else(|| crate::first_person::first_under(world, entity, |w, e| w.has::<Gun>(e)));
    let holder = world.get_mut::<GunHolder>(entity)?;
    let Some(v) = written else {
        return match what {
            GUN => Some(obj_value(held)),
            AIMING => Some(flag(holder.aiming)),
            AIMED => Some(number(aimed)),
            HIP => Some(point(holder.hip)),
            AIM_TIME => Some(number(holder.aim_time)),
            SWAY => Some(number(holder.sway)),
            BOB => Some(number(holder.bob)),
            BREATHING => Some(number(holder.breathing)),
            WALL_CHECK => Some(flag(holder.wall_check)),
            SPARE => Some(number(holder.spare as f32)),
            AUTO_CYCLE => Some(flag(holder.auto_cycle)),
            SHOT_ORIGIN => shot.map(|shot| point(shot.origin.into())),
            SHOT_DIRECTION => shot.map(|shot| point(shot.direction.into())),
            _ => None,
        };
    };
    match what {
        GUN => holder.gun = value_obj(v),
        AIMING => holder.aiming = v[0] != 0.0,
        HIP => holder.hip = [v[0], v[1], v[2]],
        AIM_TIME => holder.aim_time = v[0].max(0.02),
        SWAY => holder.sway = v[0].max(0.0),
        BOB => holder.bob = v[0].max(0.0),
        BREATHING => holder.breathing = v[0].max(0.0),
        WALL_CHECK => holder.wall_check = v[0] != 0.0,
        SPARE => holder.spare = v[0].max(0.0) as u32,
        AUTO_CYCLE => holder.auto_cycle = v[0] != 0.0,
        _ => return None,
    }
    Some([0.0; 4])
}
