//! Reloads, bolts and checks with no clips: each is planned from the gun's
//! kind and state as a run of beats, a hand reaching somewhere and doing
//! one thing there: lifting a bolt, pulling a magazine, pressing rounds
//! down from a clip. A beat that drives a part moves the hand with it, so
//! the part's own travel is the hand's path. What a beat does to the gun,
//! a round chambered or a magazine seated, happens as the beat ends.

use voxear_core::math::Vec3;

use super::gun::GunKind;
use super::hands::Grip;
use super::mechanism::{Chamber, Feed, GunState, Known, PartRole};

/// What a holder can do with a gun.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Reload,
    /// Works the bolt, slide or pump by hand.
    Cycle,
    CheckMagazine,
    CheckChamber,
}

/// A hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Where a beat takes a hand.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    Hold,
}

/// What a beat does to the gun as it ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
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

/// One thing a hand does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    /// A short name for what it is, for tests and the editor's timeline.
    pub name: &'static str,
    pub hand: Hand,
    pub goal: Goal,
    /// Seconds to reach the goal, then to do the work there.
    pub reach: f32,
    pub work: f32,
    /// A part the work moves, from one place to another.
    pub drive: Option<(PartRole, f32, f32)>,
    /// A part the hand carries while the beat lasts, as a magazine out.
    pub carry: Option<PartRole>,
    /// How the gun is held meanwhile, in degrees: rolled, its top to the
    /// right; turned, its muzzle to the right; and tipped, its muzzle up.
    pub tilt: Vec3,
    pub grip: Grip,
    pub then: Event,
}

impl Beat {
    fn new(name: &'static str, hand: Hand, goal: Goal) -> Self {
        Self {
            name,
            hand,
            goal,
            reach: 0.18,
            work: 0.15,
            drive: None,
            carry: None,
            tilt: Vec3::ZERO,
            grip: Grip::PINCH,
            then: Event::None,
        }
    }

    fn timed(mut self, reach: f32, work: f32) -> Self {
        (self.reach, self.work) = (reach, work);
        self
    }

    fn drive(mut self, part: PartRole, from: f32, to: f32) -> Self {
        self.drive = Some((part, from, to));
        self
    }

    fn carry(mut self, part: PartRole) -> Self {
        self.carry = Some(part);
        self
    }

    fn tilt(mut self, roll: f32, turn: f32, tip: f32) -> Self {
        self.tilt = Vec3::new(roll, turn, tip);
        self
    }

    fn grip(mut self, grip: Grip) -> Self {
        self.grip = grip;
        self
    }

    fn then(mut self, event: Event) -> Self {
        self.then = event;
        self
    }

    pub fn length(&self) -> f32 {
        self.reach + self.work
    }
}

