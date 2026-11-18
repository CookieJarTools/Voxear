//! Nav Agent: an Obj that finds its way to where it is sent and walks it,
//! going round the other agents and characters on the way.
//!
//! The easy way is a destination: set one and it goes. The full way is its
//! path (its corners, how it ends, what it costs), its velocity and the one
//! it wants, and how it moves: by its Character Controller, which collides
//! with everything, along the nav itself, which is cheap enough for a town
//! full of them, or not at all, for a script or the motion matcher to move
//! it from the velocity it wants.
//!
//! Agents step in the fixed step, before physics: each takes the way it
//! asked for the step before (found on navigation's threads, see
//! `voxear_nav::threads`), heads for its next corner, and then all of them
//! choose their velocities together, each avoiding the rest
//! (`voxear_nav::avoid`). Only where its Obj has authority does an agent
//! move: on a server, its mobs; a client sees them move by the network.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_nav::{
    BlockNav, FoundPath, PathAsk, PathStatus, PlaceRef, Walker,
    avoid::{Avoidance, Mover, avoid},
    blocks::{moving::slide, search::find_path},
    steer::{self, Steering},
    threads::Job,
};
use voxear_physics::Ground;
use voxear_scene::{Destroying, DimensionId, Transform, WorldTransform, dimension_of};
use voxear_terrain::blocks::BlockWorld;

use crate::{
    block_mob::{BlockMob, BlockMobState},
    navigation::{NavLinkKind, PORTAL_LINK},
    physics::{CharacterController, PhysicsScene},
    portals::NavPortal,
};

/// How many chunks of a long way an agent has found place by place at once.
const REFINED: u32 = 6;
/// How high a jump goes over the ground it leaves, in blocks.
const JUMP_HEIGHT: f32 = 1.3;
/// How near a shut door on its way an agent is when it opens it, in blocks.
const DOOR_REACH: f32 = 1.8;
/// How many places one search may look at.
const MOST_LOOKED_AT: usize = 60_000;
/// How often an agent going after a point that moves looks again, in
/// seconds, though nothing on its way changed.
const SEARCH_EVERY: f32 = 0.75;
/// How many searches agents may begin in one step: a crowd sent somewhere
/// at once has its ways found over the next few steps, not all in one.
const MOST_ASKED: usize = 64;
/// How many searches no thread has begun by the step after they were asked
/// for may be done in the step itself: so an agent's way is there the step
/// after it asked, unless a crowd asked at once.
const MOST_TAKEN_HERE: usize = 8;

/// How an agent moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavMoves {
    /// With its Obj's Character Controller, which collides with blocks,
    /// Objs and slopes as a player does.
    #[default]
    CharacterController,
    /// Along the nav itself, kept on its places: no physics, so hundreds
    /// cost little. It goes round others only by avoiding them.
    AlongTheNav,
    /// Not at all: it finds its way and says the velocity it wants, for a
    /// script or the motion matcher to move it.
    NotAtAll,
}

/// How carefully an agent goes round others.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavAvoidance {
    /// It does not, though others go round it.
    None,
    /// Heeding the four nearest.
    Low,
    /// Heeding the ten nearest.
    #[default]
    High,
}

/// Finds its Obj's way and walks it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavAgent {
    /// Its fastest, in metres a second.
    pub speed: f32,
    /// How quickly it speeds up and slows down, in metres a second each
    /// second.
    pub acceleration: f32,
    /// How fast it turns to face the way it goes, in degrees a second.
    pub turn_speed: f32,
    /// How near its destination it stops, in metres.
    pub stopping_distance: f32,
    /// Whether it slows to stop at its destination, or arrives at speed.
    pub auto_brake: bool,
    /// How wide it is round its middle, and how tall, in metres: what it
    /// fits through, and how far others keep from it.
    pub radius: f32,
    pub height: f32,
    pub avoidance: NavAvoidance,
    /// From 0 to 1: the higher, the more others go round it and the less it
    /// goes round them.
    pub priority: f32,
    pub moves: NavMoves,
    /// Whether it swims up in water too deep to stand in, or sinks; and
    /// whether its ways cross such water.
    pub swims: bool,
    /// Whether it opens shut doors on its way, or goes round them.
    pub opens_doors: bool,
    /// Whether it jumps gaps of a block or two on its way.
    pub jumps: bool,
    /// Whether it climbs vines, ropes and ladders on its way.
    pub climbs: bool,
    /// Its agent type, by its slot in Project Settings ▸ Navigation: 0
    /// for the Default one, 1 to 4 for the project's own. What each area
    /// costs it, and which of a level's baked nav meshes it walks.
    pub agent_type: u8,
    /// Where it is to go, for a game to set; `None` to stand.
    #[serde(skip)]
    pub destination: Option<Vec3>,
    /// The dimension its destination is in, where that is not its own: it
    /// walks to a Portal into that dimension and through, and on from there.
    #[serde(skip)]
    pub destination_dimension: Option<DimensionId>,
    /// Whether it is told to stand where it is, its destination kept.
    #[serde(skip)]
    pub stopped: bool,
}

impl Component for NavAgent {}

impl Default for NavAgent {
    fn default() -> Self {
        Self {
            speed: 3.5,
            acceleration: 8.0,
            turn_speed: 360.0,
            stopping_distance: 0.0,
            auto_brake: true,
            radius: 0.35,
            height: 1.8,
            avoidance: NavAvoidance::High,
            priority: 0.5,
            moves: NavMoves::CharacterController,
            swims: true,
            opens_doors: true,
            jumps: true,
            climbs: true,
            agent_type: 0,
            destination: None,
            destination_dimension: None,
            stopped: false,
        }
    }
}

impl NavAgent {
    /// Sends it to `point`, in its own dimension.
    pub fn go_to(&mut self, point: Vec3) {
        (self.destination, self.destination_dimension, self.stopped) = (Some(point), None, false);
    }

    /// Sends it to `point` in `dimension`, through a Portal into it where
    /// that is not its own.
    pub fn go_to_in(&mut self, point: Vec3, dimension: DimensionId) {
        (self.destination, self.destination_dimension, self.stopped) = (Some(point), Some(dimension), false);
    }
}

/// What a Nav Agent is doing, kept by the engine.
#[derive(Clone, Debug, Default)]
pub struct NavAgentState {
    /// The corners of its way, in the world, and the next to head for.
    pub corners: Vec<Vec3>,
    pub next: usize,
    /// How the way ends, and what it costs.
    pub status: PathStatus,
    pub cost: f32,
    /// How fast it goes, and how fast it wants to.
    pub velocity: Vec3,
    pub wanted: Vec3,
    /// Whether it is where it was sent, or has nowhere to go, as of the
    /// destination it last stepped for (see [`NavAgentState::is_arrived`]).
    pub arrived: bool,
    /// The destination it last stepped for.
    planned_for: Option<Vec3>,
    /// The world of blocks it walks, and its place there.
    pub world: Option<Entity>,
    place: Option<PlaceRef>,
    /// What the way was found in, to notice a change on it.
    found: Option<Found>,
    asked: Option<Asked>,
    /// Where the way asked for goes, as a block of its world.
    asked_to: Option<[i32; 3]>,
    search_in: f32,
    /// How fast it falls, in metres a second.
    fall: f32,
    /// For each corner, whether it is jumped to; and the shut doors on the
    /// way, by their blocks in its world, not yet opened.
    jumps: Vec<bool>,
    doors: Vec<[i32; 3]>,
    /// A jump along the nav under way: from where, to where, how far into
    /// it, how long it lasts, and how high it goes.
    jumping: Option<(Vec3, Vec3, f32, f32, f32)>,
    /// For each corner, the kind of the Nav Link it is reached by, where it
    /// is; and whether the jump under way is a link being crossed.
    links: Vec<Option<u8>>,
    crossing_link: bool,
    /// The carved obstacles its way was found among, and the nav mesh it
    /// was found on, to notice a change.
    cut_seen: u64,
    mesh_seen: usize,
    /// The Portal into its destination's dimension the way asked for leads
    /// to, and the one it heads for now.
    asked_portal: Option<Hop>,
    pub(crate) portal: Option<Hop>,
}

impl Component for NavAgentState {}

