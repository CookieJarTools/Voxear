//! The planner as it was before actions were data, kept to check that the
//! templates expand to exactly the beats it gave, for every kind, action
//! and state.

use voxear_core::math::Vec3;

use super::actions::{ActionKind, Event, Goal, Hand};
use super::gun::GunKind;
use super::hands::Grip;
use super::mechanism::{Chamber, GunState, PartRole};

/// One thing a hand does, as beats were before they were data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OldBeat {
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

impl OldBeat {
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
}

/// The beats an action takes for a gun in this state, or none when there
/// is nothing to do: a full gun is not reloaded.
pub fn legacy_plan(action: ActionKind, kind: GunKind, state: &GunState, spare: u32) -> Vec<OldBeat> {
    use Hand::{Left, Right};
    let missing = kind.capacity().saturating_sub(state.rounds);
    let full = missing == 0 && state.fed && (state.chamber == Chamber::Live || kind.open_bolt());
    if action == ActionKind::Reload && (full || spare == 0) {
        return Vec::new();
    }
    let bolt = |name, from, to| OldBeat::new(name, Right, Goal::Part(PartRole::Bolt)).drive(PartRole::Bolt, from, to);
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
                    OldBeat::new("take clip", Right, Goal::Pouch)
                        .timed(0.32, 0.08)
                        .grip(Grip::PINCH)
                        .tilt(-22.0, 0.0, 8.0),
                );
                beats.push(
                    OldBeat::new("clip in", Right, Goal::Feed)
                        .carry(PartRole::Clip)
                        .timed(0.3, 0.08)
                        .tilt(-25.0, 0.0, 10.0),
                );
                beats.push(
                    OldBeat::new("press", Right, Goal::Feed)
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
            OldBeat::new("look", Right, Goal::Part(PartRole::Bolt))
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
            beats.push(OldBeat::new("take clip", Right, Goal::Pouch).timed(0.32, 0.08).tilt(-18.0, 0.0, 8.0));
            beats.push(
                OldBeat::new("clip in", Right, Goal::Feed)
                    .carry(PartRole::Clip)
                    .timed(0.3, 0.12)
                    .tilt(-20.0, 0.0, 10.0),
            );
            beats.push(
                OldBeat::new("press", Right, Goal::Feed)
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
                OldBeat::new("mag out", Left, Goal::Part(PartRole::Magazine))
                    .drive(PartRole::Magazine, 0.0, 0.5)
                    .carry(PartRole::Magazine)
                    .timed(0.2, 0.15)
                    .tilt(20.0, 10.0, 8.0)
                    .then(Event::Unload),
                OldBeat::new("to pouch", Left, Goal::Pouch)
                    .carry(PartRole::Magazine)
                    .timed(0.3, 0.12)
                    .tilt(15.0, 8.0, 6.0),
                OldBeat::new("mag in", Left, Goal::Feed)
                    .carry(PartRole::Magazine)
                    .timed(0.32, 0.06)
                    .tilt(18.0, 10.0, 8.0),
                OldBeat::new("seat", Left, Goal::Part(PartRole::Magazine))
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
                    OldBeat::new("rack", Left, Goal::Part(handle))
                        .drive(handle, from, 1.0)
                        .timed(0.22, 0.1)
                        .tilt(-10.0, -6.0, 0.0)
                        .then(if kind.open_bolt() { Event::Open } else { Event::None }),
                );
                if !kind.open_bolt() {
                    beats.push(
                        OldBeat::new("release", Left, Goal::Part(handle))
                            .drive(handle, 1.0, 0.0)
                            .timed(0.0, 0.06)
                            .then(Event::Chamber),
                    );
                }
            }
            beats
        }
        (GunKind::OpenBoltSmg, ActionKind::Cycle) => vec![
            OldBeat::new("cock", Left, Goal::Part(PartRole::Bolt))
                .drive(PartRole::Bolt, 0.0, 1.0)
                .timed(0.22, 0.12)
                .then(Event::Open),
        ],
        (GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::Cycle | ActionKind::CheckChamber) => {
            let part = if kind == GunKind::SlidePistol { PartRole::Slide } else { PartRole::Toggle };
            match action {
                ActionKind::Cycle => vec![
                    OldBeat::new("rack", Left, Goal::Part(part))
                        .drive(part, 0.0, 1.0)
                        .timed(0.2, 0.1)
                        .then(Event::Eject),
                    OldBeat::new("release", Left, Goal::Part(part))
                        .drive(part, 1.0, 0.0)
                        .timed(0.0, 0.06)
                        .then(Event::Chamber),
                ],
                _ => vec![
                    OldBeat::new("press", Left, Goal::Part(part))
                        .drive(part, 0.0, 0.35)
                        .timed(0.22, 0.14)
                        .tilt(30.0, 0.0, 5.0),
                    OldBeat::new("look", Left, Goal::Part(part))
                        .timed(0.0, 0.5)
                        .tilt(35.0, 0.0, 6.0)
                        .then(Event::SeeChamber),
                    OldBeat::new("close", Left, Goal::Part(part)).drive(part, 0.35, 0.0).timed(0.0, 0.1),
                ],
            }
        }
        (GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol, _) => vec![
            OldBeat::new("draw out", Left, Goal::Part(PartRole::Magazine))
                .drive(PartRole::Magazine, 0.0, 0.35)
                .carry(PartRole::Magazine)
                .timed(0.2, 0.14)
                .tilt(15.0, 25.0, 10.0),
            OldBeat::new("look", Left, Goal::Part(PartRole::Magazine))
                .carry(PartRole::Magazine)
                .timed(0.0, 0.55)
                .tilt(15.0, 30.0, 12.0)
                .then(Event::SeeFeed),
            OldBeat::new("seat", Left, Goal::Part(PartRole::Magazine))
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
                    OldBeat::new("cover up", Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 0.0, 1.0)
                        .timed(0.25, 0.15)
                        .tilt(12.0, 8.0, 6.0)
                        .then(Event::Unload),
                    OldBeat::new("take belt", Left, Goal::Pouch).timed(0.35, 0.12).tilt(10.0, 8.0, 4.0),
                    OldBeat::new("lay belt", Left, Goal::Feed)
                        .timed(0.35, 0.18)
                        .tilt(14.0, 10.0, 8.0)
                        .then(Event::Load),
                    OldBeat::new("cover down", Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 1.0, 0.0)
                        .grip(Grip::OPEN)
                        .timed(0.1, 0.08)
                        .tilt(8.0, 6.0, 4.0),
                ]);
            }
            beats
        }
        (GunKind::BeltFedMg, _) => {
            vec![OldBeat::new("look", Left, Goal::Feed).timed(0.25, 0.6).tilt(25.0, 15.0, 8.0).then(Event::SeeFeed)]
        }

        // A tube: one shell at a time into the port underneath, as many as it lacks; pumped if the chamber is empty.
        (GunKind::PumpShotgun, ActionKind::Reload) => {
            let mut beats = Vec::new();
            for _ in 0..missing.min(spare) {
                beats.push(OldBeat::new("take shell", Left, Goal::Pouch).timed(0.25, 0.05).tilt(30.0, 0.0, 6.0));
                beats.push(
                    OldBeat::new("shell in", Left, Goal::Feed)
                        .timed(0.22, 0.12)
                        .tilt(35.0, 0.0, 8.0)
                        .grip(Grip::OPEN)
                        .then(Event::AddRounds(1)),
                );
            }
            if state.chamber != Chamber::Live {
                beats.push(
                    OldBeat::new("pump", Left, Goal::Part(PartRole::Pump))
                        .drive(PartRole::Pump, 0.0, 1.0)
                        .grip(Grip::SUPPORT)
                        .timed(0.15, 0.12)
                        .then(Event::Eject),
                );
                beats.push(
                    OldBeat::new("return", Left, Goal::Part(PartRole::Pump))
                        .drive(PartRole::Pump, 1.0, 0.0)
                        .grip(Grip::SUPPORT)
                        .timed(0.0, 0.1)
                        .then(Event::Chamber),
                );
            }
            beats
        }
        (GunKind::PumpShotgun, ActionKind::Cycle) => vec![
            OldBeat::new("pump", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 1.0)
                .grip(Grip::SUPPORT)
                .timed(0.08, 0.12)
                .then(Event::Eject),
            OldBeat::new("return", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 1.0, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1)
                .then(Event::Chamber),
        ],
        (GunKind::PumpShotgun, _) => vec![
            OldBeat::new("open", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 0.3)
                .grip(Grip::SUPPORT)
                .timed(0.1, 0.12)
                .tilt(30.0, 0.0, 4.0),
            OldBeat::new("look", Left, Goal::Part(PartRole::Pump))
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.5)
                .tilt(35.0, 0.0, 6.0)
                .then(Event::SeeChamber),
            OldBeat::new("close", Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.3, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1),
        ],

        // A top-break revolver: tipped open, every case thrown out, each chamber loaded, and closed.
        (GunKind::TopBreakRevolver, ActionKind::Reload) => {
            let mut beats = vec![
                OldBeat::new("break", Left, Goal::Part(PartRole::Barrel))
                    .drive(PartRole::Barrel, 0.0, 1.0)
                    .timed(0.22, 0.2)
                    .tilt(0.0, 0.0, -25.0)
                    .then(Event::Unload),
            ];
            for _ in 0..kind.capacity().min(spare + state.rounds + u32::from(state.chamber == Chamber::Live)) {
                beats.push(OldBeat::new("take round", Right, Goal::Pouch).timed(0.22, 0.05).tilt(0.0, 0.0, -30.0));
                beats.push(
                    OldBeat::new("round in", Right, Goal::Feed)
                        .timed(0.2, 0.08)
                        .tilt(0.0, 0.0, -30.0)
                        .then(Event::AddRounds(1)),
                );
            }
            beats.push(
                OldBeat::new("close", Left, Goal::Part(PartRole::Barrel))
                    .drive(PartRole::Barrel, 1.0, 0.0)
                    .grip(Grip::OPEN)
                    .timed(0.15, 0.1)
                    .then(Event::Close),
            );
            beats
        }
        (GunKind::TopBreakRevolver, _) => vec![
            OldBeat::new("break", Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.0, 0.6)
                .timed(0.22, 0.15)
                .tilt(0.0, 15.0, -20.0),
            OldBeat::new("look", Left, Goal::Part(PartRole::Barrel))
                .timed(0.0, 0.5)
                .tilt(0.0, 20.0, -25.0)
                .then(Event::SeeFeed),
            OldBeat::new("close", Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.6, 0.0)
                .grip(Grip::OPEN)
                .timed(0.0, 0.1)
                .then(Event::SeeChamber),
        ],
        (GunKind::SemiAutoRifle, _) => vec![
            bolt("open", 0.0, 0.35).timed(0.2, 0.15).tilt(-30.0, -8.0, 8.0),
            OldBeat::new("look", Right, Goal::Part(PartRole::Bolt))
                .timed(0.0, 0.5)
                .tilt(-35.0, -10.0, 10.0)
                .then(Event::SeeFeed),
            bolt("close", 0.35, 0.0).timed(0.0, 0.08).then(Event::SeeChamber),
        ],
    }
}
