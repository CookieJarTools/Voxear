//! Filling Smoke: each Smoke's grid is filled out from its Obj, nearest
//! first, to the cells round those already filled, as far as its budget, as
//! a smoke grenade's cloud fills a room and pours out through its door. A
//! cell is not filled where a collider that stays put stands in it, or
//! where one stands between it and the cell it was reached from, so the
//! smoke goes round walls, however thin. Bodies that move, as crates, and
//! triggers do not stop it. In the editor, where physics does not run, the
//! colliders are made for the fill.

use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use voxear_core::math::{IVec3, Vec3};
use voxear_ecs::{Entity, World};
use voxear_physics::scene::{PhysicsScene, Rigidbody};
use voxear_render::{SMOKE_GRID, Smoke, SmokeFill, SmokeHoles, smoke_origin};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// Fills every Smoke not yet filled, or filled for other settings or
/// another place, at `now`, the renderer's time. One filled before keeps
/// its start, so changing it does not set it spreading again.
pub fn fill_smokes(world: &mut World, now: f64) {
    world.insert_resource(SmokeClock(now));
    for holes in world.query::<&mut SmokeHoles>() {
        holes.forget_filled(now as f32);
    }
    let waiting = waiting(world);
    if waiting.is_empty() {
        return;
    }
    // In play, the physics that runs; in the editor, colliders made for the fill.
    let made;
    let physics = match world.resource::<PhysicsScene>() {
        Some(physics) => physics,
        None => {
            let mut scene = PhysicsScene::new();
            scene.ready(world);
            made = scene;
            &made
        }
    };
    let filled: Vec<(Entity, SmokeFill)> = waiting
        .into_iter()
        .map(|(entity, smoke, middle, started)| {
            let (cells, steps) = fill(world, physics, &smoke, middle);
            let origin = smoke_origin(middle, smoke.cell.max(0.05));
            let started = started.unwrap_or(now as f32);
            (entity, SmokeFill { cells: Arc::from(cells), steps, origin, filled_for: (smoke, middle), started })
        })
        .collect();
    for (entity, fill) in filled {
        world.insert(entity, fill);
    }
}

/// The Smokes to fill: each one's settings, middle, and start if it was
/// filled before.
fn waiting(world: &World) -> Vec<(Entity, Smoke, Vec3, Option<f32>)> {
    let query = world.query_ref::<(Entity, &Smoke, &Transform, Option<&WorldTransform>, Option<&SmokeFill>)>();
    query
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, smoke, transform, at, fill)| {
            let middle = at.map_or(transform.position, |w| w.0.w_axis.truncate());
            (entity, *smoke, middle, fill)
        })
        .filter(|(_, smoke, middle, fill)| fill.is_none_or(|fill| fill.filled_for != (*smoke, *middle)))
        // A change to when it spreads, lasts or disperses starts it again, to be seen; any other keeps its start.
        .map(|(entity, smoke, middle, fill)| {
            let was = fill.map(|fill| fill.filled_for.0);
            let timed = |s: &Smoke| (s.spread, s.lasts, s.fade);
            let started = fill.filter(|_| was.is_some_and(|was| timed(&was) == timed(&smoke))).map(|fill| fill.started);
            (entity, smoke, middle, started)
        })
        .collect()
}

