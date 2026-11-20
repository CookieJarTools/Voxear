//! What a session's ends say to each other, packed as few bits as they need.

use voxear_core::math::I64Vec3;
use voxear_input::ActionState;
use voxear_net::{BitReader, BitWriter};

use super::PlayerId;
use super::replication::{
    FIELDS, Fields, NetId, ObjState, Origin, read_fields, read_position, read_state, write_fields, write_position,
    write_state,
};

/// A client's input on one tick: every action, by its place in the
/// project's input actions, with what went down or up since the tick before.
pub type Command = Vec<ActionState>;

/// How far ahead a client's commands come, when none has come yet.
pub const UNHEARD: i16 = i16::MIN;

/// The most ticks of input one message carries: each is sent this many
/// times, so a few lost packets lose none.
pub const REPEATS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// To a client that has joined: who it is, the server's tick, how many
    /// ticks a second the server runs, and whether players' own Objs are
    /// theirs to run.
    Welcome { player: PlayerId, tick: u64, rate: u16, shared: bool },
    Joined(PlayerId),
    Left(PlayerId),
    /// The level the server plays, by GUID, for the client to load.
    Level(String),
    /// A client's last few ticks of input, the oldest first, and how many
    /// ticks behind the newest the server's world it showed was, for the
    /// server to see what the player saw.
    Inputs { first: u64, commands: Vec<Command>, behind: f32 },
    /// The server's tick, and how many ticks ahead of it the client's input
    /// arrives, so the client can run a little faster or slower to keep it
    /// a tick or two ahead.
    Sync { tick: u64, ahead: i16 },
    /// An Obj to make, its state on a tick, in a dimension by its number on
    /// this connection (0 the level's).
    Spawn {
        id: NetId,
        owner: Option<PlayerId>,
        origin: Origin,
        dimension: u16,
        tick: u64,
        state: ObjState,
        fields: Fields,
    },
    /// An Obj taken away: `gone` where it is no more on the server, else no
    /// longer the player's to see, as it went out of range or into a
    /// dimension they are not in.
    Despawn {
        id: NetId,
        gone: bool,
    },
    /// Objs' states on a server tick: each Obj's parts that changed, as a
    /// mask, over a state with the rest left as they were.
    Snapshot { tick: u64, anchor: Option<I64Vec3>, entries: Vec<(NetId, u8, ObjState, Option<Fields>)> },
    /// A script's call of a method on a networked Obj's behaviour, by a key:
    /// to the server from a player, or to players from the server, with the
    /// tick it was made on.
    Rpc { id: NetId, key: u32, tick: u64, bytes: Vec<u8> },
    /// A player's own Obj's replicated properties, from their machine, when
    /// players run their own.
    Fields { id: NetId, fields: Fields },
    /// A player asking for an Obj no one owns, when players run their own.
    Claim(NetId),
    /// An Obj's new owner, from the server.
    Owner { id: NetId, owner: Option<PlayerId> },
    /// The server's storms: their clock, those scripts made and the
    /// whirlpools, as `storms::StormsSaved::to_bytes` gives them.
    Storms(Vec<u8>),
    /// To a client: the dimension a number stands for on this connection,
    /// for it to make and keep until told to let it go. Its edits follow.
    Dimension {
        number: u16,
        what: DimensionWhat,
    },
    /// To a client: a dimension it may let go, as its own rules say.
    LetGo(u16),
    /// An Obj went into another dimension on a tick: from the server, where
    /// it decided so; from a player whose machine runs its own Objs, for the
    /// server to check.
    Crossed {
        id: NetId,
        dimension: u16,
        tick: u64,
    },
    /// To a client: some of a dimension's edits, those nearest where the
    /// player comes into it first, for one of its worlds of blocks, by its
    /// place among them (see `dimensions`). `numbered` are the blocks the
    /// server's game numbered as it ran, each by its number, its GUID and
    /// its state; `bytes` the changes as saves write them (`VXBLOCKS`).
    Edits {
        dimension: u16,
        world: u16,
        numbered: Vec<(u16, String, u8)>,
        bytes: Vec<u8>,
    },
    /// To a client: every edit of a dimension has been sent.
    EditsDone(u16),
    /// To the server: a dimension's edits are in, so the player may go in.
    Ready(u16),
    /// To the server: a player's script sending a Nav Agent the server
    /// moves somewhere, or stopping it: where to, in a dimension by its
    /// number on this connection, and whether it stands where it is.
    NavGo { id: NetId, to: Option<(voxear_core::math::Vec3, u16)>, stopped: bool },
}

