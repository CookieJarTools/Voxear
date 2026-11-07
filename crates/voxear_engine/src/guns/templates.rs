//! Each kind of gun's built-in actions, as data: the beats, their timing,
//! how the gun is tilted for each, and which beats play only in some
//! states or repeat. A gun's handling can offset these, or replace one
//! with an action of its own.

use super::actions::{
    ActionKind, ActionSpec, Beat, BeatKey as K, Event, Fact, Goal, Hand::Left, Hand::Right, Knock, Repeat, Throw, When,
};
use super::gun::GunKind;
use super::hands::Grip;
use super::mechanism::PartRole;

/// A built-in action for a gun of `kind`. A reload plays only when the gun
/// is not full and there are rounds to load; working the action lifts the
/// gun only a little and keeps it at the eye.
pub fn template(kind: GunKind, action: ActionKind) -> ActionSpec {
    let when = if action == ActionKind::Reload { When::all([Fact::NotFull, Fact::Spare]) } else { When::default() };
    let (lift, lowers_aim) = if action == ActionKind::Cycle { (0.4, false) } else { (1.0, true) };
    ActionSpec { when, lift, lowers_aim, beats: beats(kind, action), ..ActionSpec::default() }
}

fn bolt(key: K, from: f32, to: f32) -> Beat {
    Beat::new(key, Right, Goal::Part(PartRole::Bolt)).drive(PartRole::Bolt, from, to)
}

