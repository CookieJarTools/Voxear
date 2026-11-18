//! Nav Obstacle: a box what finds its way goes round, though it was not
//! there when the nav was made: a crate pushed into a doorway, a cart left
//! in a street, a gate swung shut.
//!
//! The easy way is the component: put one on an Obj and agents keep off
//! it. The full way is how. Standing still, an obstacle carves: it is cut
//! out of the nav, so ways are found round it, or stop short where it
//! shuts the only one. Moving, it is gone round instead, each agent near
//! it turning aside as for another agent, until it has stood still for
//! `still_time` and carves where it stopped. One with `carve` off is only
//! ever gone round, for something always on the move.
//!
//! Carving costs little. In a world of blocks the places under a carved
//! obstacle are Not Walkable to every search from then on. On a level's
//! nav mesh the polygons under it are cut on navigation's threads, from
//! the mesh as baked each time (see `voxear_nav::mesh::NavMesh::cut`), and
//! the cut mesh is handed over whole. Agents whose ways cross what changed
//! find theirs again.

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_nav::{AreaBox, NOT_WALKABLE, Walker, mesh::NavMesh, threads::Job};
use voxear_scene::{Destroying, DimensionId, Transform, WorldTransform, dimension_of};

use crate::navigation::LevelNav;

/// A box agents go round: centred on its Obj, as wide, high and long as
/// `size` in the Obj's own space, so it turns and grows with it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavObstacle {
    pub size: [f32; 3],
    /// Whether it is cut out of the nav where it stands still; otherwise
    /// it is only ever gone round.
    pub carve: bool,
    /// How far it moves, in metres, before it counts as moving and its cut
    /// is let go.
    pub move_threshold: f32,
    /// How long it stands still, in seconds, before it carves.
    pub still_time: f32,
}

impl Component for NavObstacle {}

impl Default for NavObstacle {
    fn default() -> Self {
        Self { size: [1.0, 1.0, 1.0], carve: true, move_threshold: 0.1, still_time: 0.5 }
    }
}

/// What the engine keeps of an obstacle: where its box was when it last
/// stood still, how long it has, the box it carves, and how fast it goes.
#[derive(Clone, Copy, Debug)]
struct Tracked {
    bounds: [Vec3; 2],
    still: f32,
    carved: Option<[Vec3; 2]>,
    middle: Vec3,
    velocity: Vec3,
    dimension: DimensionId,
}

/// Every obstacle's cut, and the level's nav mesh cut round them.
#[derive(Default)]
pub struct NavCuts {
    tracked: HashMap<Entity, Tracked>,
    /// A number that changes each time a box is carved or let go, the one
    /// before, and the boxes that changed between them.
    version: u64,
    previous: u64,
    changed: Vec<(DimensionId, [Vec3; 2])>,
    mesh: MeshCuts,
}

/// The level's nav mesh as baked, the cut one last put in its place, the
/// boxes that is cut round, and a cut under way.
#[derive(Default)]
struct MeshCuts {
    baked: Option<Arc<NavMesh>>,
    made: Weak<NavMesh>,
    cut_round: Vec<[Vec3; 2]>,
    cutting: Option<(Arc<Job<NavMesh>>, Vec<[Vec3; 2]>, u32)>,
}

/// What changed in the carved boxes of one dimension, for an agent to
/// tell whether its way is still good.
#[derive(Clone, Debug, Default)]
pub(crate) struct CutChanges {
    pub version: u64,
    pub previous: u64,
    pub changed: Vec<[Vec3; 2]>,
}

impl CutChanges {
    /// Whether a way found as of `seen` along `corners`, for a walker
    /// `radius` wide and `height` tall, may cross a box carved or let go
    /// since.
    pub fn crosses(&self, seen: u64, corners: &[Vec3], radius: f32, height: f32) -> bool {
        if seen == self.version {
            return false;
        }
        if seen != self.previous {
            return true;
        }
        self.changed.iter().any(|[low, high]| {
            let (low, high) = (*low - Vec3::new(radius, height, radius), *high + Vec3::splat(radius));
            corners.windows(2).any(|pair| segment_meets(pair[0], pair[1], low, high))
        })
    }
}

