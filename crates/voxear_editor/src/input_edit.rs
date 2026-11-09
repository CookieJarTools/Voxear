//! What the Input Actions window changes in a `.inputactions` file, apart
//! from how it shows it: a map's actions listed as rows, each action
//! followed by its bindings and a composite's parts, as Unity's Input
//! Actions window lists them, and each change the window makes, done to
//! the file's data. The window writes the file after each one.

use voxear_input::{
    ActionKind, ActionsFile, Binding, Processor,
    actions::{ActionData, BoundAt, FullBinding, MapData},
    controls,
};
use voxear_ui::ICONS;

/// One row of a map's actions, by its indices in the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Action(usize),
    Binding(usize, usize),
    /// A part of a composite binding, by its name: `up`, `negative` and so on.
    Part(usize, usize, &'static str),
}

impl Row {
    pub fn action(self) -> usize {
        match self {
            Self::Action(action) | Self::Binding(action, _) | Self::Part(action, ..) => action,
        }
    }
}

/// A composite's parts, in the order its rows list them.
const PART_ORDER: [&str; 6] = ["up", "down", "left", "right", "negative", "positive"];

/// The kinds of composite a binding can be: four directions making a
/// vector, as WASD does, or two making a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Composite {
    Vector,
    Axis,
}

impl Composite {
    pub fn parts(self) -> &'static [&'static str] {
        match self {
            Self::Vector => &["up", "down", "left", "right"],
            Self::Axis => &["negative", "positive"],
        }
    }
}

/// A map's rows: each action, then its bindings, a composite's followed by
/// its parts.
pub fn rows(map: &MapData) -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, action) in map.actions.iter().enumerate() {
        rows.push(Row::Action(index));
        for (at, binding) in action.bindings.iter().enumerate() {
            rows.push(Row::Binding(index, at));
            // In the order Unity lists them, rather than the order prompts read them.
            let has: Vec<&str> = binding.controls().into_iter().filter_map(|(part, _)| part).collect();
            let parts = PART_ORDER.iter().filter(|part| has.contains(part));
            rows.extend(parts.map(|&part| Row::Part(index, at, part)));
        }
    }
    rows
}

/// How a row shows: its icon, what it says, what it says after it more
/// quietly, as a control's device, and how far in it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub icon: &'static str,
    pub text: String,
    pub detail: String,
    pub depth: usize,
    /// Another action that can be on at once reads the same control.
    pub conflict: bool,
    /// A binding or part not given a control yet, shown as a placeholder.
    pub unset: bool,
    /// An action's own row, which has a plus to add a binding to it.
    pub action: bool,
}

/// How a row shows: an action with its icon, a binding's control by the
/// name people call it and its device, or a composite and its parts.
pub fn shown(map: &MapData, row: Row) -> Shown {
    let Some(action) = map.actions.get(row.action()) else { return Shown::default() };
    let binding = |at: usize| action.bindings.get(at);
    match row {
        Row::Action(_) => Shown { icon: action_icon(action), text: action.name.clone(), action: true, ..Shown::default() },
        Row::Binding(_, at) => match binding(at) {
            Some(Binding::Path(path)) => control_row(path, None),
            Some(Binding::Full(full)) if full.path.is_some() => control_row(full.path.as_deref().unwrap_or(""), None),
            Some(Binding::Full(full)) if full.up.is_some() || full.left.is_some() => {
                Shown { icon: "move", text: "2D Vector".into(), depth: 1, ..Shown::default() }
            }
            Some(Binding::Full(_)) => {
                Shown { icon: "arrow-right", text: "1D Axis".into(), depth: 1, ..Shown::default() }
            }
            None => Shown::default(),
        },
        Row::Part(_, at, part) => {
            let path = binding(at).and_then(|b| b.controls().into_iter().find(|(p, _)| *p == Some(part)));
            control_row(&path.map(|(_, path)| path).unwrap_or_default(), Some(part))
        }
    }
}

