//! A Gun Handling in the Inspector: the kind of gun it is for, how the gun
//! is held, as offsets on how every gun is, and each of its actions, which
//! can be based on a clip: the clip, the part of it the action plays, and
//! how much the body, the gun and the arms follow it. Typing or sliding
//! changes a value as it is; picking a clip or the kind redraws the
//! fields. Clip fields open the object picker, and take clips dropped from
//! the Project panel.

use voxear_engine::guns::{
    GunKind,
    actions::ActionKind,
    base_clip::BaseClip,
    handling::{ActionOffsets, GunHandling, Poses},
    templates,
};
use voxear_ui::{DragSource, NodeId, Style, Ui};

use crate::{
    asset_inspector::{Slot, part, slot},
    inspector::{number, prop, vector_row},
};

/// One of a handling's actions: a built-in one of its kind, or one of its
/// own, by its place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlingAction {
    Builtin(ActionKind),
    Own(usize),
}

/// What was done to a handling's fields this frame.
pub enum HandlingChange {
    /// A value typed or slid: the handling is to be saved as it is shown.
    Typed,
    /// The kind picked: saved, and shown again.
    Shape,
    /// An action's clip field clicked, under this node.
    Browse(HandlingAction, NodeId),
    /// Something dropped onto an action's clip field.
    Drop(HandlingAction, DragSource),
}

/// An offset on how every gun is held, as a row shows it.
#[derive(Clone, Copy)]
enum Hold {
    Hip,
    HipTurn,
    Aim,
    AimTurn,
    Shoulder,
    GripHand,
    GripTurn,
    SupportHand,
    SupportTurn,
}

const HOLDS: [(Hold, &str); 9] = [
    (Hold::Hip, "Hip"),
    (Hold::HipTurn, "Hip Turn"),
    (Hold::Aim, "Aim"),
    (Hold::AimTurn, "Aim Turn"),
    (Hold::Shoulder, "Shoulder"),
    (Hold::GripHand, "Grip Hand"),
    (Hold::GripTurn, "Grip Turn"),
    (Hold::SupportHand, "Support Hand"),
    (Hold::SupportTurn, "Support Turn"),
];

impl Hold {
    fn value(self, poses: &Poses) -> [f32; 3] {
        match self {
            Hold::Hip => poses.hip.at,
            Hold::HipTurn => poses.hip.turn,
            Hold::Aim => poses.aim.at,
            Hold::AimTurn => poses.aim.turn,
            Hold::Shoulder => poses.shoulder,
            Hold::GripHand => poses.grip.at,
            Hold::GripTurn => poses.grip.turn,
            Hold::SupportHand => poses.support.at,
            Hold::SupportTurn => poses.support.turn,
        }
    }

    fn slot(self, poses: &mut Poses) -> &mut [f32; 3] {
        match self {
            Hold::Hip => &mut poses.hip.at,
            Hold::HipTurn => &mut poses.hip.turn,
            Hold::Aim => &mut poses.aim.at,
            Hold::AimTurn => &mut poses.aim.turn,
            Hold::Shoulder => &mut poses.shoulder,
            Hold::GripHand => &mut poses.grip.at,
            Hold::GripTurn => &mut poses.grip.turn,
            Hold::SupportHand => &mut poses.support.at,
            Hold::SupportTurn => &mut poses.support.turn,
        }
    }
}

/// A handling's fields, and the handling as they show it.
pub struct GunHandlingFields {
    pub handling: GunHandling,
    kind: NodeId,
    holds: Vec<(Hold, [NodeId; 3])>,
    cant: NodeId,
    actions: Vec<ActionRow>,
}

struct ActionRow {
    action: HandlingAction,
    /// A built-in action's speed.
    speed: Option<NodeId>,
    clip: Slot,
    /// Its clip's fields, while it is based on one.
    based: Option<Based>,
}

struct Based {
    start: NodeId,
    end: NodeId,
    /// A slider and its number for each.
    body: (NodeId, NodeId),
    gun: (NodeId, NodeId),
    arms: (NodeId, NodeId),
    blend: NodeId,
}

impl GunHandlingFields {
    /// The fields of `handling` under `root`; `label` names a clip as
    /// fields show it.
    pub fn new(ui: &mut Ui, root: NodeId, handling: GunHandling, label: &dyn Fn(&str) -> String) -> Self {
        let rows = part(ui, root, "Gun Handling", "crosshair");
        let kinds: Vec<&str> = std::iter::once("Any").chain(GunKind::ALL.iter().map(|kind| kind.title())).collect();
        let picked = handling.kind.and_then(|kind| GunKind::ALL.iter().position(|&k| k == kind)).map_or(0, |at| at + 1);
        let kind = choice(ui, rows, "Gun Kind", &kinds, picked);

        let rows = part(ui, root, "Holding", "move");
        let holds = HOLDS
            .iter()
            .map(|&(hold, title)| {
                let (_, fields) = vector_row(ui, rows, title);
                for (field, n) in fields.iter().zip(hold.value(&handling.poses)) {
                    ui.set_text(*field, &number(n));
                }
                (hold, fields)
            })
            .collect();
        let cant = number_field(ui, rows, "Cant", handling.poses.cant);

        let actions = actions_of(&handling)
            .into_iter()
            .map(|(action, title)| action_row(ui, root, &handling, action, &title, label))
            .collect();
        Self { handling, kind, holds, cant, actions }
    }