/// Whether the straight line from `a` to `b` passes through the box from
/// `low` to `high`.
fn segment_meets(a: Vec3, b: Vec3, low: Vec3, high: Vec3) -> bool {
    let (mut enter, mut leave) = (0.0f32, 1.0f32);
    let along = b - a;
    for axis in 0..3 {
        if along[axis].abs() < 1e-6 {
            if a[axis] < low[axis] || a[axis] > high[axis] {
                return false;
            }
            continue;
        }
        let (t0, t1) = ((low[axis] - a[axis]) / along[axis], (high[axis] - a[axis]) / along[axis]);
        (enter, leave) = (enter.max(t0.min(t1)), leave.min(t0.max(t1)));
        if enter > leave {
            return false;
        }
    }
    true
}

/// An obstacle's box in the world, square to the world: the box round its
/// turned one.
fn bounds_of(world: &World, entity: Entity, obstacle: &NavObstacle) -> Option<[Vec3; 2]> {
    let placed = match world.get::<WorldTransform>(entity) {
        Some(placed) => placed.0,
        None => world.get::<Transform>(entity)?.matrix(),
    };
    let half = Vec3::from_array(obstacle.size).abs() * 0.5;
    let middle = placed.w_axis.truncate();
    let reach = placed.x_axis.truncate().abs() * half.x
        + placed.y_axis.truncate().abs() * half.y
        + placed.z_axis.truncate().abs() * half.z;
    let bounds = [middle - reach, middle + reach];
    (bounds[0].is_finite() && bounds[1].is_finite()).then_some(bounds)
}

/// Follows every obstacle `delta` seconds on: what stands still long
/// enough is carved, what moves is let go, and the level's nav mesh is cut
/// round the boxes carved. In the fixed step, before agents.
pub fn update(world: &mut World, delta: f32) {
    let obstacles: Vec<(Entity, NavObstacle)> = world
        .query_ref::<(Entity, &NavObstacle)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, obstacle)| (entity, *obstacle))
        .collect();
    if obstacles.is_empty() && world.resource::<NavCuts>().is_none_or(|cuts| cuts.tracked.is_empty()) {
        if let Some(mut cuts) = world.remove_resource::<NavCuts>() {
            cut_mesh(world, &mut cuts, false);
            world.insert_resource(cuts);
        }
        return;
    }
    voxear_core::profile_scope!("Nav Obstacles");
    let mut cuts = world.remove_resource::<NavCuts>().unwrap_or_default();
    let mut changed: Vec<(DimensionId, [Vec3; 2])> = Vec::new();
    let mut seen = std::collections::HashSet::with_capacity(obstacles.len());
    for (entity, obstacle) in &obstacles {
        let Some(bounds) = bounds_of(world, *entity, obstacle) else { continue };
        seen.insert(*entity);
        let dimension = dimension_of(world, *entity);
        let middle = (bounds[0] + bounds[1]) * 0.5;
        // One first seen stands as still as it has ever stood: it carves at once.
        let tracked = cuts.tracked.entry(*entity).or_insert(Tracked {
            bounds,
            still: obstacle.still_time,
            carved: None,
            middle,
            velocity: Vec3::ZERO,
            dimension,
        });
        if delta > 0.0 {
            tracked.velocity = (middle - tracked.middle) / delta;
        }
        tracked.middle = middle;
        let moved = (0..2).any(|end| (bounds[end] - tracked.bounds[end]).abs().max_element() > obstacle.move_threshold);
        if moved || tracked.dimension != dimension {
            (tracked.bounds, tracked.still, tracked.dimension) = (bounds, 0.0, dimension);
        } else {
            tracked.still += delta;
        }
        let wanted = (obstacle.carve && tracked.still >= obstacle.still_time).then_some(bounds);
        if wanted != tracked.carved {
            changed.extend(tracked.carved.map(|old| (tracked.dimension, old)));
            changed.extend(wanted.map(|new| (dimension, new)));
            tracked.carved = wanted;
        }
    }
    cuts.tracked.retain(|entity, tracked| {
        let kept = seen.contains(entity);
        if !kept && let Some(old) = tracked.carved {
            changed.push((tracked.dimension, old));
        }
        kept
    });
    if !changed.is_empty() {
        cuts.previous = cuts.version;
        cuts.version += 1;
        cuts.changed = changed;
    }
    cut_mesh(world, &mut cuts, false);
    world.insert_resource(cuts);
}

