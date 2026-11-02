//! The keyboard, the mouse and a gamepad drawn as layouts, for key maps
//! that show which actions each control is bound to (see
//! [`Input::control_uses`](crate::Input::control_uses)): each control's
//! place and size, in key widths, its shape, and the paths it stands for,
//! as a stick stands for its axes and halves and its press.
//!
//! The keyboard is ANSI, with its function row, the keys beside it and the
//! arrows, and a mouse to its right. The gamepad sits as every pad does:
//! triggers and shoulders on top, the left stick and the d-pad on the
//! left, the face buttons and the right stick on the right.

/// How a control is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlShape {
    /// A key, or a mouse button: a rounded square.
    Key,
    /// A face button, or the pad's home button: a circle.
    Round,
    /// A stick: a circle with a ring.
    Stick,
    /// A trigger, a shoulder, Select or Start: a pill.
    Pill,
}

/// A control as a layout draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutControl {
    /// The paths it stands for, the first its own, as `Keyboard/W`.
    pub paths: Vec<String>,
    /// Where it is and how big, in key widths: x, y, width, height.
    pub rect: [f32; 4],
    pub shape: ControlShape,
    /// What its face says, as on a US keyboard or an Xbox pad; key maps
    /// may say what this keyboard or pad says instead (see
    /// [`Input::control_label`](crate::Input::control_label)).
    pub label: String,
}

/// A device's controls, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    /// How big it is, in key widths.
    pub size: [f32; 2],
    pub controls: Vec<LayoutControl>,
}

impl Layout {
    /// The control that stands for a path.
    pub fn control_of(&self, path: &str) -> Option<&LayoutControl> {
        self.controls.iter().find(|control| control.paths.iter().any(|p| p == path))
    }
}

fn key(path: &str, label: &str, rect: [f32; 4]) -> LayoutControl {
    LayoutControl { paths: vec![format!("Keyboard/{path}")], rect, shape: ControlShape::Key, label: label.to_string() }
}

/// A row of keys from `x`, each a name, a label and a width.
fn row(controls: &mut Vec<LayoutControl>, x: f32, y: f32, keys: &[(&str, &str, f32)]) {
    let mut at = x;
    for &(path, label, width) in keys {
        controls.push(key(path, label, [at, y, width, 1.0]));
        at += width;
    }
}

/// The keyboard and the mouse: what the keyboard and mouse scheme reads.
pub fn keyboard_mouse() -> Layout {
    let mut c = Vec::new();
    // The function row, in groups of four.
    c.push(key("Escape", "Esc", [0.0, 0.0, 1.0, 1.0]));
    for (group, x) in [(0, 2.0), (1, 6.5), (2, 11.0)] {
        for i in 0..4 {
            let n = group * 4 + i + 1;
            c.push(key(&format!("F{n}"), &format!("F{n}"), [x + i as f32, 0.0, 1.0, 1.0]));
        }
    }
    let one = |name: &'static str| (name, name, 1.0);
    let digits: Vec<(&str, &str, f32)> = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].map(one).to_vec();
    let mut numbers = vec![("Backquote", "`", 1.0)];
    numbers.extend(digits);
    numbers.extend([("Minus", "-", 1.0), ("Equal", "=", 1.0), ("Backspace", "Backspace", 2.0)]);
    row(&mut c, 0.0, 1.25, &numbers);
    let mut top = vec![("Tab", "Tab", 1.5)];
    top.extend(["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"].map(one));
    top.extend([("BracketLeft", "[", 1.0), ("BracketRight", "]", 1.0), ("Backslash", "\\", 1.5)]);
    row(&mut c, 0.0, 2.25, &top);
    let mut home = vec![("CapsLock", "Caps", 1.75)];
    home.extend(["A", "S", "D", "F", "G", "H", "J", "K", "L"].map(one));
    home.extend([("Semicolon", ";", 1.0), ("Quote", "'", 1.0), ("Enter", "Enter", 2.25)]);
    row(&mut c, 0.0, 3.25, &home);
    let mut bottom = vec![("ShiftLeft", "Shift", 2.25)];
    bottom.extend(["Z", "X", "C", "V", "B", "N", "M"].map(one));
    bottom.extend([("Comma", ",", 1.0), ("Period", ".", 1.0), ("Slash", "/", 1.0), ("ShiftRight", "Shift", 2.75)]);
    row(&mut c, 0.0, 4.25, &bottom);
    row(
        &mut c,
        0.0,
        5.25,
        &[
            ("ControlLeft", "Ctrl", 1.25),
            ("SuperLeft", "Super", 1.25),
            ("AltLeft", "Alt", 1.25),
            ("Space", "Space", 6.25),
            ("AltRight", "Alt", 1.25),
            ("SuperRight", "Super", 1.25),
            ("ContextMenu", "Menu", 1.25),
            ("ControlRight", "Ctrl", 1.25),
        ],
    );
    // The keys beside it, and the arrows under them.
    row(&mut c, 15.25, 1.25, &[("Insert", "Ins", 1.0), ("Home", "Home", 1.0), ("PageUp", "PgUp", 1.0)]);
    row(&mut c, 15.25, 2.25, &[("Delete", "Del", 1.0), ("End", "End", 1.0), ("PageDown", "PgDn", 1.0)]);
    c.push(key("ArrowUp", "↑", [16.25, 4.25, 1.0, 1.0]));
    row(&mut c, 15.25, 5.25, &[("ArrowLeft", "←", 1.0), ("ArrowDown", "↓", 1.0), ("ArrowRight", "→", 1.0)]);
    // The mouse: its buttons, its wheel between them, its body for moving it, and its side buttons.
    let mouse = |paths: &[&str], label: &str, rect: [f32; 4], shape: ControlShape| LayoutControl {
        paths: paths.iter().map(|p| format!("Mouse/{p}")).collect(),
        rect,
        shape,
        label: label.to_string(),
    };
    c.push(mouse(&["Left"], "Left", [19.0, 1.25, 1.2, 2.0], ControlShape::Key));
    c.push(mouse(&["Middle", "Scroll"], "Wheel", [20.3, 1.5, 0.6, 1.5], ControlShape::Pill));
    c.push(mouse(&["Right"], "Right", [21.0, 1.25, 1.2, 2.0], ControlShape::Key));
    c.push(mouse(&["Delta", "Position"], "Mouse", [19.0, 3.4, 3.2, 1.75], ControlShape::Key));
    c.push(mouse(&["Back"], "Back", [19.0, 5.25, 1.55, 1.0], ControlShape::Key));
    c.push(mouse(&["Forward"], "Fwd", [20.65, 5.25, 1.55, 1.0], ControlShape::Key));
    Layout { name: "Keyboard and Mouse".into(), size: [22.2, 6.25], controls: c }
}

