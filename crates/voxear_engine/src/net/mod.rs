//! Networked play: a session, as a server, a host (a server with a player
//! of its own) or a client, kept in the world as a resource.
//!
//! The game moves in **ticks**, its fixed steps, at the project's tick rate
//! (60 a second unless `[network]` says otherwise). Each tick a client
//! sends what its player did as a command, with the last few again in case
//! one is lost, and the server runs each player's command for that tick.
//! Scripts on the server read a player's input as that player's own.
//!
//! A client keeps a tick or two ahead of the server, so its commands arrive
//! in time: the server tells it how far ahead they arrive, and it runs a
//! little faster or slower until they arrive about right.
//!
//! The game drives it: [`frame_begin`] takes in what arrived and this
//! frame's input, the engine's fixed steps call [`fixed_tick`], and
//! [`frame_end`] sends what is waiting.

mod authority;
mod inputs;
mod messages;
mod prediction;
mod replication;
mod rewind;
mod rpcs;
mod scripts;

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use anyhow::{Context, Result};
use serde::Deserialize;
use voxear_ecs::World;
use voxear_input::ActionState;
use voxear_net::{
    Channel, Client, ClientId, ClientState, Conditions, ConnectToken, ConnectionConfig, Key, Server, ServerConfig,
    ServerEvent, Simulated, TokenRequest, Transport, UdpTransport, USER_DATA_BYTES, unix_seconds,
};

use inputs::{InputBuffer, Latch};
pub use messages::Command;
pub use replication::{NetId, NetObject, Networked, ObjState, Origin, SpawnRequest};
use replication::{Replication, Tracked, Viewer};
pub use authority::{HeldBody, claim, has_authority};
pub use prediction::{Correction, Predicting, begin_replay, corrections, end_replay, mark_predicting, record, replay_tick};
pub use rewind::{MOST_REWIND, raycast_as_seen_by};
pub use rpcs::{Rpc, RpcTarget};
pub(crate) use scripts::register;
pub use scripts::NetDefaults;
use messages::{Message, REPEATS};

/// How a session's connections are doing, for the stats overlay: a
/// client's to its server, or a server's to its clients on average.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetStats {
    pub role: Option<NetRole>,
    /// Seconds there and back.
    pub round_trip: f64,
    /// The share of packets lost.
    pub loss: f64,
    pub sent_bytes_per_second: f64,
    pub received_bytes_per_second: f64,
    pub players: usize,
    pub tick: u64,
    /// Times this client's predicted Objs were put right by the server.
    pub corrections: u64,
}

/// A player in a session, numbered by the server: 0 is a host's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    /// Runs the game for its clients, with no player of its own.
    Server,
    /// A server that is also a player.
    Host,
    Client,
}

/// What happened in a session, for scripts to hear of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetEvent {
    /// This client joined its server.
    Connected,
    /// This client's session ended, or never began.
    Disconnected,
    PlayerJoined(PlayerId),
    PlayerLeft(PlayerId),
}

/// Who decides what a player's own Objs do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NetAuthority {
    /// The server decides everything; players' machines send what they
    /// press. Cheating is hard, so it suits games played against others.
    #[default]
    Server,
    /// Each player's machine runs their own Objs and tells the server how
    /// they are, which passes it on. Nothing waits on the server, so it
    /// suits games played together, where cheating matters less.
    Shared,
}

/// `[network]` in `voxear.toml`.
#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkSettings {
    /// Ticks a second, which the game's fixed steps follow while networked.
    pub tick_rate: u32,
    /// The port a server listens on.
    pub port: u16,
    pub max_players: usize,
    /// The most each end sends each other, in kilobytes a second.
    pub kilobytes_per_second: f64,
    pub authority: NetAuthority,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self { tick_rate: 60, port: 7777, max_players: 64, kilobytes_per_second: 64.0, authority: NetAuthority::Server }
    }
}

/// How a game starts its session, as the command line or the editor asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetStart {
    /// A dedicated server on a port; 0 for the project's.
    Server(u16),
    /// A host, a server with a player of its own, on a port; 0 for the
    /// project's.
    Host(u16),
    /// A client joining the server at this address.
    Connect(SocketAddr),
}

/// What a session is started with.
#[derive(Clone, Debug)]
pub struct NetSettings {
    /// The game and its version: ends of different games never join.
    pub protocol_id: u64,
    /// The key connect tokens are sealed with.
    pub key: Key,
    pub tick_rate: u32,
    pub port: u16,
    pub max_players: usize,
    pub bytes_per_second: f64,
    pub authority: NetAuthority,
    /// A network to test through, worse than the real one.
    pub conditions: Conditions,
}

/// The environment variable a dedicated server's private key comes from,
/// as 64 hex digits, shared only with whatever makes its connect tokens.
pub const KEY_VARIABLE: &str = "VOXEAR_NET_KEY";

impl NetSettings {
    /// A project's settings. Without a key in [`KEY_VARIABLE`], the key is
    /// one any copy of the game can work out, so a client joins a server by
    /// its address alone: fine for playing with friends and for testing, but
    /// a public server should have a key of its own and a service handing
    /// out tokens.
    pub fn for_project(name: &str, version: &str, network: &NetworkSettings) -> Self {
        let protocol_id = fnv(&[name.as_bytes(), b"\0", version.as_bytes()]);
        let key = std::env::var(KEY_VARIABLE).ok().and_then(|hex| parse_key(&hex)).unwrap_or_else(|| open_key(protocol_id));
        Self {
            protocol_id,
            key,
            tick_rate: network.tick_rate.clamp(10, 240),
            port: network.port,
            max_players: network.max_players.max(1),
            bytes_per_second: network.kilobytes_per_second.max(4.0) * 1024.0,
            authority: network.authority,
            conditions: Conditions::PERFECT,
        }
    }

