//! Reloads, bolts and checks with no clips: each is a run of beats, a hand
//! reaching somewhere and doing one thing there: lifting a bolt, pulling a
//! magazine, pressing rounds down from a clip. A beat that drives a part
//! moves the hand with it, so the part's own travel is the hand's path.
//! What a beat does to the gun, a round chambered or a magazine seated,
//! happens as the beat ends. Beats are data: each kind of gun's built-in
//! actions are templates (see [`super::templates`]), whose beats may play
//! only in some states or repeat, worked out from the gun's state as the
//! action starts.

use serde::{Deserialize, Serialize};

use super::base_clip::BaseClip;
use super::gun::GunKind;
use super::handling::Place;
use super::hands::Grip;
use super::mechanism::{Chamber, Feed, GunState, Known, PartRole};

/// What a holder can do with a gun.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Reload,
    /// Works the bolt, slide or pump by hand.
    Cycle,
    CheckMagazine,
    CheckChamber,
}

impl ActionKind {
    pub const ALL: [ActionKind; 4] =
        [ActionKind::Reload, ActionKind::Cycle, ActionKind::CheckMagazine, ActionKind::CheckChamber];

    /// Its name, as the editor shows it.
    pub fn title(self) -> &'static str {
        match self {
            ActionKind::Reload => "Reload",
            ActionKind::Cycle => "Cycle",
            ActionKind::CheckMagazine => "Check Magazine",
            ActionKind::CheckChamber => "Check Chamber",
        }
    }
}

/// An action a holder is asked to do: one of every gun's built-in ones,
/// or one of the gun's own, by its name in the gun's handling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionId {
    Builtin(ActionKind),
    Own(std::sync::Arc<str>),
}

impl From<ActionKind> for ActionId {
    fn from(kind: ActionKind) -> Self {
        ActionId::Builtin(kind)
    }
}

/// A hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    Left,
    #[default]
    Right,
}

/// Where a beat takes a hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// A part's handle, moving with the part.
    Part(PartRole),
    /// Where rounds go in: the magazine well or clip guide.
    Feed,
    /// The ejection port.
    Port,
    /// A pouch on the body, where spare magazines, clips and rounds are.
    Pouch,
    /// Back where the hand holds the gun.
    #[default]
    Hold,
}

/// What a beat does to the gun as it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    #[default]
    None,
    /// The chambered round or case goes out of the port.
    Eject,
    /// The next round goes into the chamber from the feed.
    Chamber,
    /// The magazine comes out, or the clip or belt.
    Unload,
    /// A full magazine, clip or belt goes in.
    Load,
    /// Rounds pressed or put in one at a time.
    AddRounds(u32),
    /// The action is held open, as an open bolt cocked.
    Open,
    /// The action closes.
    Close,
    /// The holder sees how much is in the feed, or the chamber.
    SeeFeed,
    SeeChamber,
}

/// What a beat is, as the timeline names it: each built-in beat has one,
/// which is how offsets find it, and a custom beat picks one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatKey {
    Lift,
    Pull,
    Push,
    Turn,
    Open,
    Look,
    Close,
    TakeClip,
    ClipIn,
    Press,
    Release,
    MagOut,
    ToPouch,
    MagIn,
    Seat,
    Rack,
    Cock,
    Return,
    DrawOut,
    CoverUp,
    TakeBelt,
    LayBelt,
    CoverDown,
    TakeShell,
    ShellIn,
    Pump,
    Break,
    TakeRound,
    RoundIn,
    Spin,
    Tap,
    /// A part of an action played from clips.
    Clip,
    #[default]
    Wait,
}

