//! Actions, in maps, as a `.inputactions` file writes them, and what each is
//! this frame:
//!
//! ```toml
//! format = 1
//!
//! [[map]]
//! name = "Player"
//!
//! [[map.action]]
//! name = "Move"
//! type = "vector2"
//! bindings = [
//!     { up = "Keyboard/W", down = "Keyboard/S", left = "Keyboard/A", right = "Keyboard/D" },
//!     "Gamepad/LeftStick",
//! ]
//!
//! [[map.action]]
//! name = "Jump"
//! bindings = ["Keyboard/Space", "Gamepad/South"]
//!
//! [[map.action]]
//! name = "Charge"
//! interaction = "hold(0.6)"
//! bindings = ["Mouse/Right"]
//! ```
//!
//! - An action is a `button` (the default), a `value` from -1 to 1, or a
//!   `vector2`.
//! - A binding is a control's path, or a composite of several: `up`,
//!   `down`, `left` and `right` make a vector, as WASD does, and
//!   `negative` and `positive` a value. Its `processors` change what it
//!   reads: `deadzone(min, max)`, `invert`, `invert_x`, `invert_y` and
//!   `scale(factor)`. A stick has a deadzone of 0.125 unless it says.
//! - Of its bindings, the one pushed furthest wins.
//! - Its `interaction` says when it is performed: when pressed (the
//!   default), `hold(seconds)` once held that long, or `tap(seconds)` when
//!   let go within that long.
//! - Its `category` groups it for key maps, which colour its keys by the
//!   category's `color`, and `hidden = true` keeps it out of them, as for a
//!   debug key:
//!
//! ```toml
//! [[category]]
//! name = "Movement"
//! color = "#3fb950"
//!
//! [[map.action]]
//! name = "Jump"
//! category = "Movement"
//! icon = "arrow-up"
//! bindings = ["Keyboard/Space"]
//! ```
//!
//! - Its `icon`, one of Voxear's icons by name, marks it in key maps and in
//!   the Input Actions window.
//! - A map's `excludes` names the maps that cannot be on while it is, as a
//!   game's Player and UI maps are not on together: turning it on turns
//!   them off, and of two that exclude each other only the first in the file
//!   starts on. Either naming the other is enough.
//! - The same control bound to two actions conflicts when both can be on
//!   at once: in one map, or in two that do not exclude each other.
//!
//! ```toml
//! [[map]]
//! name = "Player"
//! excludes = ["UI"]
//! ```

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::devices::{Device, Devices, Scheme};

pub const ACTIONS_FORMAT: u32 = 1;

/// How far a button's value goes before it counts as pressed.
const PRESS_POINT: f32 = 0.5;
/// A stick's deadzone, unless a binding says otherwise.
const STICK_DEADZONE: (f32, f32) = (0.125, 0.925);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsFile {
    pub format: u32,
    /// The Prompt Icons prompts draw controls with, by GUID or project
    /// path; none for the glyphs Voxear draws.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_icons: Option<String>,
    #[serde(default, rename = "category", skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoryData>,
    #[serde(default, rename = "map")]
    pub maps: Vec<MapData>,
}

impl ActionsFile {
    /// The file as `.inputactions` writes it, as the editor's Input Actions
    /// window saves it: each binding on its own line.
    pub fn to_text(&self) -> Result<String> {
        let mut document: toml_edit::DocumentMut =
            toml::to_string(self).context("writing the input actions")?.parse().context("reading them back")?;
        let maps = tables(document.get_mut("map")).into_iter().flat_map(|maps| maps.iter_mut());
        for action in maps.flat_map(|map| tables(map.get_mut("action")).into_iter().flat_map(|a| a.iter_mut())) {
            let Some(bindings) = action.get_mut("bindings").and_then(|b| b.as_array_mut()) else { continue };
            for binding in bindings.iter_mut() {
                binding.decor_mut().set_prefix("\n    ");
            }
            bindings.set_trailing("\n");
            bindings.set_trailing_comma(true);
        }
        Ok(document.to_string())
    }

    /// Checks the file as the game would read it.
    pub fn check(&self) -> Result<()> {
        Actions::of(self.clone()).map(drop)
    }