    fn connection(&self) -> ConnectionConfig {
        ConnectionConfig { bytes_per_second: self.bytes_per_second, ..ConnectionConfig::default() }
    }
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    hash
}

/// The key a game's every copy works out the same, for joining by address.
fn open_key(protocol_id: u64) -> Key {
    let mut key = [0u8; 32];
    for (i, chunk) in key.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&fnv(&[&protocol_id.to_le_bytes(), &[i as u8], b"voxear open key"]).to_le_bytes());
    }
    key
}

fn parse_key(hex: &str) -> Option<Key> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

enum End {
    Server(Box<Server>),
    Client(Box<Client>),
}

/// The session in the world: a server's clients and their players, or a
/// client's server.
pub struct NetSession {
    role: NetRole,
    settings: NetSettings,
    end: End,
    tick: u64,
    now: f64,
    local: Option<PlayerId>,
    players: Vec<PlayerId>,
    events: VecDeque<NetEvent>,
    /// This tick's command of each player, the local one's too: their
    /// actions, then their aim.
    inputs: HashMap<PlayerId, Command>,
    /// Where this machine's player aims, sent with their input.
    aim: [f32; 2],
    /// Each player's aim on this tick, kept through ticks their input was
    /// lost.
    aims: HashMap<PlayerId, [f32; 2]>,
    latch: Latch,
    // The server's.
    by_client: HashMap<ClientId, PlayerId>,
    buffers: HashMap<PlayerId, InputBuffer>,
    next_player: u16,
    level: Option<String>,
    // The client's.
    sent: VecDeque<(u64, Command)>,
    welcomed: bool,
    ended: bool,
    scale: f32,
    /// Ticks after joining in which a client puts its pace right at once.
    settling: u32,
    /// The tick before which a client heeds no word of its pace, after
    /// jumping, as what the server said before the jump is still coming.
    pace_hold: u64,
    level_request: Option<String>,
    replication: Replication,
    rpcs: rpcs::Rpcs,
    prediction: prediction::Prediction,
    rewind: rewind::Rewind,
    authority: authority::Authority,
    /// The client's own commands, by tick, for predicted ticks to run again.
    commands: VecDeque<(u64, Command)>,
}

/// Ticks ahead of the server a client aims for its commands to arrive.
const AHEAD: i64 = 2;
/// The most events kept for scripts that never ask for them.
const MOST_EVENTS: usize = 256;

impl NetSession {
    fn new(role: NetRole, settings: NetSettings, end: End) -> Self {
        Self {
            role,
            settings,
            end,
            tick: 0,
            now: 0.0,
            local: None,
            players: Vec::new(),
            events: VecDeque::new(),
            inputs: HashMap::new(),
            aim: [0.0; 2],
            aims: HashMap::new(),
            latch: Latch::default(),
            by_client: HashMap::new(),
            buffers: HashMap::new(),
            next_player: 1,
            level: None,
            sent: VecDeque::new(),
            welcomed: false,
            ended: false,
            scale: 1.0,
            settling: 0,
            pace_hold: 0,
            level_request: None,
            replication: Replication::default(),
            rpcs: rpcs::Rpcs::default(),
            prediction: prediction::Prediction::default(),
            rewind: rewind::Rewind::default(),
            authority: authority::Authority::default(),
            commands: VecDeque::new(),
        }
    }