impl BeatKey {
    /// Its name, as the timeline shows it.
    pub fn title(self) -> &'static str {
        match self {
            BeatKey::Lift => "Lift",
            BeatKey::Pull => "Pull",
            BeatKey::Push => "Push",
            BeatKey::Turn => "Turn",
            BeatKey::Open => "Open",
            BeatKey::Look => "Look",
            BeatKey::Close => "Close",
            BeatKey::TakeClip => "Take Clip",
            BeatKey::ClipIn => "Clip In",
            BeatKey::Press => "Press",
            BeatKey::Release => "Release",
            BeatKey::MagOut => "Magazine Out",
            BeatKey::ToPouch => "To Pouch",
            BeatKey::MagIn => "Magazine In",
            BeatKey::Seat => "Seat",
            BeatKey::Rack => "Rack",
            BeatKey::Cock => "Cock",
            BeatKey::Return => "Return",
            BeatKey::DrawOut => "Draw Out",
            BeatKey::CoverUp => "Cover Up",
            BeatKey::TakeBelt => "Take Belt",
            BeatKey::LayBelt => "Lay Belt",
            BeatKey::CoverDown => "Cover Down",
            BeatKey::TakeShell => "Take Shell",
            BeatKey::ShellIn => "Shell In",
            BeatKey::Pump => "Pump",
            BeatKey::Break => "Break Open",
            BeatKey::TakeRound => "Take Round",
            BeatKey::RoundIn => "Round In",
            BeatKey::Spin => "Spin",
            BeatKey::Tap => "Tap",
            BeatKey::Clip => "Clip",
            BeatKey::Wait => "Wait",
        }
    }
}

/// A part a beat moves, from one place to another, its hand riding it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drive {
    pub part: PartRole,
    pub from: f32,
    pub to: f32,
}

/// The knock a beat gives the gun as it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knock {
    /// As its event has it: a round chambered slams the bolt home, a case
    /// ejected yanks it back, a magazine or belt loaded seats it.
    #[default]
    ByEvent,
    Nothing,
    SlamHome,
    YankBack,
    PressDown,
    Seat,
    SlapShut,
}

impl Knock {
    /// The knock a beat with this knock and `event` gives.
    pub fn of(self, event: Event) -> Knock {
        match (self, event) {
            (Knock::ByEvent, Event::Chamber) => Knock::SlamHome,
            (Knock::ByEvent, Event::Eject) => Knock::YankBack,
            (Knock::ByEvent, Event::Load) => Knock::Seat,
            (Knock::ByEvent, _) => Knock::Nothing,
            (knock, _) => knock,
        }
    }
}

/// What a beat throws away as it ends: the magazine it took out, when
/// that was empty, or the stripper clip the bolt closed over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Throw {
    #[default]
    Nothing,
    EmptyMagazine,
    Clip,
}

/// Something true or not of a gun as an action starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fact {
    Fed,
    NotFed,
    ChamberLive,
    ChamberNotLive,
    Open,
    Closed,
    Full,
    NotFull,
    Spare,
    NoSpare,
    /// Something in the chamber, a live round or a spent case.
    ChamberFilled,
    ChamberEmpty,
}

impl Fact {
    pub const ALL: [Fact; 12] = [
        Fact::Fed,
        Fact::NotFed,
        Fact::ChamberLive,
        Fact::ChamberNotLive,
        Fact::Open,
        Fact::Closed,
        Fact::Full,
        Fact::NotFull,
        Fact::Spare,
        Fact::NoSpare,
        Fact::ChamberFilled,
        Fact::ChamberEmpty,
    ];

    /// Whether it is so of a gun of `kind` in `state`, its holder having
    /// `spare` rounds. Full is all the gun holds, its chamber included.
    pub fn holds(self, kind: GunKind, state: &GunState, spare: u32) -> bool {
        let missing = kind.capacity().saturating_sub(state.rounds);
        let full = missing == 0 && state.fed && (state.chamber == Chamber::Live || kind.open_bolt());
        match self {
            Fact::Fed => state.fed,
            Fact::NotFed => !state.fed,
            Fact::ChamberLive => state.chamber == Chamber::Live,
            Fact::ChamberNotLive => state.chamber != Chamber::Live,
            Fact::Open => state.open,
            Fact::Closed => !state.open,
            Fact::Full => full,
            Fact::NotFull => !full,
            Fact::Spare => spare > 0,
            Fact::NoSpare => spare == 0,
            Fact::ChamberFilled => state.chamber != Chamber::Empty,
            Fact::ChamberEmpty => state.chamber == Chamber::Empty,
        }
    }
}

