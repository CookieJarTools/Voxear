//! Replication: Objs the server shares with its players.
//!
//! An Obj with a [`NetObject`] is given an id by the server and made on
//! each player's machine: a placed Obj is found where its scene put it, a
//! prefab's copy is made from the prefab. Each tick the server works out
//! what changed, packed as tightly as it goes (a position to a millimetre,
//! a rotation in 32 bits), and fills each player's snapshot with what
//! matters most to them first, as much as fits: what did not fit waits,
//! gaining in priority, so nothing is left behind for long. What a lost
//! snapshot carried is sent again, as it is by then. On a player's machine
//! each Obj is shown a little in the past, moving smoothly between the
//! snapshots either side, however unevenly they arrive.

use std::collections::{BTreeMap, HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_net::{BitReader, BitWriter};
use voxear_scene::Transform;

use super::PlayerId;

/// Makes an Obj networked: the server sends it to its players, where it
/// moves as it moves on the server.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetObject {
    /// How much it matters next to others when not everything fits in a
    /// snapshot: 1 as usual, 2 twice as often.
    pub priority: f32,
    /// How far, in metres, a player's own Objs may be from it for it to be
    /// sent to them; 0 sends it to every player.
    pub range: f32,
}

impl Default for NetObject {
    fn default() -> Self {
        Self { priority: 1.0, range: 0.0 }
    }
}

impl Component for NetObject {}

/// An Obj's id in a session, the same on the server and every player's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u32);

/// A networked Obj's place in its session: its id, and the player who owns
/// it, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Networked {
    pub id: NetId,
    pub owner: Option<PlayerId>,
}

impl Component for Networked {}

/// Where a networked Obj comes from, so a player's machine can find or make
/// its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Placed in a scene: the scene's GUID and the Obj's path in it.
    Placed { scene: String, path: String },
    /// A copy of a prefab, by GUID.
    Prefab(String),
}

/// What a networked Obj looks like on a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjState {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    /// Its Mist's emitting, when it has one.
    pub mist: Option<bool>,
}

impl ObjState {
    pub fn of(world: &World, entity: Entity) -> Option<Self> {
        let transform = world.get::<Transform>(entity)?;
        let mist = world.get::<voxear_render::Mist>(entity).map(|mist| mist.emitting);
        Some(Self { position: transform.position, rotation: transform.rotation, scale: transform.scale, mist })
    }

    /// Puts the state on the Obj.
    fn apply(&self, world: &mut World, entity: Entity) {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            (transform.position, transform.rotation, transform.scale) = (self.position, self.rotation, self.scale);
        }
        if let (Some(emitting), Some(mist)) = (self.mist, world.get_mut::<voxear_render::Mist>(entity)) {
            mist.emitting = emitting;
        }
    }

    /// Which parts differ from `other` as sent: position, rotation, scale,
    /// mist, a bit each.
    fn changed(&self, other: &Self) -> u8 {
        let position = quantize_position(self.position) != quantize_position(other.position);
        let rotation = quantize_rotation(self.rotation) != quantize_rotation(other.rotation);
        let scale = self.scale != other.scale;
        let mist = self.mist != other.mist;
        u8::from(position) | u8::from(rotation) << 1 | u8::from(scale) << 2 | u8::from(mist) << 3
    }

    fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            position: self.position.lerp(other.position, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
            mist: if t < 0.5 { self.mist } else { other.mist },
        }
    }
}

pub(super) const ALL: u8 = 0b1_1111;

/// Positions within this many metres of the origin, either way.
const REACH: f32 = 8192.0;
/// Steps a metre: about a millimetre.
const STEPS: f32 = 1024.0;
const POSITION_BITS: u32 = 24;
const ROTATION_BITS: u32 = 10;

fn quantize_position(p: Vec3) -> [i32; 3] {
    p.to_array().map(|v| (v.clamp(-REACH, REACH) * STEPS).round() as i32)
}