    /// Whether two maps, by name, cannot be on together: either excludes
    /// the other.
    pub fn exclusive(&self, a: &str, b: &str) -> bool {
        let names = |map: &MapData, other: &str| map.excludes.iter().any(|name| name == other);
        a != b && self.maps.iter().any(|map| (map.name == a && names(map, b)) || (map.name == b && names(map, a)))
    }

    /// Every control bound in the file that conflicts: bound, or bound in
    /// part, to another action that can be on at once, in its own map or
    /// one that does not exclude it.
    pub fn conflicts(&self) -> Vec<BoundAt> {
        let mut bound = Vec::new();
        for (map_at, map) in self.maps.iter().enumerate() {
            for (action_at, action) in map.actions.iter().enumerate() {
                for (binding, found) in action.bindings.iter().enumerate() {
                    for (part, path) in found.controls() {
                        bound.push((
                            BoundAt { map: map_at, action: action_at, binding, part },
                            map.name.as_str(),
                            path,
                        ));
                    }
                }
            }
        }
        let clashes = |(a, a_map, a_path): &(BoundAt, &str, String), (b, b_map, b_path): &(BoundAt, &str, String)| {
            let same_action = a.map == b.map && a.action == b.action;
            !same_action && !self.exclusive(a_map, b_map) && overlaps(a_path, b_path)
        };
        bound.iter().filter(|one| bound.iter().any(|other| clashes(one, other))).map(|(at, ..)| *at).collect()
    }

    /// The maps that start off: of two that exclude each other, the later.
    pub fn starting_off(&self) -> Vec<String> {
        let mut off: Vec<String> = Vec::new();
        for (at, map) in self.maps.iter().enumerate() {
            let on_before = self.maps[..at].iter().filter(|before| !off.contains(&before.name));
            if on_before.clone().any(|before| self.exclusive(&before.name, &map.name)) {
                off.push(map.name.clone());
            }
        }
        off
    }
}

fn tables(item: Option<&mut toml_edit::Item>) -> Option<&mut toml_edit::ArrayOfTables> {
    item.and_then(|item| item.as_array_of_tables_mut())
}

/// A group of actions, as key maps colour them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryData {
    pub name: String,
    /// `#rrggbb`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapData {
    pub name: String,
    /// The maps that cannot be on while this one is, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
    #[serde(default, rename = "action")]
    pub actions: Vec<ActionData>,
}

/// Where one control is bound in a file: the map and action by their
/// places, the binding by its place in the action, and its part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundAt {
    pub map: usize,
    pub action: usize,
    pub binding: usize,
    pub part: Option<&'static str>,
}

/// Whether two controls' paths read the same thing, wholly or in part: the
/// same control, or a stick and an axis or half of it, or an axis and its
/// halves. A stick's press is a button of its own.
pub fn overlaps(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let stick = |path: &str| -> Option<(String, String)> {
        let at = path.find("Stick")? + "Stick".len();
        let rest = &path[at..];
        (rest != "Press").then(|| (path[..at].to_string(), rest.to_string()))
    };
    let (Some((stick_a, part_a)), Some((stick_b, part_b))) = (stick(a), stick(b)) else { return false };
    let within = |axis: &str, half: &str| match axis {
        "X" => half == "Left" || half == "Right",
        "Y" => half == "Up" || half == "Down",
        _ => false,
    };
    stick_a == stick_b
        && (part_a.is_empty() || part_b.is_empty() || within(&part_a, &part_b) || within(&part_b, &part_a))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionData {
    pub name: String,
    #[serde(default, rename = "type", skip_serializing_if = "is_button")]
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<String>,
    /// The category key maps show it in, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Left out of key maps, as a debug key is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// The icon key maps and the Input Actions window mark it with, one of
    /// Voxear's by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    #[default]
    Button,
    Value,
    Vector2,
}

fn is_button(kind: &ActionKind) -> bool {
    *kind == ActionKind::Button
}

/// A binding: a control's path, or a composite of controls, with what
/// changes what it reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Path(String),
    Full(FullBinding),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub processors: Vec<String>,
}

/// The parts of a composite binding, in the order prompts list them.
pub const PARTS: [&str; 6] = ["up", "left", "down", "right", "negative", "positive"];

impl Binding {
    fn full(&self) -> FullBinding {
        match self {
            Self::Path(path) => FullBinding { path: Some(path.clone()), ..FullBinding::default() },
            Self::Full(full) => full.clone(),
        }
    }

