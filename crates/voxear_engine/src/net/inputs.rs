//! Input as ticks: a client's frames gathered into one command a tick, and
//! the server's buffer of each player's commands, taken one a tick.

use std::collections::BTreeMap;

use voxear_input::ActionState;

use super::messages::Command;

/// A client's input since the last tick: the actions as they are now, with
/// any press, release or performance on a frame in between kept, so a tap
/// shorter than a tick is never lost.
#[derive(Debug, Default)]
pub struct Latch {
    now: Command,
    edges: Vec<(bool, bool, bool)>,
}

impl Latch {
    /// Takes in one frame's actions.
    pub fn note(&mut self, frame: &[ActionState]) {
        self.edges.resize(frame.len(), (false, false, false));
        for (edge, state) in self.edges.iter_mut().zip(frame) {
            edge.0 |= state.pressed;
            edge.1 |= state.released;
            edge.2 |= state.performed;
        }
        self.now.clear();
        self.now.extend_from_slice(frame);
    }

    /// The command for a tick, starting the next afresh.
    pub fn take(&mut self) -> Command {
        let command = self
            .now
            .iter()
            .zip(&self.edges)
            .map(|(state, &(pressed, released, performed))| ActionState { pressed, released, performed, ..*state })
            .collect();
        self.edges.iter_mut().for_each(|edge| *edge = (false, false, false));
        command
    }
}

/// How many ticks a player's last command stands in for missing ones before
/// they count as letting go of everything.
const HOLD_FOR: u32 = 6;
/// How many ticks ahead a command may come; anything further is dropped.
const AHEAD_AT_MOST: u64 = 128;

/// One player's commands on the server, waiting for their ticks.
#[derive(Debug, Default)]
pub struct InputBuffer {
    commands: BTreeMap<u64, Command>,
    /// The newest tick a command has come for.
    newest: Option<u64>,
    /// The tick last taken.
    taken: Option<u64>,
    last: Command,
    missed: u32,
}

impl InputBuffer {
    /// Keeps a command for `tick`, unless that tick has passed or is
    /// implausibly far off.
    pub fn put(&mut self, tick: u64, command: Command, server_tick: u64) {
        if self.taken.is_some_and(|taken| tick <= taken) || tick > server_tick + AHEAD_AT_MOST {
            return;
        }
        self.newest = Some(self.newest.map_or(tick, |n| n.max(tick)));
        self.commands.entry(tick).or_insert(command);
    }

    /// The command for `tick`: its own, or the last one again, without its
    /// presses, while it is missing, or nothing held after a while.
    pub fn take(&mut self, tick: u64) -> Command {
        self.taken = Some(tick);
        let own = self.commands.remove(&tick);
        self.commands = self.commands.split_off(&(tick + 1));
        match own {
            Some(command) => {
                self.missed = 0;
                self.last = command;
            }
            None => {
                self.missed += 1;
                for state in &mut self.last {
                    (state.pressed, state.released, state.performed) = (false, false, false);
                    if self.missed > HOLD_FOR {
                        *state = ActionState::default();
                    }
                }
            }
        }
        // Held for counts this side, a tick at a time.
        self.last.clone()
    }

    /// How many ticks ahead of `tick` this player's commands come.
    pub fn ahead(&self, tick: u64) -> i64 {
        self.newest.map_or(0, |newest| newest as i64 - tick as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(held: bool, pressed: bool) -> ActionState {
        ActionState { held, pressed, ..ActionState::default() }
    }

    #[test]
    fn a_tap_between_ticks_is_kept() {
        let mut latch = Latch::default();
        latch.note(&[press(true, true)]);
        latch.note(&[press(false, false)]);
        let command = latch.take();
        assert!(command[0].pressed && !command[0].held, "pressed on a frame, let go by the tick");
        latch.note(&[press(false, false)]);
        assert!(!latch.take()[0].pressed, "only once");
    }

    #[test]
    fn missing_commands_repeat_the_last_then_let_go() {
        let mut buffer = InputBuffer::default();
        buffer.put(10, vec![press(true, true)], 10);
        buffer.put(9, vec![press(false, false)], 10);
        assert!(buffer.take(10)[0].pressed);
        buffer.put(10, vec![press(false, false)], 11);
        let repeated = buffer.take(11);
        assert!(repeated[0].held && !repeated[0].pressed, "held on, not pressed again");
        for tick in 12..20 {
            buffer.take(tick);
        }
        assert!(!buffer.take(20)[0].held, "let go after a while");
        buffer.put(25, vec![press(true, false)], 21);
        assert_eq!(buffer.ahead(21), 4);
    }
}