    pub fn role(&self) -> NetRole {
        self.role
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tick_rate(&self) -> u32 {
        self.settings.tick_rate
    }

    /// This machine's player: a host's or a joined client's.
    pub fn local_player(&self) -> Option<PlayerId> {
        self.local
    }

    /// Everyone playing, the local player among them.
    pub fn players(&self) -> &[PlayerId] {
        &self.players
    }

    /// Whether this client has joined; a server always has.
    pub fn is_connected(&self) -> bool {
        match &self.end {
            End::Server(_) => true,
            End::Client(_) => self.welcomed,
        }
    }

    /// Where a server listens, or a client's server.
    pub fn address(&self) -> SocketAddr {
        match &self.end {
            End::Server(server) => server.local_addr(),
            End::Client(client) => client.server(),
        }
    }

    /// A player's input this tick, by an action's place in the project's
    /// input actions.
    pub fn input(&self, player: PlayerId, action: usize) -> ActionState {
        let actions = self.inputs.get(&player).map_or(&[][..], |command| &command[..command.len().saturating_sub(1)]);
        actions.get(action).copied().unwrap_or_default()
    }

    /// Where a player aimed this tick, as their machine set it with
    /// [`set_aim`]: two numbers of the game's choosing, such as a turn and
    /// a tilt in degrees.
    pub fn aim(&self, player: PlayerId) -> [f32; 2] {
        self.aims.get(&player).copied().unwrap_or_default()
    }

    /// Notes each player's aim from this tick's commands, keeping the last
    /// where a command is missing.
    fn note_aims(&mut self) {
        for (player, command) in &self.inputs {
            if let Some(aim) = command.last() {
                self.aims.insert(*player, aim.value);
            }
        }
    }

    pub fn stats(&self) -> NetStats {
        let mut stats = NetStats {
            role: Some(self.role),
            players: self.players.len(),
            tick: self.tick,
            corrections: self.prediction.corrections,
            ..NetStats::default()
        };
        let each: Vec<voxear_net::Stats> = match &self.end {
            End::Client(client) => vec![client.stats()],
            End::Server(server) => server.clients().into_iter().filter_map(|c| server.stats(c)).collect(),
        };
        for one in &each {
            stats.sent_bytes_per_second += one.sent_bytes_per_second;
            stats.received_bytes_per_second += one.received_bytes_per_second;
            stats.round_trip += one.rtt / each.len() as f64;
            stats.loss += one.loss / each.len() as f64;
        }
        stats
    }

    /// The round trip to the server, in seconds, on a client.
    pub fn round_trip(&self) -> f64 {
        match &self.end {
            End::Client(client) => client.stats().rtt,
            End::Server(_) => 0.0,
        }
    }

    fn send(&mut self, client: ClientId, channel: Channel, message: &Message) {
        if let End::Server(server) = &mut self.end
            && let Err(error) = server.send(client, channel, &message.encode())
        {
            tracing::warn!("could not send to player {client:?}: {error:?}");
        }
    }

    fn send_all(&mut self, message: &Message, but: Option<ClientId>) {
        let clients: Vec<ClientId> = self.by_client.keys().copied().filter(|c| Some(*c) != but).collect();
        for client in clients {
            self.send(client, Channel::RELIABLE, message);
        }
    }

    fn receive_server(&mut self) {
        let End::Server(server) = &mut self.end else { return };
        server.update(self.now);
        for event in server.take_events() {
            match event {
                ServerEvent::Connected { client, .. } => self.join(client),
                ServerEvent::Disconnected { client, reason } => {
                    let Some(player) = self.by_client.remove(&client) else { continue };
                    tracing::info!("player {} left ({reason:?})", player.0);
                    self.players.retain(|p| *p != player);
                    self.buffers.remove(&player);
                    self.inputs.remove(&player);
                    self.replication.viewers.remove(&player);
                    self.rpcs.forget(player);
                    self.rewind.forget(player);
                    if self.settings.authority == NetAuthority::Shared {
                        self.authority.left.push(player);
                    }
                    self.send_all(&Message::Left(player), None);
                    self.events.push_back(NetEvent::PlayerLeft(player));
                }
            }
        }
        let End::Server(server) = &mut self.end else { return };
        for (&client, &player) in &self.by_client {
            let mut messages = Vec::new();
            while let Some(bytes) = server.receive(client, Channel::UNRELIABLE) {
                messages.extend(Message::decode(&bytes));
            }
            while let Some(bytes) = server.receive(client, Channel::RELIABLE) {
                messages.extend(Message::decode(&bytes));
            }
            for message in messages {
                match message {
                    Message::Inputs { first, commands, behind } => {
                        let newest = first + commands.len().saturating_sub(1) as u64;
                        self.rewind.saw(player, newest, newest as f64 - f64::from(behind));
                        let buffer = self.buffers.entry(player).or_default();
                        for (offset, command) in commands.into_iter().enumerate() {
                            buffer.put(first + offset as u64, command, self.tick);
                        }
                    }
                    // A player's word on their own Objs, when they run them.
                    Message::Snapshot { tick, entries } if self.settings.authority == NetAuthority::Shared => {
                        self.authority.states_from(player, tick, entries, &self.replication.objects);
                    }
                    Message::Fields { id, fields } if self.settings.authority == NetAuthority::Shared => {
                        let Some(tracked) = self.replication.objects.get_mut(&id).filter(|t| t.owner == Some(player)) else {
                            continue;
                        };
                        let entity = tracked.entity;
                        tracked.fields.extend(fields.clone());
                        tracked.fields_changed = true;
                        self.replication.field_updates.extend(fields.into_iter().map(|(key, bytes)| (entity, key, bytes)));
                    }
                    Message::Claim(id) if self.settings.authority == NetAuthority::Shared => self.authority.claims.push((id, player)),
                    Message::Rpc { id, key, tick, bytes } if self.rpcs.allow(player, self.now) => {
                        let Some(tracked) = self.replication.objects.get(&id) else { continue };
                        let rpc = Rpc { entity: tracked.entity, key, caller: Some(player), tick, bytes };
                        self.rpcs.incoming.push_back(rpc);
                    }
                    // Anything else from a client is dropped.
                    _ => {}
                }
            }
        }
    }

    fn join(&mut self, client: ClientId) {
        let player = PlayerId(self.next_player);
        self.next_player = self.next_player.wrapping_add(1).max(1);
        tracing::info!("player {} joined", player.0);
        let shared = self.settings.authority == NetAuthority::Shared;
        let welcome = Message::Welcome { player, tick: self.tick, rate: self.settings.tick_rate as u16, shared };
        self.send(client, Channel::RELIABLE, &welcome);
        for other in self.players.clone() {
            self.send(client, Channel::RELIABLE, &Message::Joined(other));
        }
        if let Some(level) = self.level.clone() {
            self.send(client, Channel::RELIABLE, &Message::Level(level));
        }
        self.send_all(&Message::Joined(player), None);
        self.by_client.insert(client, player);
        self.players.push(player);
        self.buffers.insert(player, InputBuffer::default());
        self.replication.viewers.insert(player, Viewer::default());
        self.events.push_back(NetEvent::PlayerJoined(player));
    }

    fn receive_client(&mut self) {
        let End::Client(client) = &mut self.end else { return };
        client.update(self.now);
        let state = client.state();
        let mut messages = Vec::new();
        while let Some(bytes) = client.receive(Channel::RELIABLE) {
            messages.extend(Message::decode(&bytes));
        }
        while let Some(bytes) = client.receive(Channel::UNRELIABLE) {
            messages.extend(Message::decode(&bytes));
        }
        let rtt = client.stats().rtt;
        for message in messages {
            match message {
                Message::Welcome { player, tick, rate, shared } => {
                    self.settings.authority = if shared { NetAuthority::Shared } else { NetAuthority::Server };
                    // Ahead by half the round trip and a little more, which syncing then trims.
                    let ahead = (rtt * 0.5 * f64::from(rate)).ceil() as u64 + AHEAD as u64;
                    (self.local, self.tick, self.welcomed) = (Some(player), tick + ahead, true);
                    self.settling = rate as u32;
                    self.settings.tick_rate = u32::from(rate);
                    self.players.push(player);
                    self.events.push_back(NetEvent::Connected);
                }
                Message::Joined(player) if !self.players.contains(&player) => {
                    self.players.push(player);
                    self.events.push_back(NetEvent::PlayerJoined(player));
                }
                Message::Left(player) => {
                    self.players.retain(|p| *p != player);
                    self.events.push_back(NetEvent::PlayerLeft(player));
                }
                Message::Level(level) => self.level_request = Some(level),
                Message::Sync { tick, ahead } if ahead != messages::UNHEARD => self.keep_pace(tick, i64::from(ahead), rtt),
                Message::Spawn { id, owner, origin, tick, state, fields } => {
                    self.replication.note(id, tick, state);
                    self.replication.early_fields.insert(id, fields);
                    self.replication.spawns.push(SpawnRequest { id, origin, owner, state });
                }
                Message::Despawn(id) => {
                    if let Some(entity) = self.replication.remote.remove(&id).and_then(|remote| remote.entity) {
                        self.replication.despawns.push(entity);
                    }
                }
                Message::Snapshot { tick, entries } => self.take_snapshot(tick, entries),
                Message::Owner { id, owner } => self.authority.owners.push((id, owner)),
                Message::Rpc { id, key, tick, bytes } => match self.replication.remote.get(&id).and_then(|r| r.entity) {
                    Some(entity) => self.rpcs.incoming.push_back(Rpc { entity, key, caller: None, tick, bytes }),
                    None => self.rpcs.early.push((self.now, id, key, tick, bytes)),
                },
                _ => {}
            }
        }
        if matches!(state, ClientState::Failed(_) | ClientState::Disconnected(_)) && !self.ended {
            tracing::info!("left the session: {state:?}");
            self.events.push_back(NetEvent::Disconnected);
            (self.welcomed, self.ended) = (false, true);
        }
    }

    /// Takes in a snapshot: each Obj's parts that changed, over its last
    /// state, noted for its tick.
    fn take_snapshot(&mut self, tick: u64, entries: Vec<(NetId, u8, ObjState, Option<replication::Fields>)>) {
        let replication = &mut self.replication;
        // How unevenly snapshots come: their spacing against a tick's, smoothed.
        if let Some(last) = replication.last_arrival {
            let spacing = (self.now - last) * f64::from(self.settings.tick_rate);
            let late = (spacing - (tick.saturating_sub(replication.newest)) as f64).abs();
            replication.jitter += (late - replication.jitter) * 0.1;
        }
        replication.last_arrival = Some(self.now);
        replication.newest = replication.newest.max(tick);
        for (id, mask, parts, fields) in entries {
            // Fields come in at once, for scripts to take in; those for an Obj not made yet wait for it,
            // and a predicted Obj's are kept for when it is put right.
            if let Some(fields) = fields {
                match replication.remote.get(&id).and_then(|r| r.entity) {
                    _ if self.prediction.ids.contains(&id) => self.prediction.fields.entry(id).or_default().extend(fields),
                    Some(entity) => replication.field_updates.extend(fields.into_iter().map(|(k, v)| (entity, k, v))),
                    None => drop(replication.early_fields.insert(id, fields)),
                }
            }
            let base = replication.remote.get(&id).and_then(|r| r.samples.back()).map_or(messages::BLANK, |(_, s)| *s);
            let state = ObjState {
                position: if mask & 1 != 0 { parts.position } else { base.position },
                rotation: if mask & 2 != 0 { parts.rotation } else { base.rotation },
                scale: if mask & 4 != 0 { parts.scale } else { base.scale },
                mist: if mask & 8 != 0 { parts.mist } else { base.mist },
            };
            replication.note(id, tick, state);
        }
    }

    /// Moves the server's tick this player's machine shows on by `delta`
    /// seconds, a little behind the newest snapshot: two ticks and as much
    /// again as snapshots come unevenly, so one or two late or lost still
    /// leave a pair to move between.
    fn advance_shown(&mut self, delta: f32) {
        let replication = &mut self.replication;
        if replication.newest == 0 {
            return;
        }
        let delay = (2.0 + 2.0 * replication.jitter).clamp(2.0, 8.0);
        let target = replication.newest as f64 - delay;
        replication.shown += f64::from(delta) * f64::from(self.settings.tick_rate);
        let off = target - replication.shown;
        if off.abs() > 10.0 {
            replication.shown = target;
        } else {
            replication.shown += off * 0.05;
        }
    }

    /// The server's tick: what changed on every networked Obj, and each
    /// player's share of it, the Objs to make and take away on their
    /// machine and a snapshot of as much as fits.
    fn replicate(&mut self, world: &World) {
        let End::Server(server) = &mut self.end else { return };
        let replication = &mut self.replication;
        let gone = replication.capture(world);
        let outgoing = std::mem::take(&mut self.rpcs.outgoing);
        let owners = std::mem::take(&mut self.authority.changed_owners);
        let shared = self.settings.authority == NetAuthority::Shared;
        let tick = self.tick;
        for (&client, player) in &self.by_client {
            let at = replication.viewpoint(*player);
            let Some(viewer) = replication.viewers.get_mut(player) else { continue };
            // What a lost snapshot carried goes again, as it is now.
            for delivery in server.take_deliveries(client) {
                let voxear_net::Delivery::Lost(tag) = delivery else {
                    if let voxear_net::Delivery::Acked(tag) = delivery {
                        viewer.carried.remove(&tag);
                    }
                    continue;
                };
                for (id, mask) in viewer.carried.remove(&tag).unwrap_or_default() {
                    if let Some((pending, _)) = viewer.known.get_mut(&id) {
                        *pending |= mask;
                    }
                }
            }
            let mut reliable = Vec::new();
            for id in &gone {
                if viewer.known.remove(id).is_some() {
                    reliable.push(Message::Despawn(*id));
                }
            }
            let mut spawned = 0;
            for (&id, tracked) in &replication.objects {
                let near = at.is_none_or(|at| at.distance(tracked.state.position) <= tracked.object.range);
                let wanted = tracked.object.range <= 0.0 || near || tracked.owner == Some(*player);
                match (wanted, viewer.known.get_mut(&id)) {
                    (true, None) if spawned < replication::SPAWNS_A_TICK => {
                        spawned += 1;
                        viewer.known.insert(id, (0, 0.0));
                        let (owner, origin, state) = (tracked.owner, tracked.origin.clone(), tracked.state);
                        let fields = tracked.fields.clone();
                        reliable.push(Message::Spawn { id, owner, origin, tick, state, fields });
                    }
                    (true, Some(_)) if shared && tracked.owner == Some(*player) => {}
                    (true, Some((pending, waited))) => {
                        *pending |= tracked.changed;
                        if *pending != 0 {
                            // Nearer counts for more: full within ten metres, a fifth beyond fifty.
                            let distance = at.map_or(0.0, |at| at.distance(tracked.state.position));
                            let near = (1.0 - (distance - 10.0) / 40.0).clamp(0.2, 1.0);
                            *waited += tracked.object.priority.max(0.01) * near;
                            // A player's own go first, every tick, for their machine to check its predictions.
                            if tracked.owner == Some(*player) {
                                *waited += 1000.0;
                            }
                        }
                    }
                    (false, Some(_)) => {
                        viewer.known.remove(&id);
                        reliable.push(Message::Despawn(id));
                    }
                    _ => {}
                }
            }
            for message in reliable {
                let _ = server.send(client, Channel::RELIABLE, &message.encode());
            }
            for &(id, owner) in &owners {
                if viewer.known.contains_key(&id) {
                    let _ = server.send(client, Channel::RELIABLE, &Message::Owner { id, owner }.encode());
                }
            }
            // RPCs reach those who have the Obj, just made or not.
            for rpc in &outgoing {
                let to_them = rpc.target == RpcTarget::Players || rpc.target == RpcTarget::Player(*player);
                if to_them && viewer.known.contains_key(&rpc.id) {
                    let message = Message::Rpc { id: rpc.id, key: rpc.key, tick, bytes: rpc.bytes.clone() };
                    let channel = if rpc.reliable { Channel::RELIABLE } else { Channel::UNRELIABLE };
                    let _ = server.send(client, channel, &message.encode());
                }
            }
            let snapshot = snapshot(tick, viewer, &replication.objects);
            if let Some((bytes, carried)) = snapshot {
                let tag = viewer.next_tag;
                viewer.next_tag = viewer.next_tag.wrapping_add(1);
                viewer.carried.insert(tag, carried);
                let _ = server.send_tagged(client, Channel::UNRELIABLE, &bytes, tag);
            }
        }
    }

    /// Runs a little faster while commands arrive late, a little slower
    /// while they arrive too early, and jumps when far out, or when late
    /// just after joining, which has only a guess to start from.
    fn keep_pace(&mut self, server_tick: u64, ahead: i64, rtt: f64) {
        if !self.welcomed || self.tick < self.pace_hold {
            return;
        }
        let late = self.settling > 0 && ahead < 1;
        if late || !(-10..=30).contains(&ahead) {
            let behind = ahead - AHEAD;
            self.tick = (self.tick as i64 - behind).max(server_tick as i64) as u64;
            self.scale = 1.0;
            let round_trip = (rtt * f64::from(self.settings.tick_rate)).ceil() as u64;
            self.pace_hold = self.tick + round_trip + 4;
            return;
        }
        self.scale = match ahead {
            a if a < AHEAD => 1.05,
            a if a > AHEAD + 1 => 0.95,
            _ => 1.0,
        };
    }

    fn fixed_tick(&mut self) {
        if !self.is_connected() {
            return;
        }
        self.tick += 1;
        match self.role {
            NetRole::Server | NetRole::Host => {
                let tick = self.tick;
                for (player, buffer) in &mut self.buffers {
                    self.inputs.insert(*player, buffer.take(tick));
                }
                if let Some(local) = self.local {
                    self.inputs.insert(local, self.latch.take());
                }
                self.note_aims();
            }
            NetRole::Client => {
                self.settling = self.settling.saturating_sub(1);
                let command = self.latch.take();
                if let Some(local) = self.local {
                    self.inputs.insert(local, command.clone());
                    self.aims.insert(local, command.last().map_or(self.aim, |aim| aim.value));
                }
                self.sent.push_back((self.tick, command.clone()));
                while self.sent.len() > REPEATS {
                    self.sent.pop_front();
                }
                self.commands.push_back((self.tick, command));
                while self.commands.len() > prediction::HISTORY {
                    self.commands.pop_front();
                }
            }
        }
    }

    fn flush(&mut self) {
        match &mut self.end {
            End::Server(server) => {
                let tick = self.tick;
                for (&client, player) in &self.by_client {
                    let buffer = self.buffers.get(player).filter(|b| b.heard());
                    let ahead = buffer.map_or(messages::UNHEARD, |b| b.ahead(tick).clamp(-30_000, 30_000) as i16);
                    let sync = Message::Sync { tick, ahead };
                    let _ = server.send(client, Channel::UNRELIABLE, &sync.encode());
                }
                server.flush(self.now);
            }
            End::Client(client) => {
                if self.welcomed && client.is_connected() && !self.sent.is_empty() {
                    let first = self.sent.front().map_or(0, |(tick, _)| *tick);
                    let commands = self.sent.iter().map(|(_, command)| command.clone()).collect();
                    // What the player sees, as far behind their newest command as the server's world shows.
                    let newest = self.sent.back().map_or(0, |(tick, _)| *tick);
                    let behind = (newest as f64 - self.replication.shown).clamp(0.0, 63.0) as f32;
                    let _ = client.send(Channel::UNRELIABLE, &Message::Inputs { first, commands, behind }.encode());
                }
                client.flush(self.now);
            }
        }
    }
}

/// A player's snapshot this tick: the Objs that changed, the longest
/// waiting first, as many as fit; with what it carried. `None` when
/// nothing changed.
fn snapshot(tick: u64, viewer: &mut Viewer, objects: &std::collections::BTreeMap<NetId, Tracked>) -> Option<(Vec<u8>, Vec<(NetId, u8)>)> {
    let mut waiting: Vec<(NetId, f32)> =
        viewer.known.iter().filter(|(_, (pending, _))| *pending != 0).map(|(id, (_, waited))| (*id, *waited)).collect();
    if waiting.is_empty() {
        return None;
    }
    waiting.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut w = voxear_net::BitWriter::new();
    w.write_bits(messages::SNAPSHOT, messages::KIND_BITS);
    w.write_varint(tick);
    let mut carried = Vec::new();
    for (id, _) in waiting {
        let Some(tracked) = objects.get(&id) else { continue };
        let Some((pending, waited)) = viewer.known.get_mut(&id) else { continue };
        let before = w.clone();
        w.write_bool(true);
        w.write_varint(u64::from(id.0));
        replication::write_state(&mut w, &tracked.state, *pending);
        if *pending & replication::FIELDS != 0 {
            replication::write_fields(&mut w, &tracked.fields);
        }
        if w.bits() / 8 + 1 > replication::SNAPSHOT_BYTES {
            // Full: this one waits, and gains in priority meanwhile.
            w = before;
            break;
        }
        carried.push((id, *pending));
        (*pending, *waited) = (0, 0.0);
    }
    w.write_bool(false);
    (!carried.is_empty()).then(|| (w.finish(), carried))
}

/// Starts serving on `port` (0 lets the system choose), as a host with a
/// player of its own or as a server without, and gives back where it
/// listens.
pub fn serve(world: &mut World, settings: NetSettings, port: u16, host: bool) -> Result<SocketAddr> {
    let transport = UdpTransport::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .with_context(|| format!("listening on port {port}"))?;
    serve_on(world, settings, Box::new(transport), host)
}

/// [`serve`] through any transport, as tests do.
pub fn serve_on(world: &mut World, settings: NetSettings, transport: Box<dyn Transport>, host: bool) -> Result<SocketAddr> {
    let transport = simulated(transport, &settings);
    let config = ServerConfig {
        protocol_id: settings.protocol_id,
        private_key: settings.key,
        max_clients: settings.max_players,
        // Tokens may name any address: clients join by whichever reaches this machine.
        public_addresses: Vec::new(),
        connection: settings.connection(),
    };
    let server = Server::new(config, transport, 0.0);
    let address = server.local_addr();
    let role = if host { NetRole::Host } else { NetRole::Server };
    let mut session = NetSession::new(role, settings, End::Server(Box::new(server)));
    if host {
        session.local = Some(PlayerId(0));
        session.players.push(PlayerId(0));
    }
    tracing::info!("{} on {address}", if host { "hosting" } else { "serving" });
    world.insert_resource(session);
    Ok(address)
}

/// Joins the server at `address` with a token made here, which a server
/// keyed as this game is opens.
pub fn connect(world: &mut World, settings: NetSettings, address: SocketAddr) -> Result<()> {
    let local = if address.is_ipv6() { SocketAddr::from(([0u16; 8], 0)) } else { SocketAddr::from(([0, 0, 0, 0], 0)) };
    let transport = UdpTransport::bind(local).context("opening a socket")?;
    connect_on(world, settings, address, Box::new(transport))
}

/// [`connect`] through any transport, as tests do.
pub fn connect_on(world: &mut World, settings: NetSettings, address: SocketAddr, transport: Box<dyn Transport>) -> Result<()> {
    let id: [u8; 8] = voxear_net::random_key()[..8].try_into().expect("8 bytes");
    let token = ConnectToken::generate(TokenRequest {
        protocol_id: settings.protocol_id,
        private_key: &settings.key,
        client_id: u64::from_le_bytes(id),
        user_data: [0; USER_DATA_BYTES],
        servers: vec![address],
        expire_seconds: 30,
        timeout_seconds: 5,
        now_unix: unix_seconds() as u64,
    })
    .map_err(|error| anyhow::anyhow!("making a connect token: {error:?}"))?;
    join(world, settings, token, transport);
    Ok(())
}

/// Joins with a token from elsewhere, such as a matchmaker's.
pub fn join(world: &mut World, settings: NetSettings, token: ConnectToken, transport: Box<dyn Transport>) {
    let transport = simulated(transport, &settings);
    let client = Client::new(token, transport, settings.connection(), 0.0);
    tracing::info!("joining {}", client.server());
    world.insert_resource(NetSession::new(NetRole::Client, settings, End::Client(Box::new(client))));
}

fn simulated(transport: Box<dyn Transport>, settings: &NetSettings) -> Box<dyn Transport> {
    match settings.conditions == Conditions::PERFECT {
        true => transport,
        false => Box::new(Simulated::new(transport, settings.conditions, settings.protocol_id ^ 0x5eed)),
    }
}

/// A session ended, by [`stop`] or the game closing, tells the other end.
impl Drop for NetSession {
    fn drop(&mut self) {
        match &mut self.end {
            End::Server(server) => server.shut_down(),
            End::Client(client) => client.disconnect(),
        }
    }
}

/// Ends the session, telling the other end.
pub fn stop(world: &mut World) {
    world.remove_resource::<NetSession>();
}

/// Takes in what arrived, and the local player's input this frame, `delta`
/// seconds long; on a client, networked Objs move to where they were a
/// moment ago on the server.
pub fn frame_begin(world: &mut World, delta: f32, local: &[ActionState]) {
    let Some(mut session) = world.remove_resource::<NetSession>() else { return };
    prediction::unshow(world, delta, &mut session.prediction);
    session.now += f64::from(delta);
    // The aim goes last, after the actions, as one more value.
    let mut frame = local.to_vec();
    frame.push(ActionState { value: session.aim, ..ActionState::default() });
    session.latch.note(&frame);
    session.receive_server();
    session.receive_client();
    session.rpcs.expire(session.now);
    while session.events.len() > MOST_EVENTS {
        session.events.pop_front();
    }
    if session.role == NetRole::Client {
        session.advance_shown(delta);
        let theirs: std::collections::HashSet<NetId> = session.prediction.ids.union(&session.authority.owned).copied().collect();
        session.replication.show(world, &theirs);
    }
    authority::take_in(world, &mut session);
    world.insert_resource(session);
}

/// Moves the session on a tick, before the tick's FixedUpdate: the server
/// takes each player's command for it, a client makes its own.
pub fn fixed_tick(world: &mut World) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.fixed_tick();
    }
}

