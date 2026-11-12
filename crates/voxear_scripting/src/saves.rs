//! Saves, for `Voxear.Save` in C#: named slots, each one file in the game's
//! save folder. Scripts turn their values into bytes; this keeps the files.
//!
//! A save is written whole to a file beside the slot's and then renamed
//! over it, so a crash or a full disk partway through leaves the last good
//! save as it was. Writes queue on a thread of their own and run in order,
//! so a script can carry on while one finishes; anything that reads, lists
//! or changes slots waits for the writes queued before it.
//!
//! A save file is a header and then three parts, one after another:
//!
//! | Bytes | What |
//! | --- | --- |
//! | 8 | `VOXSAVE\0` |
//! | 4 | the file's format, 1 |
//! | 8 | when it was written, in milliseconds since 1970 |
//! | 4, 4, 8 | the lengths of the info, the picture and the data |
//! | 4 | a CRC-32 of the three parts, to tell a damaged file |
//! | … | info: JSON that scripts write about the save, read to list slots |
//! | … | picture: a PNG of the game, or nothing |
//! | … | data: the save itself, in the format the info names |
//!
//! Numbers are little-endian. Listing slots reads only each header and
//! info.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    ptr,
    sync::{Arc, Condvar, Mutex, OnceLock, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::interop::utf8;

/// A save file's extension.
pub const SAVE_EXTENSION: &str = "save";
const MAGIC: &[u8; 8] = b"VOXSAVE\0";
const FILE_FORMAT: u32 = 1;
const HEADER_LEN: usize = 40;
/// The longest slot name, in characters.
const LONGEST_NAME: usize = 64;

/// Where a game keeps its saves, and the format scripts write when they do
/// not choose one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveSettings {
    /// None where there is nowhere to keep them, such as a home-less user.
    pub folder: Option<PathBuf>,
    /// JSON that a person can read, as the editor writes, rather than the
    /// compact binary of builds.
    pub readable: bool,
}

static SETTINGS: Mutex<SaveSettings> = Mutex::new(SaveSettings { folder: None, readable: false });

/// Sets where saves go from now on.
pub(crate) fn configure(settings: SaveSettings) {
    flush();
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = settings;
}

/// Where saves are kept now; none where there is nowhere to keep them.
pub fn saves_folder() -> Option<PathBuf> {
    settings().folder
}

fn settings() -> SaveSettings {
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Waits for every write queued so far, as before the game closes.
pub fn flush() {
    if let Some(writer) = WRITER.get() {
        writer.flush();
    }
}

/// Why a save could not be read or written. Each kind has a number below
/// zero, which C#'s `SaveError` mirrors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// No slot of that name.
    Missing(String),
    /// The file is not a save, was cut short, or its bytes changed.
    Damaged(String),
    /// The disk said no: it is full, or the folder is read-only.
    Failed(String),
    /// The name cannot be a file's.
    BadName(String),
    /// There is nowhere to keep saves here.
    NoFolder,
}

impl SaveError {
    fn code(&self) -> i32 {
        match self {
            Self::Missing(_) => -1,
            Self::Damaged(_) => -2,
            Self::Failed(_) => -3,
            Self::BadName(_) => -4,
            Self::NoFolder => -5,
        }
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(slot) => write!(f, "there is no save called \"{slot}\""),
            Self::Damaged(why) | Self::Failed(why) | Self::BadName(why) => f.write_str(why),
            Self::NoFolder => f.write_str("there is no save folder here, so nothing can be saved"),
        }
    }
}

impl std::error::Error for SaveError {}

/// One of a save's three parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePart {
    Info,
    Picture,
    Data,
}

impl SavePart {
    fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Info),
            1 => Some(Self::Picture),
            2 => Some(Self::Data),
            _ => None,
        }
    }
}

/// A slot as a list of them shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotEntry {
    /// The slot's name, as its file has it.
    pub name: String,
    /// When it was written, in milliseconds since 1970.
    pub written: u64,
    /// The file's size in bytes.
    pub size: u64,
    pub info: Vec<u8>,
    pub has_picture: bool,
}

