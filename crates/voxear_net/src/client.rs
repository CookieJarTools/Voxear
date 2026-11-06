//! The client's side: joining with a connect token, then one connection to
//! the server. It tries the token's servers in turn until one lets it in.

use std::net::SocketAddr;

use crate::connection::{Channel, Connection, ConnectionConfig, Delivery, SendError, Stats};
use crate::packet::{Kind, ReplayWindow, read_sealed, write_request, write_sealed};
use crate::server::{DisconnectReason, KEEP_ALIVE, unix_seconds};
use crate::token::ConnectToken;
use crate::transport::{MAX_DATAGRAM, Transport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientState {
    Connecting,
    Connected,
    Failed(ConnectFailure),
    Disconnected(DisconnectReason),
}

/// Why joining failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// The token expired before any server let the client in.
    TokenExpired,
    /// Every server was full.
    Denied,
    /// No server answered.
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Requesting,
    Responding,
    Connected,
}

pub struct Client {
    token: ConnectToken,
    transport: Box<dyn Transport>,
    config: ConnectionConfig,
    server: usize,
    step: Step,
    state: ClientState,
    /// The challenge to send back while responding.
    response: Vec<u8>,
    sequence: u64,
    replay: ReplayWindow,
    connection: Connection,
    started: f64,
    unix_start: f64,
    step_started: f64,
    last_sent: f64,
    last_received: f64,
    now: f64,
    slot: Option<u32>,
    denied: usize,
}

impl Client {
    pub fn new(token: ConnectToken, transport: Box<dyn Transport>, config: ConnectionConfig, now: f64) -> Self {
        let connection = Connection::new(config.clone(), now);
        Self {
            token,
            transport,
            config,
            server: 0,
            step: Step::Requesting,
            state: ClientState::Connecting,
            response: Vec::new(),
            sequence: 0,
            replay: ReplayWindow::default(),
            connection,
            started: now,
            unix_start: unix_seconds(),
            step_started: now,
            last_sent: f64::NEG_INFINITY,
            last_received: now,
            now,
            slot: None,
            denied: 0,
        }
    }