/// A set of facts, written as a list of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Vec<Fact>", into = "Vec<Fact>")]
pub struct Facts(u16);

impl Facts {
    fn has(self, fact: Fact) -> bool {
        self.0 & (1 << fact as u16) != 0
    }

    fn iter(self) -> impl Iterator<Item = Fact> {
        Fact::ALL.into_iter().filter(move |&fact| self.has(fact))
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl From<Vec<Fact>> for Facts {
    fn from(facts: Vec<Fact>) -> Self {
        Facts(facts.into_iter().fold(0, |bits, fact| bits | 1 << fact as u16))
    }
}

impl From<Facts> for Vec<Fact> {
    fn from(facts: Facts) -> Self {
        facts.iter().collect()
    }
}

/// When a beat or action plays: every one of `all`, and at least one of
/// `any` if it names any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct When {
    #[serde(skip_serializing_if = "Facts::is_empty")]
    pub all: Facts,
    #[serde(skip_serializing_if = "Facts::is_empty")]
    pub any: Facts,
}

impl When {
    pub fn all(facts: impl IntoIterator<Item = Fact>) -> Self {
        When { all: Facts::from(facts.into_iter().collect::<Vec<_>>()), any: Facts::default() }
    }

    pub fn any(facts: impl IntoIterator<Item = Fact>) -> Self {
        When { all: Facts::default(), any: Facts::from(facts.into_iter().collect::<Vec<_>>()) }
    }

    pub fn is_always(&self) -> bool {
        self.all.is_empty() && self.any.is_empty()
    }

    pub fn holds(&self, kind: GunKind, state: &GunState, spare: u32) -> bool {
        self.all.iter().all(|fact| fact.holds(kind, state, spare))
            && (self.any.is_empty() || self.any.iter().any(|fact| fact.holds(kind, state, spare)))
    }
}

/// How many times a beat plays, with the beats either side of it that
/// repeat the same way: once for each stripper clip the gun takes, each
/// round it lacks, each chamber of its cylinder, a set number of times, or
/// again and again until the rounds the beats before and the beats so far
/// put in fill the feed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    Once,
    PerClip,
    PerMissingRound,
    PerChamber,
    Times(u8),
    UntilFull,
}

impl Repeat {
    /// How many times, for a gun of `kind` in `state` with `spare` rounds.
    pub fn count(self, kind: GunKind, state: &GunState, spare: u32) -> u32 {
        let missing = kind.capacity().saturating_sub(state.rounds);
        match self {
            Repeat::Once => 1,
            Repeat::PerClip => {
                let size = if let Feed::Clips(size) = kind.feed() { size.max(1) } else { 1 };
                missing.min(spare).div_ceil(size).max(1)
            }
            Repeat::PerMissingRound => missing.min(spare),
            Repeat::PerChamber => kind.capacity().min(spare + state.rounds + u32::from(state.chamber == Chamber::Live)),
            Repeat::Times(times) => u32::from(times),
            Repeat::UntilFull => missing.min(spare),
        }
    }
}

/// One thing a hand does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Beat {
    pub key: BeatKey,
    pub hand: Hand,
    pub goal: Goal,
    /// Seconds to reach the goal, then to do the work there.
    pub reach: f32,
    pub work: f32,
    /// A part the work moves, from one place to another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive: Option<Drive>,
    /// A part the hand carries while the beat lasts, as a magazine out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carry: Option<PartRole>,
    /// Whether the part it carries is put away in a pouch: the hand goes
    /// empty if that part was thrown away instead.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stow: bool,
    /// Where its hand holds, from its goal, in the gun's space, in metres,
    /// and how much more the hand is turned, in degrees about X, Y and Z.
    #[serde(skip_serializing_if = "is_nothing")]
    pub at: [f32; 3],
    #[serde(skip_serializing_if = "is_nothing")]
    pub turn: [f32; 3],
    /// How the gun is held meanwhile, in degrees: rolled, its top to the
    /// right; turned, its muzzle to the right; and tipped, its muzzle up.
    pub tilt: [f32; 3],
    pub grip: Grip,
    pub then: Event,
    pub knock: Knock,
    pub throw: Throw,
    #[serde(skip_serializing_if = "When::is_always")]
    pub when: When,
    pub repeat: Repeat,
    /// For a part of an action played from clips, which of its spec's
    /// parts it is (see [`ActionSpec::played`]).
    #[serde(skip)]
    pub played: Option<u16>,
}