fn quantize_rotation(q: Quat) -> u32 {
    let mut w = BitWriter::new();
    w.write_rotation(q.normalize().to_array(), ROTATION_BITS);
    let bytes = w.finish();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Writes the parts of `state` in `mask`.
/// Writes the parts of `state` in `mask`, but its fields.
pub(super) fn write_state(w: &mut BitWriter, state: &ObjState, mask: u8) {
    w.write_bits(u32::from(mask), 5);
    if mask & 1 != 0 {
        for v in quantize_position(state.position) {
            w.write_bits((v + (REACH * STEPS) as i32) as u32, POSITION_BITS);
        }
    }
    if mask & 2 != 0 {
        w.write_rotation(state.rotation.normalize().to_array(), ROTATION_BITS);
    }
    if mask & 4 != 0 {
        for v in state.scale.to_array() {
            w.write_f32(v);
        }
    }
    if mask & 8 != 0 {
        w.write_bool(state.mist.is_some());
        w.write_bool(state.mist.unwrap_or(false));
    }
}

/// Reads parts written by [`write_state`] over `into`, and which they were.
pub(super) fn read_state(r: &mut BitReader, into: &mut ObjState) -> Option<u8> {
    let mask = r.read_bits(5)? as u8;
    if mask & 1 != 0 {
        let mut p = [0.0f32; 3];
        for v in &mut p {
            *v = (r.read_bits(POSITION_BITS)? as i32 - (REACH * STEPS) as i32) as f32 / STEPS;
        }
        into.position = Vec3::from(p);
    }
    if mask & 2 != 0 {
        into.rotation = Quat::from_array(r.read_rotation(ROTATION_BITS)?);
    }
    if mask & 4 != 0 {
        into.scale = Vec3::new(r.read_f32()?, r.read_f32()?, r.read_f32()?);
    }
    if mask & 8 != 0 {
        let (has, emitting) = (r.read_bool()?, r.read_bool()?);
        into.mist = has.then_some(emitting);
    }
    Some(mask)
}

/// A networked Obj on the server.
pub(super) struct Tracked {
    pub entity: Entity,
    pub origin: Origin,
    pub owner: Option<PlayerId>,
    pub object: NetObject,
    pub state: ObjState,
    /// What changed this tick.
    pub changed: u8,
    /// Its scripts wrote new values for its fields since the last tick.
    pub fields_changed: bool,
    /// Its behaviours' replicated fields, written by their scripts: each
    /// behaviour's, by a key for its type.
    pub fields: Fields,
}

/// Behaviours' replicated fields, by a key for each behaviour's type.
pub type Fields = BTreeMap<u32, Vec<u8>>;

/// The bit of a change mask for behaviours' replicated fields.
pub(super) const FIELDS: u8 = 16;

pub(super) fn write_fields(w: &mut BitWriter, fields: &Fields) {
    w.write_varint(fields.len() as u64);
    for (key, bytes) in fields {
        w.write_bits(*key, 32);
        w.write_varint(bytes.len() as u64);
        w.write_bytes(bytes);
    }
}

pub(super) fn read_fields(r: &mut BitReader) -> Option<Fields> {
    let count = r.read_varint()? as usize;
    if count > 256 {
        return None;
    }
    let mut fields = Fields::new();
    for _ in 0..count {
        let key = r.read_bits(32)?;
        let length = r.read_varint()? as usize;
        if length > 4096 {
            return None;
        }
        let mut bytes = vec![0u8; length];
        r.read_bytes(&mut bytes)?;
        fields.insert(key, bytes);
    }
    Some(fields)
}

/// What one player has of the server's Objs.
#[derive(Default)]
pub(super) struct Viewer {
    /// Each Obj made on their machine, what of it still needs sending, and
    /// how long it has waited, as priority.
    pub known: HashMap<NetId, (u8, f32)>,
    /// What each snapshot not yet heard of carried.
    pub carried: HashMap<u32, Vec<(NetId, u8)>>,
    pub next_tag: u32,
}

/// A networked Obj on a player's machine.
pub(super) struct Remote {
    pub entity: Option<Entity>,
    /// Its states by tick, the oldest first.
    pub samples: VecDeque<(u64, ObjState)>,
}

/// An Obj the server told this player to make.
#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub id: NetId,
    pub origin: Origin,
    pub owner: Option<PlayerId>,
    pub state: ObjState,
}

/// The server's and the players' replication.
#[derive(Default)]
pub(super) struct Replication {
    pub next_id: u32,
    pub objects: BTreeMap<NetId, Tracked>,
    pub viewers: HashMap<PlayerId, Viewer>,
    pub remote: HashMap<NetId, Remote>,
    pub spawns: Vec<SpawnRequest>,
    pub despawns: Vec<Entity>,
    /// The server's tick a player's machine shows, a little in the past,
    /// in ticks.
    pub shown: f64,
    /// Behaviours' replicated fields that came for Objs on this machine,
    /// for scripts to take in, and those for Objs not made yet.
    pub field_updates: Vec<(Entity, u32, Vec<u8>)>,
    pub early_fields: HashMap<NetId, Fields>,
    /// The newest server tick a snapshot came from, and how unevenly they
    /// come, in ticks.
    pub newest: u64,
    pub jitter: f64,
    pub last_arrival: Option<f64>,
}

/// The most snapshot bytes for one player a tick.
pub(super) const SNAPSHOT_BYTES: usize = 1100;
/// The most Objs made on a player's machine a tick, so a crowd arriving
/// spreads over several.
pub(super) const SPAWNS_A_TICK: usize = 32;
/// How many samples each Obj keeps on a player's machine.
const SAMPLES: usize = 32;

