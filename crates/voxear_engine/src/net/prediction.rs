//! Prediction: a player's own Objs move on their machine as soon as they
//! press, rather than a round trip later. Their `[Predicted]` behaviours
//! run FixedUpdate there as on the server, with the player's input, and
//! where each Obj was after each tick is kept. When the server's state for
//! a tick comes, it is checked against what was kept for that tick. Where
//! they differ, the Obj is put back as the server had it, with its
//! replicated properties, and the ticks since run again with the input they
//! had. The difference is then shown going away over a tenth of a second,
//! rather than as a jump.

use std::collections::{HashMap, HashSet, VecDeque};

use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};

use super::replication::{Fields, NetId, ObjState};
use super::{End, NetSession, Networked};

/// An Obj this machine predicts: its player's own, with `[Predicted]`
/// behaviours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Predicting;

impl Component for Predicting {}

/// Ticks of history kept: two seconds at 60 a second, longer than any round
/// trip worth playing through.
pub(super) const HISTORY: usize = 128;
/// How far apart, in metres, the server's and this machine's Obj may be
/// before it is put right; positions are sent to a millimetre.
const TOLERANCE: f32 = 0.01;
/// Seconds over which a correction is shown going away.
const SMOOTHING: f32 = 0.1;
/// Corrections further than this, in metres, are shown at once, as a
/// teleport should be.
const SNAP: f32 = 2.0;

/// A predicted Obj the server disagrees with: as it had it on a tick.
#[derive(Clone, Debug)]
pub struct Correction {
    pub entity: Entity,
    pub id: NetId,
    pub tick: u64,
    pub state: ObjState,
}

#[derive(Default)]
pub(super) struct Prediction {
    /// Where each predicted Obj was after each recent tick.
    pub history: HashMap<NetId, VecDeque<(u64, ObjState)>>,
    /// The newest server tick each was checked against.
    checked: HashMap<NetId, u64>,
    /// The server's newest replicated properties for each, kept for
    /// corrections rather than set as they come.
    pub fields: HashMap<NetId, Fields>,
    /// Those this machine predicts.
    pub ids: HashSet<NetId>,
    /// What each Obj shows on top of where it is, going away.
    offsets: HashMap<Entity, Vec3>,
    /// While ticks run again: the tick to come back to, and where the Obj
    /// was before.
    replay: Option<(u64, Vec3)>,
    pub corrections: u64,
}

impl Prediction {
    pub fn replaying(&self) -> bool {
        self.replay.is_some()
    }
}

/// Marks this client's predicted Objs: its player's own networked Objs
/// that `predicted` says have `[Predicted]` behaviours.
pub fn mark_predicting(world: &mut World, predicted: &dyn Fn(Entity) -> bool) {
    let Some(local) = world.resource::<NetSession>().filter(|s| matches!(s.end, End::Client(_))).and_then(|s| s.local)
    else {
        return;
    };
    let owned: Vec<(Entity, NetId, bool)> = world
        .query_ref::<(Entity, &Networked)>()
        .into_iter()
        .map(|(entity, networked)| (entity, networked.id, networked.owner == Some(local) && predicted(entity)))
        .collect();
    let mut ids = HashSet::new();
    for (entity, id, wanted) in owned {
        match (wanted, world.has::<Predicting>(entity)) {
            (true, false) => drop(world.insert(entity, Predicting)),
            (false, true) => drop(world.remove::<Predicting>(entity)),
            _ => {}
        }
        if wanted {
            ids.insert(id);
        }
    }
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.prediction.history.retain(|id, _| ids.contains(id));
        session.prediction.ids = ids;
    }
}

/// Keeps where each predicted Obj is after this tick.
pub fn record(world: &mut World) {
    let Some(session) = world.resource::<NetSession>().filter(|s| !s.prediction.ids.is_empty()) else { return };
    let tick = session.tick;
    let now: Vec<(NetId, ObjState)> = world
        .query_ref::<(Entity, &Networked)>()
        .with::<Predicting>()
        .into_iter()
        .filter_map(|(entity, networked)| Some((networked.id, ObjState::of(world, entity)?)))
        .collect();
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    for (id, state) in now {
        let history = session.prediction.history.entry(id).or_default();
        while history.back().is_some_and(|(kept, _)| *kept >= tick) {
            history.pop_back();
        }
        history.push_back((tick, state));
        while history.len() > HISTORY {
            history.pop_front();
        }
    }
}