impl Default for Beat {
    fn default() -> Self {
        Self::new(BeatKey::Wait, Hand::Right, Goal::Hold)
    }
}

impl Beat {
    pub(super) fn new(key: BeatKey, hand: Hand, goal: Goal) -> Self {
        Self {
            key,
            hand,
            goal,
            reach: 0.18,
            work: 0.15,
            drive: None,
            carry: None,
            stow: false,
            at: [0.0; 3],
            turn: [0.0; 3],
            tilt: [0.0; 3],
            grip: Grip::PINCH,
            then: Event::None,
            knock: Knock::ByEvent,
            throw: Throw::Nothing,
            when: When::default(),
            repeat: Repeat::Once,
            played: None,
        }
    }

    pub(super) fn timed(mut self, reach: f32, work: f32) -> Self {
        (self.reach, self.work) = (reach, work);
        self
    }

    pub(super) fn drive(mut self, part: PartRole, from: f32, to: f32) -> Self {
        self.drive = Some(Drive { part, from, to });
        self
    }

    pub(super) fn carry(mut self, part: PartRole) -> Self {
        self.carry = Some(part);
        self
    }

    pub(super) fn tilt(mut self, roll: f32, turn: f32, tip: f32) -> Self {
        self.tilt = [roll, turn, tip];
        self
    }

    pub(super) fn grip(mut self, grip: Grip) -> Self {
        self.grip = grip;
        self
    }

    pub(super) fn then(mut self, event: Event) -> Self {
        self.then = event;
        self
    }

    pub(super) fn knock(mut self, knock: Knock) -> Self {
        self.knock = knock;
        self
    }

    pub(super) fn throw(mut self, throw: Throw) -> Self {
        self.throw = throw;
        self
    }

    pub(super) fn stow(mut self) -> Self {
        self.stow = true;
        self
    }

    pub(super) fn when(mut self, when: When) -> Self {
        self.when = when;
        self
    }

    pub(super) fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn length(&self) -> f32 {
        self.reach + self.work
    }
}

/// Something that happens to the gun a set time into a clip part, in
/// seconds from the part's start, as the clip plays at its own speed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mark {
    pub at: f32,
    pub event: Event,
}

/// A part of an action played from clips, for a gun from an animation
/// pack: the arms clip the hands and the gun go by, the gun's own clip its
/// parts move by, the stretch of them it plays, in seconds (an end at or
/// before the start plays to the clip's end), what happens to the gun when,
/// and when the part plays and how often, as a beat does.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipPart {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arms: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gun: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub start: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub end: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    #[serde(skip_serializing_if = "When::is_always")]
    pub when: When,
    pub repeat: Repeat,
}

impl ClipPart {
    /// As a beat, which the action's planning repeats and picks as it
    /// does any: `index` among its spec's parts, its length not yet known.
    pub fn beat(&self, index: usize) -> Beat {
        let played = Some(index as u16);
        let beat = Beat { when: self.when, repeat: self.repeat, knock: Knock::Nothing, played, ..Beat::default() };
        Beat { key: BeatKey::Clip, ..beat.timed(0.0, 0.0) }
    }
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

/// An action as data: when it can play at all, how far it lifts the gun
/// up before the eyes, whether it takes the gun down from them, an offset
/// on where it brings the gun, its beats, and the clip it is based on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionSpec {
    #[serde(skip_serializing_if = "When::is_always")]
    pub when: When,
    pub lift: f32,
    pub lowers_aim: bool,
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub work: Place,
    #[serde(rename = "beat")]
    pub beats: Vec<Beat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<BaseClip>,
    /// The parts it plays from clips, which its beats name, and how fast
    /// they play, 1 as the clips go.
    #[serde(skip)]
    pub played: Vec<ClipPart>,
    #[serde(skip)]
    pub speed: f32,
}

