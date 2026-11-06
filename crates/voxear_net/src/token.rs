//! Connect tokens: what lets a client into a server. Whoever decides who
//! plays where (a matchmaker, a lobby, the editor starting test players)
//! makes one with the servers' shared private key and hands it to the
//! client. Its private part, which only the servers can open, says who the
//! client is, which servers it may join, the keys for its packets and up to
//! 256 bytes of the matchmaker's own (a match, a team), and it expires.
//! The client never learns the private key, so it can neither forge a token
//! nor change what one says.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::crypto::{Key, TAG_BYTES, open_x, random_bytes, random_key, seal_x};

/// What every Voxear packet's protocol says it is, as bytes.
pub const VERSION: &[u8; 13] = b"VOXEAR NET 1\0";
/// The size of a token's private part, sealed.
pub const PRIVATE_TOKEN_BYTES: usize = 1024;
/// The size of a whole token, as handed to a client.
pub const CONNECT_TOKEN_BYTES: usize = 2048;
pub const USER_DATA_BYTES: usize = 256;
/// The most servers one token lists.
pub const MAX_SERVERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenError {
    NoServers,
    TooManyServers,
}

/// What a connect token is made from.
#[derive(Clone, Debug)]
pub struct TokenRequest<'a> {
    /// The game and its version, so clients of another can't join.
    pub protocol_id: u64,
    /// The key shared by the servers, never by clients.
    pub private_key: &'a Key,
    /// Who the client is, unique among a server's clients.
    pub client_id: u64,
    pub user_data: [u8; USER_DATA_BYTES],
    /// Where the client may connect, tried in order.
    pub servers: Vec<SocketAddr>,
    /// How long, in seconds, the token can be used to start connecting.
    pub expire_seconds: u64,
    /// How long, in seconds, either end waits without hearing from the
    /// other before the connection is given up.
    pub timeout_seconds: i32,
    /// The time now, in seconds since 1970.
    pub now_unix: u64,
}

/// A token as a client holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectToken {
    pub protocol_id: u64,
    pub created: u64,
    pub expires: u64,
    pub nonce: [u8; 24],
    pub private: Box<[u8; PRIVATE_TOKEN_BYTES]>,
    pub timeout_seconds: i32,
    pub servers: Vec<SocketAddr>,
    pub client_to_server_key: Key,
    pub server_to_client_key: Key,
}

/// The private part, as the server reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivateToken {
    pub client_id: u64,
    pub timeout_seconds: i32,
    pub servers: Vec<SocketAddr>,
    pub client_to_server_key: Key,
    pub server_to_client_key: Key,
    pub user_data: [u8; USER_DATA_BYTES],
}

impl ConnectToken {
    pub fn generate(request: TokenRequest) -> Result<Self, TokenError> {
        if request.servers.is_empty() {
            return Err(TokenError::NoServers);
        }
        if request.servers.len() > MAX_SERVERS {
            return Err(TokenError::TooManyServers);
        }
        let private = PrivateToken {
            client_id: request.client_id,
            timeout_seconds: request.timeout_seconds,
            servers: request.servers.clone(),
            client_to_server_key: random_key(),
            server_to_client_key: random_key(),
            user_data: request.user_data,
        };
        let expires = request.now_unix + request.expire_seconds;
        let nonce = random_bytes();
        let sealed = private.seal(request.private_key, request.protocol_id, expires, &nonce);
        Ok(Self {
            protocol_id: request.protocol_id,
            created: request.now_unix,
            expires,
            nonce,
            private: sealed,
            timeout_seconds: request.timeout_seconds,
            servers: request.servers,
            client_to_server_key: private.client_to_server_key,
            server_to_client_key: private.server_to_client_key,
        })
    }

    /// The token as [`CONNECT_TOKEN_BYTES`] bytes, to hand to the client.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CONNECT_TOKEN_BYTES);
        out.extend_from_slice(VERSION);
        out.extend_from_slice(&self.protocol_id.to_le_bytes());
        out.extend_from_slice(&self.created.to_le_bytes());
        out.extend_from_slice(&self.expires.to_le_bytes());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.private[..]);
        out.extend_from_slice(&self.timeout_seconds.to_le_bytes());
        write_servers(&mut out, &self.servers);
        out.extend_from_slice(&self.client_to_server_key);
        out.extend_from_slice(&self.server_to_client_key);
        out.resize(CONNECT_TOKEN_BYTES, 0);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != CONNECT_TOKEN_BYTES {
            return None;
        }
        let mut read = Reader(bytes);
        if read.take(VERSION.len())? != VERSION {
            return None;
        }
        let protocol_id = read.u64()?;
        let created = read.u64()?;
        let expires = read.u64()?;
        let nonce = read.array()?;
        let private = Box::new(read.array()?);
        let timeout_seconds = read.u32()? as i32;
        let servers = read_servers(&mut read)?;
        let client_to_server_key = read.array()?;
        let server_to_client_key = read.array()?;
        Some(Self {
            protocol_id,
            created,
            expires,
            nonce,
            private,
            timeout_seconds,
            servers,
            client_to_server_key,
            server_to_client_key,
        })
    }
}

