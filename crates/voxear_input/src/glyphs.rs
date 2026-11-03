//! Prompt glyphs: how a control is drawn in a prompt, as the player's own
//! device shows it. A key is a keycap with its label as this keyboard's
//! layout types it; a mouse button is the mouse with that button filled;
//! a gamepad's face buttons are round, lettered and coloured as an Xbox
//! pad's are, or with a PlayStation pad's cross, circle, square and
//! triangle in theirs, or lettered as a Switch's; shoulders and triggers
//! are pills, the sticks rings and the d-pad's directions arrows.
//!
//! A glyph is a description, not a picture: a game's UI draws it (see
//! `voxear_ui::PromptGlyph`), so sheets can restyle every glyph at once.

use crate::devices::Family;

/// The outline a glyph is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphShape {
    /// A keycap.
    #[default]
    Key,
    /// A face button.
    Round,
    /// A shoulder, a trigger, or a pad's menu buttons.
    Pill,
    /// A d-pad direction.
    Square,
    /// A stick, as a ring.
    Stick,
    /// No outline: the mouse.
    Bare,
}

/// One control as a prompt draws it: its outline, the text in it, an icon
/// of Voxear's in place of or beside the text, and the colour the text or
/// icon is drawn in, when the device has one for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glyph {
    pub shape: GlyphShape,
    pub text: String,
    pub icon: Option<&'static str>,
    pub color: Option<[u8; 3]>,
}

impl Glyph {
    fn text(shape: GlyphShape, text: &str) -> Self {
        Self { shape, text: text.to_string(), ..Self::default() }
    }

    fn icon(shape: GlyphShape, icon: &'static str) -> Self {
        Self { shape, icon: Some(icon), ..Self::default() }
    }

    fn colored(mut self, color: [u8; 3]) -> Self {
        self.color = Some(color);
        self
    }
}

/// The glyph of a control by its path, for a pad of `family`, with keys
/// labelled by `key_label`.
pub fn glyph(family: Family, path: &str, key_label: &dyn Fn(&str) -> String) -> Glyph {
    let Some((device, control)) = path.split_once('/') else { return Glyph::text(GlyphShape::Key, path) };
    match device {
        "Keyboard" => Glyph::text(GlyphShape::Key, &key_label(control)),
        "Mouse" => mouse(control),
        "Gamepad" => pad(family, control),
        _ => Glyph::text(GlyphShape::Key, path),
    }
}

fn mouse(control: &str) -> Glyph {
    match control {
        "Left" => Glyph::icon(GlyphShape::Bare, "mouse-left"),
        "Right" => Glyph::icon(GlyphShape::Bare, "mouse-right"),
        "Middle" => Glyph::icon(GlyphShape::Bare, "mouse-middle"),
        "Delta" | "Position" => Glyph::icon(GlyphShape::Bare, "mouse"),
        other => Glyph::text(GlyphShape::Key, other),
    }
}

fn pad(family: Family, control: &str) -> Glyph {
    if let Some(face) = face(family, control) {
        return face;
    }
    let named = |xbox: &str, play: &str, switch: &str| match family {
        Family::PlayStation => play.to_string(),
        Family::Switch => switch.to_string(),
        Family::Xbox | Family::Steam => xbox.to_string(),
    };
    let pill = |xbox: &str, play: &str, switch: &str| Glyph::text(GlyphShape::Pill, &named(xbox, play, switch));
    match control {
        "LeftShoulder" => pill("LB", "L1", "L"),
        "RightShoulder" => pill("RB", "R1", "R"),
        "LeftTrigger" => pill("LT", "L2", "ZL"),
        "RightTrigger" => pill("RT", "R2", "ZR"),
        "Select" => pill("View", "Create", "−"),
        "Start" => pill("Menu", "Options", "+"),
        "LeftStick" => Glyph::text(GlyphShape::Stick, "L"),
        "RightStick" => Glyph::text(GlyphShape::Stick, "R"),
        "LeftStickPress" => Glyph::text(GlyphShape::Stick, &named("LS", "L3", "L")),
        "RightStickPress" => Glyph::text(GlyphShape::Stick, &named("RS", "R3", "R")),
        "DpadUp" => Glyph::icon(GlyphShape::Square, "arrow-up"),
        "DpadDown" => Glyph::icon(GlyphShape::Square, "arrow-down"),
        "DpadLeft" => Glyph::icon(GlyphShape::Square, "arrow-left"),
        "DpadRight" => Glyph::icon(GlyphShape::Square, "arrow-right"),
        other => Glyph::text(GlyphShape::Pill, other),
    }
}

/// A face button: where it is on the pad, as its family marks and colours it.
fn face(family: Family, control: &str) -> Option<Glyph> {
    let at = ["South", "East", "West", "North"].iter().position(|&face| face == control)?;
    let round = |text: &str| Glyph::text(GlyphShape::Round, text);
    Some(match family {
        Family::Xbox => round(["A", "B", "X", "Y"][at]).colored(XBOX[at]),
        Family::PlayStation => Glyph::icon(GlyphShape::Round, PLAYSTATION[at]).colored(PLAYSTATION_COLORS[at]),
        Family::Switch => round(["B", "A", "Y", "X"][at]),
        Family::Steam => round(["A", "B", "X", "Y"][at]),
    })
}

/// The face buttons' colours, south, east, west and north.
const XBOX: [[u8; 3]; 4] = [[0x5d, 0xb5, 0x3a], [0xe0, 0x3c, 0x31], [0x2f, 0x7f, 0xd6], [0xf2, 0xb7, 0x05]];
const PLAYSTATION: [&str; 4] = ["pad-cross", "pad-circle", "pad-square", "pad-triangle"];
const PLAYSTATION_COLORS: [[u8; 3]; 4] =
    [[0x7c, 0xa8, 0xe8], [0xe5, 0x5a, 0x63], [0xd6, 0x8f, 0xd2], [0x3f, 0xc9, 0xa4]];

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> String {
        name.to_uppercase()
    }

    #[test]
    fn each_family_marks_its_face_buttons_its_own_way() {
        let south = |family| glyph(family, "Gamepad/South", &key);
        assert_eq!(
            south(Family::Xbox),
            Glyph { shape: GlyphShape::Round, text: "A".into(), icon: None, color: Some(XBOX[0]) }
        );
        assert_eq!(south(Family::PlayStation).icon, Some("pad-cross"));
        assert_eq!(south(Family::Switch).text, "B", "a Switch's south button is its B");
        assert_eq!(glyph(Family::PlayStation, "Gamepad/LeftTrigger", &key).text, "L2");
    }

    #[test]
    fn keys_mice_sticks_and_the_dpad_have_their_own_glyphs() {
        assert_eq!(glyph(Family::Xbox, "Keyboard/q", &key), Glyph::text(GlyphShape::Key, "Q"));
        assert_eq!(glyph(Family::Xbox, "Mouse/Right", &key).icon, Some("mouse-right"));
        assert_eq!(glyph(Family::Xbox, "Gamepad/LeftStick", &key).shape, GlyphShape::Stick);
        assert_eq!(glyph(Family::Xbox, "Gamepad/DpadUp", &key).icon, Some("arrow-up"));
    }
}
