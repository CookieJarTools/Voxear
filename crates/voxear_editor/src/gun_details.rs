//! The Gun Editor's details, beside its view: the numbers and choices of
//! what is picked. With nothing picked, how the gun is held. With an
//! action showing, the action: a built-in one's offsets, or the whole of
//! one of the gun's own. With a beat picked on the timeline, the beat: a
//! built-in beat's offsets, each beside a button that puts back its
//! built-in value, or every part of a beat of the gun's own. Each row is
//! a label that drags its number, and its fields.

use voxear_engine::guns::{
    GunKind,
    actions::{ActionKind, Beat, BeatKey, Drive, Event, Goal, Hand, Knock, Repeat, Throw},
    handling::{ActionOffsets, BeatOffset, BeatSlot, CustomAction, GunHandling, Poses},
    hands::Grip,
    mechanism::PartRole,
    templates,
};
use voxear_ui::{NodeId, Style, Ui};

use crate::{gun_timeline::part_title, inspector};

/// What the details show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// How the gun is held.
    Poses,
    /// A built-in action's offsets, or one of its beats', by the beat's key.
    Builtin(ActionKind, Option<BeatKey>),
    /// An action of the gun's own, by its place among the handling's, or
    /// one of its beats, by its place among the action's.
    Own(usize, Option<usize>),
}

/// A row of the details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Hip,
    HipTurn,
    Shoulder,
    Aim,
    AimTurn,
    Cant,
    Work,
    WorkTurn,
    ReturnPace,
    LeftPouch,
    RightPouch,
    RestGrip,
    SupportGrip,
    GripHand,
    GripHandTurn,
    SupportHand,
    SupportHandTurn,
    Speed,
    Lift,
    LowersAim,
    Name,
    Replaces,
    Pace,
    Delay,
    Key,
    Hand,
    Goal,
    Reach,
    WorkTime,
    HandAt,
    HandTurn,
    Tilt,
    Grip,
    Then,
    Rounds,
    Knock,
    Throw,
    Repeat,
    Times,
    Drive,
    DriveFrom,
    DriveTo,
    Carry,
    Stow,
}

/// What a row's fields hold.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Number(f32),
    Vector([f32; 3]),
    Choice(usize),
    Text(String),
}

/// A button of the details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    /// Makes the built-in action an action of the gun's own in its place.
    EditableCopy,
    /// Takes away a built-in action's offsets.
    ResetAction,
    AddBeat,
    DeleteAction,
    DuplicateBeat,
    DeleteBeat,
    Earlier,
    Later,
}

impl Button {
    fn title(self) -> &'static str {
        match self {
            Button::EditableCopy => "Make Editable Copy",
            Button::ResetAction => "Reset to Built-in",
            Button::AddBeat => "Add Beat",
            Button::DeleteAction => "Delete Action",
            Button::DuplicateBeat => "Duplicate",
            Button::DeleteBeat => "Delete",
            Button::Earlier => "Earlier",
            Button::Later => "Later",
        }
    }
}

/// What the details were asked this frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DetailEdit {
    Set(Field, Value),
    Reset(Field),
    Press(Button),
}

const GRIPS: [(&str, Grip); 4] =
    [("Trigger", Grip::TRIGGER), ("Support", Grip::SUPPORT), ("Pinch", Grip::PINCH), ("Open", Grip::OPEN)];
const HANDS: [(&str, Hand); 2] = [("Right", Hand::Right), ("Left", Hand::Left)];
const KNOCKS: [(&str, Knock); 7] = [
    ("By Event", Knock::ByEvent),
    ("None", Knock::Nothing),
    ("Slam Home", Knock::SlamHome),
    ("Yank Back", Knock::YankBack),
    ("Press Down", Knock::PressDown),
    ("Seat", Knock::Seat),
    ("Slap Shut", Knock::SlapShut),
];
const THROWS: [(&str, Throw); 3] =
    [("None", Throw::Nothing), ("Empty Magazine", Throw::EmptyMagazine), ("Clip", Throw::Clip)];
const PARTS: [PartRole; 8] = [
    PartRole::Bolt,
    PartRole::Slide,
    PartRole::Toggle,
    PartRole::Pump,
    PartRole::Magazine,
    PartRole::Clip,
    PartRole::Cover,
    PartRole::Barrel,
];
const EVENTS: [&str; 10] =
    ["None", "Eject", "Chamber", "Unload", "Load", "Add Rounds", "Open", "Close", "See Feed", "See Chamber"];
const REPEATS: [&str; 5] = ["Once", "Per Clip", "Per Missing Round", "Per Chamber", "Times"];
const KEYS: [BeatKey; 32] = [
    BeatKey::Lift,
    BeatKey::Pull,
    BeatKey::Push,
    BeatKey::Turn,
    BeatKey::Open,
    BeatKey::Look,
    BeatKey::Close,
    BeatKey::TakeClip,
    BeatKey::ClipIn,
    BeatKey::Press,
    BeatKey::Release,
    BeatKey::MagOut,
    BeatKey::ToPouch,
    BeatKey::MagIn,
    BeatKey::Seat,
    BeatKey::Rack,
    BeatKey::Cock,
    BeatKey::Return,
    BeatKey::DrawOut,
    BeatKey::CoverUp,
    BeatKey::TakeBelt,
    BeatKey::LayBelt,
    BeatKey::CoverDown,
    BeatKey::TakeShell,
    BeatKey::ShellIn,
    BeatKey::Pump,
    BeatKey::Break,
    BeatKey::TakeRound,
    BeatKey::RoundIn,
    BeatKey::Spin,
    BeatKey::Tap,
    BeatKey::Wait,
];