impl PrivateToken {
    fn seal(&self, key: &Key, protocol_id: u64, expires: u64, nonce: &[u8; 24]) -> Box<[u8; PRIVATE_TOKEN_BYTES]> {
        let mut plain = Vec::with_capacity(PRIVATE_TOKEN_BYTES - TAG_BYTES);
        plain.extend_from_slice(&self.client_id.to_le_bytes());
        plain.extend_from_slice(&self.timeout_seconds.to_le_bytes());
        write_servers(&mut plain, &self.servers);
        plain.extend_from_slice(&self.client_to_server_key);
        plain.extend_from_slice(&self.server_to_client_key);
        plain.extend_from_slice(&self.user_data);
        plain.resize(PRIVATE_TOKEN_BYTES - TAG_BYTES, 0);
        let sealed = seal_x(key, nonce, &aad(protocol_id, expires), &plain);
        Box::new(sealed.try_into().expect("a private token seals to its size"))
    }

    pub(crate) fn open(
        sealed: &[u8; PRIVATE_TOKEN_BYTES],
        key: &Key,
        protocol_id: u64,
        expires: u64,
        nonce: &[u8; 24],
    ) -> Option<Self> {
        let plain = open_x(key, nonce, &aad(protocol_id, expires), sealed)?;
        let mut read = Reader(&plain);
        let client_id = read.u64()?;
        let timeout_seconds = read.u32()? as i32;
        let servers = read_servers(&mut read)?;
        let client_to_server_key = read.array()?;
        let server_to_client_key = read.array()?;
        let user_data = read.array()?;
        Some(Self { client_id, timeout_seconds, servers, client_to_server_key, server_to_client_key, user_data })
    }
}

fn aad(protocol_id: u64, expires: u64) -> Vec<u8> {
    let mut aad = VERSION.to_vec();
    aad.extend_from_slice(&protocol_id.to_le_bytes());
    aad.extend_from_slice(&expires.to_le_bytes());
    aad
}

fn write_servers(out: &mut Vec<u8>, servers: &[SocketAddr]) {
    out.extend_from_slice(&(servers.len() as u32).to_le_bytes());
    for server in servers {
        match server.ip() {
            IpAddr::V4(ip) => {
                out.push(1);
                out.extend_from_slice(&ip.octets());
            }
            IpAddr::V6(ip) => {
                out.push(2);
                out.extend_from_slice(&ip.octets());
            }
        }
        out.extend_from_slice(&server.port().to_le_bytes());
    }
}

fn read_servers(read: &mut Reader) -> Option<Vec<SocketAddr>> {
    let count = read.u32()? as usize;
    if count == 0 || count > MAX_SERVERS {
        return None;
    }
    (0..count)
        .map(|_| {
            let ip = match read.take(1)?[0] {
                1 => IpAddr::V4(Ipv4Addr::from(read.array::<4>()?)),
                2 => IpAddr::V6(Ipv6Addr::from(read.array::<16>()?)),
                _ => return None,
            };
            Some(SocketAddr::new(ip, u16::from_le_bytes(read.array()?)))
        })
        .collect()
}

/// Reads fixed-size fields, `None` past the end.
pub(crate) struct Reader<'a>(pub &'a [u8]);

impl<'a> Reader<'a> {
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.0.len() {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }

    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_le_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(key: &Key) -> TokenRequest<'_> {
        TokenRequest {
            protocol_id: 0x1234,
            private_key: key,
            client_id: 42,
            user_data: [7; USER_DATA_BYTES],
            servers: vec!["127.0.0.1:40000".parse().unwrap(), "[::1]:40001".parse().unwrap()],
            expire_seconds: 30,
            timeout_seconds: 5,
            now_unix: 1_000_000,
        }
    }

    #[test]
    fn a_token_survives_being_handed_over_and_only_its_servers_open_it() {
        let key = random_key();
        let token = ConnectToken::generate(request(&key)).unwrap();
        let bytes = token.to_bytes();
        assert_eq!(bytes.len(), CONNECT_TOKEN_BYTES);
        let back = ConnectToken::from_bytes(&bytes).unwrap();
        assert_eq!(back, token);
        let private = PrivateToken::open(&back.private, &key, 0x1234, back.expires, &back.nonce).unwrap();
        assert_eq!(private.client_id, 42);
        assert_eq!(private.servers, token.servers);
        assert_eq!(private.client_to_server_key, token.client_to_server_key);
        assert_eq!(private.user_data, [7; USER_DATA_BYTES]);
        assert!(PrivateToken::open(&back.private, &random_key(), 0x1234, back.expires, &back.nonce).is_none());
        assert!(
            PrivateToken::open(&back.private, &key, 0x1234, back.expires + 1, &back.nonce).is_none(),
            "its expiry changed"
        );
        assert!(PrivateToken::open(&back.private, &key, 0x9999, back.expires, &back.nonce).is_none(), "another game");
    }

    #[test]
    fn a_token_must_name_some_servers_but_not_too_many() {
        let key = random_key();
        let none = TokenRequest { servers: Vec::new(), ..request(&key) };
        assert_eq!(ConnectToken::generate(none).unwrap_err(), TokenError::NoServers);
        let many = TokenRequest { servers: vec!["127.0.0.1:1".parse().unwrap(); 33], ..request(&key) };
        assert_eq!(ConnectToken::generate(many).unwrap_err(), TokenError::TooManyServers);
        assert!(ConnectToken::from_bytes(&[0; 100]).is_none());
    }
}