impl Default for ActionSpec {
    fn default() -> Self {
        let work = Place::default();
        Self {
            when: When::default(),
            lift: 1.0,
            lowers_aim: true,
            work,
            beats: Vec::new(),
            clip: None,
            played: Vec::new(),
            speed: 1.0,
        }
    }
}

impl ActionSpec {
    /// Played from `parts` in place of its beats, the clips carrying the gun
    /// where its beats would lift it.
    pub fn play(&mut self, parts: &[ClipPart], speed: f32) {
        self.lift = 0.0;
        self.beats = parts.iter().enumerate().map(|(index, part)| part.beat(index)).collect();
        self.played = parts.to_vec();
        self.speed = speed.max(0.05);
    }

    /// The marks of a beat that is a clip part, none for any other.
    pub fn marks(&self, beat: &Beat) -> &[Mark] {
        beat.played.and_then(|index| self.played.get(usize::from(index))).map_or(&[], |part| part.marks.as_slice())
    }
}

fn is_nothing(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}

/// The beats an action plays for a gun of `kind` in `state`, its holder
/// having `spare` rounds: none when it cannot play, as a full gun is not
/// reloaded; each beat that plays in this state, and each run of beats
/// that repeat the same way as many times as they do.
pub fn expand(spec: &ActionSpec, kind: GunKind, state: &GunState, spare: u32) -> Vec<Beat> {
    if !spec.when.holds(kind, state, spare) {
        return Vec::new();
    }
    let mut beats = Vec::new();
    // What the beats so far will have done, for the runs that repeat until the feed is full.
    let (mut planned, mut left) = (state.clone(), spare);
    let mut start = 0;
    while start < spec.beats.len() {
        let repeat = spec.beats[start].repeat;
        let end = (start..spec.beats.len()).find(|&at| spec.beats[at].repeat != repeat).unwrap_or(spec.beats.len());
        let run = &spec.beats[start..end];
        if repeat == Repeat::UntilFull {
            while planned.rounds < kind.capacity() && left > 0 {
                let before = planned.rounds;
                for beat in run {
                    if beat.when.holds(kind, &planned, left) {
                        beats.push(*beat);
                        plan_beat(spec, beat, kind, &mut planned, &mut left);
                    }
                }
                if planned.rounds <= before {
                    break;
                }
            }
        } else {
            for _ in 0..repeat.count(kind, state, spare) {
                for beat in run.iter().filter(|beat| beat.when.holds(kind, state, spare)) {
                    beats.push(*beat);
                    plan_beat(spec, beat, kind, &mut planned, &mut left);
                }
            }
        }
        start = end;
    }
    beats
}

/// Puts what a planned beat will do into the state planned so far.
fn plan_beat(spec: &ActionSpec, beat: &Beat, kind: GunKind, planned: &mut GunState, left: &mut u32) {
    for mark in spec.marks(beat) {
        apply(mark.event, kind, planned, left);
    }
    apply(beat.then, kind, planned, left);
}

/// The beats a built-in action takes for a gun in this state.
pub fn plan(action: ActionKind, kind: GunKind, state: &GunState, spare: u32) -> Vec<Beat> {
    expand(&super::templates::template(kind, action), kind, state, spare)
}