    /// Each control it reads, by its part (`None` for a binding of one),
    /// in the order prompts list them.
    pub fn controls(&self) -> Vec<(Option<&'static str>, String)> {
        let full = self.full();
        let mut found: Vec<(Option<&'static str>, String)> = full.path.clone().map(|path| (None, path)).into_iter().collect();
        for part in PARTS {
            if let Some(path) = full.part(part) {
                found.push((Some(part), path.clone()));
            }
        }
        found
    }

    /// The scheme its controls are for, by its first.
    pub fn scheme(&self) -> Scheme {
        self.controls().first().map_or(Scheme::KeyboardMouse, |(_, path)| Scheme::of(path))
    }
}

impl FullBinding {
    pub fn part(&self, part: &str) -> Option<&String> {
        match part {
            "up" => self.up.as_ref(),
            "down" => self.down.as_ref(),
            "left" => self.left.as_ref(),
            "right" => self.right.as_ref(),
            "negative" => self.negative.as_ref(),
            "positive" => self.positive.as_ref(),
            _ => self.path.as_ref(),
        }
    }

    fn set_part(&mut self, part: Option<&str>, path: String) {
        let slot = match part {
            Some("up") => &mut self.up,
            Some("down") => &mut self.down,
            Some("left") => &mut self.left,
            Some("right") => &mut self.right,
            Some("negative") => &mut self.negative,
            Some("positive") => &mut self.positive,
            _ => &mut self.path,
        };
        *slot = Some(path);
    }
}

/// When an action is performed, beyond being pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interaction {
    Press,
    /// Once held this many seconds.
    Hold(f32),
    /// When let go within this many seconds.
    Tap(f32),
}

impl Interaction {
    fn parse(text: Option<&str>) -> Result<Self> {
        let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else { return Ok(Self::Press) };
        let (name, args) = call(text)?;
        let seconds = |default: f32| args.first().copied().unwrap_or(default);
        Ok(match name {
            "press" => Self::Press,
            "hold" => Self::Hold(seconds(0.4)),
            "tap" => Self::Tap(seconds(0.2)),
            _ => anyhow::bail!("`{text}` is not an interaction: press, hold(seconds) or tap(seconds)"),
        })
    }
}

/// `name(1, 2)` as its name and numbers; `name` alone has none.
fn call(text: &str) -> Result<(&str, Vec<f32>)> {
    let Some((name, rest)) = text.split_once('(') else { return Ok((text.trim(), Vec::new())) };
    let args = rest.strip_suffix(')').with_context(|| format!("`{text}` has no closing bracket"))?;
    let numbers = args.split(',').filter(|a| !a.trim().is_empty()).map(|a| a.trim().parse::<f32>());
    let numbers = numbers.collect::<Result<Vec<_>, _>>().with_context(|| format!("`{text}` takes numbers"))?;
    Ok((name.trim(), numbers))
}

/// What changes a binding's value, in order, as the Input Actions window
/// picks it from a list and gives its values: a deadzone between `min` and
/// `max`, inverting either way or both, or scaling by a factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Processor {
    Deadzone(f32, f32),
    /// Inverting x, y or both.
    Invert(bool, bool),
    Scale(f32),
}

