//! What a world of blocks without end has made of its rings, kept on disk so
//! that ground seen once comes back at once: each coarse column's blocks as
//! they were made, with what was built on them, and the far view's tiles of
//! bare ground.
//!
//! Columns are kept in region files, eight by eight columns of one level to
//! a file, each column a record of where it is, a stamp of what it was made
//! from, and its bytes compressed with zstd. A file is only ever added to: a
//! column made again is written after the old one, and the last written is
//! the one read. Once most of a file is columns made again it is written
//! afresh with only the last of each. So keeping a column costs its own
//! bytes, however full its file is.
//!
//! Two programs may keep columns in the same folder, as an editor and a
//! render of the same project: a record is written in one go, and one read
//! cut short or not matching its check is taken as not kept. The worst that
//! happens is a column made again.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, Result, ensure};

use super::chunk::{CHUNK_LEN, Chunk};

/// How many columns along x and z a region file holds.
pub const REGION: i32 = 8;

/// What a record starts with.
const MAGIC: &[u8; 4] = b"VXKC";

/// A record's header: its magic, place, stamp, length and check.
const HEADER: usize = 4 + 4 + 4 + 8 + 4 + 8;

/// How hard each column is compressed: zstd's own usual level, quick to
/// write and to read.
const LEVEL: i32 = 3;

/// The level a world's own blocks drawn past where they are made are kept
/// at (see `stream::FINE`).
pub const FINE: u32 = u32::MAX;

/// What is kept: a far tile's bare ground, or a coarse column as it was made,
/// with what is built on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ground,
    Built,
}

/// The region file in `folder` that column `at` of `level` of `kind` is kept
/// in.
pub fn region_file(folder: &Path, kind: Kind, level: u32, at: [i32; 2]) -> PathBuf {
    let kind = match kind {
        Kind::Ground => 'g',
        Kind::Built => 'b',
    };
    let level = if level == FINE { "f".to_string() } else { level.to_string() };
    let [x, z] = at.map(|v| v.div_euclid(REGION));
    folder.join(format!("{kind}{level}_{x}_{z}.kept"))
}

/// Where each column's last record lies in a region file, as far as it has
/// been read.
#[derive(Default)]
struct Index {
    /// How much of the file has been read.
    read: u64,
    /// Each column's stamp, and where its bytes start and how many.
    columns: HashMap<[i32; 2], (u64, u64, u32, u64)>,
    /// How many bytes are records some later one replaced.
    stale: u64,
}

type Indices = Mutex<HashMap<PathBuf, Arc<Mutex<Index>>>>;

fn index_of(path: &Path) -> Arc<Mutex<Index>> {
    static INDICES: OnceLock<Indices> = OnceLock::new();
    let mut indices = INDICES.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // A long run through many regions forgets those read long ago; they are read again if they come back.
    if indices.len() > 4096 {
        indices.clear();
    }
    indices.entry(path.to_path_buf()).or_default().clone()
}

/// Brings `index` up to what `file`, `length` bytes long, holds now.
fn catch_up(index: &mut Index, file: &mut File, length: u64) -> Result<()> {
    if length < index.read {
        // Written afresh since: read from the start.
        *index = Index::default();
    }
    if length == index.read {
        return Ok(());
    }
    file.seek(SeekFrom::Start(index.read))?;
    let mut bytes = Vec::with_capacity((length - index.read) as usize);
    Read::by_ref(file).take(length - index.read).read_to_end(&mut bytes)?;
    let mut at = 0usize;
    while at + HEADER <= bytes.len() {
        let header = &bytes[at..at + HEADER];
        if &header[..4] != MAGIC {
            break;
        }
        let word = |from: usize| i32::from_le_bytes(header[from..from + 4].try_into().expect("four bytes"));
        let column = [word(4), word(8)];
        let stamp = u64::from_le_bytes(header[12..20].try_into().expect("eight bytes"));
        let length = u32::from_le_bytes(header[20..24].try_into().expect("four bytes"));
        let check = u64::from_le_bytes(header[24..32].try_into().expect("eight bytes"));
        // One being written by another program as this reads: read again later from here.
        if at + HEADER + length as usize > bytes.len() {
            break;
        }
        let start = index.read + (at + HEADER) as u64;
        if let Some((_, _, old, _)) = index.columns.insert(column, (stamp, start, length, check)) {
            index.stale += HEADER as u64 + u64::from(old);
        }
        at += HEADER + length as usize;
    }
    index.read += at as u64;
    Ok(())
}