/// A Smoke's cells filled out from `middle`, nearest first, stepping to the
/// 26 cells round each, straight or across, so it spreads as a ball until
/// walls bend it: for each cell, 0 where it does not reach, or 1 more than
/// how far it is along the way it came, in half cells; and the farthest.
pub fn fill(world: &World, physics: &PhysicsScene, smoke: &Smoke, middle: Vec3) -> (Vec<u8>, u8) {
    let n = SMOKE_GRID as i32;
    let cell = smoke.cell.max(0.05);
    let origin = smoke_origin(middle, cell);
    let centre = |c: IVec3| origin + (c.as_vec3() + 0.5) * cell;
    let index = |c: IVec3| ((c.z * n + c.y) * n + c.x) as usize;
    let stays = |entity: Entity| world.get::<Rigidbody>(entity).is_none_or(|body| body.kinematic);
    let solid = |at: Vec3| physics.overlap_sphere(at, cell * 0.45, u32::MAX, false).into_iter().any(stays);
    let walled = |from: Vec3, to: Vec3| {
        physics.raycast(from, to - from, from.distance(to), u32::MAX, false).is_some_and(|hit| stays(hit.obj))
    };
    let mut cells = vec![0u8; SMOKE_GRID * SMOKE_GRID * SMOKE_GRID];
    let start = IVec3::splat(n / 2);
    if solid(centre(start)) {
        return (cells, 0);
    }
    // How far each cell is, in half cells, along the shortest way found so far; and whether it is filled yet.
    let mut far = vec![u16::MAX; cells.len()];
    let mut blocked = vec![false; cells.len()];
    let mut queue = BinaryHeap::from([Reverse((0u16, start.to_array()))]);
    far[index(start)] = 0;
    let budget = smoke.budget();
    let mut filled = 0;
    let mut farthest = 0u16;
    while let Some(Reverse((distance, at))) = queue.pop() {
        let from = IVec3::from_array(at);
        if cells[index(from)] != 0 || distance > far[index(from)] {
            continue;
        }
        if filled >= budget {
            break;
        }
        cells[index(from)] = (distance.min(253) + 1) as u8;
        farthest = farthest.max(distance.min(253));
        filled += 1;
        for way in AROUND {
            let to = from + way;
            if to.cmplt(IVec3::ZERO).any()
                || to.cmpge(IVec3::splat(n)).any()
                || cells[index(to)] != 0
                || blocked[index(to)]
            {
                continue;
            }
            // Half cells to the next: 2 straight, 3 across a side, 3 or 4 across a corner.
            let step = match way.abs().element_sum() {
                1 => 2,
                2 => 3,
                _ => 4,
            };
            let next = distance + step;
            if next >= far[index(to)] {
                continue;
            }
            if solid(centre(to)) {
                blocked[index(to)] = true;
                continue;
            }
            if walled(centre(from), centre(to)) {
                continue;
            }
            far[index(to)] = next;
            queue.push(Reverse((next, to.to_array())));
        }
    }
    (cells, farthest.max(1) as u8)
}

/// The 26 cells round a cell.
const AROUND: [IVec3; 26] = {
    let mut ways = [IVec3::ZERO; 26];
    let mut i = 0;
    let mut k = 0;
    while k < 27 {
        let way = IVec3::new(k % 3 - 1, (k / 3) % 3 - 1, k / 9 - 1);
        if k != 13 {
            ways[i] = way;
            i += 1;
        }
        k += 1;
    }
    ways
};

/// Sets what answers `Smokes` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_smoke_actions(act);
}

/// A script's shot (0: from, to), blast (1: centre, radius) or asking
/// through smoke (2: from, to), which gives back how much shows.
fn act(world: &mut World, what: i32, v: [f32; 7]) -> f32 {
    let (a, b) = (Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5]));
    match what {
        0 => f32::from(u8::from(shoot(world, a, b))),
        1 => {
            blast(world, a, v[3]);
            1.0
        }
        _ => see_through(world, a, b),
    }
}

/// The engine's time, as the last tick had it, which shots and blasts are
/// dated by.
struct SmokeClock(f64);

fn now(world: &World) -> f32 {
    world.resource::<SmokeClock>().map_or(0.0, |clock| clock.0 as f32)
}

/// Each filled smoke's grid, as its first corner and far corner, and its
/// cell size.
fn grids(world: &World) -> Vec<(Entity, Vec3, Vec3, f32)> {
    let query = world.query_ref::<(Entity, &SmokeFill)>();
    query
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, fill)| {
            let cell = fill.filled_for.0.cell.max(0.05);
            (entity, fill.origin, fill.origin + Vec3::splat(cell * SMOKE_GRID as f32), cell)
        })
        .collect()
}

/// How far along the line from `from` to `to`, from 0 to 1, it is inside the
/// box from `low` to `high`.
fn inside(from: Vec3, to: Vec3, low: Vec3, high: Vec3) -> Option<(f32, f32)> {
    let way = to - from;
    let a = (low - from) / way;
    let b = (high - from) / way;
    let enter = a.min(b).max_element().max(0.0);
    let leave = a.max(b).min_element().min(1.0);
    (enter < leave).then_some((enter, leave))
}

