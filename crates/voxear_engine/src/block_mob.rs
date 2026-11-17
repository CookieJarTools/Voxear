//! Block Mob: an Obj that goes about a world of blocks by itself. Given
//! somewhere to go, a point, another Obj to keep near, or leave to wander,
//! it walks there as a Nav Agent walks (see `nav_agent`): its way found
//! over the world's places (`voxear_nav`) in the background, round walls,
//! up steps a block high and down drops, walked straight from corner to
//! corner with its Character Controller, going round other mobs and
//! characters. It turns to face the way it goes, falls where the ground
//! ends, and in water deeper than it can stand in it swims up to keep its
//! head out.
//!
//! A Block Mob says where to go; the agent's steps do the walking, at the
//! Block Mob's own speed, turn speed and swimming, unless the Obj has a Nav
//! Agent of its own, whose settings it then walks by. It moves the Obj and
//! nothing else. A Mob Motion on the same Obj sees it move and animates its
//! model; a game tells it where to go.

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_nav::{BlockNav, PlaceRef, Walker, blocks::search::random_near};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::BlockWorld;

use crate::{
    nav_agent::{NavAgent, NavAgentState, NavAvoidance, NavMoves},
    physics::CharacterController,
};

/// How long it stands between wanders, at least and at most, in seconds.
const RESTS: (f32, f32) = (1.5, 5.0);

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