/// The slots in one folder. Names match ignoring case on every system, as
/// Windows matches file names, so a game behaves the same everywhere.
#[derive(Clone, Debug)]
pub struct SaveFolder {
    root: PathBuf,
}

impl SaveFolder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every readable slot, newest first. A file that is not a save is left
    /// out, with a warning.
    pub fn list(&self) -> Result<Vec<SlotEntry>, SaveError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(failed(&self.root, "reading", &error)),
        };
        let mut slots = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = slot_name(&path) else { continue };
            match read_head(&path) {
                Ok((header, info)) => slots.push(SlotEntry {
                    name,
                    written: header.written,
                    size: HEADER_LEN as u64 + header.parts_len(),
                    info,
                    has_picture: header.picture > 0,
                }),
                Err(error) => tracing::warn!(target: "script", "the save \"{name}\" is left out of the list: {error}"),
            }
        }
        slots.sort_by(|a, b| b.written.cmp(&a.written).then_with(|| a.name.cmp(&b.name)));
        Ok(slots)
    }

    /// The slot's file, when there is one.
    pub fn find(&self, slot: &str) -> Result<Option<PathBuf>, SaveError> {
        check_name(slot)?;
        let exact = self.root.join(format!("{slot}.{SAVE_EXTENSION}"));
        if exact.is_file() {
            return Ok(Some(exact));
        }
        let Ok(entries) = fs::read_dir(&self.root) else { return Ok(None) };
        let wanted = slot.to_lowercase();
        Ok(entries
            .flatten()
            .map(|entry| entry.path())
            .find(|path| slot_name(path).is_some_and(|name| name.to_lowercase() == wanted) && path.is_file()))
    }

    pub fn exists(&self, slot: &str) -> Result<bool, SaveError> {
        Ok(self.find(slot)?.is_some())
    }

    /// One part of a slot. Reading the data checks the whole file.
    pub fn read(&self, slot: &str, part: SavePart) -> Result<Vec<u8>, SaveError> {
        let path = self.find(slot)?.ok_or_else(|| SaveError::Missing(slot.to_string()))?;
        match part {
            SavePart::Info => Ok(read_head(&path)?.1),
            SavePart::Picture | SavePart::Data => read_part(&path, part),
        }
    }

    /// Writes a slot whole, replacing what it held only once every byte is
    /// on disk.
    pub fn write(&self, slot: &str, info: &[u8], picture: &[u8], data: &[u8]) -> Result<(), SaveError> {
        let path = self.path_for_write(slot)?;
        write_atomically(&path, &encode(info, picture, data, now()))
    }

    /// Deletes a slot. False when there was none.
    pub fn delete(&self, slot: &str) -> Result<bool, SaveError> {
        let Some(path) = self.find(slot)? else { return Ok(false) };
        fs::remove_file(&path).map_err(|error| failed(&path, "deleting", &error))?;
        Ok(true)
    }

    /// Copies a slot to another name, replacing what that held.
    pub fn copy(&self, from: &str, to: &str) -> Result<(), SaveError> {
        let source = self.find(from)?.ok_or_else(|| SaveError::Missing(from.to_string()))?;
        let target = self.path_for_write(to)?;
        if same_file(&source, &target) {
            return Ok(());
        }
        let bytes = fs::read(&source).map_err(|error| failed(&source, "reading", &error))?;
        write_atomically(&target, &bytes)
    }

    /// Gives a slot another name, replacing what that held.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), SaveError> {
        let source = self.find(from)?.ok_or_else(|| SaveError::Missing(from.to_string()))?;
        check_name(to)?;
        let target = match self.find(to)? {
            Some(existing) if !same_file(&existing, &source) => existing,
            // Only the case changes, or the name is new.
            _ => self.root.join(format!("{to}.{SAVE_EXTENSION}")),
        };
        fs::rename(&source, &target).map_err(|error| failed(&target, "renaming to", &error))
    }

    /// The file a write to `slot` replaces: the one it has, in whatever
    /// case, or a new one.
    fn path_for_write(&self, slot: &str) -> Result<PathBuf, SaveError> {
        match self.find(slot)? {
            Some(path) => Ok(path),
            None => {
                fs::create_dir_all(&self.root).map_err(|error| failed(&self.root, "making", &error))?;
                Ok(self.root.join(format!("{slot}.{SAVE_EXTENSION}")))
            }
        }
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.file_name().map(|n| n.to_string_lossy().to_lowercase())
        == b.file_name().map(|n| n.to_string_lossy().to_lowercase())
}

