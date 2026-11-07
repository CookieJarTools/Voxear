//! An action playing: its beats one after another, each hand's way to its
//! goal timed as a person's reach is, quick to start and soft to arrive,
//! from wherever the hand was, so nothing jumps. While a beat works, the
//! part it drives moves with the hand, and the part it carries goes where
//! the hand goes. As each beat ends, what it does happens to the gun.

use voxear_core::math::{Mat4, Vec3};
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
    /// Where the beat's hand started, in the gun's space.
    pub from: Option<Vec3>,
    /// Where each hand was last, left then right.
    pub hands: [Option<Vec3>; 2],
}

impl ActionPlay {
    pub fn new(kind: ActionKind, beats: Vec<Beat>) -> Self {
        Self { kind, beats, index: 0, t: 0.0, from: None, hands: [None; 2] }
    }

    pub fn beat(&self) -> Option<&Beat> {
        self.beats.get(self.index)
    }
}

/// Where a hand working goes this frame, in the gun's space: the point its
/// fingers hold, and how they close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandAim {
    pub hand: Hand,
    pub at: Vec3,
    pub grip: Grip,
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
pub fn advance(
    world: &mut World,
    play: &mut ActionPlay,
    gun: &Gun,
    state: &mut GunState,
    spare: &mut u32,
    pouches: [Vec3; 2],
    dt: f32,
) -> Option<(HandAim, Vec3)> {
    play.t += dt;
    loop {
        let beat = *play.beat()?;
        if play.t < beat.length() {
            break;
        }
        // The beat is done: its part comes to rest where it was driven to, and what it does happens.
        if let Some((role, _, to)) = beat.drive
            && let Some((index, ..)) = part(world, state, role)
        {
            state.parts[index].1 = to;
        }
        apply(beat.then, gun.kind, state, spare);
        play.t -= beat.length();
        play.index += 1;
        play.from = None;
        if play.beat().is_none() {
            return None;
        }
    }
    let beat = *play.beat()?;
    let goal = goal_at(world, gun, state, beat.goal, beat.hand, pouches);
    let from =
        *play.from.get_or_insert_with(|| play.hands[beat.hand as usize].unwrap_or_else(|| socket(gun, beat.hand)));
    let at = if play.t < beat.reach {
        // A reach: straight for its goal on a slight arc out from the gun, timed as a person's reach is.
        let s = minimum_jerk(play.t / beat.reach.max(1e-4));
        let out = Vec3::new(if beat.hand == Hand::Right { 1.0 } else { -1.0 }, 0.4, 0.0) * 0.05;
        from.lerp(goal, s) + out * (s * std::f32::consts::PI).sin()
    } else {
        let work = minimum_jerk((play.t - beat.reach) / beat.work.max(1e-4));
        if let Some((role, start, end)) = beat.drive
            && let Some((index, ..)) = part(world, state, role)
        {
            state.parts[index].1 = start + (end - start) * work;
        }
        goal_at(world, gun, state, beat.goal, beat.hand, pouches)
    };
    play.hands[beat.hand as usize] = Some(at);
    let grip = if play.t < beat.reach * 0.7 { Grip::OPEN.lerp(beat.grip, 0.3) } else { beat.grip };
    Some((HandAim { hand: beat.hand, at, grip }, beat.tilt))
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
    let driven = play.beat().and_then(|beat| beat.drive.map(|(role, ..)| role));
    for &(entity, place, rest) in &state.parts {
        let Some(part) = world.get::<GunPart>(entity).cloned() else { continue };
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