    pub fn state(&self) -> ClientState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == ClientState::Connected
    }

    /// The server it is joining or has joined.
    pub fn server(&self) -> SocketAddr {
        self.token.servers[self.server.min(self.token.servers.len() - 1)]
    }

    /// Its slot on the server, once in.
    pub fn slot(&self) -> Option<u32> {
        self.slot
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.transport.local_addr()
    }

    fn timeout(&self) -> f64 {
        f64::from(self.token.timeout_seconds.max(1))
    }

    /// Takes in what has arrived, and moves joining on.
    pub fn update(&mut self, now: f64) {
        self.now = now;
        self.transport.update(now);
        let mut buffer = [0u8; MAX_DATAGRAM + 64];
        while let Some((length, from)) = self.transport.receive(&mut buffer) {
            if from == self.server() {
                let packet = buffer[..length].to_vec();
                self.take(&packet);
            }
        }
        match self.state {
            ClientState::Connecting => {
                let unix = self.unix_start + (now - self.started);
                if unix >= self.token.expires as f64 {
                    self.state = ClientState::Failed(ConnectFailure::TokenExpired);
                } else if now - self.step_started > self.timeout() {
                    self.next_server(false);
                }
            }
            ClientState::Connected => {
                if now - self.last_received > self.timeout() {
                    self.state = ClientState::Disconnected(DisconnectReason::TimedOut);
                } else if self.connection.error().is_some() {
                    self.disconnect_because(DisconnectReason::ChannelFull);
                }
            }
            _ => {}
        }
    }

    /// Gives up on this server and tries the next, if there is one.
    fn next_server(&mut self, denied: bool) {
        self.denied += usize::from(denied);
        self.server += 1;
        if self.server >= self.token.servers.len() {
            let all_denied = self.denied == self.token.servers.len();
            self.state =
                ClientState::Failed(if all_denied { ConnectFailure::Denied } else { ConnectFailure::TimedOut });
            return;
        }
        (self.step, self.step_started, self.last_sent) = (Step::Requesting, self.now, f64::NEG_INFINITY);
        (self.sequence, self.replay) = (0, ReplayWindow::default());
    }

    fn take(&mut self, packet: &[u8]) {
        let Some((kind, sequence, body)) =
            read_sealed(packet, &self.token.server_to_client_key, self.token.protocol_id)
        else {
            return;
        };
        if !self.replay.take(sequence) {
            return;
        }
        match (kind, self.step) {
            (Kind::Denied, Step::Requesting | Step::Responding) => self.next_server(true),
            (Kind::Challenge, Step::Requesting) => {
                self.response = body;
                (self.step, self.step_started, self.last_sent) = (Step::Responding, self.now, f64::NEG_INFINITY);
            }
            (Kind::KeepAlive, Step::Responding) if body.len() >= 8 => {
                self.slot = Some(u32::from_le_bytes(body[..4].try_into().expect("4 bytes")));
                self.step = Step::Connected;
                self.state = ClientState::Connected;
                self.last_received = self.now;
                self.connection = Connection::new(self.config.clone(), self.now);
            }
            (Kind::KeepAlive, Step::Connected) => self.last_received = self.now,
            (Kind::Payload, Step::Connected) => {
                self.last_received = self.now;
                let _ = self.connection.receive_packet(&body, self.now);
            }
            (Kind::Disconnect, Step::Connected) => self.state = ClientState::Disconnected(DisconnectReason::Closed),
            _ => {}
        }
    }

    pub fn send(&mut self, channel: Channel, message: &[u8]) -> Result<(), SendError> {
        self.connection.send(channel, message)
    }

    pub fn send_tagged(&mut self, channel: Channel, message: &[u8], tag: u32) -> Result<(), SendError> {
        self.connection.send_tagged(channel, message, tag)
    }

    pub fn receive(&mut self, channel: Channel) -> Option<Vec<u8>> {
        self.connection.receive(channel)
    }

    pub fn take_deliveries(&mut self) -> Vec<Delivery> {
        self.connection.take_deliveries()
    }

    pub fn stats(&self) -> Stats {
        self.connection.stats()
    }

    pub fn set_bytes_per_second(&mut self, rate: f64) {
        self.connection.set_bytes_per_second(rate);
    }

    /// Sends what is waiting: joining's packets ten times a second while
    /// joining, and once in, the connection's, or a keep-alive if idle.
    pub fn flush(&mut self, now: f64) {
        self.now = now;
        self.transport.update(now);
        let server = self.server();
        let (key, protocol_id) = (self.token.client_to_server_key, self.token.protocol_id);
        match (self.state, self.step) {
            (ClientState::Connecting, Step::Requesting) if now - self.last_sent >= KEEP_ALIVE => {
                let request = write_request(protocol_id, self.token.expires, &self.token.nonce, &self.token.private);
                self.transport.send(server, &request);
                self.last_sent = now;
            }
            (ClientState::Connecting, Step::Responding) if now - self.last_sent >= KEEP_ALIVE => {
                let packet = write_sealed(Kind::Response, self.sequence, &key, protocol_id, &self.response);
                self.sequence += 1;
                self.transport.send(server, &packet);
                self.last_sent = now;
            }
            (ClientState::Connected, _) => {
                let transport = &mut self.transport;
                let sequence = &mut self.sequence;
                let mut sent = false;
                self.connection.flush(now, &mut |packet| {
                    transport.send(server, &write_sealed(Kind::Payload, *sequence, &key, protocol_id, packet));
                    *sequence += 1;
                    sent = true;
                });
                if sent {
                    self.last_sent = now;
                } else if now - self.last_sent >= KEEP_ALIVE {
                    let packet = write_sealed(Kind::KeepAlive, self.sequence, &key, protocol_id, &[0; 8]);
                    self.sequence += 1;
                    self.transport.send(server, &packet);
                    self.last_sent = now;
                }
            }
            _ => {}
        }
        self.transport.flush();
    }

    /// Leaves, telling the server.
    pub fn disconnect(&mut self) {
        self.disconnect_because(DisconnectReason::Closed);
    }

    fn disconnect_because(&mut self, reason: DisconnectReason) {
        if self.state == ClientState::Connected {
            let server = self.server();
            for _ in 0..3 {
                let packet = write_sealed(
                    Kind::Disconnect,
                    self.sequence,
                    &self.token.client_to_server_key,
                    self.token.protocol_id,
                    &[],
                );
                self.sequence += 1;
                self.transport.send(server, &packet);
            }
            self.transport.flush();
        }
        self.state = ClientState::Disconnected(reason);
    }
}
