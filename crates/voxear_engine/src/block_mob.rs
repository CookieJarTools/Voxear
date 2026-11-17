//! Block Mob: an Obj that walks through a world of blocks by itself. Given
//! somewhere to go, a point, another Obj to keep near, or leave to wander,
//! it finds a way over the world's places (`voxear_nav`), round walls, up
//! steps a block high and down drops, in the background, and walks it with
//! its Character Controller, straight from corner to corner, so it is held
//! by everything that holds a character: blocks, other Objs, slopes. It
//! turns to face the way it goes, falls where the ground ends, and in water
//! deeper than it can stand in it swims up to keep its head out.
//!
//! It moves the Obj and nothing else. A Mob Motion on the same Obj sees it
//! move and animates its model; a game tells it where to go.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_nav::{
    BlockNav, FoundPath, PathAsk, PlaceRef, Walker,
    blocks::search::{find_path, random_near},
    threads::Job,
};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::BlockWorld;

use voxear_physics::Ground;

use crate::physics::{CharacterController, PhysicsScene};

/// How many places a search for a way may look at.
const MOST_LOOKED_AT: usize = 40_000;
/// How many chunks of a long way are found place by place at once.
const REFINED: u32 = 6;
/// How often a way is found afresh while it walks, in seconds, though
/// nothing on it changed: what it follows moves.
const SEARCH_EVERY: f32 = 0.75;
/// How long it stands between wanders, at least and at most, in seconds.
const RESTS: (f32, f32) = (1.5, 5.0);

/// Where a Block Mob is to go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MobGoal {
    /// Nowhere: it stands where it is.
    #[default]
    Stand,
    /// To a point in the world.
    To(Vec3),
    /// After an Obj, until it is within `near` metres of it.
    Follow { obj: Entity, near: f32 },
    /// Here and there, no further than `radius` metres from where it began.
    Wander { radius: f32 },
}

/// Walks its Obj through a world of blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockMob {
    /// How fast it walks, in metres a second.
    pub speed: f32,
    /// How fast it turns to face the way it goes, in degrees a second.
    pub turn_speed: f32,
    /// Whether it swims up in water too deep to stand in, or sinks.
    pub swims: bool,
    /// Wanders from the start, without a game telling it to: how far from
    /// where it began, in metres, or 0 to stand until told.
    pub wander: f32,
    /// Where it is to go, for a game to set.
    #[serde(skip)]
    pub goal: MobGoal,
    /// Whether it is where it was told to go, or has nowhere to go.
    #[serde(skip)]
    pub arrived: bool,
}

impl Component for BlockMob {}

impl Default for BlockMob {
    fn default() -> Self {
        Self { speed: 2.2, turn_speed: 480.0, swims: true, wander: 0.0, goal: MobGoal::Stand, arrived: true }
    }
}

impl BlockMob {
    /// Walks to a point in the world.
    pub fn move_to(&mut self, point: Vec3) {
        (self.goal, self.arrived) = (MobGoal::To(point), false);
    }

    /// Walks after an Obj, stopping within `near` metres of it.
    pub fn follow(&mut self, obj: Entity, near: f32) {
        (self.goal, self.arrived) = (MobGoal::Follow { obj, near: near.max(0.0) }, false);
    }

    /// Wanders, no further than `radius` metres from where it is now.
    pub fn wander(&mut self, radius: f32) {
        (self.goal, self.arrived) = (MobGoal::Wander { radius: radius.max(0.0) }, false);
    }

    /// Stands where it is.
    pub fn stop(&mut self) {
        (self.goal, self.arrived) = (MobGoal::Stand, true);
    }
}

/// What a Block Mob is doing, kept by the engine.
#[derive(Clone, Debug, Default)]
pub struct BlockMobState {
    /// The corners of the way it is walking, in the world, and the next of
    /// them to reach.
    way: Vec<Vec3>,
    next: usize,
    /// What the way was found in: the chunks it goes through and the
    /// navigation's version, so a change on it is noticed.
    found: Option<Found>,
    /// A way asked for, to take next frame.
    asked: Option<Asked>,
    /// Where the way was found to, and how long until one is found afresh.
    found_to: Option<[i32; 3]>,
    search_in: f32,
    /// How fast it falls, in metres a second.
    fall: f32,
    /// Wandering: where it began, where it is heading, and how long it
    /// stands before it heads somewhere else.
    home: Option<[i32; 3]>,
    heading: Option<[i32; 3]>,
    rest: f32,
    /// What it draws its wanders from.
    seed: u32,
    /// Whether its goal was taken up, for a wander asked for in its settings.
    begun: bool,
}

/// A way found, without its places.
#[derive(Clone, Debug)]
struct Found {
    version: u64,
    chunks: Vec<[i32; 3]>,
}

