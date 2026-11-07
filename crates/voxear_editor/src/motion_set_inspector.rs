//! A Motion Set in the Inspector: its states, each with its role, the clip
//! it stands with, its loops (each played forward or backwards), how long
//! changing into it takes, and the action played on the way in; and its
//! actions, each with its clip, the part of the body it plays on, how it
//! fades, loops, ends and moves the character, and its marks. Typing
//! changes a value as it is; adding, taking out, moving or picking redraws
//! the lists. Clip fields open the object picker, and take clips dropped
//! from the Project panel.

use voxear_engine::animation::{
    ActionMark, ActionMovement, BodyPart, MotionAction, MotionSet, MotionState, StateClip, StateRole,
};
use voxear_ui::{DragSource, NodeId, Ui};

use crate::{
    asset_inspector::{Slot, part, slot},
    collection,
    inspector::{number, prop},
};

/// A clip field of a set's: a state's idle, one of its loops or a new one,
/// or an action's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipSlot {
    Idle(usize),
    Loop(usize, usize),
    NewLoop(usize),
    Action(usize),
}

/// What was done to a set's fields this frame.
pub enum SetChange {
    /// A value typed: the set is to be saved as it is shown.
    Typed,
    /// Something added, taken out, moved or picked: saved, and shown again.
    Shape,
    /// A clip field's arrow clicked, under this node.
    Browse(ClipSlot, NodeId),
    /// Something dropped onto a clip field.
    Drop(ClipSlot, DragSource),
}

/// A set's fields, and the set as they show it.
pub struct MotionSetFields {
    pub set: MotionSet,
    state_items: NodeId,
    add_state: NodeId,
    states: Vec<StateRow>,
    action_items: NodeId,
    add_action: NodeId,
    actions: Vec<ActionRow>,
}

struct StateRow {
    remove: NodeId,
    name: NodeId,
    role: NodeId,
    idle: Slot,
    blend: NodeId,
    enter: NodeId,
    clip_items: NodeId,
    clips: Vec<(Slot, NodeId, NodeId)>,
    add_clip: NodeId,
}

struct ActionRow {
    remove: NodeId,
    name: NodeId,
    clip: Slot,
    body: NodeId,
    /// The bone's dropdown, and the names it offers, none first.
    bone: Option<(NodeId, Vec<String>)>,
    blend_in: NodeId,
    blend_out: NodeId,
    looping: NodeId,
    hold_end: NodeId,
    movement: NodeId,
    lunge: Option<[NodeId; 3]>,
    turn_until: NodeId,
    cancel_from: NodeId,
    moving_cancels: NodeId,
    mark_items: NodeId,
    marks: Vec<(NodeId, NodeId, NodeId)>,
    add_mark: NodeId,
}