impl Processor {
    /// Each kind of processor, as a list names them, with its values at
    /// their defaults.
    pub const KINDS: [(&'static str, Self); 5] = [
        ("Deadzone", Self::Deadzone(STICK_DEADZONE.0, STICK_DEADZONE.1)),
        ("Invert", Self::Invert(true, true)),
        ("Invert X", Self::Invert(true, false)),
        ("Invert Y", Self::Invert(false, true)),
        ("Scale", Self::Scale(1.0)),
    ];

    /// Which of [`KINDS`](Self::KINDS) it is.
    pub fn kind(self) -> usize {
        match self {
            Self::Deadzone(..) => 0,
            Self::Invert(true, true) | Self::Invert(false, false) => 1,
            Self::Invert(true, false) => 2,
            Self::Invert(false, true) => 3,
            Self::Scale(_) => 4,
        }
    }

    /// Its values, by name, as fields show them: a deadzone's Min and Max,
    /// a scale's Factor.
    pub fn values(self) -> Vec<(&'static str, f32)> {
        match self {
            Self::Deadzone(min, max) => vec![("Min", min), ("Max", max)],
            Self::Invert(..) => Vec::new(),
            Self::Scale(by) => vec![("Factor", by)],
        }
    }

    /// The same kind with its values set, in the order [`values`](Self::values) gives them.
    pub fn with_values(self, values: &[f32]) -> Self {
        let at = |i: usize, kept: f32| values.get(i).copied().filter(|v| v.is_finite()).unwrap_or(kept);
        match self {
            Self::Deadzone(min, max) => Self::Deadzone(at(0, min), at(1, max)),
            Self::Invert(x, y) => Self::Invert(x, y),
            Self::Scale(by) => Self::Scale(at(0, by)),
        }
    }

    /// As `.inputactions` writes it: `deadzone(0.1, 0.9)`, `invert_y`.
    pub fn to_text(self) -> String {
        match self {
            Self::Deadzone(min, max) => format!("deadzone({min}, {max})"),
            Self::Invert(true, false) => "invert_x".into(),
            Self::Invert(false, true) => "invert_y".into(),
            Self::Invert(..) => "invert".into(),
            Self::Scale(by) => format!("scale({by})"),
        }
    }

    /// Reads one as `.inputactions` writes it.
    pub fn parse(text: &str) -> Result<Self> {
        let (name, args) = call(text)?;
        let arg = |at: usize, default: f32| args.get(at).copied().unwrap_or(default);
        Ok(match name {
            "deadzone" => Self::Deadzone(arg(0, STICK_DEADZONE.0), arg(1, STICK_DEADZONE.1)),
            "invert" => Self::Invert(true, true),
            "invert_x" => Self::Invert(true, false),
            "invert_y" => Self::Invert(false, true),
            "scale" => Self::Scale(arg(0, 1.0)),
            _ => anyhow::bail!("`{text}` is not a processor: deadzone, invert, invert_x, invert_y or scale"),
        })
    }

    fn apply(self, [x, y]: [f32; 2]) -> [f32; 2] {
        match self {
            Self::Deadzone(min, max) => {
                let length = x.hypot(y);
                if length <= min {
                    return [0.0; 2];
                }
                let scaled = ((length - min) / (max - min).max(1e-4)).min(1.0);
                [x / length * scaled, y / length * scaled]
            }
            Self::Invert(ix, iy) => [if ix { -x } else { x }, if iy { -y } else { y }],
            Self::Scale(by) => [x * by, y * by],
        }
    }
}

/// An action as it is this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActionState {
    /// Its value: a button's from 0 to 1, a value's in `x`, a vector's.
    pub value: [f32; 2],
    /// Whether it is held: pushed past the press point.
    pub held: bool,
    /// Whether it went down, or up, this frame.
    pub pressed: bool,
    pub released: bool,
    /// Whether its interaction performed it this frame.
    pub performed: bool,
    /// How long it has been held, in seconds.
    pub held_for: f32,
}

/// One action, ready to read: its bindings, with any rebound, and what it
/// was the frame before.
#[derive(Clone, Debug)]
pub(crate) struct Action {
    pub map: String,
    pub data: ActionData,
    interaction: Interaction,
    processors: Vec<Vec<Processor>>,
    pub state: ActionState,
    /// A hold performs once until let go.
    done: bool,
}

impl Action {
    fn new(map: &str, data: ActionData) -> Result<Self> {
        let interaction = Interaction::parse(data.interaction.as_deref())
            .with_context(|| format!("{map}/{}", data.name))?;
        let mut action =
            Self { map: map.to_string(), data, interaction, processors: Vec::new(), state: ActionState::default(), done: false };
        action.read_processors()?;
        Ok(action)
    }

    pub(crate) fn read_processors(&mut self) -> Result<()> {
        self.processors = (self.data.bindings.iter())
            .map(|binding| binding.full().processors.iter().map(|text| Processor::parse(text)).collect())
            .collect::<Result<_>>()
            .with_context(|| format!("{}/{}", self.map, self.data.name))?;
        Ok(())
    }