/// Sends what is waiting: on a server, each player's share of what
/// changed on the networked Objs.
pub fn frame_end(world: &mut World) {
    let Some(mut session) = world.remove_resource::<NetSession>() else { return };
    authority::send_owned(world, &mut session);
    session.replicate(world);
    session.flush();
    prediction::show(world, &session.prediction);
    world.insert_resource(session);
}

/// Shares an Obj with the session's players, on a server: `origin` says
/// how their machines find or make their own. Gives back its id.
pub fn track(world: &mut World, entity: voxear_ecs::Entity, origin: Origin) -> Option<NetId> {
    let state = ObjState::of(world, entity)?;
    let object = world.get::<NetObject>(entity).copied().unwrap_or_default();
    let session = world.resource_mut::<NetSession>().filter(|s| matches!(s.end, End::Server(_)))?;
    let replication = &mut session.replication;
    replication.next_id += 1;
    let id = NetId(replication.next_id);
    // An owner and fields given before it was shared.
    let owner = world.remove::<WaitingOwner>(entity).and_then(|waiting| waiting.0);
    let fields = world.remove::<WaitingFields>(entity).map(|waiting| waiting.0).unwrap_or_default();
    let session = world.resource_mut::<NetSession>()?;
    let tracked = Tracked { entity, origin, owner, object, state, changed: 0, fields_changed: false, fields };
    session.replication.objects.insert(id, tracked);
    world.insert(entity, Networked { id, owner });
    Some(id)
}

