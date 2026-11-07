//! An action playing: its beats one after another, each hand's way to its
//! goal timed as a person's reach is, quick to start and soft to arrive,
//! from wherever the hand was, so nothing jumps. While a beat works, the
//! part it drives moves with the hand, and the part it carries goes where
//! the hand goes. As each beat ends, what it does happens to the gun.

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Hidden, Transform};

use super::actions::{ActionKind, Beat, Goal, Hand, apply};
use super::gun::Gun;
use super::hands::Grip;
use super::mechanism::{GunPart, GunState, PartRole};

/// An action under way.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionPlay {
    pub kind: ActionKind,
    pub beats: Vec<Beat>,
    pub index: usize,
    /// Seconds into the beat.
    pub t: f32,
    /// Where the beat's hand started, in the gun's space, and whether it
    /// set out from its hold.
    pub from: Option<Vec3>,
    pub rested: bool,
    /// Where each hand was last, left then right.
    pub hands: [Option<Vec3>; 2],
    /// Whether the magazine taken out was empty and dropped, so the hand
    /// goes to the pouch with nothing in it.
    pub dropped: bool,
    /// Knocks the gun took as beats ended, for its holder to give it: a
    /// spin of its sway, and a push of its lag, since last asked.
    pub knock: (Vec3, Vec3),
    /// How far the action lifts the gun up before the eyes, whether it
    /// takes the gun down from them, and an offset on where it brings it.
    pub lift: f32,
    pub lowers_aim: bool,
    pub work: super::handling::Place,
}

/// What a beat throwing something away needs: the gun's place and turn in
/// the world, the velocity it carries, the time, and a number from -1 to 1
/// so no two throws are alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throwing {
    pub held: Mat4,
    pub velocity: Vec3,
    pub now: f64,
    pub seed: f32,
}

impl ActionPlay {
    /// A built-in action started on a gun of `gun` in `state`, as its
    /// handling has it, its holder having `spare` rounds; none when it has
    /// nothing to do.
    pub fn start(
        kind: ActionKind,
        gun: super::gun::GunKind,
        handling: &super::handling::GunHandling,
        state: &GunState,
        spare: u32,
    ) -> Option<Self> {
        let spec = handling.spec(gun, kind);
        let beats = super::actions::expand(&spec, gun, state, spare);
        let started = Self { lift: spec.lift, lowers_aim: spec.lowers_aim, work: spec.work, ..Self::new(kind, beats) };
        (!started.beats.is_empty()).then_some(started)
    }

    pub fn new(kind: ActionKind, beats: Vec<Beat>) -> Self {
        Self {
            kind,
            beats,
            index: 0,
            t: 0.0,
            from: None,
            rested: false,
            hands: [None; 2],
            dropped: false,
            knock: (Vec3::ZERO, Vec3::ZERO),
            lift: 1.0,
            lowers_aim: true,
            work: super::handling::Place::default(),
        }
    }

    pub fn beat(&self) -> Option<&Beat> {
        self.beats.get(self.index)
    }
}

/// Where a hand working goes this frame, in the gun's space: the point its
/// fingers hold, how far they may close, how far the hand is settled into
/// its hold on the gun, 0 working to 1 holding as it does at rest, the way
/// it comes in to what it holds, out from it, none for a pouch, and how
/// much more it is turned than it would be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandAim {
    pub hand: Hand,
    pub at: Vec3,
    pub grip: Grip,
    pub settle: f32,
    pub toward: Vec3,
    pub turn: Quat,
}

/// The hand's place at rest on the gun, in the gun's space.
pub fn socket(gun: &Gun, hand: Hand) -> Vec3 {
    Vec3::from(match hand {
        Hand::Left => gun.support.position,
        Hand::Right => gun.grip.position,
    })
}

/// A part's Obj and data by its role, if the gun has one.
fn part<'a>(world: &'a World, state: &GunState, role: PartRole) -> Option<(usize, Entity, &'a GunPart)> {
    state.parts.iter().enumerate().find_map(|(index, (entity, ..))| {
        world.get::<GunPart>(*entity).filter(|part| part.role == role).map(|part| (index, *entity, part))
    })
}

/// Where a goal is now, in the gun's space.
fn goal_at(world: &World, gun: &Gun, state: &GunState, goal: Goal, hand: Hand, pouches: [Vec3; 2]) -> Vec3 {
    match goal {
        Goal::Part(role) => match part(world, state, role) {
            Some((index, _, part)) => part.moved(state.parts[index].1).transform_point3(Vec3::from(part.handle)),
            None => Vec3::from(gun.port),
        },
        Goal::Feed => Vec3::from(gun.feed),
        Goal::Port => Vec3::from(gun.port),
        Goal::Pouch => pouches[hand as usize],
        Goal::Hold => socket(gun, hand),
    }
}

