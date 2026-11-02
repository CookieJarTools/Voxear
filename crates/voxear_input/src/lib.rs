//! Input, as Unity's Input System has it: the keyboard, the mouse and
//! gamepads read as actions, such as Jump or Move, which a game asks about
//! by name rather than by key.
//!
//! - **Actions** come in maps, from the project's `.inputactions` file, or
//!   from Voxear's own when it has none (see [`actions`]). Each frame,
//!   [`Input::update`] reads every action from the devices: its value,
//!   whether it is held, and whether it went down, went up or was performed
//!   this frame.
//! - **Control schemes** are the keyboard and mouse together, or a gamepad.
//!   The one used last is the one prompts show, switching at once.
//! - **Rebinding** points a binding at another control: one named, or the
//!   next one pressed, as a rebinding screen listens for. Escape gives up
//!   listening. A player's bindings are saved as text and read back.
//! - **Prompts** name what to press for an action in the scheme used last:
//!   a key as this keyboard types it, and a pad's button as its family
//!   names it (A on Xbox, Cross on PlayStation).
//! - **Key maps** draw the keyboard and mouse, or a gamepad ([`layouts`]),
//!   with each control's actions ([`Input::control_uses`]), coloured by
//!   their categories.
//!
//! The host hands it the window's events and the mouse's moves, and calls
//! [`Input::update`] once a frame before the game's scripts run.

pub mod actions;
pub mod devices;
pub mod layouts;

use anyhow::{Context, Result};
pub use actions::{ActionKind, ActionState, ActionsFile, Binding, CategoryData};
pub use devices::{Devices, Family, Mouse, Pad, Scheme};
use winit::event::WindowEvent;

/// Voxear's own actions, for a project without any.
pub const DEFAULT_ACTIONS: &str = include_str!("defaults.inputactions");

/// An action by its place among every map's actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActionId(pub usize);

/// What rebinding is listening for: a binding of an action, or a part of
/// a composite one, and the scheme of the control to take.
#[derive(Clone, Debug, PartialEq)]
struct Listening {
    action: ActionId,
    binding: usize,
    part: Option<String>,
    scheme: Option<Scheme>,
}

/// One use of a control: the action a binding of it reads, and which part
/// of a composite binding, as a key map shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlUse {
    pub path: String,
    pub action: ActionId,
    pub map: String,
    pub name: String,
    /// Which binding of the action, and its part: `up` of a WASD binding.
    pub binding: usize,
    pub part: Option<&'static str>,
    pub category: Option<String>,
    /// The action keeps out of key maps, as a debug key does.
    pub hidden: bool,
}

impl ControlUse {
    /// What a key map says for it: the action, with its part, as
    /// "Move (Up)".
    pub fn label(&self) -> String {
        match self.part {
            Some(part) => {
                let mut part = part.chars();
                let first = part.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
                format!("{} ({first}{})", self.name, part.as_str())
            }
            None => self.name.clone(),
        }
    }
}

/// What listening for a rebind came to.
#[derive(Clone, Debug, PartialEq)]
pub enum Rebound {
    /// The binding now reads this control.
    To(String),
    /// Escape gave it up.
    Cancelled,
}

pub struct Input {
    devices: Devices,
    actions: actions::Actions,
    gilrs: Option<gilrs::Gilrs>,
    listening: Option<Listening>,
    rebound: Option<Rebound>,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    /// Input with Voxear's own actions, and gamepads when the system has
    /// them.
    pub fn new() -> Self {
        let gilrs = gilrs::Gilrs::new()
            .map_err(|error| tracing::info!("gamepads will not be read: {error}"))
            .ok();
        let actions = actions::Actions::parse(DEFAULT_ACTIONS).expect("Voxear's own actions read");
        Self { devices: Devices::default(), actions, gilrs, listening: None, rebound: None }
    }

    /// Input without gamepads, which tests and tools without a desktop use.
    pub fn without_gamepads() -> Self {
        let actions = actions::Actions::parse(DEFAULT_ACTIONS).expect("Voxear's own actions read");
        Self { devices: Devices::default(), actions, gilrs: None, listening: None, rebound: None }
    }

    /// Reads a project's actions from a `.inputactions` file's text, in
    /// place of those it had.
    pub fn load_actions(&mut self, text: &str) -> Result<()> {
        self.actions = actions::Actions::parse(text)?;
        Ok(())
    }