/// Replicated fields a networked Obj's scripts wrote before the server
/// shared it, kept until it does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaitingFields(pub replication::Fields);

impl voxear_ecs::Component for WaitingFields {}

/// Takes a behaviour's replicated fields from its script, on a server, to
/// send to players: the behaviour's type by a key, its fields as bytes.
pub fn set_fields(world: &mut World, entity: voxear_ecs::Entity, key: u32, bytes: Vec<u8>) {
    let Some(id) = world.get::<Networked>(entity).map(|n| n.id) else {
        // Not shared yet: kept until it is.
        if world.has::<NetObject>(entity) {
            let mut waiting = world.get::<WaitingFields>(entity).cloned().unwrap_or_default();
            waiting.0.insert(key, bytes);
            world.insert(entity, waiting);
        }
        return;
    };
    // A player's own, from their machine, when players run their own.
    if let Some(session) = world.resource_mut::<NetSession>().filter(|s| matches!(s.end, End::Client(_))) {
        authority::send_fields(session, id, key, bytes);
        return;
    }
    let tracked = world.resource_mut::<NetSession>().and_then(|s| s.replication.objects.get_mut(&id));
    if let Some(tracked) = tracked
        && tracked.fields.get(&key) != Some(&bytes)
    {
        tracked.fields.insert(key, bytes);
        tracked.fields_changed = true;
    }
}