/// The predicted Objs the server disagrees with, by the newest state it
/// sent for each.
pub fn corrections(world: &mut World) -> Vec<Correction> {
    let Some(session) = world.resource_mut::<NetSession>() else { return Vec::new() };
    let mut found = Vec::new();
    for &id in &session.prediction.ids {
        let Some(remote) = session.replication.remote.get(&id) else { continue };
        let (Some(entity), Some(&(tick, state))) = (remote.entity, remote.samples.back()) else { continue };
        let checked = session.prediction.checked.entry(id).or_insert(0);
        if tick <= *checked {
            continue;
        }
        *checked = tick;
        let kept =
            session.prediction.history.get(&id).and_then(|h| h.iter().find(|(t, _)| *t == tick)).map(|(_, s)| *s);
        // A tick this machine never ran, as just after joining, is put right too.
        if kept.is_none_or(|kept| kept.position.distance(state.position) > TOLERANCE) && tick < session.tick {
            found.push(Correction { entity, id, tick, state });
        }
    }
    found
}

/// Puts a predicted Obj back as the server had it, for its ticks since to
/// run again: its state, and its replicated properties for scripts to take
/// in at once.
pub fn begin_replay(world: &mut World, correction: &Correction) {
    let before = world.get::<voxear_scene::Transform>(correction.entity).map_or(Vec3::ZERO, |t| t.position);
    correction.state.apply(world, correction.entity);
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let fields = session.prediction.fields.get(&correction.id).cloned().unwrap_or_default();
    let updates = fields.into_iter().map(|(key, bytes)| (correction.entity, key, bytes));
    session.replication.field_updates.splice(0..0, updates);
    session.prediction.replay = Some((session.tick, before));
    session.prediction.corrections += 1;
    session.tick = correction.tick;
    if let Some(history) = session.prediction.history.get_mut(&correction.id) {
        history.retain(|(tick, _)| *tick <= correction.tick);
    }
}

/// Moves a replay on a tick, with the local player's input for it; false
/// once it has caught up.
pub fn replay_tick(world: &mut World) -> bool {
    let Some(session) = world.resource_mut::<NetSession>() else { return false };
    let Some((until, _)) = session.prediction.replay else { return false };
    if session.tick >= until {
        return false;
    }
    session.tick += 1;
    let tick = session.tick;
    let command = session.commands.iter().find(|(t, _)| *t == tick).map(|(_, c)| c.clone()).unwrap_or_default();
    if let Some(local) = session.local {
        if let Some(aim) = command.last() {
            session.aims.insert(local, aim.value);
        }
        session.inputs.insert(local, command);
    }
    true
}

/// Ends a replay, back on the tick it began, and shows the difference going
/// away.
pub fn end_replay(world: &mut World, correction: &Correction) {
    let now = world.get::<voxear_scene::Transform>(correction.entity).map_or(Vec3::ZERO, |t| t.position);
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let Some((until, before)) = session.prediction.replay.take() else { return };
    session.tick = until;
    let command = session.commands.iter().find(|(t, _)| *t == until).map(|(_, c)| c.clone()).unwrap_or_default();
    if let Some(local) = session.local {
        if let Some(aim) = command.last() {
            session.aims.insert(local, aim.value);
        }
        session.inputs.insert(local, command);
    }
    let offset = session.prediction.offsets.entry(correction.entity).or_insert(Vec3::ZERO);
    *offset += before - now;
    if offset.length() > SNAP {
        *offset = Vec3::ZERO;
    }
}

/// Takes the shown offsets off, for the frame's ticks to move the Objs
/// where they are, and lets them go away a little.
pub(super) fn unshow(world: &mut World, delta: f32, prediction: &mut Prediction) {
    let fade = (-delta / (SMOOTHING / 3.0)).exp();
    prediction.offsets.retain(|&entity, offset| {
        let Some(transform) = world.get_mut::<voxear_scene::Transform>(entity) else { return false };
        transform.position -= *offset;
        *offset *= fade;
        offset.length() > 0.0005
    });
}

/// Puts the shown offsets on, for the frame to draw.
pub(super) fn show(world: &mut World, prediction: &Prediction) {
    for (&entity, offset) in &prediction.offsets {
        if let Some(transform) = world.get_mut::<voxear_scene::Transform>(entity) {
            transform.position += *offset;
        }
    }
}