/// What a dimension a server names to a client is, for the client to find
/// or make its own.
#[derive(Clone, Debug, PartialEq)]
pub enum DimensionWhat {
    /// Made from a `.dimension` asset, by its GUID, as `key` (0 where it
    /// makes one).
    Asset { guid: String, key: u64 },
    /// A dungeon's own, made from its gate: the world of blocks the gate is
    /// in, by its scene's GUID and its path there, which of that world's
    /// structures the dungeon is, its site and the site's seed.
    Dungeon { scene: String, path: String, structure: u16, site: [f32; 2], seed: u32 },
}

/// The most bytes an RPC's arguments take.
pub const MOST_RPC_BYTES: usize = 8192;

/// A state with nothing set, for parts a snapshot does not carry.
pub const BLANK: ObjState = ObjState {
    position: voxear_core::math::Vec3::ZERO,
    rotation: voxear_core::math::Quat::IDENTITY,
    scale: voxear_core::math::Vec3::ONE,
    mist: None,
    root: true,
    parent: None,
};

const WELCOME: u32 = 0;
const JOINED: u32 = 1;
const LEFT: u32 = 2;
const LEVEL: u32 = 3;
const INPUTS: u32 = 4;
const SYNC: u32 = 5;
const SPAWN: u32 = 6;
const DESPAWN: u32 = 7;
pub const SNAPSHOT: u32 = 8;
const RPC: u32 = 9;
const FIELDS_MESSAGE: u32 = 10;
const CLAIM: u32 = 11;
const OWNER: u32 = 12;
const STORMS: u32 = 13;
const DIMENSION: u32 = 14;
const LET_GO: u32 = 15;
const CROSSED: u32 = 16;
const EDITS: u32 = 17;
const EDITS_DONE: u32 = 18;
const READY: u32 = 19;
const NAV_GO: u32 = 20;
/// The most bytes one message of a dimension's edits takes.
pub const MOST_EDIT_BYTES: usize = 65536;
/// The most bytes a message of storms takes.
pub const MOST_STORM_BYTES: usize = 65536;
/// Bits a message's kind takes.
pub const KIND_BITS: u32 = 5;