/// Keeps the level's nav mesh cut round the boxes carved in the dimension
/// shown: a cut begun on navigation's threads where they changed, and put
/// in place once it is done, or at once where `wait`.
fn cut_mesh(world: &mut World, cuts: &mut NavCuts, wait: bool) {
    let Some(live) = world.resource::<LevelNav>().map(|nav| nav.0.clone()) else {
        cuts.mesh = MeshCuts::default();
        return;
    };
    // A mesh put there by something else is a mesh as baked: a level opened, or one baked again. The mesh as baked
    // is still there while its first cut is made.
    let ours = std::ptr::eq(Arc::as_ptr(&live), cuts.mesh.made.as_ptr())
        || cuts.mesh.baked.as_ref().is_some_and(|baked| Arc::ptr_eq(baked, &live));
    if !ours {
        cuts.mesh = MeshCuts { baked: Some(live), ..MeshCuts::default() };
    } else if !std::ptr::eq(Arc::as_ptr(&live), cuts.mesh.made.as_ptr()) {
        // The mesh as baked is back where a cut was put: nothing is cut round now.
        cuts.mesh.cut_round.clear();
    }
    let Some(baked) = cuts.mesh.baked.clone() else { return };
    let offset = voxear_scene::WorldOrigin::of(world).0.as_vec3();
    let here = crate::dimensions::current(world);
    let mut boxes: Vec<[Vec3; 2]> = cuts
        .tracked
        .values()
        .filter(|tracked| tracked.dimension == here)
        .filter_map(|tracked| tracked.carved)
        .map(|[low, high]| [low + offset, high + offset])
        .collect();
    boxes.sort_by(|a, b| a[0].to_array().partial_cmp(&b[0].to_array()).unwrap_or(std::cmp::Ordering::Equal));
    let asked = cuts.mesh.cutting.as_ref().map_or(&cuts.mesh.cut_round, |(_, boxes, _)| boxes);
    if *asked != boxes {
        let mesh = baked.clone();
        let round = boxes.clone();
        // With none left, the mesh as baked goes back: nothing to cut.
        let job = Job::start(move || if round.is_empty() { NavMesh::default() } else { mesh.cut(&round) });
        cuts.mesh.cutting = Some((job, boxes, 0));
    }
    // The cut taken once a thread has made it, or made here where none has begun it by the step after.
    let Some((job, round, waited)) = cuts.mesh.cutting.take() else { return };
    if !wait && !job.is_done() && (job.is_begun() || waited == 0) {
        cuts.mesh.cutting = Some((job, round, waited + 1));
        return;
    }
    let Some(cut) = job.take() else { return };
    let cut = if round.is_empty() { baked } else { Arc::new(cut) };
    cuts.mesh.made = Arc::downgrade(&cut);
    cuts.mesh.cut_round = round;
    world.insert_resource(LevelNav(cut));
}

/// Waits for the level's nav mesh to be cut round every obstacle carved:
/// in tests, and where play must see them at once.
pub fn settle(world: &mut World) {
    let Some(mut cuts) = world.remove_resource::<NavCuts>() else { return };
    cut_mesh(world, &mut cuts, true);
    world.insert_resource(cuts);
}

/// What changed in the boxes carved in `dimension`.
pub(crate) fn changes(world: &World, dimension: DimensionId) -> CutChanges {
    let Some(cuts) = world.resource::<NavCuts>() else { return CutChanges::default() };
    CutChanges {
        version: cuts.version,
        previous: cuts.previous,
        changed: cuts.changed.iter().filter(|(of, _)| *of == dimension).map(|(_, bounds)| *bounds).collect(),
    }
}

/// Every box carved in `dimension`, in blocks from the corner of a world
/// of blocks at `corner` whose blocks are `size` metres a side: widened by
/// half a block, so a place counts as under one where any of its column
/// is, and Not Walkable. Each is lowered by a walker's height before a
/// search (see [`for_walker`]).
pub(crate) fn block_boxes(world: &World, corner: Vec3, size: f32, dimension: DimensionId) -> Vec<AreaBox> {
    let Some(cuts) = world.resource::<NavCuts>() else { return Vec::new() };
    let size = size.max(1e-3);
    cuts.tracked
        .values()
        .filter(|tracked| tracked.dimension == dimension)
        .filter_map(|tracked| tracked.carved)
        .map(|[low, high]| {
            let (low, high) = ((low - corner) / size, (high - corner) / size);
            let wide = Vec3::new(0.499, 0.0, 0.499);
            AreaBox { min: (low - wide).to_array(), max: (high + wide).to_array(), area: NOT_WALKABLE }
        })
        .collect()
}

