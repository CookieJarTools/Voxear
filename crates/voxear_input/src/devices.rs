//! The keyboard, the mouse and gamepads as they are this frame, each
//! control read by its path, as bindings name it:
//!
//! - `Keyboard/W`, `Keyboard/Space`, `Keyboard/ShiftLeft`, `Keyboard/1`: a
//!   key by where it is on the keyboard, as winit names it, without `Key`
//!   or `Digit` before a letter or a digit. `KeyQ` is `Keyboard/Q` on
//!   AZERTY too, where it types A; prompts say A.
//! - `Mouse/Left`, `Mouse/Right`, `Mouse/Middle`, `Mouse/Back`,
//!   `Mouse/Forward`; `Mouse/Delta`, how far it moved this frame, in
//!   pixels; `Mouse/Scroll`, in notches; and `Mouse/Position`.
//! - `Gamepad/South`, `East`, `West`, `North` (the face buttons, as they
//!   sit on any pad), `LeftShoulder`, `RightShoulder`, `LeftTrigger`,
//!   `RightTrigger` (0 to 1), `Select`, `Start`, `LeftStickPress`,
//!   `RightStickPress`, `DpadUp`, `DpadDown`, `DpadLeft`, `DpadRight`;
//!   `LeftStick` and `RightStick` as vectors, their axes as `LeftStickX`
//!   and so on, and their halves as `LeftStickUp` and so on, from 0 to 1.
//!
//! Gamepads come from gilrs, which reads them as SDL does. Any pad counts
//! for `Gamepad/...` until players are paired with pads of their own.

use std::collections::{HashMap, HashSet};

use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, PhysicalKey},
};

/// Which devices a binding is for: the keyboard and mouse together, or a
/// gamepad. The one used last is the one prompts show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scheme {
    #[default]
    KeyboardMouse,
    Gamepad,
}

impl Scheme {
    pub const ALL: [Self; 2] = [Self::KeyboardMouse, Self::Gamepad];

    /// The scheme a control's path is for.
    pub fn of(path: &str) -> Self {
        if path.starts_with("Gamepad/") { Self::Gamepad } else { Self::KeyboardMouse }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::KeyboardMouse => "Keyboard & Mouse",
            Self::Gamepad => "Gamepad",
        }
    }
}

/// The kind of gamepad, for the names its buttons have on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Family {
    #[default]
    Xbox,
    PlayStation,
    Switch,
    Steam,
}

impl Family {
    /// The family of a pad from its maker's USB vendor id.
    fn of_vendor(vendor: Option<u16>) -> Self {
        match vendor {
            Some(0x054c) => Self::PlayStation,
            Some(0x057e) => Self::Switch,
            Some(0x28de) => Self::Steam,
            _ => Self::Xbox,
        }
    }
}

/// A gamepad as it is this frame: each button's value from 0 to 1, and each
/// axis's from -1 to 1, by their names in paths.
#[derive(Clone, Debug, Default)]
pub struct Pad {
    /// Who the system says it is, the same for as long as it stays
    /// plugged in, as a player paired with it remembers it.
    pub id: u64,
    pub name: String,
    pub family: Family,
    pub buttons: HashMap<&'static str, f32>,
    pub axes: HashMap<&'static str, f32>,
}

/// A device a player plays with: the keyboard and mouse together, or a
/// gamepad by its [`Pad::id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    KeyboardMouse,
    Pad(u64),
}

/// The mouse this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mouse {
    /// Where it is over the window, in pixels from the top left.
    pub position: [f32; 2],
    /// How far it moved this frame, in pixels, right and down.
    pub delta: [f32; 2],
    /// Wheel notches this frame, right and up.
    pub scroll: [f32; 2],
}

/// The buttons a mouse has, by their names in paths.
const MOUSE_BUTTONS: [&str; 5] = ["Left", "Right", "Middle", "Back", "Forward"];

/// The devices as they are this frame, and as they were the frame before,
/// for what went down or up since.
#[derive(Default)]
pub struct Devices {
    keys: HashSet<String>,
    keys_before: HashSet<String>,
    /// The keys and buttons that went down, and up, since the frame before.
    keys_pressed: HashSet<String>,
    keys_released: HashSet<String>,
    buttons_pressed: HashSet<&'static str>,
    buttons_released: HashSet<&'static str>,
    /// What each key types on this keyboard, by its name, for prompts.
    layout: HashMap<String, String>,
    buttons: HashSet<&'static str>,
    buttons_before: HashSet<&'static str>,
    pub(crate) mouse: Mouse,
    /// Moves and wheel notches since the last frame.
    moved: [f32; 2],
    scrolled: [f32; 2],
    pub(crate) pads: Vec<Pad>,
    pads_before: Vec<HashMap<&'static str, f32>>,
    /// The scheme of the device used last.
    pub(crate) last: Scheme,
}