/// Doors agents are about to go through, by the world of blocks each is in
/// and its block, for the game to open: opening one numbers its state
/// among the world's blocks, which needs the game's content.
#[derive(Default)]
pub struct DoorsToOpen(pub Vec<(Entity, [i32; 3])>);

/// Opens the doors agents asked for this frame.
pub fn open_doors(world: &mut World, content: &mut crate::Content) {
    let Some(doors) = world.remove_resource::<DoorsToOpen>() else { return };
    for (blocks, at) in doors.0 {
        crate::blocks::set_moved(world, content, blocks, at, true, true);
    }
}

impl NavAgentState {
    /// Whether a way is being looked for.
    pub fn pending(&self) -> bool {
        self.asked.is_some()
    }

    /// Whether it has a way to walk.
    pub fn has_path(&self) -> bool {
        self.corners.len() > 1
    }

    /// Whether `agent` is where it was sent, or has nowhere to go: not
    /// while it has been sent somewhere since it last stepped.
    pub fn is_arrived(&self, agent: &NavAgent) -> bool {
        self.arrived && self.planned_for == agent.destination.filter(|_| !agent.stopped)
    }

    /// How far it is from `at` to the end of its way, on the ground.
    pub fn remaining(&self, at: Vec3) -> f32 {
        steer::remaining(at, &self.corners, self.next)
    }
}

#[derive(Clone, Debug)]
struct Found {
    version: u64,
    chunks: Vec<[i32; 3]>,
}

/// A search asked for, and how many steps it has been waited for.
#[derive(Clone)]
struct Asked(Arc<Job<FoundPath>>, u32);

impl std::fmt::Debug for Asked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Asked")
    }
}

/// A world of blocks as agents walk it.
#[derive(Clone)]
struct Walked {
    entity: Entity,
    corner: Vec3,
    size: f32,
    nav: Arc<BlockNav>,
    blocks: BlockWorld,
    dimension: DimensionId,
    /// What each area costs, and the Nav Areas' boxes over it.
    costs: voxear_nav::AreaCosts,
    boxes: Vec<voxear_nav::AreaBox>,
    /// Its Nav Links and Portals, each with the layers it lets along.
    links: Vec<(voxear_nav::Link, u32)>,
    /// The Nav Obstacles carved in it, and what changed in them last.
    obstacles: Vec<voxear_nav::AreaBox>,
    cuts: crate::nav_obstacle::CutChanges,
}

impl Walked {
    fn local(&self, at: Vec3) -> [f32; 3] {
        ((at - self.corner) / self.size).to_array()
    }

    fn world(&self, at: [f32; 3]) -> Vec3 {
        self.corner + Vec3::from_array(at) * self.size
    }

    fn walker(&self, height: f32) -> Walker {
        Walker::blocks((height / self.size).ceil().clamp(1.0, 64.0) as u16)
    }

    /// The walker `agent` is, `height` metres tall and `radius` wide round
    /// its middle: swimming, opening doors, jumping and climbing as it does,
    /// as many blocks wide as it needs.
    fn walker_of(&self, agent: &NavAgent, height: f32, radius: f32) -> Walker {
        let jump = if agent.jumps { voxear_nav::blocks::walking::MOST_JUMP } else { 0 };
        let width = (2.0 * radius / self.size - 0.25).ceil().clamp(1.0, 4.0) as u8;
        self.walker(height).with(agent.swims, agent.opens_doors, jump).climbing(agent.climbs).wide(width)
    }

    /// Where `walker`, its middle at `at`, stands in blocks: the corner
    /// column of what it stands on, for one wider than a block.
    fn anchor(&self, at: Vec3, walker: Walker) -> [f32; 3] {
        let [x, y, z] = self.local(at);
        [x - walker.middle(), y, z - walker.middle()]
    }

    /// Where `walker`'s middle is in the world, standing at `point` in
    /// blocks.
    fn centre(&self, point: [f32; 3], walker: Walker) -> Vec3 {
        self.world([point[0] + walker.middle(), point[1], point[2] + walker.middle()])
    }

    fn standing(&self, at: Vec3, walker: Walker, reach: f32) -> Option<PlaceRef> {
        self.nav.nearest(self.anchor(at + Vec3::Y * 0.1 * self.size, walker), walker, 0.0, reach)
    }
}

/// World of blocks `entity` as agents walk it: its corner at `corner`, its
/// blocks `size` metres a side.
fn walked_of(world: &mut World, entity: Entity, corner: Vec3, size: f32, blocks: BlockWorld) -> Option<Walked> {
    let nav = crate::navigation::nav_of(world, entity)?;
    let dimension = dimension_of(world, entity);
    Some(Walked {
        entity,
        corner,
        size,
        nav,
        blocks,
        dimension,
        costs: crate::navigation::costs(world),
        boxes: crate::navigation::area_boxes(world, corner, size, dimension),
        links: crate::navigation::links(world, corner, size, dimension),
        obstacles: crate::nav_obstacle::block_boxes(world, corner, size, dimension),
        cuts: crate::nav_obstacle::changes(world, dimension),
    })
}

/// A way an agent's search cannot see past, taken as one step at its end:
/// a Portal into another dimension, or a link into another world of blocks.
/// Where it begins, where it comes out, and how it is crossed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hop {
    pub approach: Vec3,
    pub exit: Vec3,
    pub kind: u8,
}

impl Hop {
    fn through(portal: &NavPortal) -> Self {
        Self { approach: portal.approach, exit: portal.exit, kind: PORTAL_LINK }
    }
}

/// The link from world of blocks `from` into `into`, both of one
/// dimension, that makes the shortest way from `at` to `destination` in
/// straight lines, for an Obj `height` metres tall of `layer`: a Nav Link
/// whose one end is in each, or a Portal from one to the other.
fn link_between(from: &Walked, into: &Walked, at: Vec3, destination: Vec3, height: f32, layer: u8) -> Option<Hop> {
    let ends = |link: &voxear_nav::Link| {
        let (a, b) = (from.world(link.from), from.world(link.to));
        let mut ways = vec![(a, b)];
        if link.both_ways {
            ways.push((b, a));
        }
        ways
    };
    crate::navigation::heeded(&from.links, layer)
        .iter()
        .flat_map(|link| ends(link).into_iter().map(move |(a, b)| (a, b, link.kind)))
        .filter(|(a, b, _)| {
            from.standing(*a, from.walker(height), 1.5).is_some()
                && into.standing(*b, into.walker(height), 1.5).is_some()
        })
        .min_by(|x, y| {
            let along = |(a, b, _): &(Vec3, Vec3, u8)| at.distance(*a) + b.distance(destination);
            along(x).total_cmp(&along(y))
        })
        .map(|(approach, exit, kind)| Hop { approach, exit, kind })
}

/// How tall an agent is and how wide round its middle: as its Character
/// Controller is, or else as its Nav Agent says.
fn size_of(agent: &NavAgent, controller: Option<CharacterController>) -> (f32, f32) {
    match controller {
        Some(controller) => (controller.height, controller.radius.max(agent.radius)),
        None => (agent.height, agent.radius),
    }
}

/// What one agent steps with this step.
struct Stepping {
    entity: Entity,
    agent: NavAgent,
    state: NavAgentState,
    at: Vec3,
    dimension: DimensionId,
    walked: Option<usize>,
    controller: Option<CharacterController>,
}

