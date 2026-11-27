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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlShape {
    /// A key, or a mouse button: a rounded square.
    Key,
    /// A face button, or the pad's home button: a circle.
    Round,
    /// A stick: a circle with a dished cap.
    Stick,
    /// A wheel: a pill.
    Pill,
    /// SVG path data in a `view` box stretched over the control, as a
    /// trigger's curve or a mouse button's.
    Path { path: &'static str, view: [f32; 2] },
}

/// The side of its device a control's label goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelSide {
    Left,
    Right,
    Top,
    Bottom,
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
    /// An icon on its face in place of its label (see `voxear_ui::ICONS`).
    pub icon: Option<&'static str>,
    /// A name written under it, as the mouse's Move.
    pub caption: Option<&'static str>,
    /// Which of the layout's devices it is on.
    pub device: usize,
    /// The side its label goes on; `None` for the nearest.
    pub side: Option<LabelSide>,
}

/// The shape of a part of a device's body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartShape {
    /// A box with corners rounded `radius` key widths.
    Rect { radius: f32 },
    /// A circle as wide as its box.
    Circle,
    /// SVG path data in a `view` box stretched over the part's box.
    Path { path: &'static str, view: [f32; 2] },
}

/// What a part of a device's body is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartLook {
    /// The device itself: the keyboard's plate, the mouse's shell, the pad's body.
    Plate,
    /// A well sunk in it, behind a stick, the d-pad, the face buttons or the home button.
    Recess,
    /// A part that is not a control, as the middle of the d-pad.
    Part,
}

/// A part of a device's body, drawn behind its controls: its box in key
/// widths, its shape and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPart {
    pub rect: [f32; 4],
    pub shape: PartShape,
    pub look: PartLook,
    /// Which of the layout's devices it belongs to.
    pub device: usize,
}

/// One device of a layout, as the keyboard or the mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Device {
    /// Its bounds, in key widths.
    pub rect: [f32; 4],
    /// Its keys stand in rows, as a keyboard's.
    pub rows: bool,
}

/// A device's controls, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    /// How big it is, in key widths.
    pub size: [f32; 2],
    pub controls: Vec<LayoutControl>,
    /// Its body, drawn in order, the plates first.
    pub body: Vec<BodyPart>,
    /// Its devices, left to right.
    pub devices: Vec<Device>,
}

impl Layout {
    /// The control that stands for a path.
    pub fn control_of(&self, path: &str) -> Option<&LayoutControl> {
        self.controls.iter().find(|control| control.paths.iter().any(|p| p == path))
    }
}

fn control(paths: Vec<String>, label: &str, rect: [f32; 4], shape: ControlShape) -> LayoutControl {
    LayoutControl { paths, rect, shape, label: label.to_string(), icon: None, caption: None, device: 0, side: None }
}

fn key(path: &str, label: &str, rect: [f32; 4]) -> LayoutControl {
    control(vec![format!("Keyboard/{path}")], label, rect, ControlShape::Key)
}

/// A row of keys from `x`, each a name, a label and a width.
fn row(controls: &mut Vec<LayoutControl>, x: f32, y: f32, keys: &[(&str, &str, f32)]) {
    let mut at = x;
    for &(path, label, width) in keys {
        controls.push(key(path, label, [at, y, width, 1.0]));
        at += width;
    }
}

/// Where the keyboard's rows are: the function row, then the rest with a
/// gap under each wide enough for three lines to run along.
const ROWS: [f32; 6] = [0.0, 1.45, 3.05, 4.65, 6.25, 7.85];

/// The mouse is drawn in pixels of a shell 112 by 184, against a key 26 wide.
const MOUSE_PX: f32 = 26.0;
const MOUSE_AT: [f32; 2] = [20.0, 0.9];

const MOUSE_SHELL: &str =
    "M56 0C92 0 112 28 112 72L112 124C112 160 90 184 56 184C22 184 0 160 0 124L0 72C0 28 20 0 56 0Z";
const MOUSE_LEFT: &str = "M48 0C20 2 0 26 0 64L0 86L48 86Z";
const MOUSE_RIGHT: &str = "M0 0C28 2 48 26 48 64L48 86L0 86Z";

