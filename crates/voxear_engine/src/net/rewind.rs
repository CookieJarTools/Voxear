//! Lag compensation: the server checks a player's shot against the world
//! as their machine showed it, a moment in the past, rather than as it is
//! by the time the shot arrives. Otherwise a player aiming true at a
//! running target would miss by however far it ran in the meantime.
//!
//! The server keeps where each shared Obj was on each tick for a second,
//! and each player's input says how far in the past their machine showed
//! the world. A ray as a player saw it puts the colliders of shared Objs
//! the ray comes near back where they were, casts it, and puts them back.
//! Going back is capped, so a player with a slow connection cannot shoot
//! someone who has long since got behind a wall.

use std::collections::{HashMap, VecDeque};

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_physics::scene::{PhysicsScene, SceneHit};

use super::replication::{NetId, ObjState};
use super::{End, NetSession, PlayerId};

/// The most seconds the server goes back for a player's ray.
pub const MOST_REWIND: f64 = 0.2;
/// How far from a ray, in metres, an Obj's middle may be for its colliders
/// to be put back: enough for a body and its limbs.
const REACH: f32 = 3.0;
/// The most entries of each player's view kept.
const VIEWS: usize = 64;

#[derive(Default)]
pub(super) struct Rewind {
    /// Where each shared Obj was after each tick, for a second.
    history: VecDeque<(u64, HashMap<NetId, ObjState>)>,
    /// Each player's view: for a tick of their input, the server tick their
    /// machine showed then.
    views: HashMap<PlayerId, VecDeque<(u64, f64)>>,
}

impl Rewind {
    /// Notes what a player's machine showed when it sent its input for
    /// `tick`.
    pub fn saw(&mut self, player: PlayerId, tick: u64, shown: f64) {
        let views = self.views.entry(player).or_default();
        if views.back().is_some_and(|(newest, _)| *newest >= tick) {
            return;
        }
        views.push_back((tick, shown));
        while views.len() > VIEWS {
            views.pop_front();
        }
    }

    pub fn forget(&mut self, player: PlayerId) {
        self.views.remove(&player);
    }

    /// The server tick a player's machine showed when it made its input
    /// for `tick`, as near as is known.
    pub(super) fn view(&self, player: PlayerId, tick: u64) -> Option<f64> {
        let views = self.views.get(&player)?;
        views.iter().rev().find(|(t, _)| *t <= tick).or(views.front()).map(|(_, shown)| *shown)
    }

    /// Where an Obj was at `at`, between the ticks either side.
    fn at(&self, id: NetId, at: f64) -> Option<ObjState> {
        let after = self.history.iter().position(|(tick, _)| *tick as f64 >= at)?;
        let (t1, s1) = (self.history[after].0, *self.history[after].1.get(&id)?);
        let Some((t0, states)) = after.checked_sub(1).map(|i| &self.history[i]) else { return Some(s1) };
        let Some(s0) = states.get(&id) else { return Some(s1) };
        let t = ((at - *t0 as f64) / (t1 - t0).max(1) as f64) as f32;
        Some(s0.lerp(&s1, t.clamp(0.0, 1.0)))
    }
}

/// Keeps where each shared Obj is after this tick, on a server.
pub fn remember(world: &mut World) {
    let Some(session) = world.resource::<NetSession>().filter(|s| matches!(s.end, End::Server(_))) else { return };
    if session.replication.objects.is_empty() {
        return;
    }
    let tick = session.tick;
    let keep = session.settings.tick_rate as usize;
    let states: HashMap<NetId, ObjState> = session
        .replication
        .objects
        .iter()
        .filter_map(|(id, tracked)| Some((*id, ObjState::of(world, tracked.entity)?)))
        .collect();
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let history = &mut session.rewind.history;
    if history.back().is_some_and(|(newest, _)| *newest >= tick) {
        history.pop_back();
    }
    history.push_back((tick, states));
    while history.len() > keep {
        history.pop_front();
    }
}