/// Steps every Nav Agent `delta` seconds on: in the fixed step, before
/// physics.
pub fn step_agents(world: &mut World, delta: f32) {
    if delta <= 0.0 {
        return;
    }
    take_asked(world);
    let mut agents: Vec<(Entity, NavAgent)> = world
        .query_ref::<(Entity, &NavAgent)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, _)| crate::net::has_authority(world, *entity))
        .map(|(entity, agent)| (entity, agent.clone()))
        .collect();
    // Block Mobs without a Nav Agent of their own walk as one made of their settings, to where their goal says.
    let mobs: Vec<(Entity, NavAgent)> = world
        .query_ref::<(Entity, &BlockMob, &CharacterController)>()
        .without::<NavAgent>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| crate::net::has_authority(world, *entity))
        .map(|(entity, mob, controller)| {
            let destination = world.get::<BlockMobState>(entity).and_then(|state| state.destination);
            (entity, mob.as_agent(controller, destination))
        })
        .collect();
    agents.extend(mobs);
    if agents.is_empty() {
        return;
    }
    voxear_core::profile_scope!("Nav Agents");
    let worlds: Vec<(Entity, Vec3, f32, BlockWorld)> = world
        .query_ref::<(Entity, &BlockWorld, &Transform)>()
        .into_iter()
        .map(|(entity, blocks, transform)| {
            let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
            (entity, corner, blocks.block_size, blocks.clone())
        })
        .collect();
    let walked: Vec<Walked> = worlds
        .into_iter()
        .filter_map(|(entity, corner, size, blocks)| walked_of(world, entity, corner, size, blocks))
        .collect();

    // The level's nav mesh for each agent type: its own, or the Default one's.
    let meshes: Vec<Option<(Arc<voxear_nav::mesh::NavMesh>, Vec3)>> =
        (0..crate::navigation::AGENT_TYPES).map(|slot| crate::navigation::level_nav_for(world, slot as u8)).collect();
    let mesh_of = |agent: &NavAgent| meshes[usize::from(agent.agent_type).min(meshes.len() - 1)].as_ref();
    let portals = crate::portals::nav_portals(world);
    // Each agent: its way taken or asked for, and the velocity it wants.
    let mut steps: Vec<Stepping> = Vec::with_capacity(agents.len());
    let mut budget = Budget { asked: MOST_ASKED, taken_here: MOST_TAKEN_HERE, doors: Vec::new() };
    for (entity, agent) in agents {
        let Some(at) = position(world, entity) else { continue };
        let mut state = world.get::<NavAgentState>(entity).cloned().unwrap_or_else(|| NavAgentState {
            search_in: SEARCH_EVERY * (entity.index() % 8) as f32 / 8.0,
            ..NavAgentState::default()
        });
        let controller = world.get::<CharacterController>(entity).copied();
        let (height, radius) = size_of(&agent, controller);
        // The world it walks: the one it stands in, or else the first there is.
        // The world it walks: of its own dimension, the one it stands in, or else the first there is.
        let dimension = dimension_of(world, entity);
        let ours = |walked: &&Walked| walked.dimension == dimension;
        // Crossing a link, it keeps the world it set off from until it lands: over a gap it stands in none.
        let index = match state.crossing_link {
            true => walked.iter().position(|walked| Some(walked.entity) == state.world),
            false => walked
                .iter()
                .position(|walked| ours(&walked) && walked.standing(at, walked.walker(height), 2.0).is_some())
                .or_else(|| walked.iter().position(|walked| ours(&walked))),
        };
        state.world = index.map(|index| walked[index].entity);
        if state.crossing_link {
            steps.push(Stepping { entity, agent, state, at, dimension, walked: index, controller });
            continue;
        }
        // Its layer, which Portals may not let through.
        let layer = world.get::<voxear_scene::Layer>(entity).map_or(0, |layer| layer.0);
        state.portal = match portal_toward(&portals, &agent, at, dimension, layer) {
            Ok(Some(portal)) => Some(Hop::through(&portal)),
            // Sent to another world of blocks of its own dimension: by a link into it.
            Ok(None) => agent.destination.filter(|_| !agent.stopped).and_then(|destination| {
                let here = &walked[index?];
                let there = walked.iter().find(|walked| {
                    ours(walked)
                        && walked.entity != here.entity
                        && walked.standing(destination, walked.walker(height), 2.0).is_some()
                        && here.standing(destination, here.walker(height), 2.0).is_none()
                })?;
                link_between(here, there, at, destination, height, layer)
            }),
            // Sent into a dimension no Portal here leads into: nowhere to go.
            Err(()) => {
                state.corners.clear();
                (state.wanted, state.arrived, state.status) = (Vec3::ZERO, false, PathStatus::None);
                steps.push(Stepping { entity, agent, state, at, dimension, walked: index, controller });
                continue;
            }
        };
        if let Some(walked) = index.map(|index| &walked[index]) {
            plan(world, &agent, &mut state, walked, &portals, layer, at, (height, radius), delta, &mut budget);
        } else if let Some((mesh, offset)) = mesh_of(&agent) {
            // No world of blocks under it: the level's nav mesh, where it has one.
            plan_mesh(world, &agent, &mut state, mesh, *offset, dimension, &portals, layer, at, delta, &mut budget);
        } else {
            state.wanted = Vec3::ZERO;
        }
        steps.push(Stepping { entity, agent, state, at, dimension, walked: index, controller });
    }

    // The doors they are about to go through, for the game to open.
    if !budget.doors.is_empty() {
        if world.resource::<DoorsToOpen>().is_none() {
            world.insert_resource(DoorsToOpen::default());
        }
        world.resource_mut::<DoorsToOpen>().expect("put in above").0.append(&mut budget.doors);
    }
    // All of them together: each velocity chosen avoiding the rest, and every other character, of its own dimension.
    let mut others: Vec<(DimensionId, Mover)> = world
        .query_ref::<(Entity, &CharacterController)>()
        .without::<NavAgent>()
        .without::<BlockMob>()
        .into_iter()
        .filter_map(|(entity, controller)| {
            let at = position(world, entity)?;
            let moved = world
                .resource::<PhysicsScene>()
                .and_then(|physics| physics.character_state(entity))
                .map_or(Vec3::ZERO, |state| state.velocity);
            let mover = Mover {
                position: steer::flat(at),
                velocity: steer::flat(moved),
                wanted: steer::flat(moved),
                radius: controller.radius,
                speed: steer::flat(moved).length(),
                priority: 1.0,
                avoids: false,
            };
            Some((dimension_of(world, entity), mover))
        })
        .collect();
    // Obstacles not carved, moving or never carving, are gone round as they come.
    others.extend(crate::nav_obstacle::movers(world).into_iter().map(|(dimension, at, velocity, radius)| {
        let mover = Mover {
            position: steer::flat(at),
            velocity: steer::flat(velocity),
            wanted: steer::flat(velocity),
            radius,
            speed: steer::flat(velocity).length(),
            priority: 1.0,
            avoids: false,
        };
        (dimension, mover)
    }));
    let movers: Vec<Mover> = steps
        .iter()
        .map(|step| {
            let radius =
                step.controller.map_or(step.agent.radius, |controller| controller.radius.max(step.agent.radius));
            Mover {
                position: steer::flat(step.at),
                velocity: steer::flat(step.state.velocity),
                wanted: steer::flat(step.state.wanted),
                radius,
                speed: step.agent.speed,
                priority: step.agent.priority,
                avoids: step.agent.avoidance != NavAvoidance::None,
            }
        })
        .collect();
    let heeding = steps.iter().map(|step| step.agent.avoidance).max_by_key(|quality| *quality as u8);
    let avoidance =
        Avoidance { neighbours: if heeding == Some(NavAvoidance::Low) { 4 } else { 10 }, ..Avoidance::default() };
    let mut chosen = vec![Vec2::ZERO; movers.len()];
    let mut dimensions: Vec<DimensionId> = steps.iter().map(|step| step.dimension).collect();
    dimensions.sort_unstable();
    dimensions.dedup();
    for dimension in dimensions {
        let agents: Vec<usize> = (0..steps.len()).filter(|at| steps[*at].dimension == dimension).collect();
        let mut here: Vec<Mover> = agents.iter().map(|at| movers[*at]).collect();
        here.extend(others.iter().filter(|(of, _)| *of == dimension).map(|(_, mover)| *mover));
        for (at, velocity) in agents.into_iter().zip(avoid(&here, &avoidance, delta)) {
            chosen[at] = velocity;
        }
    }
    let agents_count = chosen.len();

    let Some(mut physics) = world.remove_resource::<PhysicsScene>() else {
        // With no physics there is nothing to stand on or fall on: only those moving along the nav move.
        for (step, chosen) in steps.into_iter().zip(chosen.into_iter().take(agents_count)) {
            let mesh = mesh_of(&step.agent);
            finish(world, step, chosen, &walked, None, mesh, &portals, delta);
        }
        return;
    };
    for (step, chosen) in steps.into_iter().zip(chosen.into_iter().take(agents_count)) {
        let mesh = mesh_of(&step.agent);
        finish(world, step, chosen, &walked, Some(&mut physics), mesh, &portals, delta);
    }
    world.insert_resource(physics);
}

