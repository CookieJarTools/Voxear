//! Crowd records: characters far beyond the streamed world kept as a few
//! numbers rather than Objs. A Crowd Member marked **Keep as Record**, made
//! from a prefab and in the level's dimension, becomes a record once it is
//! further than Project Settings ▸ Crowds ▸ **Records beyond** from every
//! viewpoint: its prefab, where it is, which way it faces, where it was sent
//! and the rest of its way there, and how fast it walks. Its Obj goes. A
//! record walks on, once a second, along its way and then straight on to
//! where it was sent, with no ground under it, as the world round it may not
//! be there to stand on. Within that distance less a margin, it is made
//! again from its prefab, a few a frame, where it got to, and sent on. Saves
//! of the scene keep the records.
//!
//! Positions are kept from the level's real middle (see
//! `voxear_scene::WorldOrigin`), so they stay put as the world's middle
//! follows the camera.

use voxear_assets::AssetGuid;
use voxear_core::math::{DVec3, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, DimensionId, Transform, WorldOrigin};

use crate::crowds::{CrowdMember, CrowdSettings};
use crate::nav_agent::{NavAgent, NavAgentState};
use crate::prefab::{PrefabInstance, Spawned};

/// How far inside Records beyond a record comes back as an Obj, so one at
/// the edge does not go back and forth.
pub const RECORD_MARGIN: f32 = 30.0;
/// The most records made Objs again, and Objs made records, a frame.
const CHANGES_A_FRAME: usize = 16;
/// Seconds between a record's walks.
const WALK_EVERY: f32 = 1.0;

/// A character kept as numbers while far off.
#[derive(Clone, Debug, PartialEq)]
pub struct CrowdRecord {
    /// Its number in the level's records, the same on every machine.
    pub id: u32,
    pub prefab: AssetGuid,
    /// Where it is, from the level's real middle, and which way it faces,
    /// about up.
    pub position: DVec3,
    pub yaw: f32,
    /// Where it was sent, from the level's real middle, and the corners of
    /// its way there it has not passed.
    pub destination: Option<DVec3>,
    pub way: Vec<DVec3>,
    /// How fast it walks, in metres a second.
    pub speed: f32,
}

/// Every record in the level, and the time since they last walked.
#[derive(Clone, Debug, Default)]
pub struct CrowdRecords {
    pub records: Vec<CrowdRecord>,
    walked: f32,
    /// The level the records are in; another level starts without them.
    level: Option<String>,
    /// The number the next record made takes.
    next_id: u32,
    /// Counts each time records are made or made Objs again, or put in
    /// place: players are sent them again when it moves on.
    pub version: u64,
    /// Put in place from a save or by the server: they are the open level's,
    /// whichever it is when they are next walked.
    adopting: bool,
}