/// A search for a way going on in the background.
#[derive(Clone)]
struct Asked(Arc<Job<FoundPath>>);

impl std::fmt::Debug for Asked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Asked")
    }
}

impl Component for BlockMobState {}

impl BlockMobState {
    /// The corners of the way it is walking, in the world.
    pub fn way(&self) -> &[Vec3] {
        &self.way
    }

    /// A number from 0 up to 1, another each time.
    fn chance(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// A world of blocks as a mob walks it: where its corner is, how large a
/// block is, its places, and how deep the water is where the mob is.
struct Walked {
    corner: Vec3,
    size: f32,
    nav: Arc<BlockNav>,
    water: Option<f32>,
}

impl Walked {
    /// A point in the world, in blocks from the world's corner.
    fn local(&self, at: Vec3) -> [f32; 3] {
        ((at - self.corner) / self.size).to_array()
    }

    /// A point in blocks from the world's corner, in the world.
    fn world(&self, at: [f32; 3]) -> Vec3 {
        self.corner + Vec3::from_array(at) * self.size
    }

    /// The place to stand nearest `at`, within `reach` blocks up or down.
    fn standing(&self, at: Vec3, walker: Walker, reach: f32) -> Option<PlaceRef> {
        self.nav.nearest(self.local(at), walker, 0.0, reach)
    }

    /// The block a place's feet are in.
    fn cell(&self, at: PlaceRef) -> Option<[i32; 3]> {
        self.nav.cell(at)
    }

    /// The middle of a place's floor, in the world.
    fn floor(&self, cell: [i32; 3], walker: Walker) -> Vec3 {
        let middle = [cell[0] as f32 + 0.5, cell[1] as f32 + 0.1, cell[2] as f32 + 0.5];
        match self.nav.standing_at(middle, walker, 0.5).and_then(|at| self.nav.point(at)) {
            Some(point) => self.world(point),
            None => self.world([middle[0], cell[1] as f32, middle[2]]),
        }
    }
}

/// Walks every Block Mob `delta` seconds on. Before animation, so what
/// animates the model sees this frame's step.
pub fn walk_block_mobs(world: &mut World, delta: f32) {
    if delta <= 0.0 {
        return;
    }
    let mobs: Vec<(Entity, BlockMob, CharacterController)> = world
        .query_ref::<(Entity, &BlockMob, &CharacterController)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, mob, controller)| (entity, mob.clone(), *controller))
        .collect();
    if mobs.is_empty() {
        return;
    }
    let worlds: Vec<(Entity, Vec3, f32, BlockWorld)> = world
        .query_ref::<(Entity, &BlockWorld, &Transform)>()
        .into_iter()
        .map(|(entity, blocks, transform)| {
            let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
            (entity, corner, blocks.block_size, blocks.clone())
        })
        .collect();
    // Every world a mob may walk in has its places kept from now on.
    let navs: Vec<Option<Arc<BlockNav>>> =
        worlds.iter().map(|(entity, ..)| crate::navigation::nav_of(world, *entity)).collect();
    let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
    let gravity = physics.gravity().length();
    for (entity, mut mob, controller) in mobs {
        let Some(at) = world.get::<Transform>(entity).map(|transform| transform.position) else { continue };
        // Mobs made together search on different frames: each its own share of the time between searches.
        let mut state = world.get::<BlockMobState>(entity).cloned().unwrap_or_else(|| BlockMobState {
            seed: entity.index() ^ 0x9e37_79b9,
            search_in: SEARCH_EVERY * (entity.index() % 8) as f32 / 8.0,
            ..BlockMobState::default()
        });
        // The world of blocks it is in: of its own dimension, the one it can stand in, or else the first there is.
        let here = voxear_scene::dimension_of(world, entity);
        let mine = |world_entity: &Entity| voxear_scene::dimension_of(world, *world_entity) == here;
        let ours = worlds.iter().zip(&navs).filter(|((world_entity, ..), _)| mine(world_entity));
        let walked = ours.filter_map(|((_, corner, size, whole), nav)| {
            let water = whole.water_depth((at - *corner).to_array(), 4.0);
            Some(Walked { corner: *corner, size: *size, nav: nav.clone()?, water })
        });
        let walker_in =
            |walked: &Walked| Walker::blocks((controller.height / walked.size).ceil().clamp(1.0, 64.0) as u16);
        let mut walked: Vec<Walked> = walked.collect();
        let place = walked
            .iter()
            .position(|walked| walked.standing(at + Vec3::Y * 0.1, walker_in(walked), 2.0).is_some())
            .unwrap_or(0);
        let walked = (!walked.is_empty()).then(|| walked.swap_remove(place));

        // A step a block high is one it climbs.
        if let (Some(walked), Some(controller)) = (&walked, world.get_mut::<CharacterController>(entity)) {
            controller.step_offset = controller.step_offset.max(walked.size * 1.05);
        }
        // Set to wander from the start, it does so without being told.
        let begins = !state.begun && mob.wander > 0.0 && mob.goal == MobGoal::Stand;
        state.begun = true;
        if begins {
            mob.wander(mob.wander);
        }

        let mut along = Vec3::ZERO;
        if let Some(walked) = &walked {
            let walker = walker_in(walked);
            let standing = walked.standing(at + Vec3::Y * 0.1, walker, 2.0);
            state.home = state.home.or(standing.and_then(|place| walked.cell(place)));
            let target = target(world, &mob, &mut state, walked, (at, standing), walker, delta);
            mob.arrived = target.is_none();
            if let (Some(target), Some(standing)) = (target, standing) {
                along = towards(&mut state, walked, standing, target, at, walker, delta);
            } else if let Some(target) = target {
                // In the air or the water, with nowhere to find a way from: straight at it.
                along = Vec3::new(target.x - at.x, 0.0, target.z - at.z).normalize_or_zero();
            }
        }

        // It faces the way it goes, turning no faster than it may.
        if along != Vec3::ZERO
            && let Some(transform) = world.get_mut::<Transform>(entity)
        {
            let facing = transform.rotation * Vec3::Z;
            let (now, wanted) = (facing.x.atan2(facing.z), along.x.atan2(along.z));
            let turn = (wanted - now + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let most = mob.turn_speed.to_radians() * delta;
            transform.rotation = Quat::from_rotation_y(now + turn.clamp(-most, most));
        }
        // Up and down: falling, or in water over its middle, swimming up.
        let deep =
            walked.as_ref().and_then(|walked| walked.water).is_some_and(|depth| depth > controller.height * 0.55);
        let (pace, rise) = match (deep, mob.swims) {
            (true, true) => {
                state.fall = 0.0;
                (0.6, 1.6)
            }
            _ => {
                state.fall = (state.fall + gravity * delta).min(50.0);
                (1.0, -state.fall)
            }
        };
        let moved = physics.move_character(world, entity, along * mob.speed * pace + Vec3::Y * rise, delta);
        if moved.is_some_and(|moved| moved.ground == Ground::Ground) {
            // Standing: pressed down a little, to stay on steps going down.
            state.fall = 2.0;
        }
        world.insert(entity, state);
        if let Some(kept) = world.get_mut::<BlockMob>(entity) {
            kept.arrived = mob.arrived;
            if begins {
                kept.goal = mob.goal;
            }
        }
    }
    world.insert_resource(physics);
}

/// The point a mob is heading for, by its goal; `None` when it is there, or
/// has nowhere to go.
fn target(
    world: &World,
    mob: &BlockMob,
    state: &mut BlockMobState,
    walked: &Walked,
    (at, standing): (Vec3, Option<PlaceRef>),
    walker: Walker,
    delta: f32,
) -> Option<Vec3> {
    let flat = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, 0.0, a.z - b.z).length();
    match mob.goal {
        MobGoal::Stand => None,
        MobGoal::To(point) => (flat(point, at) > 0.15 * walked.size.max(1.0)).then_some(point),
        MobGoal::Follow { obj, near } => {
            let followed = world.get::<Transform>(obj)?.position;
            (flat(followed, at) > near.max(0.2)).then_some(followed)
        }
        MobGoal::Wander { radius } => {
            let (home, standing) = (state.home?, standing?);
            let standing_cell = walked.cell(standing)?;
            if state.heading == Some(standing_cell) || state.heading.is_none() {
                // There, or not yet heading anywhere: it stands a while, then picks somewhere new.
                state.rest -= delta;
                if state.rest > 0.0 {
                    return None;
                }
                let reach = (radius / walked.size).max(1.0);
                let from = walked.nav.standing_at(
                    [home[0] as f32 + 0.5, home[1] as f32 + 0.1, home[2] as f32 + 0.5],
                    walker,
                    3.0,
                );
                let pick = state.chance();
                let seed = (pick * u32::MAX as f32) as u32;
                let costs = voxear_nav::AreaCosts::default();
                state.heading = from
                    .and_then(|from| random_near(&walked.nav, from, walker, &costs, reach, seed))
                    .and_then(|to| walked.cell(to))
                    .filter(|cell| *cell != standing_cell);
                state.rest = RESTS.0 + state.chance() * (RESTS.1 - RESTS.0);
            }
            state.heading.map(|cell| walked.floor(cell, walker))
        }
    }
}

