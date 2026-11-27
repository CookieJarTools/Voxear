//! Endless Voxel Volumes in a session, as the network carries them (what is
//! sent is worked out in `crate::voxel_sync`): on the server, each player's
//! queue of news, those joining given everything first within a share of
//! their connection, and the falling pieces' poses twenty times a second,
//! each snapshot a delta against the last that player acknowledged; on a
//! player's machine, what came in order, and each piece's poses, shown a
//! moment behind as Objs are, moving between them.

use std::collections::{BTreeMap, HashMap, VecDeque};

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_net::{BitReader, BitWriter, Channel, Delivery};

use super::{End, Message, NetRole, NetSession, PlayerId};
use crate::voxel_sync::{PiecePose, TURN_BITS, unzigzag, zigzag};

/// The share of a connection's budget news of the volumes may take while a
/// player joining is sent what there was.
const SHARE: f64 = 0.5;
/// Ticks between snapshots of the pieces' poses: twenty a second at sixty.
const POSES_EVERY: u64 = 3;
/// Ticks a player who joined waits for an Obj of their own, for the
/// regions nearest it to go first, before they are sent anyway.
const JOIN_WAIT: u32 = 60;
/// Tags of the pieces' snapshots, apart from the Objs' own.
pub(super) const POSE_TAG: u32 = 0x8000_0000;
/// Snapshots a player's machine keeps, for the server to send the next
/// against.
const KEPT_VIEWS: usize = 64;

/// Bytes of the volumes sent and heard, for the stats and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoxelBytes {
    /// News of what changed as the game played, to every player.
    pub news: u64,
    /// What players joining were sent of what there was.
    pub joining: u64,
    /// Snapshots of the pieces falling.
    pub poses: u64,
    /// Requests, from players' machines.
    pub asks: u64,
}

/// A player's share, on the server.
#[derive(Default)]
struct Player {
    /// Whether everything there was has been gathered for them, after which news goes to them too.
    given: bool,
    /// Whether the game was asked to gather it, and how long they waited for an Obj.
    asked: bool,
    waited: u32,
    queue: VecDeque<Vec<u8>>,
    allowance: f64,
    /// The pieces' poses as each snapshot sent left their machine, by its number, and the newest they had.
    views: BTreeMap<u32, HashMap<u32, PiecePose>>,
    acked: Option<u32>,
    next_view: u32,
    delivered: Vec<Delivery>,
}

#[derive(Default)]
pub(super) struct VoxelNet {
    players: HashMap<PlayerId, Player>,
    joins: Vec<PlayerId>,
    asks: Vec<(PlayerId, Vec<u8>)>,
    poses: Vec<(u32, PiecePose)>,
    pub bytes: VoxelBytes,
    // A player's machine's.
    news: Vec<Vec<u8>>,
    views: BTreeMap<u32, HashMap<u32, PiecePose>>,
    samples: HashMap<u32, VecDeque<(u64, PiecePose)>>,
    newest: u64,
    shown: f64,
}

impl VoxelNet {
    /// A player left.
    pub(super) fn forget(&mut self, player: PlayerId) {
        self.players.remove(&player);
    }

    /// A tagged delivery of one of the pieces' snapshots, on the server.
    pub(super) fn delivered(&mut self, player: PlayerId, delivery: Delivery) {
        self.players.entry(player).or_default().delivered.push(delivery);
    }

    /// A player's request, heard by the server.
    pub(super) fn asked(&mut self, player: PlayerId, bytes: Vec<u8>) {
        self.bytes.asks += bytes.len() as u64;
        self.asks.push((player, bytes));
    }

    /// News from the server, on a player's machine.
    pub(super) fn heard(&mut self, bytes: Vec<u8>) {
        self.news.push(bytes);
    }