impl Replication {
    /// Each Obj's state now, noting what changed; Objs gone are forgotten,
    /// and given back so players are told.
    pub fn capture(&mut self, world: &World) -> Vec<NetId> {
        let mut gone = Vec::new();
        for (id, tracked) in &mut self.objects {
            let alive = world.is_alive(tracked.entity) && !world.has::<voxear_scene::Destroying>(tracked.entity);
            match ObjState::of(world, tracked.entity).filter(|_| alive) {
                Some(state) => {
                    tracked.changed = state.changed(&tracked.state) | if tracked.fields_changed { FIELDS } else { 0 };
                    tracked.fields_changed = false;
                    tracked.state = state;
                    if let Some(object) = world.get::<NetObject>(tracked.entity) {
                        tracked.object = *object;
                    }
                }
                None => gone.push(*id),
            }
        }
        for id in &gone {
            self.objects.remove(id);
        }
        gone
    }

    /// Where a player is, for range and priority: their first owned Obj.
    pub fn viewpoint(&self, player: PlayerId) -> Option<Vec3> {
        self.objects.values().find(|t| t.owner == Some(player)).map(|t| t.state.position)
    }

    /// The samples' state shown at server tick `at`, between the two either
    /// side, or the nearest when there is nothing on one side.
    pub fn sample(samples: &VecDeque<(u64, ObjState)>, at: f64) -> Option<ObjState> {
        let after = samples.iter().position(|(tick, _)| *tick as f64 >= at);
        match after {
            Some(0) => samples.front().map(|(_, s)| *s),
            Some(i) => {
                let ((t0, s0), (t1, s1)) = (samples[i - 1], samples[i]);
                let t = ((at - t0 as f64) / (t1 - t0).max(1) as f64) as f32;
                Some(s0.lerp(&s1, t.clamp(0.0, 1.0)))
            }
            // Past the newest: held there, as guessing on moves things through walls.
            None => samples.back().map(|(_, s)| *s),
        }
    }

    /// Notes a state for a remote Obj on tick `tick`.
    pub fn note(&mut self, id: NetId, tick: u64, state: ObjState) {
        let remote = self.remote.entry(id).or_insert_with(|| Remote { entity: None, samples: VecDeque::new() });
        if remote.samples.back().is_some_and(|(newest, _)| *newest >= tick) {
            // An older snapshot overtaken by a newer: its news is already out of date.
            return;
        }
        remote.samples.push_back((tick, state));
        while remote.samples.len() > SAMPLES {
            remote.samples.pop_front();
        }
    }

    /// Moves every remote Obj to where it was `delay` ticks ago on the
    /// server, as the snapshots tell it.
    pub fn show(&self, world: &mut World) {
        for remote in self.remote.values() {
            let (Some(entity), Some(state)) = (remote.entity, Self::sample(&remote.samples, self.shown)) else {
                continue;
            };
            state.apply(world, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_survives_being_sent_to_a_millimetre() {
        let state = ObjState {
            position: Vec3::new(123.4567, -2.0001, 4000.25),
            rotation: Quat::from_euler(voxear_core::math::EulerRot::YXZ, 1.0, 0.3, -0.2),
            scale: Vec3::new(1.0, 2.0, 1.0),
            mist: Some(true),
        };
        let mut w = BitWriter::new();
        write_state(&mut w, &state, ALL);
        let bits = w.bits();
        let bytes = w.finish();
        let mut back = ObjState { position: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE, mist: None };
        assert_eq!(read_state(&mut BitReader::new(&bytes), &mut back), Some(ALL));
        assert!(back.position.distance(state.position) < 0.001, "{} against {}", back.position, state.position);
        assert!(back.rotation.dot(state.rotation).abs() > 0.9999);
        assert_eq!((back.scale, back.mist), (state.scale, state.mist));
        let mut moved = BitWriter::new();
        write_state(&mut moved, &state, 0b11);
        assert!(moved.bits() <= 5 + 72 + 32, "a moving Obj in {} bits", moved.bits());
        assert!(bits < 220);
    }

    #[test]
    fn shown_between_samples_and_held_past_the_newest() {
        let at = |x: f32| ObjState { position: Vec3::new(x, 0.0, 0.0), rotation: Quat::IDENTITY, scale: Vec3::ONE, mist: None };
        let samples: VecDeque<(u64, ObjState)> = [(10, at(0.0)), (12, at(2.0)), (14, at(6.0))].into();
        assert_eq!(Replication::sample(&samples, 11.0).unwrap().position.x, 1.0);
        assert_eq!(Replication::sample(&samples, 13.5).unwrap().position.x, 5.0);
        assert_eq!(Replication::sample(&samples, 20.0).unwrap().position.x, 6.0, "held");
        assert_eq!(Replication::sample(&samples, 5.0).unwrap().position.x, 0.0);
    }
}