fn failed(path: &Path, doing: &str, error: &io::Error) -> SaveError {
    SaveError::Failed(format!("{doing} {}: {error}", path.display()))
}

/// The slot a file in the save folder holds, or None for other files, such
/// as one still being written.
fn slot_name(path: &Path) -> Option<String> {
    if path.extension()? != SAVE_EXTENSION {
        return None;
    }
    Some(path.file_stem()?.to_string_lossy().into_owned())
}

/// Slot names are file names on every system: letters, digits, spaces, `-`,
/// `_`, `.` and brackets, not starting or ending with a space or a dot, and
/// none of the names Windows keeps for devices.
pub fn check_name(slot: &str) -> Result<(), SaveError> {
    let bad = |why: &str| Err(SaveError::BadName(format!("\"{slot}\" cannot name a save: {why}")));
    if slot.trim().is_empty() {
        return bad("it is empty");
    }
    if slot.chars().count() > LONGEST_NAME {
        return bad(&format!("it is longer than {LONGEST_NAME} characters"));
    }
    if let Some(c) = slot.chars().find(|&c| !(c.is_alphanumeric() || " -_.()".contains(c))) {
        return bad(&format!("use letters, digits, spaces, - and _, not '{c}'"));
    }
    if slot.starts_with(['.', ' ']) || slot.ends_with(['.', ' ']) {
        return bad("it cannot start or end with a space or a dot");
    }
    let stem = slot.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if device {
        return bad("Windows keeps that name for a device");
    }
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

#[derive(Clone, Copy, Debug)]
struct Header {
    written: u64,
    info: u32,
    picture: u32,
    data: u64,
    crc: u32,
}

impl Header {
    fn parts_len(&self) -> u64 {
        u64::from(self.info) + u64::from(self.picture) + self.data
    }

    fn parse(bytes: &[u8; HEADER_LEN]) -> Result<Self, String> {
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
        if &bytes[..8] != MAGIC {
            return Err("it is not a save file".into());
        }
        let format = u32_at(8);
        if format > FILE_FORMAT {
            return Err(format!("it was written by a newer Voxear, as save file format {format}"));
        }
        Ok(Self { written: u64_at(12), info: u32_at(20), picture: u32_at(24), data: u64_at(28), crc: u32_at(36) })
    }
}

fn encode(info: &[u8], picture: &[u8], data: &[u8], written: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + info.len() + picture.len() + data.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FILE_FORMAT.to_le_bytes());
    bytes.extend_from_slice(&written.to_le_bytes());
    bytes.extend_from_slice(&(info.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(picture.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&crc32(&[info, picture, data]).to_le_bytes());
    bytes.extend_from_slice(info);
    bytes.extend_from_slice(picture);
    bytes.extend_from_slice(data);
    bytes
}

fn damaged(path: &Path, why: impl fmt::Display) -> SaveError {
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    SaveError::Damaged(format!("the save \"{name}\" cannot be read: {why}"))
}

/// The header and the info, without reading the rest.
fn read_head(path: &Path) -> Result<(Header, Vec<u8>), SaveError> {
    let mut file = File::open(path).map_err(|error| failed(path, "opening", &error))?;
    let length = file.metadata().map_err(|error| failed(path, "reading", &error))?.len();
    let mut head = [0; HEADER_LEN];
    file.read_exact(&mut head).map_err(|_| damaged(path, "it is cut short"))?;
    let header = Header::parse(&head).map_err(|why| damaged(path, why))?;
    if HEADER_LEN as u64 + header.parts_len() != length {
        return Err(damaged(path, "it is cut short, or has more in it than its header says"));
    }
    let mut info = vec![0; header.info as usize];
    file.read_exact(&mut info).map_err(|error| failed(path, "reading", &error))?;
    Ok((header, info))
}

fn read_part(path: &Path, part: SavePart) -> Result<Vec<u8>, SaveError> {
    let bytes = fs::read(path).map_err(|error| failed(path, "reading", &error))?;
    let head: &[u8; HEADER_LEN] =
        bytes.get(..HEADER_LEN).and_then(|h| h.try_into().ok()).ok_or_else(|| damaged(path, "it is cut short"))?;
    let header = Header::parse(head).map_err(|why| damaged(path, why))?;
    if HEADER_LEN as u64 + header.parts_len() != bytes.len() as u64 {
        return Err(damaged(path, "it is cut short, or has more in it than its header says"));
    }
    let (info, rest) = bytes[HEADER_LEN..].split_at(header.info as usize);
    let (picture, data) = rest.split_at(header.picture as usize);
    if crc32(&[info, picture, data]) != header.crc {
        return Err(damaged(path, "its bytes changed after it was written"));
    }
    Ok(match part {
        SavePart::Info => info.to_vec(),
        SavePart::Picture => picture.to_vec(),
        SavePart::Data => data.to_vec(),
    })
}

/// Writes `bytes` to a file beside `path`, makes sure they reached the disk,
/// and renames it over `path`, which replaces the old file in one step on
/// Linux and Windows alike.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let temporary = path.with_extension(format!("{SAVE_EXTENSION}.writing"));
    let written = File::create(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(error) = written.and_then(|()| fs::rename(&temporary, path)) {
        let _ = fs::remove_file(&temporary);
        return Err(failed(path, "writing", &error));
    }
    // The rename itself reaches the disk once the folder is synced too.
    #[cfg(unix)]
    if let Some(folder) = path.parent() {
        let _ = File::open(folder).and_then(|folder| folder.sync_all());
    }
    Ok(())
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut at = 0;
    while at < 256 {
        let mut crc = at as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
            bit += 1;
        }
        table[at] = crc;
        at += 1;
    }
    table
};

/// The CRC-32 that zip and PNG use, over several slices in turn.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        for &byte in *part {
            crc = CRC_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    !crc
}

