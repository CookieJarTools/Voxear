//! Encryption: every packet after the first is sealed with ChaCha20-Poly1305
//! under a key for its direction, its sequence number the nonce, so it can
//! be neither read, changed nor replayed. Connect tokens' private parts are
//! sealed with XChaCha20-Poly1305, whose long nonce can be random.

use chacha20poly1305::{
    ChaCha20Poly1305, Key as CipherKey, Nonce, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

/// A 256-bit key.
pub type Key = [u8; 32];

/// The bytes sealing adds: the authentication tag.
pub const TAG_BYTES: usize = 16;

/// A key from the system's randomness.
pub fn random_key() -> Key {
    random_bytes()
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    // Without the system's randomness nothing could be kept secret, so there is no going on.
    getrandom::fill(&mut bytes).expect("the system's randomness");
    bytes
}

/// Seals `message` under `key` with `sequence` as the nonce, bound to `aad`.
pub(crate) fn seal(key: &Key, sequence: u64, aad: &[u8], message: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(CipherKey::from_slice(key));
    let payload = Payload { msg: message, aad };
    cipher.encrypt(Nonce::from_slice(&nonce(sequence)), payload).expect("sealing never fails for these sizes")
}

/// Opens what [`seal`] sealed; `None` if anything about it was changed.
pub(crate) fn open(key: &Key, sequence: u64, aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(CipherKey::from_slice(key));
    cipher.decrypt(Nonce::from_slice(&nonce(sequence)), Payload { msg: sealed, aad }).ok()
}

pub(crate) fn seal_x(key: &Key, nonce: &[u8; 24], aad: &[u8], message: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key));
    cipher
        .encrypt(XNonce::from_slice(nonce), Payload { msg: message, aad })
        .expect("sealing never fails for these sizes")
}

pub(crate) fn open_x(key: &Key, nonce: &[u8; 24], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(CipherKey::from_slice(key));
    cipher.decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad }).ok()
}

fn nonce(sequence: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_opens_only_unchanged() {
        let key = random_key();
        let sealed = seal(&key, 7, b"aad", b"hello");
        assert_eq!(sealed.len(), 5 + TAG_BYTES);
        assert_eq!(open(&key, 7, b"aad", &sealed).as_deref(), Some(&b"hello"[..]));
        assert_eq!(open(&key, 8, b"aad", &sealed), None, "another sequence");
        assert_eq!(open(&key, 7, b"other", &sealed), None, "other bound data");
        let mut changed = sealed.clone();
        changed[0] ^= 1;
        assert_eq!(open(&key, 7, b"aad", &changed), None, "a changed byte");
        assert_eq!(open(&random_key(), 7, b"aad", &sealed), None, "another key");
    }
}