impl Devices {
    /// Takes a window's event: keys, mouse buttons, the pointer and the
    /// wheel. The window losing the focus lets go of every key and button.
    pub fn handle(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let name = key_name(&format!("{code:?}"));
                if let Key::Character(text) = &event.logical_key {
                    self.layout.insert(name.clone(), text.to_uppercase());
                }
                match event.state {
                    ElementState::Pressed => drop(self.keys.insert(name)),
                    ElementState::Released => drop(self.keys.remove(&name)),
                }
                self.last = Scheme::KeyboardMouse;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(name) = mouse_button(*button) else { return };
                match state {
                    ElementState::Pressed => drop(self.buttons.insert(name)),
                    ElementState::Released => drop(self.buttons.remove(name)),
                }
                self.last = Scheme::KeyboardMouse;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse.position = [position.x as f32, position.y as f32];
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let [x, y] = match delta {
                    MouseScrollDelta::LineDelta(x, y) => [*x, *y],
                    MouseScrollDelta::PixelDelta(at) => [at.x as f32 / 40.0, at.y as f32 / 40.0],
                };
                (self.scrolled[0], self.scrolled[1]) = (self.scrolled[0] + x, self.scrolled[1] + y);
                self.last = Scheme::KeyboardMouse;
            }
            WindowEvent::Focused(false) => self.release_all(),
            _ => {}
        }
    }

    /// How far the mouse moved, from the device rather than the pointer, so
    /// it keeps counting while the pointer is held in place.
    pub fn mouse_moved(&mut self, dx: f64, dy: f64) {
        (self.moved[0], self.moved[1]) = (self.moved[0] + dx as f32, self.moved[1] + dy as f32);
        if dx.abs() + dy.abs() > 2.0 {
            self.last = Scheme::KeyboardMouse;
        }
    }

    /// Lets go of every key and button, as when the window loses the focus.
    pub fn release_all(&mut self) {
        self.keys.clear();
        self.buttons.clear();
        for pad in &mut self.pads {
            pad.buttons.clear();
            pad.axes.clear();
        }
    }

    /// Starts a frame: what moved since the last becomes this frame's, and
    /// the keys and buttons that went down or up since are this frame's.
    pub(crate) fn begin_frame(&mut self) {
        self.mouse.delta = std::mem::take(&mut self.moved);
        self.mouse.scroll = std::mem::take(&mut self.scrolled);
        self.keys_pressed = self.keys.difference(&self.keys_before).cloned().collect();
        self.keys_released = self.keys_before.difference(&self.keys).cloned().collect();
        self.keys_before.clone_from(&self.keys);
        self.buttons_pressed = self.buttons.difference(&self.buttons_before).copied().collect();
        self.buttons_released = self.buttons_before.difference(&self.buttons).copied().collect();
        self.buttons_before.clone_from(&self.buttons);
    }

    /// Ends a frame: each pad's buttons now are what they were before the
    /// next, as pads change only when polled.
    pub(crate) fn end_frame(&mut self) {
        self.pads_before = self.pads.iter().map(|pad| pad.buttons.clone()).collect();
    }

    /// Whether a key is down, by its name, and whether it went down or up
    /// this frame.
    pub fn key(&self, name: &str) -> (bool, bool, bool) {
        (self.keys.contains(name), self.keys_pressed.contains(name), self.keys_released.contains(name))
    }

    /// The same for a mouse button.
    pub fn mouse_button(&self, name: &str) -> (bool, bool, bool) {
        let name: &str = name;
        (
            self.buttons.contains(name),
            self.buttons_pressed.iter().any(|b| *b == name),
            self.buttons_released.iter().any(|b| *b == name),
        )
    }

    pub fn mouse(&self) -> Mouse {
        self.mouse
    }

    pub fn pads(&self) -> &[Pad] {
        &self.pads
    }

    /// A control's value by its path: a vector for sticks and the mouse's
    /// moves, and a value in `x` for the rest. Nothing for a path that
    /// names no control.
    pub fn read(&self, path: &str) -> Option<[f32; 2]> {
        self.read_from(path, None)
    }

    /// A control's value as [`read`](Self::read) has it, from one device
    /// only when `only` says: the keyboard and mouse, or one pad, as a
    /// player paired with it reads it.
    pub fn read_from(&self, path: &str, only: Option<Device>) -> Option<[f32; 2]> {
        let (device, control) = path.split_once('/')?;
        let pads = |pad: &&Pad| only.is_none_or(|only| only == Device::Pad(pad.id));
        if matches!(device, "Keyboard" | "Mouse") && only.is_some_and(|only| only != Device::KeyboardMouse) {
            return Some([0.0; 2]);
        }
        let one = |down: bool| Some([if down { 1.0 } else { 0.0 }, 0.0]);
        match device {
            "Keyboard" => one(self.keys.contains(control)),
            "Mouse" => match control {
                "Delta" => Some(self.mouse.delta),
                "Scroll" => Some(self.mouse.scroll),
                "Position" => Some(self.mouse.position),
                _ => MOUSE_BUTTONS.contains(&control).then(|| one(self.buttons.contains(control))).flatten(),
            },
            "Gamepad" => Some(
                self.pads
                    .iter()
                    .filter(pads)
                    .map(|pad| pad_control(pad, control))
                    .fold([0.0; 2], |a, b| if b[0].hypot(b[1]) > a[0].hypot(a[1]) { b } else { a }),
            ),
            _ => None,
        }
    }

    /// A control that went down this frame, as rebinding listens for: a
    /// key, a mouse button, a pad's button, or a stick pushed far one way.
    /// Escape is not one: it gives up listening.
    pub(crate) fn newly_pressed(&self, scheme: Option<Scheme>) -> Option<String> {
        let wants = |found: Scheme| scheme.is_none_or(|scheme| scheme == found);
        if wants(Scheme::KeyboardMouse) {
            if let Some(key) = self.keys_pressed.iter().find(|key| *key != "Escape") {
                return Some(format!("Keyboard/{key}"));
            }
            if let Some(button) = self.buttons_pressed.iter().next() {
                return Some(format!("Mouse/{button}"));
            }
        }
        if !wants(Scheme::Gamepad) {
            return None;
        }
        for (at, pad) in self.pads.iter().enumerate() {
            let before = self.pads_before.get(at);
            let was = |name: &str| before.and_then(|b| b.get(name)).copied().unwrap_or(0.0);
            let pressed = pad.buttons.iter().find(|&(name, &value)| value >= 0.5 && was(name) < 0.5);
            if let Some((name, _)) = pressed {
                return Some(format!("Gamepad/{name}"));
            }
            for stick in ["LeftStick", "RightStick"] {
                for half in ["Up", "Down", "Left", "Right"] {
                    if pad_control(pad, &format!("{stick}{half}"))[0] > 0.7 {
                        return Some(format!("Gamepad/{stick}{half}"));
                    }
                }
            }
        }
        None
    }

    /// A device a key, mouse button or pad button went down on this frame,
    /// that `taken` does not hold, as a player joins with: the keyboard
    /// and mouse, or a pad.
    pub(crate) fn device_pressed(&self, taken: &[Device]) -> Option<Device> {
        let free = |device: &Device| !taken.contains(device);
        let typed = !self.keys_pressed.is_empty() || !self.buttons_pressed.is_empty();
        if typed && free(&Device::KeyboardMouse) {
            return Some(Device::KeyboardMouse);
        }
        self.pads.iter().enumerate().find_map(|(at, pad)| {
            let before = self.pads_before.get(at);
            let was = |name: &str| before.and_then(|b| b.get(name)).copied().unwrap_or(0.0);
            let pressed = pad.buttons.iter().any(|(name, &value)| value >= 0.5 && was(name) < 0.5);
            Some(Device::Pad(pad.id)).filter(|device| pressed && free(device))
        })
    }

    /// The pad of this id, while it is plugged in.
    pub fn pad(&self, id: u64) -> Option<&Pad> {
        self.pads.iter().find(|pad| pad.id == id)
    }

    /// Whether Escape went down this frame, which gives up a rebinding.
    pub(crate) fn escaped(&self) -> bool {
        self.key("Escape").1
    }

    /// What a key is called in prompts: what it types on this keyboard, if
    /// it has typed, or else its name, spaced.
    /// What a key types on this keyboard, as its presses have shown, by the
    /// key's name: `A` for `Q` on a French keyboard. None until it is known.
    pub fn typed(&self, name: &str) -> Option<&str> {
        self.layout.get(name).map(String::as_str).filter(|typed| typed.chars().any(|c| !c.is_whitespace()))
    }

    /// Learns what a key types, as a key event teaches it, for a host that
    /// knows the keyboard's layout another way, and for tests.
    pub fn set_typed(&mut self, name: &str, text: &str) {
        self.layout.insert(name.to_string(), text.to_uppercase());
    }

    pub fn key_label(&self, name: &str) -> String {
        if let Some(typed) = self.layout.get(name).filter(|typed| typed.chars().any(|c| !c.is_whitespace())) {
            return typed.clone();
        }
        spaced(name)
    }

    /// Takes what gilrs heard since the last frame, keeping each pad's
    /// buttons and axes.
    pub(crate) fn poll_pads(&mut self, gilrs: &mut gilrs::Gilrs) {
        while let Some(event) = gilrs.next_event() {
            if let gilrs::EventType::Connected | gilrs::EventType::Disconnected = event.event {
                self.relist_pads(gilrs);
            }
            let moved = matches!(
                event.event,
                gilrs::EventType::ButtonPressed(..) | gilrs::EventType::ButtonChanged(_, 0.3.., _)
            ) || matches!(event.event, gilrs::EventType::AxisChanged(_, value, _) if value.abs() > 0.5);
            if moved {
                self.last = Scheme::Gamepad;
            }
        }
        if self.pads.len() != gilrs.gamepads().count() {
            self.relist_pads(gilrs);
        }
        for ((_, gamepad), pad) in gilrs.gamepads().zip(&mut self.pads) {
            for (button, name) in PAD_BUTTONS {
                let value = gamepad.button_data(button).map_or(0.0, |data| data.value());
                pad.buttons.insert(name, value);
            }
            for (axis, name) in PAD_AXES {
                pad.axes.insert(name, gamepad.value(axis));
            }
        }
    }

    fn relist_pads(&mut self, gilrs: &gilrs::Gilrs) {
        self.pads = gilrs
            .gamepads()
            .map(|(id, gamepad)| Pad {
                id: usize::from(id) as u64,
                name: gamepad.name().to_string(),
                family: Family::of_vendor(gamepad.vendor_id()),
                ..Pad::default()
            })
            .collect();
    }

    /// The family of the first pad, for its buttons' names in prompts.
    pub fn family(&self) -> Family {
        self.pads.first().map(|pad| pad.family).unwrap_or_default()
    }

    /// Puts a pad here as it would be, for tests and tools without one.
    pub fn set_pad(&mut self, index: usize, pad: Pad) {
        if self.pads.len() <= index {
            self.pads.resize_with(index + 1, Pad::default);
        }
        self.pads[index] = pad;
        self.last = Scheme::Gamepad;
    }

    /// Where the pointer is over whatever shows the game, in its pixels from
    /// its top left, as the editor's Game view gives it.
    pub fn set_pointer(&mut self, x: f32, y: f32) {
        self.mouse.position = [x, y];
    }

    /// Presses or lets go of a key by its name, as a keyboard would.
    pub fn set_key(&mut self, name: &str, down: bool) {
        match down {
            true => drop(self.keys.insert(name.to_string())),
            false => drop(self.keys.remove(name)),
        }
        self.last = Scheme::KeyboardMouse;
    }
}

