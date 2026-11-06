//! Filling Smoke: each Smoke's grid is filled out from its Obj, nearest
//! first, to the cells round those already filled, as far as its budget, as
//! a smoke grenade's cloud fills a room and pours out through its door. A
//! cell is not filled where a collider that stays put stands in it, or
//! where one stands between it and the cell it was reached from, so the
//! smoke goes round walls, however thin. Bodies that move, as crates, and
//! triggers do not stop it. In the editor, where physics does not run, the
//! colliders are made for the fill.
//!
//! A Smoke starts as its Obj appears, or with Play On Start off, when a
//! script plays it (`SmokeCue`), as a grenade's fuse does. In play, a cloud
//! stays where it was filled however its Obj moves after, as a rolling
//! canister's does; in the editor it follows its Obj.

use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use voxear_core::math::{IVec3, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_physics::scene::{PhysicsScene, Rigidbody};
use voxear_render::{SMOKE_GRID, Smoke, SmokeCue, SmokeFill, SmokeHoles, smoke_origin};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// How many cells a tick fills in play, of all the Smokes being filled:
/// about a millisecond's worth, so a grenade going off does not stall a
/// frame. A whole Smoke takes a few ticks.
const CELLS_A_TICK: usize = 1500;

/// Fills every Smoke not yet filled, or filled for other settings or
/// another place, at `now`, the renderer's time. One filled before keeps
/// its start, so changing it does not set it spreading again. In play the
/// fills go on over a few ticks, the cloud showing once filled; in the
/// editor each is filled at once.
pub fn fill_smokes(world: &mut World, now: f64) {
    world.insert_resource(SmokeClock(now));
    for holes in world.query::<&mut SmokeHoles>() {
        holes.forget_filled(now as f32);
    }
    let playing = world.resource::<PhysicsScene>().is_some();
    let waiting = waiting(world);
    if waiting.is_empty() && !playing {
        return;
    }
    for (entity, ..) in &waiting {
        world.remove::<SmokeCue>(*entity);
    }
    // In play, the physics that runs; in the editor, colliders made for the fill.
    let made = (!playing).then(|| {
        let mut scene = PhysicsScene::new();
        scene.ready(world);
        scene
    });
    let Some(physics) = world.resource::<PhysicsScene>().or(made.as_ref()) else { return };
    let begun: Vec<(Entity, Option<SmokeFilling>)> = waiting
        .into_iter()
        .map(|(entity, smoke, middle, timing)| {
            let Some((started, ends)) = timing else { return (entity, None) };
            let fill = Filling::new(world, physics, &smoke, middle);
            (entity, Some(SmokeFilling { smoke, middle, started: started.unwrap_or(now as f32), ends, fill }))
        })
        .collect();
    let mut filling: Vec<(Entity, SmokeFilling)> = Vec::new();
    for (entity, begun) in begun {
        world.remove::<SmokeFilling>(entity);
        match begun {
            Some(begun) => filling.push((entity, begun)),
            None => drop(world.remove::<SmokeFill>(entity)),
        }
    }
    // Those begun on earlier ticks go on.
    let going: Vec<Entity> = world.query_ref::<(Entity, &SmokeFilling)>().into_iter().map(|(e, _)| e).collect();
    for entity in going {
        if let Some(going) = world.remove::<SmokeFilling>(entity) {
            filling.push((entity, going));
        }
    }
    let mut left = if playing { CELLS_A_TICK } else { usize::MAX };
    let Some(physics) = world.resource::<PhysicsScene>().or(made.as_ref()) else { return };
    let mut done = Vec::new();
    let mut still = Vec::new();
    for (entity, mut going) in filling {
        let before = going.fill.filled;
        let finished = left > 0 && going.fill.advance(world, physics, left);
        left = left.saturating_sub(going.fill.filled - before);
        if finished { done.push((entity, going)) } else { still.push((entity, going)) }
    }
    for (entity, going) in still {
        world.insert(entity, going);
    }
    for (entity, SmokeFilling { smoke, middle, started, ends, fill }) in done {
        if !world.has::<Smoke>(entity) {
            continue;
        }
        let origin = fill.origin;
        let (cells, steps) = fill.cells();
        let filled_for = (smoke, middle);
        world.insert(entity, SmokeFill { cells: Arc::from(cells), steps, origin, filled_for, started, ends });
    }
}

/// A Smoke being filled in play, over a few ticks: what for, its timing,
/// and how far the fill has got.
struct SmokeFilling {
    smoke: Smoke,
    middle: Vec3,
    started: f32,
    ends: Option<f32>,
    fill: Filling,
}

impl Component for SmokeFilling {}

/// When a Smoke filled again starts, if it keeps an earlier start, and when
/// it was told to disperse; `None` for one to take away.
type Timing = Option<(Option<f32>, Option<f32>)>;

/// The Smokes to fill, or to take away: each one's settings, middle, and
/// timing.
fn waiting(world: &World) -> Vec<(Entity, Smoke, Vec3, Timing)> {
    let playing = world.resource::<PhysicsScene>().is_some();
    let query = world.query_ref::<(
        Entity,
        &Smoke,
        &Transform,
        Option<&WorldTransform>,
        (Option<&SmokeFill>, Option<&SmokeFilling>),
        Option<&SmokeCue>,
    )>();
    query
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, smoke, transform, at, (fill, filling), cue)| {
            let here = at.map_or(transform.position, |w| w.0.w_axis.truncate());
            if let Some(cue) = cue {
                return Some((entity, *smoke, here, Some((Some(cue.at), None))));
            }
            // What it was last filled, or is being filled, for, and when it started and was told to disperse.
            let had = filling
                .map(|f| (f.smoke, f.middle, f.started, f.ends))
                .or(fill.map(|f| (f.filled_for.0, f.filled_for.1, f.started, f.ends)));
            let Some((was, was_at, started, ends)) = had else {
                return smoke.play_on_start.then_some((entity, *smoke, here, Some((None, None))));
            };
            // Filled as its Obj appeared, and since told not to start so: gone until played.
            if was.play_on_start && !smoke.play_on_start {
                return Some((entity, *smoke, here, None));
            }
            let middle = if playing { was_at } else { here };
            if (was, was_at) == (*smoke, middle) {
                return None;
            }
            // A change to when it spreads, lasts or disperses starts it again, to be seen; any other keeps its start.
            let timed = |s: &Smoke| (s.spread, s.lasts, s.fade);
            let started = (timed(&was) == timed(smoke)).then_some(started);
            Some((entity, *smoke, middle, Some((started, ends))))
        })
        .collect()
}