impl CrowdRecords {
    /// The records as a save keeps them, small: each prefab once, then each
    /// record by its prefab's number, where it is to a centimetre, its yaw
    /// in a byte, its speed in centimetres a second, and its destination and
    /// way's corners each from the point before.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut prefabs: Vec<AssetGuid> = Vec::new();
        for record in &self.records {
            if !prefabs.contains(&record.prefab) {
                prefabs.push(record.prefab);
            }
        }
        let mut bytes = RECORDS_MAGIC.to_vec();
        put_varint(&mut bytes, prefabs.len() as u64);
        for prefab in &prefabs {
            bytes.extend(prefab.to_u128().to_le_bytes());
        }
        put_varint(&mut bytes, self.records.len() as u64);
        for record in &self.records {
            let prefab = prefabs.iter().position(|p| *p == record.prefab).unwrap_or(0);
            put_varint(&mut bytes, prefab as u64);
            put_varint(&mut bytes, u64::from(record.id));
            put_point(&mut bytes, record.position, DVec3::ZERO);
            let turn = (record.yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 256.0).round() as u32;
            bytes.push((turn % 256) as u8);
            put_varint(&mut bytes, (record.speed.clamp(0.0, 1000.0) * 100.0).round() as u64);
            bytes.push(u8::from(record.destination.is_some()));
            if let Some(to) = record.destination {
                put_point(&mut bytes, to, record.position);
            }
            put_varint(&mut bytes, record.way.len() as u64);
            let mut from = record.position;
            for &corner in &record.way {
                put_point(&mut bytes, corner, from);
                from = corner;
            }
        }
        bytes
    }

    /// Records from what [`to_bytes`](Self::to_bytes) wrote; `None` for
    /// anything else.
    pub fn from_bytes(bytes: &[u8]) -> Option<Vec<CrowdRecord>> {
        // The first layout had no numbers: they are given in order.
        let (mut at, numbered) = match bytes.strip_prefix(RECORDS_MAGIC) {
            Some(rest) => (rest, true),
            None => (bytes.strip_prefix(FIRST_RECORDS_MAGIC)?, false),
        };
        let prefabs: Vec<AssetGuid> = (0..take_varint(&mut at)?)
            .map(|_| {
                let (guid, rest) = at.split_first_chunk::<16>()?;
                at = rest;
                Some(AssetGuid::from_u128(u128::from_le_bytes(*guid)))
            })
            .collect::<Option<_>>()?;
        let count = take_varint(&mut at)?;
        let mut records = Vec::with_capacity(count.min(1 << 20) as usize);
        for number in 0..count {
            let prefab = *prefabs.get(take_varint(&mut at)? as usize)?;
            let id = match numbered {
                true => u32::try_from(take_varint(&mut at)?).ok()?,
                false => number as u32 + 1,
            };
            let position = take_point(&mut at, DVec3::ZERO)?;
            let (&turn, rest) = at.split_first()?;
            at = rest;
            let yaw = f32::from(turn) / 256.0 * std::f32::consts::TAU;
            let speed = take_varint(&mut at)? as f32 / 100.0;
            let (&has, rest) = at.split_first()?;
            at = rest;
            let destination = match has {
                0 => None,
                _ => Some(take_point(&mut at, position)?),
            };
            let mut way = Vec::new();
            let mut from = position;
            for _ in 0..take_varint(&mut at)? {
                from = take_point(&mut at, from)?;
                way.push(from);
            }
            records.push(CrowdRecord { id, prefab, position, yaw, destination, way, speed });
        }
        at.is_empty().then_some(records)
    }
}

/// What a save's records start with, and their layout's version.
const RECORDS_MAGIC: &[u8] = b"VXCR\x02";
const FIRST_RECORDS_MAGIC: &[u8] = b"VXCR\x01";

