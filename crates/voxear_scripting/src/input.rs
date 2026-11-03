//! The game's input, for `Input`, `InputAction`, `Keyboard`, `Mouse` and
//! `Gamepad` in C# (see `voxear_input`): actions found by name and read as
//! they are this frame, any control read by its path, text such as an
//! action's prompt, and changes: a rebinding, listening for one, saved
//! bindings read back, and maps turned on or off.

use voxear_input::{Input, Rebound, Scheme, actions::PARTS};

use crate::{interop::utf8, services::with_services};

/// An action as C# reads it: its value, and its phases as bits.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputActionData {
    pub value: [f32; 2],
    /// 1 held, 2 pressed this frame, 4 released this frame, 8 performed.
    pub flags: i32,
    pub held_for: f32,
}

/// What [`input_text`] writes.
const TEXT_PROMPT: i32 = 0;
const TEXT_SCHEME: i32 = 1;
const TEXT_SAVED: i32 = 2;
const TEXT_MAPS: i32 = 3;
const TEXT_BINDINGS: i32 = 4;
const TEXT_REBOUND: i32 = 5;
const TEXT_NAME: i32 = 6;
const TEXT_CATEGORIES: i32 = 7;
const TEXT_ACTIONS: i32 = 8;
const TEXT_PLAYERS: i32 = 9;
const TEXT_JOINED: i32 = 10;

/// What [`input_change`] does.
const CHANGE_REBIND: i32 = 0;
const CHANGE_LISTEN: i32 = 1;
const CHANGE_POLL: i32 = 2;
const CHANGE_LOAD: i32 = 3;
const CHANGE_RESET: i32 = 4;
const CHANGE_MAP: i32 = 5;
const CHANGE_CANCEL: i32 = 6;
const CHANGE_RESET_ACTION: i32 = 7;
const CHANGE_JOINING: i32 = 8;
const CHANGE_REMOVE_PLAYER: i32 = 9;
const CHANGE_MOST_PLAYERS: i32 = 10;

/// An action's number as scripts give it: the action in its low 16 bits, and
/// above them one more than the player whose reading of it they want, or 0
/// for the game's own, every device as one.
fn player_action(code: i32) -> (Option<usize>, voxear_input::ActionId) {
    let code = code.max(0) as u32;
    let player = (code >> 16) as usize;
    (player.checked_sub(1), voxear_input::ActionId((code & 0xffff) as usize))
}

/// Every player, a line each: 1 while their device is there, the kind of
/// device (`Keyboard` or `Gamepad`) and its name, between tabs; then a last
/// line, 1 while joining is on.
fn players(input: &Input) -> String {
    let mut lines: Vec<String> = (0..input.players())
        .map(|player| {
            let connected = i32::from(input.player_connected(player));
            match input.player_device(player) {
                Some(voxear_input::Device::Pad(id)) => {
                    let name = input.devices().pad(id).map_or("", |pad| pad.name.as_str());
                    format!("{connected}\tGamepad\t{name}")
                }
                _ => format!("{connected}\tKeyboard\tKeyboard and Mouse"),
            }
        })
        .collect();
    lines.push(i32::from(input.is_joining()).to_string());
    lines.join("\n")
}

fn with_input<T>(f: impl FnOnce(&mut Input) -> T) -> Option<T> {
    with_services(|services| services.input().map(f)).flatten()
}

