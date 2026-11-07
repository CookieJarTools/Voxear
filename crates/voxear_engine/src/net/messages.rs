//! What a session's ends say to each other, packed as few bits as they need.

use voxear_input::ActionState;
use voxear_net::{BitReader, BitWriter};

use super::PlayerId;
use super::replication::{FIELDS, Fields, NetId, ObjState, Origin, read_fields, read_state, write_fields, write_state};

/// A client's input on one tick: every action, by its place in the
/// project's input actions, with what went down or up since the tick before.
pub type Command = Vec<ActionState>;

/// The most ticks of input one message carries: each is sent this many
/// times, so a few lost packets lose none.
pub const REPEATS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// To a client that has joined: who it is, the server's tick, and how
    /// many ticks a second the server runs.
    Welcome { player: PlayerId, tick: u64, rate: u16 },
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
    /// An Obj to make, its state on a tick.
    Spawn { id: NetId, owner: Option<PlayerId>, origin: Origin, tick: u64, state: ObjState, fields: Fields },
    Despawn(NetId),
    /// Objs' states on a server tick: each Obj's parts that changed, as a
    /// mask, over a state with the rest left as they were.
    Snapshot { tick: u64, entries: Vec<(NetId, u8, ObjState, Option<Fields>)> },
    /// A script's call of a method on a networked Obj's behaviour, by a key:
    /// to the server from a player, or to players from the server, with the
    /// tick it was made on.
    Rpc { id: NetId, key: u32, tick: u64, bytes: Vec<u8> },
}

/// The most bytes an RPC's arguments take.
pub const MOST_RPC_BYTES: usize = 8192;

/// A state with nothing set, for parts a snapshot does not carry.
pub const BLANK: ObjState = ObjState {
    position: voxear_core::math::Vec3::ZERO,
    rotation: voxear_core::math::Quat::IDENTITY,
    scale: voxear_core::math::Vec3::ONE,
    mist: None,
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
/// Bits a message's kind takes.
pub const KIND_BITS: u32 = 4;

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            Message::Welcome { player, tick, rate } => {
                w.write_bits(WELCOME, KIND_BITS);
                w.write_bits(u32::from(player.0), 16);
                w.write_varint(*tick);
                w.write_bits(u32::from(*rate), 16);
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
            Message::Spawn { id, owner, origin, tick, state, fields } => {
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
                w.write_varint(*tick);
                write_state(&mut w, state, super::replication::ALL);
                write_fields(&mut w, fields);
            }
            Message::Despawn(id) => {
                w.write_bits(DESPAWN, KIND_BITS);
                w.write_varint(u64::from(id.0));
            }
            Message::Snapshot { tick, entries } => {
                w.write_bits(SNAPSHOT, KIND_BITS);
                w.write_varint(*tick);
                for (id, mask, state, fields) in entries {
                    w.write_bool(true);
                    w.write_varint(u64::from(id.0));
                    write_state(&mut w, state, *mask);
                    if mask & FIELDS != 0 {
                        write_fields(&mut w, fields.as_ref().unwrap_or(&Fields::new()));
                    }
                }
                w.write_bool(false);
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

    /// The message in `bytes`, or `None` for anything malformed.
    pub fn decode(bytes: &[u8]) -> Option<Message> {
        let mut r = BitReader::new(bytes);
        Some(match r.read_bits(KIND_BITS)? {
            WELCOME => Message::Welcome {
                player: PlayerId(r.read_bits(16)? as u16),
                tick: r.read_varint()?,
                rate: r.read_bits(16)? as u16,
            },
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
                let tick = r.read_varint()?;
                let mut state = BLANK;
                read_state(&mut r, &mut state)?;
                let fields = read_fields(&mut r)?;
                Message::Spawn { id, owner: has_owner.then_some(owner), origin, tick, state, fields }
            }
            DESPAWN => Message::Despawn(NetId(r.read_varint()? as u32)),
            SNAPSHOT => {
                let tick = r.read_varint()?;
                let mut entries = Vec::new();
                while r.read_bool()? {
                    let id = NetId(r.read_varint()? as u32);
                    let mut state = BLANK;
                    let mask = read_state(&mut r, &mut state)?;
                    let fields = if mask & FIELDS != 0 { Some(read_fields(&mut r)?) } else { None };
                    entries.push((id, mask, state, fields));
                }
                Message::Snapshot { tick, entries }
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
            Message::Welcome { player: PlayerId(3), tick: 123_456, rate: 60 },
            Message::Joined(PlayerId(7)),
            Message::Left(PlayerId(7)),
            Message::Level("0123456789abcdef0123456789abcdef".into()),
            Message::Inputs { first: 99, commands: vec![vec![jump, look], vec![ActionState::default(); 2]], behind: 4.5 },
            Message::Sync { tick: 5, ahead: -3 },
            Message::Rpc { id: NetId(12), key: 0xdead_beef, tick: 77, bytes: vec![1, 2, 3] },
        ];
        for message in messages {
            assert_eq!(Message::decode(&message.encode()), Some(message.clone()), "{message:?}");
        }
        let still = Message::Inputs { first: 0, commands: vec![vec![ActionState::default(); 20]], behind: 0.0 }.encode();
        assert!(still.len() <= 18, "twenty still actions, and what the player saw, in {} bytes", still.len());
        assert_eq!(Message::decode(&[0xFF]), None);
        assert_eq!(Message::decode(&[]), None);
    }
}