/// A Smoke's cells filled out from `middle`, nearest first, stepping to the
/// 26 cells round each, straight or across, so it spreads as a ball until
/// walls bend it: for each cell, 0 where it does not reach, or 1 more than
/// how far it is along the way it came, in half cells; and the farthest.
pub fn fill(world: &World, physics: &PhysicsScene, smoke: &Smoke, middle: Vec3) -> (Vec<u8>, u8) {
    let mut filling = Filling::new(world, physics, smoke, middle);
    filling.advance(world, physics, usize::MAX);
    filling.cells()
}

/// A fill under way (see `fill`): the cells filled so far, how far each
/// cell is, in half cells, along the shortest way found so far, whether
/// something stands in it once asked (0 not yet, 1 free, 2 blocked), and
/// the cells to go on from.
struct Filling {
    cells: Vec<u8>,
    far: Vec<u16>,
    blocked: Vec<u8>,
    queue: BinaryHeap<Reverse<(u16, [i32; 3])>>,
    filled: usize,
    budget: usize,
    farthest: u16,
    origin: Vec3,
    cell: f32,
}

/// Whether a collider that stays put, not a body physics moves, is `entity`'s.
fn stays(world: &World, entity: Entity) -> bool {
    world.get::<Rigidbody>(entity).is_none_or(|body| body.kinematic)
}

fn index(c: IVec3) -> usize {
    let n = SMOKE_GRID as i32;
    ((c.z * n + c.y) * n + c.x) as usize
}