impl MotionSetFields {
    /// The fields of `set` under `root`; `label` names a clip as fields show
    /// it, and `bones` names the bones of a clip's skeleton, once read.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        set: MotionSet,
        label: &dyn Fn(&str) -> String,
        bones: &dyn Fn(&str) -> Vec<String>,
    ) -> Self {
        let rows = part(ui, root, "States", "play");
        let list = collection::collection(ui, rows, set.states.len(), true, Some("Add State"));
        let actions: Vec<&str> = std::iter::once("None").chain(set.actions.iter().map(|a| a.name.as_str())).collect();
        let states = set
            .states
            .iter()
            .enumerate()
            .map(|(at, state)| state_row(ui, list.items, at, state, &actions, label))
            .collect();
        let (state_items, add_state) = (list.items, list.add.expect("a set takes states"));
        let rows = part(ui, root, "Actions", "play");
        let list = collection::collection(ui, rows, set.actions.len(), true, Some("Add Action"));
        let actions = set
            .actions
            .iter()
            .enumerate()
            .map(|(at, action)| action_row(ui, list.items, at, action, label, &bones(&action.clip)))
            .collect();
        let (action_items, add_action) = (list.items, list.add.expect("a set takes actions"));
        Self { set, state_items, add_state, states, action_items, add_action, actions }
    }

    /// What was done this frame, the set changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<SetChange> {
        if ui.clicked(self.add_state) {
            let name = format!("State {}", self.set.states.len() + 1);
            self.set.states.push(MotionState { name, ..MotionState::default() });
            return Some(SetChange::Shape);
        }
        if ui.clicked(self.add_action) {
            let name = format!("Action {}", self.set.actions.len() + 1);
            self.set.actions.push(MotionAction { name, ..MotionAction::default() });
            return Some(SetChange::Shape);
        }
        if let Some((from, to)) = collection::moved(ui, self.state_items) {
            let state = self.set.states.remove(from);
            self.set.states.insert(to, state);
            return Some(SetChange::Shape);
        }
        if let Some((from, to)) = collection::moved(ui, self.action_items) {
            let action = self.set.actions.remove(from);
            self.set.actions.insert(to, action);
            return Some(SetChange::Shape);
        }
        for at in 0..self.states.len() {
            if let Some(change) = self.update_state(ui, at) {
                return Some(change);
            }
        }
        for at in 0..self.actions.len() {
            if let Some(change) = self.update_action(ui, at) {
                return Some(change);
            }
        }
        None
    }

    fn update_state(&mut self, ui: &mut Ui, at: usize) -> Option<SetChange> {
        let row = &self.states[at];
        let state = &mut self.set.states[at];
        if ui.clicked(row.remove) {
            self.set.states.remove(at);
            return Some(SetChange::Shape);
        }
        if let Some(change) = clip_field(ui, row.idle, ClipSlot::Idle(at)) {
            return Some(change);
        }
        if ui.clicked(row.add_clip) {
            return Some(SetChange::Browse(ClipSlot::NewLoop(at), row.add_clip));
        }
        if let Some((from, to)) = collection::moved(ui, row.clip_items) {
            let clip = state.clips.remove(from);
            state.clips.insert(to, clip);
            return Some(SetChange::Shape);
        }
        for (place, &(field, backwards, remove)) in row.clips.iter().enumerate() {
            if let Some(change) = clip_field(ui, field, ClipSlot::Loop(at, place)) {
                return Some(change);
            }
            if ui.changed(backwards) {
                state.clips[place].backwards = ui.is_checked(backwards);
                return Some(SetChange::Shape);
            }
            if ui.clicked(remove) {
                state.clips.remove(place);
                return Some(SetChange::Shape);
            }
        }
        if ui.changed(row.role) {
            state.role = StateRole::ALL.get(ui.selected_option(row.role)).copied().unwrap_or_default();
            return Some(SetChange::Shape);
        }
        if ui.changed(row.enter) {
            let index = ui.selected_option(row.enter);
            state.enter =
                index.checked_sub(1).and_then(|i| self.set.actions.get(i)).map_or(String::new(), |a| a.name.clone());
            return Some(SetChange::Shape);
        }
        let state = &mut self.set.states[at];
        if let Some(name) = typed(ui, row.name, &state.name) {
            state.name = name;
            return Some(SetChange::Typed);
        }
        if let Some(blend) = typed_number(ui, row.blend, state.blend, 0.0, 5.0) {
            state.blend = blend;
            return Some(SetChange::Typed);
        }
        None
    }

    fn update_action(&mut self, ui: &mut Ui, at: usize) -> Option<SetChange> {
        let row = &self.actions[at];
        let action = &mut self.set.actions[at];
        if ui.clicked(row.remove) {
            self.set.actions.remove(at);
            return Some(SetChange::Shape);
        }
        if let Some(change) = clip_field(ui, row.clip, ClipSlot::Action(at)) {
            return Some(change);
        }
        if ui.clicked(row.add_mark) {
            let name = format!("Mark {}", action.marks.len() + 1);
            action.marks.push(ActionMark { name, time: 0.0 });
            return Some(SetChange::Shape);
        }
        if let Some((from, to)) = collection::moved(ui, row.mark_items) {
            let mark = action.marks.remove(from);
            action.marks.insert(to, mark);
            return Some(SetChange::Shape);
        }
        for (place, &(name, time, remove)) in row.marks.iter().enumerate() {
            if ui.clicked(remove) {
                action.marks.remove(place);
                return Some(SetChange::Shape);
            }
            let mark = &mut action.marks[place];
            if let Some(typed) = typed(ui, name, &mark.name) {
                mark.name = typed;
                return Some(SetChange::Typed);
            }
            if let Some(typed) = typed_number(ui, time, mark.time, 0.0, 600.0) {
                mark.time = typed;
                return Some(SetChange::Typed);
            }
        }
        if ui.changed(row.body) {
            action.body = BodyPart::ALL.get(ui.selected_option(row.body)).copied().unwrap_or_default();
            return Some(SetChange::Shape);
        }
        if let Some((bone, names)) = row.bone.as_ref().filter(|(bone, _)| ui.changed(*bone)) {
            let picked = ui.selected_option(*bone);
            action.bone = names.get(picked).filter(|_| picked > 0).cloned().unwrap_or_default();
            return Some(SetChange::Shape);
        }
        if ui.changed(row.movement) {
            action.movement = ActionMovement::ALL.get(ui.selected_option(row.movement)).copied().unwrap_or_default();
            return Some(SetChange::Shape);
        }
        for (toggle, value) in [
            (row.looping, &mut action.looping),
            (row.hold_end, &mut action.hold_end),
            (row.moving_cancels, &mut action.moving_cancels),
        ] {
            if ui.changed(toggle) {
                *value = ui.is_checked(toggle);
                return Some(SetChange::Shape);
            }
        }
        if let Some(name) = typed(ui, row.name, &action.name) {
            action.name = name;
            return Some(SetChange::Typed);
        }
        let mut numbers = vec![
            (row.blend_in, &mut action.blend_in, 5.0),
            (row.blend_out, &mut action.blend_out, 5.0),
            (row.turn_until, &mut action.turn_until, 600.0),
            (row.cancel_from, &mut action.cancel_from, 600.0),
        ];
        if let Some([distance, start, length]) = row.lunge {
            numbers.extend([
                (distance, &mut action.lunge_distance, 50.0),
                (start, &mut action.lunge_start, 600.0),
                (length, &mut action.lunge_length, 600.0),
            ]);
        }
        for (field, value, most) in numbers {
            if let Some(typed) = typed_number(ui, field, *value, 0.0, most) {
                *value = typed;
                return Some(SetChange::Typed);
            }
        }
        None
    }
}