/// Where an Obj is in the world.
fn position(world: &World, entity: Entity) -> Option<Vec3> {
    world
        .get::<WorldTransform>(entity)
        .map(|transform| transform.0.w_axis.truncate())
        .or_else(|| world.get::<Transform>(entity).map(|transform| transform.position))
}

/// How many searches agents may still begin and do themselves this step,
/// and the doors they are about to go through.
struct Budget {
    asked: usize,
    taken_here: usize,
    doors: Vec<(Entity, [i32; 3])>,
}

/// Whether an agent at `at` has just reached the edge it jumps from: the
/// corner before the one it heads for, which is jumped to.
fn link_reached(state: &NavAgentState, at: Vec3, size: f32) -> Option<u8> {
    let kind = (*state.links.get(state.next)?)?;
    let edge = state.corners.get(state.next.checked_sub(1)?)?;
    ((steer::flat(*edge) - steer::flat(at)).length() < 0.6 * size).then_some(kind)
}

fn takes_off(state: &NavAgentState, at: Vec3, size: f32) -> bool {
    state.next > 0
        && state.jumps.get(state.next) == Some(&true)
        && state
            .corners
            .get(state.next - 1)
            .is_some_and(|edge| (steer::flat(*edge) - steer::flat(at)).length() < 0.6 * size)
}

/// The Portal an agent at `at` in `dimension`, of `layer`, heads for to
/// reach its destination's dimension: `None` where that is its own, or the
/// first of the Portals one after another that make the shortest way there
/// in straight lines, from one Portal's far side to the next; `Err` where
/// no Portals lead there.
fn portal_toward(
    portals: &[NavPortal],
    agent: &NavAgent,
    at: Vec3,
    dimension: DimensionId,
    layer: u8,
) -> Result<Option<NavPortal>, ()> {
    let (Some(into), Some(destination)) = (agent.destination_dimension, agent.destination) else { return Ok(None) };
    if into == dimension {
        return Ok(None);
    }
    // Each Portal into another dimension: how far it is by the shortest way to it, and the first Portal of that way.
    let ways: Vec<&NavPortal> = portals.iter().filter(|way| way.from != way.into && way.passes(layer)).collect();
    let mut far: Vec<(f32, usize)> = ways
        .iter()
        .enumerate()
        .map(|(n, way)| if way.from == dimension { (at.distance(way.approach), n) } else { (f32::INFINITY, n) })
        .collect();
    let mut done = vec![false; ways.len()];
    while let Some(next) =
        (0..ways.len()).filter(|n| !done[*n] && far[*n].0.is_finite()).min_by(|a, b| far[*a].0.total_cmp(&far[*b].0))
    {
        done[next] = true;
        let (here, first) = far[next];
        for (n, way) in ways.iter().enumerate() {
            let through = here + ways[next].exit.distance(way.approach);
            if !done[n] && way.from == ways[next].into && through < far[n].0 {
                far[n] = (through, first);
            }
        }
    }
    (0..ways.len())
        .filter(|n| ways[*n].into == into && far[*n].0.is_finite())
        .min_by(|a, b| {
            let whole = |n: usize| far[n].0 + ways[n].exit.distance(destination);
            whole(*a).total_cmp(&whole(*b))
        })
        .map(|n| Some(*ways[far[n].1]))
        .ok_or(())
}

/// Whether the corner an agent heads for is the far side of a Portal it
/// has walked to but was not taken through, as one shut since: its way is
/// to be found again.
fn left_at_portal(state: &NavAgentState, portals: &[NavPortal], at: Vec3) -> bool {
    if state.crossing_link || state.links.get(state.next) != Some(&Some(PORTAL_LINK)) {
        return false;
    }
    let exit = state.corners[state.next];
    match portals.iter().find(|way| way.exit.distance(exit) < 0.3) {
        Some(way) => way.through.distance(at) < 0.4,
        None => true,
    }
}

/// The far side of the Portal or link `state` heads for, put on the end of
/// a way found up to it, as one link more.
fn through_portal(state: &mut NavAgentState, hop: Option<Hop>) {
    if let Some(hop) = hop.filter(|_| state.status == PathStatus::Complete && !state.corners.is_empty()) {
        state.corners.push(hop.exit);
        state.links.resize(state.corners.len() - 1, None);
        state.links.push(Some(hop.kind));
        state.jumps.resize(state.corners.len(), false);
    }
}

/// Takes the way asked for last step, asks for one afresh where it is
/// wanted, and works out the velocity the agent wants along it.
#[allow(clippy::too_many_arguments)]
fn plan(
    world: &World,
    agent: &NavAgent,
    state: &mut NavAgentState,
    walked: &Walked,
    portals: &[NavPortal],
    layer: u8,
    at: Vec3,
    (height, radius): (f32, f32),
    delta: f32,
    budget: &mut Budget,
) {
    let walker = walked.walker_of(agent, height, radius);
    // The way asked for: done by now on a thread, or done here while few are waiting on the threads.
    let taken = state.asked.take().and_then(|Asked(job, waited)| {
        let here = !job.is_done() && !job.is_begun() && budget.taken_here > 0;
        let found = match job.is_done() || here {
            true => {
                budget.taken_here -= usize::from(here);
                job.take()
            }
            false => None,
        };
        if found.is_none() {
            state.asked = Some(Asked(job, waited + 1));
        }
        found
    });
    if let Some(found) = taken {
        state.corners = found.corners.iter().map(|corner| walked.centre(*corner, walker)).collect();
        state.next = 1.min(state.corners.len().saturating_sub(1));
        (state.status, state.cost) = (found.status, found.cost);
        state.found = Some(Found { version: found.version, chunks: found.chunks });
        (state.jumps, state.doors, state.links) = (found.jumps, found.doors, found.links);
        through_portal(state, state.asked_portal);
    }
    state.place = walked.standing(at, walker, 2.0);
    state.planned_for = agent.destination.filter(|_| !agent.stopped);
    let Some(destination) = agent.destination.filter(|_| !agent.stopped) else {
        state.corners.clear();
        (state.wanted, state.arrived, state.status) = (Vec3::ZERO, true, PathStatus::None);
        return;
    };
    // Into another dimension: to the Portal there first.
    let crossing = state.portal;
    let destination = crossing.map_or(destination, |portal| portal.approach);
    let to = walked.standing(destination, walker, 3.0).and_then(|place| walked.nav.cell(place));
    state.search_in -= delta;
    let stale = state.found.as_ref().is_some_and(|found| {
        let nav = &walked.nav;
        nav.version() != found.version
            && found.chunks.iter().any(|key| {
                nav.slot_of(*key).and_then(|slot| nav.slot(slot)).is_none_or(|slot| slot.changed > found.version)
            })
    });
    // A way crossing an obstacle carved or let go since it was found.
    let radius = radius.max(0.3 * walked.size);
    let remaining = &state.corners[state.next.saturating_sub(1).min(state.corners.len())..];
    let stale = stale || walked.cuts.crosses(state.cut_seen, remaining, radius, height);
    let stale = stale || left_at_portal(state, portals, at);
    let moved_portal = state.asked_portal != crossing;
    let stale = stale || moved_portal && state.asked.is_none();
    if walked.cuts.version != state.cut_seen && !stale {
        state.cut_seen = walked.cuts.version;
    }
    // A way found part of the way is asked for again near its end.
    let near_end = state.status == PathStatus::Continues && state.remaining(at) < 32.0 * walked.size;
    let moved_goal = state.asked_to != to;
    let wanted = moved_goal || stale || near_end || state.search_in <= 0.0 && to.is_none();
    if state.asked.is_none() && wanted && budget.asked > 0 {
        budget.asked -= 1;
        let from = state.place.and_then(|place| walked.nav.point(place)).unwrap_or(walked.anchor(at, walker));
        let mut ask = PathAsk::new(from, walked.anchor(destination, walker), walker);
        // What each area costs its agent type.
        let costs = crate::navigation::agent_type(world, agent.agent_type).costs;
        (ask.costs, ask.boxes) = (costs, walked.boxes.clone());
        ask.links = crate::navigation::heeded(&walked.links, layer);
        ask.boxes.extend(crate::nav_obstacle::for_walker(&walked.obstacles, walker));
        (ask.most, ask.refine) = (MOST_LOOKED_AT, Some(REFINED));
        (state.cut_seen, state.asked_portal) = (walked.cuts.version, crossing);
        let nav = walked.nav.clone();
        state.asked = Some(Asked(Job::start(move || find_path(&nav, &ask)), 0));
        (state.asked_to, state.search_in) = (to, SEARCH_EVERY);
    }
    let steering = Steering {
        speed: agent.speed,
        acceleration: agent.acceleration,
        stopping: agent.stopping_distance,
        brakes: agent.auto_brake,
        corner: (0.3 * walked.size).max(agent.radius),
    };
    // Its way, and then the destination itself from the way's end, where that is near.
    let mut corners = state.corners.clone();
    if corners.is_empty() {
        corners.push(at);
    }
    let last = *corners.last().unwrap_or(&at);
    if (steer::flat(destination) - steer::flat(last)).length() < 1.5 * walked.size
        && state.status != PathStatus::Continues
        && crossing.is_none()
    {
        corners.push(destination);
    }
    state.next = steer::next_corner(at, &corners, state.next.max(1).min(corners.len() - 1), &steering);
    let wanted = steer::wanted(at, &corners, state.next, &steering);
    state.wanted = Vec3::new(wanted.x, 0.0, wanted.y);
    // Doors on the way, opened as it comes near them.
    let reach = DOOR_REACH * walked.size;
    let near = |cell: &[i32; 3]| {
        let middle = walked.world([cell[0] as f32 + 0.5, cell[1] as f32, cell[2] as f32 + 0.5]);
        (steer::flat(middle) - steer::flat(at)).length() <= reach
    };
    budget.doors.extend(state.doors.iter().filter(|cell| near(cell)).map(|cell| (walked.entity, *cell)));
    state.doors.retain(|cell| !near(cell));
    let left = steer::remaining(at, &corners, state.next);
    state.arrived = left <= agent.stopping_distance + 0.15 * walked.size.max(1.0)
        && matches!(state.status, PathStatus::Complete | PathStatus::None)
        && !state.pending()
        && crossing.is_none();
}