    /// Reads its bindings from the devices, and moves its phases on by
    /// `dt` seconds.
    fn update(&mut self, devices: &Devices, only: Option<Device>, dt: f32) {
        let mut best = [0.0f32; 2];
        for (binding, processors) in self.data.bindings.iter().zip(&self.processors) {
            let mut value = read(binding, devices, only);
            for processor in processors {
                value = processor.apply(value);
            }
            if magnitude(value) > magnitude(best) {
                best = value;
            }
        }
        let value = match self.data.kind {
            ActionKind::Button => [magnitude(best).min(1.0), 0.0],
            ActionKind::Value => [best[0], 0.0],
            ActionKind::Vector2 => best,
        };
        let held = magnitude(value) >= PRESS_POINT || (self.data.kind != ActionKind::Button && magnitude(value) > 0.0);
        let was = self.state;
        let held_for = if held { was.held_for + dt } else { 0.0 };
        let (pressed, released) = (held && !was.held, !held && was.held);
        let performed = match self.interaction {
            Interaction::Press if self.data.kind == ActionKind::Button => pressed,
            Interaction::Press => held && (pressed || value != was.value),
            Interaction::Hold(seconds) => held && !self.done && held_for >= seconds,
            Interaction::Tap(seconds) => released && was.held_for <= seconds,
        };
        if performed {
            self.done = true;
        }
        if !held {
            self.done = false;
        }
        self.state = ActionState { value, held, pressed, released, performed, held_for };
    }
}

/// A binding's value from the devices: a control's, or a composite's.
fn read(binding: &Binding, devices: &Devices, only: Option<Device>) -> [f32; 2] {
    let full = binding.full();
    let one =
        |path: &Option<String>| path.as_deref().and_then(|path| devices.read_from(path, only)).map_or(0.0, |v| v[0]);
    if let Some(path) = &full.path {
        let value = devices.read_from(path, only).unwrap_or_default();
        // A stick gets its deadzone, unless the binding gives one.
        let stick = path.ends_with("Stick") && !full.processors.iter().any(|p| p.starts_with("deadzone"));
        return if stick { Processor::Deadzone(STICK_DEADZONE.0, STICK_DEADZONE.1).apply(value) } else { value };
    }
    if full.up.is_some() || full.down.is_some() || full.left.is_some() || full.right.is_some() {
        let [x, y] = [one(&full.right) - one(&full.left), one(&full.up) - one(&full.down)];
        let length = x.hypot(y);
        // Diagonals are as long as straight lines, as with a stick.
        return if length > 1.0 { [x / length, y / length] } else { [x, y] };
    }
    [one(&full.positive) - one(&full.negative), 0.0]
}

fn magnitude([x, y]: [f32; 2]) -> f32 {
    x.hypot(y)
}

/// Every map's actions, ready to read, with what was rebound.
#[derive(Clone, Debug, Default)]
pub(crate) struct Actions {
    pub file: ActionsFile,
    pub list: Vec<Action>,
    /// Maps turned off, whose actions read nothing.
    pub off: Vec<String>,
}

impl Actions {
    pub fn parse(text: &str) -> Result<Self> {
        let file: ActionsFile = toml::from_str(text).context("reading the input actions")?;
        ensure!(file.format <= ACTIONS_FORMAT, "input actions of format {} are newer than this Voxear", file.format);
        Self::of(file)
    }

    pub fn of(file: ActionsFile) -> Result<Self> {
        let mut list = Vec::new();
        for map in &file.maps {
            for action in &map.actions {
                list.push(Action::new(&map.name, action.clone())?);
            }
        }
        let off = file.starting_off();
        Ok(Self { file, list, off })
    }

    /// An action by its name, `Jump`, or with its map, `Player/Jump`.
    pub fn find(&self, name: &str) -> Option<usize> {
        let (map, action) = name.split_once('/').map_or((None, name), |(map, action)| (Some(map), action));
        self.list.iter().position(|a| a.data.name == action && map.is_none_or(|map| a.map == map))
    }

    /// Reads every action from the devices, or from one only, as a
    /// player's are.
    pub fn update(&mut self, devices: &Devices, only: Option<Device>, dt: f32) {
        for action in &mut self.list {
            if self.off.contains(&action.map) {
                action.state = ActionState::default();
                continue;
            }
            action.update(devices, only, dt);
        }
    }

    /// An action's state this frame.
    pub fn state(&self, action: usize) -> ActionState {
        self.list.get(action).map(|a| a.state).unwrap_or_default()
    }

    /// Lets go of every action at once, as while listening for a rebind.
    pub fn quiet(&mut self) {
        self.list.iter_mut().for_each(|action| action.state = ActionState::default());
    }

