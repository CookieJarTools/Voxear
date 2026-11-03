//! Every control a binding can read, by device and by group, with the name
//! people call it: what the Input Actions window's control picker offers,
//! as Unity's binding path picker does, so a binding is chosen from a list
//! or heard with Listen rather than typed.

/// One control a binding can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    /// Its path, as bindings save it: `Keyboard/W`.
    pub path: String,
    /// What people call it: "W", "Left Shift", "Left Stick Up".
    pub name: String,
    /// Where the picker lists it: its device, then its group, as
    /// `Keyboard/Letters`.
    pub group: &'static str,
}

const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const EDITING: [&str; 15] = [
    "Space",
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
];
const MODIFIERS: [&str; 10] = [
    "ShiftLeft",
    "ShiftRight",
    "ControlLeft",
    "ControlRight",
    "AltLeft",
    "AltRight",
    "SuperLeft",
    "SuperRight",
    "CapsLock",
    "ContextMenu",
];
const PUNCTUATION: [(&str, &str); 11] = [
    ("Backquote", "`"),
    ("Minus", "-"),
    ("Equal", "="),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Backslash", "\\"),
    ("Semicolon", ";"),
    ("Quote", "'"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
];
const NUMPAD: [&str; 6] = ["Add", "Subtract", "Multiply", "Divide", "Decimal", "Enter"];
const MOUSE: [(&str, &str); 8] = [
    ("Left", "Left Button"),
    ("Right", "Right Button"),
    ("Middle", "Middle Button"),
    ("Back", "Back Button"),
    ("Forward", "Forward Button"),
    ("Delta", "Movement"),
    ("Scroll", "Scroll Wheel"),
    ("Position", "Position"),
];
const PAD_BUTTONS: [(&str, &str); 13] = [
    ("South", "South Button (A)"),
    ("East", "East Button (B)"),
    ("West", "West Button (X)"),
    ("North", "North Button (Y)"),
    ("LeftShoulder", "Left Shoulder"),
    ("RightShoulder", "Right Shoulder"),
    ("LeftTrigger", "Left Trigger"),
    ("RightTrigger", "Right Trigger"),
    ("Select", "View"),
    ("Start", "Menu"),
    ("Mode", "Home"),
    ("LeftStickPress", "Left Stick Press"),
    ("RightStickPress", "Right Stick Press"),
];
const DPAD: [&str; 4] = ["Up", "Down", "Left", "Right"];
const STICK: [(&str, &str); 7] =
    [("", ""), ("X", " X"), ("Y", " Y"), ("Up", " Up"), ("Down", " Down"), ("Left", " Left"), ("Right", " Right")];

/// Every control, the keyboard's first, then the mouse's and a gamepad's,
/// each device's in its groups.
pub fn all() -> Vec<Control> {
    let mut found = Vec::new();
    let mut add = |device: &str, name: &str, shown: &str, group: &'static str| {
        found.push(Control { path: format!("{device}/{name}"), name: shown.to_string(), group });
    };
    for letter in LETTERS.chars().map(String::from) {
        add("Keyboard", &letter, &letter, "Keyboard/Letters");
    }
    for digit in (0..10).map(|d| d.to_string()) {
        add("Keyboard", &digit, &digit, "Keyboard/Numbers");
    }
    for n in 1..=12 {
        add("Keyboard", &format!("F{n}"), &format!("F{n}"), "Keyboard/Function Keys");
    }
    for key in EDITING {
        add("Keyboard", key, &spaced(key), "Keyboard/Editing and Arrows");
    }
    for key in MODIFIERS {
        add("Keyboard", key, &spaced(key), "Keyboard/Modifiers");
    }
    for (key, shown) in PUNCTUATION {
        add("Keyboard", key, &format!("{shown}  {}", spaced(key)), "Keyboard/Punctuation");
    }
    for n in 0..10 {
        add("Keyboard", &format!("Numpad{n}"), &format!("Numpad {n}"), "Keyboard/Numpad");
    }
    for key in NUMPAD {
        add("Keyboard", &format!("Numpad{key}"), &format!("Numpad {key}"), "Keyboard/Numpad");
    }
    for (name, shown) in MOUSE {
        add("Mouse", name, shown, "Mouse");
    }
    for (name, shown) in PAD_BUTTONS {
        add("Gamepad", name, shown, "Gamepad/Buttons");
    }
    for way in DPAD {
        add("Gamepad", &format!("Dpad{way}"), &format!("D-pad {way}"), "Gamepad/D-pad");
    }
    for (side, group) in [("Left", "Gamepad/Left Stick"), ("Right", "Gamepad/Right Stick")] {
        for (part, shown) in STICK {
            add("Gamepad", &format!("{side}Stick{part}"), &format!("{side} Stick{shown}"), group);
        }
    }
    found
}

/// What people call a control, by its path, with its device: "W Key",
/// "Mouse Left Button", "Gamepad Left Stick Up"; the path itself for one
/// Voxear does not know.
pub fn describe(path: &str) -> String {
    let Some(found) = all().into_iter().find(|control| control.path == path) else { return path.to_string() };
    match path.split_once('/').map(|(device, _)| device) {
        Some("Keyboard") => format!("{} Key", found.name),
        Some(device) => format!("{device} {}", found.name),
        None => found.name,
    }
}

/// Whether a path names a control Voxear knows.
pub fn is_known(path: &str) -> bool {
    all().iter().any(|control| control.path == path)
}

/// A name with a space before each capital past the first, and a side
/// said first: `ShiftLeft` is "Left Shift", `ArrowUp` is "Up Arrow".
fn spaced(name: &str) -> String {
    if let Some(way) = name.strip_prefix("Arrow") {
        return format!("{way} Arrow");
    }
    let (name, side) = match ["Left", "Right"].iter().find_map(|side| name.strip_suffix(side).map(|n| (n, *side))) {
        Some((rest, side)) if !rest.is_empty() => (rest, Some(side)),
        _ => (name, None),
    };
    let mut out = side.map(|side| format!("{side} ")).unwrap_or_default();
    for (at, c) in name.chars().enumerate() {
        if at > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_is_listed_once_in_its_devices_group() {
        let every = all();
        for (at, control) in every.iter().enumerate() {
            assert!(!every[at + 1..].iter().any(|other| other.path == control.path), "{} twice", control.path);
            let device = control.path.split('/').next().unwrap();
            assert!(control.group.starts_with(device), "{} in {}", control.path, control.group);
        }
        assert_eq!(describe("Keyboard/ShiftLeft"), "Left Shift Key");
        assert_eq!(describe("Gamepad/LeftStickUp"), "Gamepad Left Stick Up");
        assert_eq!(describe("Mouse/Delta"), "Mouse Movement");
        assert!(is_known("Keyboard/7") && is_known("Gamepad/DpadLeft") && !is_known("Keyboard/Wobble"));
    }

    #[test]
    fn every_control_a_layout_draws_is_listed() {
        for layout in [crate::layouts::keyboard_mouse(), crate::layouts::gamepad()] {
            for path in layout.controls.iter().flat_map(|control| &control.paths) {
                assert!(is_known(path), "{path} is drawn but not listed");
            }
        }
    }
}
