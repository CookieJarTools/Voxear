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

mod inputs;
mod messages;
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
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self { tick_rate: 60, port: 7777, max_players: 64, kilobytes_per_second: 64.0 }
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
    /// This tick's command of each player, the local one's too.
    inputs: HashMap<PlayerId, Command>,
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
    level_request: Option<String>,
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
            latch: Latch::default(),
            by_client: HashMap::new(),
            buffers: HashMap::new(),
            next_player: 1,
            level: None,
            sent: VecDeque::new(),
            welcomed: false,
            ended: false,
            scale: 1.0,
            level_request: None,
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
        self.inputs.get(&player).and_then(|command| command.get(action)).copied().unwrap_or_default()
    }

    pub fn stats(&self) -> NetStats {
        let mut stats = NetStats { role: Some(self.role), players: self.players.len(), tick: self.tick, ..NetStats::default() };
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
                    self.send_all(&Message::Left(player), None);
                    self.events.push_back(NetEvent::PlayerLeft(player));
                }
            }
        }
        let End::Server(server) = &mut self.end else { return };
        for (&client, &player) in &self.by_client {
            while let Some(bytes) = server.receive(client, Channel::UNRELIABLE) {
                if let Some(Message::Inputs { first, commands }) = Message::decode(&bytes) {
                    let buffer = self.buffers.entry(player).or_default();
                    for (offset, command) in commands.into_iter().enumerate() {
                        buffer.put(first + offset as u64, command, self.tick);
                    }
                }
            }
            // Clients have nothing to say reliably yet; anything there is dropped.
            while server.receive(client, Channel::RELIABLE).is_some() {}
        }
    }

    fn join(&mut self, client: ClientId) {
        let player = PlayerId(self.next_player);
        self.next_player = self.next_player.wrapping_add(1).max(1);
        tracing::info!("player {} joined", player.0);
        let welcome = Message::Welcome { player, tick: self.tick, rate: self.settings.tick_rate as u16 };
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
                Message::Welcome { player, tick, rate } => {
                    // Ahead by half the round trip and a little more, which syncing then trims.
                    let ahead = (rtt * 0.5 * f64::from(rate)).ceil() as u64 + AHEAD as u64;
                    (self.local, self.tick, self.welcomed) = (Some(player), tick + ahead, true);
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
                Message::Sync { tick, ahead } => self.keep_pace(tick, i64::from(ahead)),
                _ => {}
            }
        }
        if matches!(state, ClientState::Failed(_) | ClientState::Disconnected(_)) && !self.ended {
            tracing::info!("left the session: {state:?}");
            self.events.push_back(NetEvent::Disconnected);
            (self.welcomed, self.ended) = (false, true);
        }
    }

    /// Runs a little faster while commands arrive late, a little slower
    /// while they arrive too early, and jumps when far out.
    fn keep_pace(&mut self, server_tick: u64, ahead: i64) {
        if !self.welcomed {
            return;
        }
        if !(-10..=30).contains(&ahead) {
            let behind = ahead - AHEAD;
            self.tick = (self.tick as i64 - behind).max(server_tick as i64) as u64;
            self.scale = 1.0;
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
            }
            NetRole::Client => {
                let command = self.latch.take();
                if let Some(local) = self.local {
                    self.inputs.insert(local, command.clone());
                }
                self.sent.push_back((self.tick, command));
                while self.sent.len() > REPEATS {
                    self.sent.pop_front();
                }
            }
        }
    }

    fn flush(&mut self) {
        match &mut self.end {
            End::Server(server) => {
                let tick = self.tick;
                for (&client, player) in &self.by_client {
                    let ahead = self.buffers.get(player).map_or(0, |b| b.ahead(tick)).clamp(-30_000, 30_000);
                    let sync = Message::Sync { tick, ahead: ahead as i16 };
                    let _ = server.send(client, Channel::UNRELIABLE, &sync.encode());
                }
                server.flush(self.now);
            }
            End::Client(client) => {
                if self.welcomed && client.is_connected() && !self.sent.is_empty() {
                    let first = self.sent.front().map_or(0, |(tick, _)| *tick);
                    let commands = self.sent.iter().map(|(_, command)| command.clone()).collect();
                    let _ = client.send(Channel::UNRELIABLE, &Message::Inputs { first, commands }.encode());
                }
                client.flush(self.now);
            }
        }
    }
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
/// seconds long.
pub fn frame_begin(world: &mut World, delta: f32, local: &[ActionState]) {
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    session.now += f64::from(delta);
    session.latch.note(local);
    session.receive_server();
    session.receive_client();
    while session.events.len() > MOST_EVENTS {
        session.events.pop_front();
    }
}

/// Moves the session on a tick, before the tick's FixedUpdate: the server
/// takes each player's command for it, a client makes its own.
pub fn fixed_tick(world: &mut World) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.fixed_tick();
    }
}

/// Sends what is waiting.
pub fn frame_end(world: &mut World) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.flush();
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