/// Plays `dt` seconds of an action: its parts driven and carried, its
/// events happening as beats end. Gives back the working hand's aim and
/// the gun's tilt, or `None` once the action is over.
#[allow(clippy::too_many_arguments)]
pub fn advance(
    world: &mut World,
    play: &mut ActionPlay,
    gun: &Gun,
    state: &mut GunState,
    spare: &mut u32,
    pouches: [Vec3; 2],
    throwing: Throwing,
    dt: f32,
) -> Option<(HandAim, Vec3)> {
    play.t += dt;
    loop {
        let beat = *play.beat()?;
        if play.t < beat.length() {
            break;
        }
        // The beat is done: its part comes to rest where it was driven to, and what it does happens.
        if let Some(drive) = beat.drive
            && let Some((index, ..)) = part(world, state, drive.part)
        {
            state.parts[index].1 = drive.to;
        }
        let (chamber, rounds) = (state.chamber, state.rounds);
        apply(beat.then, gun.kind, state, spare);
        throw_away(world, play, &beat, gun, state, (chamber, rounds), throwing);
        knock(play, &beat);
        play.t -= beat.length();
        play.index += 1;
        play.from = None;
        play.beat()?;
    }
    let beat = *play.beat()?;
    let goal = goal_at(world, gun, state, beat.goal, beat.hand, pouches) + Vec3::from(beat.at);
    if play.from.is_none() {
        play.rested = play.hands[beat.hand as usize].is_none();
    }
    let toward = approach(beat.goal, gun, beat.hand);
    let from =
        *play.from.get_or_insert_with(|| play.hands[beat.hand as usize].unwrap_or_else(|| socket(gun, beat.hand)));
    let at = if play.t < beat.reach {
        // A reach: out from the gun on a slight arc, timed as a person's reach is, and the last few centimetres
        // straight in along the way the goal is come at, so the hand does not cut through the gun.
        let s = minimum_jerk(play.t / beat.reach.max(1e-4));
        // Out to the hand's side first, further for a pouch on the body, so it goes round the gun, not through it.
        let side = if beat.hand == Hand::Right { 1.0 } else { -1.0 };
        let out = Vec3::new(side, 0.4, 0.0) * if beat.goal == Goal::Pouch { 0.16 } else { 0.06 };
        curve([from, from + out, goal + toward * 0.08, goal], s)
    } else {
        let work = minimum_jerk((play.t - beat.reach) / beat.work.max(1e-4));
        if let Some(drive) = beat.drive
            && let Some((index, ..)) = part(world, state, drive.part)
        {
            state.parts[index].1 = drive.from + (drive.to - drive.from) * work;
        }
        goal_at(world, gun, state, beat.goal, beat.hand, pouches) + Vec3::from(beat.at)
    };
    play.hands[beat.hand as usize] = Some(at);
    let grip = if play.t < beat.reach * 0.7 { Grip::OPEN.lerp(beat.grip, 0.3) } else { beat.grip };
    // A hand leaving its hold eases out of it over the first half of its reach.
    let settle = if play.rested { 1.0 - minimum_jerk(play.t / (beat.reach * 0.5).max(1e-4)) } else { 0.0 };
    let [x, y, z] = beat.turn.map(f32::to_radians);
    let turn = Quat::from_euler(voxear_core::math::EulerRot::XYZ, x, y, z);
    Some((HandAim { hand: beat.hand, at, grip, settle, toward, turn }, Vec3::from(beat.tilt)))
}

/// The way a hand comes in to a goal, in the gun's space, out from it: to
/// a magazine from below, to a part's handle from its side and above, to a
/// feed from the side it opens on, to the port from the side; to a pouch
/// or its hold, from wherever it is.
fn approach(goal: Goal, gun: &Gun, hand: Hand) -> Vec3 {
    let side = if hand == Hand::Right { 1.0 } else { -1.0 };
    match goal {
        Goal::Part(PartRole::Magazine) => Vec3::NEG_Y,
        Goal::Part(_) => Vec3::new(side * 0.7, 0.6, -0.2).normalize(),
        Goal::Feed if gun.feed[1] < 0.0 => Vec3::NEG_Y,
        Goal::Feed => Vec3::new(side * 0.15, 1.0, 0.0).normalize(),
        Goal::Port => Vec3::new(side, 0.3, 0.0).normalize(),
        Goal::Pouch | Goal::Hold => Vec3::ZERO,
    }
}

/// A smooth path through four points, from the first to the last, leaving
/// toward the second and coming in from the third, `s` of the way along.
pub fn curve([a, b, c, d]: [Vec3; 4], s: f32) -> Vec3 {
    let r = 1.0 - s;
    a * (r * r * r) + b * (3.0 * r * r * s) + c * (3.0 * r * s * s) + d * (s * s * s)
}

/// Puts the part the beat under way carries where its hand is now.
pub fn carry_now(world: &mut World, state: &GunState, play: &ActionPlay) {
    let hand = play.beat().map(|beat| beat.hand as usize);
    if let Some(at) = hand.and_then(|hand| play.hands[hand]) {
        carry(world, state, play, at);
    }
}