    /// The actions as their file has them, rebound or not.
    pub fn actions_file(&self) -> ActionsFile {
        let mut file = self.actions.file.clone();
        for map in &mut file.maps {
            for action in &mut map.actions {
                if let Some(found) = self.actions.find(&format!("{}/{}", map.name, action.name)) {
                    action.bindings.clone_from(&self.actions.list[found].data.bindings);
                }
            }
        }
        file
    }

    /// Takes a window's event: keys, mouse buttons, the pointer and the
    /// wheel.
    pub fn handle(&mut self, event: &WindowEvent) {
        self.devices.handle(event);
    }

    /// Takes the mouse's moves, from the device.
    pub fn mouse_moved(&mut self, dx: f64, dy: f64) {
        self.devices.mouse_moved(dx, dy);
    }

    /// Lets go of every key and button, as when the game stops or the
    /// window loses the focus.
    pub fn release_all(&mut self) {
        self.devices.release_all();
        self.actions.quiet();
    }

    /// Reads the devices and every action for a frame `dt` seconds long.
    /// While rebinding listens, actions read nothing, so the key pressed
    /// does not also act.
    pub fn update(&mut self, dt: f32) {
        self.devices.begin_frame();
        if let Some(gilrs) = &mut self.gilrs {
            self.devices.poll_pads(gilrs);
        }
        match self.listening.take() {
            Some(listening) => self.listen_frame(listening),
            None => self.actions.update(&self.devices, dt),
        }
        self.devices.end_frame();
    }

    fn listen_frame(&mut self, listening: Listening) {
        self.actions.quiet();
        if self.devices.escaped() {
            self.rebound = Some(Rebound::Cancelled);
            return;
        }
        let Some(path) = self.devices.newly_pressed(listening.scheme) else {
            self.listening = Some(listening);
            return;
        };
        match self.actions.rebind(listening.action.0, listening.binding, listening.part.as_deref(), &path) {
            Ok(()) => self.rebound = Some(Rebound::To(path)),
            Err(error) => {
                tracing::warn!("could not rebind: {error:#}");
                self.rebound = Some(Rebound::Cancelled);
            }
        }
    }

    /// An action by its name, `Jump`, or with its map, `Player/Jump`.
    pub fn action(&self, name: &str) -> Option<ActionId> {
        self.actions.find(name).map(ActionId)
    }

    /// An action's name and its map's.
    pub fn action_name(&self, action: ActionId) -> Option<(&str, &str)> {
        self.actions.list.get(action.0).map(|a| (a.map.as_str(), a.data.name.as_str()))
    }

    pub fn state(&self, action: ActionId) -> ActionState {
        self.actions.list.get(action.0).map(|a| a.state).unwrap_or_default()
    }

    /// Every map's name, in order.
    pub fn maps(&self) -> Vec<String> {
        self.actions.file.maps.iter().map(|map| map.name.clone()).collect()
    }

    /// Turns a map's actions on or off, as a game does for its menus.
    pub fn set_map_enabled(&mut self, map: &str, on: bool) {
        self.actions.off.retain(|off| off != map);
        if !on {
            self.actions.off.push(map.to_string());
        }
    }

    pub fn map_enabled(&self, map: &str) -> bool {
        !self.actions.off.iter().any(|off| off == map)
    }

    pub fn devices(&self) -> &Devices {
        &self.devices
    }

    pub fn devices_mut(&mut self) -> &mut Devices {
        &mut self.devices
    }

    /// The scheme of the device used last.
    pub fn scheme(&self) -> Scheme {
        self.devices.last
    }

    /// The categories key maps colour actions by, in order.
    pub fn categories(&self) -> &[CategoryData] {
        &self.actions.file.categories
    }

    /// Every control every action's bindings read, as they are now, rebound
    /// or not: one use a binding, or a part of a composite one, in the
    /// order of the actions.
    pub fn control_uses(&self) -> Vec<ControlUse> {
        let mut uses = Vec::new();
        for (at, action) in self.actions.list.iter().enumerate() {
            for (binding, bound) in action.data.bindings.iter().enumerate() {
                for (part, path) in bound.controls() {
                    uses.push(ControlUse {
                        path,
                        action: ActionId(at),
                        map: action.map.clone(),
                        name: action.data.name.clone(),
                        binding,
                        part,
                        category: action.data.category.clone(),
                        hidden: action.data.hidden,
                    });
                }
            }
        }
        uses
    }

    /// An action's bindings as they are now.
    pub fn bindings(&self, action: ActionId) -> &[Binding] {
        self.actions.list.get(action.0).map_or(&[], |a| a.data.bindings.as_slice())
    }