/// What a Block Mob is doing about where it is to go, kept by the engine.
#[derive(Clone, Debug, Default)]
pub struct BlockMobState {
    /// Where it is walking to now, by its goal; `None` to stand.
    pub destination: Option<Vec3>,
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

impl Component for BlockMobState {}

impl BlockMobState {
    /// A number from 0 up to 1, another each time.
    fn chance(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

impl BlockMob {
    /// The Nav Agent it walks as, where its Obj has none of its own: its
    /// speed, turn speed and swimming, the size of its Character Controller.
    pub fn as_agent(&self, controller: &CharacterController, destination: Option<Vec3>) -> NavAgent {
        NavAgent {
            speed: self.speed,
            acceleration: 60.0,
            turn_speed: self.turn_speed,
            stopping_distance: 0.0,
            auto_brake: true,
            radius: controller.radius,
            height: controller.height,
            avoidance: NavAvoidance::High,
            priority: 0.5,
            moves: NavMoves::CharacterController,
            swims: self.swims,
            destination,
            stopped: false,
        }
    }
}

/// A world of blocks as a mob finds where to go in it.
struct Walked {
    corner: Vec3,
    size: f32,
    nav: std::sync::Arc<BlockNav>,
}

impl Walked {
    fn local(&self, at: Vec3) -> [f32; 3] {
        ((at - self.corner) / self.size).to_array()
    }

    fn world(&self, at: [f32; 3]) -> Vec3 {
        self.corner + Vec3::from_array(at) * self.size
    }

    /// The place to stand nearest `at`, within `reach` blocks up or down.
    fn standing(&self, at: Vec3, walker: Walker, reach: f32) -> Option<PlaceRef> {
        self.nav.nearest(self.local(at), walker, 0.0, reach)
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

/// Works out where every Block Mob is to walk to now, by its goal, `delta`
/// seconds on: before the agents step, in the fixed step. Its Nav Agent,
/// where it has one, is sent there; its own steps (see
/// `nav_agent::step_agents`) take it there where it has none.
pub fn set_destinations(world: &mut World, delta: f32) {
    let mobs: Vec<(Entity, BlockMob, CharacterController)> = world
        .query_ref::<(Entity, &BlockMob, &CharacterController)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| crate::net::has_authority(world, *entity))
        .map(|(entity, mob, controller)| (entity, mob.clone(), *controller))
        .collect();
    if mobs.is_empty() {
        return;
    }
    let worlds: Vec<(Entity, Vec3, f32)> = world
        .query_ref::<(Entity, &BlockWorld, &Transform)>()
        .into_iter()
        .map(|(entity, blocks, transform)| {
            (entity, voxear_terrain::corner(transform, world.get::<WorldTransform>(entity)), blocks.block_size)
        })
        .collect();
    // Every world a mob may walk in has its places kept from now on.
    let walked: Vec<Walked> = worlds
        .into_iter()
        .filter_map(|(entity, corner, size)| {
            Some(Walked { corner, size, nav: crate::navigation::nav_of(world, entity)? })
        })
        .collect();
    for (entity, mut mob, controller) in mobs {
        let Some(at) = position(world, entity) else { continue };
        let mut state = world
            .get::<BlockMobState>(entity)
            .cloned()
            .unwrap_or_else(|| BlockMobState { seed: entity.index() ^ 0x9e37_79b9, ..BlockMobState::default() });
        // Set to wander from the start, it does so without being told.
        let begins = !state.begun && mob.wander > 0.0 && mob.goal == MobGoal::Stand;
        state.begun = true;
        if begins {
            mob.wander(mob.wander);
        }
        let walker_in =
            |walked: &Walked| Walker::blocks((controller.height / walked.size).ceil().clamp(1.0, 64.0) as u16);
        let place = walked
            .iter()
            .position(|walked| walked.standing(at + Vec3::Y * 0.1, walker_in(walked), 2.0).is_some())
            .unwrap_or(0);
        let destination = walked.get(place).and_then(|walked| {
            let walker = walker_in(walked);
            let standing = walked.standing(at + Vec3::Y * 0.1, walker, 2.0).and_then(|place| walked.nav.cell(place));
            state.home = state.home.or(standing);
            target(world, &mob, &mut state, walked, (at, standing), walker, delta)
        });
        mob.arrived = destination.is_none();
        state.destination = destination;
        // Its own Nav Agent, where it has one, walks it there.
        if let Some(agent) = world.get_mut::<NavAgent>(entity) {
            (agent.destination, agent.stopped) = (destination, false);
        }
        world.insert(entity, state);
        if let Some(kept) = world.get_mut::<BlockMob>(entity) {
            kept.arrived = mob.arrived;
            if begins {
                kept.goal = mob.goal;
            }
        }
    }
}

/// Where an Obj is in the world.
fn position(world: &World, entity: Entity) -> Option<Vec3> {
    world
        .get::<WorldTransform>(entity)
        .map(|transform| transform.0.w_axis.truncate())
        .or_else(|| world.get::<Transform>(entity).map(|transform| transform.position))
}

/// The point a mob is heading for, by its goal; `None` when it is there, or
/// has nowhere to go.
fn target(
    world: &World,
    mob: &BlockMob,
    state: &mut BlockMobState,
    walked: &Walked,
    (at, standing): (Vec3, Option<[i32; 3]>),
    walker: Walker,
    delta: f32,
) -> Option<Vec3> {
    let flat = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, 0.0, a.z - b.z).length();
    match mob.goal {
        MobGoal::Stand => None,
        MobGoal::To(point) => (flat(point, at) > 0.15 * walked.size.max(1.0)).then_some(point),
        MobGoal::Follow { obj, near } => {
            let followed = position(world, obj)?;
            (flat(followed, at) > near.max(0.2)).then_some(followed)
        }
        MobGoal::Wander { radius } => {
            let (home, standing) = (state.home?, standing?);
            if state.heading == Some(standing) || state.heading.is_none() {
                // There, or not yet heading anywhere: it stands a while, then picks somewhere new.
                state.rest -= delta;
                if state.rest > 0.0 {
                    return None;
                }
                let reach = (radius / walked.size).max(1.0);
                let middle = [home[0] as f32 + 0.5, home[1] as f32 + 0.1, home[2] as f32 + 0.5];
                let from = walked.nav.standing_at(middle, walker, 3.0);
                let seed = (state.chance() * u32::MAX as f32) as u32;
                let costs = voxear_nav::AreaCosts::default();
                state.heading = from
                    .and_then(|from| random_near(&walked.nav, from, walker, &costs, reach, seed))
                    .and_then(|to| walked.nav.cell(to))
                    .filter(|cell| Some(*cell) != Some(standing));
                state.rest = RESTS.0 + state.chance() * (RESTS.1 - RESTS.0);
            }
            state.heading.map(|cell| walked.floor(cell, walker))
        }
    }
}

/// The corners of the way a Block Mob is walking, in the world.
pub fn way(world: &World, entity: Entity) -> Vec<Vec3> {
    world.get::<NavAgentState>(entity).map(|state| state.corners.clone()).unwrap_or_default()
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
            world.remove::<NavAgentState>(entity);
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