fn put_varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push((value as u8) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn take_varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// A point to a centimetre, each axis from `from`'s, zigzagged.
fn put_point(bytes: &mut Vec<u8>, point: DVec3, from: DVec3) {
    for (axis, base) in point.to_array().into_iter().zip(from.to_array()) {
        let steps = ((axis * 100.0).round() - (base * 100.0).round()) as i64;
        put_varint(bytes, ((steps << 1) ^ (steps >> 63)) as u64);
    }
}

fn take_point(bytes: &mut &[u8], from: DVec3) -> Option<DVec3> {
    let mut point = [0.0; 3];
    for (axis, base) in point.iter_mut().zip(from.to_array()) {
        let zigzag = take_varint(bytes)?;
        let steps = (zigzag >> 1) as i64 ^ -((zigzag & 1) as i64);
        *axis = ((base * 100.0).round() + steps as f64) / 100.0;
    }
    Some(DVec3::from_array(point))
}

impl CrowdRecord {
    /// Walks `seconds` on along its way, then straight on to where it was
    /// sent, stopping there.
    pub fn walk(&mut self, seconds: f32) {
        let mut left = f64::from(self.speed.max(0.0) * seconds);
        while left > 0.0 {
            let Some(next) = self.way.first().copied().or(self.destination) else { return };
            let to = next - self.position;
            let far = to.length();
            if far > 1e-6 {
                self.yaw = (to.x as f32).atan2(to.z as f32);
            }
            if far > left {
                self.position += to / far * left;
                return;
            }
            self.position = next;
            left -= far;
            if self.way.is_empty() {
                self.destination = None;
                return;
            }
            self.way.remove(0);
        }
    }
}

/// Objs to make records of and records to make Objs of this frame, as
/// `update` finds them.
#[derive(Default)]
pub struct RecordChanges {
    /// Records to make again: the prefab's GUID, and the record.
    pub make: Vec<CrowdRecord>,
}

/// Where the crowd is measured from, from the level's real middle.
fn viewpoints(world: &World) -> Vec<DVec3> {
    let origin = WorldOrigin::of(world).0.as_dvec3();
    let mut points: Vec<Vec3> = crate::terrain::viewer(world).into_iter().collect();
    points.extend(crate::net::player_viewpoints(world));
    points.into_iter().map(|point| point.as_dvec3() + origin).collect()
}

/// Makes records of the Crowd Members that went beyond Records beyond, their
/// Objs destroyed; walks the records on; and gives back the records come
/// near enough to be made again, taken from the store. `level` is the level
/// open, whose records these are. Nothing while no viewpoint is known.
/// Without `makes`, as on a player's machine, whose records are the
/// server's, records are only walked.
pub fn update(world: &mut World, delta: f32, level: Option<&str>, makes: bool) -> (RecordChanges, Vec<Entity>) {
    let settings = world.resource::<CrowdSettings>().cloned().unwrap_or_default();
    let mut records = world.remove_resource::<CrowdRecords>().unwrap_or_default();
    if records.adopting {
        (records.level, records.adopting) = (level.map(str::to_string), false);
    }
    if records.level.as_deref() != level {
        let version = records.version + 1;
        records = CrowdRecords { level: level.map(str::to_string), version, ..CrowdRecords::default() };
    }
    if !makes {
        walk_on(&mut records, delta);
        world.insert_resource(records);
        return (RecordChanges::default(), Vec::new());
    }
    let points = viewpoints(world);
    let beyond = f64::from(settings.records_beyond);
    let mut changes = RecordChanges::default();
    let mut gone = Vec::new();
    if beyond <= 0.0 || points.is_empty() || !settings.enabled {
        world.insert_resource(records);
        return (changes, gone);
    }
    let nearest = |at: DVec3| points.iter().map(|point| point.distance(at)).fold(f64::INFINITY, f64::min);
    let origin = WorldOrigin::of(world).0.as_dvec3();
    // Crowd Members gone far enough: each its prefab and how it stands.
    let members: Vec<(Entity, AssetGuid, Transform)> = world
        .query_ref::<(Entity, &CrowdMember, &Transform)>()
        .without::<Destroying>()
        .without::<voxear_scene::Parent>()
        .into_iter()
        .filter(|(entity, member, _)| member.keep_as_record && crate::net::has_authority(world, *entity))
        .filter_map(|(entity, _, transform)| {
            let spawned = world.get::<Spawned>(entity).filter(|spawned| spawned.root == entity).map(|s| s.prefab);
            let prefab = spawned.or_else(|| world.get::<PrefabInstance>(entity).map(|instance| instance.0))?;
            Some((entity, prefab, *transform))
        })
        .collect();
    for (entity, prefab, transform) in members {
        if gone.len() >= CHANGES_A_FRAME {
            break;
        }
        let at = transform.position.as_dvec3() + origin;
        if voxear_scene::dimension_of(world, entity) != DimensionId::LEVEL || nearest(at) <= beyond {
            continue;
        }
        let agent = world.get::<NavAgent>(entity);
        let state = world.get::<NavAgentState>(entity);
        let way = state
            .map(|state| state.corners.iter().skip(state.next).map(|c| c.as_dvec3() + origin).collect())
            .unwrap_or_default();
        let (yaw, _, _) = transform.rotation.to_euler(voxear_core::math::EulerRot::YXZ);
        records.next_id += 1;
        records.version += 1;
        records.records.push(CrowdRecord {
            id: records.next_id,
            prefab,
            position: at,
            yaw,
            destination: agent.and_then(|agent| agent.destination).map(|to| to.as_dvec3() + origin),
            way,
            speed: agent.map_or(0.0, |agent| agent.speed),
        });
        gone.push(entity);
    }
    walk_on(&mut records, delta);
    // Those come near again are made again, a few a frame.
    let within = (beyond - f64::from(RECORD_MARGIN)).max(0.0);
    let mut kept = Vec::with_capacity(records.records.len());
    for record in records.records.drain(..) {
        if changes.make.len() < CHANGES_A_FRAME && nearest(record.position) <= within {
            changes.make.push(record);
        } else {
            kept.push(record);
        }
    }
    records.records = kept;
    if !changes.make.is_empty() {
        records.version += 1;
    }
    world.insert_resource(records);
    (changes, gone)
}

/// Once a second, every record walks on.
fn walk_on(records: &mut CrowdRecords, delta: f32) {
    records.walked += delta;
    if records.walked >= WALK_EVERY {
        let seconds = std::mem::take(&mut records.walked);
        records.records.iter_mut().for_each(|record| record.walk(seconds));
    }
}

/// The records within `within` metres of `at`, in this world, the nearest
/// first, as bytes of no more than `most`; and the records' version.
pub(crate) fn near_bytes(world: &World, at: Vec3, within: f32, most: usize) -> (Vec<u8>, u64) {
    let Some(records) = world.resource::<CrowdRecords>() else { return (Vec::new(), 0) };
    let at = at.as_dvec3() + WorldOrigin::of(world).0.as_dvec3();
    let mut near: Vec<(f64, &CrowdRecord)> = records
        .records
        .iter()
        .map(|record| (record.position.distance(at), record))
        .filter(|(far, _)| *far <= f64::from(within))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    // About 20 bytes a record, and a prefab's GUID now and then: as many as fit.
    let fit = (most.saturating_sub(64) / 40).max(1);
    let sent = CrowdRecords {
        records: near.into_iter().take(fit).map(|(_, r)| r.clone()).collect(),
        ..CrowdRecords::default()
    };
    (sent.to_bytes(), records.version)
}

/// On a player's machine: the records the server sent, in place of those
/// it had, walked on from here as the server walks its own.
pub(crate) fn take_from_server(world: &mut World, bytes: &[u8]) -> bool {
    let Some(sent) = CrowdRecords::from_bytes(bytes) else { return false };
    let mut records = world.remove_resource::<CrowdRecords>().unwrap_or_default();
    records.records = sent;
    records.version += 1;
    records.adopting = true;
    world.insert_resource(records);
    true
}

/// A record drawn far off: its number, its prefab, where it is in this
/// world, which way it faces, and how fast it walks there, 0 standing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub id: u32,
    pub prefab: AssetGuid,
    pub at: Vec3,
    pub yaw: f32,
    pub speed: f32,
}