/// How a field shows: a number, three, a menu of these, or a text.
enum Shape {
    Number,
    Vector,
    Choice(Vec<String>),
    Text,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Hip => "Hip",
            Field::HipTurn => "Hip Turn",
            Field::Shoulder => "Shoulder",
            Field::Aim => "Aim",
            Field::AimTurn => "Aim Turn",
            Field::Cant => "Cant",
            Field::Work | Field::WorkTime => "Work",
            Field::WorkTurn => "Work Turn",
            Field::ReturnPace => "Return Pace",
            Field::LeftPouch => "Left Pouch",
            Field::RightPouch => "Right Pouch",
            Field::RestGrip => "Rest Grip",
            Field::SupportGrip => "Support Grip",
            Field::GripHand => "Grip Hand",
            Field::GripHandTurn => "Grip Turn",
            Field::SupportHand => "Support Hand",
            Field::SupportHandTurn => "Support Turn",
            Field::Speed => "Speed",
            Field::Lift => "Lift",
            Field::LowersAim => "Lowers Aim",
            Field::Name => "Name",
            Field::Replaces => "Replaces",
            Field::Pace => "Pace",
            Field::Delay => "Delay",
            Field::Key => "Beat",
            Field::Hand => "Hand",
            Field::Goal => "Goal",
            Field::Reach => "Reach",
            Field::HandAt => "Hand",
            Field::HandTurn => "Hand Turn",
            Field::Tilt => "Tilt",
            Field::Grip => "Grip",
            Field::Then => "Then",
            Field::Rounds => "Rounds",
            Field::Knock => "Knock",
            Field::Throw => "Throw",
            Field::Repeat => "Repeat",
            Field::Times => "Times",
            Field::Drive => "Drives",
            Field::DriveFrom => "From",
            Field::DriveTo => "To",
            Field::Carry => "Carries",
            Field::Stow => "Stows",
        }
    }

    fn shape(self, target: &Target) -> Shape {
        let names = |names: &[&str]| Shape::Choice(names.iter().map(|n| n.to_string()).collect());
        let grips = |first: &str| {
            let mut names = vec![first.to_string()];
            names.extend(GRIPS.iter().map(|(name, _)| name.to_string()));
            Shape::Choice(names)
        };
        let parts = || {
            let mut names = vec!["None".to_string()];
            names.extend(PARTS.iter().map(|&part| part_title(part).to_string()));
            Shape::Choice(names)
        };
        match self {
            Field::Hip | Field::HipTurn | Field::Shoulder | Field::Aim | Field::AimTurn => Shape::Vector,
            Field::Work | Field::WorkTurn | Field::LeftPouch | Field::RightPouch => Shape::Vector,
            Field::HandAt | Field::HandTurn | Field::Tilt => Shape::Vector,
            Field::GripHand | Field::GripHandTurn | Field::SupportHand | Field::SupportHandTurn => Shape::Vector,
            Field::RestGrip | Field::SupportGrip => grips("Usual"),
            Field::Grip if matches!(target, Target::Builtin(..)) => grips("As Built"),
            Field::Grip => grips("Custom"),
            Field::LowersAim | Field::Stow => names(&["No", "Yes"]),
            Field::Name => Shape::Text,
            Field::Replaces => {
                let mut names = vec!["None".to_string()];
                names.extend(ActionKind::ALL.iter().map(|kind| kind.title().to_string()));
                Shape::Choice(names)
            }
            Field::Key => Shape::Choice(KEYS.iter().map(|key| key.title().to_string()).collect()),
            Field::Hand => names(&HANDS.map(|(name, _)| name)),
            Field::Goal => Shape::Choice(goals().into_iter().map(|(name, _)| name).collect()),
            Field::Then => names(&EVENTS),
            Field::Knock => names(&KNOCKS.map(|(name, _)| name)),
            Field::Throw => names(&THROWS.map(|(name, _)| name)),
            Field::Repeat => names(&REPEATS),
            Field::Drive | Field::Carry => parts(),
            _ => Shape::Number,
        }
    }
}

/// The goals a beat of the gun's own can take its hand to.
fn goals() -> Vec<(String, Goal)> {
    let mut goals: Vec<(String, Goal)> =
        PARTS.iter().map(|&part| (part_title(part).to_string(), Goal::Part(part))).collect();
    goals.extend([
        ("Feed".to_string(), Goal::Feed),
        ("Port".to_string(), Goal::Port),
        ("Pouch".to_string(), Goal::Pouch),
        ("Hold".to_string(), Goal::Hold),
    ]);
    goals
}