/// gilrs's buttons by their names in paths. Its `LeftTrigger` is the
/// shoulder button, and `LeftTrigger2` the trigger.
const PAD_BUTTONS: [(gilrs::Button, &str); 17] = [
    (gilrs::Button::South, "South"),
    (gilrs::Button::East, "East"),
    (gilrs::Button::West, "West"),
    (gilrs::Button::North, "North"),
    (gilrs::Button::LeftTrigger, "LeftShoulder"),
    (gilrs::Button::RightTrigger, "RightShoulder"),
    (gilrs::Button::LeftTrigger2, "LeftTrigger"),
    (gilrs::Button::RightTrigger2, "RightTrigger"),
    (gilrs::Button::Select, "Select"),
    (gilrs::Button::Start, "Start"),
    (gilrs::Button::Mode, "Mode"),
    (gilrs::Button::LeftThumb, "LeftStickPress"),
    (gilrs::Button::RightThumb, "RightStickPress"),
    (gilrs::Button::DPadUp, "DpadUp"),
    (gilrs::Button::DPadDown, "DpadDown"),
    (gilrs::Button::DPadLeft, "DpadLeft"),
    (gilrs::Button::DPadRight, "DpadRight"),
];

const PAD_AXES: [(gilrs::Axis, &str); 4] = [
    (gilrs::Axis::LeftStickX, "LeftStickX"),
    (gilrs::Axis::LeftStickY, "LeftStickY"),
    (gilrs::Axis::RightStickX, "RightStickX"),
    (gilrs::Axis::RightStickY, "RightStickY"),
];