    /// A snapshot of the pieces' poses from the server, on a player's
    /// machine: against one it had, its poses noted for the tick it shows.
    pub(super) fn heard_poses(&mut self, bytes: &[u8]) {
        let Some((number, base, tick, entries, dropped)) = read_poses(bytes) else { return };
        let mut view = match base {
            Some(base) => match self.views.get(&base) {
                Some(view) => view.clone(),
                None => return,
            },
            None => HashMap::new(),
        };
        for (id, pose) in entries {
            let pose = match pose {
                Entry::Whole(pose) => pose,
                Entry::Moved { by, turn } => {
                    let Some(was) = view.get(&id) else { continue };
                    let at = [0, 1, 2].map(|a| was.at[a].wrapping_add(by[a]));
                    PiecePose { at, turn: turn.unwrap_or(was.turn) }
                }
            };
            view.insert(id, pose);
        }
        for id in dropped {
            view.remove(&id);
        }
        self.samples.retain(|id, _| view.contains_key(id));
        for (id, pose) in &view {
            let samples = self.samples.entry(*id).or_default();
            if samples.back().is_none_or(|(at, _)| *at < tick) {
                samples.push_back((tick, *pose));
            }
            while samples.len() > 32 {
                samples.pop_front();
            }
        }
        self.newest = self.newest.max(tick);
        self.views.insert(number, view);
        while self.views.len() > KEPT_VIEWS {
            self.views.pop_first();
        }
    }

    /// Moves the tick the pieces are shown at on by `ticks`, a little behind
    /// the newest snapshot.
    pub(super) fn advance(&mut self, ticks: f64) {
        if self.newest == 0 {
            return;
        }
        let target = self.newest as f64 - (POSES_EVERY as f64 + 2.0);
        self.shown += ticks;
        let off = target - self.shown;
        if off.abs() > 20.0 {
            self.shown = target;
        } else {
            self.shown += off * 0.05;
        }
    }
}

/// A pose in a snapshot: whole, for a piece new to it, or moved from the
/// pose its base had, and turned where it turned.
enum Entry {
    Whole(PiecePose),
    Moved { by: [i32; 3], turn: Option<u64> },
}

fn write_turn(w: &mut BitWriter, turn: u64) {
    w.write_bits(turn as u32, 32);
    w.write_bits((turn >> 32) as u32, 2 + 3 * TURN_BITS - 32);
}

fn read_turn(r: &mut BitReader) -> Option<u64> {
    let low = u64::from(r.read_bits(32)?);
    Some(low | u64::from(r.read_bits(2 + 3 * TURN_BITS - 32)?) << 32)
}

/// A snapshot: its number, the one it goes against (none: from nothing),
/// the server's tick, then each piece that moved: its number, whether it
/// is whole, its place or how far it moved in millimetres, and its turn
/// where it turned; then the pieces of its base no longer falling.
fn write_poses(number: u32, base: Option<u32>, tick: u64, entries: &[(u32, Entry)], dropped: &[u32]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_varint(u64::from(number));
    w.write_varint(base.map_or(0, |base| u64::from(base) + 1));
    w.write_varint(tick);
    w.write_varint(entries.len() as u64);
    for (id, entry) in entries {
        w.write_varint(u64::from(*id));
        match entry {
            Entry::Whole(pose) => {
                w.write_bool(true);
                pose.at.iter().for_each(|&a| w.write_varint(zigzag(a)));
                write_turn(&mut w, pose.turn);
            }
            Entry::Moved { by, turn } => {
                w.write_bool(false);
                by.iter().for_each(|&a| w.write_varint(zigzag(a)));
                w.write_bool(turn.is_some());
                if let Some(turn) = turn {
                    write_turn(&mut w, *turn);
                }
            }
        }
    }
    w.write_varint(dropped.len() as u64);
    dropped.iter().for_each(|&id| w.write_varint(u64::from(id)));
    w.finish()
}

type Poses = (u32, Option<u32>, u64, Vec<(u32, Entry)>, Vec<u32>);