    /// Points a binding of an action, or one part of a composite binding,
    /// at a control by its path.
    pub fn rebind(&mut self, action: ActionId, binding: usize, part: Option<&str>, path: &str) -> Result<()> {
        self.actions.rebind(action.0, binding, part, path)
    }

    /// Listens for the next control pressed, of `scheme` when given, to
    /// bind it in place of a binding of an action, or one part of it.
    pub fn listen(&mut self, action: ActionId, binding: usize, part: Option<&str>, scheme: Option<Scheme>) {
        self.rebound = None;
        self.listening = Some(Listening { action, binding, part: part.map(str::to_string), scheme });
    }

    pub fn is_listening(&self) -> bool {
        self.listening.is_some()
    }

    /// What the last listening came to, once it has.
    pub fn take_rebound(&mut self) -> Option<Rebound> {
        self.rebound.take()
    }

    /// The same, left to take.
    pub fn peek_rebound(&self) -> Option<&Rebound> {
        self.rebound.as_ref()
    }

    /// Gives up listening, changing nothing.
    pub fn cancel_listening(&mut self) {
        if self.listening.take().is_some() {
            self.rebound = Some(Rebound::Cancelled);
        }
    }

    /// The other actions of an action's map that a control is bound to.
    pub fn conflicts(&self, action: ActionId, path: &str) -> Vec<String> {
        self.actions.conflicts(action.0, path)
    }

    /// Every action's bindings as they are now, as text to save a player's
    /// rebinding in.
    pub fn saved_bindings(&self) -> String {
        let bindings: std::collections::BTreeMap<String, Vec<Binding>> = self.actions.bindings().into_iter().collect();
        toml::to_string(&bindings).unwrap_or_default()
    }

    /// Reads back what [`saved_bindings`](Self::saved_bindings) wrote. An
    /// action the text does not name keeps its bindings, and one it names
    /// that the game no longer has is passed over.
    pub fn load_bindings(&mut self, text: &str) -> Result<()> {
        let saved: std::collections::BTreeMap<String, Vec<Binding>> =
            toml::from_str(text).context("reading saved bindings")?;
        for (name, bindings) in saved {
            let Some(found) = self.actions.find(&name) else { continue };
            let action = &mut self.actions.list[found];
            action.data.bindings = bindings;
            action.read_processors()?;
        }
        Ok(())
    }

    /// Puts every binding back as the actions' file has it.
    pub fn reset_bindings(&mut self) -> Result<()> {
        self.actions = actions::Actions::of(self.actions.file.clone())?;
        Ok(())
    }

    /// What to press for an action in the scheme used last, as a prompt
    /// shows it: `Space`, `W A S D`, or a pad's `A`. Empty when it has no
    /// binding in that scheme.
    pub fn prompt(&self, action: ActionId) -> String {
        self.prompt_for(action, self.scheme())
    }

    pub fn prompt_for(&self, action: ActionId, scheme: Scheme) -> String {
        let Some(binding) = self.bindings(action).iter().find(|b| b.scheme() == scheme) else { return String::new() };
        let labels: Vec<String> = binding.controls().iter().map(|(_, path)| self.control_label(path)).collect();
        labels.join(" ")
    }

    /// A control as prompts name it.
    pub fn control_label(&self, path: &str) -> String {
        let Some((device, control)) = path.split_once('/') else { return path.to_string() };
        match device {
            "Keyboard" => self.devices.key_label(control),
            "Mouse" => match control {
                "Left" | "Right" | "Middle" => format!("{control} Click"),
                "Delta" | "Position" => "Mouse".to_string(),
                _ => control.to_string(),
            },
            "Gamepad" => pad_label(self.devices.family(), control),
            _ => path.to_string(),
        }
    }
}