/// The first collider a ray meets as `player`'s machine showed the world
/// when they made their input for `tick`: on a server, shared Objs near
/// the ray are put back where they were then, no more than
/// [`MOST_REWIND`] seconds ago. Their own Objs stay where they are. Without
/// a session, or for a host's own player, as the world is now.
#[allow(clippy::too_many_arguments)]
pub fn raycast_as_seen_by(
    world: &mut World,
    player: PlayerId,
    tick: u64,
    origin: Vec3,
    direction: Vec3,
    distance: f32,
    mask: u32,
    triggers: bool,
) -> Option<SceneHit> {
    let moved = past_poses(world, player, tick, origin, direction, distance);
    let mut scene = world.remove_resource::<PhysicsScene>().unwrap_or_default();
    scene.ready(world);
    let mut put_back = Vec::with_capacity(moved.len());
    for (entity, past, now) in &moved {
        for body in bodies_of(world, &scene, *entity) {
            let (position, rotation) = scene.world().transform(body);
            put_back.push((body, position, rotation));
            // The body's place on its Obj, carried from where the Obj is to where it was.
            let turn = past.rotation * now.rotation.inverse();
            let then = past.position + turn * (position - now.position);
            scene.world_mut().set_transform(body, then, turn * rotation);
        }
    }
    let hit = scene.raycast(origin, direction, distance, mask, triggers);
    for (body, position, rotation) in put_back {
        scene.world_mut().set_transform(body, position, rotation);
    }
    world.insert_resource(scene);
    hit
}

/// The shared Objs to put back for a ray, each with where it was and where
/// it is: those not `player`'s whose middle, then, comes within reach of
/// the ray.
fn past_poses(
    world: &World,
    player: PlayerId,
    tick: u64,
    origin: Vec3,
    direction: Vec3,
    distance: f32,
) -> Vec<(Entity, ObjState, ObjState)> {
    let Some(session) = world.resource::<NetSession>().filter(|s| matches!(s.end, End::Server(_))) else {
        return Vec::new();
    };
    if session.local == Some(player) {
        return Vec::new();
    }
    let Some(seen) = session.rewind.view(player, tick) else { return Vec::new() };
    let now = session.tick as f64;
    let seen = seen.clamp(now - MOST_REWIND * f64::from(session.settings.tick_rate), now);
    if now - seen < 0.01 {
        return Vec::new();
    }
    let direction = direction.normalize_or_zero();
    let reach = if distance.is_finite() { distance } else { 1.0e4 };
    let mut moved = Vec::new();
    for (id, tracked) in &session.replication.objects {
        if tracked.owner == Some(player) {
            continue;
        }
        let (Some(past), Some(current)) = (session.rewind.at(*id, seen), ObjState::of(world, tracked.entity)) else {
            continue;
        };
        if distance_to_ray(past.position, origin, direction, reach) <= REACH {
            moved.push((tracked.entity, past, current));
        }
    }
    moved
}

/// How far a point is from a ray `reach` metres long.
fn distance_to_ray(point: Vec3, origin: Vec3, direction: Vec3, reach: f32) -> f32 {
    let along = (point - origin).dot(direction).clamp(0.0, reach);
    point.distance(origin + direction * along)
}

/// The bodies of an Obj's colliders, and its children's.
fn bodies_of(world: &World, scene: &PhysicsScene, entity: Entity) -> Vec<voxear_physics::BodyId> {
    let mut children: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (child, parent) in world.query_ref::<(Entity, &voxear_scene::Parent)>() {
        children.entry(parent.0).or_default().push(child);
    }
    let mut bodies = Vec::new();
    let mut open = vec![entity];
    while let Some(next) = open.pop() {
        bodies.extend(scene.body(next));
        open.extend(children.get(&next).into_iter().flatten());
    }
    bodies
}