/// The way to walk this frame, along the ground: towards the next corner
/// of the way to `target`. A way is asked for afresh when the target is
/// another place, when a chunk the way goes through changed, or when it is
/// time; it is searched for in the background and taken the next frame.
fn towards(
    state: &mut BlockMobState,
    walked: &Walked,
    standing: PlaceRef,
    target: Vec3,
    at: Vec3,
    walker: Walker,
    delta: f32,
) -> Vec3 {
    // The way asked for last frame.
    if let Some(Asked(job)) = state.asked.take()
        && let Some(found) = job.take()
    {
        state.way = found.corners.iter().map(|corner| walked.world(*corner)).collect();
        state.next = 1;
        state.found = Some(Found { version: found.version, chunks: found.chunks });
    }
    let to = walked.standing(target + Vec3::Y * 0.1, walker, 3.0).and_then(|place| walked.cell(place));
    state.search_in -= delta;
    let stale = state.found.as_ref().is_some_and(|found| {
        let nav = &walked.nav;
        nav.version() != found.version
            && found.chunks.iter().any(|key| {
                nav.slot_of(*key).and_then(|slot| nav.slot(slot)).is_none_or(|slot| slot.changed > found.version)
            })
    });
    if state.asked.is_none() && (state.found_to != to || state.search_in <= 0.0 || stale) {
        let from = walked.nav.point(standing).unwrap_or(walked.local(at));
        let mut ask = PathAsk::new(from, walked.local(target), walker);
        (ask.most, ask.refine) = (MOST_LOOKED_AT, Some(REFINED));
        let nav = walked.nav.clone();
        state.asked = Some(Asked(Job::start(move || find_path(&nav, &ask))));
        (state.found_to, state.search_in) = (to, SEARCH_EVERY);
    }
    let flat = |point: Vec3| Vec3::new(point.x - at.x, 0.0, point.z - at.z);
    // Past each corner of the way as it is reached.
    while state.way.get(state.next).is_some_and(|corner| flat(*corner).length() < walked.size * 0.3) {
        state.next += 1;
    }
    match state.way.get(state.next) {
        Some(corner) => flat(*corner).normalize_or_zero(),
        // At the last corner, or with no way found: straight to the point itself, where it is near.
        None if flat(target).length() < walked.size * 1.5 => flat(target).normalize_or_zero(),
        None => Vec3::ZERO,
    }
}