/// The thread that writes saves, in the order they were queued.
struct Writer {
    jobs: Mutex<mpsc::Sender<Job>>,
    state: Arc<(Mutex<Tickets>, Condvar)>,
}

struct Job {
    ticket: u32,
    folder: SaveFolder,
    slot: String,
    info: Vec<u8>,
    data: Vec<u8>,
}

#[derive(Default)]
struct Tickets {
    next: u32,
    /// Each write not yet asked about since it finished: None while it runs.
    writes: HashMap<u32, Option<Result<(), SaveError>>>,
    running: usize,
}

static WRITER: OnceLock<Writer> = OnceLock::new();

impl Writer {
    fn get() -> &'static Writer {
        WRITER.get_or_init(|| {
            let (jobs, queue) = mpsc::channel::<Job>();
            let state = Arc::new((Mutex::new(Tickets::default()), Condvar::new()));
            let shared = state.clone();
            std::thread::Builder::new()
                .name("save writer".into())
                .spawn(move || {
                    for job in queue {
                        let result = job.folder.write(&job.slot, &job.info, &[], &job.data);
                        let (tickets, finished) = &*shared;
                        let mut tickets = tickets.lock().unwrap_or_else(|e| e.into_inner());
                        tickets.writes.insert(job.ticket, Some(result));
                        tickets.running -= 1;
                        finished.notify_all();
                    }
                })
                .expect("starting the save writer");
            Writer { jobs: Mutex::new(jobs), state }
        })
    }

    fn queue(&self, folder: SaveFolder, slot: String, info: Vec<u8>, data: Vec<u8>) -> u32 {
        let (tickets, _) = &*self.state;
        let mut tickets = tickets.lock().unwrap_or_else(|e| e.into_inner());
        tickets.next = tickets.next.wrapping_add(1) & i32::MAX as u32;
        let ticket = tickets.next;
        tickets.writes.insert(ticket, None);
        tickets.running += 1;
        drop(tickets);
        let sent = self.jobs.lock().unwrap_or_else(|e| e.into_inner()).send(Job { ticket, folder, slot, info, data });
        if sent.is_err() {
            let (tickets, _) = &*self.state;
            let mut tickets = tickets.lock().unwrap_or_else(|e| e.into_inner());
            tickets.writes.insert(ticket, Some(Err(SaveError::Failed("the save writer has stopped".into()))));
            tickets.running -= 1;
        }
        ticket
    }

    /// How a write went, once it has finished: None while it runs, unless
    /// `wait` says to wait for it. Asked about once finished, it is forgotten.
    fn status(&self, ticket: u32, wait: bool) -> Option<Result<(), SaveError>> {
        let (tickets, finished) = &*self.state;
        let mut tickets = tickets.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match tickets.writes.get(&ticket) {
                None => return Some(Ok(())),
                Some(Some(_)) => return tickets.writes.remove(&ticket).flatten(),
                Some(None) if !wait => return None,
                Some(None) => tickets = finished.wait(tickets).unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    fn flush(&self) {
        let (tickets, finished) = &*self.state;
        let mut tickets = tickets.lock().unwrap_or_else(|e| e.into_inner());
        while tickets.running > 0 {
            tickets = finished.wait(tickets).unwrap_or_else(|e| e.into_inner());
        }
    }
}

thread_local! {
    /// The message of the last failure on this thread, for `save_info`.
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    /// The last part read, kept between the call that asks its length and
    /// the one that copies it, so the file is read once.
    static LAST_READ: RefCell<Option<(String, i32, Vec<u8>)>> = const { RefCell::new(None) };
}

/// The failure's number for C#, keeping its message for `save_info`.
fn report(error: SaveError) -> i32 {
    LAST_ERROR.set(error.to_string());
    error.code()
}

/// The file beside the saves that keeps the language the player chose.
const LANGUAGE_FILE: &str = "language.txt";

/// The language the player last chose, by its code, kept beside the saves
/// so a game starts in it again; none when they never chose one.
pub fn saved_language() -> Option<String> {
    read_language(&settings().folder?)
}

/// Keeps the language the player chose, for [`saved_language`].
pub(crate) fn save_language(code: &str) {
    let Some(folder) = settings().folder else { return };
    if let Err(error) = write_language(&folder, code) {
        tracing::warn!("could not keep the language chosen: {error}");
    }
}

fn read_language(folder: &std::path::Path) -> Option<String> {
    let code = std::fs::read_to_string(folder.join(LANGUAGE_FILE)).ok()?;
    Some(code.trim().to_string()).filter(|code| !code.is_empty())
}

fn write_language(folder: &std::path::Path, code: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    std::fs::write(folder.join(LANGUAGE_FILE), code)
}

fn folder() -> Result<SaveFolder, SaveError> {
    settings().folder.map(SaveFolder::new).ok_or(SaveError::NoFolder)
}

/// Queues a slot's info and data to be written, returning a ticket for
/// `save_wait`, or a failure below 0.
pub(crate) unsafe extern "C" fn save_write(
    slot: *const u8,
    slot_len: i32,
    info: *const u8,
    info_len: i32,
    data: *const u8,
    data_len: i64,
) -> i32 {
    let slot = unsafe { utf8(slot, slot_len) }.into_owned();
    let folder = match folder().and_then(|folder| check_name(&slot).map(|()| folder)) {
        Ok(folder) => folder,
        Err(error) => return report(error),
    };
    let info = unsafe { bytes(info, info_len as i64) }.to_vec();
    let data = unsafe { bytes(data, data_len) }.to_vec();
    Writer::get().queue(folder, slot, info, data) as i32
}

/// 1 once a write has finished, 0 while it runs, or its failure below 0.
pub(crate) unsafe extern "C" fn save_wait(ticket: i32, wait: i32) -> i32 {
    match Writer::get().status(ticket as u32, wait != 0) {
        None => 0,
        Some(Ok(())) => 1,
        Some(Err(error)) => report(error),
    }
}

/// A slot's part (0 info, 1 picture, 2 data), copied when it fits, and its
/// length returned either way, or a failure below 0.
pub(crate) unsafe extern "C" fn save_read(
    slot: *const u8,
    slot_len: i32,
    part: i32,
    buffer: *mut u8,
    capacity: i64,
) -> i64 {
    let slot = unsafe { utf8(slot, slot_len) }.into_owned();
    let Some(which) = SavePart::from_code(part) else {
        return i64::from(report(SaveError::Failed("no such part".into())));
    };
    let kept = LAST_READ.take().filter(|(name, kept, _)| *name == slot && *kept == part);
    let bytes = match kept {
        Some((_, _, bytes)) => bytes,
        None => {
            flush();
            match folder().and_then(|folder| folder.read(&slot, which)) {
                Ok(bytes) => bytes,
                Err(error) => return i64::from(report(error)),
            }
        }
    };
    let length = bytes.len() as i64;
    if !buffer.is_null() && length <= capacity {
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
    } else {
        LAST_READ.set(Some((slot, part, bytes)));
    }
    length
}

/// Deletes a slot (0), copies it to `other` (1), renames it to `other` (2),
/// or tells whether it exists (3): 1 for done or yes, 0 for no such slot to
/// delete or no, or a failure below 0.
pub(crate) unsafe extern "C" fn save_change(
    change: i32,
    slot: *const u8,
    slot_len: i32,
    other: *const u8,
    other_len: i32,
) -> i32 {
    let slot = unsafe { utf8(slot, slot_len) };
    let other = unsafe { utf8(other, other_len) };
    flush();
    let changed = folder().and_then(|folder| match change {
        0 => folder.delete(&slot),
        1 => folder.copy(&slot, &other).map(|()| true),
        2 => folder.rename(&slot, &other).map(|()| true),
        _ => folder.exists(&slot),
    });
    match changed {
        Ok(done) => i32::from(done),
        Err(error) => report(error),
    }
}

/// What scripts ask about saves, written when it fits, its length returned
/// either way: the slots as JSON (0), the save folder (1), the format to
/// write when a script names none (2), and the last failure's message (3).
pub(crate) unsafe extern "C" fn save_info(what: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let text = match what {
        0 => {
            flush();
            match folder().and_then(|folder| folder.list()) {
                Ok(slots) => slots_json(&slots),
                Err(SaveError::NoFolder) => "[]".into(),
                Err(error) => return report(error),
            }
        }
        1 => settings().folder.map(|folder| folder.to_string_lossy().into_owned()).unwrap_or_default(),
        2 => if settings().readable { "json" } else { "binary" }.into(),
        _ => LAST_ERROR.with_borrow(Clone::clone),
    };
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

/// `[{ name, time, size, picture, info }]`, the info as scripts wrote it, or
/// null when it is not JSON.
fn slots_json(slots: &[SlotEntry]) -> String {
    let listed: Vec<serde_json::Value> = slots
        .iter()
        .map(|slot| {
            let info = serde_json::from_slice::<serde_json::Value>(&slot.info).unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "name": slot.name,
                "time": slot.written,
                "size": slot.size,
                "picture": slot.has_picture,
                "info": info,
            })
        })
        .collect();
    serde_json::to_string(&listed).unwrap_or_else(|_| "[]".into())
}

