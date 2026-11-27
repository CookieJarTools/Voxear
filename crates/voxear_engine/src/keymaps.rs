//! Key maps of the game's input: a device's layout (see
//! `voxear_input::layouts`) with each control's actions, as Voxear UI's key
//! map draws them (see `voxear_ui::KeymapData`). The editor's Input Actions
//! window and a game's own key binding screen both draw them from here.
//!
//! Prompts draw an action's controls as glyphs from here too (see
//! [`prompt_glyphs`]).
//!
//! A key bound to actions shown is assigned, and labelled with them, each
//! with its icon; one bound only to hidden actions is marked hidden,
//! unlabelled unless hidden actions are shown; the rest are unused. A key
//! two actions that can be on at once both read is a conflict, whichever
//! map is shown. Coloured by category, each assigned key takes the colour
//! of its first action's category, but for a conflict, which stays marked.
//! The device's outline is drawn behind its keys.

use voxear_input::{ControlUse, Input, Scheme, layouts};

use crate::prompt_icons::PromptSprites;
use voxear_ui::{
    ArtLook, ArtShape, Color, GlyphShape, KeyShape, KeyState, KeymapArt, KeymapData, KeymapDevice, KeymapKey,
    KeymapLine, KeymapSide, PromptGlyph,
};

/// An action's controls, by its name (`Jump` or `Player/Jump`), as a
/// prompt draws them on the device the player used last: none for an
/// action there is not, or one with no binding for that device.
pub fn prompt_glyphs(input: &Input, action: &str) -> Vec<PromptGlyph> {
    prompt_glyphs_with(input, action, None)
}

/// An action's controls as [`prompt_glyphs`] gives them, each the
/// picture `sprites` has for it in place of the glyph Voxear draws, where
/// it has one (see [`crate::prompt_icons`]).
pub fn prompt_glyphs_with(input: &Input, action: &str, sprites: Option<&PromptSprites>) -> Vec<PromptGlyph> {
    let Some(action) = input.action(action) else { return Vec::new() };
    let glyphs = input.glyphs(action).into_iter().map(prompt_glyph);
    let Some(sprites) = sprites else { return glyphs.collect() };
    let family = input.pad_family();
    let paths = input.prompt_controls(action);
    glyphs
        .zip(paths)
        .map(|(glyph, path)| match sprites.get(&path, family).filter(|_| types_as_us(input, &path)) {
            Some(image) => PromptGlyph { image: Some(image), ..glyph },
            None => glyph,
        })
        .collect()
}

/// Whether a control shows what a picture of it would: anything but a key
/// this keyboard types differently from a US one, as `Q` types A on a
/// French keyboard, whose picture would show the wrong letter. That key is
/// drawn as Voxear draws it, with the letter it types.
fn types_as_us(input: &Input, path: &str) -> bool {
    static US: std::sync::OnceLock<layouts::Layout> = std::sync::OnceLock::new();
    let Some(key) = path.strip_prefix("Keyboard/") else { return true };
    let Some(typed) = input.devices().typed(key) else { return true };
    let us = US.get_or_init(layouts::keyboard_mouse).control_of(path).map(|control| control.label.as_str());
    us.is_none_or(|us| typed.eq_ignore_ascii_case(us))
}

/// A control, by its path, as a prompt draws it on the player's device.
pub fn control_prompt_glyph(input: &Input, path: &str) -> PromptGlyph {
    prompt_glyph(input.control_glyph(path))
}

/// A control as [`control_prompt_glyph`] gives it, with the picture
/// `sprites` has for it in place of the drawn glyph, where it has one.
pub fn control_prompt_glyph_with(input: &Input, path: &str, sprites: Option<&PromptSprites>) -> PromptGlyph {
    let glyph = control_prompt_glyph(input, path);
    let image = sprites.and_then(|sprites| sprites.get(path, input.pad_family())).filter(|_| types_as_us(input, path));
    PromptGlyph { image: image.or(glyph.image), ..glyph }
}