/// The bytes kept for column `at` of `level` of `kind` in `folder`, where
/// they were kept made from `stamp`; none where they were not, or were made
/// from something else, or cannot be read.
pub fn read(folder: &Path, kind: Kind, level: u32, at: [i32; 2], stamp: u64) -> Option<Vec<u8>> {
    let path = region_file(folder, kind, level, at);
    let index = index_of(&path);
    let mut index = index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = File::open(&path).ok()?;
    let length = file.metadata().ok()?.len();
    catch_up(&mut index, &mut file, length).ok()?;
    let &(kept, start, length, check) = index.columns.get(&at)?;
    if kept != stamp {
        return None;
    }
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut compressed = vec![0u8; length as usize];
    file.read_exact(&mut compressed).ok()?;
    if crate::layers::stable_hash(&compressed) != check {
        return None;
    }
    zstd::decode_all(&compressed[..]).ok()
}

/// Whether column `at` of `level` of `kind` is kept in `folder`, made from
/// `stamp`: as [`read`] would find it, without reading it.
pub fn has(folder: &Path, kind: Kind, level: u32, at: [i32; 2], stamp: u64) -> bool {
    let path = region_file(folder, kind, level, at);
    let index = index_of(&path);
    let mut index = index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Ok(mut file) = File::open(&path) else { return false };
    let Ok(metadata) = file.metadata() else { return false };
    catch_up(&mut index, &mut file, metadata.len()).is_ok()
        && index.columns.get(&at).is_some_and(|(kept, ..)| *kept == stamp)
}

/// The file in a folder of kept columns saying which levels were baked
/// whole (see `stream::bake`), a level a line.
const BAKED: &str = "baked";

/// Notes in `folder` that level `level` is baked whole over the world.
pub fn mark_baked(folder: &Path, level: u32) {
    let mut levels = baked_levels(folder);
    levels |= 1 << level.min(31);
    let text: String = (0..32).filter(|level| levels & (1 << level) != 0).map(|level| format!("{level}\n")).collect();
    let _ = std::fs::create_dir_all(folder);
    let writing = folder.join(format!("{BAKED}.{}", std::process::id()));
    if std::fs::write(&writing, text).is_ok() && std::fs::rename(&writing, folder.join(BAKED)).is_err() {
        let _ = std::fs::remove_file(&writing);
    }
}

/// The levels baked whole in `folder`, a bit each: read again only when its
/// file changes, so asking every frame costs a look at the file's time.
pub fn baked_levels(folder: &Path) -> u32 {
    type Seen = Mutex<HashMap<PathBuf, (std::time::SystemTime, u32)>>;
    static SEEN: OnceLock<Seen> = OnceLock::new();
    let path = folder.join(BAKED);
    let Ok(changed) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()) else { return 0 };
    let mut seen = SEEN.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((when, levels)) = seen.get(&path)
        && *when == changed
    {
        return *levels;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let levels = text.lines().filter_map(|line| line.trim().parse::<u32>().ok()).filter(|level| *level < 32);
    let levels = levels.fold(0u32, |bits, level| bits | 1 << level);
    seen.insert(path, (changed, levels));
    levels
}

/// Keeps `bytes` for column `at` of `level` of `kind` in `folder`, made
/// from `stamp`. Not kept where the folder cannot be written to, as in a
/// game installed where it may not write.
pub fn write(folder: &Path, kind: Kind, level: u32, at: [i32; 2], stamp: u64, bytes: &[u8]) {
    let Ok(compressed) = zstd::encode_all(bytes, LEVEL) else { return };
    let path = region_file(folder, kind, level, at);
    let mut record = Vec::with_capacity(HEADER + compressed.len());
    record.extend(MAGIC);
    record.extend(at[0].to_le_bytes());
    record.extend(at[1].to_le_bytes());
    record.extend(stamp.to_le_bytes());
    record.extend((compressed.len() as u32).to_le_bytes());
    record.extend(crate::layers::stable_hash(&compressed).to_le_bytes());
    record.extend(&compressed);
    let index = index_of(&path);
    let mut index = index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = std::fs::create_dir_all(folder);
    let appended = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut file| {
        file.write_all(&record)?;
        file.metadata()
    });
    let Ok(metadata) = appended else { return };
    // Written afresh once most of it is columns made again since.
    let Ok(mut file) = File::open(&path) else { return };
    if catch_up(&mut index, &mut file, metadata.len()).is_err() {
        return;
    }
    if index.stale > 256 * 1024 && index.stale * 2 > index.read {
        let _ = compact(&path, &mut index, &mut file);
    }
}