/// Finds an agent's way over the level's nav mesh where it is asked for (a
/// search there costs too little to do on a thread), and works out the
/// velocity it wants along it. `offset` is the mesh's from the world.
#[allow(clippy::too_many_arguments)]
fn plan_mesh(
    world: &World,
    agent: &NavAgent,
    state: &mut NavAgentState,
    mesh: &voxear_nav::mesh::NavMesh,
    offset: Vec3,
    dimension: DimensionId,
    portals: &[NavPortal],
    layer: u8,
    at: Vec3,
    delta: f32,
    budget: &mut Budget,
) {
    state.planned_for = agent.destination.filter(|_| !agent.stopped);
    let Some(destination) = state.planned_for else {
        state.corners.clear();
        (state.wanted, state.arrived, state.status) = (Vec3::ZERO, true, PathStatus::None);
        return;
    };
    // Into another dimension: to the Portal there first.
    let crossing = state.portal;
    let destination = crossing.map_or(destination, |portal| portal.approach);
    let key = (destination / 0.25).round().as_ivec3().to_array();
    state.search_in -= delta;
    // A mesh cut afresh round obstacles is searched again at once; nothing is while a link is crossed.
    let mesh_at = mesh as *const voxear_nav::mesh::NavMesh as usize;
    let recut = state.mesh_seen != mesh_at;
    let wanted = state.asked_to != Some(key) || state.search_in <= 0.0 || recut || left_at_portal(state, portals, at);
    if wanted && !state.crossing_link && budget.asked > 0 {
        budget.asked -= 1;
        state.mesh_seen = mesh_at;
        let (links, kinds) = crate::navigation::mesh_links(world, offset, dimension, layer);
        let found = mesh.find_path_linked(at + offset, destination + offset, 2.0, &links);
        state.corners = found.corners.iter().map(|corner| *corner - offset).collect();
        state.next = 1.min(state.corners.len().saturating_sub(1));
        state.status = match found.status {
            voxear_nav::mesh::MeshPathStatus::Complete => PathStatus::Complete,
            voxear_nav::mesh::MeshPathStatus::Partial => PathStatus::Partial,
            voxear_nav::mesh::MeshPathStatus::None => PathStatus::None,
        };
        state.cost = state.corners.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        (state.asked_to, state.search_in) = (Some(key), SEARCH_EVERY);
        state.links = found.links.iter().map(|link| link.map(|n| kinds[n])).collect();
        (state.jumps, state.doors) = (vec![false; state.corners.len()], Vec::new());
        through_portal(state, crossing);
    }
    let steering = Steering {
        speed: agent.speed,
        acceleration: agent.acceleration,
        stopping: agent.stopping_distance,
        brakes: agent.auto_brake,
        corner: agent.radius.max(0.3),
    };
    let mut corners = state.corners.clone();
    if corners.is_empty() {
        corners.push(at);
    }
    state.next = steer::next_corner(at, &corners, state.next.max(1).min(corners.len() - 1), &steering);
    let wanted = steer::wanted(at, &corners, state.next, &steering);
    state.wanted = Vec3::new(wanted.x, 0.0, wanted.y);
    let left = steer::remaining(at, &corners, state.next);
    state.arrived = left <= agent.stopping_distance + 0.15
        && matches!(state.status, PathStatus::Complete | PathStatus::None)
        && crossing.is_none();
}

