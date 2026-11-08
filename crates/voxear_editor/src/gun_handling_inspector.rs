//! A Gun Handling in the Inspector: the kind of gun it is for, how the gun
//! is held, as offsets on how every gun is, the clips a gun from an
//! animation pack is held by, and each of its actions. An action plays as
//! built, based on a clip (the clip, the part of it the action plays, and
//! how much the body, the gun and the arms follow it), or from clips, part
//! by part, each with its marks. Typing, sliding or picking from a menu
//! changes a value as it is; picking a clip or the kind, a mode, or adding
//! or taking out a part or a mark redraws the fields. Clip fields open the
//! object picker, and take clips dropped from the Project panel.

use voxear_engine::guns::{
    GunKind,
    actions::{ActionKind, ClipPart, Event, Fact, Mark, Repeat, When},
    base_clip::BaseClip,
    handling::{ActionOffsets, GunHandling, PackCamera, PackClips, Poses},
    templates,
};
use voxear_ui::{DragSource, NodeId, Style, Ui};

use crate::{
    asset_inspector::{Slot, part, slot},
    collection,
    inspector::{number, prop, vector_row},
};

/// One of a handling's actions: a built-in one of its kind, or one of its
/// own, by its place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlingAction {
    Builtin(ActionKind),
    Own(usize),
}

/// One of a handling's clip fields: the clip an action is based on, the
/// arms or the gun clip of one of an action's parts by its place, or one
/// of the clips the gun is held by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlingClip {
    Based(HandlingAction),
    Arms(HandlingAction, usize),
    Gun(HandlingAction, usize),
    Held(HeldClip),
}

/// One of the clips a gun from an animation pack is held by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldClip {
    Rest,
    Idle,
    Walk,
    Run,
    RunStart,
    Sprint,
    SprintStart,
    Fire,
    AimedFire,
    GunFire,
}

const HELD_CLIPS: [(HeldClip, &str); 10] = [
    (HeldClip::Rest, "Rest"),
    (HeldClip::Idle, "Idle"),
    (HeldClip::Walk, "Walk"),
    (HeldClip::Run, "Run"),
    (HeldClip::RunStart, "Run Start"),
    (HeldClip::Sprint, "Sprint"),
    (HeldClip::SprintStart, "Sprint Start"),
    (HeldClip::Fire, "Fire"),
    (HeldClip::AimedFire, "Aimed Fire"),
    (HeldClip::GunFire, "Gun Fire"),
];

impl HeldClip {
    fn slot(self, pack: &mut PackClips) -> &mut String {
        match self {
            HeldClip::Rest => &mut pack.rest,
            HeldClip::Idle => &mut pack.idle,
            HeldClip::Walk => &mut pack.walk,
            HeldClip::Run => &mut pack.run,
            HeldClip::RunStart => &mut pack.run_start,
            HeldClip::Sprint => &mut pack.sprint,
            HeldClip::SprintStart => &mut pack.sprint_start,
            HeldClip::Fire => &mut pack.fire,
            HeldClip::AimedFire => &mut pack.aimed_fire,
            HeldClip::GunFire => &mut pack.gun_fire,
        }
    }

    fn clip(self, pack: &PackClips) -> &str {
        match self {
            HeldClip::Rest => &pack.rest,
            HeldClip::Idle => &pack.idle,
            HeldClip::Walk => &pack.walk,
            HeldClip::Run => &pack.run,
            HeldClip::RunStart => &pack.run_start,
            HeldClip::Sprint => &pack.sprint,
            HeldClip::SprintStart => &pack.sprint_start,
            HeldClip::Fire => &pack.fire,
            HeldClip::AimedFire => &pack.aimed_fire,
            HeldClip::GunFire => &pack.gun_fire,
        }
    }
}

/// What was done to a handling's fields this frame.
pub enum HandlingChange {
    /// A value typed or slid: the handling is to be saved as it is shown.
    Typed,
    /// The kind or a mode picked, or a part or mark added or taken out:
    /// saved, and shown again.
    Shape,
    /// A clip field clicked, under this node.
    Browse(HandlingClip, NodeId),
    /// Something dropped onto a clip field.
    Drop(HandlingClip, DragSource),
}

/// How an action plays: as built, based on a clip, or from clips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Built,
    Based,
    Clips,
}