/// The rows for `target`, with the buttons under them.
fn rows(handling: &GunHandling, target: &Target) -> (Vec<Field>, Vec<Button>) {
    use Field::*;
    match target {
        Target::Poses => (
            vec![
                Hip,
                HipTurn,
                Shoulder,
                Aim,
                AimTurn,
                Cant,
                GripHand,
                GripHandTurn,
                RestGrip,
                SupportHand,
                SupportHandTurn,
                SupportGrip,
                Work,
                WorkTurn,
                ReturnPace,
                LeftPouch,
                RightPouch,
            ],
            Vec::new(),
        ),
        Target::Builtin(_, None) => {
            (vec![Speed, Lift, Work, WorkTurn], vec![Button::EditableCopy, Button::ResetAction])
        }
        Target::Builtin(_, Some(_)) => (vec![Pace, Delay, HandAt, HandTurn, Tilt, Grip], Vec::new()),
        Target::Own(_, None) => {
            (vec![Name, Replaces, Lift, LowersAim, Work, WorkTurn], vec![Button::AddBeat, Button::DeleteAction])
        }
        Target::Own(action, Some(beat)) => {
            let beat = handling.actions.get(*action).and_then(|own| own.beats.get(*beat));
            let mut fields = vec![Key, Hand, Goal, Reach, WorkTime, HandAt, HandTurn, Tilt, Grip, Then];
            if beat.is_some_and(|beat| matches!(beat.then, Event::AddRounds(_))) {
                fields.push(Rounds);
            }
            fields.extend([Knock, Throw, Repeat]);
            if beat.is_some_and(|beat| matches!(beat.repeat, self::Repeat::Times(_))) {
                fields.push(Times);
            }
            fields.push(Drive);
            if beat.is_some_and(|beat| beat.drive.is_some()) {
                fields.extend([DriveFrom, DriveTo]);
            }
            fields.extend([Carry, Stow]);
            (fields, vec![Button::DuplicateBeat, Button::DeleteBeat, Button::Earlier, Button::Later])
        }
    }
}

/// A row's fields.
enum Inputs {
    Number(NodeId),
    Vector([NodeId; 3]),
    Choice(NodeId),
    Text(NodeId),
}

struct Row {
    field: Field,
    inputs: Inputs,
    /// Puts back the built-in value; none for a field with none.
    reset: Option<NodeId>,
}

pub(crate) struct Details {
    root: NodeId,
    /// What the rows are for, and which fields they are.
    shown: Option<(Target, Vec<Field>)>,
    rows: Vec<Row>,
    buttons: Vec<(NodeId, Button)>,
}

impl Details {
    pub fn new(root: NodeId) -> Self {
        Self { root, shown: None, rows: Vec::new(), buttons: Vec::new() }
    }

    /// Shows `target` of `handling`, on a gun of `kind`: its rows made
    /// again when it is another, and its values written into the fields
    /// not being typed in.
    pub fn show(&mut self, ui: &mut Ui, handling: &GunHandling, kind: GunKind, target: &Target) {
        let (fields, buttons) = rows(handling, target);
        if self.shown.as_ref() != Some(&(target.clone(), fields.clone())) {
            self.build(ui, handling, target, &fields, &buttons);
            self.shown = Some((target.clone(), fields));
        }
        for row in &self.rows {
            let value = value(handling, kind, target, row.field);
            write(ui, &row.inputs, &value);
            if let Some(reset) = row.reset {
                let built = built(kind, target, row.field);
                ui.set_visible(reset, built.is_some_and(|built| built != value));
            }
        }
    }

    fn build(&mut self, ui: &mut Ui, handling: &GunHandling, target: &Target, fields: &[Field], buttons: &[Button]) {
        for child in ui.children(self.root) {
            ui.remove(child);
        }
        let title = ui.add_label(self.root, &title_of(handling, target));
        ui.add_class(title, "gun-details-title");
        self.rows = fields.iter().map(|&field| make_row(ui, self.root, target, field)).collect();
        let bar = ui.panel(self.root, Style::row());
        ui.add_class(bar, "gun-details-buttons");
        self.buttons = buttons
            .iter()
            .map(|&button| {
                let made = ui.button(bar, button.title());
                ui.add_class(made, "header-button");
                (made, button)
            })
            .collect();
    }

    /// What was typed, dragged, picked or pressed this frame.
    pub fn edits(&self, ui: &Ui) -> Vec<DetailEdit> {
        let mut edits = Vec::new();
        for row in &self.rows {
            if row.reset.is_some_and(|reset| ui.clicked(reset)) {
                edits.push(DetailEdit::Reset(row.field));
                continue;
            }
            if let Some(value) = read(ui, &row.inputs) {
                edits.push(DetailEdit::Set(row.field, value));
            }
        }
        edits.extend(self.buttons.iter().filter(|(node, _)| ui.clicked(*node)).map(|&(_, b)| DetailEdit::Press(b)));
        edits
    }

    /// Whether one of its fields is being typed in.
    pub fn typing(&self, ui: &Ui) -> bool {
        let focused = ui.focused();
        self.rows.iter().any(|row| match &row.inputs {
            Inputs::Number(field) | Inputs::Text(field) => focused == Some(*field),
            Inputs::Vector(fields) => fields.iter().any(|&field| focused == Some(field)),
            Inputs::Choice(_) => false,
        })
    }
}

