//! Packets as they go over the wire. The first a client sends, its
//! connection request, carries its token's private part, which is sealed
//! already, and is padded to exactly [`REQUEST_BYTES`], so a server turns
//! away anything else at once, and nothing a server answers to someone not
//! yet connected is ever larger than what they sent. Every other packet is
//! sealed whole under its direction's key, its sequence number the nonce.

use crate::crypto::{Key, TAG_BYTES, open, seal};
use crate::token::{PRIVATE_TOKEN_BYTES, Reader, USER_DATA_BYTES, VERSION};

/// The exact size of a connection request.
pub const REQUEST_BYTES: usize = 1 + VERSION.len() + 8 + 8 + 24 + PRIVATE_TOKEN_BYTES;
/// A challenge token's size, sealed.
pub(crate) const CHALLENGE_BYTES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Kind {
    Request = 0,
    Denied = 1,
    Challenge = 2,
    Response = 3,
    KeepAlive = 4,
    Payload = 5,
    Disconnect = 6,
}

impl Kind {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Request,
            1 => Self::Denied,
            2 => Self::Challenge,
            3 => Self::Response,
            4 => Self::KeepAlive,
            5 => Self::Payload,
            6 => Self::Disconnect,
            _ => return None,
        })
    }
}

/// A connection request, as read.
pub(crate) struct Request {
    pub protocol_id: u64,
    pub expires: u64,
    pub nonce: [u8; 24],
    pub private: Box<[u8; PRIVATE_TOKEN_BYTES]>,
}

pub(crate) fn write_request(
    protocol_id: u64,
    expires: u64,
    nonce: &[u8; 24],
    private: &[u8; PRIVATE_TOKEN_BYTES],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(REQUEST_BYTES);
    out.push(Kind::Request as u8);
    out.extend_from_slice(VERSION);
    out.extend_from_slice(&protocol_id.to_le_bytes());
    out.extend_from_slice(&expires.to_le_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(private);
    debug_assert_eq!(out.len(), REQUEST_BYTES);
    out
}

/// A connection request, if `bytes` is exactly one for this protocol; the
/// cheap checks come before anything is decrypted.
pub(crate) fn read_request(bytes: &[u8], protocol_id: u64) -> Option<Request> {
    if bytes.len() != REQUEST_BYTES || bytes[0] != Kind::Request as u8 {
        return None;
    }
    let mut read = Reader(&bytes[1..]);
    if read.take(VERSION.len())? != VERSION || read.u64()? != protocol_id {
        return None;
    }
    let expires = read.u64()?;
    let nonce = read.array()?;
    let private = Box::new(read.array()?);
    Some(Request { protocol_id, expires, nonce, private })
}

/// Seals a packet of `kind` with `sequence` under `key`.
pub(crate) fn write_sealed(kind: Kind, sequence: u64, key: &Key, protocol_id: u64, body: &[u8]) -> Vec<u8> {
    // The sequence in as few bytes as it needs.
    let bytes = (8 - sequence.leading_zeros() as usize / 8).max(1);
    let prefix = kind as u8 | (bytes as u8) << 4;
    let mut out = Vec::with_capacity(1 + bytes + body.len() + TAG_BYTES);
    out.push(prefix);
    out.extend_from_slice(&sequence.to_le_bytes()[..bytes]);
    out.extend_from_slice(&seal(key, sequence, &aad(protocol_id, prefix), body));
    out
}

/// Opens a sealed packet: its kind, sequence and body.
pub(crate) fn read_sealed(bytes: &[u8], key: &Key, protocol_id: u64) -> Option<(Kind, u64, Vec<u8>)> {
    let (&prefix, rest) = bytes.split_first()?;
    let kind = Kind::from_u8(prefix & 0x0F).filter(|k| *k != Kind::Request)?;
    let count = (prefix >> 4) as usize;
    if !(1..=8).contains(&count) || rest.len() < count + TAG_BYTES {
        return None;
    }
    let mut sequence = [0u8; 8];
    sequence[..count].copy_from_slice(&rest[..count]);
    let sequence = u64::from_le_bytes(sequence);
    let body = open(key, sequence, &aad(protocol_id, prefix), &rest[count..])?;
    Some((kind, sequence, body))
}

/// The kind a packet says it is, before opening it.
pub(crate) fn kind_of(bytes: &[u8]) -> Option<Kind> {
    Kind::from_u8(*bytes.first()? & 0x0F)
}

fn aad(protocol_id: u64, prefix: u8) -> Vec<u8> {
    let mut aad = Vec::with_capacity(VERSION.len() + 9);
    aad.extend_from_slice(VERSION);
    aad.extend_from_slice(&protocol_id.to_le_bytes());
    aad.push(prefix);
    aad
}

/// A challenge token: what the server asks a client to send back, sealed
/// with a key only the server knows, so a client that answers must have got
/// the challenge at the address it claims.
pub(crate) fn seal_challenge(key: &Key, sequence: u64, client_id: u64, user_data: &[u8; USER_DATA_BYTES]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(CHALLENGE_BYTES - TAG_BYTES);
    plain.extend_from_slice(&client_id.to_le_bytes());
    plain.extend_from_slice(user_data);
    plain.resize(CHALLENGE_BYTES - TAG_BYTES, 0);
    seal(key, sequence, &[], &plain)
}

pub(crate) fn open_challenge(key: &Key, sequence: u64, sealed: &[u8]) -> Option<(u64, [u8; USER_DATA_BYTES])> {
    if sealed.len() != CHALLENGE_BYTES {
        return None;
    }
    let plain = open(key, sequence, &[], sealed)?;
    let mut read = Reader(&plain);
    Some((read.u64()?, read.array()?))
}

/// Which recent sequence numbers have come, so a packet recorded and sent
/// again is turned away.
#[derive(Clone, Debug)]
pub(crate) struct ReplayWindow {
    latest: u64,
    seen: Box<[u64; 256]>,
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self { latest: 0, seen: Box::new([u64::MAX; 256]) }
    }
}

impl ReplayWindow {
    /// Whether `sequence` is new, noting it if so.
    pub fn take(&mut self, sequence: u64) -> bool {
        if sequence.saturating_add(256) <= self.latest {
            return false;
        }
        let slot = &mut self.seen[(sequence % 256) as usize];
        if *slot == sequence {
            return false;
        }
        *slot = sequence;
        self.latest = self.latest.max(sequence);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::random_key;

    #[test]
    fn a_sealed_packet_opens_with_its_kind_and_sequence() {
        let key = random_key();
        for sequence in [0u64, 255, 256, 1 << 40] {
            let packet = write_sealed(Kind::Payload, sequence, &key, 9, b"body");
            assert_eq!(kind_of(&packet), Some(Kind::Payload));
            assert_eq!(read_sealed(&packet, &key, 9), Some((Kind::Payload, sequence, b"body".to_vec())));
            assert_eq!(read_sealed(&packet, &key, 10), None, "another protocol");
        }
        let mut changed = write_sealed(Kind::KeepAlive, 3, &key, 9, b"x");
        changed[0] = Kind::Payload as u8 | 1 << 4;
        assert_eq!(read_sealed(&changed, &key, 9), None, "its kind changed");
        assert_eq!(read_sealed(&[0x15], &key, 9), None);
    }

    #[test]
    fn replays_are_turned_away() {
        let mut window = ReplayWindow::default();
        assert!(window.take(0));
        assert!(window.take(5));
        assert!(!window.take(5));
        assert!(window.take(3));
        assert!(window.take(1000));
        assert!(!window.take(500), "too old to tell");
    }
}