unsafe fn bytes<'a>(data: *const u8, len: i64) -> &'a [u8] {
    if data.is_null() || len <= 0 { &[] } else { unsafe { std::slice::from_raw_parts(data, len as usize) } }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary one, gone afterwards.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn folder() -> (Scratch, SaveFolder) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("voxear-saves-{}-{n}", std::process::id()));
        let folder = SaveFolder::new(dir.join("Saves"));
        (Scratch(dir), folder)
    }

    #[test]
    fn the_language_chosen_is_kept_beside_the_saves() {
        let (dir, _) = folder();
        let saves = dir.0.join("Saves");
        assert_eq!(read_language(&saves), None, "none chosen yet");
        write_language(&saves, "pt-BR").unwrap();
        assert_eq!(read_language(&saves).as_deref(), Some("pt-BR"));
    }

    #[test]
    fn a_slot_round_trips_and_lists_newest_first() {
        let (_dir, folder) = folder();
        assert_eq!(folder.list().unwrap(), Vec::new(), "no folder yet is no slots");
        folder.write("first", br#"{"version":1}"#, &[], b"one").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        folder.write("Second Go", br#"{"version":2}"#, b"png", b"two").unwrap();

        assert_eq!(folder.read("first", SavePart::Data).unwrap(), b"one");
        assert_eq!(folder.read("Second Go", SavePart::Picture).unwrap(), b"png");
        assert_eq!(folder.read("second go", SavePart::Info).unwrap(), br#"{"version":2}"#, "names match ignoring case");
        let listed = folder.list().unwrap();
        let names: Vec<&str> = listed.iter().map(|slot| slot.name.as_str()).collect();
        assert_eq!(names, ["Second Go", "first"]);
        assert!(listed[0].has_picture && !listed[1].has_picture);
        assert_eq!(listed[1].size, (HEADER_LEN + 13 + 3) as u64);
    }

    #[test]
    fn writing_again_replaces_and_leaves_nothing_behind() {
        let (_dir, folder) = folder();
        folder.write("slot", b"{}", &[], b"old").unwrap();
        folder.write("SLOT", b"{}", &[], b"new").unwrap();
        assert_eq!(folder.read("slot", SavePart::Data).unwrap(), b"new");
        let files: Vec<String> = fs::read_dir(folder.root())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["slot.save"], "one file, in the case it was first written");
    }

    #[test]
    fn slots_copy_rename_and_delete() {
        let (_dir, folder) = folder();
        folder.write("a", b"{}", &[], b"data").unwrap();
        folder.copy("a", "b").unwrap();
        folder.rename("b", "c").unwrap();
        assert!(!folder.exists("b").unwrap());
        assert_eq!(folder.read("c", SavePart::Data).unwrap(), b"data");
        assert!(folder.delete("a").unwrap());
        assert!(!folder.delete("a").unwrap(), "nothing left to delete");
        assert_eq!(folder.copy("a", "d"), Err(SaveError::Missing("a".into())));
        folder.rename("c", "C").unwrap();
        assert_eq!(folder.list().unwrap()[0].name, "C", "a rename may change only the case");
    }

    #[test]
    fn a_damaged_save_says_so() {
        let (_dir, folder) = folder();
        folder.write("slot", b"{}", &[], b"precious").unwrap();
        let path = folder.find("slot").unwrap().unwrap();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, &bytes).unwrap();
        let error = folder.read("slot", SavePart::Data).unwrap_err();
        assert!(matches!(&error, SaveError::Damaged(why) if why.contains("changed")), "{error}");

        fs::write(&path, &bytes[..30]).unwrap();
        assert!(matches!(folder.read("slot", SavePart::Data), Err(SaveError::Damaged(_))));
        assert_eq!(folder.list().unwrap(), Vec::new(), "left out of the list");
        fs::write(&path, b"not a save at all, but long enough to have a header's worth").unwrap();
        let error = folder.read("slot", SavePart::Info).unwrap_err();
        assert!(error.to_string().contains("not a save file"), "{error}");
    }

    #[test]
    fn names_that_cannot_be_files_are_refused() {
        let long = "x".repeat(65);
        for bad in ["", " ", "a/b", "..", "con", "Com1.x", ".hidden", "trailing.", &long] {
            assert!(matches!(check_name(bad), Err(SaveError::BadName(_))), "{bad:?}");
        }
        for good in ["slot1", "Slot 2", "auto-save_3", "Ünïcödé", "Chapter (2)", "console"] {
            assert!(check_name(good).is_ok(), "{good:?}");
        }
    }

    #[test]
    fn queued_writes_finish_in_order() {
        let (_dir, folder) = folder();
        let writer = Writer::get();
        let tickets: Vec<u32> = (0..20)
            .map(|n| writer.queue(folder.clone(), "slot".into(), b"{}".to_vec(), n.to_string().into_bytes()))
            .collect();
        writer.flush();
        for ticket in tickets {
            assert_eq!(writer.status(ticket, false), Some(Ok(())));
        }
        assert_eq!(folder.read("slot", SavePart::Data).unwrap(), b"19");
        let bad = writer.queue(SaveFolder::new(folder.root().join("slot.save")), "x".into(), Vec::new(), Vec::new());
        assert!(matches!(writer.status(bad, true), Some(Err(SaveError::Failed(_)))), "a folder that is a file fails");
    }
}