impl Filling {
    fn new(world: &World, physics: &PhysicsScene, smoke: &Smoke, middle: Vec3) -> Self {
        let cell = smoke.cell.max(0.05);
        let size = SMOKE_GRID * SMOKE_GRID * SMOKE_GRID;
        let mut filling = Self {
            cells: vec![0; size],
            far: vec![u16::MAX; size],
            blocked: vec![0; size],
            queue: BinaryHeap::new(),
            filled: 0,
            budget: smoke.budget(),
            farthest: 0,
            origin: smoke_origin(middle, cell),
            cell,
        };
        // From the middle, or where it is set down in something, as a canister resting on the ground is, the
        // nearest free cell, above first.
        let n = SMOKE_GRID as i32;
        let mut near: Vec<IVec3> = (0..125).map(|k| IVec3::new(k % 5 - 2, (k / 5) % 5 - 2, k / 25 - 2)).collect();
        near.sort_by_key(|o| (o.length_squared(), -o.y));
        let start = near.into_iter().map(|o| IVec3::splat(n / 2) + o).find(|&c| !filling.solid(world, physics, c));
        if let Some(start) = start {
            filling.far[index(start)] = 0;
            filling.queue.push(Reverse((0, start.to_array())));
        }
        filling
    }

    fn centre(&self, c: IVec3) -> Vec3 {
        self.origin + (c.as_vec3() + 0.5) * self.cell
    }

    fn solid(&self, world: &World, physics: &PhysicsScene, c: IVec3) -> bool {
        let found = physics.overlap_sphere(self.centre(c), self.cell * 0.45, u32::MAX, false);
        found.into_iter().any(|entity| stays(world, entity))
    }

    /// Fills as many as `most` more cells: whether it is done.
    fn advance(&mut self, world: &World, physics: &PhysicsScene, most: usize) -> bool {
        let n = SMOKE_GRID as i32;
        let mut more = 0;
        while let Some(Reverse((distance, at))) = self.queue.pop() {
            let from = IVec3::from_array(at);
            if self.cells[index(from)] != 0 || distance > self.far[index(from)] {
                continue;
            }
            if self.filled >= self.budget {
                self.queue.clear();
                break;
            }
            if more >= most {
                self.queue.push(Reverse((distance, at)));
                return false;
            }
            self.cells[index(from)] = (distance.min(253) + 1) as u8;
            self.farthest = self.farthest.max(distance.min(253));
            self.filled += 1;
            more += 1;
            for way in AROUND {
                let to = from + way;
                if to.cmplt(IVec3::ZERO).any()
                    || to.cmpge(IVec3::splat(n)).any()
                    || self.cells[index(to)] != 0
                    || self.blocked[index(to)] == 2
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
                if next >= self.far[index(to)] {
                    continue;
                }
                if self.blocked[index(to)] == 0 {
                    self.blocked[index(to)] = if self.solid(world, physics, to) { 2 } else { 1 };
                }
                if self.blocked[index(to)] == 2 {
                    continue;
                }
                let (a, b) = (self.centre(from), self.centre(to));
                let wall = physics.raycast(a, b - a, a.distance(b), u32::MAX, false);
                if wall.is_some_and(|hit| stays(world, hit.obj)) {
                    continue;
                }
                self.far[index(to)] = next;
                self.queue.push(Reverse((next, to.to_array())));
            }
        }
        true
    }

    /// The cells filled and the farthest, 0 when none could be.
    fn cells(self) -> (Vec<u8>, u8) {
        let steps = if self.filled == 0 { 0 } else { self.farthest.max(1) as u8 };
        (self.cells, steps)
    }
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

/// Sets what answers `Smokes` and `Smoke` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_smoke_actions(act);
    voxear_scripting::set_smoke_values(values);
}

// What `values` reads or writes, as `NativeSmoke` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const PLAY: i32 = 3;
const DISPERSE: i32 = 4;
const PLAYING: i32 = 5;
const SIZE: i32 = 6;
const CELL: i32 = 7;
const DENSITY: i32 = 8;
const COLOR: i32 = 9;
const SPREAD: i32 = 10;
const LASTS: i32 = 11;
const FADE: i32 = 12;
const SEED: i32 = 13;
const PLAY_ON_START: i32 = 14;

