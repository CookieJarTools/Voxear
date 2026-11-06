//! Mist trails: each Mist's trail of points, kept as its Obj moves. While
//! it is emitting, a point is left where the Obj is each time it has moved
//! far enough or a little time has passed, the newest following the Obj
//! between; points older than the Mist lingers are let go. For C#, `Mist`
//! in Voxear.Core reads and writes an Obj's Mist through one call, by
//! number, as `smoke` does a Smoke's.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{MAX_MIST_POINTS, MIST_RISE, Mist, MistTrail};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::content::{linear, srgb};

/// How long, in seconds, before a Mist standing still leaves another point,
/// so its trail does not age away under it.
const STILL: f32 = 0.1;

/// Moves every Mist's trail on to `now`, the renderer's time.
pub fn trail_mists(world: &mut World, now: f64) {
    let now = now as f32;
    world.insert_resource(MistClock(now));
    let mists: Vec<(Entity, Mist, Vec3, bool)> = world
        .query_ref::<(Entity, &Mist, &Transform, Option<&WorldTransform>, Option<&MistTrail>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, mist, transform, at, trail)| {
            (entity, *mist, at.map_or(transform.position, |w| w.0.w_axis.truncate()), trail.is_some())
        })
        .collect();
    for (entity, mist, here, has) in mists {
        if !has {
            world.insert(entity, MistTrail::default());
        }
        let Some(trail) = world.get_mut::<MistTrail>(entity) else { continue };
        trail.points.retain(|&(_, made)| now - made < mist.linger.max(1e-3));
        if !mist.emitting {
            continue;
        }
        // A new point once the Obj is half its width on from the one before the newest, or the newest has stood a
        // while; between, the newest follows it.
        let apart = (mist.width * 0.5).max(0.15);
        let count = trail.points.len();
        let before = count.checked_sub(2).map(|i| trail.points[i].0);
        match trail.points.last_mut() {
            Some((last, made)) if before.is_some_and(|b| b.distance(here) < apart) && now - *made < STILL => {
                *last = here;
            }
            _ => trail.points.push((here, now)),
        }
        if trail.points.len() > MAX_MIST_POINTS {
            trail.points.remove(0);
        }
    }
}

/// The engine's time, as trails are kept by.
struct MistClock(f32);

/// How thick the mists are along the line from `from` to `to`, as light
/// a metre takes added along it, without their swirls: for
/// `smoke::see_through`.
pub(crate) fn depth_through(world: &World, from: Vec3, to: Vec3) -> f32 {
    let now = world.resource::<MistClock>().map_or(0.0, |clock| clock.0);
    let length = from.distance(to);
    let mut depth = 0.0;
    for (mist, trail) in world.query_ref::<(&Mist, &MistTrail)>().without::<Destroying>() {
        if trail.points.is_empty() {
            continue;
        }
        let step = (mist.width * 0.25).max(0.05);
        let steps = (length / step).ceil().clamp(1.0, 2000.0);
        for i in 0..steps as usize {
            let at = from.lerp(to, (i as f32 + 0.5) / steps);
            depth += thickness(mist, trail, at, now) * length / steps;
        }
    }
    depth
}

/// How thick `mist` is at `at`: as `mistDensity` in `shaders/smoke.slang`,
/// without its swirls.
fn thickness(mist: &Mist, trail: &MistTrail, at: Vec3, now: f32) -> f32 {
    let linger = mist.linger.max(1e-3);
    let mut nearest = f32::MAX;
    let mut aged = 1.0;
    let rise = |made: f32| Vec3::Y * MIST_RISE * (now - made);
    for (i, &(a, made_a)) in trail.points.iter().enumerate() {
        let (b, made_b) = trail.points[(i + 1).min(trail.points.len() - 1)];
        let from = a + rise(made_a);
        let line = b + rise(made_b) - from;
        let along = ((at - from).dot(line) / line.length_squared().max(1e-6)).clamp(0.0, 1.0);
        let age = ((now - (made_a + (made_b - made_a) * along)) / linger).clamp(0.0, 1.0);
        let radius = mist.width.max(0.01) * 0.5 * (1.0 + age);
        let share = at.distance_squared(from + line * along) / (radius * radius);
        if share < nearest {
            (nearest, aged) = (share, age);
        }
    }
    if nearest >= 1.0 {
        return 0.0;
    }
    (1.0 - nearest) * (1.0 - aged).powf(1.5) * mist.density.max(0.0)
}

/// Sets what answers `Mist` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_mist_values(values);
}

// What `values` reads or writes, as `NativeMist` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const EMITTING: i32 = 3;
const WIDTH: i32 = 4;
const DENSITY: i32 = 5;
const COLOR: i32 = 6;
const LINGER: i32 = 7;
const SEED: i32 = 8;

/// Reads value `what` of `entity`'s Mist, or with `written`, writes it:
/// `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<Mist>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<Mist>(entity) {
                world.insert(entity, Mist::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<MistTrail>(entity);
            return world.remove::<Mist>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    let mist = world.get_mut::<Mist>(entity)?;
    match written {
        None => Some(match what {
            EMITTING => number(f32::from(u8::from(mist.emitting))),
            WIDTH => number(mist.width),
            DENSITY => number(mist.density),
            COLOR => {
                let [r, g, b] = srgb(mist.color);
                [r, g, b, 1.0]
            }
            LINGER => number(mist.linger),
            SEED => number(mist.seed as f32),
            _ => return None,
        }),
        Some([x, y, z, w]) => {
            match what {
                EMITTING => mist.emitting = x != 0.0,
                WIDTH => mist.width = x.clamp(0.05, 10.0),
                DENSITY => mist.density = x.max(0.0),
                COLOR => mist.color = linear([x, y, z].map(|c| c.clamp(0.0, 1.0))),
                LINGER => mist.linger = x.clamp(0.05, 30.0),
                SEED => mist.seed = x.max(0.0) as u32,
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trail_follows_its_obj_lingers_after_it_stops_and_ages_away() {
        let mut world = World::new();
        let wizard = world.spawn((Mist::default(), Transform::default()));
        for i in 0..=20 {
            world.insert(wizard, Transform::from_position(Vec3::new(i as f32 * 0.2, 1.0, 0.0)));
            trail_mists(&mut world, 1.0 + i as f64 / 60.0);
        }
        let trail = world.get::<MistTrail>(wizard).unwrap().clone();
        assert!(trail.points.len() >= 8, "a point each half width: {}", trail.points.len());
        assert_eq!(trail.points.last().unwrap().0, Vec3::new(4.0, 1.0, 0.0), "its newest where the Obj is");
        // Stopped, it stays as it was, then ages away.
        values(&mut world, wizard, EMITTING, Some([0.0; 4]));
        world.insert(wizard, Transform::from_position(Vec3::new(9.0, 1.0, 0.0)));
        trail_mists(&mut world, 1.5);
        assert_eq!(world.get::<MistTrail>(wizard).unwrap().points.len(), trail.points.len());
        // Hidden in it from across, and seen beside it.
        let across = (-depth_through(&world, Vec3::new(2.0, 1.0, -3.0), Vec3::new(2.0, 1.0, 3.0))).exp();
        let beside = (-depth_through(&world, Vec3::new(2.0, 4.0, -3.0), Vec3::new(2.0, 4.0, 3.0))).exp();
        assert!(across < 0.5 && beside == 1.0, "{across} {beside}");
        trail_mists(&mut world, 3.0);
        assert!(world.get::<MistTrail>(wizard).unwrap().points.is_empty(), "gone after it lingers");
    }
}