/// Behaviours' replicated fields that came from the server, for scripts to
/// take in: each Obj, its behaviour's type by a key, and its fields.
pub fn take_field_updates(world: &mut World) -> Vec<(voxear_ecs::Entity, u32, Vec<u8>)> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.replication.field_updates)).unwrap_or_default()
}

/// An owner given to a networked Obj before the server shared it, kept
/// until it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitingOwner(pub Option<PlayerId>);

impl voxear_ecs::Component for WaitingOwner {}

/// Gives a networked Obj to a player, on a server: theirs is sent to them
/// wherever it is, and where it is counts as where they are.
pub fn set_owner(world: &mut World, entity: voxear_ecs::Entity, owner: Option<PlayerId>) {
    let Some(networked) = world.get_mut::<Networked>(entity) else {
        world.insert(entity, WaitingOwner(owner));
        return;
    };
    let changed = networked.owner != owner;
    networked.owner = owner;
    let id = networked.id;
    if let Some(session) = world.resource_mut::<NetSession>()
        && let Some(tracked) = session.replication.objects.get_mut(&id)
    {
        tracked.owner = owner;
        if changed {
            session.authority.changed_owners.push((id, owner));
        }
    }
}

/// The Objs the server told this client to make, for the game to find or
/// make and [`bind`].
pub fn take_spawns(world: &mut World) -> Vec<SpawnRequest> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.replication.spawns)).unwrap_or_default()
}