fn read_poses(bytes: &[u8]) -> Option<Poses> {
    let mut r = BitReader::new(bytes);
    let number = u32::try_from(r.read_varint()?).ok()?;
    let base = match r.read_varint()? {
        0 => None,
        base => Some(u32::try_from(base - 1).ok()?),
    };
    let tick = r.read_varint()?;
    let count = r.read_varint()? as usize;
    let mut entries = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let id = u32::try_from(r.read_varint()?).ok()?;
        let entry = if r.read_bool()? {
            let at = [unzigzag(r.read_varint()?), unzigzag(r.read_varint()?), unzigzag(r.read_varint()?)];
            Entry::Whole(PiecePose { at, turn: read_turn(&mut r)? })
        } else {
            let by = [unzigzag(r.read_varint()?), unzigzag(r.read_varint()?), unzigzag(r.read_varint()?)];
            let turn = if r.read_bool()? { Some(read_turn(&mut r)?) } else { None };
            Entry::Moved { by, turn }
        };
        entries.push((id, entry));
    }
    let count = r.read_varint()? as usize;
    let dropped =
        (0..count.min(4096)).map(|_| r.read_varint().and_then(|id| u32::try_from(id).ok())).collect::<Option<_>>()?;
    Some((number, base, tick, entries, dropped))
}

/// On a server, each frame after the Objs' snapshots: asks the game to
/// gather everything for those joining, sends each player their share of
/// what is queued, and every few ticks a snapshot of the pieces' poses.
pub(super) fn send(session: &mut NetSession) {
    if session.role == NetRole::Client {
        return;
    }
    let End::Server(server) = &mut session.end else { return };
    let (budget, rate, tick) = (session.settings.bytes_per_second, session.settings.tick_rate, session.tick);
    let voxels = &mut session.voxels;
    for (&client, &player) in &session.by_client {
        let at = session.replication.viewpoint(player);
        let one = voxels.players.entry(player).or_default();
        if !one.asked {
            one.waited += 1;
            if at.is_some() || one.waited >= JOIN_WAIT {
                one.asked = true;
                voxels.joins.push(player);
            }
        }
        // What was queued, within a share of the budget while there is much of it.
        one.allowance = (one.allowance + budget * SHARE / f64::from(rate.max(1))).min(budget);
        while !one.queue.is_empty() {
            if one.allowance <= 0.0 && one.queue.len() > 1 {
                break;
            }
            let bytes = one.queue.pop_front().expect("a message");
            one.allowance -= bytes.len() as f64;
            let _ = server.send(client, Channel::RELIABLE, &Message::Voxels(bytes).encode(session.origin));
        }
        if one.queue.is_empty() {
            one.allowance = one.allowance.min(budget * SHARE);
        }
        // What their machine had of the poses: the newest snapshot acknowledged.
        for delivery in std::mem::take(&mut one.delivered) {
            match delivery {
                Delivery::Acked(tag) => {
                    let number = tag & !POSE_TAG;
                    if one.acked.is_none_or(|acked| number > acked) && one.views.contains_key(&number) {
                        one.acked = Some(number);
                    }
                }
                Delivery::Lost(tag) => drop(one.views.remove(&(tag & !POSE_TAG))),
            }
        }
        if let Some(acked) = one.acked {
            one.views.retain(|number, _| *number >= acked);
        }
        if !one.given || tick % POSES_EVERY != 0 {
            continue;
        }
        let base = one.acked.and_then(|acked| one.views.get(&acked).map(|view| (acked, view)));
        let mut entries = Vec::new();
        let mut view = HashMap::new();
        for &(id, pose) in &voxels.poses {
            view.insert(id, pose);
            match base.and_then(|(_, view)| view.get(&id)) {
                Some(was) if *was == pose => {}
                Some(was) => {
                    let by = [0, 1, 2].map(|a| pose.at[a].wrapping_sub(was.at[a]));
                    entries.push((id, Entry::Moved { by, turn: (was.turn != pose.turn).then_some(pose.turn) }));
                }
                None => entries.push((id, Entry::Whole(pose))),
            }
        }
        let dropped: Vec<u32> =
            base.map_or(Vec::new(), |(_, was)| was.keys().copied().filter(|id| !view.contains_key(id)).collect());
        if entries.is_empty() && dropped.is_empty() && base.is_some() {
            continue;
        }
        if voxels.poses.is_empty() && base.is_none() {
            continue;
        }
        let number = one.next_view;
        one.next_view = one.next_view.wrapping_add(1) & !POSE_TAG;
        let bytes = Message::VoxelPoses(write_poses(number, base.map(|(n, _)| n), tick, &entries, &dropped))
            .encode(session.origin);
        voxels.bytes.poses += bytes.len() as u64;
        one.views.insert(number, view);
        let _ = server.send_tagged(client, Channel::UNRELIABLE, &bytes, number | POSE_TAG);
    }
}