/// An action's icon: its own, or one for its type.
pub fn action_icon(action: &ActionData) -> &'static str {
    let own = action.icon.as_deref().and_then(|name| ICONS.iter().find(|&&icon| icon == name).copied());
    own.unwrap_or(match action.kind {
        ActionKind::Button => "play",
        ActionKind::Value => "arrow-right",
        ActionKind::Vector2 => "move",
    })
}

/// A binding's control, or a composite's part, as its row shows it.
fn control_row(path: &str, part: Option<&str>) -> Shown {
    let depth = if part.is_some() { 2 } else { 1 };
    let (name, device) = control_name(path);
    let text = match part {
        Some(part) => format!("{}: {name}", capital(part)),
        None => name,
    };
    Shown { icon: device_icon(path), text, detail: device, depth, conflict: false, unset: path.is_empty(), action: false }
}

/// What people call a control, and its device: "Left Shift" and
/// "Keyboard"; "None" for a binding or part not given one yet.
pub fn control_name(path: &str) -> (String, String) {
    if path.is_empty() {
        return ("None".into(), String::new());
    }
    let device = path.split_once('/').map_or("", |(device, _)| device).to_string();
    let name = controls::all().into_iter().find(|c| c.path == path).map_or_else(|| path.to_string(), |c| c.name);
    (name, device)
}

/// A device's icon, by a path of one of its controls.
pub fn device_icon(path: &str) -> &'static str {
    match path.split_once('/').map(|(device, _)| device) {
        Some("Keyboard") => "keyboard",
        Some("Mouse") => "mouse",
        Some("Gamepad") => "gamepad",
        // Not given a control yet.
        _ => "more",
    }
}

/// Which of a map's rows conflict: a control another action that can be on
/// at once reads too, an action any of whose controls does, a composite
/// any of whose parts does.
pub fn conflicts(file: &ActionsFile, map: usize) -> Vec<bool> {
    let found: Vec<BoundAt> = file.conflicts().into_iter().filter(|at| at.map == map).collect();
    let Some(data) = file.maps.get(map) else { return Vec::new() };
    rows(data)
        .into_iter()
        .map(|row| match row {
            Row::Action(action) => found.iter().any(|at| at.action == action),
            Row::Binding(action, binding) => found.iter().any(|at| at.action == action && at.binding == binding),
            Row::Part(action, binding, part) => {
                found.iter().any(|at| at.action == action && at.binding == binding && at.part == Some(part))
            }
        })
        .collect()
}

/// Makes two maps exclusive, so they are never on together, or not.
pub fn set_exclusive(file: &mut ActionsFile, map: usize, other: usize, on: bool) {
    let (Some(a), Some(b)) = (file.maps.get(map), file.maps.get(other)) else { return };
    let (a, b) = (a.name.clone(), b.name.clone());
    if map == other || file.exclusive(&a, &b) == on {
        return;
    }
    if on {
        file.maps[map].excludes.push(b);
        return;
    }
    file.maps[map].excludes.retain(|name| *name != b);
    file.maps[other].excludes.retain(|name| *name != a);
}

/// Takes a map away, and its name from the maps that exclude it.
pub fn remove_map(file: &mut ActionsFile, map: usize) {
    if map >= file.maps.len() {
        return;
    }
    let gone = file.maps.remove(map).name;
    file.maps.iter_mut().for_each(|m| m.excludes.retain(|name| *name != gone));
}

/// The control a binding row, or a part row, reads.
pub fn path(map: &MapData, row: Row) -> Option<String> {
    let action = map.actions.get(row.action())?;
    let (at, part) = match row {
        Row::Action(_) => return None,
        Row::Binding(_, at) => (at, None),
        Row::Part(_, at, part) => (at, Some(part)),
    };
    let controls = action.bindings.get(at)?.controls();
    controls.into_iter().find(|(p, _)| *p == part).map(|(_, path)| path)
}