/// The records to draw: those within `within` metres of where the game
/// looks from, the nearest `most`.
pub fn to_draw(world: &World, within: f32, most: usize) -> Vec<Drawn> {
    let Some(records) = world.resource::<CrowdRecords>() else { return Vec::new() };
    let Some(eye) = crate::terrain::viewer(world) else { return Vec::new() };
    let origin = WorldOrigin::of(world).0.as_dvec3();
    let mut near: Vec<(f32, Drawn)> = records
        .records
        .iter()
        .map(|record| {
            let at = (record.position - origin).as_vec3();
            let speed = if record.destination.is_some() { record.speed } else { 0.0 };
            (at.distance(eye), Drawn { id: record.id, prefab: record.prefab, at, yaw: record.yaw, speed })
        })
        .filter(|(far, _)| *far <= within)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().take(most).map(|(_, drawn)| drawn).collect()
}

/// Where a record's Obj is put, in this world's own space, and which way
/// it faces.
pub fn placement(world: &World, record: &CrowdRecord) -> (Vec3, Quat) {
    let origin = WorldOrigin::of(world).0.as_dvec3();
    ((record.position - origin).as_vec3(), Quat::from_rotation_y(record.yaw))
}

/// Sends a record's new Obj on where its record was going.
pub fn send_on(world: &mut World, entity: Entity, record: &CrowdRecord) {
    let origin = WorldOrigin::of(world).0.as_dvec3();
    if let (Some(to), Some(agent)) = (record.destination, world.get_mut::<NavAgent>(entity)) {
        agent.go_to((to - origin).as_vec3());
    }
}

/// How many records the level has.
pub fn count(world: &World) -> usize {
    world.resource::<CrowdRecords>().map_or(0, |records| records.records.len())
}

/// The records as a save keeps them, as bytes; or, given bytes, those put
/// in place of the level's: none where they are not records' bytes.
pub fn state(world: &mut World, given: Option<&[u8]>) -> Option<Vec<u8>> {
    let Some(bytes) = given else {
        return Some(world.resource::<CrowdRecords>().map(CrowdRecords::to_bytes).unwrap_or_default());
    };
    let restored = CrowdRecords::from_bytes(bytes)?;
    let mut records = world.remove_resource::<CrowdRecords>().unwrap_or_default();
    records.next_id = restored.iter().map(|record| record.id).max().unwrap_or(0);
    records.records = restored;
    records.walked = 0.0;
    records.version += 1;
    records.adopting = true;
    world.insert_resource(records);
    Some(Vec::new())
}