/// A gamepad, its buttons named as on an Xbox pad.
pub fn gamepad() -> Layout {
    let pad = |paths: &[&str], label: &str, rect: [f32; 4], shape: ControlShape| LayoutControl {
        paths: paths.iter().map(|p| format!("Gamepad/{p}")).collect(),
        rect,
        shape,
        label: label.to_string(),
    };
    let stick = |side: &str| -> Vec<String> {
        ["", "X", "Y", "Up", "Down", "Left", "Right", "Press"].iter().map(|part| format!("{side}Stick{part}")).collect()
    };
    let left: Vec<String> = stick("Left");
    let right: Vec<String> = stick("Right");
    let left: Vec<&str> = left.iter().map(String::as_str).collect();
    let right: Vec<&str> = right.iter().map(String::as_str).collect();
    let c = vec![
        pad(&["LeftTrigger"], "LT", [1.5, 0.0, 3.0, 0.8], ControlShape::Pill),
        pad(&["RightTrigger"], "RT", [11.5, 0.0, 3.0, 0.8], ControlShape::Pill),
        pad(&["LeftShoulder"], "LB", [1.5, 1.0, 3.0, 0.7], ControlShape::Pill),
        pad(&["RightShoulder"], "RB", [11.5, 1.0, 3.0, 0.7], ControlShape::Pill),
        pad(&left, "LS", [2.0, 2.3, 2.0, 2.0], ControlShape::Stick),
        pad(&["DpadUp"], "↑", [4.6, 4.4, 0.9, 0.9], ControlShape::Key),
        pad(&["DpadLeft"], "←", [3.7, 5.3, 0.9, 0.9], ControlShape::Key),
        pad(&["DpadRight"], "→", [5.5, 5.3, 0.9, 0.9], ControlShape::Key),
        pad(&["DpadDown"], "↓", [4.6, 6.2, 0.9, 0.9], ControlShape::Key),
        pad(&["Select"], "View", [6.3, 2.9, 1.2, 0.6], ControlShape::Pill),
        pad(&["Mode"], "Home", [7.55, 3.8, 0.9, 0.9], ControlShape::Round),
        pad(&["Start"], "Menu", [8.5, 2.9, 1.2, 0.6], ControlShape::Pill),
        pad(&["North"], "Y", [12.0, 1.9, 1.0, 1.0], ControlShape::Round),
        pad(&["West"], "X", [11.0, 2.9, 1.0, 1.0], ControlShape::Round),
        pad(&["East"], "B", [13.0, 2.9, 1.0, 1.0], ControlShape::Round),
        pad(&["South"], "A", [12.0, 3.9, 1.0, 1.0], ControlShape::Round),
        pad(&right, "RS", [9.5, 4.7, 2.0, 2.0], ControlShape::Stick),
    ];
    Layout { name: "Gamepad".into(), size: [16.0, 7.1], controls: c }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_has_a_place_inside_its_layout_and_none_overlap() {
        for layout in [keyboard_mouse(), gamepad()] {
            for (at, a) in layout.controls.iter().enumerate() {
                let [x, y, w, h] = a.rect;
                assert!(
                    x >= 0.0 && y >= 0.0 && x + w <= layout.size[0] + 1e-4 && y + h <= layout.size[1] + 1e-4,
                    "{a:?}"
                );
                for b in &layout.controls[at + 1..] {
                    let [bx, by, bw, bh] = b.rect;
                    let apart = x + w <= bx + 1e-4 || bx + bw <= x + 1e-4 || y + h <= by + 1e-4 || by + bh <= y + 1e-4;
                    assert!(apart, "{} and {} overlap", a.label, b.label);
                }
            }
        }
        let keyboard = keyboard_mouse();
        assert_eq!(keyboard.control_of("Keyboard/Space").map(|c| c.rect[2]), Some(6.25));
        assert_eq!(keyboard.controls.iter().filter(|c| c.paths[0].starts_with("Keyboard/")).count(), 84);
        assert_eq!(gamepad().control_of("Gamepad/LeftStickUp").map(|c| c.label.as_str()), Some("LS"));
    }
}
