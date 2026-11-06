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
use voxear_render::{SMOKE_GRID, Smoke, SmokeFill, smoke_origin};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// Fills every Smoke not yet filled, or filled for other settings or
/// another place, at `now`, the renderer's time. One filled before keeps
/// its start, so changing it does not set it spreading again.
pub fn fill_smokes(world: &mut World, now: f64) {
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
        .map(|(entity, smoke, middle, fill)| (entity, smoke, middle, fill.map(|fill| fill.started)))
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