/// The beats an action takes for a gun in this state, or none when there
/// is nothing to do: a full gun is not reloaded.
pub fn plan(action: ActionKind, kind: GunKind, state: &GunState, spare: u32) -> Vec<Beat> {
    use Hand::{Left, Right};
    let missing = kind.capacity().saturating_sub(state.rounds);
    let full = missing == 0 && state.fed && (state.chamber == Chamber::Live || kind.open_bolt());
    if action == ActionKind::Reload && (full || spare == 0) {
        return Vec::new();
    }
    let bolt = |name, from, to| Beat::new(name, Right, Goal::Part(PartRole::Bolt)).drive(PartRole::Bolt, from, to);
    match (kind, action) {
        // A turn-bolt: lift, pull, push, turn; a reload presses a stripper clip's rounds down between.
        (GunKind::BoltActionRifle, ActionKind::Cycle) => vec![
            bolt("lift", 0.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.16, 0.1),
            bolt("pull", 1.0, 2.0).tilt(-14.0, 0.0, 0.0).timed(0.0, 0.14).then(Event::Eject),
            bolt("push", 2.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.03, 0.13).then(Event::Chamber),
            bolt("turn", 1.0, 0.0).tilt(-8.0, 0.0, 0.0).timed(0.0, 0.09),
        ],
        (GunKind::BoltActionRifle, ActionKind::Reload) => {
            let clips = (missing.min(spare) as f32 / 5.0).ceil().max(1.0) as usize;
            let mut beats = vec![
                bolt("lift", 0.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.16, 0.1),
                bolt("pull", 1.0, 2.0).tilt(-18.0, 0.0, 6.0).timed(0.0, 0.14).then(Event::Eject),
            ];
            for _ in 0..clips {
                beats.push(
                    Beat::new("take clip", Right, Goal::Pouch)
                        .timed(0.32, 0.08)
                        .grip(Grip::PINCH)
                        .tilt(-22.0, 0.0, 8.0),
                );
                beats.push(
                    Beat::new("clip in", Right, Goal::Feed)
                        .carry(PartRole::Clip)
                        .timed(0.3, 0.08)
                        .tilt(-25.0, 0.0, 10.0),
                );
                beats.push(
                    Beat::new("press", Right, Goal::Feed)
                        .carry(PartRole::Clip)
                        .drive(PartRole::Clip, 0.0, 0.25)
                        .grip(Grip::OPEN)
                        .timed(0.02, 0.35)
                        .tilt(-25.0, 0.0, 12.0)
                        .then(Event::AddRounds(5)),
                );
            }
            beats.push(bolt("push", 2.0, 1.0).tilt(-14.0, 0.0, 4.0).timed(0.2, 0.15).then(Event::Chamber));
            beats.push(bolt("turn", 1.0, 0.0).tilt(-8.0, 0.0, 0.0).timed(0.0, 0.09));
            beats
        }
        (GunKind::BoltActionRifle, _) => vec![
            bolt("lift", 0.0, 1.0).tilt(-15.0, 0.0, 0.0).timed(0.16, 0.1),
            bolt("open", 1.0, 1.45).tilt(-40.0, -10.0, 10.0).timed(0.0, 0.18),
            Beat::new("look", Right, Goal::Part(PartRole::Bolt))
                .tilt(-45.0, -12.0, 14.0)
                .timed(0.0, 0.55)
                .then(Event::SeeFeed),
            bolt("close", 1.45, 1.0).tilt(-15.0, 0.0, 0.0).timed(0.0, 0.14).then(Event::SeeChamber),
            bolt("turn", 1.0, 0.0).timed(0.0, 0.09),
        ],

        // An en-bloc rifle: a full clip pressed straight down, the thumb snatched out as the bolt slams home.
        (GunKind::SemiAutoRifle, ActionKind::Reload) => {
            let mut beats = Vec::new();
            if state.fed {
                beats.push(bolt("pull", 0.0, 1.0).timed(0.2, 0.15).tilt(-12.0, 0.0, 0.0).then(Event::Unload));
            }
            beats.push(Beat::new("take clip", Right, Goal::Pouch).timed(0.32, 0.08).tilt(-18.0, 0.0, 8.0));
            beats.push(
                Beat::new("clip in", Right, Goal::Feed).carry(PartRole::Clip).timed(0.3, 0.12).tilt(-20.0, 0.0, 10.0),
            );
            beats.push(
                Beat::new("press", Right, Goal::Feed)
                    .carry(PartRole::Clip)
                    .drive(PartRole::Clip, 0.0, 0.3)
                    .grip(Grip::OPEN)
                    .timed(0.0, 0.2)
                    .tilt(-20.0, 0.0, 12.0)
                    .then(Event::Load),
            );
            beats.push(bolt("release", 1.0, 0.0).timed(0.05, 0.06).then(Event::Chamber));
            beats
        }
        (GunKind::SemiAutoRifle, ActionKind::Cycle) => vec![
            bolt("pull", 0.0, 1.0).timed(0.2, 0.14).then(Event::Eject),
            bolt("release", 1.0, 0.0).timed(0.0, 0.06).then(Event::Chamber),
        ],

        // A box magazine: out, a fresh one from the pouch, in and slapped home; then the action worked if it is open.
        (GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::Reload) => {
            let handle = if kind == GunKind::OpenBoltSmg {
                PartRole::Bolt
            } else if kind == GunKind::SlidePistol {
                PartRole::Slide
            } else {
                PartRole::Toggle
            };
            let mut beats = vec![
                Beat::new("mag out", Left, Goal::Part(PartRole::Magazine))
                    .drive(PartRole::Magazine, 0.0, 0.5)
                    .carry(PartRole::Magazine)
                    .timed(0.2, 0.15)
                    .tilt(20.0, 10.0, 8.0)
                    .then(Event::Unload),
                Beat::new("to pouch", Left, Goal::Pouch)
                    .carry(PartRole::Magazine)
                    .timed(0.3, 0.12)
                    .tilt(15.0, 8.0, 6.0),
                Beat::new("mag in", Left, Goal::Feed).carry(PartRole::Magazine).timed(0.32, 0.06).tilt(18.0, 10.0, 8.0),
                Beat::new("seat", Left, Goal::Part(PartRole::Magazine))
                    .drive(PartRole::Magazine, 0.3, 0.0)
                    .grip(Grip::OPEN)
                    .timed(0.0, 0.1)
                    .tilt(12.0, 6.0, 4.0)
                    .then(Event::Load),
            ];
            let cock = kind.open_bolt() && !state.open;
            if state.open || cock || state.chamber != Chamber::Live {
                // A slide locked back is already open; a closed one is pulled from rest.
                let from: f32 = if state.open && !kind.open_bolt() { 1.0 } else { 0.0 };
                beats.push(
                    Beat::new("rack", Left, Goal::Part(handle))
                        .drive(handle, from, 1.0)
                        .timed(0.22, 0.1)
                        .tilt(-10.0, -6.0, 0.0)
                        .then(if kind.open_bolt() { Event::Open } else { Event::None }),
                );
                if !kind.open_bolt() {
                    beats.push(
                        Beat::new("release", Left, Goal::Part(handle))
                            .drive(handle, 1.0, 0.0)
                            .timed(0.0, 0.06)
                            .then(Event::Chamber),
                    );
                }
            }
            beats
        }
        (GunKind::OpenBoltSmg, ActionKind::Cycle) => vec![
            Beat::new("cock", Left, Goal::Part(PartRole::Bolt))
                .drive(PartRole::Bolt, 0.0, 1.0)
                .timed(0.22, 0.12)
                .then(Event::Open),
        ],
        (GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::Cycle | ActionKind::CheckChamber) => {
            let part = if kind == GunKind::SlidePistol { PartRole::Slide } else { PartRole::Toggle };
            match action {
                ActionKind::Cycle => vec![
                    Beat::new("rack", Left, Goal::Part(part)).drive(part, 0.0, 1.0).timed(0.2, 0.1).then(Event::Eject),
                    Beat::new("release", Left, Goal::Part(part))
                        .drive(part, 1.0, 0.0)
                        .timed(0.0, 0.06)
                        .then(Event::Chamber),
                ],
                _ => vec![
                    Beat::new("press", Left, Goal::Part(part))
                        .drive(part, 0.0, 0.35)
                        .timed(0.22, 0.14)
                        .tilt(30.0, 0.0, 5.0),
                    Beat::new("look", Left, Goal::Part(part))
                        .timed(0.0, 0.5)
                        .tilt(35.0, 0.0, 6.0)
                        .then(Event::SeeChamber),
                    Beat::new("close", Left, Goal::Part(part)).drive(part, 0.35, 0.0).timed(0.0, 0.1),
                ],
            }
        }
        (GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol, _) => vec![
            Beat::new("draw out", Left, Goal::Part(PartRole::Magazine))
                .drive(PartRole::Magazine, 0.0, 0.35)
                .carry(PartRole::Magazine)
                .timed(0.2, 0.14)
                .tilt(15.0, 25.0, 10.0),
            Beat::new("look", Left, Goal::Part(PartRole::Magazine))
                .carry(PartRole::Magazine)
                .timed(0.0, 0.55)
                .tilt(15.0, 30.0, 12.0)
                .then(Event::SeeFeed),
            Beat::new("seat", Left, Goal::Part(PartRole::Magazine))
                .drive(PartRole::Magazine, 0.35, 0.0)
                .grip(Grip::OPEN)
                .timed(0.0, 0.1),
        ],

        // A belt: cocked, the cover up, a fresh belt laid and the cover slapped shut.
        (GunKind::BeltFedMg, ActionKind::Reload | ActionKind::Cycle) => {
            let mut beats = vec![
                bolt("cock", 0.0, 1.0).timed(0.22, 0.14).then(Event::Open),
                bolt("return", 1.0, 0.0).timed(0.0, 0.1),
            ];
            if action == ActionKind::Reload {
                beats.extend([
                    Beat::new("cover up", Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 0.0, 1.0)
                        .timed(0.25, 0.15)
                        .tilt(12.0, 8.0, 6.0)
                        .then(Event::Unload),
                    Beat::new("take belt", Left, Goal::Pouch).timed(0.35, 0.12).tilt(10.0, 8.0, 4.0),
                    Beat::new("lay belt", Left, Goal::Feed).timed(0.35, 0.18).tilt(14.0, 10.0, 8.0).then(Event::Load),
                    Beat::new("cover down", Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 1.0, 0.0)
                        .grip(Grip::OPEN)
                        .timed(0.1, 0.08)
                        .tilt(8.0, 6.0, 4.0),
                ]);
            }
            beats
        }
        (GunKind::BeltFedMg, _) => {
            vec![Beat::new("look", Left, Goal::Feed).timed(0.25, 0.6).tilt(25.0, 15.0, 8.0).then(Event::SeeFeed)]
        }

        // A tube: one shell at a time into the port underneath, as many as it lacks; pumped if the chamber is empty.
        (GunKind::PumpShotgun, ActionKind::Reload) => {
            let mut beats = Vec::new();
            for _ in 0..missing.min(spare) {
                beats.push(Beat::new("take shell", Left, Goal::Pouch).timed(0.25, 0.05).tilt(30.0, 0.0, 6.0));
                beats.push(
                    Beat::new("shell in", Left, Goal::Feed)
                        .timed(0.22, 0.12)
                        .tilt(35.0, 0.0, 8.0)
                        .grip(Grip::OPEN)
                        .then(Event::AddRounds(1)),
                );
            }
            if state.chamber != Chamber::Live {
                beats.push(
                    Beat::new("pump", Left, Goal::Part(PartRole::Pump))
                        .drive(PartRole::Pump, 0.0, 1.0)
                        .grip(Grip::SUPPORT)
                        .timed(0.15, 0.12)
                        .then(Event::Eject),
                );
                beats.push(
                    Beat::new("return", Left, Goal::Part(PartRole::Pump))
                        .drive(PartRole::Pump, 1.0, 0.0)
                        .grip(Grip::SUPPORT)
                        .timed(0.0, 0.1)
                        .then(Event::Chamber),
                );
            }
            beats
        }
        (GunKind::PumpShotgun, ActionKind::Cycle) => vec![
            Beat::new("pump", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 1.0)
                .grip(Grip::SUPPORT)
                .timed(0.08, 0.12)
                .then(Event::Eject),
            Beat::new("return", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 1.0, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1)
                .then(Event::Chamber),
        ],
        (GunKind::PumpShotgun, _) => vec![
            Beat::new("open", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 0.3)
                .grip(Grip::SUPPORT)
                .timed(0.1, 0.12)
                .tilt(30.0, 0.0, 4.0),
            Beat::new("look", Left, Goal::Part(PartRole::Pump))
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.5)
                .tilt(35.0, 0.0, 6.0)
                .then(Event::SeeChamber),
            Beat::new("close", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.3, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1),
        ],

        // A top-break revolver: tipped open, every case thrown out, each chamber loaded, and closed.
        (GunKind::TopBreakRevolver, ActionKind::Reload) => {
            let mut beats = vec![
                Beat::new("break", Left, Goal::Part(PartRole::Barrel))
                    .drive(PartRole::Barrel, 0.0, 1.0)
                    .timed(0.22, 0.2)
                    .tilt(0.0, 0.0, -25.0)
                    .then(Event::Unload),
            ];
            for _ in 0..kind.capacity().min(spare + state.rounds + u32::from(state.chamber == Chamber::Live)) {
                beats.push(Beat::new("take round", Right, Goal::Pouch).timed(0.22, 0.05).tilt(0.0, 0.0, -30.0));
                beats.push(
                    Beat::new("round in", Right, Goal::Feed)
                        .timed(0.2, 0.08)
                        .tilt(0.0, 0.0, -30.0)
                        .then(Event::AddRounds(1)),
                );
            }
            beats.push(
                Beat::new("close", Left, Goal::Part(PartRole::Barrel))
                    .drive(PartRole::Barrel, 1.0, 0.0)
                    .grip(Grip::OPEN)
                    .timed(0.15, 0.1)
                    .then(Event::Close),
            );
            beats
        }
        (GunKind::TopBreakRevolver, _) => vec![
            Beat::new("break", Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.0, 0.6)
                .timed(0.22, 0.15)
                .tilt(0.0, 15.0, -20.0),
            Beat::new("look", Left, Goal::Part(PartRole::Barrel))
                .timed(0.0, 0.5)
                .tilt(0.0, 20.0, -25.0)
                .then(Event::SeeFeed),
            Beat::new("close", Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.6, 0.0)
                .grip(Grip::OPEN)
                .timed(0.0, 0.1)
                .then(Event::SeeChamber),
        ],
        (GunKind::SemiAutoRifle, _) => vec![
            bolt("open", 0.0, 0.35).timed(0.2, 0.15).tilt(-30.0, -8.0, 8.0),
            Beat::new("look", Right, Goal::Part(PartRole::Bolt))
                .timed(0.0, 0.5)
                .tilt(-35.0, -10.0, 10.0)
                .then(Event::SeeFeed),
            bolt("close", 0.35, 0.0).timed(0.0, 0.08).then(Event::SeeChamber),
        ],
    }
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
        beats.iter().map(|beat| beat.name).collect()
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
        assert_eq!(names(&beats), ["lift", "pull", "take clip", "clip in", "press", "push", "turn"]);
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
        assert_eq!(names(&beats), ["take clip", "clip in", "press", "release"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 7 && spare == 8);

        let kind = GunKind::SlidePistol;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 14;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["mag out", "to pouch", "mag in", "seat", "rack", "release"]);
        play(&beats, kind, &mut state, &mut spare);
        assert!(state.ready(kind) && state.rounds == 6 && spare == 7);

        let kind = GunKind::OpenBoltSmg;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
        }
        let mut spare = 64;
        let beats = plan(ActionKind::Reload, kind, &state, spare);
        assert_eq!(names(&beats), ["mag out", "to pouch", "mag in", "seat", "rack"]);
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
        assert_eq!(names(&beats).iter().filter(|n| **n == "shell in").count(), 3, "{:?}", names(&beats));
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
        assert_eq!(names.first(), Some(&"break"));
        assert_eq!(names.last(), Some(&"close"));
        assert_eq!(names.iter().filter(|n| **n == "round in").count(), 6);
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
}