/// Moves an agent by the velocity chosen for it, as it moves, and keeps
/// what it is doing.
fn finish(
    world: &mut World,
    mut step: Stepping,
    chosen: Vec2,
    walked: &[Walked],
    physics: Option<&mut PhysicsScene>,
    level_mesh: Option<&(Arc<voxear_nav::mesh::NavMesh>, Vec3)>,
    portals: &[NavPortal],
    delta: f32,
) {
    let agent = &step.agent;
    let state = &mut step.state;
    // A velocity that is no number, as from agents standing in one place, is no velocity.
    let chosen = if chosen.is_finite() { chosen } else { Vec2::ZERO };
    let velocity = if state.velocity.is_finite() { steer::flat(state.velocity) } else { Vec2::ZERO };
    let flat = steer::accelerate(velocity, chosen, agent.acceleration.max(0.01) * 4.0, delta);
    let walked = step.walked.map(|index| &walked[index]);
    let facing = Vec3::new(flat.x, 0.0, flat.y);
    let (height, radius) = size_of(agent, step.controller);
    // A Nav Link reached, or one being crossed: along its line, or its arc for a jump, whatever moves the agent. A
    // Portal is walked into, to just through its opening, for it to take the agent through.
    let mut on_link = false;
    let size = walked.map_or(1.0, |walked| walked.size);
    if agent.moves != NavMoves::NotAtAll && (walked.is_some() || level_mesh.is_some()) {
        if !state.crossing_link
            && let Some(kind) = link_reached(state, step.at, size)
        {
            let end = state.corners[state.next];
            let to = match kind {
                PORTAL_LINK => portals
                    .iter()
                    .filter(|way| way.exit.distance(end) < 0.3)
                    .min_by(|a, b| a.exit.distance(end).total_cmp(&b.exit.distance(end)))
                    .map(|way| way.through),
                _ => Some(end),
            };
            if let Some(to) = to {
                let along = (to - step.at).length();
                let (pace, high) = match NavLinkKind::of_number(kind) {
                    _ if kind == PORTAL_LINK => (1.0, 0.0),
                    NavLinkKind::Jump => (1.0, JUMP_HEIGHT * size),
                    NavLinkKind::Climb => (0.5, 0.0),
                    NavLinkKind::Teleport => (f32::INFINITY, 0.0),
                    NavLinkKind::Walk => (1.0, 0.0),
                };
                let lasts = if pace.is_finite() { (along / (agent.speed * pace).max(0.3)).max(0.2) } else { 0.0 };
                (state.jumping, state.crossing_link) = (Some((step.at, to, 0.0, lasts, high)), true);
            }
        }
        if state.crossing_link
            && let Some((from, to, into, lasts, high)) = state.jumping
        {
            let arc = |into: f32| {
                let share = if lasts > 0.0 { into / lasts } else { 1.0 };
                from.lerp(to, share) + Vec3::Y * (4.0 * high * share * (1.0 - share))
            };
            // Far from where the crossing had it: a Portal took it through, or something else moved it.
            if into > 0.0 && (step.at - arc(into)).length() > size.max(1.0) {
                (state.jumping, state.crossing_link) = (None, false);
                state.place =
                    walked.and_then(|walked| walked.standing(step.at, walked.walker_of(agent, height, radius), 1.0));
            } else {
                let into = (into + delta).min(lasts);
                let next = arc(into);
                if let Some(transform) = world.get_mut::<Transform>(step.entity) {
                    transform.position += next - step.at;
                }
                if into >= lasts {
                    (state.jumping, state.crossing_link, state.fall) = (None, false, 2.0);
                    state.place =
                        walked.and_then(|walked| walked.standing(to, walked.walker_of(agent, height, radius), 1.0));
                } else {
                    state.jumping = Some((from, to, into, lasts, high));
                }
                state.velocity = if lasts > 0.0 { (to - from) / lasts } else { Vec3::ZERO };
                on_link = true;
            }
        }
    }
    match (agent.moves, physics, step.controller) {
        _ if on_link => {}
        (NavMoves::CharacterController, Some(physics), Some(controller)) => {
            // Off the edge it was to jump from: up, as high as a jump goes.
            let gravity = physics.gravity().length();
            if let Some(walked) = walked
                && state.fall <= 2.0
                && takes_off(state, step.at, walked.size)
            {
                state.fall = -(2.0 * gravity * JUMP_HEIGHT * walked.size).sqrt();
            }
            let deep = walked
                .and_then(|walked| walked.blocks.water_depth((step.at - walked.corner).to_array(), 4.0))
                .is_some_and(|depth| depth > controller.height * 0.55);
            let (pace, rise) = match (deep, agent.swims) {
                (true, true) => {
                    state.fall = 0.0;
                    (0.6, 1.6)
                }
                _ => {
                    state.fall = (state.fall + gravity * delta).min(50.0);
                    (1.0, -state.fall)
                }
            };
            // A step a block high is one it climbs.
            if let (Some(walked), Some(controller)) = (walked, world.get_mut::<CharacterController>(step.entity)) {
                controller.step_offset = controller.step_offset.max(walked.size * 1.05);
            }
            let moved = physics.move_character(world, step.entity, facing * pace + Vec3::Y * rise, delta);
            if moved.is_some_and(|moved| moved.ground == Ground::Ground) {
                state.fall = 2.0;
            }
            state.velocity = moved.map_or(facing, |moved| Vec3::new(moved.velocity.x, 0.0, moved.velocity.z));
        }
        (NavMoves::NotAtAll, ..) => state.velocity = facing,
        // Along the nav: kept on its places, falling where they drop away.
        (_, physics, _) => {
            state.velocity = facing;
            // A jump: over the gap to where it lands, along an arc.
            if let Some(walked) = walked
                && state.jumping.is_none()
                && takes_off(state, step.at, walked.size)
            {
                let to = state.corners[state.next];
                let along = (steer::flat(to) - steer::flat(step.at)).length();
                let lasts = (along / agent.speed.max(0.5)).max(0.3);
                state.jumping = Some((step.at, to, 0.0, lasts, JUMP_HEIGHT * walked.size));
            }
            if let Some((from, to, into, lasts, high)) = state.jumping {
                let into = (into + delta).min(lasts);
                let share = into / lasts;
                let next = from.lerp(to, share) + Vec3::Y * (4.0 * high * share * (1.0 - share));
                if let Some(transform) = world.get_mut::<Transform>(step.entity) {
                    transform.position += next - step.at;
                }
                state.jumping = (into < lasts).then_some((from, to, into, lasts, high));
                if state.jumping.is_none() {
                    state.place =
                        walked.and_then(|walked| walked.standing(to, walked.walker_of(agent, height, radius), 1.0));
                }
            } else if let (Some(walked), Some(place)) = (walked, state.place) {
                let walker = walked.walker_of(agent, height, radius);
                let at = walked.anchor(step.at, walker);
                let by = [facing.x * delta / walked.size, facing.z * delta / walked.size];
                // Its corner column kept clear of walls by what of it reaches past that column's middle.
                let margin = (radius / walked.size - walker.middle()).max(0.1);
                let (place, moved) = slide(&walked.nav, place, at, by, walker, margin);
                state.place = Some(place);
                let moved = walked.centre(moved, walker);
                let floor = moved.y;
                let gravity = physics.map_or(9.81, |physics| physics.gravity().length());
                let mut y = step.at.y;
                if y > floor + 0.01 {
                    state.fall = (state.fall + gravity * delta).min(50.0);
                    y = (y - state.fall * delta).max(floor);
                } else {
                    state.fall = 0.0;
                    y = floor;
                }
                let next = Vec3::new(moved.x, y, moved.z);
                if let Some(transform) = world.get_mut::<Transform>(step.entity) {
                    transform.position += next - step.at;
                }
            } else if let (None, Some((mesh, offset))) = (walked, level_mesh) {
                // On the level's nav mesh: each way kept to where it has a floor, the floor followed.
                let mut next = step.at;
                for by in [Vec3::new(facing.x, 0.0, 0.0), Vec3::new(0.0, 0.0, facing.z)] {
                    let tried = next + by * delta + *offset;
                    if mesh.height_at(tried.x, tried.z, next.y + offset.y).is_some() {
                        next += by * delta;
                    }
                }
                let on = next + *offset;
                if let Some(floor) = mesh.height_at(on.x, on.z, on.y).map(|floor| floor - offset.y) {
                    let gravity = physics.map_or(9.81, |physics| physics.gravity().length());
                    if next.y > floor + 0.01 {
                        state.fall = (state.fall + gravity * delta).min(50.0);
                        next.y = (next.y - state.fall * delta).max(floor);
                    } else {
                        (state.fall, next.y) = (0.0, floor);
                    }
                }
                if let Some(transform) = world.get_mut::<Transform>(step.entity) {
                    transform.position += next - step.at;
                }
            }
        }
    }
    // It faces the way it goes, turning no faster than it may.
    if agent.moves != NavMoves::NotAtAll
        && flat.length() > 0.05
        && let Some(transform) = world.get_mut::<Transform>(step.entity)
    {
        let facing = transform.rotation * Vec3::Z;
        let (now, wanted) = (facing.x.atan2(facing.z), flat.x.atan2(flat.y));
        let turn = (wanted - now + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let most = agent.turn_speed.to_radians() * delta;
        transform.rotation = Quat::from_rotation_y(now + turn.clamp(-most, most));
    }
    world.insert(step.entity, step.state);
}

// What scripts read and write of a Nav Agent, as `NavAgent` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const DESTINATION: i32 = 3;
const STOP: i32 = 4;
const RESUME: i32 = 5;
const STOPPED: i32 = 6;
const WARP: i32 = 7;
const SPEED: i32 = 8;
const ACCELERATION: i32 = 9;
const TURN_SPEED: i32 = 10;
const STOPPING_DISTANCE: i32 = 11;
const AUTO_BRAKE: i32 = 12;
const RADIUS: i32 = 13;
const HEIGHT: i32 = 14;
const AVOIDANCE: i32 = 15;
const PRIORITY: i32 = 16;
const MOVES: i32 = 17;
const SWIMS: i32 = 18;
const OPENS_DOORS: i32 = 33;
const JUMPS: i32 = 34;
const DESTINATION_DIMENSION: i32 = 35;
const CLIMBS: i32 = 36;
const AGENT_TYPE: i32 = 37;
const HAS_PATH: i32 = 20;
const PATH_PENDING: i32 = 21;
const ARRIVED: i32 = 22;
const STATUS: i32 = 23;
const REMAINING: i32 = 24;
const VELOCITY: i32 = 25;
const WANTED: i32 = 26;
const NEXT_CORNER: i32 = 27;
const CORNER_COUNT: i32 = 28;
const COST: i32 = 29;
const CORNER_AT: i32 = 30;
const CORNER: i32 = 31;

// What scripts ask of `Navigation`, whichever Obj asks: where from and how tall, then where to, then what was found
// and each of its corners; the nearest place to stand to a point; a place to wander to.
const FROM: i32 = 3;
const FIND: i32 = 4;
const FOUND: i32 = 5;
const FOUND_CORNER_AT: i32 = 6;
const FOUND_CORNER: i32 = 7;
const NEAREST: i32 = 8;
const NEAREST_FOUND: i32 = 9;
const RANDOM: i32 = 11;
const RANDOM_FOUND: i32 = 12;
const AREA_AT: i32 = 13;
const AREA_FOUND: i32 = 14;

/// What a script last asked of navigation.
#[derive(Default)]
struct ScriptAsked {
    from: Vec3,
    height: f32,
    found: (PathStatus, Vec<Vec3>, f32),
    corner: usize,
    nearest: Option<Vec3>,
    random: Option<Vec3>,
    area: Option<u8>,
    seed: u32,
}

/// Sets what answers `NavAgent` and `Navigation` in scripts.
pub fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::NavAgent, values);
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::Navigation, navigation_values);
}