const MODES: [(Mode, &str); 3] = [(Mode::Built, "Built-in"), (Mode::Based, "Based on Clip"), (Mode::Clips, "Clips")];

/// When a part plays, as its menu offers it.
const WHENS: [(Option<Fact>, &str); 11] = [
    (None, "Always"),
    (Some(Fact::ChamberFilled), "Chamber Filled"),
    (Some(Fact::ChamberEmpty), "Chamber Empty"),
    (Some(Fact::ChamberLive), "Round Chambered"),
    (Some(Fact::ChamberNotLive), "No Round Chambered"),
    (Some(Fact::Fed), "Fed"),
    (Some(Fact::NotFed), "Not Fed"),
    (Some(Fact::Open), "Open"),
    (Some(Fact::Closed), "Closed"),
    (Some(Fact::Full), "Full"),
    (Some(Fact::NotFull), "Not Full"),
];

const REPEATS: [(Repeat, &str); 5] = [
    (Repeat::Once, "Once"),
    (Repeat::PerClip, "Per Clip"),
    (Repeat::PerMissingRound, "Per Missing Round"),
    (Repeat::PerChamber, "Per Chamber"),
    (Repeat::UntilFull, "Until Full"),
];

const EVENTS: [(Event, &str); 9] = [
    (Event::Eject, "Eject"),
    (Event::Chamber, "Chamber"),
    (Event::AddRounds(1), "Round In"),
    (Event::Open, "Open"),
    (Event::Close, "Close"),
    (Event::Unload, "Unload"),
    (Event::Load, "Load"),
    (Event::SeeFeed, "See Feed"),
    (Event::SeeChamber, "See Chamber"),
];

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
    held: HeldRows,
    actions: Vec<ActionRow>,
}

/// Where the camera looks from while clips hold the gun, as its menu names
/// each.
const CAMERAS: [(PackCamera, &str); 2] = [(PackCamera::Filmed, "As Filmed"), (PackCamera::Eyes, "At the Eyes")];

/// The clips the gun is held by: the carrier's menu and the bones it
/// offers after "Auto", where it sits from it, each clip, and the numbers.
struct HeldRows {
    carrier: (NodeId, Vec<String>),
    carrier_at: [NodeId; 3],
    carrier_turn: [NodeId; 3],
    eyes: [NodeId; 3],
    camera: NodeId,
    clips: Vec<(HeldClip, Slot)>,
    walk_speed: NodeId,
    run_speed: NodeId,
    blend: NodeId,
    hand_return: NodeId,
    /// How much of each layer goes on top of the clips: Look Sway, Move
    /// Sway, Bob, Breathing and Head Bob.
    layers: [NodeId; 5],
}

struct ActionRow {
    action: HandlingAction,
    mode: NodeId,
    /// A built-in action's speed.
    speed: Option<NodeId>,
    /// The clip it is based on, and its fields once it has one.
    clip: Option<Slot>,
    based: Option<Based>,
    /// Its parts, played from clips.
    parts: Option<Parts>,
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

struct Parts {
    items: NodeId,
    add: NodeId,
    rows: Vec<PartRow>,
}

struct PartRow {
    remove: NodeId,
    arms: Slot,
    gun: Slot,
    start: NodeId,
    end: NodeId,
    when: NodeId,
    repeat: NodeId,
    mark_items: NodeId,
    add_mark: NodeId,
    /// Each mark's event menu, time and remove button.
    marks: Vec<(NodeId, NodeId, NodeId)>,
}

impl GunHandlingFields {
    /// The fields of `handling` under `root`; `label` names a clip as
    /// fields show it, and `bones` gives the bones of a clip's skeleton.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        handling: GunHandling,
        label: &dyn Fn(&str) -> String,
        bones: &dyn Fn(&str) -> Vec<String>,
    ) -> Self {
        let rows = part(ui, root, "Gun Handling", "crosshair");
        let kinds: Vec<&str> = std::iter::once("Any").chain(GunKind::ALL.iter().map(|kind| kind.title())).collect();
        let picked = handling.kind.and_then(|kind| GunKind::ALL.iter().position(|&k| k == kind)).map_or(0, |at| at + 1);
        let kind = choice(ui, rows, "Gun Kind", &kinds, picked);

        let rows = part(ui, root, "Holding", "move");
        let holds = HOLDS
            .iter()
            .map(|&(hold, title)| {
                let fields = vector(ui, rows, title, hold.value(&handling.poses));
                (hold, fields)
            })
            .collect();
        let cant = number_field(ui, rows, "Cant", handling.poses.cant);
        let held = held_rows(ui, root, &handling, label, bones);

        let actions = actions_of(&handling)
            .into_iter()
            .map(|(action, title)| action_row(ui, root, &handling, action, &title, label))
            .collect();
        Self { handling, kind, holds, cant, held, actions }
    }