/// Puts the part a beat carries where its hand is, and shows clips only
/// while a hand has one.
fn carry(world: &mut World, state: &GunState, play: &ActionPlay, at: Vec3) {
    let carried = play.beat().and_then(|beat| beat.carry);
    let driven = play.beat().and_then(|beat| beat.drive.map(|drive| drive.part));
    for &(entity, place, rest) in &state.parts {
        let Some(part) = world.get::<GunPart>(entity).cloned() else { continue };
        // An empty magazine dropped leaves the hand empty on its way to the pouch.
        if part.role == PartRole::Magazine {
            let away = play.dropped && play.beat().is_some_and(|beat| beat.stow);
            match away {
                true => drop(world.insert(entity, Hidden)),
                false => drop(world.remove::<Hidden>(entity)),
            }
        }
        if part.role == PartRole::Clip {
            let shown = carried == Some(PartRole::Clip) || driven == Some(PartRole::Clip);
            match shown {
                true => drop(world.remove::<Hidden>(entity)),
                false => drop(world.insert(entity, Hidden)),
            }
        }
        if carried != Some(part.role) || (driven == Some(part.role) && play.t >= play.beat().map_or(0.0, |b| b.reach)) {
            continue;
        }
        let moved = Mat4::from_translation(at - part.moved(place).transform_point3(Vec3::from(part.handle)))
            * part.moved(place);
        let (scale, rotation, position) = (moved * rest).to_scale_rotation_translation();
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            (transform.position, transform.rotation, transform.scale) = (position, rotation, scale);
        }
    }
}

/// The knock a beat gives the gun as it ends: a bolt slammed home or
/// yanked back, a clip pressed down, a magazine seated, a cover slapped
/// shut. Each is a spin (up, aside, roll) and a push along the gun.
fn knock(play: &mut ActionPlay, beat: &Beat) {
    use super::actions::Knock;
    let (spin, push) = match beat.knock.of(beat.then) {
        Knock::SlamHome => (Vec3::new(-1.2, 0.3, 0.6), Vec3::new(0.0, 0.0, 0.05)),
        Knock::YankBack => (Vec3::new(0.6, -0.2, -0.8), Vec3::new(0.01, 0.0, -0.06)),
        Knock::PressDown => (Vec3::new(1.4, 0.0, 0.0), Vec3::new(0.0, -0.06, 0.0)),
        Knock::Seat => (Vec3::new(-1.6, 0.2, 0.0), Vec3::new(0.0, 0.07, 0.0)),
        Knock::SlapShut => (Vec3::new(1.8, 0.0, 0.0), Vec3::new(0.0, -0.06, 0.0)),
        Knock::ByEvent | Knock::Nothing => return,
    };
    play.knock.0 += spin;
    play.knock.1 += push;
}

/// Throws away what a beat that just ended leaves: a case ejected by hand,
/// an empty magazine dropped, a stripper clip flicked off as the bolt
/// closes over it.
fn throw_away(
    world: &mut World,
    play: &mut ActionPlay,
    beat: &Beat,
    gun: &Gun,
    state: &GunState,
    (chamber, rounds): (super::mechanism::Chamber, u32),
    throwing: Throwing,
) {
    use super::actions::{Event, Throw};
    use super::leftovers::{throw, throw_case};
    use super::mechanism::{Chamber, Feed};
    let Throwing { held, velocity, now, seed } = throwing;
    let (right, up) = (held.transform_vector3(Vec3::X), held.transform_vector3(Vec3::Y));
    if beat.then == Event::Eject && chamber != Chamber::Empty {
        throw_case(world, state, held, velocity, now, seed);
    }
    match beat.throw {
        Throw::EmptyMagazine if rounds == 0 => {
            play.dropped = true;
            let down = velocity - up * 1.2 + right * seed * 0.3;
            throw(
                world,
                state,
                PartRole::Magazine,
                "Empty Magazine",
                down,
                right * 4.0,
                Vec3::new(0.03, 0.18, 0.07),
                now,
            );
        }
        Throw::Clip if matches!(gun.kind.feed(), Feed::Clips(_)) => {
            let flick = velocity + right * 1.8 + up * 1.5;
            throw(world, state, PartRole::Clip, "Stripper Clip", flick, up * 12.0, Vec3::new(0.012, 0.05, 0.06), now);
        }
        _ => {}
    }
}

/// The share of the way a person's reach has gone at `t`, 0 to 1: quick to
/// start, quickest midway, and soft to arrive.
pub fn minimum_jerk(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (10.0 - 15.0 * t + 6.0 * t * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reach_starts_and_ends_at_rest_and_is_quickest_midway() {
        assert_eq!((minimum_jerk(0.0), minimum_jerk(1.0)), (0.0, 1.0));
        let speed = |t: f32| (minimum_jerk(t + 0.01) - minimum_jerk(t)) / 0.01;
        assert!(speed(0.0) < 0.01 && speed(0.99) < 0.05);
        assert!(speed(0.5) > speed(0.25) && speed(0.5) > speed(0.75));
    }
}
