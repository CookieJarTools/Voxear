//! The server's side: who may join, joining, and each client's connection.
//!
//! A client joins in two steps. It sends its token's sealed private part in
//! a connection request; the server opens it, checks it is for this server
//! and unexpired and not already used from elsewhere, and answers with a
//! challenge only it could have sealed. The client sends the challenge
//! back, which proves the packets really come from where they say, and only
//! then does it get a slot. Until then the server keeps nothing but a short
//! note of the address, and answers nothing with more bytes than it got.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use crate::connection::{Channel, Connection, ConnectionConfig, Delivery, SendError, Stats};
use crate::crypto::{Key, random_key};
use crate::packet::{
    Kind, ReplayWindow, kind_of, open_challenge, read_request, read_sealed, seal_challenge, write_sealed,
};
use crate::token::{PrivateToken, USER_DATA_BYTES};
use crate::transport::{MAX_DATAGRAM, Transport};

/// How often an idle end sends a packet to show it is still there.
pub(crate) const KEEP_ALIVE: f64 = 0.1;
/// How many connection requests a second one address may send.
const REQUESTS_PER_SECOND: u32 = 20;

/// A client, as the token named it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Nothing came from the other end for its timeout.
    TimedOut,
    /// The other end said it was leaving.
    Closed,
    /// This end chose to end it.
    Kicked,
    /// It sent more than a channel could hold.
    ChannelFull,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerEvent {
    Connected { client: ClientId, user_data: Box<[u8; USER_DATA_BYTES]> },
    Disconnected { client: ClientId, reason: DisconnectReason },
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// The game and its version; tokens must say the same.
    pub protocol_id: u64,
    /// The key tokens are sealed with, shared with whatever makes them.
    pub private_key: Key,
    pub max_clients: usize,
    /// Where clients reach this server; a token must name one of these.
    /// None lets a token name any, for a game whose clients join by address
    /// with tokens they make themselves.
    pub public_addresses: Vec<SocketAddr>,
    pub connection: ConnectionConfig,
}

struct Slot {
    id: ClientId,
    address: SocketAddr,
    send_key: Key,
    receive_key: Key,
    sequence: u64,
    replay: ReplayWindow,
    connection: Connection,
    last_received: f64,
    last_sent: f64,
    timeout: f64,
    user_data: Box<[u8; USER_DATA_BYTES]>,
}

/// An address that has asked to join and been challenged.
struct Pending {
    send_key: Key,
    receive_key: Key,
    timeout: f64,
    expires_at: f64,
    sequence: u64,
}

pub struct Server {
    config: ServerConfig,
    transport: Box<dyn Transport>,
    challenge_key: Key,
    challenge_sequence: u64,
    slots: Vec<Option<Slot>>,
    by_address: HashMap<SocketAddr, usize>,
    pending: HashMap<SocketAddr, Pending>,
    /// Tokens used, by their seal, and from where, until they expire.
    used: HashMap<[u8; 16], (SocketAddr, f64)>,
    requests: HashMap<IpAddr, (f64, u32)>,
    events: Vec<ServerEvent>,
    now: f64,
    unix_start: f64,
    started: f64,
}