/// The keyboard and the mouse: what the keyboard and mouse scheme reads.
pub fn keyboard_mouse() -> Layout {
    let mut c = Vec::new();
    // The function row, in groups of four.
    c.push(key("Escape", "Esc", [0.0, ROWS[0], 1.0, 1.0]));
    for (group, x) in [(0, 2.0), (1, 6.5), (2, 11.0)] {
        for i in 0..4 {
            let n = group * 4 + i + 1;
            c.push(key(&format!("F{n}"), &format!("F{n}"), [x + i as f32, ROWS[0], 1.0, 1.0]));
        }
    }
    let one = |name: &'static str| (name, name, 1.0);
    let digits: Vec<(&str, &str, f32)> = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].map(one).to_vec();
    let mut numbers = vec![("Backquote", "`", 1.0)];
    numbers.extend(digits);
    numbers.extend([("Minus", "-", 1.0), ("Equal", "=", 1.0), ("Backspace", "Backspace", 2.0)]);
    row(&mut c, 0.0, ROWS[1], &numbers);
    let mut top = vec![("Tab", "Tab", 1.5)];
    top.extend(["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"].map(one));
    top.extend([("BracketLeft", "[", 1.0), ("BracketRight", "]", 1.0), ("Backslash", "\\", 1.5)]);
    row(&mut c, 0.0, ROWS[2], &top);
    let mut home = vec![("CapsLock", "Caps", 1.75)];
    home.extend(["A", "S", "D", "F", "G", "H", "J", "K", "L"].map(one));
    home.extend([("Semicolon", ";", 1.0), ("Quote", "'", 1.0), ("Enter", "Enter", 2.25)]);
    row(&mut c, 0.0, ROWS[3], &home);
    let mut bottom = vec![("ShiftLeft", "Shift", 2.25)];
    bottom.extend(["Z", "X", "C", "V", "B", "N", "M"].map(one));
    bottom.extend([("Comma", ",", 1.0), ("Period", ".", 1.0), ("Slash", "/", 1.0), ("ShiftRight", "Shift", 2.75)]);
    row(&mut c, 0.0, ROWS[4], &bottom);
    row(
        &mut c,
        0.0,
        ROWS[5],
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
    row(&mut c, 15.25, ROWS[1], &[("Insert", "Ins", 1.0), ("Home", "Home", 1.0), ("PageUp", "PgUp", 1.0)]);
    row(&mut c, 15.25, ROWS[2], &[("Delete", "Del", 1.0), ("End", "End", 1.0), ("PageDown", "PgDn", 1.0)]);
    c.push(key("ArrowUp", "↑", [16.25, ROWS[4], 1.0, 1.0]));
    row(&mut c, 15.25, ROWS[5], &[("ArrowLeft", "←", 1.0), ("ArrowDown", "↓", 1.0), ("ArrowRight", "→", 1.0)]);
    let keyboard = [-0.4, -0.4, 19.05, ROWS[5] + 1.8];

    // The mouse, seen from above: its buttons either side of the wheel, the palm that moves it, and its side
    // buttons on its left, under the thumb, Forward the nearer its front, named Mouse 4 and 5 as players call them.
    let at =
        |[x, y, w, h]: [f32; 4]| [MOUSE_AT[0] + x / MOUSE_PX, MOUSE_AT[1] + y / MOUSE_PX, w / MOUSE_PX, h / MOUSE_PX];
    let mouse = |paths: &[&str], label: &str, rect: [f32; 4], shape: ControlShape, side: LabelSide| LayoutControl {
        device: 1,
        side: Some(side),
        ..control(paths.iter().map(|p| format!("Mouse/{p}")).collect(), label, at(rect), shape)
    };
    let button = |path| ControlShape::Path { path, view: [48.0, 86.0] };
    c.push(mouse(&["Left"], "Left", [2.0, 2.0, 48.0, 86.0], button(MOUSE_LEFT), LabelSide::Top));
    c.push(mouse(&["Middle", "Scroll"], "↕", [51.0, 18.0, 10.0, 36.0], ControlShape::Pill, LabelSide::Right));
    c.push(mouse(&["Right"], "Right", [62.0, 2.0, 48.0, 86.0], button(MOUSE_RIGHT), LabelSide::Right));
    c.push(LayoutControl {
        icon: Some("move"),
        caption: Some("Move"),
        ..mouse(&["Delta", "Position"], "Move", [42.0, 116.0, 28.0, 28.0], ControlShape::Round, LabelSide::Right)
    });
    c.push(mouse(&["Forward"], "M5", [-14.0, 92.0, 16.0, 22.0], ControlShape::Key, LabelSide::Left));
    c.push(mouse(&["Back"], "M4", [-14.0, 118.0, 16.0, 22.0], ControlShape::Key, LabelSide::Left));
    let mouse_rect = at([-14.0, 0.0, 126.0, 184.0]);

    let body = vec![
        BodyPart { rect: keyboard, shape: PartShape::Rect { radius: 0.38 }, look: PartLook::Plate, device: 0 },
        BodyPart {
            rect: at([0.0, 0.0, 112.0, 184.0]),
            shape: PartShape::Path { path: MOUSE_SHELL, view: [112.0, 184.0] },
            look: PartLook::Plate,
            device: 1,
        },
    ];
    let devices = vec![Device { rect: keyboard, rows: true }, Device { rect: mouse_rect, rows: false }];
    let size = [mouse_rect[0] + mouse_rect[2], keyboard[1] + keyboard[3]];
    Layout { name: "Keyboard and Mouse".into(), size, controls: c, body, devices }
}

