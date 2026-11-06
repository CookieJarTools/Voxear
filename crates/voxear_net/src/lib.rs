//! Networking's transport: connections over UDP that are encrypted and
//! authenticated, with reliable and unreliable channels, and a simulator of
//! bad networks to test through.
//!
//! - A [`Server`] lets in clients holding a [`ConnectToken`] made with its
//!   private key, then keeps a [`Connection`] to each.
//! - A [`Client`] joins with a token and keeps one connection.
//! - Both send through a [`Transport`]: a [`UdpTransport`], a
//!   [`MemoryNetwork`]'s ends in tests, or either wrapped in a
//!   [`Simulated`] network.
//!
//! Neither keeps a clock: every call that needs the time is given it, in
//! seconds, so a game drives them from its own ticks, and tests run as fast
//! as they like.

pub mod bits;
mod client;
pub mod connection;
mod crypto;
mod packet;
pub mod sequence;
mod server;
mod simulator;
mod token;
mod transport;

pub use bits::{BitReader, BitWriter};
pub use client::{Client, ClientState, ConnectFailure};
pub use connection::{
    Channel, ChannelConfig, ChannelKind, Connection, ConnectionConfig, Delivery, MAX_PAYLOAD, SendError, Stats,
    default_channels,
};
pub use crypto::{Key, random_key};
pub use packet::REQUEST_BYTES;
pub use server::{ClientId, DisconnectReason, Server, ServerConfig, ServerEvent, unix_seconds};
pub use simulator::{Conditions, Simulated};
pub use token::{CONNECT_TOKEN_BYTES, ConnectToken, MAX_SERVERS, TokenError, TokenRequest, USER_DATA_BYTES};
pub use transport::{MAX_DATAGRAM, MemoryNetwork, MemoryTransport, Transport, UdpTransport};
