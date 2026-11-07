//! What a session's ends say to each other, packed as few bits as they need.

use voxear_input::ActionState;
use voxear_net::{BitReader, BitWriter};

use super::PlayerId;

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
    /// A client's last few ticks of input, the oldest first.
    Inputs { first: u64, commands: Vec<Command> },
    /// The server's tick, and how many ticks ahead of it the client's input
    /// arrives, so the client can run a little faster or slower to keep it
    /// a tick or two ahead.
    Sync { tick: u64, ahead: i16 },
}

const WELCOME: u32 = 0;
const JOINED: u32 = 1;
const LEFT: u32 = 2;
const LEVEL: u32 = 3;
const INPUTS: u32 = 4;
const SYNC: u32 = 5;

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            Message::Welcome { player, tick, rate } => {
                w.write_bits(WELCOME, 3);
                w.write_bits(u32::from(player.0), 16);
                w.write_varint(*tick);
                w.write_bits(u32::from(*rate), 16);
            }
            Message::Joined(player) => {
                w.write_bits(JOINED, 3);
                w.write_bits(u32::from(player.0), 16);
            }
            Message::Left(player) => {
                w.write_bits(LEFT, 3);
                w.write_bits(u32::from(player.0), 16);
            }
            Message::Level(level) => {
                w.write_bits(LEVEL, 3);
                w.write_varint(level.len() as u64);
                w.write_bytes(level.as_bytes());
            }
            Message::Inputs { first, commands } => {
                w.write_bits(INPUTS, 3);
                w.write_varint(*first);
                w.write_bits(commands.len() as u32, 4);
                for command in commands {
                    write_command(&mut w, command);
                }
            }
            Message::Sync { tick, ahead } => {
                w.write_bits(SYNC, 3);
                w.write_varint(*tick);
                w.write_bits(*ahead as u16 as u32, 16);
            }
        }
        w.finish()
    }

    /// The message in `bytes`, or `None` for anything malformed.
    pub fn decode(bytes: &[u8]) -> Option<Message> {
        let mut r = BitReader::new(bytes);
        Some(match r.read_bits(3)? {
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
                Message::Inputs { first, commands }
            }
            SYNC => Message::Sync { tick: r.read_varint()?, ahead: r.read_bits(16)? as u16 as i16 },
            _ => return None,
        })
    }
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
            Message::Inputs { first: 99, commands: vec![vec![jump, look], vec![ActionState::default(); 2]] },
            Message::Sync { tick: 5, ahead: -3 },
        ];
        for message in messages {
            assert_eq!(Message::decode(&message.encode()), Some(message.clone()), "{message:?}");
        }
        let still = Message::Inputs { first: 0, commands: vec![vec![ActionState::default(); 20]] }.encode();
        assert!(still.len() <= 16, "twenty still actions in {} bytes", still.len());
        assert_eq!(Message::decode(&[0xFF]), None);
        assert_eq!(Message::decode(&[]), None);
    }
}