/// The pad is drawn in pixels of a body 520 wide, against a key 32 wide.
const PAD_PX: f32 = 32.0;

/// The pad's body: a wide top dipping a little in the middle, grips
/// flaring down and out, and a high arch between them.
const PAD_BODY: &str = "M260 68C300 68 330 60 370 60C430 60 470 68 490 96C510 126 516 176 516 236\
                        C516 296 508 346 470 348C440 350 420 326 400 296C384 270 362 250 330 250L190 250\
                        C158 250 136 270 120 296C100 326 80 350 50 348C12 346 4 296 4 236C4 176 10 126 30 96\
                        C50 68 90 60 150 60C190 60 220 68 260 68Z";
/// A trigger, rising from the top of the body.
const PAD_TRIGGER: &str = "M4 30C8 12 32 3 66 3C98 3 122 12 126 30Z";
/// The bumpers, curving along the body's shoulders.
const PAD_LEFT_BUMPER: &str = "M3 21C18 9 54 3 96 3C122 3 138 6 147 10L147 16C135 13 120 12 96 12C60 12 28 15 9 22Z";
const PAD_RIGHT_BUMPER: &str = "M147 21C132 9 96 3 54 3C28 3 12 6 3 10L3 16C15 13 30 12 54 12C90 12 122 15 141 22Z";