    /// What was done this frame, the handling changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<HandlingChange> {
        if ui.changed(self.kind) {
            self.handling.kind = ui.selected_option(self.kind).checked_sub(1).map(|at| GunKind::ALL[at]);
            return Some(HandlingChange::Shape);
        }
        let mut typed = false;
        for (hold, fields) in &self.holds {
            if fields.iter().any(|&field| ui.changed(field))
                && let [Some(x), Some(y), Some(z)] = fields.map(|field| parsed(ui, field))
            {
                *hold.slot(&mut self.handling.poses) = [x, y, z];
                typed = true;
            }
        }
        if let Some(cant) = changed_number(ui, self.cant) {
            self.handling.poses.cant = cant;
            typed = true;
        }
        for row in &self.actions {
            if let Some(source) = ui.dropped_onto(row.clip.field) {
                return Some(HandlingChange::Drop(row.action, source));
            }
            if ui.clicked(row.clip.arrow) || ui.clicked(row.clip.field) {
                return Some(HandlingChange::Browse(row.action, row.clip.field));
            }
            if let Some(speed) = row.speed.and_then(|field| changed_number(ui, field))
                && let HandlingAction::Builtin(kind) = row.action
            {
                offsets_of(&mut self.handling, kind).speed = speed.max(0.05);
                tidy(&mut self.handling);
                typed = true;
            }
            if let Some(based) = &row.based
                && let Some(base) = base_of(&mut self.handling, row.action)
            {
                typed |= based.update(ui, base);
            }
        }
        typed.then_some(HandlingChange::Typed)
    }
}

impl Based {
    /// Reads what was typed or slid into `base`; whether anything was.
    fn update(&self, ui: &mut Ui, base: &mut BaseClip) -> bool {
        let mut typed = false;
        for (field, value) in [(self.start, &mut base.start), (self.end, &mut base.end), (self.blend, &mut base.blend)]
        {
            if let Some(n) = changed_number(ui, field) {
                (*value, typed) = (n.max(0.0), true);
            }
        }
        for ((slider, field), value) in
            [(self.body, &mut base.body), (self.gun, &mut base.gun), (self.arms, &mut base.arms)]
        {
            if ui.changed(slider) {
                *value = ui.slider_value(slider);
                ui.set_text(field, &number(*value));
                typed = true;
            } else if let Some(n) = changed_number(ui, field) {
                *value = n.clamp(0.0, 1.0);
                ui.set_slider_value(slider, *value);
                typed = true;
            }
        }
        typed
    }
}

/// The actions a handling shows: the built-in ones its kind does, every
/// one when it is for any kind, then its own, each with its title.
fn actions_of(handling: &GunHandling) -> Vec<(HandlingAction, String)> {
    let does =
        |action: ActionKind| handling.kind.is_none_or(|kind| !templates::template(kind, action).beats.is_empty());
    let built_in = ActionKind::ALL.into_iter().filter(|&action| does(action));
    let built_in = built_in.map(|action| (HandlingAction::Builtin(action), action.title().to_string()));
    let own = handling.actions.iter().enumerate().map(|(at, own)| (HandlingAction::Own(at), own.name.clone()));
    built_in.chain(own).collect()
}

fn action_row(
    ui: &mut Ui,
    root: NodeId,
    handling: &GunHandling,
    action: HandlingAction,
    title: &str,
    label: &dyn Fn(&str) -> String,
) -> ActionRow {
    let rows = part(ui, root, title, "play");
    let speed = match action {
        HandlingAction::Builtin(kind) => {
            let speed = handling.offsets(kind).map_or(1.0, |offsets| offsets.speed);
            Some(number_field(ui, rows, "Speed", speed))
        }
        HandlingAction::Own(_) => None,
    };
    let base = clip_of(handling, action);
    let clip = slot(ui, rows, "Clip", "play");
    let named = base.map(|base| base.clip.as_str()).filter(|clip| !clip.is_empty());
    ui.set_text(clip.label, &named.map_or_else(|| "None (Clip)".to_string(), label));
    ui.set_visible(clip.icon, named.is_some());
    let based = base.filter(|base| !base.clip.is_empty()).map(|base| Based {
        start: number_field(ui, rows, "Start", base.start),
        end: number_field(ui, rows, "End", base.end),
        body: weight(ui, rows, "Body", base.body),
        gun: weight(ui, rows, "Gun", base.gun),
        arms: weight(ui, rows, "Arms", base.arms),
        blend: number_field(ui, rows, "Blend", base.blend),
    });
    ActionRow { action, speed, clip, based }
}