fn prompt_glyph(glyph: voxear_input::Glyph) -> PromptGlyph {
    use voxear_input::GlyphShape as Input;
    let shape = match glyph.shape {
        Input::Key => GlyphShape::Key,
        Input::Round => GlyphShape::Round,
        Input::Pill => GlyphShape::Pill,
        Input::Square => GlyphShape::Square,
        Input::Stick => GlyphShape::Stick,
        Input::Bare => GlyphShape::Bare,
    };
    let color = glyph.color.map(|[r, g, b]| Color::rgb(r, g, b));
    PromptGlyph { shape, text: glyph.text, icon: glyph.icon, color, image: None }
}

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
/// or pad names it, and its actions, each with a chip naming the control as
/// the action reads it, as `L3` for a stick pressed.
pub fn keymap_data(input: &Input, scheme: Scheme, options: &KeymapOptions) -> KeymapData {
    let layout = layout_of(scheme);
    let every = input.control_uses();
    let uses: Vec<&ControlUse> =
        every.iter().filter(|u| options.map.as_ref().is_none_or(|map| *map == u.map)).collect();
    let keys = layout
        .controls
        .iter()
        .map(|control| {
            let on: Vec<&ControlUse> = uses.iter().copied().filter(|u| control.paths.contains(&u.path)).collect();
            let shown: Vec<&&ControlUse> = on.iter().filter(|u| !u.hidden || options.show_hidden).collect();
            let conflict = every.iter().any(|u| u.conflict && control.paths.contains(&u.path));
            let state = match (shown.iter().any(|u| !u.hidden), on.is_empty()) {
                _ if conflict => KeyState::Conflict,
                (true, _) => KeyState::Assigned,
                (false, false) => KeyState::Hidden,
                (false, true) => KeyState::Unused,
            };
            let face = face(input, &control.paths[0], &control.label);
            let mut callout: Vec<KeymapLine> = Vec::new();
            for found in &shown {
                let (text, chip) = (found.label(), chip(&found.path, &face));
                if !callout.iter().any(|line| line.text == text && line.chip == chip) {
                    callout.push(KeymapLine { text, icon: found.icon.clone(), chip });
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
                    layouts::ControlShape::Path { path, view } => KeyShape::Path { path, view },
                },
                label: face,
                icon: control.icon.map(str::to_string),
                caption: control.caption.map(str::to_string),
                state,
                color,
                callout,
                device: control.device,
                side: control.side.map(|side| match side {
                    layouts::LabelSide::Left => KeymapSide::Left,
                    layouts::LabelSide::Right => KeymapSide::Right,
                    layouts::LabelSide::Top => KeymapSide::Top,
                    layouts::LabelSide::Bottom => KeymapSide::Bottom,
                }),
            }
        })
        .collect();
    let art = layout
        .body
        .iter()
        .map(|part| KeymapArt {
            rect: part.rect,
            shape: match part.shape {
                layouts::PartShape::Rect { radius } => ArtShape::Rect { radius },
                layouts::PartShape::Circle => ArtShape::Circle,
                layouts::PartShape::Path { path, view } => ArtShape::Path { path, view },
            },
            look: match part.look {
                layouts::PartLook::Plate => ArtLook::Plate,
                layouts::PartLook::Recess => ArtLook::Recess,
                layouts::PartLook::Part => ArtLook::Part,
            },
            device: part.device,
        })
        .collect();
    let devices = layout.devices.iter().map(|d| KeymapDevice { rect: d.rect, rows: d.rows }).collect();
    KeymapData { size: layout.size, keys, art, devices }
}