/// Puts what a beat does into the gun's state, taking reloaded rounds from
/// the holder's spare ones.
pub fn apply(event: Event, kind: GunKind, state: &mut GunState, spare: &mut u32) {
    let capacity = kind.capacity();
    match event {
        Event::None => {}
        Event::Eject => state.chamber = Chamber::Empty,
        Event::Chamber => {
            if state.chamber == Chamber::Empty && state.rounds > 0 && state.fed {
                state.rounds -= 1;
                state.chamber = Chamber::Live;
            }
            state.open = false;
        }
        Event::Unload => {
            match kind.feed() {
                // What was left in the magazine goes back in the pouch.
                Feed::Magazine | Feed::Belt | Feed::EnBloc => *spare += std::mem::take(&mut state.rounds),
                // Every case and round thrown out.
                Feed::Cylinder => {
                    *spare += std::mem::take(&mut state.rounds) + u32::from(state.chamber == Chamber::Live);
                    state.chamber = Chamber::Empty;
                }
                Feed::Clips(_) | Feed::Tube => {}
            }
            state.fed = matches!(kind.feed(), Feed::Clips(_) | Feed::Tube | Feed::Cylinder);
        }
        Event::Load => {
            let taken = capacity.min(*spare);
            *spare -= taken;
            state.rounds = taken;
            state.fed = true;
            state.known_feed = Known::Exact;
            state.counted = 0;
        }
        Event::AddRounds(count) => {
            let room = capacity.saturating_sub(
                state.rounds + u32::from(kind.feed() == Feed::Cylinder && state.chamber == Chamber::Live),
            );
            let taken = count.min(room).min(*spare);
            *spare -= taken;
            if kind.feed() == Feed::Cylinder && state.chamber == Chamber::Empty && taken > 0 {
                state.chamber = Chamber::Live;
                state.rounds += taken - 1;
            } else {
                state.rounds += taken;
            }
            state.fed = true;
            if state.rounds + u32::from(state.chamber == Chamber::Live) >= capacity {
                state.known_feed = Known::Exact;
                state.counted = 0;
            }
        }
        Event::Open => state.open = true,
        Event::Close => state.open = false,
        Event::SeeFeed => {
            // Small feeds are counted; a long magazine or belt only judged.
            state.known_feed = if capacity <= 10 { Known::Exact } else { Known::Rough };
            state.counted = 0;
        }
        Event::SeeChamber => state.known_chamber = Known::Exact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(beats: &[Beat]) -> Vec<&'static str> {
        beats.iter().map(|beat| beat.key.title()).collect()
    }

    /// Plays a plan's events, as the beats would end one by one.
    fn play(beats: &[Beat], kind: GunKind, state: &mut GunState, spare: &mut u32) {
        for beat in beats {
            apply(beat.then, kind, state, spare);
        }
    }

    #[test]
    fn a_bolt_action_reload_lifts_pulls_presses_a_clip_and_closes_and_ends_full() {
        let kind = GunKind::BoltActionRifle;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
            state.cycle();
        }
        let mut spare = 20;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["Lift", "Pull", "Take Clip", "Clip In", "Press", "Push", "Turn"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 4 && spare == 15, "{state:?} {spare}");
        assert!(plan(ActionKind::Reload, kind, &GunState::loaded(kind), 20).is_empty(), "a full rifle is not reloaded");
    }

    #[test]
    fn an_en_bloc_rifle_takes_a_full_clip_and_a_box_magazine_is_swapped_and_racked() {
        let kind = GunKind::SemiAutoRifle;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 16;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["Take Clip", "Clip In", "Press", "Release"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 7 && spare == 8);

        let kind = GunKind::SlidePistol;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 14;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["Magazine Out", "To Pouch", "Magazine In", "Seat", "Rack", "Release"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 6 && spare == 7);

        let kind = GunKind::OpenBoltSmg;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 64;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["Magazine Out", "To Pouch", "Magazine In", "Seat", "Rack"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 32 && spare == 32);
    }

    #[test]
    fn a_shotgun_loads_shell_by_shell_and_a_revolver_breaks_open() {
        let kind = GunKind::PumpShotgun;
        let mut state = GunState::loaded(kind);
        for _ in 0..3 {
            state.shoot(kind);
            state.cycle();
        }
        let mut spare = 10;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats).iter().filter(|n| **n == "Shell In").count(), 3, "{:?}", names(&beats));
        play(&beats, kind, &mut state, &mut spare);
        assert_eq!((state.rounds, spare), (5, 7));

        let kind = GunKind::TopBreakRevolver;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 12;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        let names = names(&beats);
        assert_eq!(names.first(), Some(&"Break Open"));
        assert_eq!(names.last(), Some(&"Close"));
        assert_eq!(names.iter().filter(|n| **n == "Round In").count(), 6);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 5 && state.chamber == Chamber::Live && spare == 6);
    }

    #[test]
    fn checks_tell_the_holder_what_is_in_the_gun_roughly_for_a_long_magazine() {
        let kind = GunKind::OpenBoltSmg;
        let mut state = GunState::loaded(kind);
        state.known_feed = Known::Unknown;
        let mut spare = 0;
        play(&plan(ActionKind::CheckMagazine, kind, &state, 0), kind, &mut state, &mut spare);
        assert_eq!(state.known_feed, Known::Rough);
        let kind = GunKind::SlidePistol;
        let mut state = GunState::loaded(kind);
        state.known_chamber = Known::Unknown;
        play(&plan(ActionKind::CheckChamber, kind, &state, 0), kind, &mut state, &mut spare);
        assert_eq!(state.known_chamber, Known::Exact);
        assert!(state.ready(kind), "a press check leaves the round chambered");
    }

    /// A reload played from clips, a round at a time: the first part opens
    /// the bolt and puts one in, the loop one more each time until the feed
    /// is full, and the last chambers one and shuts the bolt.
    fn round_by_round() -> ActionSpec {
        let mark = |at, event| Mark { at, event };
        let first =
            ClipPart { marks: vec![mark(0.9, Event::Open), mark(2.0, Event::AddRounds(1))], ..ClipPart::default() };
        let pass =
            ClipPart { marks: vec![mark(0.7, Event::AddRounds(1))], repeat: Repeat::UntilFull, ..ClipPart::default() };
        let last = ClipPart { marks: vec![mark(0.6, Event::Chamber)], ..ClipPart::default() };
        let mut spec = ActionSpec::default();
        spec.play(&[first, pass, last], 1.0);
        spec
    }

    /// Plays a plan's marks and events, as its parts would.
    fn play_parts(spec: &ActionSpec, beats: &[Beat], kind: GunKind, state: &mut GunState, spare: &mut u32) {
        for beat in beats {
            spec.marks(beat).iter().for_each(|mark| apply(mark.event, kind, state, spare));
            apply(beat.then, kind, state, spare);
        }
    }

    #[test]
    fn a_part_repeated_until_full_loads_the_rounds_the_parts_before_it_left_to_load() {
        let (kind, spec) = (GunKind::BoltActionRifle, round_by_round());
        let parts = |beats: &[Beat]| beats.iter().map(|beat| beat.played.unwrap()).collect::<Vec<_>>();
        let mut empty = GunState::loaded(kind);
        (empty.rounds, empty.chamber) = (0, Chamber::Empty);
        let beats = expand(&spec, kind, &empty, 20);
        assert_eq!(parts(&beats), [0, 1, 1, 1, 1, 2], "one in first, four more, then shut");
        let mut spare = 20;
        play_parts(&spec, &beats, kind, &mut empty, &mut spare);
        assert!(empty.chamber == Chamber::Live && empty.rounds == 4 && spare == 15, "{empty:?}");

        let mut three = GunState::loaded(kind);
        three.rounds = 3;
        assert_eq!(parts(&expand(&spec, kind, &three, 20)), [0, 1, 2], "two missing: the first part's and one more");
        assert_eq!(parts(&expand(&spec, kind, &three, 1)), [0, 2], "one spare: the first part takes it");
        assert!(spec.beats.iter().all(|beat| beat.key == BeatKey::Clip && beat.length() == 0.0));
    }

    #[test]
    fn a_parts_when_picks_it_by_what_is_in_the_chamber() {
        let kind = GunKind::BoltActionRifle;
        let throws = ClipPart { gun: "Cycle".into(), when: When::all([Fact::ChamberFilled]), ..ClipPart::default() };
        let empty = ClipPart { gun: "No Bullet".into(), when: When::all([Fact::ChamberEmpty]), ..ClipPart::default() };
        let mut spec = ActionSpec::default();
        spec.play(&[throws, empty], 1.0);
        let mut state = GunState::loaded(kind);
        state.shoot(kind);
        assert_eq!(state.chamber, Chamber::Spent);
        assert_eq!(expand(&spec, kind, &state, 0).iter().map(|b| b.played).collect::<Vec<_>>(), [Some(0)]);
        state.chamber = Chamber::Empty;
        assert_eq!(expand(&spec, kind, &state, 0).iter().map(|b| b.played).collect::<Vec<_>>(), [Some(1)]);
    }
}