/// `boxes` as a search for `walker` takes them: each reaching down as far
/// as the walker is tall, as a place is under a box where the walker's body
/// there would be in it.
pub(crate) fn for_walker(boxes: &[AreaBox], walker: Walker) -> impl Iterator<Item = AreaBox> + '_ {
    let tall = f32::from(walker.height) / 16.0;
    boxes.iter().map(move |area_box| {
        let mut lowered = *area_box;
        lowered.min[1] -= tall - 0.01;
        lowered.max[1] -= 0.01;
        lowered
    })
}

/// Every obstacle not carved in `dimension`, moving or never carving, as
/// a mover agents go round: its middle, how fast it goes, and how wide it
/// is round its middle on the ground.
pub(crate) fn movers(world: &World) -> Vec<(DimensionId, Vec3, Vec3, f32)> {
    let Some(cuts) = world.resource::<NavCuts>() else { return Vec::new() };
    cuts.tracked
        .values()
        .filter(|tracked| tracked.carved.is_none())
        .map(|tracked| {
            let half = (tracked.bounds[1] - tracked.bounds[0]) * 0.5;
            (tracked.dimension, tracked.middle, tracked.velocity, half.x.hypot(half.z))
        })
        .collect()
}

/// How many obstacles there are, and how many of them carve now.
pub fn counts(world: &World) -> (usize, usize) {
    world.resource::<NavCuts>().map_or((0, 0), |cuts| {
        (cuts.tracked.len(), cuts.tracked.values().filter(|tracked| tracked.carved.is_some()).count())
    })
}

/// Whether obstacle `entity` carves now.
pub fn is_carving(world: &World, entity: Entity) -> bool {
    world
        .resource::<NavCuts>()
        .and_then(|cuts| cuts.tracked.get(&entity))
        .is_some_and(|tracked| tracked.carved.is_some())
}

/// Sets what answers `NavObstacle` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::NavObstacle, values);
}

// What `values` reads or writes, as `NavObstacle` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const SIZE: i32 = 3;
const CARVE: i32 = 4;
const MOVE_THRESHOLD: i32 = 5;
const STILL_TIME: i32 = 6;
const CARVING: i32 = 7;

/// Reads value `what` of `entity`'s Nav Obstacle, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let toggle = |on: bool| number(f32::from(u8::from(on)));
    match (what, written) {
        (HAS, None) => return Some(toggle(world.has::<NavObstacle>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<NavObstacle>(entity) {
                world.insert(entity, NavObstacle::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<NavObstacle>(entity).map(|_| [0.0; 4]),
        (CARVING, None) if world.has::<NavObstacle>(entity) => return Some(toggle(is_carving(world, entity))),
        _ => {}
    }
    let obstacle = world.get_mut::<NavObstacle>(entity)?;
    match (what, written) {
        (SIZE, None) => Some([obstacle.size[0], obstacle.size[1], obstacle.size[2], 0.0]),
        (CARVE, None) => Some(toggle(obstacle.carve)),
        (MOVE_THRESHOLD, None) => Some(number(obstacle.move_threshold)),
        (STILL_TIME, None) => Some(number(obstacle.still_time)),
        (SIZE, Some([x, y, z, _])) => {
            obstacle.size = [x, y, z].map(|n| if n.is_finite() { n.clamp(0.0, 10_000.0) } else { 1.0 });
            Some([0.0; 4])
        }
        (CARVE, Some([on, ..])) => {
            obstacle.carve = on != 0.0;
            Some([0.0; 4])
        }
        (MOVE_THRESHOLD, Some([far, ..])) if far.is_finite() => {
            obstacle.move_threshold = far.clamp(0.0, 100.0);
            Some([0.0; 4])
        }
        (STILL_TIME, Some([time, ..])) if time.is_finite() => {
            obstacle.still_time = time.clamp(0.0, 60.0);
            Some([0.0; 4])
        }
        _ => None,
    }
}