/// A pad's control by its name in paths.
fn pad_control(pad: &Pad, control: &str) -> [f32; 2] {
    let axis = |name: &str| pad.axes.get(name).copied().unwrap_or(0.0);
    let stick = |side: &str| [axis(&format!("{side}StickX")), axis(&format!("{side}StickY"))];
    match control {
        "LeftStick" => stick("Left"),
        "RightStick" => stick("Right"),
        _ => {
            let half = ["Up", "Down", "Left", "Right"].iter().find_map(|half| {
                let side = control.strip_suffix(half)?.strip_suffix("Stick")?;
                let [x, y] = stick(side);
                Some(match *half {
                    "Up" => y.max(0.0),
                    "Down" => (-y).max(0.0),
                    "Left" => (-x).max(0.0),
                    _ => x.max(0.0),
                })
            });
            let value = half.or_else(|| pad.axes.get(control).copied()).or_else(|| pad.buttons.get(control).copied());
            [value.unwrap_or(0.0), 0.0]
        }
    }
}

/// A key's name in paths from winit's: `KeyW` is `W`, and `Digit1` is `1`.
fn key_name(winit: &str) -> String {
    let short = |prefix: &str| {
        winit.strip_prefix(prefix).filter(|rest| rest.len() == 1 && rest.chars().all(|c| c.is_ascii_alphanumeric()))
    };
    short("Key").or_else(|| short("Digit")).unwrap_or(winit).to_string()
}

