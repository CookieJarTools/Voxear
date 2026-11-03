//! The keyboard, the mouse and a gamepad drawn as layouts, for key maps
//! that show which actions each control is bound to (see
//! [`Input::control_uses`](crate::Input::control_uses)): each control's
//! place and size, in key widths, its shape, and the paths it stands for,
//! as a stick stands for its axes and halves and its press.
//!
//! The keyboard is ANSI, with its function row, the keys beside it and the
//! arrows, and a mouse to its right, its side buttons on its left. The gamepad sits as every pad does:
//! triggers and shoulders on top, the left stick and the d-pad on the
//! left, the face buttons and the right stick on the right.
//!
//! Behind its controls each layout has an outline of the device itself, as
//! rounded shapes that together make its body: the keyboard's case and the
//! mouse, or a pad's body and its grips.

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

/// A part of a device's body, drawn behind its controls: a rectangle in
/// key widths, with corners rounded `radius` key widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPart {
    pub rect: [f32; 4],
    pub radius: f32,
}

const fn part(rect: [f32; 4], radius: f32) -> BodyPart {
    BodyPart { rect, radius }
}

/// A device's controls, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    /// How big it is, in key widths.
    pub size: [f32; 2],
    pub controls: Vec<LayoutControl>,
    /// Its body, the parts overlapping into one shape.
    pub body: Vec<BodyPart>,
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
    // The mouse: its buttons, its wheel between them, its body for moving it, and its side buttons on its left,
    // under the thumb, Forward the nearer its front, named Mouse 4 and 5 as players call them.
    let mouse = |paths: &[&str], label: &str, rect: [f32; 4], shape: ControlShape| LayoutControl {
        paths: paths.iter().map(|p| format!("Mouse/{p}")).collect(),
        rect,
        shape,
        label: label.to_string(),
    };
    c.push(mouse(&["Left"], "Left", [19.8, 1.25, 1.2, 2.0], ControlShape::Key));
    c.push(mouse(&["Middle", "Scroll"], "↕", [21.1, 1.5, 0.6, 1.5], ControlShape::Pill));
    c.push(mouse(&["Right"], "Right", [21.8, 1.25, 1.2, 2.0], ControlShape::Key));
    c.push(mouse(&["Delta", "Position"], "Mouse", [19.8, 3.4, 3.2, 1.75], ControlShape::Key));
    c.push(mouse(&["Forward"], "M5", [18.95, 2.4, 0.75, 1.0], ControlShape::Key));
    c.push(mouse(&["Back"], "M4", [18.95, 3.55, 0.75, 1.0], ControlShape::Key));
    // The keyboard's case, and the mouse round its buttons, its side buttons standing out of its left.
    let body = vec![part([-0.2, -0.2, 18.45, 6.65], 0.3), part([19.45, 0.95, 3.75, 5.0], 1.6)];
    Layout { name: "Keyboard and Mouse".into(), size: [23.2, 6.25], controls: c, body }
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
    // The body across the top, the grips down either side, and between
    // them the lower middle the d-pad and the right stick sit on.
    let body = vec![
        part([0.7, 1.35, 14.6, 3.9], 1.8),
        part([3.1, 3.6, 9.8, 3.5], 1.5),
        part([0.2, 2.6, 4.2, 4.5], 2.0),
        part([11.6, 2.6, 4.2, 4.5], 2.0),
    ];
    Layout { name: "Gamepad".into(), size: [16.0, 7.1], controls: c, body }
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

    #[test]
    fn a_pads_controls_sit_on_its_body() {
        let pad = gamepad();
        let on = |[x, y]: [f32; 2]| {
            pad.body.iter().any(|p| {
                let [px, py, pw, ph] = p.rect;
                x >= px && x <= px + pw && y >= py && y <= py + ph
            })
        };
        // The triggers stand above the body, as a pad's do.
        for control in pad.controls.iter().filter(|c| !c.label.ends_with('T')) {
            let [x, y, w, h] = control.rect;
            assert!(on([x + w / 2.0, y + h / 2.0]), "{} is off the pad", control.label);
        }
    }
}