/// A pad's control as its family names it.
fn pad_label(family: Family, control: &str) -> String {
    let face = |xbox: &'static str, play: &'static str, switch: &'static str| match family {
        Family::PlayStation => play,
        Family::Switch => switch,
        Family::Xbox | Family::Steam => xbox,
    };
    let label = match control {
        "South" => face("A", "Cross", "B"),
        "East" => face("B", "Circle", "A"),
        "West" => face("X", "Square", "Y"),
        "North" => face("Y", "Triangle", "X"),
        "LeftShoulder" => face("LB", "L1", "L"),
        "RightShoulder" => face("RB", "R1", "R"),
        "LeftTrigger" => face("LT", "L2", "ZL"),
        "RightTrigger" => face("RT", "R2", "ZR"),
        "Select" => face("View", "Create", "-"),
        "Start" => face("Menu", "Options", "+"),
        "LeftStickPress" => face("LS", "L3", "Left Stick"),
        "RightStickPress" => face("RS", "R3", "Right Stick"),
        "LeftStick" => "Left Stick",
        "RightStick" => "Right Stick",
        "DpadUp" => "D-pad Up",
        "DpadDown" => "D-pad Down",
        "DpadLeft" => "D-pad Left",
        "DpadRight" => "D-pad Right",
        other => return other.to_string(),
    };
    label.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_say_which_actions_they_serve_with_their_categories() {
        let mut input = Input::without_gamepads();
        let file = "format = 1\n\n[[category]]\nname = \"Movement\"\ncolor = \"#3fb950\"\n\n[[map]]\nname = \"Player\"\n\n\
                    [[map.action]]\nname = \"Move\"\ntype = \"vector2\"\ncategory = \"Movement\"\n\
                    bindings = [{ up = \"Keyboard/W\", down = \"Keyboard/S\" }, \"Gamepad/LeftStick\"]\n\n\
                    [[map.action]]\nname = \"Cheat\"\nhidden = true\nbindings = [\"Keyboard/W\"]\n";
        input.load_actions(file).unwrap();
        assert_eq!(input.categories()[0].color, "#3fb950");
        let on_w: Vec<String> = input.control_uses().into_iter().filter(|u| u.path == "Keyboard/W").map(|u| u.label()).collect();
        assert_eq!(on_w, ["Move (Up)", "Cheat"]);
        let uses = input.control_uses();
        let cheat = uses.iter().find(|u| u.name == "Cheat").unwrap();
        assert!(cheat.hidden && cheat.category.is_none());
        let stick = uses.iter().find(|u| u.path == "Gamepad/LeftStick").unwrap();
        assert_eq!((stick.binding, stick.category.as_deref()), (1, Some("Movement")));
        assert_eq!(layouts::gamepad().control_of(&stick.path).map(|c| c.label.as_str()), Some("LS"));
    }

    #[test]
    fn the_default_actions_read_and_prompt_in_the_scheme_used_last() {
        let mut input = Input::without_gamepads();
        let jump = input.action("Jump").expect("the default actions have Jump");
        input.devices_mut().set_key("Space", true);
        input.update(0.016);
        assert!(input.state(jump).pressed);
        assert_eq!(input.prompt(jump), "Space");
        let moves = input.action("Player/Move").unwrap();
        assert_eq!(input.prompt(moves), "W A S D", "up, left, down, right, as players read them");

        let mut pad = Pad { family: Family::PlayStation, ..Pad::default() };
        pad.buttons.insert("South", 1.0);
        input.devices_mut().set_pad(0, pad);
        input.update(0.016);
        assert_eq!(input.scheme(), Scheme::Gamepad);
        assert_eq!(input.prompt(jump), "Cross", "a PlayStation pad's names");
    }

    #[test]
    fn listening_rebinds_to_the_next_control_and_bindings_save_as_text() {
        let mut input = Input::without_gamepads();
        let jump = input.action("Jump").unwrap();
        input.listen(jump, 0, None, Some(Scheme::KeyboardMouse));
        input.update(0.016);
        assert!(input.is_listening() && input.take_rebound().is_none(), "nothing pressed yet");
        input.devices_mut().set_key("J", true);
        input.update(0.016);
        assert_eq!(input.take_rebound(), Some(Rebound::To("Keyboard/J".into())));
        assert!(!input.state(jump).pressed, "the key pressed to rebind does not also jump");
        input.devices_mut().set_key("J", false);
        input.update(0.016);
        input.devices_mut().set_key("J", true);
        input.update(0.016);
        assert!(input.state(jump).pressed, "J jumps now");

        let saved = input.saved_bindings();
        let mut other = Input::without_gamepads();
        other.load_bindings(&saved).unwrap();
        assert_eq!(other.bindings(jump)[0], Binding::Path("Keyboard/J".into()));
        other.reset_bindings().unwrap();
        assert_eq!(other.bindings(jump)[0], Binding::Path("Keyboard/Space".into()));

        input.listen(jump, 0, None, None);
        input.devices_mut().set_key("Escape", true);
        input.update(0.016);
        assert_eq!(input.take_rebound(), Some(Rebound::Cancelled));
        assert_eq!(input.bindings(jump)[0], Binding::Path("Keyboard/J".into()), "Escape changes nothing");
    }
}