/// What the details are headed with.
fn title_of(handling: &GunHandling, target: &Target) -> String {
    match target {
        Target::Poses => "Holding".into(),
        Target::Builtin(action, None) => format!("{} · Built-in", action.title()),
        Target::Builtin(action, Some(key)) => format!("{} · {}", action.title(), key.title()),
        Target::Own(action, None) => handling.actions.get(*action).map_or(String::new(), |own| own.name.clone()),
        Target::Own(action, Some(beat)) => {
            let own = handling.actions.get(*action);
            let key = own.and_then(|own| own.beats.get(*beat)).map_or("", |beat| beat.key.title());
            format!("{} · {key}", own.map_or("", |own| own.name.as_str()))
        }
    }
}

fn make_row(ui: &mut Ui, root: NodeId, target: &Target, field: Field) -> Row {
    let (row, inputs) = match field.shape(target) {
        Shape::Vector => {
            let (row, fields) = inspector::vector_row(ui, root, field.label());
            (row, Inputs::Vector(fields))
        }
        Shape::Number => {
            let row = inspector::prop(ui, root, field.label());
            let input = ui.text_field(row, "");
            ui.add_class(input, "value-input");
            ui.add_class(input, "number-input");
            (row, Inputs::Number(input))
        }
        Shape::Text => {
            let row = inspector::prop(ui, root, field.label());
            let input = ui.text_field(row, "");
            ui.add_class(input, "value-input");
            (row, Inputs::Text(input))
        }
        Shape::Choice(options) => {
            let row = inspector::prop(ui, root, field.label());
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            let menu = ui.dropdown(row, &labels, 0);
            ui.add_class(menu, "value-choice");
            ui.add_class(menu, "attr-dropdown");
            (row, Inputs::Choice(menu))
        }
    };
    let has_built = !matches!(target, Target::Own(..));
    let reset = has_built.then(|| {
        let reset = ui.icon_button(row, "undo");
        ui.add_class(reset, "gun-reset");
        ui.set_tooltip(reset, "Reset to built-in", None);
        reset
    });
    Row { field, inputs, reset }
}

fn write(ui: &mut Ui, inputs: &Inputs, value: &Value) {
    let put = |ui: &mut Ui, field: NodeId, text: &str| {
        if ui.focused() != Some(field) && ui.text(field) != text {
            ui.set_text(field, text);
        }
    };
    match (inputs, value) {
        (Inputs::Number(field), Value::Number(n)) => put(ui, *field, &inspector::number(*n)),
        (Inputs::Text(field), Value::Text(text)) => put(ui, *field, text),
        (Inputs::Vector(fields), Value::Vector(v)) => {
            for (field, n) in fields.iter().zip(v) {
                put(ui, *field, &inspector::number(*n));
            }
        }
        (Inputs::Choice(menu), Value::Choice(at)) if ui.selected_option(*menu) != *at => {
            ui.set_selected_option(*menu, *at);
        }
        _ => {}
    }
}

fn read(ui: &Ui, inputs: &Inputs) -> Option<Value> {
    let number = |field: NodeId| ui.text(field).trim().parse::<f32>().ok();
    match inputs {
        Inputs::Number(field) if ui.changed(*field) => number(*field).map(Value::Number),
        Inputs::Text(field) if ui.submitted(*field) => Some(Value::Text(ui.text(*field).trim().to_string())),
        Inputs::Vector(fields) if fields.iter().any(|&field| ui.changed(field)) => {
            let v = fields.map(number);
            Some(Value::Vector([v[0]?, v[1]?, v[2]?]))
        }
        Inputs::Choice(menu) if ui.changed(*menu) => Some(Value::Choice(ui.selected_option(*menu))),
        _ => None,
    }
}

fn grip_choice(grip: Option<Grip>) -> usize {
    match grip {
        None => 0,
        Some(grip) => GRIPS.iter().position(|(_, named)| *named == grip).map_or(0, |at| at + 1),
    }
}