/// The actions a control serves, as a key map labels it: the whole of what
/// its layout draws, as a stick with its halves and its press. A line each:
/// the action by its map and name, which of its bindings, the part of a
/// composite one (or empty), what a key map says for it, its category (or
/// empty), and 1 when it is hidden, between tabs.
pub(crate) unsafe extern "C" fn input_uses(path: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let path = unsafe { utf8(path, len) };
    let layout = match Scheme::of(&path) {
        Scheme::KeyboardMouse => voxear_input::layouts::keyboard_mouse(),
        Scheme::Gamepad => voxear_input::layouts::gamepad(),
    };
    let paths: Vec<String> = layout.control_of(&path).map_or_else(|| vec![path.to_string()], |c| c.paths.clone());
    let text = with_input(|input| {
        let uses = input.control_uses().into_iter().filter(|u| paths.contains(&u.path));
        let lines = uses.map(|u| {
            let category = u.category.clone().unwrap_or_default();
            let part = u.part.unwrap_or_default();
            format!("{}/{}\t{}\t{part}\t{}\t{category}\t{}", u.map, u.name, u.binding, u.label(), i32::from(u.hidden))
        });
        lines.collect::<Vec<_>>().join("\n")
    });
    let text = text.unwrap_or_default();
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

/// A control as prompts name it, by its path: a key as this keyboard types
/// it, a pad's button as its family names it. Written when it fits; its
/// length either way.
pub(crate) unsafe extern "C" fn input_label(path: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let path = unsafe { utf8(path, len) };
    let label = with_input(|input| input.control_label(&path)).unwrap_or_else(|| path.to_string());
    if !buffer.is_null() && label.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(label.as_ptr(), buffer, label.len()) };
    }
    label.len() as i32
}