/// Writes the region file at `path` afresh with only each column's last
/// record, beside it, then in its place.
fn compact(path: &Path, index: &mut Index, file: &mut File) -> Result<()> {
    let mut fresh = Vec::new();
    let mut columns: Vec<_> = index.columns.iter().map(|(at, kept)| (*at, *kept)).collect();
    columns.sort_by_key(|(_, (_, start, ..))| *start);
    for (at, (stamp, start, length, check)) in columns {
        let mut compressed = vec![0u8; length as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut compressed)?;
        fresh.extend(MAGIC);
        fresh.extend(at[0].to_le_bytes());
        fresh.extend(at[1].to_le_bytes());
        fresh.extend(stamp.to_le_bytes());
        fresh.extend(length.to_le_bytes());
        fresh.extend(check.to_le_bytes());
        fresh.extend(compressed);
    }
    let writing = path.with_extension(format!("kept.{}", std::process::id()));
    std::fs::write(&writing, &fresh)?;
    if let Err(error) = std::fs::rename(&writing, path) {
        let _ = std::fs::remove_file(&writing);
        return Err(error.into());
    }
    *index = Index::default();
    Ok(())
}

/// A coarse column's chunks as bytes, for [`write`]: each chunk's place,
/// palette, bits, words, and how full each block is where it keeps that.
pub fn chunks_to_bytes(chunks: &[([i32; 3], Chunk)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend((chunks.len() as u32).to_le_bytes());
    for (key, chunk) in chunks {
        key.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        let (palette, bits, words) = chunk.parts();
        bytes.extend((palette.len() as u16).to_le_bytes());
        palette.iter().for_each(|block| bytes.extend(block.to_le_bytes()));
        bytes.push(bits as u8);
        words.iter().for_each(|word| bytes.extend(word.to_le_bytes()));
        match chunk.fills() {
            Some(fills) => {
                bytes.push(1);
                bytes.extend(fills);
            }
            None => bytes.push(0),
        }
    }
    bytes
}

/// The chunks [`chunks_to_bytes`] gave.
pub fn chunks_from_bytes(bytes: &[u8]) -> Result<Vec<([i32; 3], Chunk)>> {
    let mut at = 0usize;
    let mut take = |count: usize| -> Result<&[u8]> {
        let taken = bytes.get(at..at + count).context("kept chunks cut short")?;
        at += count;
        Ok(taken)
    };
    let count = u32::from_le_bytes(take(4)?.try_into()?) as usize;
    let mut chunks = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let mut key = [0i32; 3];
        for v in &mut key {
            *v = i32::from_le_bytes(take(4)?.try_into()?);
        }
        let entries = u16::from_le_bytes(take(2)?.try_into()?) as usize;
        let palette = (0..entries).map(|_| Ok(u16::from_le_bytes(take(2)?.try_into()?))).collect::<Result<Vec<_>>>()?;
        let bits = u32::from(take(1)?[0]);
        let words = if bits == 0 { 0 } else { CHUNK_LEN.div_ceil((64 / bits.max(1)) as usize) };
        let words = (0..words).map(|_| Ok(u64::from_le_bytes(take(8)?.try_into()?))).collect::<Result<Vec<_>>>()?;
        let chunk = Chunk::from_parts(palette, bits, words).context("kept chunk that is not one")?;
        let chunk = match take(1)?[0] {
            1 => chunk.with_fills(take(CHUNK_LEN)?.to_vec()).context("kept fills that are not its blocks'")?,
            _ => chunk,
        };
        chunks.push((key, chunk));
    }
    ensure!(at == bytes.len(), "kept chunks with more after them");
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::chunk::AIR;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("voxear-kept-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_column_kept_is_read_back_by_its_stamp_and_the_last_kept_is_the_one_read() {
        let folder = folder("stamp");
        assert_eq!(read(&folder, Kind::Built, 2, [3, -4], 7), None, "nothing kept yet");
        write(&folder, Kind::Built, 2, [3, -4], 7, b"first");
        write(&folder, Kind::Built, 2, [4, -4], 7, b"beside");
        assert_eq!(read(&folder, Kind::Built, 2, [3, -4], 7).as_deref(), Some(&b"first"[..]));
        assert_eq!(read(&folder, Kind::Built, 2, [3, -4], 8), None, "made from something else");
        assert_eq!(read(&folder, Kind::Ground, 2, [3, -4], 7), None, "another kind");
        assert_eq!(read(&folder, Kind::Built, 3, [3, -4], 7), None, "another level");
        write(&folder, Kind::Built, 2, [3, -4], 8, b"again");
        assert_eq!(read(&folder, Kind::Built, 2, [3, -4], 8).as_deref(), Some(&b"again"[..]));
        assert_eq!(read(&folder, Kind::Built, 2, [4, -4], 7).as_deref(), Some(&b"beside"[..]));
        // Both columns in one region file, eight by eight to a file.
        assert_eq!(region_file(&folder, Kind::Built, 2, [3, -4]), region_file(&folder, Kind::Built, 2, [4, -4]));
        assert_ne!(region_file(&folder, Kind::Built, 2, [3, -4]), region_file(&folder, Kind::Built, 2, [8, -4]));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_region_mostly_made_again_is_written_afresh_keeping_the_last_of_each() {
        let folder = folder("compact");
        let big: Vec<u8> = (0..200_000u32).map(|v| (v.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for round in 0..6u64 {
            for x in 0..4 {
                write(&folder, Kind::Built, 1, [x, 0], round, &big);
            }
        }
        let file = region_file(&folder, Kind::Built, 1, [0, 0]);
        let length = std::fs::metadata(&file).unwrap().len();
        assert!(length < 4 * 3 * 220_000, "written afresh: {length} bytes");
        for x in 0..4 {
            assert_eq!(read(&folder, Kind::Built, 1, [x, 0], 5).as_deref(), Some(&big[..]));
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_record_cut_short_or_spoilt_is_not_read_and_one_written_after_it_is() {
        let folder = folder("spoilt");
        write(&folder, Kind::Ground, 4, [0, 0], 1, b"whole");
        let file = region_file(&folder, Kind::Ground, 4, [0, 0]);
        // Its last byte changed, as a program stopped while writing.
        let mut bytes = std::fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&file, &bytes).unwrap();
        // Read afresh, as another program would.
        let other = folder.join("copy");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::copy(&file, region_file(&other, Kind::Ground, 4, [0, 0])).unwrap();
        assert_eq!(read(&other, Kind::Ground, 4, [0, 0], 1), None);
        write(&other, Kind::Ground, 4, [0, 0], 1, b"again");
        assert_eq!(read(&other, Kind::Ground, 4, [0, 0], 1).as_deref(), Some(&b"again"[..]));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn chunks_kept_come_back_as_they_were_with_how_full_their_blocks_are() {
        let mut ground = Chunk::filled(AIR);
        ground.set(1, 2, 3, 4);
        ground.set(5, 0, 7, 9);
        let mut smooth = Chunk::filled(AIR);
        smooth.set_cell(2, 2, 2, 6, 200);
        let chunks = vec![([0, -1, 2], ground), ([0, -2, 2], Chunk::filled(3)), ([0, 0, 2], smooth)];
        let back = chunks_from_bytes(&chunks_to_bytes(&chunks)).unwrap();
        assert_eq!(back.len(), 3);
        let side = crate::blocks::chunk::CHUNK;
        for ((key, chunk), (k, c)) in chunks.iter().zip(&back) {
            assert_eq!(key, k);
            for (x, y, z) in (0..side).flat_map(|x| (0..side).flat_map(move |y| (0..side).map(move |z| (x, y, z)))) {
                assert_eq!((chunk.get(x, y, z), chunk.fill(x, y, z)), (c.get(x, y, z), c.fill(x, y, z)));
            }
        }
        assert_eq!(back[2].1.fill(2, 2, 2), 200);
        assert!(chunks_from_bytes(&[1, 0, 0, 0]).is_err(), "cut short");
    }
}