/// A gamepad, its buttons named as on an Xbox pad: the left stick high and
/// the right low, the d-pad below the left stick and the face buttons above
/// the right.
pub fn gamepad() -> Layout {
    let at = |[x, y, w, h]: [f32; 4]| [x / PAD_PX, y / PAD_PX, w / PAD_PX, h / PAD_PX];
    let pad = |paths: &[&str], label: &str, rect: [f32; 4], shape: ControlShape, side: LabelSide| LayoutControl {
        side: Some(side),
        ..control(paths.iter().map(|p| format!("Gamepad/{p}")).collect(), label, at(rect), shape)
    };
    let round = |[x, y]: [f32; 2], r: f32| [x - r, y - r, r * 2.0, r * 2.0];
    let stick = |side: &str| -> Vec<String> {
        ["", "X", "Y", "Up", "Down", "Left", "Right", "Press"].iter().map(|part| format!("{side}Stick{part}")).collect()
    };
    let left: Vec<String> = stick("Left");
    let right: Vec<String> = stick("Right");
    let left: Vec<&str> = left.iter().map(String::as_str).collect();
    let right: Vec<&str> = right.iter().map(String::as_str).collect();
    let trigger = ControlShape::Path { path: PAD_TRIGGER, view: [130.0, 30.0] };
    let bumper = |path| ControlShape::Path { path, view: [150.0, 22.0] };
    use ControlShape::{Key, Round, Stick};
    use LabelSide::{Bottom, Left, Right, Top};
    let (ls, dpad, rs, faces, home) = ([128.0, 132.0], [190.0, 202.0], [330.0, 202.0], [395.0, 132.0], [260.0, 92.0]);
    let face = |dx: f32, dy: f32| round([faces[0] + dx, faces[1] + dy], 14.0);
    let c = vec![
        pad(&["LeftTrigger"], "LT", [70.0, 6.0, 130.0, 30.0], trigger, Left),
        pad(&["RightTrigger"], "RT", [320.0, 6.0, 130.0, 30.0], trigger, Right),
        pad(&["LeftShoulder"], "LB", [56.0, 38.0, 150.0, 22.0], bumper(PAD_LEFT_BUMPER), Left),
        pad(&["RightShoulder"], "RB", [314.0, 38.0, 150.0, 22.0], bumper(PAD_RIGHT_BUMPER), Right),
        pad(&left, "LS", round(ls, 28.0), Stick, Left),
        pad(&["DpadUp"], "↑", [180.0, 170.0, 20.0, 22.0], Key, Left),
        pad(&["DpadLeft"], "←", [158.0, 192.0, 22.0, 20.0], Key, Left),
        pad(&["DpadRight"], "→", [200.0, 192.0, 22.0, 20.0], Key, Bottom),
        pad(&["DpadDown"], "↓", [180.0, 212.0, 20.0, 22.0], Key, Left),
        LayoutControl { icon: Some("copy"), ..pad(&["Select"], "View", round([222.0, 130.0], 9.0), Round, Top) },
        LayoutControl { icon: Some("ring"), ..pad(&["Mode"], "Home", round(home, 14.0), Round, Top) },
        LayoutControl { icon: Some("menu"), ..pad(&["Start"], "Menu", round([298.0, 130.0], 9.0), Round, Top) },
        pad(&["North"], "Y", face(0.0, -32.0), Round, Right),
        pad(&["West"], "X", face(-32.0, 0.0), Round, Right),
        pad(&["East"], "B", face(32.0, 0.0), Round, Right),
        pad(&["South"], "A", face(0.0, 32.0), Round, Right),
        pad(&right, "RS", round(rs, 28.0), Stick, Right),
    ];
    let well = |centre: [f32; 2], r: f32| BodyPart {
        rect: at(round(centre, r)),
        shape: PartShape::Circle,
        look: PartLook::Recess,
        device: 0,
    };
    let body = vec![
        BodyPart {
            rect: at([0.0, 0.0, 520.0, 352.0]),
            shape: PartShape::Path { path: PAD_BODY, view: [520.0, 352.0] },
            look: PartLook::Plate,
            device: 0,
        },
        well(ls, 42.0),
        well(dpad, 36.0),
        well(rs, 40.0),
        well(faces, 52.0),
        well(home, 20.0),
        BodyPart {
            rect: at([180.0, 192.0, 20.0, 20.0]),
            shape: PartShape::Rect { radius: 0.0 },
            look: PartLook::Part,
            device: 0,
        },
    ];
    let size = [520.0 / PAD_PX, 352.0 / PAD_PX];
    let devices = vec![Device { rect: [0.0, 0.0, size[0], size[1]], rows: false }];
    Layout { name: "Gamepad".into(), size, controls: c, body, devices }
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
    fn every_control_is_inside_its_device() {
        for layout in [keyboard_mouse(), gamepad()] {
            for control in &layout.controls {
                let [x, y, w, h] = control.rect;
                let [dx, dy, dw, dh] = layout.devices[control.device].rect;
                let inside = x >= dx - 1e-4 && y >= dy - 1e-4 && x + w <= dx + dw + 1e-4 && y + h <= dy + dh + 1e-4;
                assert!(inside, "{} is outside its device", control.label);
            }
        }
        let keyboard = keyboard_mouse();
        assert_eq!(keyboard.control_of("Mouse/Delta").map(|c| (c.device, c.icon)), Some((1, Some("move"))));
    }

    #[test]
    fn a_pads_controls_sit_in_its_wells_and_label_where_they_lead_cleanly() {
        let pad = gamepad();
        let centre = |path: &str| {
            let [x, y, w, h] = pad.control_of(path).unwrap().rect;
            [x + w / 2.0, y + h / 2.0]
        };
        let in_a_well = |[x, y]: [f32; 2]| {
            pad.body.iter().filter(|p| p.look == PartLook::Recess).any(|p| {
                let [px, py, pw, _] = p.rect;
                let r = pw / 2.0;
                (x - px - r).hypot(y - py - r) <= r
            })
        };
        for path in ["Gamepad/LeftStick", "Gamepad/DpadUp", "Gamepad/RightStick", "Gamepad/North", "Gamepad/Mode"] {
            assert!(in_a_well(centre(path)), "{path} sits in a well");
        }
        // The left stick high and the right low, the face buttons above the right stick.
        assert!(centre("Gamepad/LeftStick")[1] < centre("Gamepad/RightStick")[1]);
        assert!(centre("Gamepad/North")[1] < centre("Gamepad/RightStick")[1]);
        let side = |path: &str| pad.control_of(path).unwrap().side;
        assert_eq!(side("Gamepad/LeftTrigger"), Some(LabelSide::Left));
        assert_eq!(side("Gamepad/RightShoulder"), Some(LabelSide::Right));
        assert_eq!(side("Gamepad/Select"), Some(LabelSide::Top));
        assert_eq!(side("Gamepad/DpadRight"), Some(LabelSide::Bottom));
    }
}
