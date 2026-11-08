//! Voxear's own wrapping for what a build ships that would otherwise sit in
//! it in a common format: the catalog, and sound files. A wrapped file is
//! eight bytes saying what it holds, a format number, how it was
//! compressed, a salt and its size, then its bytes, compressed where that
//! makes them smaller and scrambled, so no other program opens it as it is.
//!
//! Scrambling is not encryption. It keeps a game's files from being copied
//! out of a build as they are, as asset licences ask, not from someone set
//! on reading them. The salt comes from the bytes themselves, so wrapping
//! the same bytes gives the same file, and a rebuild that changes nothing
//! writes nothing new.

use anyhow::{Context, Result, ensure};

use crate::bundle::Compression;

/// What a wrapped sound starts with.
pub const SOUND_MAGIC: &[u8; 8] = b"VXSOUND\0";
const FORMAT: u32 = 1;
const HEADER_SIZE: usize = 32;
/// Mixed into every salt, so a salt alone does not give the keystream.
const KEY: u64 = 0x7a3c_91e4_5b0d_f268;
/// Zstd's level for wrapped files, as for bundles.
const ZSTD_LEVEL: i32 = 19;

/// `data` wrapped as a `magic` file, compressed as `compression` says when
/// that makes it smaller.
pub fn wrap(magic: &[u8; 8], data: &[u8], compression: Compression) -> Vec<u8> {
    let compressed = match compression {
        Compression::Lz4 => Some((lz4_flex::block::compress(data), 1u8)),
        // Compressing in memory cannot fail but for running out of it, when storing it as it is will do.
        Compression::Zstd => zstd::bulk::compress(data, ZSTD_LEVEL).ok().map(|bytes| (bytes, 2)),
        Compression::None => None,
    };
    let (mut stored, codec) = compressed.filter(|(bytes, _)| bytes.len() < data.len()).unwrap_or((data.to_vec(), 0));
    let salt = fnv(data);
    scramble(salt, &mut stored);
    let mut bytes = Vec::with_capacity(HEADER_SIZE + stored.len());
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&FORMAT.to_le_bytes());
    bytes.extend_from_slice(&[codec, 0, 0, 0]);
    bytes.extend_from_slice(&salt.to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&stored);
    bytes
}

/// The bytes [`wrap`] wrapped as a `magic` file. `what` names the file for
/// messages.
pub fn unwrap(magic: &[u8; 8], bytes: &[u8], what: &str) -> Result<Vec<u8>> {
    ensure!(bytes.len() >= HEADER_SIZE && bytes.starts_with(magic), "{what} is not a Voxear file of its kind");
    let word = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
    let format = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
    ensure!(format == FORMAT, "{what} was packed by a newer Voxear (format {format})");
    let (codec, salt, size) = (bytes[12], word(16), word(24) as usize);
    let mut stored = bytes[HEADER_SIZE..].to_vec();
    scramble(salt, &mut stored);
    let data = match codec {
        0 => stored,
        1 => lz4_flex::block::decompress(&stored, size).with_context(|| format!("unpacking {what}"))?,
        2 => zstd::bulk::decompress(&stored, size).with_context(|| format!("unpacking {what}"))?,
        _ => anyhow::bail!("{what} is compressed in a way this Voxear does not know"),
    };
    ensure!(data.len() == size, "{what} is cut short");
    Ok(data)
}

/// A sound file wrapped to ship, with its extension, which says how it
/// decodes. A file compressed already, as OGG, FLAC and MP3 are, should be
/// given [`Compression::None`].
pub fn wrap_sound(extension: &str, file: &[u8], compression: Compression) -> Vec<u8> {
    let extension = extension.as_bytes();
    let mut data = Vec::with_capacity(1 + extension.len() + file.len());
    data.push(extension.len().min(255) as u8);
    data.extend_from_slice(&extension[..extension.len().min(255)]);
    data.extend_from_slice(file);
    wrap(SOUND_MAGIC, &data, compression)
}

/// A sound file and its extension, from [`wrap_sound`]'s bytes.
pub fn unwrap_sound(bytes: &[u8], what: &str) -> Result<(String, Vec<u8>)> {
    let mut data = unwrap(SOUND_MAGIC, bytes, what)?;
    let length = usize::from(*data.first().with_context(|| format!("{what} is empty"))?);
    ensure!(data.len() > length, "{what} is cut short");
    let extension = String::from_utf8_lossy(&data[1..=length]).into_owned();
    data.drain(..=length);
    Ok((extension, data))
}

/// Turns `bytes` over with a keystream from `salt`; doing it again turns
/// them back.
fn scramble(salt: u64, bytes: &mut [u8]) {
    let mut state = salt ^ KEY;
    let mut words = bytes.chunks_exact_mut(8);
    for word in &mut words {
        let mixed = u64::from_le_bytes((&*word).try_into().expect("eight bytes")) ^ next(&mut state);
        word.copy_from_slice(&mixed.to_le_bytes());
    }
    let rest = words.into_remainder();
    for (byte, key) in rest.iter_mut().zip(next(&mut state).to_le_bytes()) {
        *byte ^= key;
    }
}

/// SplitMix64's next number: quick, and every bit depends on every step.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A 64-bit FNV-1a hash, for a salt that follows the bytes.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short WAV file: its header, then a tone that repeats.
    fn wav() -> Vec<u8> {
        let mut file = b"RIFF\x24\x10\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data".to_vec();
        file.extend((0..4096u32).flat_map(|i| (((i % 64) as i16 - 32) * 512).to_le_bytes()));
        file
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn a_wrapped_sound_unwraps_to_the_file_it_was_and_shows_none_of_it() {
        let file = wav();
        for compression in [Compression::Lz4, Compression::Zstd, Compression::None] {
            let wrapped = wrap_sound("wav", &file, compression);
            assert!(wrapped.starts_with(SOUND_MAGIC));
            assert_eq!(unwrap_sound(&wrapped, "a sound").unwrap(), ("wav".to_string(), file.clone()), "{compression:?}");
            assert!(!contains(&wrapped, b"RIFF") && !contains(&wrapped, b"WAVE"), "no header shows: {compression:?}");
            assert!(!contains(&wrapped, &file[200..216]), "no run of the samples shows: {compression:?}");
            assert!(!contains(&wrapped, b"wav"), "nor its kind");
        }
        assert!(wrap_sound("wav", &file, Compression::Lz4).len() < file.len() / 2, "a WAV file still shrinks");
        assert_eq!(wrap_sound("wav", &file, Compression::Lz4), wrap_sound("wav", &file, Compression::Lz4), "the same bytes wrap the same");
    }

    #[test]
    fn a_damaged_or_foreign_file_is_refused() {
        let wrapped = wrap(b"VXCATLOG", b"what a catalog says", Compression::None);
        assert!(unwrap(SOUND_MAGIC, &wrapped, "a sound").is_err(), "another kind of file");
        assert!(unwrap(b"VXCATLOG", &wrapped[..20], "a catalog").is_err(), "cut short");
        assert_eq!(unwrap(b"VXCATLOG", &wrapped, "a catalog").unwrap(), b"what a catalog says");
        let empty = wrap(SOUND_MAGIC, b"", Compression::Lz4);
        assert!(unwrap_sound(&empty, "a sound").is_err());
    }
}