/// The status of a way as scripts number it.
fn status_number(status: PathStatus) -> f32 {
    match status {
        PathStatus::None => 0.0,
        PathStatus::Complete => 1.0,
        PathStatus::Partial => 2.0,
        PathStatus::Continues => 3.0,
    }
}

fn asked(world: &mut World) -> &mut ScriptAsked {
    if world.resource::<ScriptAsked>().is_none() {
        world.insert_resource(ScriptAsked::default());
    }
    world.resource_mut::<ScriptAsked>().expect("put in above")
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| Some([n, 0.0, 0.0, 0.0]);
    let toggle = |on: bool| number(f32::from(u8::from(on)));
    let point = |at: Vec3| Some([at.x, at.y, at.z, 0.0]);
    match (what, written) {
        (HAS, None) => return toggle(world.has::<NavAgent>(entity)),
        (ADD, Some(_)) => {
            if !world.has::<NavAgent>(entity) {
                world.insert(entity, NavAgent::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<NavAgentState>(entity);
            return world.remove::<NavAgent>(entity).map(|_| [0.0; 4]);
        }
        (WARP, Some([x, y, z, _])) => {
            world.get::<NavAgent>(entity)?;
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = Vec3::new(x, y, z);
            }
            world.remove::<NavAgentState>(entity);
            return Some([0.0; 4]);
        }
        (CORNER_AT, Some([at, ..])) => {
            asked(world).corner = at.max(0.0) as usize;
            return Some([0.0; 4]);
        }
        _ => {}
    }
    // What it is doing, read from what the engine keeps.
    if written.is_none() && (HAS_PATH..=CORNER).contains(&what) {
        let at = position(world, entity).unwrap_or_default();
        let corner = world.resource::<ScriptAsked>().map_or(0, |asked| asked.corner);
        let state = world.get::<NavAgentState>(entity).cloned().unwrap_or_default();
        let agent = world.get::<NavAgent>(entity)?;
        let arrived =
            world.get::<NavAgentState>(entity).map_or(agent.destination.is_none(), |state| state.is_arrived(agent));
        return match what {
            HAS_PATH => toggle(state.has_path()),
            PATH_PENDING => toggle(state.pending()),
            ARRIVED => toggle(arrived),
            STATUS => number(status_number(state.status)),
            REMAINING => number(state.remaining(at)),
            VELOCITY => point(state.velocity),
            WANTED => point(state.wanted),
            NEXT_CORNER => point(state.corners.get(state.next).copied().unwrap_or(at)),
            CORNER_COUNT => number(state.corners.len() as f32),
            COST => number(state.cost),
            CORNER => point(*state.corners.get(corner)?),
            _ => None,
        };
    }
    // The dimension its destination is in, by the handle scripts know it by, as a number's bits.
    match (what, written) {
        (DESTINATION_DIMENSION, Some([handle, has, ..])) => {
            let into = (has != 0.0).then(|| crate::dimensions::from_handle(world, handle.to_bits())).flatten();
            world.get_mut::<NavAgent>(entity)?.destination_dimension = into;
            direct(world, entity);
            return Some([0.0; 4]);
        }
        (DESTINATION_DIMENSION, None) => {
            let agent = world.get::<NavAgent>(entity)?;
            let into = agent.destination_dimension.unwrap_or_else(|| dimension_of(world, entity));
            let handle = crate::dimensions::handle_of(world, into);
            return Some(handle.map_or([0.0; 4], |handle| [f32::from_bits(handle), 1.0, 0.0, 0.0]));
        }
        _ => {}
    }
    let agent = world.get_mut::<NavAgent>(entity)?;
    match (what, written) {
        (DESTINATION, Some([x, y, z, has])) => {
            agent.destination = (has != 0.0).then(|| Vec3::new(x, y, z));
            (agent.destination_dimension, agent.stopped) = (None, false);
        }
        (DESTINATION, None) => {
            let to = agent.destination;
            return Some(to.map_or([0.0; 4], |to| [to.x, to.y, to.z, 1.0]));
        }
        (STOP, Some(_)) => agent.stopped = true,
        (RESUME, Some(_)) => agent.stopped = false,
        (STOPPED, None) => return toggle(agent.stopped),
        (SPEED, Some([n, ..])) => agent.speed = n.clamp(0.0, 100.0),
        (SPEED, None) => return number(agent.speed),
        (ACCELERATION, Some([n, ..])) => agent.acceleration = n.clamp(0.0, 1000.0),
        (ACCELERATION, None) => return number(agent.acceleration),
        (TURN_SPEED, Some([n, ..])) => agent.turn_speed = n.clamp(0.0, 3600.0),
        (TURN_SPEED, None) => return number(agent.turn_speed),
        (STOPPING_DISTANCE, Some([n, ..])) => agent.stopping_distance = n.clamp(0.0, 1000.0),
        (STOPPING_DISTANCE, None) => return number(agent.stopping_distance),
        (AUTO_BRAKE, Some([on, ..])) => agent.auto_brake = on != 0.0,
        (AUTO_BRAKE, None) => return toggle(agent.auto_brake),
        (RADIUS, Some([n, ..])) => agent.radius = n.clamp(0.01, 50.0),
        (RADIUS, None) => return number(agent.radius),
        (HEIGHT, Some([n, ..])) => agent.height = n.clamp(0.05, 100.0),
        (HEIGHT, None) => return number(agent.height),
        (AVOIDANCE, Some([n, ..])) => {
            agent.avoidance = match n as i32 {
                0 => NavAvoidance::None,
                1 => NavAvoidance::Low,
                _ => NavAvoidance::High,
            }
        }
        (AVOIDANCE, None) => return number(agent.avoidance as u8 as f32),
        (PRIORITY, Some([n, ..])) => agent.priority = n.clamp(0.0, 1.0),
        (PRIORITY, None) => return number(agent.priority),
        (MOVES, Some([n, ..])) => {
            agent.moves = match n as i32 {
                1 => NavMoves::AlongTheNav,
                2 => NavMoves::NotAtAll,
                _ => NavMoves::CharacterController,
            }
        }
        (MOVES, None) => return number(agent.moves as u8 as f32),
        (SWIMS, Some([on, ..])) => agent.swims = on != 0.0,
        (SWIMS, None) => return toggle(agent.swims),
        (OPENS_DOORS, Some([on, ..])) => agent.opens_doors = on != 0.0,
        (OPENS_DOORS, None) => return toggle(agent.opens_doors),
        (JUMPS, Some([on, ..])) => agent.jumps = on != 0.0,
        (JUMPS, None) => return toggle(agent.jumps),
        (CLIMBS, Some([on, ..])) => agent.climbs = on != 0.0,
        (CLIMBS, None) => return toggle(agent.climbs),
        (AGENT_TYPE, Some([slot, ..])) => {
            agent.agent_type = (slot.max(0.0) as usize).min(crate::navigation::AGENT_TYPES - 1) as u8
        }
        (AGENT_TYPE, None) => return number(f32::from(agent.agent_type)),
        _ => return None,
    }
    if matches!(what, DESTINATION | STOP | RESUME) {
        direct(world, entity);
    }
    Some([0.0; 4])
}

/// Sends where an agent this machine does not move is sent to the machine
/// that does: from a client, to the server (see `net::ask_nav`).
pub fn direct(world: &mut World, entity: Entity) {
    if crate::net::has_authority(world, entity) {
        return;
    }
    let Some(agent) = world.get::<NavAgent>(entity) else { return };
    let (to, stopped, into) = (agent.destination, agent.stopped, agent.destination_dimension);
    let dimension = into.unwrap_or_else(|| dimension_of(world, entity));
    crate::net::ask_nav(world, entity, to.map(|to| (to, dimension)), stopped);
}

/// Sends each agent where players asked the server to, on the server.
fn take_asked(world: &mut World) {
    for asked in crate::net::take_nav_asked(world) {
        let own = dimension_of(world, asked.entity);
        let Some(agent) = world.get_mut::<NavAgent>(asked.entity) else { continue };
        agent.destination = asked.to.map(|(to, _)| to);
        agent.destination_dimension = asked.to.map(|(_, into)| into).filter(|into| *into != own);
        agent.stopped = asked.stopped;
    }
}

/// The world of blocks a point is in, as agents walk it, for something
/// `height` metres tall: the one with a place to stand nearest it, or else
/// the first there is.
fn walked_at(world: &mut World, at: Vec3, height: f32) -> Option<Walked> {
    let worlds: Vec<(Entity, Vec3, f32, BlockWorld)> = world
        .query_ref::<(Entity, &BlockWorld, &Transform)>()
        .into_iter()
        .map(|(entity, blocks, transform)| {
            let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
            (entity, corner, blocks.block_size, blocks.clone())
        })
        .collect();
    let walked: Vec<Walked> = worlds
        .into_iter()
        .filter_map(|(entity, corner, size, blocks)| walked_of(world, entity, corner, size, blocks))
        .collect();
    // Scripts ask in the dimension the game is seen from.
    let current = voxear_scene::CurrentDimension::of(world);
    let walked: Vec<Walked> = walked.into_iter().filter(|walked| walked.dimension == current).collect();
    let index = walked.iter().position(|walked| walked.standing(at, walked.walker(height), 4.0).is_some()).unwrap_or(0);
    walked.into_iter().nth(index)
}

/// The way from `from` to `to` for something `height` metres tall that
/// swims, opens doors and jumps, over the world of blocks `from` is in, of
/// the dimension the game is seen from: how it ends, its corners in the
/// world, and what it costs.
pub fn find_way(world: &mut World, from: Vec3, to: Vec3, height: f32) -> (PathStatus, Vec<Vec3>, f32) {
    // A level of models: its baked nav mesh.
    if walked_at(world, from, height).is_none()
        && let Some((mesh, offset)) = crate::navigation::level_nav(world)
    {
        let (links, _) = crate::navigation::mesh_links(world, offset, voxear_scene::CurrentDimension::of(world), 0);
        let found = mesh.find_path_linked(from + offset, to + offset, 2.0, &links);
        let corners: Vec<Vec3> = found.corners.iter().map(|corner| *corner - offset).collect();
        let length = corners.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        let status = match found.status {
            voxear_nav::mesh::MeshPathStatus::Complete => PathStatus::Complete,
            voxear_nav::mesh::MeshPathStatus::Partial => PathStatus::Partial,
            voxear_nav::mesh::MeshPathStatus::None => PathStatus::None,
        };
        return (status, corners, length);
    }
    let found = walked_at(world, from, height).map(|walked| {
        let agent = NavAgent::default();
        let walker = walked.walker_of(&agent, height, agent.radius);
        let mut ask = PathAsk::new(walked.anchor(from, walker), walked.anchor(to, walker), walker);
        (ask.costs, ask.boxes) = (walked.costs, walked.boxes.clone());
        ask.links = crate::navigation::heeded(&walked.links, 0);
        ask.boxes.extend(crate::nav_obstacle::for_walker(&walked.obstacles, walker));
        let found = find_path(&walked.nav, &ask);
        (found.status, found.corners.iter().map(|corner| walked.centre(*corner, walker)).collect(), found.cost)
    });
    found.unwrap_or((PathStatus::None, Vec::new(), 0.0))
}

/// The place to stand nearest `at`, on its floor, no more than `reach`
/// metres off, in the dimension the game is seen from.
pub fn nearest_place(world: &mut World, at: Vec3, reach: f32) -> Option<Vec3> {
    let Some(walked) = walked_at(world, at, 1.8) else {
        let (mesh, offset) = crate::navigation::level_nav(world)?;
        return mesh.locate(at + offset, reach).map(|(_, point)| point - offset);
    };
    let reach = (reach / walked.size).max(0.5);
    let place = walked.nav.nearest(walked.local(at), walked.walker(1.8), reach, reach)?;
    Some(walked.world(walked.nav.point(place)?))
}

/// The area of ground of the place to stand nearest `at`, no more than
/// `reach` metres off: as its block or a Nav Area box marks it, or Not
/// Walkable under a carved Nav Obstacle.
pub fn area_at(world: &mut World, at: Vec3, reach: f32) -> Option<u8> {
    let walked = walked_at(world, at, 1.8)?;
    let reach = (reach / walked.size).max(0.5);
    let walker = walked.walker(1.8);
    let place = walked.nav.nearest(walked.local(at), walker, reach, reach)?;
    let point = walked.nav.point(place)?;
    let mut boxes = walked.boxes.clone();
    boxes.extend(crate::nav_obstacle::for_walker(&walked.obstacles, walker));
    let boxed = boxes.iter().rev().find(|area_box| area_box.holds(point));
    Some(boxed.map_or(walked.nav.place(place)?.0.area, |area_box| area_box.area))
}

fn navigation_values(world: &mut World, _: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let found_point = |at: Option<Vec3>| Some(at.map_or([0.0; 4], |at| [at.x, at.y, at.z, 1.0]));
    match (what, written) {
        (FROM, Some([x, y, z, height])) => {
            let asked = asked(world);
            (asked.from, asked.height) = (Vec3::new(x, y, z), height.max(0.05));
        }
        (FIND, Some([x, y, z, _])) => {
            let (from, height) = { (asked(world).from, asked(world).height) };
            asked(world).found = find_way(world, from, Vec3::new(x, y, z), height);
        }
        (FOUND, None) => {
            let (status, corners, cost) = &asked(world).found;
            return Some([status_number(*status), corners.len() as f32, *cost, 0.0]);
        }
        (FOUND_CORNER_AT, Some([at, ..])) => asked(world).corner = at.max(0.0) as usize,
        (FOUND_CORNER, None) => {
            let asked = asked(world);
            let corner = *asked.found.1.get(asked.corner)?;
            return Some([corner.x, corner.y, corner.z, 0.0]);
        }
        (NEAREST, Some([x, y, z, reach])) => asked(world).nearest = nearest_place(world, Vec3::new(x, y, z), reach),
        (NEAREST_FOUND, None) => return found_point(asked(world).nearest),
        (RANDOM, Some([x, y, z, radius])) => {
            let at = Vec3::new(x, y, z);
            let seed = {
                let asked = asked(world);
                asked.seed = asked.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                asked.seed
            };
            let random = walked_at(world, at, 1.8).and_then(|walked| {
                let walker = walked.walker(1.8);
                let from = walked.standing(at, walker, 4.0)?;
                let costs = voxear_nav::AreaCosts::default();
                let place = voxear_nav::blocks::search::random_near(
                    &walked.nav,
                    from,
                    walker,
                    &costs,
                    radius / walked.size,
                    seed >> 4,
                )?;
                Some(walked.world(walked.nav.point(place)?))
            });
            asked(world).random = random;
        }
        (RANDOM_FOUND, None) => return found_point(asked(world).random),
        (AREA_AT, Some([x, y, z, reach])) => asked(world).area = area_at(world, Vec3::new(x, y, z), reach),
        (AREA_FOUND, None) => {
            let area = asked(world).area;
            return Some([f32::from(area.unwrap_or(0)), f32::from(u8::from(area.is_some())), 0.0, 0.0]);
        }
        _ => return None,
    }
    Some([0.0; 4])
}