/// Sets the control a binding row, or a part row, reads.
pub fn set_path(map: &mut MapData, row: Row, path: &str) {
    let (action, at, part) = match row {
        Row::Action(_) => return,
        Row::Binding(action, at) => (action, at, None),
        Row::Part(action, at, part) => (action, at, Some(part)),
    };
    let Some(binding) = map.actions.get_mut(action).and_then(|a| a.bindings.get_mut(at)) else { return };
    let mut full = full(binding);
    let slot = match part {
        Some("up") => &mut full.up,
        Some("down") => &mut full.down,
        Some("left") => &mut full.left,
        Some("right") => &mut full.right,
        Some("negative") => &mut full.negative,
        Some("positive") => &mut full.positive,
        _ => &mut full.path,
    };
    *slot = Some(path.to_string());
    *binding = simplest(full);
}

/// A binding's processors, read; any the file has that cannot be are left out.
pub fn typed_processors(map: &MapData, row: Row) -> Vec<Processor> {
    processors(map, row).iter().filter_map(|text| Processor::parse(text).ok()).collect()
}

/// A binding's processors, as the file writes them: `deadzone(0.1, 0.9)`.
pub fn processors(map: &MapData, row: Row) -> Vec<String> {
    let (Row::Binding(action, at) | Row::Part(action, at, _)) = row else { return Vec::new() };
    let binding = map.actions.get(action).and_then(|a| a.bindings.get(at));
    binding.map(|binding| full(binding).processors).unwrap_or_default()
}

pub fn set_processors(map: &mut MapData, row: Row, processors: Vec<String>) {
    let (Row::Binding(action, at) | Row::Part(action, at, _)) = row else { return };
    let Some(binding) = map.actions.get_mut(action).and_then(|a| a.bindings.get_mut(at)) else { return };
    let mut full = full(binding);
    full.processors = processors;
    *binding = simplest(full);
}

/// Adds a map, named so no other map is, and gives its index.
pub fn add_map(file: &mut ActionsFile) -> usize {
    let name = free_name(file.maps.iter().map(|m| m.name.as_str()), "New Map");
    file.maps.push(MapData { name, excludes: Vec::new(), actions: Vec::new() });
    file.maps.len() - 1
}

/// Adds a button action after the others, with no bindings, and gives its row.
pub fn add_action(map: &mut MapData) -> Row {
    let name = free_name(map.actions.iter().map(|a| a.name.as_str()), "New Action");
    map.actions.push(ActionData {
        name,
        id: voxear_assets::AssetGuid::new_random().to_string(),
        kind: ActionKind::Button,
        interaction: None,
        category: None,
        hidden: false,
        icon: None,
        bindings: Vec::new(),
    });
    Row::Action(map.actions.len() - 1)
}

/// Adds an empty binding to an action, or a composite with its parts, and
/// gives the row to fill in: the binding's, or a composite's first part.
pub fn add_binding(map: &mut MapData, action: usize, composite: Option<Composite>) -> Option<Row> {
    let bindings = &mut map.actions.get_mut(action)?.bindings;
    let Some(composite) = composite else {
        bindings.push(Binding::Path(String::new()));
        return Some(Row::Binding(action, bindings.len() - 1));
    };
    let mut full = FullBinding::default();
    for part in composite.parts() {
        let slot = match *part {
            "up" => &mut full.up,
            "down" => &mut full.down,
            "left" => &mut full.left,
            "right" => &mut full.right,
            "negative" => &mut full.negative,
            _ => &mut full.positive,
        };
        *slot = Some(String::new());
    }
    bindings.push(Binding::Full(full));
    Some(Row::Part(action, bindings.len() - 1, composite.parts()[0]))
}

/// Takes away what a row is: an action with its bindings, or a binding. A
/// part's row takes away its whole composite.
pub fn remove(map: &mut MapData, row: Row) {
    match row {
        Row::Action(action) if action < map.actions.len() => drop(map.actions.remove(action)),
        Row::Binding(action, at) | Row::Part(action, at, _) => {
            let Some(bindings) = map.actions.get_mut(action).map(|a| &mut a.bindings) else { return };
            if at < bindings.len() {
                bindings.remove(at);
            }
        }
        Row::Action(_) => {}
    }
}