/// A state's item: its fields, and its loops in a list of their own.
fn state_row(
    ui: &mut Ui,
    items: NodeId,
    at: usize,
    state: &MotionState,
    actions: &[&str],
    label: &dyn Fn(&str) -> String,
) -> StateRow {
    let item = collection::item(ui, items, at, true);
    let rows = item.body;
    let name = text(ui, rows, "Name", &state.name);
    let roles = StateRole::ALL.map(StateRole::label);
    let role = choice(ui, rows, "Role", &roles, StateRole::ALL.iter().position(|&r| r == state.role).unwrap_or(0));
    let idle = clip_slot(ui, rows, "Idle Clip", &state.idle, label);
    let blend = text(ui, rows, "Blend Time", &number(state.blend));
    let entered = actions.iter().skip(1).position(|&a| a == state.enter).map_or(0, |i| i + 1);
    let enter = choice(ui, rows, "Enter Action", actions, entered);
    let loops = prop(ui, rows, "Clips");
    let list = collection::collection(ui, loops, state.clips.len(), true, Some("Add Clip"));
    let clips =
        state.clips.iter().enumerate().map(|(place, clip)| loop_row(ui, list.items, place, clip, label)).collect();
    let add_clip = list.add.expect("a state takes clips");
    ui.accept_drops(add_clip, true);
    StateRow {
        remove: item.remove.expect("states can be taken out"),
        name,
        role,
        idle,
        blend,
        enter,
        clip_items: list.items,
        clips,
        add_clip,
    }
}

/// A loop's item: its clip, and whether it plays backwards.
fn loop_row(
    ui: &mut Ui,
    items: NodeId,
    place: usize,
    clip: &StateClip,
    label: &dyn Fn(&str) -> String,
) -> (Slot, NodeId, NodeId) {
    let item = collection::item(ui, items, place, true);
    let field = clip_slot(ui, item.body, "Clip", &clip.clip, label);
    let backwards = toggle(ui, item.body, "Backwards", clip.backwards);
    (field, backwards, item.remove.expect("loops can be taken out"))
}