/// The chip naming a control in a label: what its face says, but for a
/// stick pressed, as `L3`, and the mouse's parts by what they are.
fn chip(path: &str, face: &str) -> String {
    let named = match path {
        "Gamepad/LeftStickPress" => "L3",
        "Gamepad/RightStickPress" => "R3",
        "Mouse/Scroll" => "Wheel",
        "Mouse/Middle" => "Middle",
        "Mouse/Delta" | "Mouse/Position" => "Move",
        _ => face,
    };
    named.to_string()
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
    entries.push(("Conflict".to_string(), None, KeyState::Conflict));
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
    fn a_prompt_draws_an_actions_controls_on_the_device_used_last() {
        let mut input = Input::without_gamepads();
        input.load_actions(ACTIONS).unwrap();
        let jump = prompt_glyphs(&input, "Jump");
        assert_eq!(jump.len(), 1);
        assert_eq!((jump[0].shape, jump[0].text.as_str()), (GlyphShape::Key, "Space"));
        let moves: Vec<String> = prompt_glyphs(&input, "Player/Move").into_iter().map(|g| g.text).collect();
        assert_eq!(moves, ["W", "S"], "a composite's every part");
        assert!(prompt_glyphs(&input, "Fly").is_empty(), "no such action");

        let pad = input.glyphs_for(input.action("Jump").unwrap(), Scheme::Gamepad);
        let south = prompt_glyph(pad[0].clone());
        assert_eq!((south.shape, south.text.as_str()), (GlyphShape::Round, "A"));
        assert_eq!(south.color, Some(Color::rgb(0x5d, 0xb5, 0x3a)), "an Xbox pad's green A");
    }

    #[test]
    fn keys_are_assigned_hidden_or_unused_and_coloured_by_category() {
        let mut input = Input::without_gamepads();
        input.load_actions(ACTIONS).unwrap();
        let data = keymap_data(&input, Scheme::KeyboardMouse, &KeymapOptions::default());
        let key = |face: &str| data.keys.iter().find(|k| k.label == face).unwrap().clone();
        assert_eq!(
            (key("W").state, key("W").callout.clone()),
            (KeyState::Assigned, vec![KeymapLine::with_chip("Move (Up)", "W")])
        );
        assert_eq!((key("N").state, key("N").callout.len()), (KeyState::Hidden, 0), "hidden, unlabelled");
        assert_eq!(key("Q").state, KeyState::Unused);
        assert_eq!(key("W").color, None, "the sheets colour assigned keys");

        let options = KeymapOptions { colors: KeymapColors::Categories, show_hidden: true, map: None };
        let data = keymap_data(&input, Scheme::KeyboardMouse, &options);
        let key = |face: &str| data.keys.iter().find(|k| k.label == face).unwrap().clone();
        assert_eq!(key("W").color, Some(Color::hex(0x3fb950)));
        assert_eq!(key("Space").color, None, "no category");
        assert_eq!(key("N").callout, [KeymapLine::with_chip("Noclip", "N")], "hidden actions shown when asked");
        let legend: Vec<String> = legend(&input, KeymapColors::Categories).into_iter().map(|e| e.0).collect();
        assert_eq!(legend, ["No category", "Movement", "Conflict", "Hidden", "Unused"]);

        let pad = keymap_data(&input, Scheme::Gamepad, &KeymapOptions::default());
        let a = pad.keys.iter().find(|k| k.callout == [KeymapLine::with_chip("Jump", "A")]).unwrap();
        assert_eq!(a.label, "A");
        let other = KeymapOptions { map: Some("UI".into()), ..KeymapOptions::default() };
        assert!(keymap_data(&input, Scheme::KeyboardMouse, &other).keys.iter().all(|k| k.state == KeyState::Unused));
    }

    /// A sailing game's actions, every control labelled and some in conflict.
    const SAILING: &str = r#"format = 1

[[map]]
name = "Player"

[[map.action]]
name = "Move"
type = "vector2"
bindings = [{ up = "Keyboard/W", down = "Keyboard/S", left = "Keyboard/A", right = "Keyboard/D" }, "Gamepad/LeftStick"]

[[map.action]]
name = "Look"
type = "vector2"
bindings = ["Mouse/Delta", "Gamepad/RightStick"]

[[map.action]]
name = "Zoom"
type = "vector2"
bindings = ["Mouse/Scroll", { up = "Gamepad/DpadUp", down = "Gamepad/DpadDown" }]

[[map.action]]
name = "Special 1"
bindings = ["Keyboard/1", "Gamepad/DpadRight"]

[[map.action]]
name = "Special 2"
bindings = ["Keyboard/2", "Gamepad/RightShoulder"]

[[map.action]]
name = "Special 3"
bindings = ["Keyboard/3", "Gamepad/LeftShoulder"]

[[map.action]]
name = "Interact"
bindings = ["Keyboard/E", "Gamepad/West"]

[[map.action]]
name = "Run"
bindings = ["Keyboard/ShiftLeft", "Gamepad/LeftStickPress"]

[[map.action]]
name = "Sail Colour"
bindings = ["Keyboard/Z", "Gamepad/DpadUp"]

[[map.action]]
name = "Sail Emblem"
bindings = ["Keyboard/X", "Gamepad/DpadDown"]

[[map.action]]
name = "Emblem Colour"
bindings = ["Keyboard/C", "Gamepad/DpadLeft"]

[[map.action]]
name = "Fish"
bindings = ["Keyboard/F", "Gamepad/North"]

[[map.action]]
name = "Hoist"
bindings = ["Keyboard/H", "Gamepad/Select"]

[[map.action]]
name = "Cannons"
bindings = ["Keyboard/V", "Gamepad/RightStickPress"]

[[map.action]]
name = "Bait"
bindings = ["Keyboard/B", "Gamepad/DpadLeft"]

[[map.action]]
name = "Open Map"
bindings = ["Keyboard/M"]

[[map.action]]
name = "Free Pointer"
bindings = ["Keyboard/Escape"]

[[map.action]]
name = "Dive"
bindings = ["Keyboard/ControlLeft", "Gamepad/East"]

[[map.action]]
name = "Jump"
bindings = ["Keyboard/Space", "Gamepad/South"]

[[map.action]]
name = "Attack"
bindings = ["Mouse/Left", "Gamepad/RightTrigger"]

[[map.action]]
name = "Heavy Attack"
bindings = ["Mouse/Right", "Gamepad/LeftTrigger"]

[[map.action]]
name = "Hold Pointer"
bindings = ["Mouse/Left"]
"#;

    #[test]
    fn every_binding_is_labelled_with_a_line_that_crosses_nothing() {
        let mut input = Input::without_gamepads();
        input.load_actions(SAILING).unwrap();
        for scheme in [Scheme::KeyboardMouse, Scheme::Gamepad] {
            let data = keymap_data(&input, scheme, &KeymapOptions::default());
            for size in [[1500, 620], [1200, 540]] {
                let mut ui = voxear_ui::Ui::new();
                let root = ui.root();
                let holder = ui.panel(root, voxear_ui::Style::cover());
                let map = ui.keymap(holder);
                ui.set_keymap(map, data.clone());
                for _ in 0..3 {
                    ui.frame(size[0], size[1], 1.0);
                }
                let labelled = data.keys.iter().filter(|k| !k.callout.is_empty()).count();
                assert_eq!(ui.keymap_lines(map).len(), labelled, "{scheme:?}: a line for each labelled control");
                let problems = ui.keymap_crossings(map);
                assert!(problems.is_empty(), "{scheme:?} at {size:?}: {problems:#?}");
                if scheme == Scheme::KeyboardMouse {
                    // Labels stand in their keys' order.
                    let top = |text: &str| label_top(&ui, map, text).unwrap_or_else(|| panic!("{text} labelled"));
                    for order in
                        [["Special 1", "Special 2", "Special 3"], ["Move (Left)", "Move (Down)", "Move (Right)"]]
                    {
                        assert!(top(order[0]) < top(order[1]) && top(order[1]) < top(order[2]), "{order:?}");
                    }
                    assert!(top("Sail Colour") < top("Sail Emblem") && top("Sail Emblem") < top("Emblem Colour"));
                }
            }
        }
        let keyboard = keymap_data(&input, Scheme::KeyboardMouse, &KeymapOptions::default());
        let sides = |data: &KeymapData, face: &str| {
            let at = data.keys.iter().position(|k| k.label == face).unwrap();
            (voxear_ui::keymap_sides(data)[at], data.keys[at].state)
        };
        assert_eq!(sides(&keyboard, "Esc").0, KeymapSide::Top);
        assert_eq!(sides(&keyboard, "Space").0, KeymapSide::Bottom);
        assert_eq!(sides(&keyboard, "A").0, KeymapSide::Left);
        assert_eq!(sides(&keyboard, "F").0, KeymapSide::Right, "no more than three of a row to one side");
        assert_eq!(sides(&keyboard, "Left"), (KeymapSide::Top, KeyState::Conflict), "Attack and Hold Pointer");
        let pad = keymap_data(&input, Scheme::Gamepad, &KeymapOptions::default());
        let stick = pad.keys.iter().find(|k| k.label == "LS").unwrap();
        assert_eq!(stick.callout, [KeymapLine::with_chip("Move", "LS"), KeymapLine::with_chip("Run", "L3")]);
        assert_eq!(sides(&pad, "←").1, KeyState::Conflict, "Bait and Emblem Colour");
    }

    /// Where the label naming an action is, from the top, by the text in it.
    fn label_top(ui: &voxear_ui::Ui, node: voxear_ui::NodeId, text: &str) -> Option<f32> {
        if ui.has_class(node, "keymap-callout") {
            let mut found = false;
            let mut stack = vec![node];
            while let Some(at) = stack.pop() {
                found |= ui.text(at) == text;
                stack.extend(ui.children(at));
            }
            return found.then(|| ui.rect(node)[1]);
        }
        ui.children(node).into_iter().find_map(|child| label_top(ui, child, text))
    }

    #[test]
    fn a_key_two_actions_on_at_once_read_is_a_conflict() {
        let text = "format = 1\n\n[[map]]\nname = \"Player\"\nexcludes = [\"UI\"]\n\n[[map.action]]\nname = \"Jump\"\n\
                    icon = \"arrow-up\"\nbindings = [\"Keyboard/Space\"]\n\n[[map.action]]\nname = \"Dash\"\n\
                    bindings = [\"Keyboard/E\"]\n\n[[map]]\nname = \"UI\"\n\n[[map.action]]\nname = \"Submit\"\n\
                    bindings = [\"Keyboard/Space\"]\n\n[[map]]\nname = \"Debug\"\n\n[[map.action]]\nname = \"Fly\"\n\
                    bindings = [\"Keyboard/E\"]\n";
        let mut input = Input::without_gamepads();
        input.load_actions(text).unwrap();
        let data = keymap_data(&input, Scheme::KeyboardMouse, &KeymapOptions::default());
        let key = |face: &str| data.keys.iter().find(|k| k.label == face).unwrap().clone();
        assert_eq!(key("Space").state, KeyState::Assigned, "Player and UI are never on together");
        assert_eq!(key("Space").callout[0].icon.as_deref(), Some("arrow-up"));
        assert_eq!(key("E").state, KeyState::Conflict, "Debug can be on with Player");
        let only_player = KeymapOptions { map: Some("Player".into()), ..KeymapOptions::default() };
        let data = keymap_data(&input, Scheme::KeyboardMouse, &only_player);
        let e = data.keys.iter().find(|k| k.label == "E").unwrap();
        assert_eq!((e.state, e.callout.len()), (KeyState::Conflict, 1), "marked whichever map is shown");
        assert!(!data.art.is_empty(), "the keyboard drawn behind its keys");
        assert_eq!(data.devices.len(), 2, "the keyboard and the mouse");
    }
}