/// Reads value `what` of `entity`'s Smoke, or with `written`, writes it:
/// `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let now = now(world);
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let flag = |on: bool| number(f32::from(u8::from(on)));
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<Smoke>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<Smoke>(entity) {
                world.insert(entity, Smoke::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<SmokeFill>(entity);
            world.remove::<SmokeCue>(entity);
            world.remove::<SmokeFilling>(entity);
            world.remove::<SmokeHoles>(entity);
            return world.remove::<Smoke>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    let smoke = *world.get::<Smoke>(entity)?;
    match (what, written) {
        // Over again from where its Obj is now.
        (PLAY, Some(_)) => drop(world.insert(entity, SmokeCue { at: now })),
        (DISPERSE, Some(_)) => {
            let fill = world.get_mut::<SmokeFill>(entity)?;
            fill.ends = fill.ends.or(Some(now));
        }
        (PLAYING, None) => {
            let fill = world.get::<SmokeFill>(entity).filter(|fill| fill.dispersed(&smoke, now) < 1.0);
            return Some(flag(fill.is_some() || world.has::<SmokeCue>(entity) || world.has::<SmokeFilling>(entity)));
        }
        (_, None) => {
            return Some(match what {
                SIZE => number(smoke.size),
                CELL => number(smoke.cell),
                DENSITY => number(smoke.density),
                COLOR => {
                    let [r, g, b] = crate::content::srgb(smoke.color);
                    [r, g, b, 1.0]
                }
                SPREAD => number(smoke.spread),
                LASTS => number(smoke.lasts),
                FADE => number(smoke.fade),
                SEED => number(smoke.seed as f32),
                PLAY_ON_START => flag(smoke.play_on_start),
                _ => return None,
            });
        }
        (_, Some([x, y, z, _])) => {
            let smoke = world.get_mut::<Smoke>(entity)?;
            match what {
                SIZE => smoke.size = x.clamp(0.1, 30.0),
                CELL => smoke.cell = x.clamp(0.1, 2.0),
                DENSITY => smoke.density = x.max(0.0),
                COLOR => smoke.color = crate::content::linear([x, y, z].map(|c| c.clamp(0.0, 1.0))),
                SPREAD => smoke.spread = x.max(0.0),
                LASTS => smoke.lasts = x.max(0.0),
                FADE => smoke.fade = x.max(0.0),
                SEED => smoke.seed = x.max(0.0) as u32,
                PLAY_ON_START => smoke.play_on_start = x != 0.0,
                _ => return None,
            }
        }
    }
    Some(written.unwrap_or_default())
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

    #[test]
    fn a_grenade_smokes_only_when_played_and_stays_where_it_started() {
        let mut world = World::new();
        let smoke = Smoke { size: 3.0, cell: 0.5, play_on_start: false, ..Smoke::default() };
        let grenade = world.spawn((smoke, Transform::from_position(Vec3::new(0.0, 0.05, 0.0))));
        // The ground it lies on, its middle in the ground's top.
        world.spawn((
            Collider { size: [20.0, 1.0, 20.0], ..Collider::default() },
            Transform::from_position(Vec3::new(0.0, -0.45, 0.0)),
        ));
        let mut physics = PhysicsScene::new();
        physics.ready(&world);
        world.insert_resource(physics);
        fill_smokes(&mut world, 1.0);
        assert!(world.get::<SmokeFill>(grenade).is_none(), "nothing until played");
        assert_eq!(values(&mut world, grenade, PLAYING, None), Some([0.0; 4]));
        values(&mut world, grenade, PLAY, Some([0.0; 4]));
        fill_smokes(&mut world, 2.0);
        let fill = world.get::<SmokeFill>(grenade).cloned().expect("played");
        assert!(fill.steps > 0, "from the free cell above the ground it lies in");
        assert_eq!(fill.started, 1.0, "from when it was played");
        // Rolled on, the cloud stays where it started.
        world.insert(grenade, Transform::from_position(Vec3::new(4.0, 0.05, 0.0)));
        fill_smokes(&mut world, 3.0);
        assert_eq!(world.get::<SmokeFill>(grenade).unwrap().origin, fill.origin);
        // Told to disperse, it is gone a disperse time later.
        values(&mut world, grenade, DISPERSE, Some([0.0; 4]));
        let fill = world.get::<SmokeFill>(grenade).unwrap().clone();
        assert_eq!(fill.ends, Some(3.0));
        assert!(fill.dispersed(&smoke, 4.5) > 0.4 && fill.dispersed(&smoke, 6.1) == 1.0);
        fill_smokes(&mut world, 6.5);
        assert_eq!(values(&mut world, grenade, PLAYING, None), Some([0.0; 4]), "dispersed");
        // Played again, it starts over where it is now.
        values(&mut world, grenade, PLAY, Some([0.0; 4]));
        fill_smokes(&mut world, 7.0);
        let again = world.get::<SmokeFill>(grenade).unwrap();
        assert_eq!((again.filled_for.1.x, again.started, again.ends), (4.0, 6.5, None));
    }

    #[test]
    fn turning_play_on_start_off_takes_the_cloud_away_until_played() {
        let mut world = World::new();
        let entity = world.spawn((Smoke { size: 2.0, cell: 0.5, ..Smoke::default() }, Transform::default()));
        fill_smokes(&mut world, 1.0);
        assert!(world.get::<SmokeFill>(entity).is_some());
        values(&mut world, entity, PLAY_ON_START, Some([0.0; 4]));
        fill_smokes(&mut world, 2.0);
        assert!(world.get::<SmokeFill>(entity).is_none());
    }

    #[test]
    fn a_full_smoke_fills_in_a_few_ticks_in_play() {
        let mut world = World::new();
        let smoke = Smoke::default();
        world.spawn((smoke, Transform::from_position(Vec3::new(0.0, 1.0, 0.0))));
        // A room six metres across with a door, round it.
        for (at, size) in [
            (Vec3::new(0.0, -0.1, 0.0), [6.0, 0.2, 6.0]),
            (Vec3::new(0.0, 3.1, 0.0), [6.0, 0.2, 6.0]),
            (Vec3::new(-3.0, 1.5, 0.0), [0.2, 3.0, 6.0]),
            (Vec3::new(0.0, 1.5, 3.0), [6.0, 3.0, 0.2]),
            (Vec3::new(0.0, 1.5, -3.0), [6.0, 3.0, 0.2]),
            (Vec3::new(3.0, 1.5, 1.8), [0.2, 3.0, 2.4]),
            (Vec3::new(3.0, 1.5, -1.8), [0.2, 3.0, 2.4]),
        ] {
            world.spawn((Collider { size, ..Collider::default() }, Transform::from_position(at)));
        }
        let mut physics = PhysicsScene::new();
        physics.ready(&world);
        let started = std::time::Instant::now();
        let (cells, _) = fill(&world, &physics, &smoke, Vec3::new(0.0, 1.0, 0.0));
        let took = started.elapsed();
        assert!(cells.iter().any(|&c| c != 0));
        eprintln!("filled {} cells in {took:?}", cells.iter().filter(|&&c| c != 0).count());
        // In play it is filled over a few ticks, the same in the end.
        world.insert_resource(physics);
        let mut ticks = 0;
        while world.query_ref::<&SmokeFill>().into_iter().next().is_none() {
            fill_smokes(&mut world, 1.0 + ticks as f64 / 60.0);
            ticks += 1;
            assert!(ticks < 10, "filled within a few ticks");
        }
        assert!(ticks >= 2, "over more than one tick: {ticks}");
        let sliced = world.query_ref::<&SmokeFill>().into_iter().next().unwrap().clone();
        assert_eq!(&sliced.cells[..], &cells[..]);
        assert_eq!(sliced.started, 1.0, "from when it began");
    }
}