/// Renames an action, unless another of the map has the name; true when it did.
pub fn rename_action(map: &mut MapData, action: usize, name: &str) -> bool {
    let name = name.trim();
    let taken = map.actions.iter().enumerate().any(|(at, a)| at != action && a.name == name);
    if name.is_empty() || taken || name.contains('/') {
        return false;
    }
    map.actions.get_mut(action).map(|a| a.name = name.to_string()).is_some()
}

/// Renames a map, unless another has the name; true when it did.
pub fn rename_map(file: &mut ActionsFile, map: usize, name: &str) -> bool {
    let name = name.trim();
    let taken = file.maps.iter().enumerate().any(|(at, m)| at != map && m.name == name);
    if name.is_empty() || taken || name.contains('/') {
        return false;
    }
    let Some(old) = file.maps.get(map).map(|m| m.name.clone()) else { return false };
    file.maps[map].name = name.to_string();
    // The maps that exclude it by its old name exclude it by its new one.
    for excluded in file.maps.iter_mut().flat_map(|m| m.excludes.iter_mut()).filter(|n| **n == old) {
        *excluded = name.to_string();
    }
    true
}

/// The interaction an action's dropdown shows, of None, Hold and Tap, and
/// its seconds.
pub fn interaction(action: &ActionData) -> (usize, Option<f32>) {
    let Some(text) = action.interaction.as_deref() else { return (0, None) };
    let seconds = text.split_once('(').and_then(|(_, rest)| rest.trim_end_matches(')').trim().parse().ok());
    match text.split('(').next().unwrap_or_default().trim() {
        "hold" => (1, seconds),
        "tap" => (2, seconds),
        _ => (0, None),
    }
}

/// Sets an action's interaction: 0 when pressed, 1 held for `seconds`, 2
/// tapped within them.
pub fn set_interaction(action: &mut ActionData, kind: usize, seconds: Option<f32>) {
    let seconds = |default: f32| format_seconds(seconds.unwrap_or(default));
    action.interaction = match kind {
        1 => Some(format!("hold({})", seconds(0.4))),
        2 => Some(format!("tap({})", seconds(0.2))),
        _ => None,
    };
}

fn format_seconds(seconds: f32) -> String {
    let text = format!("{seconds:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn full(binding: &Binding) -> FullBinding {
    match binding {
        Binding::Path(path) => FullBinding { path: Some(path.clone()), ..FullBinding::default() },
        Binding::Full(full) => full.clone(),
    }
}

/// A binding of one control with nothing to change it is written as its path alone.
fn simplest(full: FullBinding) -> Binding {
    let alone = full.processors.is_empty() && full.path.is_some();
    let composite = [&full.up, &full.down, &full.left, &full.right, &full.negative, &full.positive];
    match (alone && composite.iter().all(|part| part.is_none()), &full.path) {
        (true, Some(path)) => Binding::Path(path.clone()),
        _ => Binding::Full(full),
    }
}

fn free_name<'a>(taken: impl Iterator<Item = &'a str> + Clone, base: &str) -> String {
    let free = |name: &str| !taken.clone().any(|t| t == name);
    if free(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} {n}")).find(|name| free(name)).unwrap_or_default()
}

fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> MapData {
        let text = "format = 1\n[[map]]\nname = \"Player\"\n[[map.action]]\nname = \"Move\"\ntype = \"vector2\"\n\
                    bindings = [{ up = \"Keyboard/W\", down = \"Keyboard/S\", left = \"Keyboard/A\", right = \"Keyboard/D\" }, \
                    \"Gamepad/LeftStick\"]\n[[map.action]]\nname = \"Jump\"\nbindings = [\"Keyboard/Space\"]\n";
        toml::from_str::<ActionsFile>(text).unwrap().maps.remove(0)
    }

    #[test]
    fn rows_list_each_action_then_its_bindings_and_a_composites_parts() {
        let map = player();
        let labels: Vec<String> = rows(&map).into_iter().map(|row| shown(&map, row).text).collect();
        assert_eq!(
            labels,
            ["Move", "2D Vector", "Up: W", "Down: S", "Left: A", "Right: D", "Left Stick", "Jump", "Space"]
        );
        let space = shown(&map, Row::Binding(1, 0));
        assert_eq!((space.icon, space.detail.as_str(), space.depth), ("keyboard", "Keyboard", 1));
        assert_eq!(shown(&map, Row::Action(0)).icon, "move", "a vector's own icon until it is given one");
    }

    #[test]
    fn exclusive_maps_are_kept_by_name_and_their_conflicts_marked() {
        let text = "format = 1\n[[map]]\nname = \"Player\"\n[[map.action]]\nname = \"Jump\"\n\
                    bindings = [\"Keyboard/Space\"]\n[[map]]\nname = \"UI\"\n[[map.action]]\nname = \"Submit\"\n\
                    bindings = [\"Keyboard/Space\"]\n";
        let mut file: ActionsFile = toml::from_str(text).unwrap();
        assert_eq!(conflicts(&file, 0), [true, true], "Jump and its Space");
        set_exclusive(&mut file, 1, 0, true);
        assert_eq!(file.maps[1].excludes, ["Player"]);
        assert_eq!(conflicts(&file, 0), [false, false], "never on together");
        assert!(rename_map(&mut file, 0, "Hero"));
        assert_eq!(file.maps[1].excludes, ["Hero"], "kept by its new name");
        set_exclusive(&mut file, 0, 1, false);
        assert!(file.maps[1].excludes.is_empty(), "either map can let the other go");
        set_exclusive(&mut file, 0, 1, true);
        remove_map(&mut file, 1);
        assert!(file.maps[0].excludes.is_empty());
    }

    #[test]
    fn bindings_are_added_set_and_taken_away() {
        let mut map = player();
        let row = add_binding(&mut map, 1, None).unwrap();
        set_path(&mut map, row, "Gamepad/South");
        assert_eq!(map.actions[1].bindings[1], Binding::Path("Gamepad/South".into()), "written as its path alone");
        set_path(&mut map, Row::Part(0, 0, "up"), "Keyboard/Up");
        assert_eq!(path(&map, Row::Part(0, 0, "up")).as_deref(), Some("Keyboard/Up"));
        set_processors(&mut map, row, vec!["invert".into()]);
        assert!(matches!(&map.actions[1].bindings[1], Binding::Full(full) if full.processors == ["invert"]));

        let axis = add_binding(&mut map, 1, Some(Composite::Axis)).unwrap();
        assert_eq!(axis, Row::Part(1, 2, "negative"));
        let empty = shown(&map, axis);
        assert_eq!((empty.text.as_str(), empty.unset), ("Negative: None", true), "a part not given a control yet");
        remove(&mut map, axis);
        remove(&mut map, Row::Action(0));
        assert_eq!(map.actions.len(), 1);
        assert_eq!(map.actions[0].bindings.len(), 2);
    }

    #[test]
    fn new_names_are_free_and_renames_keep_them_so() {
        let mut map = player();
        assert_eq!(add_action(&mut map), Row::Action(2));
        assert_eq!(add_action(&mut map), Row::Action(3));
        assert_eq!((map.actions[2].name.as_str(), map.actions[3].name.as_str()), ("New Action", "New Action 2"));
        assert!(!rename_action(&mut map, 2, "Jump"), "taken");
        assert!(!rename_action(&mut map, 2, " "));
        assert!(rename_action(&mut map, 2, "Crouch"));
    }

    #[test]
    fn interactions_read_and_write_their_seconds() {
        let mut action = player().actions.remove(1);
        set_interaction(&mut action, 1, Some(0.75));
        assert_eq!(action.interaction.as_deref(), Some("hold(0.75)"));
        assert_eq!(interaction(&action), (1, Some(0.75)));
        set_interaction(&mut action, 0, None);
        assert_eq!(interaction(&action), (0, None));
    }
}