/// The most records drawn at once.
pub const MOST_DRAWN: usize = 2000;

/// Marks the Objs drawing a record far off: its models, with nothing else
/// of it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordGhost;
impl voxear_ecs::Component for RecordGhost {}

/// Each record drawn: the Obj it stands at, the models under it each with
/// the clips it walks and stands with, and how far through them it is.
#[derive(Default)]
pub(crate) struct Ghosts(pub std::collections::HashMap<u32, Ghost>);

pub(crate) struct Ghost {
    pub root: Entity,
    pub parts: Vec<(Entity, Option<std::sync::Arc<voxear_render::Clip>>, Option<std::sync::Arc<voxear_render::Clip>>)>,
    pub time: f32,
}

/// Moves each ghost on towards its record, smoothly as its record walks in
/// steps, and plays its clips: walking while its record walks, standing
/// while it stands. Set down where its record is when far behind.
pub(crate) fn move_ghost(world: &mut World, ghost: &mut Ghost, drawn: &Drawn, delta: f32) {
    ghost.time += delta;
    if let Some(transform) = world.get_mut::<Transform>(ghost.root) {
        let to = drawn.at - transform.position;
        let step = drawn.speed.max(0.5) * 1.5 * delta;
        transform.position = match to.length() {
            far if far > 10.0 || far <= step => drawn.at,
            far => transform.position + to / far * step,
        };
        transform.rotation = Quat::from_rotation_y(drawn.yaw);
    }
    for (part, walk, stand) in &ghost.parts {
        let clip = if drawn.speed > 0.05 { walk.clone().or(stand.clone()) } else { stand.clone().or(walk.clone()) };
        match clip {
            Some(clip) => {
                drop(world.insert(*part, voxear_render::PlayedClip { clip, time: ghost.time, looping: true }))
            }
            None => drop(world.remove::<voxear_render::PlayedClip>(*part)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> CrowdRecord {
        CrowdRecord {
            id: 3,
            prefab: AssetGuid::from_u128(7),
            position: DVec3::ZERO,
            yaw: 0.0,
            destination: Some(DVec3::new(10.0, 0.0, 10.0)),
            way: vec![DVec3::new(0.0, 0.0, 10.0)],
            speed: 2.0,
        }
    }

    #[test]
    fn a_record_walks_its_way_then_on_to_where_it_was_sent_and_stops() {
        let mut walker = record();
        walker.walk(3.0);
        assert!((walker.position - DVec3::new(0.0, 0.0, 6.0)).length() < 1e-6, "{}", walker.position);
        walker.walk(3.0);
        assert!(
            (walker.position - DVec3::new(2.0, 0.0, 10.0)).length() < 1e-6,
            "round the corner: {}",
            walker.position
        );
        assert!(walker.way.is_empty());
        assert!((walker.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "facing along x: {}", walker.yaw);
        walker.walk(100.0);
        assert_eq!(walker.position, DVec3::new(10.0, 0.0, 10.0));
        assert_eq!(walker.destination, None, "there, it stops");
    }

    #[test]
    fn records_come_back_from_their_bytes_small() {
        let far = CrowdRecord { position: DVec3::new(-12_345.67, 80.5, 99_000.01), yaw: 1.5, ..record() };
        let still = CrowdRecord { destination: None, way: Vec::new(), ..record() };
        let records = CrowdRecords { records: vec![record(), far.clone(), still], ..Default::default() };
        let bytes = records.to_bytes();
        let back = CrowdRecords::from_bytes(&bytes).expect("read back");
        assert_eq!(back.len(), 3);
        assert_eq!(back[0], record());
        assert!((back[1].position - far.position).length() < 0.006, "to a centimetre: {}", back[1].position);
        assert!((back[1].yaw - far.yaw).abs() < 0.013, "yaw to a 256th of a turn: {}", back[1].yaw);
        assert_eq!(back[2].destination, None);
        // One prefab's GUID, then a dozen bytes or so a record.
        assert!(bytes.len() < 6 + 16 + 3 * 24, "{} bytes", bytes.len());
        assert_eq!(CrowdRecords::from_bytes(b"storms"), None);
        assert_eq!(CrowdRecords::from_bytes(&bytes[..bytes.len() - 1]), None, "cut short");
    }
}