    /// What was done this frame, the handling changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<HandlingChange> {
        if ui.changed(self.kind) {
            self.handling.kind = ui.selected_option(self.kind).checked_sub(1).map(|at| GunKind::ALL[at]);
            return Some(HandlingChange::Shape);
        }
        let mut typed = false;
        for (hold, fields) in &self.holds {
            if let Some(value) = changed_vector(ui, *fields) {
                *hold.slot(&mut self.handling.poses) = value;
                typed = true;
            }
        }
        if let Some(cant) = changed_number(ui, self.cant) {
            self.handling.poses.cant = cant;
            typed = true;
        }
        match self.update_held(ui) {
            Some(Some(change)) => return Some(change),
            Some(None) => typed = true,
            None => {}
        }
        for at in 0..self.actions.len() {
            match self.update_action(ui, at) {
                Some(Some(change)) => return Some(change),
                Some(None) => typed = true,
                None => {}
            }
        }
        typed.then_some(HandlingChange::Typed)
    }

    /// What was done to the clips the gun is held by: a change that is
    /// more than a value, or `Some(None)` for a value typed or picked.
    fn update_held(&mut self, ui: &mut Ui) -> Option<Option<HandlingChange>> {
        let (held, pack) = (&self.held, &mut self.handling.pack);
        for &(clip, slot) in &held.clips {
            if let Some(change) = clip_field(ui, slot, HandlingClip::Held(clip)) {
                return Some(Some(change));
            }
        }
        let mut typed = false;
        let (carrier, bones) = &held.carrier;
        if ui.changed(*carrier) {
            pack.carrier =
                ui.selected_option(*carrier).checked_sub(1).and_then(|at| bones.get(at)).cloned().unwrap_or_default();
            typed = true;
        }
        if let Some(at) = changed_vector(ui, held.carrier_at) {
            (pack.carrier_offset.at, typed) = (at, true);
        }
        if let Some(turn) = changed_vector(ui, held.carrier_turn) {
            (pack.carrier_offset.turn, typed) = (turn, true);
        }
        if let Some(eyes) = changed_vector(ui, held.eyes) {
            (pack.eyes, typed) = (eyes, true);
        }
        if ui.changed(held.camera) {
            pack.camera = CAMERAS[ui.selected_option(held.camera).min(CAMERAS.len() - 1)].0;
            typed = true;
        }
        let [look, moving, bob, breathing, head] = held.layers;
        for (field, value) in [
            (held.walk_speed, &mut pack.walk_speed),
            (held.run_speed, &mut pack.run_speed),
            (held.blend, &mut pack.blend),
            (held.hand_return, &mut pack.hand_return),
            (look, &mut pack.look_sway),
            (moving, &mut pack.move_sway),
            (bob, &mut pack.bob),
            (breathing, &mut pack.breathing),
            (head, &mut pack.head_bob),
        ] {
            if let Some(n) = changed_number(ui, field) {
                (*value, typed) = (n.max(0.0), true);
            }
        }
        typed.then_some(None)
    }

    /// What was done to an action's fields, as [`Self::update_held`] says.
    fn update_action(&mut self, ui: &mut Ui, at: usize) -> Option<Option<HandlingChange>> {
        let row = &self.actions[at];
        let action = row.action;
        if ui.changed(row.mode) {
            let mode = MODES.get(ui.selected_option(row.mode)).map_or(Mode::Built, |(mode, _)| *mode);
            set_mode(&mut self.handling, action, mode);
            return Some(Some(HandlingChange::Shape));
        }
        if let Some(change) = row.clip.and_then(|slot| clip_field(ui, slot, HandlingClip::Based(action))) {
            return Some(Some(change));
        }
        let mut typed = false;
        if let Some(speed) = row.speed.and_then(|field| changed_number(ui, field))
            && let HandlingAction::Builtin(kind) = action
        {
            offsets_of(&mut self.handling, kind).speed = speed.max(0.05);
            tidy(&mut self.handling);
            typed = true;
        }
        if let Some(based) = &row.based
            && let Some(base) = base_of(&mut self.handling, action)
        {
            typed |= based.update(ui, base);
        }
        if let Some(parts) = &row.parts {
            match update_parts(ui, parts, action, parts_of(&mut self.handling, action)) {
                Some(Some(change)) => {
                    tidy(&mut self.handling);
                    return Some(Some(change));
                }
                Some(None) => typed = true,
                None => {}
            }
        }
        typed.then_some(None)
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

/// What was done to an action's parts, as [`GunHandlingFields::update_held`]
/// says.
fn update_parts(
    ui: &mut Ui,
    parts: &Parts,
    action: HandlingAction,
    played: Option<&mut Vec<ClipPart>>,
) -> Option<Option<HandlingChange>> {
    let played = played?;
    if ui.clicked(parts.add) {
        played.push(ClipPart::default());
        return Some(Some(HandlingChange::Shape));
    }
    if let Some((from, to)) = collection::moved(ui, parts.items) {
        let moved = played.remove(from);
        played.insert(to, moved);
        return Some(Some(HandlingChange::Shape));
    }
    let mut typed = false;
    for (place, row) in parts.rows.iter().enumerate() {
        if ui.clicked(row.remove) {
            played.remove(place);
            return Some(Some(HandlingChange::Shape));
        }
        for (slot, field) in
            [(row.arms, HandlingClip::Arms(action, place)), (row.gun, HandlingClip::Gun(action, place))]
        {
            if let Some(change) = clip_field(ui, slot, field) {
                return Some(Some(change));
            }
        }
        let part = &mut played[place];
        if ui.clicked(row.add_mark) {
            part.marks.push(Mark { at: 0.0, event: Event::Eject });
            return Some(Some(HandlingChange::Shape));
        }
        if let Some((from, to)) = collection::moved(ui, row.mark_items) {
            let moved = part.marks.remove(from);
            part.marks.insert(to, moved);
            return Some(Some(HandlingChange::Shape));
        }
        for (field, value) in [(row.start, &mut part.start), (row.end, &mut part.end)] {
            if let Some(n) = changed_number(ui, field) {
                (*value, typed) = (n.max(0.0), true);
            }
        }
        if ui.changed(row.when)
            && let Some((fact, _)) = WHENS.get(ui.selected_option(row.when))
        {
            (part.when, typed) = (fact.map_or_else(When::default, |fact| When::all([fact])), true);
        }
        if ui.changed(row.repeat)
            && let Some((repeat, _)) = REPEATS.get(ui.selected_option(row.repeat))
        {
            (part.repeat, typed) = (*repeat, true);
        }
        for (mark, &(event, time, remove)) in row.marks.iter().enumerate() {
            if ui.clicked(remove) {
                part.marks.remove(mark);
                return Some(Some(HandlingChange::Shape));
            }
            let mark = &mut part.marks[mark];
            if ui.changed(event)
                && let Some((picked, _)) = EVENTS.get(ui.selected_option(event))
            {
                (mark.event, typed) = (*picked, true);
            }
            if let Some(n) = changed_number(ui, time) {
                (mark.at, typed) = (n.max(0.0), true);
            }
        }
    }
    typed.then_some(None)
}

/// A clip field clicked, or something dropped on it.
fn clip_field(ui: &mut Ui, slot: Slot, field: HandlingClip) -> Option<HandlingChange> {
    if let Some(source) = ui.dropped_onto(slot.field) {
        return Some(HandlingChange::Drop(field, source));
    }
    (ui.clicked(slot.arrow) || ui.clicked(slot.field)).then_some(HandlingChange::Browse(field, slot.field))
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

/// The clips the gun is held by, as a part of their own.
fn held_rows(
    ui: &mut Ui,
    root: NodeId,
    handling: &GunHandling,
    label: &dyn Fn(&str) -> String,
    bones: &dyn Fn(&str) -> Vec<String>,
) -> HeldRows {
    let pack = &handling.pack;
    let rows = part(ui, root, "Clips", "play");
    // The carrier is a bone of the arms clips' skeleton, offered from the first arms clip there is.
    let first_arms = [&pack.idle, &pack.walk, &pack.run, &pack.sprint, &pack.fire, &pack.aimed_fire]
        .into_iter()
        .map(String::as_str)
        .chain(handling.offsets.iter().flat_map(|offsets| offsets.parts.iter().map(|part| part.arms.as_str())))
        .chain(handling.actions.iter().flat_map(|own| own.parts.iter().map(|part| part.arms.as_str())))
        .find(|clip| !clip.is_empty());
    let mut named = first_arms.map(bones).unwrap_or_default();
    if !pack.carrier.is_empty() && !named.contains(&pack.carrier) {
        named.insert(0, pack.carrier.clone());
    }
    let options: Vec<&str> = std::iter::once("Auto").chain(named.iter().map(String::as_str)).collect();
    let picked = named.iter().position(|bone| *bone == pack.carrier).map_or(0, |at| at + 1);
    let carrier = (choice(ui, rows, "Carrier", &options, picked), named.clone());
    let carrier_at = vector(ui, rows, "Carrier At", pack.carrier_offset.at);
    let carrier_turn = vector(ui, rows, "Carrier Turn", pack.carrier_offset.turn);
    let eyes = vector(ui, rows, "Eyes", pack.eyes);
    let names: Vec<&str> = CAMERAS.iter().map(|(_, name)| *name).collect();
    let camera = choice(ui, rows, "Camera", &names, CAMERAS.iter().position(|(at, _)| *at == pack.camera).unwrap_or(0));
    let clips =
        HELD_CLIPS.iter().map(|&(clip, title)| (clip, clip_slot(ui, rows, title, clip.clip(pack), label))).collect();
    let walk_speed = number_field(ui, rows, "Walk Speed", pack.walk_speed);
    let run_speed = number_field(ui, rows, "Run Speed", pack.run_speed);
    let blend = number_field(ui, rows, "Blend", pack.blend);
    let hand_return = number_field(ui, rows, "Hand Return", pack.hand_return);
    let layers = [
        ("Look Sway", pack.look_sway),
        ("Move Sway", pack.move_sway),
        ("Bob", pack.bob),
        ("Breathing", pack.breathing),
        ("Head Bob", pack.head_bob),
    ]
    .map(|(name, value)| number_field(ui, rows, name, value));
    HeldRows {
        carrier,
        carrier_at,
        carrier_turn,
        eyes,
        camera,
        clips,
        walk_speed,
        run_speed,
        blend,
        hand_return,
        layers,
    }
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
    let mode = mode_of(handling, action);
    let titles = MODES.map(|(_, title)| title);
    let picked = MODES.iter().position(|(m, _)| *m == mode).unwrap_or(0);
    let mode_field = choice(ui, rows, "Mode", &titles, picked);
    let speed = match action {
        HandlingAction::Builtin(kind) => {
            let speed = handling.offsets(kind).map_or(1.0, |offsets| offsets.speed);
            Some(number_field(ui, rows, "Speed", speed))
        }
        HandlingAction::Own(_) => None,
    };
    let base = clip_of(handling, action);
    let clip =
        (mode == Mode::Based).then(|| clip_slot(ui, rows, "Clip", base.map_or("", |base| base.clip.as_str()), label));
    let based = base.filter(|base| mode == Mode::Based && !base.clip.is_empty()).map(|base| Based {
        start: number_field(ui, rows, "Start", base.start),
        end: number_field(ui, rows, "End", base.end),
        body: weight(ui, rows, "Body", base.body),
        gun: weight(ui, rows, "Gun", base.gun),
        arms: weight(ui, rows, "Arms", base.arms),
        blend: number_field(ui, rows, "Blend", base.blend),
    });
    let parts = (mode == Mode::Clips).then(|| parts_rows(ui, rows, played(handling, action), label));
    ActionRow { action, mode: mode_field, speed, clip, based, parts }
}

/// An action's parts, in a list, each with its marks in a list of its own.
fn parts_rows(ui: &mut Ui, rows: NodeId, parts: &[ClipPart], label: &dyn Fn(&str) -> String) -> Parts {
    let listed = prop(ui, rows, "Parts");
    let list = collection::collection(ui, listed, parts.len(), true, Some("Add Part"));
    let rows = parts
        .iter()
        .enumerate()
        .map(|(place, part)| {
            let item = collection::item(ui, list.items, place, true);
            let body = item.body;
            let arms = clip_slot(ui, body, "Arms", &part.arms, label);
            let gun = clip_slot(ui, body, "Gun", &part.gun, label);
            let start = number_field(ui, body, "Start", part.start);
            let end = number_field(ui, body, "End", part.end);
            let when = when_choice(ui, body, part.when);
            let repeat = repeat_choice(ui, body, part.repeat);
            let marked = prop(ui, body, "Marks");
            let marks_list = collection::collection(ui, marked, part.marks.len(), true, Some("Add Mark"));
            let marks = part
                .marks
                .iter()
                .enumerate()
                .map(|(at, mark)| {
                    let item = collection::item(ui, marks_list.items, at, true);
                    let event = event_choice(ui, item.body, mark.event);
                    let time = number_field(ui, item.body, "Time", mark.at);
                    (event, time, item.remove.expect("marks can be taken out"))
                })
                .collect();
            PartRow {
                remove: item.remove.expect("parts can be taken out"),
                arms,
                gun,
                start,
                end,
                when,
                repeat,
                mark_items: marks_list.items,
                add_mark: marks_list.add.expect("a part takes marks"),
                marks,
            }
        })
        .collect();
    Parts { items: list.items, add: list.add.expect("an action takes parts"), rows }
}

/// A part's When menu: one fact, or always; a When written otherwise by
/// hand shows as it is, after them, until another is picked.
fn when_choice(ui: &mut Ui, rows: NodeId, when: When) -> NodeId {
    let facts: Vec<Fact> = when.all.into();
    let shown = match (when.is_always(), facts.as_slice(), when.any.is_empty()) {
        (true, ..) => Some(0),
        (false, [fact], true) => WHENS.iter().position(|(f, _)| *f == Some(*fact)),
        _ => None,
    };
    let mut titles: Vec<&str> = WHENS.iter().map(|(_, title)| *title).collect();
    if shown.is_none() {
        titles.push("Written");
    }
    choice(ui, rows, "When", &titles, shown.unwrap_or(WHENS.len()))
}

fn repeat_choice(ui: &mut Ui, rows: NodeId, repeat: Repeat) -> NodeId {
    let mut titles: Vec<String> = REPEATS.iter().map(|(_, title)| title.to_string()).collect();
    let shown = REPEATS.iter().position(|(r, _)| *r == repeat).unwrap_or_else(|| {
        let Repeat::Times(times) = repeat else { return 0 };
        titles.push(format!("{times} Times"));
        REPEATS.len()
    });
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    choice(ui, rows, "Repeat", &titles, shown)
}

fn event_choice(ui: &mut Ui, rows: NodeId, event: Event) -> NodeId {
    let titles = EVENTS.map(|(_, title)| title);
    let shown = EVENTS
        .iter()
        .position(|(e, _)| *e == event || matches!((e, event), (Event::AddRounds(_), Event::AddRounds(_))))
        .unwrap_or(0);
    choice(ui, rows, "Event", &titles, shown)
}

/// How an action plays now.
fn mode_of(handling: &GunHandling, action: HandlingAction) -> Mode {
    match (played(handling, action).is_empty(), clip_of(handling, action)) {
        (false, _) => Mode::Clips,
        (true, Some(_)) => Mode::Based,
        (true, None) => Mode::Built,
    }
}

/// Has an action play as `mode` says: as built, with nothing on it; based
/// on a clip, none picked yet; or from clips, with a part to start with.
fn set_mode(handling: &mut GunHandling, action: HandlingAction, mode: Mode) {
    let based = (mode == Mode::Based).then(|| clip_of(handling, action).cloned().unwrap_or_default());
    match action {
        HandlingAction::Builtin(kind) => offsets_of(handling, kind).clip = based,
        HandlingAction::Own(at) => {
            if let Some(own) = handling.actions.get_mut(at) {
                own.clip = based;
            }
        }
    }
    if let Some(parts) = parts_of(handling, action) {
        match mode {
            Mode::Clips if parts.is_empty() => parts.push(ClipPart::default()),
            Mode::Clips => {}
            Mode::Built | Mode::Based => parts.clear(),
        }
    }
    tidy(handling);
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

/// The parts an action plays from clips, none when it plays otherwise.
fn played(handling: &GunHandling, action: HandlingAction) -> &[ClipPart] {
    match action {
        HandlingAction::Builtin(kind) => handling.offsets(kind).map_or(&[], |offsets| offsets.parts.as_slice()),
        HandlingAction::Own(at) => handling.actions.get(at).map_or(&[], |own| own.parts.as_slice()),
    }
}

/// An action's parts to change, a built-in's offsets made when it has
/// none.
fn parts_of(handling: &mut GunHandling, action: HandlingAction) -> Option<&mut Vec<ClipPart>> {
    match action {
        HandlingAction::Builtin(kind) => Some(&mut offsets_of(handling, kind).parts),
        HandlingAction::Own(at) => handling.actions.get_mut(at).map(|own| &mut own.parts),
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

/// Sets one of a handling's clip fields to the clip `clip` refers to, or
/// to none for an empty one: an action based on a clip keeps how much it
/// was followed, and with none plays as built again.
pub fn set_clip(handling: &mut GunHandling, field: HandlingClip, clip: String) {
    match field {
        HandlingClip::Based(action) => {
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
        }
        HandlingClip::Arms(action, place) | HandlingClip::Gun(action, place) => {
            let part = parts_of(handling, action).and_then(|parts| parts.get_mut(place));
            if let Some(part) = part {
                *if matches!(field, HandlingClip::Arms(..)) { &mut part.arms } else { &mut part.gun } = clip;
            }
        }
        HandlingClip::Held(held) => *held.slot(&mut handling.pack) = clip,
    }
    tidy(handling);
}

/// A clip field, showing the clip by its name and file.
fn clip_slot(ui: &mut Ui, rows: NodeId, name: &str, clip: &str, label: &dyn Fn(&str) -> String) -> Slot {
    let field = slot(ui, rows, name, "play");
    ui.set_text(field.label, &if clip.is_empty() { "None (Clip)".to_string() } else { label(clip) });
    ui.set_visible(field.icon, !clip.is_empty());
    field
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

fn vector(ui: &mut Ui, rows: NodeId, label: &str, value: [f32; 3]) -> [NodeId; 3] {
    let (_, fields) = vector_row(ui, rows, label);
    for (field, n) in fields.iter().zip(value) {
        ui.set_text(*field, &number(n));
    }
    fields
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

fn changed_vector(ui: &Ui, fields: [NodeId; 3]) -> Option<[f32; 3]> {
    if !fields.iter().any(|&field| ui.changed(field)) {
        return None;
    }
    let [x, y, z] = fields.map(|field| parsed(ui, field));
    Some([x?, y?, z?])
}

#[cfg(test)]
mod tests {
    use voxear_engine::guns::handling::CustomAction;

    use super::*;

    #[test]
    fn a_built_in_action_based_on_a_clip_keeps_how_much_it_was_followed_and_none_takes_it_away() {
        let mut handling = GunHandling::default();
        let reload = HandlingAction::Builtin(ActionKind::Reload);
        set_clip(&mut handling, HandlingClip::Based(reload), "Anims#clip:Reload".into());
        base_of(&mut handling, reload).unwrap().gun = 0.5;
        set_clip(&mut handling, HandlingClip::Based(reload), "Anims#clip:Reload_Fast".into());
        let base = clip_of(&handling, reload).unwrap();
        assert_eq!((base.clip.as_str(), base.gun), ("Anims#clip:Reload_Fast", 0.5));
        set_clip(&mut handling, HandlingClip::Based(reload), String::new());
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

    #[test]
    fn an_action_played_from_clips_starts_with_a_part_and_as_built_again_has_none() {
        let mut handling = GunHandling::default();
        let cycle = HandlingAction::Builtin(ActionKind::Cycle);
        set_mode(&mut handling, cycle, Mode::Clips);
        assert_eq!((mode_of(&handling, cycle), played(&handling, cycle).len()), (Mode::Clips, 1));
        set_clip(&mut handling, HandlingClip::Gun(cycle, 0), "Gun#clip:Cycle".into());
        assert_eq!(played(&handling, cycle)[0].gun, "Gun#clip:Cycle");
        set_mode(&mut handling, cycle, Mode::Based);
        assert_eq!(mode_of(&handling, cycle), Mode::Based, "based on a clip, none picked yet");
        set_mode(&mut handling, cycle, Mode::Built);
        assert!(handling.offsets.is_empty(), "as built, nothing is left on it");
        set_clip(&mut handling, HandlingClip::Held(HeldClip::Idle), "Arms#clip:idle".into());
        assert_eq!(handling.pack.idle, "Arms#clip:idle");
    }
}