impl Server {
    pub fn new(config: ServerConfig, transport: Box<dyn Transport>, now: f64) -> Self {
        let slots = (0..config.max_clients).map(|_| None).collect();
        Self {
            config,
            transport,
            challenge_key: random_key(),
            challenge_sequence: 0,
            slots,
            by_address: HashMap::new(),
            pending: HashMap::new(),
            used: HashMap::new(),
            requests: HashMap::new(),
            events: Vec::new(),
            now,
            unix_start: unix_seconds(),
            started: now,
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.transport.local_addr()
    }

    fn unix_now(&self) -> u64 {
        (self.unix_start + (self.now - self.started)) as u64
    }

    /// Takes in what has arrived, and drops clients gone quiet.
    pub fn update(&mut self, now: f64) {
        self.now = now;
        self.transport.update(now);
        let mut buffer = [0u8; MAX_DATAGRAM + 64];
        while let Some((length, from)) = self.transport.receive(&mut buffer) {
            self.take(&buffer[..length], from);
        }
        for index in 0..self.slots.len() {
            let Some(slot) = &self.slots[index] else { continue };
            if now - slot.last_received > slot.timeout {
                self.drop_slot(index, DisconnectReason::TimedOut, false);
            } else if slot.connection.error().is_some() {
                self.drop_slot(index, DisconnectReason::ChannelFull, true);
            }
        }
        self.pending.retain(|_, p| p.expires_at > now);
        self.used.retain(|_, (_, until)| *until > now);
        self.requests.retain(|_, (since, _)| now - *since < 1.0);
    }

    fn take(&mut self, packet: &[u8], from: SocketAddr) {
        match kind_of(packet) {
            Some(Kind::Request) => self.take_request(packet, from),
            Some(Kind::Response) => self.take_response(packet, from),
            Some(_) => self.take_connected(packet, from),
            None => {}
        }
    }

    fn take_request(&mut self, packet: &[u8], from: SocketAddr) {
        // Too many from one address are ignored before anything is decrypted.
        let (since, count) = self.requests.entry(from.ip()).or_insert((self.now, 0));
        if self.now - *since >= 1.0 {
            (*since, *count) = (self.now, 0);
        }
        *count += 1;
        if *count > REQUESTS_PER_SECOND {
            return;
        }
        let Some(request) = read_request(packet, self.config.protocol_id) else { return };
        if request.expires <= self.unix_now() {
            return;
        }
        let Some(token) = PrivateToken::open(
            &request.private,
            &self.config.private_key,
            request.protocol_id,
            request.expires,
            &request.nonce,
        ) else {
            return;
        };
        let named = |s: &SocketAddr| self.config.public_addresses.contains(s);
        if !self.config.public_addresses.is_empty() && !token.servers.iter().any(named) {
            return;
        }
        if self.by_address.contains_key(&from) || self.slots.iter().flatten().any(|s| s.id.0 == token.client_id) {
            return;
        }
        // One token, one connection: a token seen from another address is turned away.
        let seal: [u8; 16] = request.private[request.private.len() - 16..].try_into().expect("16 bytes");
        if self.used.get(&seal).is_some_and(|(address, _)| *address != from) {
            return;
        }
        let timeout = f64::from(token.timeout_seconds.max(1));
        let until = self.now + (request.expires.saturating_sub(self.unix_now())) as f64;
        self.used.insert(seal, (from, until));
        let pending = self.pending.entry(from).or_insert(Pending {
            send_key: token.server_to_client_key,
            receive_key: token.client_to_server_key,
            timeout,
            expires_at: self.now + timeout,
            sequence: 0,
        });
        if self.slots.iter().all(Option::is_some) {
            let denied = write_sealed(Kind::Denied, pending.sequence, &pending.send_key, self.config.protocol_id, &[]);
            pending.sequence += 1;
            self.transport.send(from, &denied);
            return;
        }
        let challenge = seal_challenge(&self.challenge_key, self.challenge_sequence, token.client_id, &token.user_data);
        let mut body = self.challenge_sequence.to_le_bytes().to_vec();
        body.extend_from_slice(&challenge);
        self.challenge_sequence += 1;
        let packet = write_sealed(Kind::Challenge, pending.sequence, &pending.send_key, self.config.protocol_id, &body);
        pending.sequence += 1;
        self.transport.send(from, &packet);
    }

    fn take_response(&mut self, packet: &[u8], from: SocketAddr) {
        let Some(pending) = self.pending.get(&from) else { return };
        let Some((Kind::Response, _, body)) = read_sealed(packet, &pending.receive_key, self.config.protocol_id) else {
            return;
        };
        if body.len() < 8 {
            return;
        }
        let sequence = u64::from_le_bytes(body[..8].try_into().expect("8 bytes"));
        let Some((client_id, user_data)) = open_challenge(&self.challenge_key, sequence, &body[8..]) else { return };
        if self.by_address.contains_key(&from) || self.slots.iter().flatten().any(|s| s.id.0 == client_id) {
            return;
        }
        let Some(index) = self.slots.iter().position(Option::is_none) else {
            // Filled while it was answering the challenge.
            if let Some(pending) = self.pending.get_mut(&from) {
                let denied =
                    write_sealed(Kind::Denied, pending.sequence, &pending.send_key, self.config.protocol_id, &[]);
                pending.sequence += 1;
                self.transport.send(from, &denied);
            }
            return;
        };
        let Some(pending) = self.pending.remove(&from) else { return };
        let slot = Slot {
            id: ClientId(client_id),
            address: from,
            send_key: pending.send_key,
            receive_key: pending.receive_key,
            sequence: pending.sequence,
            replay: ReplayWindow::default(),
            connection: Connection::new(self.config.connection.clone(), self.now),
            last_received: self.now,
            last_sent: f64::NEG_INFINITY,
            timeout: pending.timeout,
            user_data: Box::new(user_data),
        };
        self.slots[index] = Some(slot);
        self.by_address.insert(from, index);
        self.events.push(ServerEvent::Connected { client: ClientId(client_id), user_data: Box::new(user_data) });
        // Told at once, so the client knows it is in.
        self.send_keep_alive(index);
    }

    fn take_connected(&mut self, packet: &[u8], from: SocketAddr) {
        let Some(&index) = self.by_address.get(&from) else { return };
        let protocol_id = self.config.protocol_id;
        let Some(slot) = self.slots[index].as_mut() else { return };
        let Some((kind, sequence, body)) = read_sealed(packet, &slot.receive_key, protocol_id) else { return };
        if !slot.replay.take(sequence) {
            return;
        }
        slot.last_received = self.now;
        match kind {
            Kind::Payload => {
                let _ = slot.connection.receive_packet(&body, self.now);
            }
            Kind::Disconnect => self.drop_slot(index, DisconnectReason::Closed, false),
            _ => {}
        }
    }

    fn send_keep_alive(&mut self, index: usize) {
        let protocol_id = self.config.protocol_id;
        let max = self.config.max_clients as u32;
        let Some(slot) = self.slots[index].as_mut() else { return };
        let mut body = (index as u32).to_le_bytes().to_vec();
        body.extend_from_slice(&max.to_le_bytes());
        let packet = write_sealed(Kind::KeepAlive, slot.sequence, &slot.send_key, protocol_id, &body);
        slot.sequence += 1;
        slot.last_sent = self.now;
        self.transport.send(slot.address, &packet);
    }

    fn drop_slot(&mut self, index: usize, reason: DisconnectReason, tell: bool) {
        let Some(mut slot) = self.slots[index].take() else { return };
        if tell {
            // Several times, as any one may be lost.
            for _ in 0..3 {
                let packet =
                    write_sealed(Kind::Disconnect, slot.sequence, &slot.send_key, self.config.protocol_id, &[]);
                slot.sequence += 1;
                self.transport.send(slot.address, &packet);
            }
            self.transport.flush();
        }
        self.by_address.remove(&slot.address);
        self.events.push(ServerEvent::Disconnected { client: slot.id, reason });
    }

    fn slot_mut(&mut self, client: ClientId) -> Option<&mut Slot> {
        self.slots.iter_mut().flatten().find(|s| s.id == client)
    }

    pub fn send(&mut self, client: ClientId, channel: Channel, message: &[u8]) -> Result<(), SendError> {
        self.slot_mut(client).ok_or(SendError::NoChannel)?.connection.send(channel, message)
    }

    pub fn send_tagged(
        &mut self,
        client: ClientId,
        channel: Channel,
        message: &[u8],
        tag: u32,
    ) -> Result<(), SendError> {
        self.slot_mut(client).ok_or(SendError::NoChannel)?.connection.send_tagged(channel, message, tag)
    }

    pub fn receive(&mut self, client: ClientId, channel: Channel) -> Option<Vec<u8>> {
        self.slot_mut(client)?.connection.receive(channel)
    }

    pub fn take_deliveries(&mut self, client: ClientId) -> Vec<Delivery> {
        self.slot_mut(client).map(|s| s.connection.take_deliveries()).unwrap_or_default()
    }

    pub fn stats(&self, client: ClientId) -> Option<Stats> {
        self.slots.iter().flatten().find(|s| s.id == client).map(|s| s.connection.stats())
    }

    /// The bytes the token gave this client, a matchmaker's own.
    pub fn user_data(&self, client: ClientId) -> Option<&[u8; USER_DATA_BYTES]> {
        self.slots.iter().flatten().find(|s| s.id == client).map(|s| &*s.user_data)
    }

    /// The clients connected, in slot order.
    pub fn clients(&self) -> Vec<ClientId> {
        self.slots.iter().flatten().map(|s| s.id).collect()
    }

    pub fn take_events(&mut self) -> Vec<ServerEvent> {
        std::mem::take(&mut self.events)
    }

    /// Sends what every client has waiting, and a keep-alive to any that
    /// has had nothing for a while.
    pub fn flush(&mut self, now: f64) {
        self.now = now;
        self.transport.update(now);
        let protocol_id = self.config.protocol_id;
        for index in 0..self.slots.len() {
            let Some(slot) = self.slots[index].as_mut() else { continue };
            let transport = &mut self.transport;
            let (address, key, sequence) = (slot.address, slot.send_key, &mut slot.sequence);
            let mut sent = false;
            slot.connection.flush(now, &mut |packet| {
                transport.send(address, &write_sealed(Kind::Payload, *sequence, &key, protocol_id, packet));
                *sequence += 1;
                sent = true;
            });
            if sent {
                slot.last_sent = now;
            } else if now - slot.last_sent >= KEEP_ALIVE {
                self.send_keep_alive(index);
            }
        }
        self.transport.flush();
    }

    /// Ends a client's connection, telling it so.
    pub fn disconnect(&mut self, client: ClientId) {
        if let Some(index) = self.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.id == client)) {
            self.drop_slot(index, DisconnectReason::Kicked, true);
        }
    }

    /// Ends every connection, telling each client.
    pub fn shut_down(&mut self) {
        for index in 0..self.slots.len() {
            self.drop_slot(index, DisconnectReason::Kicked, true);
        }
    }

    pub fn set_bytes_per_second(&mut self, client: ClientId, rate: f64) {
        if let Some(slot) = self.slot_mut(client) {
            slot.connection.set_bytes_per_second(rate);
        }
    }
}

/// Seconds since 1970, as tokens count time.
pub fn unix_seconds() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}