    /// Points a binding, or a part of a composite one, at another control.
    pub fn rebind(&mut self, action: usize, binding: usize, part: Option<&str>, path: &str) -> Result<()> {
        let action = self.list.get_mut(action).context("no such action")?;
        let found = action.data.bindings.get_mut(binding).context("no such binding")?;
        let mut full = found.full();
        full.set_part(part, path.to_string());
        *found = match full == (FullBinding { path: full.path.clone(), ..FullBinding::default() }) {
            true => Binding::Path(full.path.unwrap_or_default()),
            false => Binding::Full(full),
        };
        action.read_processors()
    }

    /// The other actions that a control is bound to, wholly or in part,
    /// that can be on at once with this one, by place, as a rebinding
    /// screen warns of: those of its own map, and of maps that do not
    /// exclude it.
    pub fn conflicts(&self, action: usize, path: &str) -> Vec<usize> {
        let Some(this) = self.list.get(action) else { return Vec::new() };
        (self.list.iter().enumerate())
            .filter(|(at, other)| *at != action && !self.file.exclusive(&this.map, &other.map))
            .filter(|(_, other)| {
                other.data.bindings.iter().any(|b| b.controls().iter().any(|(_, p)| overlaps(p, path)))
            })
            .map(|(at, _)| at)
            .collect()
    }

    /// Turns a map on, and those it cannot be on with off; or turns it off.
    pub fn set_enabled(&mut self, map: &str, on: bool) {
        self.off.retain(|off| off != map);
        if !on {
            self.off.push(map.to_string());
            return;
        }
        let excluded: Vec<String> =
            self.file.maps.iter().filter(|m| self.file.exclusive(map, &m.name)).map(|m| m.name.clone()).collect();
        for name in excluded {
            if !self.off.contains(&name) {
                self.off.push(name);
            }
        }
    }