fn grip_of(choice: usize) -> Option<Grip> {
    choice.checked_sub(1).and_then(|at| GRIPS.get(at)).map(|(_, grip)| *grip)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

fn less(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

/// The built-in beat of a gun of `kind`'s action with this key.
fn template_beat(kind: GunKind, action: ActionKind, key: BeatKey) -> Option<Beat> {
    templates::template(kind, action).beats.into_iter().find(|beat| beat.key == key)
}

/// What a field holds now.
pub(crate) fn value(handling: &GunHandling, kind: GunKind, target: &Target, field: Field) -> Value {
    match target {
        Target::Poses => pose_value(&handling.poses, field),
        Target::Builtin(action, None) => {
            let offsets = handling.offsets(*action).cloned().unwrap_or_default();
            let template = templates::template(kind, *action);
            match field {
                Field::Speed => Value::Number(offsets.speed),
                Field::Lift => Value::Number(offsets.lift.unwrap_or(template.lift)),
                Field::Work => Value::Vector(offsets.work.at),
                _ => Value::Vector(offsets.work.turn),
            }
        }
        Target::Builtin(action, Some(key)) => {
            let by = handling.offsets(*action).and_then(|o| o.beats.iter().find(|b| b.beat == BeatSlot::Known(*key)));
            let by = by.cloned().unwrap_or_default();
            let tilt = template_beat(kind, *action, *key).map_or([0.0; 3], |beat| beat.tilt);
            match field {
                Field::Pace => Value::Number(by.pace),
                Field::Delay => Value::Number(by.delay),
                Field::HandAt => Value::Vector(by.at),
                Field::HandTurn => Value::Vector(by.turn),
                Field::Tilt => Value::Vector(add(tilt, by.tilt)),
                _ => Value::Choice(grip_choice(by.grip)),
            }
        }
        Target::Own(action, None) => {
            let own = handling.actions.get(*action).cloned().unwrap_or_default();
            match field {
                Field::Name => Value::Text(own.name),
                Field::Replaces => Value::Choice(
                    own.replaces
                        .and_then(|kind| ActionKind::ALL.iter().position(|k| *k == kind))
                        .map_or(0, |at| at + 1),
                ),
                Field::Lift => Value::Number(own.lift),
                Field::LowersAim => Value::Choice(usize::from(own.lowers_aim)),
                Field::Work => Value::Vector(own.work.at),
                _ => Value::Vector(own.work.turn),
            }
        }
        Target::Own(action, Some(beat)) => {
            let beat = handling.actions.get(*action).and_then(|own| own.beats.get(*beat)).copied().unwrap_or_default();
            beat_value(&beat, field)
        }
    }
}

fn pose_value(poses: &Poses, field: Field) -> Value {
    match field {
        Field::Hip => Value::Vector(poses.hip.at),
        Field::HipTurn => Value::Vector(poses.hip.turn),
        Field::Shoulder => Value::Vector(poses.shoulder),
        Field::Aim => Value::Vector(poses.aim.at),
        Field::AimTurn => Value::Vector(poses.aim.turn),
        Field::Cant => Value::Number(poses.cant),
        Field::Work => Value::Vector(poses.work.at),
        Field::WorkTurn => Value::Vector(poses.work.turn),
        Field::ReturnPace => Value::Number(poses.return_pace),
        Field::LeftPouch => Value::Vector(poses.pouches[0]),
        Field::RightPouch => Value::Vector(poses.pouches[1]),
        Field::RestGrip => Value::Choice(grip_choice(poses.rest_grip)),
        Field::GripHand => Value::Vector(poses.grip.at),
        Field::GripHandTurn => Value::Vector(poses.grip.turn),
        Field::SupportHand => Value::Vector(poses.support.at),
        Field::SupportHandTurn => Value::Vector(poses.support.turn),
        _ => Value::Choice(grip_choice(poses.rest_support)),
    }
}

fn beat_value(beat: &Beat, field: Field) -> Value {
    let part_choice =
        |part: Option<PartRole>| part.and_then(|p| PARTS.iter().position(|&q| q == p)).map_or(0, |at| at + 1);
    match field {
        Field::Key => Value::Choice(KEYS.iter().position(|&key| key == beat.key).unwrap_or(0)),
        Field::Hand => Value::Choice(HANDS.iter().position(|(_, hand)| *hand == beat.hand).unwrap_or(0)),
        Field::Goal => Value::Choice(goals().iter().position(|(_, goal)| *goal == beat.goal).unwrap_or(0)),
        Field::Reach => Value::Number(beat.reach),
        Field::WorkTime => Value::Number(beat.work),
        Field::HandAt => Value::Vector(beat.at),
        Field::HandTurn => Value::Vector(beat.turn),
        Field::Tilt => Value::Vector(beat.tilt),
        Field::Grip => Value::Choice(grip_choice(Some(beat.grip))),
        Field::Then => Value::Choice(event_choice(beat.then)),
        Field::Rounds => Value::Number(match beat.then {
            Event::AddRounds(rounds) => rounds as f32,
            _ => 1.0,
        }),
        Field::Knock => Value::Choice(KNOCKS.iter().position(|(_, knock)| *knock == beat.knock).unwrap_or(0)),
        Field::Throw => Value::Choice(THROWS.iter().position(|(_, throw)| *throw == beat.throw).unwrap_or(0)),
        Field::Repeat => Value::Choice(match beat.repeat {
            Repeat::Once => 0,
            Repeat::PerClip => 1,
            Repeat::PerMissingRound => 2,
            Repeat::PerChamber => 3,
            Repeat::Times(_) => 4,
        }),
        Field::Times => Value::Number(match beat.repeat {
            Repeat::Times(times) => f32::from(times),
            _ => 1.0,
        }),
        Field::Drive => Value::Choice(part_choice(beat.drive.map(|drive| drive.part))),
        Field::DriveFrom => Value::Number(beat.drive.map_or(0.0, |drive| drive.from)),
        Field::DriveTo => Value::Number(beat.drive.map_or(1.0, |drive| drive.to)),
        Field::Carry => Value::Choice(part_choice(beat.carry)),
        _ => Value::Choice(usize::from(beat.stow)),
    }
}

fn event_choice(event: Event) -> usize {
    match event {
        Event::None => 0,
        Event::Eject => 1,
        Event::Chamber => 2,
        Event::Unload => 3,
        Event::Load => 4,
        Event::AddRounds(_) => 5,
        Event::Open => 6,
        Event::Close => 7,
        Event::SeeFeed => 8,
        Event::SeeChamber => 9,
    }
}

fn event_of(choice: usize, rounds: u32) -> Event {
    match choice {
        1 => Event::Eject,
        2 => Event::Chamber,
        3 => Event::Unload,
        4 => Event::Load,
        5 => Event::AddRounds(rounds.max(1)),
        6 => Event::Open,
        7 => Event::Close,
        8 => Event::SeeFeed,
        9 => Event::SeeChamber,
        _ => Event::None,
    }
}

/// A field's built-in value, none for one with none: an offset's is
/// nothing, and a built-in beat's tilt is the template's.
pub(crate) fn built(kind: GunKind, target: &Target, field: Field) -> Option<Value> {
    match target {
        Target::Poses => Some(pose_value(&Poses::default(), field)),
        Target::Builtin(action, None) => {
            let handling = GunHandling::default();
            Some(value(&handling, kind, &Target::Builtin(*action, None), field))
        }
        Target::Builtin(action, Some(key)) => {
            let handling = GunHandling::default();
            Some(value(&handling, kind, &Target::Builtin(*action, Some(*key)), field))
        }
        Target::Own(..) => None,
    }
}

/// Puts `value` in a field of `handling`. A built-in action's offsets that
/// are all nothing are taken out, so the file keeps only what was changed.
pub(crate) fn set(handling: &mut GunHandling, kind: GunKind, target: &Target, field: Field, value: Value) {
    match target {
        Target::Poses => set_pose(&mut handling.poses, field, value),
        Target::Builtin(action, beat) => {
            let at = match handling.offsets.iter().position(|o| o.action == *action) {
                Some(at) => at,
                None => {
                    handling.offsets.push(ActionOffsets { action: *action, ..ActionOffsets::default() });
                    handling.offsets.len() - 1
                }
            };
            let offsets = &mut handling.offsets[at];
            match beat {
                None => set_action_offset(offsets, kind, field, value),
                Some(key) => set_beat_offset(offsets, kind, *key, field, value),
            }
            if handling.offsets[at] == (ActionOffsets { action: *action, ..ActionOffsets::default() }) {
                handling.offsets.remove(at);
            }
        }
        Target::Own(action, None) => {
            if let Some(own) = handling.actions.get_mut(*action) {
                set_own_action(own, field, value);
            }
        }
        Target::Own(action, Some(beat)) => {
            if let Some(beat) = handling.actions.get_mut(*action).and_then(|own| own.beats.get_mut(*beat)) {
                set_beat(beat, field, value);
            }
        }
    }
}

fn set_pose(poses: &mut Poses, field: Field, value: Value) {
    match (field, value) {
        (Field::Hip, Value::Vector(v)) => poses.hip.at = v,
        (Field::HipTurn, Value::Vector(v)) => poses.hip.turn = v,
        (Field::Shoulder, Value::Vector(v)) => poses.shoulder = v,
        (Field::Aim, Value::Vector(v)) => poses.aim.at = v,
        (Field::AimTurn, Value::Vector(v)) => poses.aim.turn = v,
        (Field::Cant, Value::Number(n)) => poses.cant = n,
        (Field::Work, Value::Vector(v)) => poses.work.at = v,
        (Field::WorkTurn, Value::Vector(v)) => poses.work.turn = v,
        (Field::ReturnPace, Value::Number(n)) => poses.return_pace = n.max(0.05),
        (Field::LeftPouch, Value::Vector(v)) => poses.pouches[0] = v,
        (Field::RightPouch, Value::Vector(v)) => poses.pouches[1] = v,
        (Field::RestGrip, Value::Choice(at)) => poses.rest_grip = grip_of(at),
        (Field::SupportGrip, Value::Choice(at)) => poses.rest_support = grip_of(at),
        (Field::GripHand, Value::Vector(v)) => poses.grip.at = v,
        (Field::GripHandTurn, Value::Vector(v)) => poses.grip.turn = v,
        (Field::SupportHand, Value::Vector(v)) => poses.support.at = v,
        (Field::SupportHandTurn, Value::Vector(v)) => poses.support.turn = v,
        _ => {}
    }
}

fn set_action_offset(offsets: &mut ActionOffsets, kind: GunKind, field: Field, value: Value) {
    let template = templates::template(kind, offsets.action);
    match (field, value) {
        (Field::Speed, Value::Number(n)) => offsets.speed = n.max(0.05),
        (Field::Lift, Value::Number(n)) => offsets.lift = (n != template.lift).then_some(n),
        (Field::Work, Value::Vector(v)) => offsets.work.at = v,
        (Field::WorkTurn, Value::Vector(v)) => offsets.work.turn = v,
        _ => {}
    }
}

fn set_beat_offset(offsets: &mut ActionOffsets, kind: GunKind, key: BeatKey, field: Field, value: Value) {
    let slot = BeatSlot::Known(key);
    let at = match offsets.beats.iter().position(|b| b.beat == slot) {
        Some(at) => at,
        None => {
            offsets.beats.push(BeatOffset { beat: slot.clone(), ..BeatOffset::default() });
            offsets.beats.len() - 1
        }
    };
    let tilt = template_beat(kind, offsets.action, key).map_or([0.0; 3], |beat| beat.tilt);
    let by = &mut offsets.beats[at];
    match (field, value) {
        (Field::Pace, Value::Number(n)) => by.pace = n.max(0.05),
        (Field::Delay, Value::Number(n)) => by.delay = n,
        (Field::HandAt, Value::Vector(v)) => by.at = v,
        (Field::HandTurn, Value::Vector(v)) => by.turn = v,
        (Field::Tilt, Value::Vector(v)) => by.tilt = less(v, tilt),
        (Field::Grip, Value::Choice(choice)) => by.grip = grip_of(choice),
        _ => {}
    }
    if offsets.beats[at] == (BeatOffset { beat: slot, ..BeatOffset::default() }) {
        offsets.beats.remove(at);
    }
}

fn set_own_action(own: &mut CustomAction, field: Field, value: Value) {
    match (field, value) {
        (Field::Name, Value::Text(name)) if !name.is_empty() => own.name = name,
        (Field::Replaces, Value::Choice(at)) => {
            own.replaces = at.checked_sub(1).and_then(|at| ActionKind::ALL.get(at)).copied()
        }
        (Field::Lift, Value::Number(n)) => own.lift = n,
        (Field::LowersAim, Value::Choice(at)) => own.lowers_aim = at == 1,
        (Field::Work, Value::Vector(v)) => own.work.at = v,
        (Field::WorkTurn, Value::Vector(v)) => own.work.turn = v,
        _ => {}
    }
}

fn set_beat(beat: &mut Beat, field: Field, value: Value) {
    let part = |at: usize| at.checked_sub(1).and_then(|at| PARTS.get(at)).copied();
    match (field, value) {
        (Field::Key, Value::Choice(at)) => beat.key = KEYS.get(at).copied().unwrap_or_default(),
        (Field::Hand, Value::Choice(at)) => beat.hand = HANDS.get(at).map_or(Hand::Right, |(_, hand)| *hand),
        (Field::Goal, Value::Choice(at)) => beat.goal = goals().get(at).map_or(Goal::Hold, |(_, goal)| *goal),
        (Field::Reach, Value::Number(n)) => beat.reach = n.max(0.0),
        (Field::WorkTime, Value::Number(n)) => beat.work = n.max(0.0),
        (Field::HandAt, Value::Vector(v)) => beat.at = v,
        (Field::HandTurn, Value::Vector(v)) => beat.turn = v,
        (Field::Tilt, Value::Vector(v)) => beat.tilt = v,
        (Field::Grip, Value::Choice(at)) => beat.grip = grip_of(at).unwrap_or(beat.grip),
        (Field::Then, Value::Choice(at)) => {
            let rounds = if let Event::AddRounds(rounds) = beat.then { rounds } else { 1 };
            beat.then = event_of(at, rounds);
        }
        (Field::Rounds, Value::Number(n)) => beat.then = Event::AddRounds(n.round().max(1.0) as u32),
        (Field::Knock, Value::Choice(at)) => beat.knock = KNOCKS.get(at).map_or(Knock::ByEvent, |(_, k)| *k),
        (Field::Throw, Value::Choice(at)) => beat.throw = THROWS.get(at).map_or(Throw::Nothing, |(_, t)| *t),
        (Field::Repeat, Value::Choice(at)) => {
            let times = if let Repeat::Times(times) = beat.repeat { times } else { 2 };
            beat.repeat =
                [Repeat::Once, Repeat::PerClip, Repeat::PerMissingRound, Repeat::PerChamber, Repeat::Times(times)]
                    .get(at)
                    .copied()
                    .unwrap_or_default();
        }
        (Field::Times, Value::Number(n)) => beat.repeat = Repeat::Times(n.round().clamp(1.0, 255.0) as u8),
        (Field::Drive, Value::Choice(at)) => {
            let (from, to) = beat.drive.map_or((0.0, 1.0), |drive| (drive.from, drive.to));
            beat.drive = part(at).map(|part| Drive { part, from, to });
        }
        (Field::DriveFrom, Value::Number(n)) => {
            if let Some(drive) = beat.drive.as_mut() {
                drive.from = n.clamp(0.0, 1.0);
            }
        }
        (Field::DriveTo, Value::Number(n)) => {
            if let Some(drive) = beat.drive.as_mut() {
                drive.to = n.clamp(0.0, 1.0);
            }
        }
        (Field::Carry, Value::Choice(at)) => beat.carry = part(at),
        (Field::Stow, Value::Choice(at)) => beat.stow = at == 1,
        _ => {}
    }
}

/// Makes a beat `length` seconds long: a built-in one by its pace, after
/// its delay, and one of the gun's own by its reach and work alike.
pub(crate) fn retime(handling: &mut GunHandling, kind: GunKind, target: &Target, length: f32) {
    match target {
        Target::Builtin(action, Some(key)) => {
            let Some(beat) = template_beat(kind, *action, *key) else { return };
            let speed = handling.offsets(*action).map_or(1.0, |offsets| offsets.speed.max(0.05));
            let delay = match value(handling, kind, target, Field::Delay) {
                Value::Number(delay) => delay,
                _ => 0.0,
            };
            let pace = beat.length() / speed / (length - delay).max(0.02);
            set(handling, kind, target, Field::Pace, Value::Number(pace));
        }
        Target::Own(action, Some(beat)) => {
            let Some(beat) = handling.actions.get_mut(*action).and_then(|own| own.beats.get_mut(*beat)) else { return };
            let scale = length.max(0.02) / beat.length().max(0.02);
            (beat.reach, beat.work) = (beat.reach * scale, beat.work * scale);
        }
        _ => {}
    }
}

/// Where a beat starts from, as dragging it along moves it: a built-in
/// one's delay, and one of the gun's own's reach.
pub(crate) fn start_of(handling: &GunHandling, kind: GunKind, target: &Target) -> f32 {
    match (target, value(handling, kind, target, Field::Delay)) {
        (Target::Builtin(..), Value::Number(delay)) => delay,
        (Target::Own(..), _) => match value(handling, kind, target, Field::Reach) {
            Value::Number(reach) => reach,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

/// Starts a beat `start` on from where [`start_of`] has it: a built-in
/// one's delay never taking more than its whole reach.
pub(crate) fn shift(handling: &mut GunHandling, kind: GunKind, target: &Target, start: f32) {
    match target {
        Target::Builtin(action, Some(key)) => {
            let reach = handling.spec(kind, *action).beats.iter().find(|b| b.key == *key).map_or(0.0, |b| b.reach);
            let delay = match value(handling, kind, target, Field::Delay) {
                Value::Number(delay) => delay,
                _ => 0.0,
            };
            let least = delay - reach + 0.01;
            set(handling, kind, target, Field::Delay, Value::Number(start.max(least)));
        }
        Target::Own(..) => set(handling, kind, target, Field::Reach, Value::Number(start.max(0.0))),
        _ => {}
    }
}

/// Puts a field back as built.
pub(crate) fn reset(handling: &mut GunHandling, kind: GunKind, target: &Target, field: Field) {
    if let Some(built) = built(kind, target, field) {
        set(handling, kind, target, field, built);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_put_back_as_built_leaves_nothing_in_the_file() {
        let kind = GunKind::BoltActionRifle;
        let mut handling = GunHandling::default();
        let beat = Target::Builtin(ActionKind::Reload, Some(BeatKey::Press));
        set(&mut handling, kind, &beat, Field::Pace, Value::Number(1.25));
        let tilt = template_beat(kind, ActionKind::Reload, BeatKey::Press).unwrap().tilt;
        set(&mut handling, kind, &beat, Field::Tilt, Value::Vector(add(tilt, [-4.0, 0.0, 2.0])));
        let by = &handling.offsets(ActionKind::Reload).unwrap().beats[0];
        assert_eq!((by.pace, by.tilt), (1.25, [-4.0, 0.0, 2.0]), "the tilt shown is the built-in's and the offset's");
        assert_eq!(value(&handling, kind, &beat, Field::Tilt), Value::Vector(add(tilt, [-4.0, 0.0, 2.0])));
        reset(&mut handling, kind, &beat, Field::Pace);
        reset(&mut handling, kind, &beat, Field::Tilt);
        assert!(handling.offsets.is_empty(), "{:?}", handling.offsets);
        let action = Target::Builtin(ActionKind::Cycle, None);
        set(&mut handling, kind, &action, Field::Lift, Value::Number(0.4));
        assert!(handling.offsets.is_empty(), "a lift as built is no offset");
        set(&mut handling, kind, &action, Field::Lift, Value::Number(0.7));
        assert_eq!(handling.offsets(ActionKind::Cycle).unwrap().lift, Some(0.7));
    }

    #[test]
    fn a_beat_of_the_guns_own_takes_every_value_its_rows_offer() {
        let mut beat = Beat::default();
        set_beat(&mut beat, Field::Then, Value::Choice(5));
        set_beat(&mut beat, Field::Rounds, Value::Number(5.0));
        set_beat(&mut beat, Field::Repeat, Value::Choice(4));
        set_beat(&mut beat, Field::Times, Value::Number(3.0));
        set_beat(&mut beat, Field::Drive, Value::Choice(1));
        set_beat(&mut beat, Field::DriveTo, Value::Number(0.6));
        set_beat(&mut beat, Field::Goal, Value::Choice(4));
        assert_eq!(beat.then, Event::AddRounds(5));
        assert_eq!(beat.repeat, Repeat::Times(3));
        assert_eq!(beat.drive, Some(Drive { part: PartRole::Bolt, from: 0.0, to: 0.6 }));
        assert_eq!(beat.goal, Goal::Part(PartRole::Magazine));
        for field in [Field::Then, Field::Rounds, Field::Repeat, Field::Times, Field::Drive, Field::Goal] {
            let shown = beat_value(&beat, field);
            let mut again = beat;
            set_beat(&mut again, field, shown);
            assert_eq!(again, beat, "{field:?} reads back as it was set");
        }
    }
}