/// A name with a space before each capital past the first, and a side
/// said first, as people write it: `ShiftLeft` is "Left Shift".
fn spaced(name: &str) -> String {
    let (name, side) = match ["Left", "Right"].iter().find_map(|side| name.strip_suffix(side).map(|n| (n, *side))) {
        Some((rest, side)) if !rest.is_empty() && !rest.starts_with("Arrow") => (rest, Some(side)),
        _ => (name, None),
    };
    let mut out = side.map(|side| format!("{side} ")).unwrap_or_default();
    for (at, c) in name.chars().enumerate() {
        if at > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out.replace("Arrow ", "")
}

fn mouse_button(button: MouseButton) -> Option<&'static str> {
    Some(match button {
        MouseButton::Left => "Left",
        MouseButton::Right => "Right",
        MouseButton::Middle => "Middle",
        MouseButton::Back => "Back",
        MouseButton::Forward => "Forward",
        MouseButton::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_named_as_paths_say_and_prompts_spell_them() {
        assert_eq!(key_name("KeyW"), "W");
        assert_eq!(key_name("Digit7"), "7");
        assert_eq!(key_name("Space"), "Space");
        assert_eq!(key_name("ShiftLeft"), "ShiftLeft");
        assert_eq!(spaced("ShiftLeft"), "Left Shift");
        assert_eq!(spaced("ArrowUp"), "Up");
        assert_eq!(spaced("PageDown"), "Page Down");
    }

    #[test]
    fn a_stick_reads_whole_or_by_halves() {
        let mut devices = Devices::default();
        let mut pad = Pad::default();
        pad.axes.insert("LeftStickX", -0.8);
        pad.axes.insert("LeftStickY", 0.25);
        pad.buttons.insert("South", 1.0);
        devices.set_pad(0, pad);
        assert_eq!(devices.read("Gamepad/LeftStick"), Some([-0.8, 0.25]));
        assert_eq!(devices.read("Gamepad/LeftStickLeft"), Some([0.8, 0.0]));
        assert_eq!(devices.read("Gamepad/LeftStickRight"), Some([0.0, 0.0]));
        assert_eq!(devices.read("Gamepad/South"), Some([1.0, 0.0]));
        assert_eq!(devices.read("Nothing/Here"), None);
    }
}