    /// Each action's bindings as they are now, rebound or not, by map and
    /// name, as a player's bindings are saved.
    pub fn bindings(&self) -> HashMap<String, Vec<Binding>> {
        self.list.iter().map(|a| (format!("{}/{}", a.map, a.data.name), a.data.bindings.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_written_out_reads_back_the_same() {
        let text = "format = 1\n[[category]]\nname = \"Movement\"\ncolor = \"#3fb950\"\n[[map]]\nname = \"Player\"\n\
                    [[map.action]]\nname = \"Move\"\ntype = \"vector2\"\ncategory = \"Movement\"\nbindings = [\
                    { up = \"Keyboard/W\", down = \"Keyboard/S\", left = \"Keyboard/A\", right = \"Keyboard/D\" }, \
                    \"Gamepad/LeftStick\"]\n[[map.action]]\nname = \"Jump\"\ninteraction = \"hold(0.5)\"\n\
                    bindings = [\"Keyboard/Space\"]\n";
        let file: ActionsFile = toml::from_str(text).unwrap();
        let written = file.to_text().unwrap();
        assert_eq!(toml::from_str::<ActionsFile>(&written).unwrap(), file, "{written}");
        assert!(written.contains("bindings = [\n    { up = \"Keyboard/W\""), "a binding a line: {written}");
        file.check().unwrap();
    }

    const FILE: &str = r#"
        format = 1

        [[map]]
        name = "Player"

        [[map.action]]
        name = "Move"
        type = "vector2"
        bindings = [{ up = "Keyboard/W", down = "Keyboard/S", left = "Keyboard/A", right = "Keyboard/D" }, "Gamepad/LeftStick"]

        [[map.action]]
        name = "Jump"
        bindings = ["Keyboard/Space", "Gamepad/South"]

        [[map.action]]
        name = "Charge"
        interaction = "hold(0.5)"
        bindings = ["Keyboard/E"]
    "#;

    #[test]
    fn composites_sticks_and_holds_read_as_actions() {
        let mut actions = Actions::parse(FILE).unwrap();
        let mut devices = Devices::default();
        let (moves, jump, charge) =
            (actions.find("Move").unwrap(), actions.find("Player/Jump").unwrap(), actions.find("Charge").unwrap());
        devices.set_key("W", true);
        devices.set_key("D", true);
        devices.set_key("Space", true);
        actions.update(&devices, None, 0.016);
        let [x, y] = actions.list[moves].state.value;
        assert!((x - y).abs() < 1e-6 && (x.hypot(y) - 1.0).abs() < 1e-5, "a diagonal is as long as a straight line");
        let state = actions.list[jump].state;
        assert!(state.held && state.pressed && state.performed);
        actions.update(&devices, None, 0.016);
        assert!(!actions.list[jump].state.pressed, "pressed only the frame it went down");

        devices.set_key("E", true);
        actions.update(&devices, None, 0.3);
        assert!(!actions.list[charge].state.performed, "not held long enough");
        actions.update(&devices, None, 0.3);
        assert!(actions.list[charge].state.performed, "held past half a second");
        actions.update(&devices, None, 0.3);
        assert!(!actions.list[charge].state.performed, "performed once until let go");

        let mut pad = crate::devices::Pad::default();
        pad.axes.insert("LeftStickX", 0.05);
        devices.set_key("W", false);
        devices.set_key("D", false);
        devices.set_pad(0, pad);
        actions.update(&devices, None, 0.016);
        assert_eq!(actions.list[moves].state.value, [0.0, 0.0], "inside the stick's deadzone");
    }

    #[test]
    fn a_binding_is_rebound_and_its_conflicts_found() {
        let mut actions = Actions::parse(FILE).unwrap();
        let jump = actions.find("Jump").unwrap();
        actions.rebind(jump, 0, None, "Keyboard/E").unwrap();
        assert_eq!(actions.list[jump].data.bindings[0], Binding::Path("Keyboard/E".into()));
        assert_eq!(actions.conflicts(jump, "Keyboard/E"), [actions.find("Charge").unwrap()]);
        let moves = actions.find("Move").unwrap();
        actions.rebind(moves, 0, Some("up"), "Keyboard/Z").unwrap();
        let Binding::Full(full) = &actions.list[moves].data.bindings[0] else { panic!("still a composite") };
        assert_eq!(full.up.as_deref(), Some("Keyboard/Z"));
        assert!(Processor::parse("deadzone(0.2)").is_ok() && Processor::parse("wobble").is_err());
        for (_, kind) in Processor::KINDS {
            assert_eq!(Processor::parse(&kind.to_text()).unwrap(), kind, "{kind:?} reads back");
        }
        assert_eq!(Processor::Scale(1.0).with_values(&[2.5]).to_text(), "scale(2.5)");
        assert!(Interaction::parse(Some("hold(1)")).is_ok() && Interaction::parse(Some("spin")).is_err());
    }

    const MAPS: &str = r#"
        format = 1

        [[map]]
        name = "Player"
        excludes = ["Menu"]

        [[map.action]]
        name = "Move"
        type = "vector2"
        bindings = ["Gamepad/LeftStick"]

        [[map.action]]
        name = "Sprint"
        bindings = ["Gamepad/LeftStickPress", "Keyboard/ShiftLeft"]

        [[map]]
        name = "Menu"

        [[map.action]]
        name = "Up"
        bindings = ["Gamepad/LeftStickUp"]

        [[map]]
        name = "Debug"

        [[map.action]]
        name = "Noclip"
        bindings = ["Keyboard/ShiftLeft"]
    "#;

    #[test]
    fn maps_that_exclude_each_other_are_not_on_together_nor_conflict() {
        let mut actions = Actions::parse(MAPS).unwrap();
        assert_eq!(actions.off, ["Menu"], "of two that exclude each other, the first starts on");
        actions.set_enabled("Menu", true);
        assert_eq!(actions.off, ["Player"], "turning one on turns the other off");

        let file = &actions.file;
        assert!(file.exclusive("Menu", "Player") && !file.exclusive("Player", "Debug"));
        let (sprint, noclip) = (actions.find("Sprint").unwrap(), actions.find("Noclip").unwrap());
        assert!(actions.conflicts(actions.find("Move").unwrap(), "Gamepad/LeftStick").is_empty(), "Up is in Menu");
        assert_eq!(actions.conflicts(sprint, "Keyboard/ShiftLeft"), [noclip], "Debug can be on with Player");

        let conflicts = file.conflicts();
        let shift =
            |map: usize, action: usize| BoundAt { map, action, binding: if map == 0 { 1 } else { 0 }, part: None };
        assert_eq!(conflicts, [shift(0, 1), shift(2, 0)], "the stick and its press are apart");
        assert!(
            overlaps("Gamepad/RightStickX", "Gamepad/RightStickLeft")
                && !overlaps("Gamepad/LeftStickX", "Gamepad/LeftStickUp")
        );
    }
}