/// The Objs the server took away, for the game to destroy.
pub fn take_despawns(world: &mut World) -> Vec<voxear_ecs::Entity> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.replication.despawns)).unwrap_or_default()
}

/// Ties a networked Obj's id to the Obj found or made for it on this
/// client. Physics no longer moves it here: the server does.
pub fn bind(world: &mut World, request: &SpawnRequest, entity: voxear_ecs::Entity) {
    world.insert(entity, Networked { id: request.id, owner: request.owner });
    authority::hold_body(world, entity);
    if let Some(session) = world.resource_mut::<NetSession>() {
        let replication = &mut session.replication;
        if let Some(remote) = replication.remote.get_mut(&request.id) {
            remote.entity = Some(entity);
        }
        let early = replication.early_fields.remove(&request.id).unwrap_or_default();
        replication.field_updates.extend(early.into_iter().map(|(key, bytes)| (entity, key, bytes)));
        session.rpcs.arrived(request.id, entity);
    }
}

/// Sends an RPC: a method of a networked Obj's behaviour, by a key, with
/// its arguments as bytes, to run on the server from a player's machine,
/// or on players' machines from the server. A host runs its own at home.
/// Gives back whether it went.
pub fn rpc(world: &mut World, entity: voxear_ecs::Entity, key: u32, bytes: Vec<u8>, target: RpcTarget, reliable: bool) -> bool {
    let Some(id) = world.get::<Networked>(entity).map(|n| n.id).filter(|id| id.0 != 0) else {
        tracing::warn!(target: "script", "an RPC on an Obj not shared yet, or without a Net Object, goes nowhere");
        return false;
    };
    let Some(session) = world.resource_mut::<NetSession>() else { return false };
    // Ticks run again were run once already, RPCs and all.
    if session.prediction.replaying() {
        return false;
    }
    if bytes.len() > messages::MOST_RPC_BYTES {
        tracing::error!(target: "script", "an RPC's arguments come to more than {} bytes", messages::MOST_RPC_BYTES);
        return false;
    }
    let reliable = reliable || bytes.len() > rpcs::MOST_UNRELIABLE_BYTES;
    let tick = session.tick;
    match (&mut session.end, target) {
        (End::Client(client), RpcTarget::Server) => {
            let channel = if reliable { Channel::RELIABLE } else { Channel::UNRELIABLE };
            client.send(channel, &Message::Rpc { id, key, tick, bytes }.encode()).is_ok()
        }
        (End::Client(_), _) => {
            tracing::warn!(target: "script", "only the server runs RPCs on players' machines");
            false
        }
        (End::Server(_), RpcTarget::Server) => {
            let Some(local) = session.local else {
                tracing::warn!(target: "script", "a dedicated server has no player to make an RPC to itself");
                return false;
            };
            session.rpcs.incoming.push_back(Rpc { entity, key, caller: Some(local), tick, bytes });
            true
        }
        (End::Server(_), target) => {
            if session.local.is_some_and(|local| target == RpcTarget::Players || target == RpcTarget::Player(local)) {
                session.rpcs.incoming.push_back(Rpc { entity, key, caller: None, tick, bytes: bytes.clone() });
            }
            session.rpcs.outgoing.push(rpcs::Outgoing { id, key, target, reliable, bytes });
            true
        }
    }
}