/// On a server: players to gather everything for, as they join.
pub fn take_voxel_joins(world: &mut World) -> Vec<PlayerId> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.voxels.joins)).unwrap_or_default()
}

/// Where a player is on a server: their first owned Obj.
pub fn player_place(world: &World, player: PlayerId) -> Option<Vec3> {
    world.resource::<NetSession>()?.replication.viewpoint(player).map(|(at, _)| at)
}

/// Gives a player joining everything there was, in messages, before any
/// news after.
pub fn owe_voxels(world: &mut World, player: PlayerId, messages: Vec<Vec<u8>>) {
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let voxels = &mut session.voxels;
    let one = voxels.players.entry(player).or_default();
    voxels.bytes.joining += messages.iter().map(|m| m.len() as u64).sum::<u64>();
    one.queue.extend(messages);
    one.given = true;
}

/// Sends news of the volumes to every player given everything there was.
pub fn send_voxels(world: &mut World, messages: Vec<Vec<u8>>) {
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let voxels = &mut session.voxels;
    for one in voxels.players.values_mut().filter(|one| one.given) {
        voxels.bytes.news += messages.iter().map(|m| m.len() as u64).sum::<u64>();
        one.queue.extend(messages.iter().cloned());
    }
}

/// Where each piece falling is this frame, on a server, by its number.
pub fn set_piece_poses(world: &mut World, poses: Vec<(u32, PiecePose)>) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.voxels.poses = poses;
    }
}

/// What players asked of the volumes, on a server, each with who asked.
pub fn take_voxel_asks(world: &mut World) -> Vec<(PlayerId, Vec<u8>)> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.voxels.asks)).unwrap_or_default()
}

/// Sends a request of the volumes to the server, from a player's machine.
pub fn ask_voxels(world: &mut World, bytes: Vec<u8>) -> bool {
    let Some(session) = world.resource_mut::<NetSession>() else { return false };
    let origin = session.origin;
    session.voxels.bytes.asks += bytes.len() as u64;
    match &mut session.end {
        End::Client(client) => client.send(Channel::RELIABLE, &Message::VoxelAsk(bytes).encode(origin)).is_ok(),
        End::Server(_) => false,
    }
}

/// News of the volumes that came, on a player's machine, in order.
pub fn take_voxel_news(world: &mut World) -> Vec<Vec<u8>> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.voxels.news)).unwrap_or_default()
}

/// Where each piece is shown now on a player's machine, by its number: in
/// metres from its volume's corner, and turned.
pub fn piece_poses(world: &World) -> Vec<(u32, Vec3, Quat)> {
    let Some(session) = world.resource::<NetSession>() else { return Vec::new() };
    let voxels = &session.voxels;
    let at = voxels.shown;
    voxels
        .samples
        .iter()
        .filter_map(|(id, samples)| {
            let after = samples.iter().position(|(tick, _)| *tick as f64 >= at);
            let (position, rotation) = match after {
                Some(0) | None => {
                    let (_, pose) = if after.is_none() { samples.back()? } else { samples.front()? };
                    (pose.position(), pose.rotation())
                }
                Some(n) => {
                    let ((t0, a), (t1, b)) = (samples[n - 1], samples[n]);
                    let t = ((at - t0 as f64) / (t1 - t0).max(1) as f64).clamp(0.0, 1.0) as f32;
                    (a.position().lerp(b.position(), t), a.rotation().slerp(b.rotation(), t))
                }
            };
            Some((*id, position, rotation))
        })
        .collect()
}

/// Bytes of the volumes sent and heard.
pub fn voxel_bytes(world: &World) -> VoxelBytes {
    world.resource::<NetSession>().map(|s| s.voxels.bytes).unwrap_or_default()
}