/// An action's item: its fields, the lunge's only for a lunge and the bone
/// only for a bone's part, and its marks in a list of their own.
fn action_row(
    ui: &mut Ui,
    items: NodeId,
    at: usize,
    action: &MotionAction,
    label: &dyn Fn(&str) -> String,
    bones: &[String],
) -> ActionRow {
    let item = collection::item(ui, items, at, true);
    let rows = item.body;
    let name = text(ui, rows, "Name", &action.name);
    let clip = clip_slot(ui, rows, "Clip", &action.clip, label);
    let parts = BodyPart::ALL.map(BodyPart::label);
    let body = choice(ui, rows, "Body", &parts, BodyPart::ALL.iter().position(|&b| b == action.body).unwrap_or(0));
    let bone = (action.body == BodyPart::Bone).then(|| {
        let names: Vec<String> = std::iter::once("None".to_string()).chain(bones.iter().cloned()).collect();
        let at = names.iter().position(|name| *name == action.bone).unwrap_or(0);
        let options: Vec<&str> = names.iter().map(String::as_str).collect();
        (choice(ui, rows, "Bone", &options, at), names)
    });
    let blend_in = text(ui, rows, "Blend In", &number(action.blend_in));
    let blend_out = text(ui, rows, "Blend Out", &number(action.blend_out));
    let looping = toggle(ui, rows, "Loop", action.looping);
    let hold_end = toggle(ui, rows, "Hold End", action.hold_end);
    let movements = ActionMovement::ALL.map(ActionMovement::label);
    let moving = ActionMovement::ALL.iter().position(|&m| m == action.movement).unwrap_or(0);
    let movement = choice(ui, rows, "Movement", &movements, moving);
    let lunge = (action.movement == ActionMovement::Lunge).then(|| {
        [
            text(ui, rows, "Lunge Distance", &number(action.lunge_distance)),
            text(ui, rows, "Lunge Start", &number(action.lunge_start)),
            text(ui, rows, "Lunge Length", &number(action.lunge_length)),
        ]
    });
    let turn_until = text(ui, rows, "Turn Until", &number(action.turn_until));
    let cancel_from = text(ui, rows, "Cancel From", &number(action.cancel_from));
    let moving_cancels = toggle(ui, rows, "Moving Cancels", action.moving_cancels);
    let marked = prop(ui, rows, "Marks");
    let list = collection::collection(ui, marked, action.marks.len(), true, Some("Add Mark"));
    let marks = action
        .marks
        .iter()
        .enumerate()
        .map(|(place, mark)| {
            let item = collection::item(ui, list.items, place, true);
            let name = text(ui, item.body, "Name", &mark.name);
            let time = text(ui, item.body, "Time", &number(mark.time));
            (name, time, item.remove.expect("marks can be taken out"))
        })
        .collect();
    ActionRow {
        remove: item.remove.expect("actions can be taken out"),
        name,
        clip,
        body,
        bone,
        blend_in,
        blend_out,
        looping,
        hold_end,
        movement,
        lunge,
        turn_until,
        cancel_from,
        moving_cancels,
        mark_items: list.items,
        marks,
        add_mark: list.add.expect("an action takes marks"),
    }
}

/// A clip field, showing the clip by its name and file.
fn clip_slot(ui: &mut Ui, rows: NodeId, name: &str, clip: &str, label: &dyn Fn(&str) -> String) -> Slot {
    let field = slot(ui, rows, name, "play");
    ui.set_text(field.label, &if clip.is_empty() { "None (Clip)".to_string() } else { label(clip) });
    ui.set_visible(field.icon, !clip.is_empty());
    field
}

/// A clip field's arrow or the field clicked, or something dropped on it.
fn clip_field(ui: &Ui, field: Slot, slot: ClipSlot) -> Option<SetChange> {
    if let Some(source) = ui.dropped_onto(field.field) {
        return Some(SetChange::Drop(slot, source));
    }
    (ui.clicked(field.arrow) || ui.clicked(field.field)).then(|| SetChange::Browse(slot, field.field))
}

fn text(ui: &mut Ui, rows: NodeId, label: &str, value: &str) -> NodeId {
    let row = prop(ui, rows, label);
    let field = ui.text_field(row, value);
    ui.add_class(field, "value-input");
    field
}

fn choice(ui: &mut Ui, rows: NodeId, label: &str, options: &[&str], selected: usize) -> NodeId {
    let row = prop(ui, rows, label);
    let dropdown = ui.dropdown(row, options, selected);
    ui.add_class(dropdown, "value-choice");
    dropdown
}

fn toggle(ui: &mut Ui, rows: NodeId, label: &str, on: bool) -> NodeId {
    let row = prop(ui, rows, label);
    ui.toggle(row, "", on)
}

/// Text typed into `field`, once done with (Enter, or the field left),
/// when it differs from `shown`.
fn typed(ui: &Ui, field: NodeId, shown: &str) -> Option<String> {
    let done = ui.submitted(field) || ui.focused() != Some(field);
    let text = ui.text(field).trim().to_string();
    (done && !text.is_empty() && text != shown).then_some(text)
}

/// A number typed into `field`, once done with, kept between `low` and
/// `high`, when it differs from `shown`.
fn typed_number(ui: &Ui, field: NodeId, shown: f32, low: f32, high: f32) -> Option<f32> {
    let done = ui.submitted(field) || ui.focused() != Some(field);
    let value = ui.text(field).trim().parse::<f32>().ok().filter(|v| v.is_finite())?.clamp(low, high);
    (done && value != shown).then_some(value)
}