/// The next RPC that came, for scripts to run.
pub fn take_rpc(world: &mut World) -> Option<Rpc> {
    let session = world.resource_mut::<NetSession>()?;
    let rpc = session.rpcs.incoming.pop_front()?;
    session.rpcs.tick = rpc.tick;
    Some(rpc)
}

/// Keeps what this tick left: on a client, where its predicted Objs are;
/// on a server, where every shared Obj is, for rays as players saw them.
pub fn after_tick(world: &mut World) {
    prediction::record(world);
    rewind::remember(world);
}

/// Sets where this machine's player aims, sent with their input each tick
/// for the server to read as [`NetSession::aim`].
pub fn set_aim(world: &mut World, aim: [f32; 2]) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.aim = aim;
    }
}

/// Whether a client is running its predicted ticks again.
pub fn replaying(world: &World) -> bool {
    world.resource::<NetSession>().is_some_and(|session| session.prediction.replaying())
}

/// The tick the RPC scripts took last was made on.
pub fn rpc_tick(world: &World) -> u64 {
    world.resource::<NetSession>().map_or(0, |session| session.rpcs.tick)
}

/// The networked Obj with `id` on this machine.
pub fn entity_of(world: &World, id: NetId) -> Option<voxear_ecs::Entity> {
    let session = world.resource::<NetSession>()?;
    match &session.end {
        End::Server(_) => session.replication.objects.get(&id).map(|tracked| tracked.entity),
        End::Client(_) => session.replication.remote.get(&id).and_then(|remote| remote.entity),
    }
}

/// How fast a client's ticks run against real time to keep its pace.
pub fn time_scale(world: &World) -> f32 {
    world.resource::<NetSession>().map_or(1.0, |session| session.scale)
}

/// The level the server plays, for a server to tell clients that join.
pub fn set_level(world: &mut World, level: Option<&str>) {
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    if session.level.as_deref() == level || !matches!(session.end, End::Server(_)) {
        return;
    }
    session.level = level.map(str::to_string);
    if let Some(level) = level {
        session.send_all(&Message::Level(level.to_string()), None);
    }
}

/// A level the server told this client to load, once.
pub fn take_level_request(world: &mut World) -> Option<String> {
    world.resource_mut::<NetSession>()?.level_request.take()
}

/// What happened since last asked.
pub fn take_events(world: &mut World) -> Vec<NetEvent> {
    world.resource_mut::<NetSession>().map(|session| session.events.drain(..).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