impl Message {
    /// The message as bytes, from a machine whose world's middle is
    /// `origin` from the level's (see `replication`).
    pub fn encode(&self, world_origin: I64Vec3) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            Message::Welcome { player, tick, rate, shared } => {
                w.write_bits(WELCOME, KIND_BITS);
                w.write_bits(u32::from(player.0), 16);
                w.write_varint(*tick);
                w.write_bits(u32::from(*rate), 16);
                w.write_bool(*shared);
            }
            Message::Joined(player) => {
                w.write_bits(JOINED, KIND_BITS);
                w.write_bits(u32::from(player.0), 16);
            }
            Message::Left(player) => {
                w.write_bits(LEFT, KIND_BITS);
                w.write_bits(u32::from(player.0), 16);
            }
            Message::Level(level) => {
                w.write_bits(LEVEL, KIND_BITS);
                w.write_varint(level.len() as u64);
                w.write_bytes(level.as_bytes());
            }
            Message::Storms(bytes) => {
                w.write_bits(STORMS, KIND_BITS);
                w.write_varint(bytes.len() as u64);
                w.write_bytes(bytes);
            }
            Message::Inputs { first, commands, behind } => {
                w.write_bits(INPUTS, KIND_BITS);
                w.write_varint(*first);
                w.write_bits(commands.len() as u32, 4);
                for command in commands {
                    write_command(&mut w, command);
                }
                // Sixteenths of a tick, up to 64 ticks.
                w.write_bits((behind * 16.0).round().clamp(0.0, 1023.0) as u32, 10);
            }
            Message::Sync { tick, ahead } => {
                w.write_bits(SYNC, KIND_BITS);
                w.write_varint(*tick);
                w.write_bits(*ahead as u16 as u32, 16);
            }
            Message::Spawn { id, owner, origin, dimension, tick, state, fields } => {
                w.write_bits(SPAWN, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_bool(owner.is_some());
                w.write_bits(u32::from(owner.map_or(0, |p| p.0)), 16);
                match origin {
                    Origin::Placed { scene, path } => {
                        w.write_bool(true);
                        write_text(&mut w, scene);
                        write_text(&mut w, path);
                    }
                    Origin::Prefab(prefab) => {
                        w.write_bool(false);
                        write_text(&mut w, prefab);
                    }
                }
                w.write_varint(u64::from(*dimension));
                w.write_varint(*tick);
                write_state(&mut w, state, super::replication::ALL, world_origin, None);
                write_fields(&mut w, fields);
            }
            Message::Despawn { id, gone } => {
                w.write_bits(DESPAWN, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_bool(*gone);
            }
            Message::Dimension { number, what } => {
                w.write_bits(DIMENSION, KIND_BITS);
                w.write_varint(u64::from(*number));
                match what {
                    DimensionWhat::Asset { guid, key } => {
                        w.write_bool(false);
                        write_text(&mut w, guid);
                        w.write_varint(*key);
                    }
                    DimensionWhat::Dungeon { scene, path, structure, site, seed } => {
                        w.write_bool(true);
                        write_text(&mut w, scene);
                        write_text(&mut w, path);
                        w.write_varint(u64::from(*structure));
                        w.write_f32(site[0]);
                        w.write_f32(site[1]);
                        w.write_bits(*seed, 32);
                    }
                }
            }
            Message::LetGo(number) => {
                w.write_bits(LET_GO, KIND_BITS);
                w.write_varint(u64::from(*number));
            }
            Message::Crossed { id, dimension, tick } => {
                w.write_bits(CROSSED, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_varint(u64::from(*dimension));
                w.write_varint(*tick);
            }
            Message::Edits { dimension, world, numbered, bytes } => {
                w.write_bits(EDITS, KIND_BITS);
                w.write_varint(u64::from(*dimension));
                w.write_varint(u64::from(*world));
                w.write_varint(numbered.len() as u64);
                for (number, guid, state) in numbered {
                    w.write_varint(u64::from(*number));
                    write_text(&mut w, guid);
                    w.write_bits(u32::from(*state), 8);
                }
                w.write_varint(bytes.len() as u64);
                w.write_bytes(bytes);
            }
            Message::EditsDone(number) => {
                w.write_bits(EDITS_DONE, KIND_BITS);
                w.write_varint(u64::from(*number));
            }
            Message::Ready(number) => {
                w.write_bits(READY, KIND_BITS);
                w.write_varint(u64::from(*number));
            }
            Message::NavGo { id, to, stopped } => {
                w.write_bits(NAV_GO, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_bool(*stopped);
                w.write_bool(to.is_some());
                if let Some((point, dimension)) = to {
                    write_position(&mut w, *point, true, world_origin);
                    w.write_varint(u64::from(*dimension));
                }
            }
            Message::Snapshot { tick, anchor, entries } => {
                w.write_bits(SNAPSHOT, KIND_BITS);
                w.write_varint(*tick);
                super::replication::write_anchor(&mut w, *anchor);
                for (id, mask, state, fields) in entries {
                    w.write_bool(true);
                    w.write_varint(u64::from(id.0));
                    write_state(&mut w, state, *mask, world_origin, *anchor);
                    if mask & FIELDS != 0 {
                        write_fields(&mut w, fields.as_ref().unwrap_or(&Fields::new()));
                    }
                }
                w.write_bool(false);
            }
            Message::Fields { id, fields } => {
                w.write_bits(FIELDS_MESSAGE, KIND_BITS);
                w.write_varint(u64::from(id.0));
                write_fields(&mut w, fields);
            }
            Message::Claim(id) => {
                w.write_bits(CLAIM, KIND_BITS);
                w.write_varint(u64::from(id.0));
            }
            Message::Owner { id, owner } => {
                w.write_bits(OWNER, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_bool(owner.is_some());
                w.write_bits(u32::from(owner.map_or(0, |p| p.0)), 16);
            }
            Message::Rpc { id, key, tick, bytes } => {
                w.write_bits(RPC, KIND_BITS);
                w.write_varint(u64::from(id.0));
                w.write_bits(*key, 32);
                w.write_varint(*tick);
                w.write_varint(bytes.len() as u64);
                w.write_bytes(bytes);
            }
        }
        w.finish()
    }

    /// The message in `bytes`, or `None` for anything malformed, on a
    /// machine whose world's middle is `origin` from the level's.
    pub fn decode(bytes: &[u8], world_origin: I64Vec3) -> Option<Message> {
        let mut r = BitReader::new(bytes);
        Some(match r.read_bits(KIND_BITS)? {
            WELCOME => Message::Welcome {
                player: PlayerId(r.read_bits(16)? as u16),
                tick: r.read_varint()?,
                rate: r.read_bits(16)? as u16,
                shared: r.read_bool()?,
            },
            FIELDS_MESSAGE => Message::Fields { id: NetId(r.read_varint()? as u32), fields: read_fields(&mut r)? },
            CLAIM => Message::Claim(NetId(r.read_varint()? as u32)),
            OWNER => {
                let id = NetId(r.read_varint()? as u32);
                let has_owner = r.read_bool()?;
                let owner = PlayerId(r.read_bits(16)? as u16);
                Message::Owner { id, owner: has_owner.then_some(owner) }
            }
            JOINED => Message::Joined(PlayerId(r.read_bits(16)? as u16)),
            LEFT => Message::Left(PlayerId(r.read_bits(16)? as u16)),
            LEVEL => {
                let length = r.read_varint()? as usize;
                if length > 4096 {
                    return None;
                }
                let mut text = vec![0u8; length];
                r.read_bytes(&mut text)?;
                Message::Level(String::from_utf8(text).ok()?)
            }
            STORMS => {
                let length = r.read_varint()? as usize;
                if length > MOST_STORM_BYTES {
                    return None;
                }
                let mut bytes = vec![0u8; length];
                r.read_bytes(&mut bytes)?;
                Message::Storms(bytes)
            }
            INPUTS => {
                let first = r.read_varint()?;
                let count = r.read_bits(4)? as usize;
                let commands = (0..count).map(|_| read_command(&mut r)).collect::<Option<_>>()?;
                let behind = r.read_bits(10)? as f32 / 16.0;
                Message::Inputs { first, commands, behind }
            }
            SYNC => Message::Sync { tick: r.read_varint()?, ahead: r.read_bits(16)? as u16 as i16 },
            SPAWN => {
                let id = NetId(r.read_varint()? as u32);
                let has_owner = r.read_bool()?;
                let owner = PlayerId(r.read_bits(16)? as u16);
                let origin = match r.read_bool()? {
                    true => Origin::Placed { scene: read_text(&mut r)?, path: read_text(&mut r)? },
                    false => Origin::Prefab(read_text(&mut r)?),
                };
                let dimension = read_number(&mut r)?;
                let tick = r.read_varint()?;
                let mut state = BLANK;
                read_state(&mut r, &mut state, world_origin, None)?;
                let fields = read_fields(&mut r)?;
                Message::Spawn { id, owner: has_owner.then_some(owner), origin, dimension, tick, state, fields }
            }
            DESPAWN => Message::Despawn { id: NetId(r.read_varint()? as u32), gone: r.read_bool()? },
            DIMENSION => {
                let number = read_number(&mut r)?;
                let what = match r.read_bool()? {
                    false => DimensionWhat::Asset { guid: read_text(&mut r)?, key: r.read_varint()? },
                    true => DimensionWhat::Dungeon {
                        scene: read_text(&mut r)?,
                        path: read_text(&mut r)?,
                        structure: u16::try_from(r.read_varint()?).ok()?,
                        site: [r.read_f32()?, r.read_f32()?],
                        seed: r.read_bits(32)?,
                    },
                };
                Message::Dimension { number, what }
            }
            LET_GO => Message::LetGo(read_number(&mut r)?),
            CROSSED => Message::Crossed {
                id: NetId(r.read_varint()? as u32),
                dimension: read_number(&mut r)?,
                tick: r.read_varint()?,
            },
            EDITS => {
                let (dimension, world) = (read_number(&mut r)?, read_number(&mut r)?);
                let count = r.read_varint()? as usize;
                if count > 4096 {
                    return None;
                }
                let numbered = (0..count)
                    .map(|_| Some((read_number(&mut r)?, read_text(&mut r)?, r.read_bits(8)? as u8)))
                    .collect::<Option<Vec<_>>>()?;
                let length = r.read_varint()? as usize;
                if length > MOST_EDIT_BYTES {
                    return None;
                }
                let mut bytes = vec![0u8; length];
                r.read_bytes(&mut bytes)?;
                Message::Edits { dimension, world, numbered, bytes }
            }
            EDITS_DONE => Message::EditsDone(read_number(&mut r)?),
            READY => Message::Ready(read_number(&mut r)?),
            NAV_GO => {
                let id = NetId(r.read_varint()? as u32);
                let stopped = r.read_bool()?;
                let to = match r.read_bool()? {
                    true => Some((read_position(&mut r, world_origin)?.0, read_number(&mut r)?)),
                    false => None,
                };
                Message::NavGo { id, to, stopped }
            }
            SNAPSHOT => {
                let tick = r.read_varint()?;
                let anchor = super::replication::read_anchor(&mut r)?;
                let mut entries = Vec::new();
                while r.read_bool()? {
                    let id = NetId(r.read_varint()? as u32);
                    let mut state = BLANK;
                    let mask = read_state(&mut r, &mut state, world_origin, anchor)?;
                    let fields = if mask & FIELDS != 0 { Some(read_fields(&mut r)?) } else { None };
                    entries.push((id, mask, state, fields));
                }
                Message::Snapshot { tick, anchor, entries }
            }
            RPC => {
                let id = NetId(r.read_varint()? as u32);
                let key = r.read_bits(32)?;
                let tick = r.read_varint()?;
                let length = r.read_varint()? as usize;
                if length > MOST_RPC_BYTES {
                    return None;
                }
                let mut bytes = vec![0u8; length];
                r.read_bytes(&mut bytes)?;
                Message::Rpc { id, key, tick, bytes }
            }
            _ => return None,
        })
    }
}

/// A number of 16 bits written as a varint, as a dimension's is.
fn read_number(r: &mut BitReader) -> Option<u16> {
    u16::try_from(r.read_varint()?).ok()
}

fn write_text(w: &mut BitWriter, text: &str) {
    w.write_varint(text.len() as u64);
    w.write_bytes(text.as_bytes());
}

fn read_text(r: &mut BitReader) -> Option<String> {
    let length = r.read_varint()? as usize;
    if length > 4096 {
        return None;
    }
    let mut text = vec![0u8; length];
    r.read_bytes(&mut text)?;
    String::from_utf8(text).ok()
}

/// An action costs five bits when still, and its values when it has them.
fn write_command(w: &mut BitWriter, command: &Command) {
    w.write_varint(command.len() as u64);
    for state in command {
        w.write_bool(state.held);
        w.write_bool(state.pressed);
        w.write_bool(state.released);
        w.write_bool(state.performed);
        let moved = state.value != [0.0; 2];
        w.write_bool(moved);
        if moved {
            w.write_f32(state.value[0]);
            w.write_f32(state.value[1]);
        }
    }
}

fn read_command(r: &mut BitReader) -> Option<Command> {
    let count = r.read_varint()? as usize;
    // No game has this many actions; a packet saying so is garbled.
    if count > 1024 {
        return None;
    }
    (0..count)
        .map(|_| {
            let (held, pressed, released, performed) = (r.read_bool()?, r.read_bool()?, r.read_bool()?, r.read_bool()?);
            let value = if r.read_bool()? { [r.read_f32()?, r.read_f32()?] } else { [0.0; 2] };
            Some(ActionState { value, held, pressed, released, performed, held_for: 0.0 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_comes_back_as_sent() {
        let jump = ActionState { held: true, pressed: true, ..ActionState::default() };
        let look = ActionState { value: [0.25, -3.5], ..ActionState::default() };
        let messages = [
            Message::Welcome { player: PlayerId(3), tick: 123_456, rate: 60, shared: true },
            Message::Fields { id: NetId(4), fields: [(9, vec![1, 2])].into_iter().collect() },
            Message::Claim(NetId(5)),
            Message::Owner { id: NetId(5), owner: Some(PlayerId(2)) },
            Message::Owner { id: NetId(5), owner: None },
            Message::Joined(PlayerId(7)),
            Message::Left(PlayerId(7)),
            Message::Level("0123456789abcdef0123456789abcdef".into()),
            Message::Inputs { first: 99, commands: vec![vec![jump, look], vec![ActionState::default(); 2]], behind: 4.5 },
            Message::Sync { tick: 5, ahead: -3 },
            Message::Rpc { id: NetId(12), key: 0xdead_beef, tick: 77, bytes: vec![1, 2, 3] },
            Message::Storms(b"STM1 and the rest".to_vec()),
            Message::Despawn { id: NetId(8), gone: false },
            Message::Dimension { number: 3, what: DimensionWhat::Asset { guid: "0123456789abcdef".into(), key: 9 } },
            Message::Dimension {
                number: 4,
                what: DimensionWhat::Dungeon {
                    scene: "fedcba9876543210".into(),
                    path: "2/7".into(),
                    structure: 1,
                    site: [1200.5, -77.25],
                    seed: 0xfeed_beef,
                },
            },
            Message::LetGo(3),
            Message::Crossed { id: NetId(12), dimension: 4, tick: 901 },
            Message::Edits { dimension: 4, world: 0, numbered: vec![(40, "abc".into(), 2)], bytes: vec![9; 300] },
            Message::EditsDone(4),
            Message::Ready(4),
            Message::NavGo {
                id: NetId(6),
                to: Some((voxear_core::math::Vec3::new(12.5, 3.0, -40.25), 2)),
                stopped: false,
            },
            Message::NavGo { id: NetId(6), to: None, stopped: true },
        ];
        for message in messages {
            let sent = message.encode(I64Vec3::ZERO);
            assert_eq!(Message::decode(&sent, I64Vec3::ZERO), Some(message.clone()), "{message:?}");
        }
        let still = Message::Inputs { first: 0, commands: vec![vec![ActionState::default(); 20]], behind: 0.0 };
        let still = still.encode(I64Vec3::ZERO);
        assert!(still.len() <= 18, "twenty still actions, and what the player saw, in {} bytes", still.len());
        assert_eq!(Message::decode(&[0xFF], I64Vec3::ZERO), None);
        assert_eq!(Message::decode(&[], I64Vec3::ZERO), None);
    }
}