fn beats(kind: GunKind, action: ActionKind) -> Vec<Beat> {
    match (kind, action) {
        // A turn-bolt: lift, pull, push, turn; a reload presses a stripper clip's rounds down between.
        (GunKind::BoltActionRifle, ActionKind::Cycle) => vec![
            bolt(K::Lift, 0.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.16, 0.1),
            bolt(K::Pull, 1.0, 2.0).tilt(-14.0, 0.0, 0.0).timed(0.0, 0.14).then(Event::Eject),
            bolt(K::Push, 2.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.03, 0.13).then(Event::Chamber),
            bolt(K::Turn, 1.0, 0.0).tilt(-8.0, 0.0, 0.0).timed(0.0, 0.09),
        ],
        (GunKind::BoltActionRifle, ActionKind::Reload) => vec![
            bolt(K::Lift, 0.0, 1.0).tilt(-12.0, 0.0, 0.0).timed(0.16, 0.1),
            bolt(K::Pull, 1.0, 2.0).tilt(-18.0, 0.0, 6.0).timed(0.0, 0.14).then(Event::Eject),
            Beat::new(K::TakeClip, Right, Goal::Pouch)
                .timed(0.32, 0.08)
                .grip(Grip::PINCH)
                .tilt(-22.0, 0.0, 8.0)
                .repeat(Repeat::PerClip),
            Beat::new(K::ClipIn, Right, Goal::Feed)
                .carry(PartRole::Clip)
                .timed(0.3, 0.08)
                .tilt(-25.0, 0.0, 10.0)
                .repeat(Repeat::PerClip),
            Beat::new(K::Press, Right, Goal::Feed)
                .carry(PartRole::Clip)
                .drive(PartRole::Clip, 0.0, 0.25)
                .grip(Grip::OPEN)
                .timed(0.02, 0.35)
                .tilt(-25.0, 0.0, 12.0)
                .then(Event::AddRounds(5))
                .knock(Knock::PressDown)
                .repeat(Repeat::PerClip),
            // The bolt closing strips the emptied clip off, and it is flicked away.
            bolt(K::Push, 2.0, 1.0).tilt(-14.0, 0.0, 4.0).timed(0.2, 0.15).then(Event::Chamber).throw(Throw::Clip),
            bolt(K::Turn, 1.0, 0.0).tilt(-8.0, 0.0, 0.0).timed(0.0, 0.09),
        ],
        (GunKind::BoltActionRifle, _) => vec![
            bolt(K::Lift, 0.0, 1.0).tilt(-15.0, 0.0, 0.0).timed(0.16, 0.1),
            bolt(K::Open, 1.0, 1.45).tilt(-40.0, -10.0, 10.0).timed(0.0, 0.18),
            Beat::new(K::Look, Right, Goal::Part(PartRole::Bolt))
                .tilt(-45.0, -12.0, 14.0)
                .timed(0.0, 0.55)
                .then(Event::SeeFeed),
            bolt(K::Close, 1.45, 1.0).tilt(-15.0, 0.0, 0.0).timed(0.0, 0.14).then(Event::SeeChamber),
            bolt(K::Turn, 1.0, 0.0).timed(0.0, 0.09),
        ],

        // An en-bloc rifle: a full clip pressed straight down, the thumb snatched out as the bolt slams home.
        (GunKind::SemiAutoRifle, ActionKind::Reload) => vec![
            bolt(K::Pull, 0.0, 1.0)
                .timed(0.2, 0.15)
                .tilt(-12.0, 0.0, 0.0)
                .then(Event::Unload)
                .when(When::all([Fact::Fed])),
            Beat::new(K::TakeClip, Right, Goal::Pouch).timed(0.32, 0.08).tilt(-18.0, 0.0, 8.0),
            Beat::new(K::ClipIn, Right, Goal::Feed).carry(PartRole::Clip).timed(0.3, 0.12).tilt(-20.0, 0.0, 10.0),
            Beat::new(K::Press, Right, Goal::Feed)
                .carry(PartRole::Clip)
                .drive(PartRole::Clip, 0.0, 0.3)
                .grip(Grip::OPEN)
                .timed(0.0, 0.2)
                .tilt(-20.0, 0.0, 12.0)
                .then(Event::Load),
            bolt(K::Release, 1.0, 0.0).timed(0.05, 0.06).then(Event::Chamber),
        ],
        (GunKind::SemiAutoRifle, ActionKind::Cycle) => vec![
            bolt(K::Pull, 0.0, 1.0).timed(0.2, 0.14).then(Event::Eject),
            bolt(K::Release, 1.0, 0.0).timed(0.0, 0.06).then(Event::Chamber),
        ],

        // A box magazine: out, a fresh one from the pouch, in and slapped home; then the action worked if it is open.
        (GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::Reload) => {
            let handle = match kind {
                GunKind::OpenBoltSmg => PartRole::Bolt,
                GunKind::SlidePistol => PartRole::Slide,
                _ => PartRole::Toggle,
            };
            let mut beats = vec![
                Beat::new(K::MagOut, Left, Goal::Part(PartRole::Magazine))
                    .drive(PartRole::Magazine, 0.0, 0.5)
                    .carry(PartRole::Magazine)
                    .timed(0.2, 0.15)
                    .tilt(20.0, 10.0, 8.0)
                    .then(Event::Unload)
                    .throw(Throw::EmptyMagazine),
                Beat::new(K::ToPouch, Left, Goal::Pouch)
                    .carry(PartRole::Magazine)
                    .stow()
                    .timed(0.3, 0.12)
                    .tilt(15.0, 8.0, 6.0),
                Beat::new(K::MagIn, Left, Goal::Feed).carry(PartRole::Magazine).timed(0.32, 0.06).tilt(18.0, 10.0, 8.0),
                Beat::new(K::Seat, Left, Goal::Part(PartRole::Magazine))
                    .drive(PartRole::Magazine, 0.3, 0.0)
                    .grip(Grip::OPEN)
                    .timed(0.0, 0.1)
                    .tilt(12.0, 6.0, 4.0)
                    .then(Event::Load),
            ];
            let rack = |from: f32| {
                Beat::new(K::Rack, Left, Goal::Part(handle))
                    .drive(handle, from, 1.0)
                    .timed(0.22, 0.1)
                    .tilt(-10.0, -6.0, 0.0)
            };
            if kind.open_bolt() {
                // An open bolt is cocked each time, and stays back.
                beats.push(rack(0.0).then(Event::Open));
            } else {
                // A slide locked back is pulled from there; a closed one from rest, only if nothing is chambered.
                beats.push(rack(1.0).when(When::all([Fact::Open])));
                beats.push(rack(0.0).when(When::all([Fact::Closed, Fact::ChamberNotLive])));
                beats.push(
                    Beat::new(K::Release, Left, Goal::Part(handle))
                        .drive(handle, 1.0, 0.0)
                        .timed(0.0, 0.06)
                        .then(Event::Chamber)
                        .when(When::any([Fact::Open, Fact::ChamberNotLive])),
                );
            }
            beats
        }
        (GunKind::OpenBoltSmg, ActionKind::Cycle) => vec![
            Beat::new(K::Cock, Left, Goal::Part(PartRole::Bolt))
                .drive(PartRole::Bolt, 0.0, 1.0)
                .timed(0.22, 0.12)
                .then(Event::Open),
        ],
        (GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::Cycle) => {
            let part = if kind == GunKind::SlidePistol { PartRole::Slide } else { PartRole::Toggle };
            vec![
                Beat::new(K::Rack, Left, Goal::Part(part)).drive(part, 0.0, 1.0).timed(0.2, 0.1).then(Event::Eject),
                Beat::new(K::Release, Left, Goal::Part(part))
                    .drive(part, 1.0, 0.0)
                    .timed(0.0, 0.06)
                    .then(Event::Chamber),
            ]
        }
        (GunKind::SlidePistol | GunKind::TogglePistol, ActionKind::CheckChamber) => {
            let part = if kind == GunKind::SlidePistol { PartRole::Slide } else { PartRole::Toggle };
            vec![
                Beat::new(K::Press, Left, Goal::Part(part))
                    .drive(part, 0.0, 0.35)
                    .timed(0.22, 0.14)
                    .tilt(30.0, 0.0, 5.0),
                Beat::new(K::Look, Left, Goal::Part(part)).timed(0.0, 0.5).tilt(35.0, 0.0, 6.0).then(Event::SeeChamber),
                Beat::new(K::Close, Left, Goal::Part(part)).drive(part, 0.35, 0.0).timed(0.0, 0.1),
            ]
        }
        (GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol, _) => vec![
            Beat::new(K::DrawOut, Left, Goal::Part(PartRole::Magazine))
                .drive(PartRole::Magazine, 0.0, 0.35)
                .carry(PartRole::Magazine)
                .timed(0.2, 0.14)
                .tilt(15.0, 25.0, 10.0),
            Beat::new(K::Look, Left, Goal::Part(PartRole::Magazine))
                .carry(PartRole::Magazine)
                .timed(0.0, 0.55)
                .tilt(15.0, 30.0, 12.0)
                .then(Event::SeeFeed),
            Beat::new(K::Seat, Left, Goal::Part(PartRole::Magazine))
                .drive(PartRole::Magazine, 0.35, 0.0)
                .grip(Grip::OPEN)
                .timed(0.0, 0.1),
        ],

        // A belt: cocked, the cover up, a fresh belt laid and the cover slapped shut.
        (GunKind::BeltFedMg, ActionKind::Reload | ActionKind::Cycle) => {
            let mut beats = vec![
                bolt(K::Cock, 0.0, 1.0).timed(0.22, 0.14).then(Event::Open),
                bolt(K::Return, 1.0, 0.0).timed(0.0, 0.1),
            ];
            if action == ActionKind::Reload {
                beats.extend([
                    Beat::new(K::CoverUp, Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 0.0, 1.0)
                        .timed(0.25, 0.15)
                        .tilt(12.0, 8.0, 6.0)
                        .then(Event::Unload),
                    Beat::new(K::TakeBelt, Left, Goal::Pouch).timed(0.35, 0.12).tilt(10.0, 8.0, 4.0),
                    Beat::new(K::LayBelt, Left, Goal::Feed).timed(0.35, 0.18).tilt(14.0, 10.0, 8.0).then(Event::Load),
                    Beat::new(K::CoverDown, Left, Goal::Part(PartRole::Cover))
                        .drive(PartRole::Cover, 1.0, 0.0)
                        .grip(Grip::OPEN)
                        .timed(0.1, 0.08)
                        .tilt(8.0, 6.0, 4.0)
                        .knock(Knock::SlapShut),
                ]);
            }
            beats
        }
        (GunKind::BeltFedMg, _) => {
            vec![Beat::new(K::Look, Left, Goal::Feed).timed(0.25, 0.6).tilt(25.0, 15.0, 8.0).then(Event::SeeFeed)]
        }

        // A tube: one shell at a time into the port underneath, as many as it lacks; pumped if the chamber is empty.
        (GunKind::PumpShotgun, ActionKind::Reload) => vec![
            Beat::new(K::TakeShell, Left, Goal::Pouch)
                .timed(0.25, 0.05)
                .tilt(30.0, 0.0, 6.0)
                .repeat(Repeat::PerMissingRound),
            Beat::new(K::ShellIn, Left, Goal::Feed)
                .timed(0.22, 0.12)
                .tilt(35.0, 0.0, 8.0)
                .grip(Grip::OPEN)
                .then(Event::AddRounds(1))
                .repeat(Repeat::PerMissingRound),
            Beat::new(K::Pump, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 1.0)
                .grip(Grip::SUPPORT)
                .timed(0.15, 0.12)
                .then(Event::Eject)
                .when(When::all([Fact::ChamberNotLive])),
            Beat::new(K::Return, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 1.0, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1)
                .then(Event::Chamber)
                .when(When::all([Fact::ChamberNotLive])),
        ],
        (GunKind::PumpShotgun, ActionKind::Cycle) => vec![
            Beat::new(K::Pump, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 1.0)
                .grip(Grip::SUPPORT)
                .timed(0.08, 0.12)
                .then(Event::Eject),
            Beat::new(K::Return, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 1.0, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1)
                .then(Event::Chamber),
        ],
        (GunKind::PumpShotgun, _) => vec![
            Beat::new(K::Open, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.0, 0.3)
                .grip(Grip::SUPPORT)
                .timed(0.1, 0.12)
                .tilt(30.0, 0.0, 4.0),
            Beat::new(K::Look, Left, Goal::Part(PartRole::Pump))
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.5)
                .tilt(35.0, 0.0, 6.0)
                .then(Event::SeeChamber),
            Beat::new(K::Close, Left, Goal::Part(PartRole::Pump))
                .drive(PartRole::Pump, 0.3, 0.0)
                .grip(Grip::SUPPORT)
                .timed(0.0, 0.1),
        ],

        // A top-break revolver: tipped open, every case thrown out, each chamber loaded, and closed.
        (GunKind::TopBreakRevolver, ActionKind::Reload) => vec![
            Beat::new(K::Break, Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.0, 1.0)
                .timed(0.22, 0.2)
                .tilt(0.0, 0.0, -25.0)
                .then(Event::Unload),
            Beat::new(K::TakeRound, Right, Goal::Pouch)
                .timed(0.22, 0.05)
                .tilt(0.0, 0.0, -30.0)
                .repeat(Repeat::PerChamber),
            Beat::new(K::RoundIn, Right, Goal::Feed)
                .timed(0.2, 0.08)
                .tilt(0.0, 0.0, -30.0)
                .then(Event::AddRounds(1))
                .repeat(Repeat::PerChamber),
            Beat::new(K::Close, Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 1.0, 0.0)
                .grip(Grip::OPEN)
                .timed(0.15, 0.1)
                .then(Event::Close),
        ],
        (GunKind::TopBreakRevolver, _) => vec![
            Beat::new(K::Break, Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.0, 0.6)
                .timed(0.22, 0.15)
                .tilt(0.0, 15.0, -20.0),
            Beat::new(K::Look, Left, Goal::Part(PartRole::Barrel))
                .timed(0.0, 0.5)
                .tilt(0.0, 20.0, -25.0)
                .then(Event::SeeFeed),
            Beat::new(K::Close, Left, Goal::Part(PartRole::Barrel))
                .drive(PartRole::Barrel, 0.6, 0.0)
                .grip(Grip::OPEN)
                .timed(0.0, 0.1)
                .then(Event::SeeChamber),
        ],
        (GunKind::SemiAutoRifle, _) => vec![
            bolt(K::Open, 0.0, 0.35).timed(0.2, 0.15).tilt(-30.0, -8.0, 8.0),
            Beat::new(K::Look, Right, Goal::Part(PartRole::Bolt))
                .timed(0.0, 0.5)
                .tilt(-35.0, -10.0, 10.0)
                .then(Event::SeeFeed),
            bolt(K::Close, 0.35, 0.0).timed(0.0, 0.08).then(Event::SeeChamber),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::super::legacy_plan::legacy_plan;
    use super::*;
    use crate::guns::actions::plan;
    use crate::guns::mechanism::{Chamber, Feed, GunState};

    /// The key the planner before templates gave a beat by its name.
    fn key_of(name: &str) -> K {
        match name {
            "lift" => K::Lift,
            "pull" => K::Pull,
            "push" => K::Push,
            "turn" => K::Turn,
            "open" => K::Open,
            "look" => K::Look,
            "close" => K::Close,
            "take clip" => K::TakeClip,
            "clip in" => K::ClipIn,
            "press" => K::Press,
            "release" => K::Release,
            "mag out" => K::MagOut,
            "to pouch" => K::ToPouch,
            "mag in" => K::MagIn,
            "seat" => K::Seat,
            "rack" => K::Rack,
            "cock" => K::Cock,
            "return" => K::Return,
            "draw out" => K::DrawOut,
            "cover up" => K::CoverUp,
            "take belt" => K::TakeBelt,
            "lay belt" => K::LayBelt,
            "cover down" => K::CoverDown,
            "take shell" => K::TakeShell,
            "shell in" => K::ShellIn,
            "pump" => K::Pump,
            "break" => K::Break,
            "take round" => K::TakeRound,
            "round in" => K::RoundIn,
            other => panic!("no key for {other}"),
        }
    }

    /// Every mix of rounds, chamber, feed, open action and spare rounds.
    fn states(kind: GunKind) -> Vec<(GunState, u32)> {
        let mut states = Vec::new();
        for rounds in [0, 1, kind.capacity()] {
            for chamber in [Chamber::Empty, Chamber::Live, Chamber::Spent] {
                for (fed, open) in [(true, false), (true, true), (false, false), (false, true)] {
                    for spare in [0, 3, 100] {
                        let state = GunState { rounds, chamber, fed, open, ..GunState::loaded(kind) };
                        states.push((state, spare));
                    }
                }
            }
        }
        states
    }

    #[test]
    fn the_templates_play_exactly_the_beats_the_planner_before_them_did_in_every_state() {
        for kind in GunKind::ALL {
            for action in ActionKind::ALL {
                for (state, spare) in states(kind) {
                    let (now, before) = (plan(action, kind, &state, spare), legacy_plan(action, kind, &state, spare));
                    let at = format!("{kind:?} {action:?} {state:?} spare {spare}");
                    assert_eq!(now.len(), before.len(), "{at}: {now:?} {before:?}");
                    for (now, before) in now.iter().zip(&before) {
                        assert_eq!(now.key, key_of(before.name), "{at}");
                        assert_eq!((now.hand, now.goal, now.carry), (before.hand, before.goal, before.carry), "{at}");
                        assert_eq!(
                            (now.reach, now.work, now.grip, now.then),
                            (before.reach, before.work, before.grip, before.then)
                        );
                        assert_eq!(now.drive.map(|d| (d.part, d.from, d.to)), before.drive, "{at}");
                        assert_eq!(now.tilt, before.tilt.to_array(), "{at}");
                        // What the names used to decide, the typed fields decide now.
                        let knock = match (before.then, before.name) {
                            (Event::Chamber, _) => Knock::SlamHome,
                            (Event::Eject, _) => Knock::YankBack,
                            (Event::AddRounds(_), "press") => Knock::PressDown,
                            (Event::Load, _) => Knock::Seat,
                            (_, "cover down") => Knock::SlapShut,
                            _ => Knock::Nothing,
                        };
                        assert_eq!(now.knock.of(now.then), knock, "{at} {}", before.name);
                        let clip = before.then == Event::Chamber
                            && before.name == "push"
                            && action == ActionKind::Reload
                            && matches!(kind.feed(), Feed::Clips(_));
                        let magazine = before.then == Event::Unload && kind.feed() == Feed::Magazine;
                        let throw = if clip {
                            Throw::Clip
                        } else if magazine {
                            Throw::EmptyMagazine
                        } else {
                            Throw::Nothing
                        };
                        assert_eq!(now.throw, throw, "{at} {}", before.name);
                        assert_eq!(now.stow, before.name == "to pouch", "{at}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_template_reads_back_from_its_file_as_it_was_written() {
        for kind in GunKind::ALL {
            for action in ActionKind::ALL {
                let spec = template(kind, action);
                let text = toml::to_string(&spec).unwrap();
                let read: ActionSpec = toml::from_str(&text).unwrap();
                assert_eq!(read, spec, "{kind:?} {action:?}:\n{text}");
            }
        }
    }
}