/// A shot from `from` to `to`, as a bullet's: a hole along it through every
/// smoke it passes through, filling back in over a second or so. Whether it
/// passed through any.
pub fn shoot(world: &mut World, from: Vec3, to: Vec3) -> bool {
    let time = now(world);
    let through: Vec<(Entity, f32, f32)> = grids(world)
        .into_iter()
        .filter_map(|(entity, low, high, _)| Some((entity, inside(from, to, low, high)?)))
        .map(|(e, (a, b))| (e, a, b))
        .collect();
    for &(entity, enter, leave) in &through {
        let (a, b) = (from.lerp(to, enter), from.lerp(to, leave));
        let mut holes = world.get::<SmokeHoles>(entity).cloned().unwrap_or_default();
        holes.shoot(a, b, time);
        world.insert(entity, holes);
    }
    !through.is_empty()
}

/// A blast at `centre`, as a grenade's: clearing every smoke within `radius`
/// metres of it, which fills back in over a few seconds.
pub fn blast(world: &mut World, centre: Vec3, radius: f32) {
    let time = now(world);
    let near: Vec<Entity> = grids(world)
        .into_iter()
        .filter(|(_, low, high, _)| centre.clamp(*low, *high).distance(centre) <= radius)
        .map(|(entity, ..)| entity)
        .collect();
    for entity in near {
        let mut holes = world.get::<SmokeHoles>(entity).cloned().unwrap_or_default();
        holes.blast(centre, radius, time);
        world.insert(entity, holes);
    }
}