/// What a script asks of a Block Mob, by number, as `NativeBlockMob` in
/// Voxear.Core has them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const MOVE_TO: i32 = 3;
const FOLLOW: i32 = 4;
const WANDER: i32 = 5;
const STOP: i32 = 6;
const ARRIVED: i32 = 7;
const SPEED: i32 = 8;
const TURN_SPEED: i32 = 9;
const SWIMS: i32 = 10;
const GOAL: i32 = 11;

/// Sets what answers `BlockMob` in scripts.
pub fn register() {
    voxear_scripting::set_block_mob_values(values);
}

/// Reads, or with `written` writes, value `what` of an Obj's Block Mob.
/// The Obj to follow comes as the two halves of its number, each in a
/// float's bits, and then how near to come.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let toggle = |on: bool| number(f32::from(u8::from(on)));
    match (what, written) {
        (HAS, None) => return Some(toggle(world.has::<BlockMob>(entity))),
        // It walks with a Character Controller, so it is given one where it has none.
        (ADD, Some(_)) => {
            if !world.has::<CharacterController>(entity) {
                world.insert(entity, CharacterController::default());
            }
            if !world.has::<BlockMob>(entity) {
                world.insert(entity, BlockMob::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<BlockMobState>(entity);
            return world.remove::<BlockMob>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    let mob = world.get_mut::<BlockMob>(entity)?;
    match (what, written) {
        (MOVE_TO, Some([x, y, z, _])) => mob.move_to(Vec3::new(x, y, z)),
        (FOLLOW, Some([low, high, near, _])) => {
            let followed = Entity::from_bits(u64::from(low.to_bits()) | u64::from(high.to_bits()) << 32);
            mob.follow(followed, near);
        }
        (WANDER, Some([radius, ..])) => mob.wander(radius),
        (STOP, Some(_)) => mob.stop(),
        (ARRIVED, None) => return Some(toggle(mob.arrived)),
        (GOAL, None) => {
            let goal = match mob.goal {
                MobGoal::Stand => 0.0,
                MobGoal::To(_) => 1.0,
                MobGoal::Follow { .. } => 2.0,
                MobGoal::Wander { .. } => 3.0,
            };
            return Some(number(goal));
        }
        (SPEED, Some([speed, ..])) => mob.speed = speed.max(0.0),
        (SPEED, None) => return Some(number(mob.speed)),
        (TURN_SPEED, Some([speed, ..])) => mob.turn_speed = speed.max(0.0),
        (TURN_SPEED, None) => return Some(number(mob.turn_speed)),
        (SWIMS, Some([on, ..])) => mob.swims = on != 0.0,
        (SWIMS, None) => return Some(toggle(mob.swims)),
        _ => return None,
    }
    Some([0.0; 4])
}