/// The other actions of the same map bound to a control, by its path, as a
/// rebinding screen warns of: a name a line. Written when it fits; its
/// length either way.
pub(crate) unsafe extern "C" fn input_conflicts(
    action: i32,
    path: *const u8,
    len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let path = unsafe { utf8(path, len) };
    let action = voxear_input::ActionId(action.max(0) as usize);
    let text = with_input(|input| input.conflicts(action, &path).join("\n")).unwrap_or_default();
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

/// An action by its name, `Jump` or `Player/Jump`, or -1.
pub(crate) unsafe extern "C" fn input_action(name: *const u8, len: i32) -> i32 {
    let name = unsafe { utf8(name, len) };
    with_input(|input| input.action(&name).map_or(-1, |found| found.0 as i32)).unwrap_or(-1)
}

/// An action this frame, written to `out`; 0 when there is no input here.
pub(crate) unsafe extern "C" fn input_state(action: i32, out: *mut InputActionData) -> i32 {
    let (player, action) = player_action(action);
    let read = |input: &mut Input| match player {
        Some(player) => input.player_state(player, action),
        None => input.state(action),
    };
    let Some(state) = with_input(read) else {
        return 0;
    };
    let flags = i32::from(state.held)
        | i32::from(state.pressed) << 1
        | i32::from(state.released) << 2
        | i32::from(state.performed) << 3;
    if !out.is_null() {
        unsafe { *out = InputActionData { value: state.value, flags, held_for: state.held_for } };
    }
    1
}

/// A control by its path, as `Keyboard/W` or `Gamepad/LeftStick`: its value
/// written to `value`, and for a key or mouse button, bits as an action's
/// (1 down, 2 went down, 4 went up). -1 for a path that names nothing.
pub(crate) unsafe extern "C" fn input_control(path: *const u8, len: i32, value: *mut [f32; 2]) -> i32 {
    let path = unsafe { utf8(path, len) };
    let read = with_input(|input| {
        let devices = input.devices();
        let found = devices.read(&path)?;
        let (device, control) = path.split_once('/')?;
        let (down, pressed, released) = match device {
            "Keyboard" => devices.key(control),
            "Mouse" => devices.mouse_button(control),
            _ => (found[0] >= 0.5, false, false),
        };
        Some((found, i32::from(down) | i32::from(pressed) << 1 | i32::from(released) << 2))
    });
    let Some((found, flags)) = read.flatten() else { return -1 };
    if !value.is_null() {
        unsafe { *value = found };
    }
    flags
}

/// Text about input, written into `buffer` when it fits; returns its length
/// in bytes either way, or -1 for none: an action's prompt (0), the scheme
/// used last (1), the saved bindings (2), the maps' names a line each (3),
/// an action's bindings' controls a line each (4), the control the last
/// rebinding took (5), or an action's name with its map's (6).
pub(crate) unsafe extern "C" fn input_text(what: i32, action: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let action = voxear_input::ActionId(action.max(0) as usize);
    let text = with_input(|input| match what {
        TEXT_PROMPT => Some(input.prompt(action)),
        TEXT_SCHEME => Some(input.scheme().name().to_string()),
        TEXT_SAVED => Some(input.saved_bindings()),
        TEXT_MAPS => Some(input.maps().join("\n")),
        TEXT_BINDINGS => {
            let lines = input.bindings(action).iter().map(|binding| {
                let controls = binding.controls();
                let parts = controls.iter().map(|(part, path)| match part {
                    Some(part) => format!("{part}={path}"),
                    None => path.clone(),
                });
                parts.collect::<Vec<_>>().join(" ")
            });
            Some(lines.collect::<Vec<_>>().join("\n"))
        }
        TEXT_REBOUND => match input.peek_rebound() {
            Some(Rebound::To(path)) => Some(path.clone()),
            _ => None,
        },
        TEXT_NAME => input.action_name(action).map(|(map, name)| format!("{map}/{name}")),
        TEXT_ACTIONS => {
            let lines = input.actions().into_iter().map(|(_, map, name, category, hidden)| {
                format!("{map}/{name}\t{}\t{}", category.unwrap_or_default(), i32::from(hidden))
            });
            Some(lines.collect::<Vec<_>>().join("\n"))
        }
        TEXT_CATEGORIES => {
            let lines = input.categories().iter().map(|category| format!("{}\t{}", category.name, category.color));
            Some(lines.collect::<Vec<_>>().join("\n"))
        }
        TEXT_PLAYERS => Some(players(input)),
        TEXT_JOINED => Some(input.joined().iter().map(usize::to_string).collect::<Vec<_>>().join("\n")),
        _ => None,
    });
    let Some(text) = text.flatten() else { return -1 };
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

/// Changes input: a binding rebound to the control `text` names, `extra`
/// being the part of a composite (an index into its parts, or -1); listening
/// for the next control for a binding, `text` the part and `extra` the
/// scheme (-1 any, 0 keyboard and mouse, 1 gamepad); whether listening is
/// done (1), gave up (-1) or goes on (0); saved bindings read back from
/// `text`; every binding put back; a map, by name, turned on (`extra` 1) or
/// off; and listening given up. Below 0 when it could not be done.
pub(crate) unsafe extern "C" fn input_change(
    what: i32,
    action: i32,
    binding: i32,
    extra: i32,
    text: *const u8,
    len: i32,
) -> i32 {
    let text = unsafe { utf8(text, len) };
    let action = voxear_input::ActionId(action.max(0) as usize);
    let binding = binding.max(0) as usize;
    let done = with_input(|input| match what {
        CHANGE_REBIND => {
            let part = usize::try_from(extra).ok().and_then(|at| PARTS.get(at).copied());
            input.rebind(action, binding, part, &text).map_or(-1, |()| 0)
        }
        CHANGE_LISTEN => {
            let scheme = usize::try_from(extra).ok().and_then(|at| Scheme::ALL.get(at).copied());
            let part = Some(text.as_ref()).filter(|part| !part.is_empty());
            input.listen(action, binding, part, scheme);
            0
        }
        CHANGE_POLL => match (input.is_listening(), input.peek_rebound()) {
            (true, _) => 0,
            (false, Some(Rebound::To(_))) => 1,
            (false, _) => -1,
        },
        CHANGE_LOAD => input.load_bindings(&text).map_or(-1, |()| 0),
        CHANGE_RESET => input.reset_bindings().map_or(-1, |()| 0),
        CHANGE_MAP => {
            input.set_map_enabled(&text, extra != 0);
            0
        }
        CHANGE_CANCEL => {
            input.cancel_listening();
            0
        }
        CHANGE_RESET_ACTION => input.reset_action(action).map_or(-1, |()| 0),
        CHANGE_JOINING => {
            input.set_joining(extra != 0);
            0
        }
        CHANGE_REMOVE_PLAYER => {
            input.remove_player(binding);
            0
        }
        CHANGE_MOST_PLAYERS => {
            input.set_most_players(extra.max(1) as usize);
            0
        }
        _ => -1,
    });
    done.unwrap_or(-2)
}