/// How much of what is at `to` shows from `from` through the smoke between:
/// 1 for none in the way, near 0 hidden. As thick as the smoke is drawn,
/// though without its swirls: as far as it has spread, as it thins, with
/// its holes.
pub fn see_through(world: &World, from: Vec3, to: Vec3) -> f32 {
    let time = now(world);
    let mut depth = 0.0;
    for (entity, low, high, cell) in grids(world) {
        let Some((enter, leave)) = inside(from, to, low, high) else { continue };
        let (Some(smoke), Some(fill)) = (world.get::<Smoke>(entity), world.get::<SmokeFill>(entity)) else { continue };
        let holes = world.get::<SmokeHoles>(entity);
        let length = from.distance(to) * (leave - enter);
        let steps = (length / (cell * 0.5)).ceil().max(1.0);
        let step = length / steps;
        let age = time - fill.started;
        let grown = (age / smoke.spread.max(1e-3)).clamp(0.0, 1.0);
        let front = grown * (f32::from(fill.steps) + 2.0);
        for i in 0..steps as usize {
            let at = from.lerp(to, enter + (leave - enter) * (i as f32 + 0.5) / steps);
            let c = ((at - low) / cell).floor().as_ivec3().clamp(IVec3::ZERO, IVec3::splat(SMOKE_GRID as i32 - 1));
            let n = SMOKE_GRID as i32;
            let reached = fill.cells[((c.z * n + c.y) * n + c.x) as usize];
            if reached == 0 {
                continue;
            }
            let filled = ((front - f32::from(reached - 1)) * 0.5).clamp(0.0, 1.0);
            let left = holes.map_or(1.0, |holes| holes.left(at, time));
            depth += filled * left * smoke.density * smoke.thickness(age) * step;
        }
    }
    (-depth).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_physics::scene::Collider;

    fn spread(world: &mut World) -> SmokeFill {
        fill_smokes(world, 1.0);
        world.query_ref::<&SmokeFill>().into_iter().next().cloned().expect("filled")
    }

    #[test]
    fn smoke_fills_its_budget_round_a_wall_but_not_through_it() {
        let mut world = World::new();
        let smoke = Smoke { size: 6.0, cell: 0.5, ..Smoke::default() };
        world.spawn((smoke, Transform::from_position(Vec3::new(0.0, 1.0, 0.0))));
        // A wall a metre and a half along +X, four metres wide and high, thinner than a cell.
        let wall = Collider { size: [0.1, 4.0, 4.0], ..Collider::default() };
        world.spawn((wall, Transform::from_position(Vec3::new(1.5, 1.0, 0.0))));
        let fill = spread(&mut world);
        let count = fill.cells.iter().filter(|&&c| c != 0).count();
        assert_eq!(count, smoke.budget(), "as much as its budget");
        let cell_at = |at: Vec3| {
            let c = ((at - fill.origin) / 0.5).floor().as_ivec3();
            fill.cells[((c.z * 32 + c.y) * 32 + c.x) as usize]
        };
        assert!(cell_at(Vec3::new(1.0, 1.0, 0.0)) != 0, "up to the wall");
        let past = cell_at(Vec3::new(2.0, 1.0, 0.0));
        let round = cell_at(Vec3::new(1.0, 1.0, 0.0));
        assert!(past == 0 || past > round + 6, "behind it only the long way round: {past} against {round}");
        assert_eq!(fill.started, 1.0);
        // Moved, it fills again where it is now, keeping its start.
        let entity = world.query_ref::<(Entity, &Smoke)>().into_iter().next().unwrap().0;
        world.insert(entity, Transform::from_position(Vec3::new(-3.0, 1.0, 0.0)));
        fill_smokes(&mut world, 5.0);
        let moved = world.get::<SmokeFill>(entity).unwrap();
        assert_eq!((moved.filled_for.1.x, moved.started), (-3.0, 1.0));
    }

    #[test]
    fn in_the_open_smoke_spreads_as_a_ball() {
        let mut world = World::new();
        world.spawn((Smoke { size: 8.0, cell: 0.5, ..Smoke::default() }, Transform::from_position(Vec3::ZERO)));
        let fill = spread(&mut world);
        let steps_at =
            |offset: IVec3| fill.cells[((16 + offset.z) * 32 + 16 + offset.y) as usize * 32 + (16 + offset.x) as usize];
        // Five cells out along an axis, and as far out across a corner.
        let (straight, across) = (f32::from(steps_at(IVec3::new(5, 0, 0))), f32::from(steps_at(IVec3::new(3, 3, 3))));
        assert!(
            straight > 0.0 && (across - straight).abs() / straight < 0.25,
            "as far out, as far along: {straight} and {across}"
        );
    }

    #[test]
    fn a_shot_through_smoke_lets_the_eye_through_for_a_second_and_a_blast_for_longer() {
        let mut world = World::new();
        world.spawn((
            Smoke { size: 6.0, cell: 0.5, spread: 0.1, ..Smoke::default() },
            Transform::from_position(Vec3::ZERO),
        ));
        fill_smokes(&mut world, 0.0);
        fill_smokes(&mut world, 2.0);
        let (eye, target) = (Vec3::new(-10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0));
        assert!(see_through(&world, eye, target) < 0.01, "hidden by the cloud");
        assert!(see_through(&world, Vec3::new(-10.0, 9.0, 0.0), Vec3::new(10.0, 9.0, 0.0)) == 1.0, "clear above it");
        assert!(shoot(&mut world, eye, target), "through the smoke");
        fill_smokes(&mut world, 2.2);
        assert!(see_through(&world, eye, target) > 0.9, "seen along the shot's tunnel");
        assert!(see_through(&world, eye + Vec3::Y, target + Vec3::Y) < 0.01, "but not a metre above it");
        fill_smokes(&mut world, 4.0);
        assert!(see_through(&world, eye, target) < 0.01, "filled back in");
        blast(&mut world, Vec3::ZERO, 2.5);
        fill_smokes(&mut world, 4.5);
        assert!(
            see_through(&world, Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)) > 0.7,
            "mostly cleared by the blast"
        );
        fill_smokes(&mut world, 12.0);
        assert!(see_through(&world, eye, target) < 0.01, "filled back in, slowly");
    }

    #[test]
    fn a_crate_does_not_stop_smoke_and_none_fills_inside_a_wall() {
        let mut world = World::new();
        let smoke = Smoke { size: 2.0, cell: 0.5, ..Smoke::default() };
        world.spawn((smoke, Transform::from_position(Vec3::ZERO)));
        world.spawn((Collider::default(), Rigidbody::default(), Transform::from_position(Vec3::ZERO)));
        assert!(spread(&mut world).steps > 0, "a crate moved by physics is no wall");
        let mut buried = World::new();
        buried.spawn((smoke, Transform::from_position(Vec3::ZERO)));
        buried.spawn((Collider { size: [4.0; 3], ..Collider::default() }, Transform::from_position(Vec3::ZERO)));
        assert!(spread(&mut buried).cells.iter().all(|&c| c == 0), "none inside a wall");
    }
}
