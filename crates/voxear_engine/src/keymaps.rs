//! Key maps of the game's input: a device's layout (see
//! `voxear_input::layouts`) with each control's actions, as Voxear UI's key
//! map draws them (see `voxear_ui::KeymapData`). The editor's Input Actions
//! window and a game's own key binding screen both draw them from here.
//!
//! A key bound to actions shown is assigned, and labelled with them; one
//! bound only to hidden actions is marked hidden, unlabelled unless hidden
//! actions are shown; the rest are unused. Coloured by category, each
//! assigned key takes the colour of its first action's category.

use voxear_input::{ControlUse, Input, Scheme, layouts};
use voxear_ui::{Color, KeyShape, KeyState, KeymapData, KeymapKey};

/// How keys are coloured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeymapColors {
    /// Assigned, hidden or unused, as the sheets colour them.
    #[default]
    Assigned,
    /// By each key's actions' category, as the input actions colour them.
    Categories,
}

/// What a key map shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapOptions {
    pub colors: KeymapColors,
    /// Labels hidden actions too.
    pub show_hidden: bool,
    /// Only this map's actions, by name; `None` for every map.
    pub map: Option<String>,
}

/// The device a scheme is played with, laid out.
pub fn layout_of(scheme: Scheme) -> layouts::Layout {
    match scheme {
        Scheme::KeyboardMouse => layouts::keyboard_mouse(),
        Scheme::Gamepad => layouts::gamepad(),
    }
}

/// A scheme's device as a key map: each control's label as this keyboard
/// or pad names it, and its actions.
pub fn keymap_data(input: &Input, scheme: Scheme, options: &KeymapOptions) -> KeymapData {
    let layout = layout_of(scheme);
    let uses: Vec<ControlUse> =
        input.control_uses().into_iter().filter(|u| options.map.as_ref().is_none_or(|map| *map == u.map)).collect();
    let keys = layout
        .controls
        .iter()
        .map(|control| {
            let on: Vec<&ControlUse> = uses.iter().filter(|u| control.paths.contains(&u.path)).collect();
            let shown: Vec<&&ControlUse> = on.iter().filter(|u| !u.hidden || options.show_hidden).collect();
            let state = match (shown.iter().any(|u| !u.hidden), on.is_empty()) {
                (true, _) => KeyState::Assigned,
                (false, false) => KeyState::Hidden,
                (false, true) => KeyState::Unused,
            };
            let mut callout: Vec<String> = Vec::new();
            for found in &shown {
                let label = found.label();
                if !callout.contains(&label) {
                    callout.push(label);
                }
            }
            let color = match (options.colors, state) {
                (KeymapColors::Categories, KeyState::Assigned) => {
                    let first = shown.iter().find(|u| !u.hidden).and_then(|u| u.category.as_deref());
                    first.and_then(|name| category_color(input, name))
                }
                _ => None,
            };
            KeymapKey {
                rect: control.rect,
                shape: match control.shape {
                    layouts::ControlShape::Key => KeyShape::Key,
                    layouts::ControlShape::Round => KeyShape::Round,
                    layouts::ControlShape::Stick => KeyShape::Stick,
                    layouts::ControlShape::Pill => KeyShape::Pill,
                },
                label: face(input, &control.paths[0], &control.label),
                state,
                color,
                callout,
            }
        })
        .collect();
    KeymapData { size: layout.size, keys }
}

/// The legend for a key map coloured `colors`: each entry's name and its
/// colour, `None` for one the sheets colour, with the state it stands for.
pub fn legend(input: &Input, colors: KeymapColors) -> Vec<(String, Option<Color>, KeyState)> {
    let mut entries = Vec::new();
    match colors {
        KeymapColors::Assigned => entries.push(("Assigned".to_string(), None, KeyState::Assigned)),
        KeymapColors::Categories => {
            entries.push(("No category".to_string(), None, KeyState::Assigned));
            for category in input.categories() {
                entries.push((category.name.clone(), category_color(input, &category.name), KeyState::Assigned));
            }
        }
    }
    entries.push(("Hidden".to_string(), None, KeyState::Hidden));
    entries.push(("Unused".to_string(), None, KeyState::Unused));
    entries
}

/// A category's colour, from its `#rrggbb`.
pub fn category_color(input: &Input, name: &str) -> Option<Color> {
    let category = input.categories().iter().find(|c| c.name == name)?;
    let hex = category.color.trim().trim_start_matches('#');
    (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten().map(Color::hex)
}

/// What a control's face says: a key as this keyboard types it, a pad's
/// button as its family names it, and the layout's own name for the rest.
fn face(input: &Input, path: &str, fallback: &str) -> String {
    let named = match path.split_once('/') {
        Some(("Keyboard", key)) if key.chars().count() == 1 => input.control_label(path),
        Some(("Gamepad", control)) if !control.contains("Stick") && !control.starts_with("Dpad") => {
            input.control_label(path)
        }
        _ => String::new(),
    };
    if named.is_empty() || named.chars().count() > 6 { fallback.to_string() } else { named }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIONS: &str = "format = 1\n\n[[category]]\nname = \"Movement\"\ncolor = \"#3fb950\"\n\n\
                           [[map]]\nname = \"Player\"\n\n[[map.action]]\nname = \"Move\"\ntype = \"vector2\"\n\
                           category = \"Movement\"\nbindings = [{ up = \"Keyboard/W\", down = \"Keyboard/S\" }]\n\n\
                           [[map.action]]\nname = \"Jump\"\nbindings = [\"Keyboard/Space\", \"Gamepad/South\"]\n\n\
                           [[map.action]]\nname = \"Noclip\"\nhidden = true\nbindings = [\"Keyboard/N\"]\n";

    #[test]
    fn keys_are_assigned_hidden_or_unused_and_coloured_by_category() {
        let mut input = Input::without_gamepads();
        input.load_actions(ACTIONS).unwrap();
        let data = keymap_data(&input, Scheme::KeyboardMouse, &KeymapOptions::default());
        let key = |face: &str| data.keys.iter().find(|k| k.label == face).unwrap().clone();
        assert_eq!((key("W").state, key("W").callout.clone()), (KeyState::Assigned, vec!["Move (Up)".to_string()]));
        assert_eq!((key("N").state, key("N").callout.len()), (KeyState::Hidden, 0), "hidden, unlabelled");
        assert_eq!(key("Q").state, KeyState::Unused);
        assert_eq!(key("W").color, None, "the sheets colour assigned keys");

        let options = KeymapOptions { colors: KeymapColors::Categories, show_hidden: true, map: None };
        let data = keymap_data(&input, Scheme::KeyboardMouse, &options);
        let key = |face: &str| data.keys.iter().find(|k| k.label == face).unwrap().clone();
        assert_eq!(key("W").color, Some(Color::hex(0x3fb950)));
        assert_eq!(key("Space").color, None, "no category");
        assert_eq!(key("N").callout, ["Noclip"], "hidden actions shown when asked");
        let legend: Vec<String> = legend(&input, KeymapColors::Categories).into_iter().map(|e| e.0).collect();
        assert_eq!(legend, ["No category", "Movement", "Hidden", "Unused"]);

        let pad = keymap_data(&input, Scheme::Gamepad, &KeymapOptions::default());
        let a = pad.keys.iter().find(|k| k.callout == ["Jump"]).unwrap();
        assert_eq!(a.label, "A");
        let other = KeymapOptions { map: Some("UI".into()), ..KeymapOptions::default() };
        assert!(keymap_data(&input, Scheme::KeyboardMouse, &other).keys.iter().all(|k| k.state == KeyState::Unused));
    }
}