/// The clip an action is based on, if any.
fn clip_of(handling: &GunHandling, action: HandlingAction) -> Option<&BaseClip> {
    match action {
        HandlingAction::Builtin(kind) => handling.offsets(kind)?.clip.as_ref(),
        HandlingAction::Own(at) => handling.actions.get(at)?.clip.as_ref(),
    }
}

fn base_of(handling: &mut GunHandling, action: HandlingAction) -> Option<&mut BaseClip> {
    match action {
        HandlingAction::Builtin(kind) => handling.offsets.iter_mut().find(|o| o.action == kind)?.clip.as_mut(),
        HandlingAction::Own(at) => handling.actions.get_mut(at)?.clip.as_mut(),
    }
}

/// A built-in action's offsets, made when it has none.
fn offsets_of(handling: &mut GunHandling, action: ActionKind) -> &mut ActionOffsets {
    let at = match handling.offsets.iter().position(|offsets| offsets.action == action) {
        Some(at) => at,
        None => {
            handling.offsets.push(ActionOffsets { action, ..ActionOffsets::default() });
            handling.offsets.len() - 1
        }
    };
    &mut handling.offsets[at]
}

/// Takes away offsets on built-in actions that offset nothing.
fn tidy(handling: &mut GunHandling) {
    handling.offsets.retain(|offsets| *offsets != ActionOffsets { action: offsets.action, ..ActionOffsets::default() });
}

/// Bases `action` on the clip `clip` refers to, keeping how much it was
/// followed, or on none for an empty one.
pub fn set_clip(handling: &mut GunHandling, action: HandlingAction, clip: String) {
    let based = (!clip.is_empty()).then(|| {
        let was = clip_of(handling, action).cloned().unwrap_or_default();
        BaseClip { clip, ..was }
    });
    match action {
        HandlingAction::Builtin(kind) => offsets_of(handling, kind).clip = based,
        HandlingAction::Own(at) => {
            if let Some(own) = handling.actions.get_mut(at) {
                own.clip = based;
            }
        }
    }
    tidy(handling);
}

fn choice(ui: &mut Ui, rows: NodeId, label: &str, options: &[&str], selected: usize) -> NodeId {
    let row = prop(ui, rows, label);
    let dropdown = ui.dropdown(row, options, selected);
    ui.add_class(dropdown, "value-choice");
    dropdown
}

fn number_field(ui: &mut Ui, rows: NodeId, label: &str, value: f32) -> NodeId {
    let row = prop(ui, rows, label);
    let field = ui.text_field(row, &number(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    field
}

/// A row with a slider from 0 to 1 and its number beside it.
fn weight(ui: &mut Ui, rows: NodeId, label: &str, value: f32) -> (NodeId, NodeId) {
    let row = prop(ui, rows, label);
    let holder = ui.panel(row, Style::row());
    ui.add_class(holder, "attr-range");
    let slider = ui.slider(holder, 0.0, 1.0, value);
    ui.add_class(slider, "attr-slider");
    let field = ui.text_field(holder, &number(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    ui.add_class(field, "attr-range-input");
    (slider, field)
}

fn parsed(ui: &Ui, field: NodeId) -> Option<f32> {
    ui.text(field).trim().parse().ok()
}

fn changed_number(ui: &Ui, field: NodeId) -> Option<f32> {
    ui.changed(field).then(|| parsed(ui, field)).flatten()
}

#[cfg(test)]
mod tests {
    use voxear_engine::guns::handling::CustomAction;

    use super::*;

    #[test]
    fn a_built_in_action_based_on_a_clip_keeps_how_much_it_was_followed_and_none_takes_it_away() {
        let mut handling = GunHandling::default();
        let reload = HandlingAction::Builtin(ActionKind::Reload);
        set_clip(&mut handling, reload, "Anims#clip:Reload".into());
        base_of(&mut handling, reload).unwrap().gun = 0.5;
        set_clip(&mut handling, reload, "Anims#clip:Reload_Fast".into());
        let base = clip_of(&handling, reload).unwrap();
        assert_eq!((base.clip.as_str(), base.gun), ("Anims#clip:Reload_Fast", 0.5));
        set_clip(&mut handling, reload, String::new());
        assert!(handling.offsets.is_empty(), "with no clip and nothing else, no offsets are left");
    }

    #[test]
    fn a_handling_shows_the_built_in_actions_then_its_own() {
        let inspect = CustomAction { name: "Inspect".into(), ..CustomAction::default() };
        let handling = GunHandling { actions: vec![inspect], ..GunHandling::default() };
        let shown = actions_of(&handling);
        assert_eq!(shown.len(), ActionKind::ALL.len() + 1, "for any kind, every built-in, and its own");
        assert_eq!(shown.last(), Some(&(HandlingAction::Own(0), "Inspect".to_string())));
    }
}
