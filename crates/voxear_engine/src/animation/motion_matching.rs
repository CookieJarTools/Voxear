//! Motion matching: rather than clips chosen by hand, a Motion Matcher
//! plays whichever of its model's looping clips move as the character is
//! wanted to, blending the few that match best. Ten times a second, or at
//! once when what is wanted changes, it searches every frame of the clips
//! for how its root moves there: its speed, its direction and the path
//! ahead, against the velocity wanted, as seen from the body. Each clip's
//! weight fades towards how well it matched, so nothing pops; below a walk,
//! a clip that stands still fades in instead.
//!
//! The clips blended play in step, each at the same share of the way
//! through its loop, so their feet fall together, and a little faster or
//! slower so their feet keep pace with the ground the Obj covers. Where
//! the clips have a foot down, it stays where it came down, the leg bending
//! to reach it, until the clips lift it or the body has gone too far from
//! it; then it eases back to the clips' foot.
//!
//! The game moves the Obj, by a Character Controller or otherwise, and the
//! model stands on it. The body turns, at its turn rate, the way it is
//! wanted to face, or the way it goes. Going another way than it faces,
//! its hips and legs turn towards the way it goes, as far as its leg turn
//! allows, and its spine twists back so its chest still faces ahead: so a
//! walk forward carries it forward and to the side, and a walk back, back
//! and to the side, turning smoothly between them.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::ease::smooth;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, ModelAnimation, ModelRenderer, Pose, Skeleton};
use voxear_scene::{Destroying, Name, Transform};

use super::{
    actions::{self, ActionCommand, ActionPlay, MotionMarks},
    clips::Clips,
    motion_database::{ClipRun, Ground, MotionDatabase, facing, wrap},
    motion_set::{ActionMovement, MARK_PART, MotionSet, StateRole},
    retarget::fitted,
    settling::Settling,
    states::{StatePlay, state_play},
};

/// How quickly the Obj's seen velocity follows its movement, in seconds to
/// halve the gap: enough to smooth a frame's jitter.
const SEEN_HALFLIFE: f32 = 0.05;
/// How many of the best clips blend.
const CANDIDATES: usize = 4;
/// How sharply a better match outweighs a worse one.
const SHARPNESS: f32 = 2.5;
/// How nearly a clip must head the way wanted to blend: within about 18°.
const ALIGNED: f32 = 0.95;
/// How much the velocity wanted must change, in metres a second, to search
/// again at once.
const CHANGED: f32 = 0.25;
/// How much faster or slower clips may play to keep pace with the ground.
const PACE: (f32, f32) = (0.75, 1.33);
/// How quickly the body, and its legs, ease round to a new way, in seconds
/// to halve the gap.
const TURN_HALFLIFE: f32 = 0.08;
/// How long, in seconds to halve it, the hips take to settle round when the
/// legs change way, as from forward to back.
const FLIP_HALFLIFE: f32 = 0.15;
/// How far, in metres, a foot held down may be from where the clips put
/// it before it lets go.
const REACH: f32 = 0.3;
/// How much of a look up or down the spine bends, the neck taking the
/// rest, and how far, in degrees, at most.
const AIM_SHARE: f32 = 0.3;
const AIM_BEND: f32 = 20.0;
/// How long, in seconds, a foot let go takes to ease back to the clips'.
const LET_GO: f32 = 0.15;
/// How much nearer, in radians, another clip's way must be to the way
/// it goes before the legs change to it: about 20°.
const STICK: f32 = 0.35;

/// Animates its Obj's model by motion matching over the model's looping
/// clips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionMatcher {
    /// The velocity the game wants, in metres a second; none expects the
    /// Obj to carry on as it moves.
    #[serde(skip)]
    pub wanted_velocity: Option<Vec3>,
    /// The way the game wants the body to face, as when aiming while
    /// stepping aside or back; none faces the way it goes.
    #[serde(skip)]
    pub wanted_facing: Option<Vec3>,
    /// How far, in degrees, the game wants the body to bend at the spine to
    /// look up or down, down positive, so its arms aim where it looks.
    #[serde(skip)]
    pub aim_pitch: f32,
    /// The state the game wants it in, by its role or its name in the set;
    /// none keeps it in the one it is in, from the set's first walking one.
    #[serde(skip)]
    pub state: Option<StateChoice>,
    /// What it can do: its Motion Set, `Characters/Wizard.motionset`; none
    /// plays the model's own loops.
    pub motion_set: String,
    /// Actions the game asked to play or stop since the last frame.
    #[serde(skip)]
    pub commands: Vec<ActionCommand>,
    /// Moving the Obj as its clips carry it, rather than the game moving it
    /// and the clips following.
    pub root_motion: bool,
    /// Below this many metres a second wanted, it stands.
    pub idle_speed: f32,
    /// Seconds between searches.
    pub search_every: f32,
    /// How long, in seconds, a change of clips takes to fade in.
    pub blend: f32,
    /// How fast the body turns, in degrees a second; 0 turns at once.
    pub turn_rate: f32,
    /// Facing the way it goes when nothing else is wanted, rather than the
    /// Obj's forward.
    pub turn_to_movement: bool,
    /// How far, in degrees, the hips and legs may turn from the way the
    /// body faces, towards the way it goes; 0 keeps them with the body.
    pub leg_turn: f32,
    /// Playing clips a little faster or slower so their feet keep pace with
    /// the ground.
    pub match_speed: bool,
    /// Holding each foot where it comes down while the clips have it down,
    /// the leg bending to reach it.
    pub foot_locking: bool,
    /// Drawing the velocity wanted, the clips' velocity and the body's
    /// facing, and which clips play.
    pub debug: bool,
}

impl Component for MotionMatcher {}

/// A state of a Motion Set a game asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum StateChoice {
    /// The first with this role.
    Role(StateRole),
    /// The one with this name.
    Named(String),
}

impl Default for MotionMatcher {
    fn default() -> Self {
        Self {
            wanted_velocity: None,
            wanted_facing: None,
            aim_pitch: 0.0,
            state: None,
            motion_set: String::new(),
            commands: Vec::new(),
            root_motion: false,
            idle_speed: 0.15,
            search_every: 0.1,
            blend: 0.15,
            turn_rate: 540.0,
            turn_to_movement: true,
            leg_turn: 90.0,
            match_speed: true,
            foot_locking: true,
            debug: false,
        }
    }
}

/// What a Motion Matcher is doing, kept by the engine.
#[derive(Clone, Default)]
pub struct MotionMatcherState {
    /// The state playing, by its place in the set, ready to play.
    state: usize,
    play: Option<Arc<StatePlay>>,
    /// Each run of the database's weight, and the weight it fades towards.
    weights: Vec<f32>,
    targets: Vec<f32>,
    idle_weight: f32,
    /// How far through their loops the clips playing are, from 0 to 1.
    phase: f32,
    idle_time: f32,
    /// How much faster than made the clips play.
    rate: f32,
    since_search: f32,
    searched: Option<Vec3>,
    /// The way the body faces in the world.
    body_yaw: Option<f32>,
    /// The way it goes in the world, eased, and the way, as seen from their
    /// roots, of the clips its legs take it there with.
    heading: Option<f32>,
    stride: Option<f32>,
    /// The last two poses shown, and the offset a change of the legs' way
    /// is still settling.
    shown: Vec<BonePose>,
    before: Vec<BonePose>,
    settling: Option<Settling>,
    /// Each foot held down, left then right, and whether the clips had it
    /// down last frame.
    locks: [Option<FootLock>; 2],
    down: [bool; 2],
    /// The action playing over the state's clips.
    action: Option<ActionPlay>,
    /// The Obj's place last frame, and its velocity as seen.
    last: Option<Vec3>,
    velocity: Vec3,
    /// How the clips carry the body this frame, in the world, in metres a
    /// second, whether or not they move the Obj.
    travel: Vec3,
    /// Whether the action playing stands the character or moves it itself.
    taken: bool,
}

impl Component for MotionMatcherState {}

/// A foot held where it came down: where in the world, how much it holds,
/// fading once let go, and whether it still holds.
#[derive(Clone, Copy, Debug)]
struct FootLock {
    at: Vec3,
    weight: f32,
    held: bool,
}

impl MotionMatcherState {
    /// Moves where the Obj was last and where its feet are held by `by`, as
    /// the world's origin moves everything (see `crate::origin`), so the move
    /// reads as no step at all.
    pub(crate) fn shift(world: &mut World, by: Vec3) {
        for state in world.query::<&mut MotionMatcherState>() {
            if let Some(last) = state.last.as_mut() {
                *last += by;
            }
            for lock in state.locks.iter_mut().flatten() {
                lock.at += by;
            }
        }
    }

    /// Which feet the clips have down, left then right: a foot coming down
    /// is a footfall, for what bobs with the steps.
    pub fn feet_down(&self) -> [bool; 2] {
        self.down
    }

    /// Each clip playing, by its name, and its share of the pose, the clip
    /// it stands with included, most first.
    pub fn playing(&self) -> Vec<(String, f32)> {
        let Some(play) = &self.play else { return Vec::new() };
        let walking = 1.0 - self.idle_weight;
        let name = |clip: usize| play.animation.clips[clip].name.clone();
        let mut out: Vec<(String, f32)> = play
            .database
            .runs
            .iter()
            .zip(&self.weights)
            .filter(|(_, w)| **w * walking > 1e-3)
            .map(|(run, w)| (name(run.clip), w * walking))
            .collect();
        if let Some(idle) = play.idle.filter(|_| self.idle_weight > 1e-3) {
            out.push((name(idle), self.idle_weight));
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }

    /// The state playing, by its place in its set.
    pub fn state(&self) -> usize {
        self.state
    }

    /// The way the body faces in the world, as a turn about Y from +Z.
    pub fn body_yaw(&self) -> Option<f32> {
        self.body_yaw
    }

    /// How much faster than made the clips play.
    pub fn rate(&self) -> f32 {
        self.rate
    }
}

/// Sets what answers `MotionMatcher` in scripts.
pub fn register() {
    voxear_scripting::set_motion_matcher_values(values);
    voxear_scripting::set_motion_matcher_texts(texts);
}

// What `values` reads or writes, as `NativeMotionMatcher` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const WANTED_VELOCITY: i32 = 3;
const WANTED_FACING: i32 = 4;
const FOLLOW: i32 = 5;
const IDLE_SPEED: i32 = 6;
const SEARCH_EVERY: i32 = 7;
const BLEND: i32 = 8;
const TURN_RATE: i32 = 9;
const TURN_TO_MOVEMENT: i32 = 10;
const MATCH_SPEED: i32 = 11;
const DEBUG: i32 = 12;
const LEG_TURN: i32 = 13;
const FOOT_LOCKING: i32 = 14;
const AIM_PITCH: i32 = 15;
const STATE_ROLE: i32 = 16;
const ROOT_MOTION: i32 = 17;
const ACTION_TIME: i32 = 18;
const CLIP_VELOCITY: i32 = 19;
const ACTION_TAKES_MOVEMENT: i32 = 20;
const FEET_DOWN: i32 = 21;

// What `texts` reads or does, as `NativeMotionMatcher` in Voxear.Core numbers them.
const MOTION_SET: i32 = 0;
const SET_MOTION_SET: i32 = 1;
const PLAY: i32 = 2;
const STOP: i32 = 3;
const STATE_NAME: i32 = 4;
const SET_STATE: i32 = 5;
const ACTION_NAME: i32 = 6;

/// Reads value `what` of `entity`'s Motion Matcher, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let toggle = |on: bool| number(f32::from(u8::from(on)));
    let vector = |v: Option<Vec3>| v.map_or([0.0; 4], |v| [v.x, v.y, v.z, 1.0]);
    match (what, written) {
        (HAS, None) => return Some(toggle(world.has::<MotionMatcher>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<MotionMatcher>(entity) {
                world.insert(entity, MotionMatcher::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<MotionMatcherState>(entity);
            world.remove::<Pose>(entity);
            return world.remove::<MotionMatcher>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    match (what, written) {
        (STATE_ROLE, None) => {
            let role = playing_state(world, entity).map(|(set, state)| set.states[state].role);
            let index = StateRole::ALL.iter().position(|r| Some(*r) == role).map_or(-1.0, |i| i as f32);
            return Some(number(index));
        }
        (CLIP_VELOCITY, None) => {
            world.get::<MotionMatcher>(entity)?;
            return Some(vector(world.get::<MotionMatcherState>(entity).map(|state| state.travel)));
        }
        (ACTION_TAKES_MOVEMENT, None) => {
            world.get::<MotionMatcher>(entity)?;
            return Some(toggle(world.get::<MotionMatcherState>(entity).is_some_and(|state| state.taken)));
        }
        // Each foot, then whether the matcher has played a frame to have them down at all.
        (FEET_DOWN, None) => {
            world.get::<MotionMatcher>(entity)?;
            let feet = world.get::<MotionMatcherState>(entity).map(MotionMatcherState::feet_down);
            let flag = |on: bool| f32::from(u8::from(on));
            return Some(feet.map_or([0.0; 4], |[left, right]| [flag(left), flag(right), 0.0, 1.0]));
        }
        (ACTION_TIME, None) => {
            let pending = world.get::<MotionMatcher>(entity)?.commands.iter().any(|c| matches!(c, ActionCommand::Play(_)));
            let action = world.get::<MotionMatcherState>(entity).and_then(|state| state.action.as_ref());
            return Some(number(action.filter(|_| !pending).map_or(0.0, |action| action.time)));
        }
        _ => {}
    }
    let matcher = world.get_mut::<MotionMatcher>(entity)?;
    match written {
        None => Some(match what {
            WANTED_VELOCITY => vector(matcher.wanted_velocity),
            WANTED_FACING => vector(matcher.wanted_facing),
            IDLE_SPEED => number(matcher.idle_speed),
            SEARCH_EVERY => number(matcher.search_every),
            BLEND => number(matcher.blend),
            TURN_RATE => number(matcher.turn_rate),
            TURN_TO_MOVEMENT => toggle(matcher.turn_to_movement),
            MATCH_SPEED => toggle(matcher.match_speed),
            DEBUG => toggle(matcher.debug),
            LEG_TURN => number(matcher.leg_turn),
            FOOT_LOCKING => toggle(matcher.foot_locking),
            AIM_PITCH => number(matcher.aim_pitch),
            ROOT_MOTION => toggle(matcher.root_motion),
            _ => return None,
        }),
        Some([x, y, z, w]) => {
            let given = Vec3::new(x, y, z);
            match what {
                WANTED_VELOCITY => matcher.wanted_velocity = Some(given),
                // No way at all faces the way it goes again.
                WANTED_FACING => matcher.wanted_facing = (given.length_squared() > 1e-8).then_some(given),
                FOLLOW => (matcher.wanted_velocity, matcher.wanted_facing) = (None, None),
                IDLE_SPEED => matcher.idle_speed = x.clamp(0.0, 5.0),
                SEARCH_EVERY => matcher.search_every = x.clamp(0.02, 1.0),
                BLEND => matcher.blend = x.clamp(0.0, 2.0),
                TURN_RATE => matcher.turn_rate = x.clamp(0.0, 3600.0),
                TURN_TO_MOVEMENT => matcher.turn_to_movement = x != 0.0,
                MATCH_SPEED => matcher.match_speed = x != 0.0,
                DEBUG => matcher.debug = x != 0.0,
                LEG_TURN => matcher.leg_turn = x.clamp(0.0, 180.0),
                FOOT_LOCKING => matcher.foot_locking = x != 0.0,
                AIM_PITCH => matcher.aim_pitch = x.clamp(-90.0, 90.0),
                ROOT_MOTION => matcher.root_motion = x != 0.0,
                STATE_ROLE => {
                    let role = StateRole::ALL.get(x.max(0.0) as usize).copied().unwrap_or_default();
                    matcher.state = Some(StateChoice::Role(role));
                }
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

/// Reads text `what` of `entity`'s Motion Matcher, or does what it asks with
/// `text`: an empty answer for done, `None` when it has no matcher, or no
/// such action or state is playing.
fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if !world.is_alive(entity) {
        return None;
    }
    match what {
        STATE_NAME => return playing_state(world, entity).map(|(set, state)| set.states[state].name.clone()),
        ACTION_NAME => return action_name(world, entity),
        _ => {}
    }
    let matcher = world.get_mut::<MotionMatcher>(entity)?;
    match what {
        MOTION_SET => return Some(matcher.motion_set.clone()),
        SET_MOTION_SET if matcher.motion_set != text => {
            matcher.motion_set = text.to_string();
            (matcher.state, matcher.commands) = (None, Vec::new());
            // The old set's state and action mean nothing in the new one.
            if let Some(state) = world.get_mut::<MotionMatcherState>(entity) {
                (state.state, state.action) = (usize::MAX, None);
            }
        }
        SET_MOTION_SET => {}
        PLAY => matcher.commands.push(ActionCommand::Play(text.to_string())),
        STOP => matcher.commands.push(ActionCommand::Stop),
        SET_STATE => matcher.state = Some(StateChoice::Named(text.to_string())),
        _ => return None,
    }
    Some(String::new())
}

/// The set an Obj's Motion Matcher plays and the state of it playing, or to
/// play from the next frame as the game asked.
fn playing_state(world: &World, entity: Entity) -> Option<(Arc<MotionSet>, usize)> {
    let matcher = world.get::<MotionMatcher>(entity)?;
    let set = world.resource::<Clips>()?.set(&matcher.motion_set)?.clone();
    let state = world.get::<MotionMatcherState>(entity).cloned().unwrap_or_default();
    let index = chosen(&set, matcher, &state);
    (index < set.states.len()).then_some((set, index))
}

/// The name of the action an Obj's Motion Matcher plays, or plays from the
/// next frame as the game asked: `None` with none, or one stopped.
fn action_name(world: &World, entity: Entity) -> Option<String> {
    let matcher = world.get::<MotionMatcher>(entity)?;
    match matcher.commands.last() {
        Some(ActionCommand::Play(name)) => return Some(name.clone()),
        Some(ActionCommand::Stop) => return None,
        None => {}
    }
    let action = world.get::<MotionMatcherState>(entity)?.action.as_ref().filter(|action| !action.ending)?;
    let set = world.resource::<Clips>()?.set(&matcher.motion_set)?;
    Some(set.actions.get(action.index)?.name.clone())
}

/// Moves every Motion Matcher on by `delta` seconds and poses its model,
/// and with root motion, moves its Obj as its clips carry it.
pub fn match_motion(world: &mut World, delta: f32) {
    let matchers: Vec<(Entity, MotionMatcher, Arc<ModelAnimation>)> = world
        .query_ref::<(Entity, &MotionMatcher, &ModelRenderer)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, matcher, model)| Some((entity, matcher.clone(), model.animation.clone()?)))
        .collect();
    for (entity, matcher, animation) in matchers {
        let set = world.resource::<Clips>().and_then(|clips| clips.set(&matcher.motion_set)).cloned();
        // A set named but not read yet: nothing to play until it is.
        if !matcher.motion_set.is_empty() && set.is_none() {
            let why = format!("its Motion Set {} has not been read", matcher.motion_set);
            nothing_to_play(world, entity, &why);
            continue;
        }
        let mut state = world.get::<MotionMatcherState>(entity).cloned().unwrap_or_default();
        let wanted = set.as_deref().map_or(0, |set| chosen(set, &matcher, &state));
        let Some(play) = state_play(world, &matcher.motion_set, set.as_deref(), wanted, &animation) else {
            let why = match set.as_deref().and_then(|set| set.states.get(wanted)) {
                Some(made) => {
                    let found =
                        |clip: &String| world.resource::<Clips>().is_some_and(|clips| clips.get(clip).is_some());
                    let missing: Vec<&str> = made
                        .clips
                        .iter()
                        .map(|entry| &entry.clip)
                        .filter(|clip| !found(clip))
                        .map(String::as_str)
                        .collect();
                    format!("its state {} could not be made; clips not found: {missing:?}", made.name)
                }
                None => "its model has no clips to match over".to_string(),
            };
            nothing_to_play(world, entity, &why);
            continue;
        };
        if let Some(unplayable) = world.resource_mut::<Unplayable>() {
            unplayable.0.remove(&entity);
        }
        // Into another state, its enter action plays.
        let taken = world.get_mut::<MotionMatcher>(entity).map(|m| std::mem::take(&mut m.commands));
        let mut commands = taken.unwrap_or_default();
        let entered =
            set.as_deref().and_then(|set| set.states.get(wanted)).map(|s| s.enter.clone()).unwrap_or_default();
        if state.play.is_some() && wanted != state.state && !entered.is_empty() {
            commands.insert(0, ActionCommand::Play(entered));
        }
        state.state = wanted;
        let placed = voxear_scene::world_matrix(world, entity);
        let (asked, moved) = act(world, entity, &mut state, &matcher, set.as_deref(), &play, commands, delta);
        let pose = step(&mut state, &asked, &play, placed, delta);
        if matcher.debug {
            draw(&state, &matcher, &play, placed);
        }
        // An action that stands the character or moves it itself is all that carries it.
        let turned = Quat::from_rotation_y(state.body_yaw.unwrap_or_default());
        let walked = if state.taken { Vec3::ZERO } else { carried(&state, &play) };
        state.travel = walked + turned * moved;
        if (matcher.root_motion || moved != Vec3::ZERO) && delta > 0.0 {
            let walked = if matcher.root_motion { walked } else { Vec3::ZERO };
            carry(world, entity, walked + turned * moved, delta);
        }
        let bones = play.animation.skeleton.model_matrices(&pose).into();
        world.insert(entity, Pose { bones });
        world.insert(entity, state);
    }
}

/// Motion Matchers with nothing to play, and for how many frames.
#[derive(Default)]
struct Unplayable(HashMap<Entity, u32>);

/// Counts a frame a Motion Matcher had nothing to play, telling of it once
/// it has had nothing for a second and a half: its body stands in its rest
/// pose meanwhile, and nothing else would say why.
fn nothing_to_play(world: &mut World, entity: Entity, why: &str) {
    let name = world.get::<Name>(entity).map_or_else(|| format!("{entity:?}"), |name| name.0.clone());
    if world.resource::<Unplayable>().is_none() {
        world.insert_resource(Unplayable::default());
    }
    let Some(unplayable) = world.resource_mut::<Unplayable>() else { return };
    let frames = unplayable.0.entry(entity).or_default();
    *frames += 1;
    if *frames == 90 {
        tracing::warn!("{name}'s Motion Matcher has had nothing to play for 90 frames, so it stands at rest: {why}");
    }
}

/// Plays the actions the game asked for, and the one playing on: fades it,
/// tells scripts of the marks it passed, and asks the matcher to stand or
/// stop turning as it says. Returns the matcher as the state's clips are to
/// play this frame, and how the action carries the character, as seen from
/// the body.
#[allow(clippy::too_many_arguments)]
fn act(
    world: &mut World,
    entity: Entity,
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    set: Option<&MotionSet>,
    play: &StatePlay,
    commands: Vec<ActionCommand>,
    delta: f32,
) -> (MotionMatcher, Vec3) {
    let mut asked = matcher.clone();
    state.taken = false;
    let Some(set) = set else { return (asked, Vec3::ZERO) };
    for command in commands {
        match command {
            ActionCommand::Stop => {
                if let Some(action) = &mut state.action {
                    action.ending = true;
                }
            }
            ActionCommand::Play(name) => {
                let Some(index) = set.action_named(&name) else { continue };
                let action = &set.actions[index];
                let source = world.resource::<Clips>().and_then(|clips| clips.get(&action.clip)).cloned();
                let Some(source) = source else { continue };
                let clip = fitted(world, &source, &play.animation);
                let mask = actions::mask(&play.animation.skeleton, &play.database.spine, action.body, &action.bone);
                let (mask, looping) = (mask.into(), action.looping);
                state.action = Some(ActionPlay { index, clip, mask, time: 0.0, weight: 0.0, ending: false, looping });
            }
        }
    }
    let Some(action) = &mut state.action else { return (asked, Vec3::ZERO) };
    let Some(made) = set.actions.get(action.index) else {
        state.action = None;
        return (asked, Vec3::ZERO);
    };
    let wanting = matcher.wanted_velocity.map_or(state.velocity, flat).length() > matcher.idle_speed;
    let was = action.time;
    let (playing, passed) = action.advance(made, wanting, delta);
    if !passed.is_empty() {
        if world.resource::<MotionMarks>().is_none() {
            world.insert_resource(MotionMarks::default());
        }
        if let Some(marks) = world.resource_mut::<MotionMarks>() {
            let set = &matcher.motion_set;
            marks.0.extend(passed.into_iter().map(|name| (entity, format!("{set}#{MARK_PART}{name}"))));
        }
    }
    if !playing {
        state.action = None;
        return (asked, Vec3::ZERO);
    }
    // Standing or carried by the action, the body holds its facing once Turn Until is past.
    state.taken = made.movement != ActionMovement::Keep && !action.ending;
    if made.movement != ActionMovement::Keep {
        asked.wanted_velocity = Some(Vec3::ZERO);
    }
    if made.movement != ActionMovement::Keep
        && action.time > made.turn_until
        && let Some(body) = state.body_yaw
    {
        asked.wanted_facing = Some(Quat::from_rotation_y(body) * Vec3::Z);
    }
    let skeleton = &play.animation.skeleton;
    let moved = action.carried(made, skeleton, play.database.root, play.database.rest_root, was);
    (asked, moved)
}

/// The state a matcher plays: the one its game asks for, else the one it
/// plays, else the set's first walking one.
fn chosen(set: &MotionSet, matcher: &MotionMatcher, state: &MotionMatcherState) -> usize {
    let asked = match &matcher.state {
        Some(StateChoice::Role(role)) => set.state_for(*role),
        Some(StateChoice::Named(name)) => set.state_named(name),
        None => None,
    };
    let playing = state.play.is_some().then_some(state.state).filter(|&s| s < set.states.len());
    asked.or(playing).or_else(|| set.state_for(StateRole::Grounded)).unwrap_or(0)
}

/// How fast the clips playing carry the body along the ground, in the
/// world, in metres a second.
fn carried(state: &MotionMatcherState, play: &StatePlay) -> Vec3 {
    let walking = 1.0 - state.idle_weight;
    let along: Vec3 = play.database.runs.iter().zip(&state.weights).map(|(run, w)| run.velocity * *w).sum();
    Quat::from_rotation_y(state.body_yaw.unwrap_or_default()) * along * walking * state.rate
}

/// Moves the character along the ground at `velocity` for `delta` seconds:
/// the nearest Obj with a Character Controller at or above the matcher's,
/// so walls and steps hold it, as a player above its model; else the
/// matcher's own, straight.
fn carry(world: &mut World, entity: Entity, velocity: Vec3, delta: f32) {
    let mut at = Some(entity);
    while let Some(obj) = at.filter(|obj| !world.has::<crate::physics::CharacterController>(*obj)) {
        at = world.get::<voxear_scene::Parent>(obj).map(|parent| parent.0);
    }
    if let Some(mover) = at
        && let Some(mut physics) = world.remove_resource::<crate::physics::PhysicsScene>()
    {
        // A little down too, to keep it on the ground.
        physics.move_character(world, mover, velocity + Vec3::NEG_Y * 2.0, delta);
        world.insert_resource(physics);
        return;
    }
    let parent = world.get::<voxear_scene::Parent>(entity).map(|parent| voxear_scene::world_matrix(world, parent.0));
    let moved = parent.map_or(velocity, |parent| parent.inverse().transform_vector3(velocity)) * delta;
    if let Some(transform) = world.get_mut::<Transform>(entity) {
        transform.position += moved;
    }
}

/// Moves `state` on by `delta`, its Obj placed by `placed`: turns the body,
/// searches when it is time, fades the clips' weights and plays them on.
/// Returns the model's pose.
fn step(
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    play: &Arc<StatePlay>,
    placed: Mat4,
    delta: f32,
) -> Vec<BonePose> {
    let (animation, database) = (&*play.animation, &play.database);
    let obj = Ground::new(placed.w_axis.truncate(), facing(placed.transform_vector3(Vec3::Z)));
    // Into another state, its clips start from nothing, the old pose settling into theirs.
    let changed_state = state.play.as_ref().is_some_and(|was| !Arc::ptr_eq(was, play));
    if !state.play.as_ref().is_some_and(|was| Arc::ptr_eq(was, play)) {
        let runs = database.runs.len();
        match state.play.is_some() {
            true => {
                (state.weights, state.targets) = (vec![0.0; runs], vec![0.0; runs]);
                (state.searched, state.stride) = (None, None);
            }
            false => {
                *state = MotionMatcherState {
                    weights: vec![0.0; runs],
                    targets: vec![0.0; runs],
                    idle_weight: 1.0,
                    rate: 1.0,
                    state: state.state,
                    action: state.action.take(),
                    ..MotionMatcherState::default()
                }
            }
        }
        state.play = Some(play.clone());
    }
    if delta > 0.0 {
        if let Some(last) = state.last {
            let seen = flat((obj.position - last) / delta);
            state.velocity = state.velocity.lerp(seen, 1.0 - 0.5f32.powf(delta / SEEN_HALFLIFE));
        }
        state.last = Some(obj.position);
    }
    let wanted = matcher.wanted_velocity.map_or(state.velocity, flat);
    let body = turn(state, matcher, obj, wanted, delta);
    let idle = play.idle;
    let standing = idle.is_some() && wanted.length() <= matcher.idle_speed;
    let stride = state.stride;
    let local = head(state, matcher, database, wanted, body, delta);
    // Changing the way its legs take it, as from forward to back, the hips
    // turn round: straight to the new clips, the old pose settling into them.
    let flipped = !standing && stride.is_some() && state.stride != stride;
    state.since_search += delta;
    let changed = state.searched.is_none_or(|searched| searched.distance(local) >= CHANGED);
    let unset = state.targets.iter().all(|&w| w <= 0.0);
    if !standing && (state.since_search >= matcher.search_every || changed || unset || flipped) {
        aim(state, database, local);
        state.since_search = 0.0;
        state.searched = Some(local);
        if flipped {
            state.weights.copy_from_slice(&state.targets);
        }
    }
    fade(state, matcher, standing && idle.is_some(), delta);
    play_on(state, matcher, database, delta);
    let mut pose = pose(state, matcher, animation, database, idle, body);
    if let Some(action) = &state.action {
        action.lay_over(&mut pose, &animation.skeleton.rest(), database.root);
    }
    turn_root(&mut pose, &animation.skeleton, database.root, wrap(body - obj.yaw));
    // Looking up or down, the spine bends about the body's right.
    let right = Quat::from_rotation_y(wrap(body - obj.yaw)) * Vec3::X;
    let pitch = (matcher.aim_pitch * AIM_SHARE).clamp(-AIM_BEND, AIM_BEND).to_radians();
    twist(&mut pose, &animation.skeleton, &database.spine, right, pitch);
    if (flipped || changed_state) && delta > 0.0 && state.shown.len() == pose.len() {
        let old = [state.shown.as_slice(), &state.before];
        let halflife = if changed_state { play.blend * 0.5 } else { FLIP_HALFLIFE };
        state.settling = Some(Settling::between(old, [&pose, &pose], delta, halflife.max(1e-3)));
    }
    if let Some(settling) = &mut state.settling
        && !settling.settle(delta, &mut pose)
    {
        state.settling = None;
    }
    // Feet are held as the state's clips have them, not where an action's legs take them.
    let ankles = database.legs.map(|legs| [legs[0][2], legs[1][2]]);
    let legs_taken = state.action.as_ref().zip(ankles).map_or(0.0, |(action, ankles)| action.legs_taken(ankles));
    match (matcher.foot_locking && legs_taken < 0.5, database.legs) {
        (true, Some(legs)) => {
            let down = down(state, database, idle);
            let toes = database.toes.unwrap_or([legs[0][2], legs[1][2]]);
            hold_feet(state, &animation.skeleton, legs, toes, down, &mut pose, placed, delta);
        }
        _ => state.locks = [None, None],
    }
    state.before = std::mem::replace(&mut state.shown, pose.clone());
    pose
}

/// Turns the body towards the way it is wanted to face, or the way it
/// goes, at its turn rate; returns the way it faces.
fn turn(state: &mut MotionMatcherState, matcher: &MotionMatcher, obj: Ground, wanted: Vec3, delta: f32) -> f32 {
    let body = state.body_yaw.get_or_insert(obj.yaw);
    let moving = wanted.length() > matcher.idle_speed;
    let target = matcher
        .wanted_facing
        .map(flat)
        .filter(|way| way.length_squared() > 1e-6)
        .map(facing)
        .or_else(|| (matcher.turn_to_movement && moving).then(|| facing(wanted)))
        .or_else(|| (!matcher.turn_to_movement).then_some(obj.yaw));
    if let Some(target) = target {
        let eased = wrap(target - *body) * (1.0 - 0.5f32.powf(delta / TURN_HALFLIFE));
        let most = matcher.turn_rate.to_radians() * delta;
        *body = match matcher.turn_rate > 0.0 {
            true => wrap(*body + eased.clamp(-most, most)),
            false => target,
        };
    }
    *body
}

/// Eases the way it goes towards the way wanted, and picks the way, among
/// its clips', its legs take it there with: the nearest, kept until
/// another is clearly nearer. Returns the velocity to search for, as seen
/// from that clip's root: along the clip's way, but for whatever of the
/// turn is past what the legs may make.
fn head(
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    database: &MotionDatabase,
    wanted: Vec3,
    body: f32,
    delta: f32,
) -> Vec3 {
    if wanted.length() > matcher.idle_speed {
        let way = facing(wanted);
        let heading = state.heading.get_or_insert(way);
        *heading = wrap(*heading + wrap(way - *heading) * (1.0 - 0.5f32.powf(delta / TURN_HALFLIFE)));
    }
    let Some(heading) = state.heading else { return Vec3::ZERO };
    let aside = wrap(heading - body);
    let gap = |way: f32| wrap(aside - way).abs();
    let nearest = database
        .runs
        .iter()
        .filter(|run| run.moves())
        .map(|run| facing(run.velocity))
        .min_by(|a, b| gap(*a).total_cmp(&gap(*b)));
    if let Some(nearest) = nearest {
        state.stride = Some(match state.stride {
            Some(kept) if gap(kept) <= gap(nearest) + STICK => kept,
            _ => nearest,
        });
    }
    let stride = state.stride.unwrap_or(aside);
    let past = wrap(aside - stride) - legs(matcher, aside, stride);
    Quat::from_rotation_y(stride + past) * Vec3::Z * wanted.length()
}

/// How far the legs turn from the body for a clip whose root goes
/// `stride`, seen from it, when the body goes `aside`, seen from the way it
/// faces: as far towards it as they may.
fn legs(matcher: &MotionMatcher, aside: f32, stride: f32) -> f32 {
    let limit = matcher.leg_turn.to_radians();
    wrap(aside - stride).clamp(-limit, limit)
}

/// Sets the weights to fade towards from the clips that best move at
/// `local`, seen from the body: the few best, weighed by how much worse
/// each matched than the best of them, but only those heading the way
/// wanted.
fn aim(state: &mut MotionMatcherState, database: &MotionDatabase, local: Vec3) {
    let found = database.search(&database.query(local));
    let way = local.normalize_or_zero();
    let aligned = |run: usize| {
        let heading = database.runs[run].velocity.normalize_or_zero();
        way == Vec3::ZERO || heading.dot(way) >= ALIGNED
    };
    let candidates: Vec<(usize, f32)> =
        found.iter().take(CANDIDATES).copied().filter(|&(run, _)| aligned(run)).collect();
    // Against the best, not on their own: a state whose clips differ little
    // has every cost large, far from any of its speeds.
    let best = candidates.iter().map(|&(_, cost)| cost).fold(f32::MAX, f32::min);
    state.targets.fill(0.0);
    for (run, cost) in candidates {
        state.targets[run] += (-(cost - best) * SHARPNESS).exp();
    }
    let total: f32 = state.targets.iter().sum();
    match total > 1e-6 {
        true => state.targets.iter_mut().for_each(|w| *w /= total),
        // None heads the way wanted: keep playing what was fading in, or else
        // the clip heading most nearly that way.
        false => {
            if state.weights.iter().any(|&w| w > 0.0) {
                state.targets.copy_from_slice(&state.weights);
            } else if let Some(&(run, _)) = found.iter().max_by(|a, b| {
                let along = |run: usize| database.runs[run].velocity.normalize_or_zero().dot(way);
                along(a.0).total_cmp(&along(b.0))
            }) {
                state.targets[run] = 1.0;
            }
        }
    }
}

/// Fades each weight, and standing's, towards its target.
fn fade(state: &mut MotionMatcherState, matcher: &MotionMatcher, standing: bool, delta: f32) {
    let k = match matcher.blend > 0.0 {
        true => 1.0 - (-delta * 2.0 / matcher.blend).exp(),
        false => 1.0,
    };
    for (weight, target) in state.weights.iter_mut().zip(&state.targets) {
        *weight += (target - *weight) * k;
    }
    let total: f32 = state.weights.iter().sum();
    if total > 1e-6 {
        state.weights.iter_mut().for_each(|w| *w /= total);
    }
    state.idle_weight += (f32::from(u8::from(standing)) - state.idle_weight) * k;
}

/// Moves the clips playing on through their loops, together, as fast as
/// their feet keep pace with the ground the Obj covers.
fn play_on(state: &mut MotionMatcherState, matcher: &MotionMatcher, database: &MotionDatabase, delta: f32) {
    let blended = |pick: fn(&ClipRun) -> f32| -> f32 {
        database.runs.iter().zip(&state.weights).map(|(run, w)| pick(run) * w).sum()
    };
    let (duration, speed) = (blended(ClipRun::duration), blended(|run| run.velocity.length()));
    let seen = state.velocity.length();
    state.rate = match matcher.match_speed && speed > 0.3 && state.idle_weight < 0.5 {
        true => (seen / speed).clamp(PACE.0, PACE.1),
        false => 1.0,
    };
    if duration > 1e-3 {
        state.phase = (state.phase + delta * state.rate / duration).rem_euclid(1.0);
    }
    state.idle_time += delta;
}

/// The model's pose: the clips playing, each at the same share of its
/// loop and with its root kept over the Obj, blended by their weights, and
/// the clip it stands with over them.
fn pose(
    state: &MotionMatcherState,
    matcher: &MotionMatcher,
    animation: &ModelAnimation,
    database: &MotionDatabase,
    idle: Option<usize>,
    body: f32,
) -> Vec<BonePose> {
    let rest = animation.skeleton.rest();
    let sample = |clip: usize, time: f32| {
        let mut pose = rest.clone();
        animation.clips[clip].sample(time, true, &mut pose);
        let root = database.root;
        pose[root] = BonePose {
            translation: Vec3::new(rest[root].translation.x, pose[root].translation.y, rest[root].translation.z),
            rotation: rest[root].rotation,
            ..pose[root]
        };
        pose
    };
    let walking = 1.0 - state.idle_weight;
    let mut poses: Vec<(Vec<BonePose>, f32)> = database
        .runs
        .iter()
        .zip(&state.weights)
        .filter(|(_, w)| **w * walking > 1e-3)
        .map(|(run, w)| {
            let mut pose = sample(run.clip, state.phase * run.duration());
            if let Some(heading) = state.heading {
                let turned = legs(matcher, wrap(heading - body), facing(run.velocity));
                turn_root(&mut pose, &animation.skeleton, database.root, turned);
                twist(&mut pose, &animation.skeleton, &database.spine, Vec3::Y, -turned);
            }
            (pose, w * walking)
        })
        .collect();
    if let Some(idle) = idle.filter(|_| state.idle_weight > 1e-3) {
        poses.push((sample(idle, state.idle_time), state.idle_weight));
    }
    blend(&poses, &rest)
}

/// Poses blended by their weights, each bone's turn taken the short way.
fn blend(poses: &[(Vec<BonePose>, f32)], rest: &[BonePose]) -> Vec<BonePose> {
    let total: f32 = poses.iter().map(|(_, w)| w).sum();
    if total <= 1e-6 {
        return rest.to_vec();
    }
    (0..rest.len())
        .map(|bone| {
            let first = poses[0].0[bone].rotation;
            let mut out =
                BonePose { translation: Vec3::ZERO, rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 0.0), scale: Vec3::ZERO };
            for (pose, weight) in poses {
                let share = weight / total;
                let part = pose[bone];
                let rotation = if part.rotation.dot(first) < 0.0 { -part.rotation } else { part.rotation };
                out.translation += part.translation * share;
                out.scale += part.scale * share;
                out.rotation = out.rotation + rotation * share;
            }
            out.rotation = out.rotation.normalize();
            out
        })
        .collect()
}

/// Turns the model about its root by `turn` about Y, so the body faces
/// its own way on the Obj.
fn turn_root(pose: &mut [BonePose], skeleton: &Skeleton, root: usize, turn: f32) {
    if turn.abs() < 1e-5 {
        return;
    }
    let matrices = skeleton.model_matrices(pose);
    let wanted = Mat4::from_quat(Quat::from_rotation_y(turn)) * matrices[root];
    let parent = skeleton.bones[root].parent.map_or(Mat4::IDENTITY, |parent| matrices[parent]);
    let (scale, rotation, translation) = (parent.inverse() * wanted).to_scale_rotation_translation();
    pose[root] = BonePose { translation, rotation, scale };
}

/// Whether the clips playing have each foot down, by the share of them that
/// do.
fn down(state: &MotionMatcherState, database: &MotionDatabase, idle: Option<usize>) -> [bool; 2] {
    let walking = 1.0 - state.idle_weight;
    let mut share = [0.0f32; 2];
    for (run, weight) in database.runs.iter().zip(&state.weights) {
        let down = database.contacts(run, state.phase * run.duration());
        for foot in 0..2 {
            share[foot] += weight * walking * f32::from(u8::from(down[foot]));
        }
    }
    // Standing on a clip it has no frames of, both feet are down.
    let standing = idle.and_then(|idle| database.runs.iter().find(|run| run.clip == idle));
    let still = standing.map_or([true; 2], |run| database.contacts(run, state.idle_time));
    for foot in 0..2 {
        share[foot] += state.idle_weight * f32::from(u8::from(still[foot]));
    }
    share.map(|share| share >= 0.5)
}

/// Holds each foot where it came down, by its toes, while the clips have
/// it down, and eases it back to theirs once they lift it or the leg would
/// have to reach too far, bending each leg to put its foot there.
#[allow(clippy::too_many_arguments)]
fn hold_feet(
    state: &mut MotionMatcherState,
    skeleton: &Skeleton,
    legs: [[usize; 3]; 2],
    toes: [usize; 2],
    down: [bool; 2],
    pose: &mut [BonePose],
    placed: Mat4,
    delta: f32,
) {
    let from_world = placed.inverse();
    for foot in 0..2 {
        let matrices = skeleton.model_matrices(pose);
        let at = |bone: usize| placed.transform_point3(matrices[bone].w_axis.truncate());
        let [hip, knee, ankle_bone] = legs[foot];
        // The ankle goes where the toes held down need it, the foot turned as the clips turn it.
        let (toe, ankle) = (at(toes[foot]), at(ankle_bone));
        let leg = at(knee).distance(at(hip)) + ankle.distance(at(knee));
        let came_down = down[foot] && !state.down[foot];
        let lock = &mut state.locks[foot];
        if let Some(held) = lock
            && held.held
            && (!down[foot] || held.at.distance(toe) > REACH || (held.at + ankle - toe).distance(at(hip)) > leg * 0.98)
        {
            held.held = false;
        }
        if came_down || (down[foot] && lock.is_none()) {
            // Down where it shows now, so nothing jumps.
            let shown = lock.map_or(toe, |held| toe.lerp(held.at, smooth(held.weight)));
            *lock = Some(FootLock { at: shown, weight: 1.0, held: true });
        }
        if let Some(held) = lock {
            if !held.held {
                held.weight -= delta / LET_GO;
            }
            match held.weight > 0.0 {
                true => {
                    let target = toe.lerp(held.at, smooth(held.weight)) + (ankle - toe);
                    reach(pose, skeleton, legs[foot], from_world.transform_point3(target));
                }
                false => *lock = None,
            }
        }
    }
    state.down = down;
}

/// Bends a leg, its hip, knee and ankle, so the ankle reaches `target` in
/// the model's space, the knee bending in the plane it bends in already;
/// the foot keeps the turn it had.
fn reach(pose: &mut [BonePose], skeleton: &Skeleton, [hip, knee, ankle]: [usize; 3], target: Vec3) {
    let matrices = skeleton.model_matrices(pose);
    let at = |bone: usize| matrices[bone].w_axis.truncate();
    let turn_of = |m: &Mat4| m.to_scale_rotation_translation().1;
    let (a, b, c) = (at(hip), at(knee), at(ankle));
    let (hip_turn, knee_turn, foot_turn) =
        (turn_of(&matrices[hip]), turn_of(&matrices[knee]), turn_of(&matrices[ankle]));
    let (upper, lower) = ((b - a).length(), (c - b).length());
    if upper < 1e-4 || lower < 1e-4 {
        return;
    }
    let span = (target - a).length().clamp(1e-3, upper + lower - 1e-3);
    let angle = |x: Vec3, y: Vec3| x.normalize_or_zero().dot(y.normalize_or_zero()).clamp(-1.0, 1.0).acos();
    let law = |near: f32, far: f32, opposite: f32| {
        ((opposite * opposite - near * near - far * far) / (-2.0 * near * far)).clamp(-1.0, 1.0).acos()
    };
    let (hip_now, knee_now, aim_now) = (angle(c - a, b - a), angle(a - b, c - b), angle(c - a, target - a));
    let (hip_then, knee_then) = (law(upper, span, lower), law(upper, lower, span));
    let bend = (c - a).cross(b - a).try_normalize().unwrap_or_else(|| hip_turn * Vec3::X);
    let swing = (c - a).cross(target - a).try_normalize();
    let open_hip = Quat::from_axis_angle(hip_turn.inverse() * bend, hip_then - hip_now);
    let open_knee = Quat::from_axis_angle(knee_turn.inverse() * bend, knee_then - knee_now);
    let aim = swing.map_or(Quat::IDENTITY, |swing| Quat::from_axis_angle(hip_turn.inverse() * swing, aim_now));
    pose[hip].rotation = (pose[hip].rotation * aim * open_hip).normalize();
    pose[knee].rotation = (pose[knee].rotation * open_knee).normalize();
    let knee_after = turn_of(&skeleton.model_matrices(pose)[knee]);
    pose[ankle].rotation = (knee_after.inverse() * foot_turn).normalize();
}

/// Turns `bones`, a run of the spine from the hips up, about `axis` in the
/// model's space by `turn` between them, each its share.
fn twist(pose: &mut [BonePose], skeleton: &Skeleton, bones: &[usize], axis: Vec3, turn: f32) {
    if bones.is_empty() || turn.abs() < 1e-5 {
        return;
    }
    let share = Quat::from_axis_angle(axis, turn / bones.len() as f32);
    for &bone in bones {
        let matrices = skeleton.model_matrices(pose);
        let parent =
            skeleton.bones[bone].parent.map_or(Quat::IDENTITY, |p| matrices[p].to_scale_rotation_translation().1);
        pose[bone].rotation = (parent.inverse() * share * parent * pose[bone].rotation).normalize();
    }
}

/// Draws the velocity wanted, the clips' velocity and the body's facing,
/// and names the clips playing.
fn draw(state: &MotionMatcherState, matcher: &MotionMatcher, play: &StatePlay, placed: Mat4) {
    use voxear_core::debug_draw::{arrow, text};
    let Some(body) = state.body_yaw else { return };
    let at = placed.w_axis.truncate() + Vec3::Y * 0.05;
    let wanted = matcher.wanted_velocity.map_or(state.velocity, flat);
    let clips = carried(state, play) / state.rate.max(1e-3);
    arrow(at, at + wanted * 0.5, [0.3, 0.8, 1.0, 1.0], 0.0, true);
    arrow(at, at + clips * 0.5, [1.0, 0.6, 0.2, 1.0], 0.0, true);
    arrow(at, at + Quat::from_rotation_y(body) * Vec3::Z * 0.6, [1.0, 1.0, 1.0, 1.0], 0.0, true);
    let names: Vec<String> =
        state.playing().iter().take(3).map(|(name, w)| format!("{name} {:.0}%", w * 100.0)).collect();
    text(at + Vec3::Y * 2.1, &format!("{}  ×{:.2}", names.join("  "), state.rate), [1.0, 1.0, 1.0, 1.0], 0.0);
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::{Bone, CLIP_RATE, Clip};

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn a_script_reads_which_feet_the_clips_have_down_once_the_matcher_has_played() {
        let mut world = World::new();
        let matcher = world.spawn((MotionMatcher::default(),));
        let bare = world.spawn(());
        assert_eq!(values(&mut world, bare, FEET_DOWN, None), None, "no matcher, no feet");
        assert_eq!(values(&mut world, matcher, FEET_DOWN, None), Some([0.0; 4]), "nothing played yet");
        world.insert(matcher, MotionMatcherState { down: [true, false], ..MotionMatcherState::default() });
        assert_eq!(values(&mut world, matcher, FEET_DOWN, None), Some([1.0, 0.0, 0.0, 1.0]), "the left foot down");
    }

    /// One loop a second long of a root, hips and two feet: the root moving
    /// A frame's bones: the root at `root`, the hips over it, each leg's hip,
    /// knee and ankle, the knees a little bent and the ankles at `feet` from
    /// the hips, and the spine.
    fn frame(root: Vec3, feet: [Vec3; 2]) -> [BonePose; 9] {
        let at = |translation| BonePose { translation, ..BonePose::default() };
        let leg = |side: f32, foot: Vec3| {
            let hip = Vec3::new(0.1 * side, -0.05, 0.0);
            let knee = Vec3::new(0.0, -0.4, 0.1);
            [at(hip), at(knee), at(foot - hip - knee)]
        };
        let [l, r] = [leg(1.0, feet[0]), leg(-1.0, feet[1])];
        [at(root), at(Vec3::new(0.0, 0.8, 0.0)), l[0], l[1], l[2], r[0], r[1], r[2], at(Vec3::new(0.0, 0.3, 0.0))]
    }

    /// One loop a second long: the root moving at `velocity`, the feet
    /// swinging along it.
    fn clip(name: &str, velocity: Vec3) -> Clip {
        let frames = 61;
        let swing = velocity.normalize_or_zero() * 0.1 * velocity.length().sqrt();
        let poses = (0..frames)
            .flat_map(|f| {
                let t = f as f32 / CLIP_RATE;
                let stride = swing * (std::f32::consts::TAU * t).sin();
                frame(velocity * t, [Vec3::new(0.1, -0.75, 0.0) + stride, Vec3::new(-0.1, -0.75, 0.0) - stride])
            })
            .collect();
        Clip { name: name.into(), frames, poses }
    }

    /// A loop that stays in place while its planted foot slides back at
    /// `speed`: each foot down for half the loop, then lifted and swung
    /// forward.
    fn in_place(name: &str, speed: f32) -> Clip {
        let frames = 61;
        let reach = speed * 0.25;
        let foot = |t: f32, side: f32| {
            let t = (t + if side > 0.0 { 0.0 } else { 0.5 }).rem_euclid(1.0);
            let (z, y) = match t < 0.5 {
                true => (reach - speed * t, 0.0),
                false => (-reach + speed * (t - 0.5), 0.1),
            };
            Vec3::new(0.1 * side, y - 0.8, z)
        };
        let poses = (0..frames)
            .flat_map(|f| {
                let t = f as f32 / CLIP_RATE;
                frame(Vec3::ZERO, [foot(t, 1.0), foot(t, -1.0)])
            })
            .collect();
        Clip { name: name.into(), frames, poses }
    }

    fn walker(clips: Vec<Clip>) -> ModelAnimation {
        let rest = BonePose::default();
        let bone = |name: &str, parent| Bone { name: name.into(), parent, rest };
        let skeleton = Skeleton {
            bones: vec![
                bone("root", None),
                bone("hips", Some(0)),
                bone("thigh.L", Some(1)),
                bone("shin.L", Some(2)),
                bone("foot.L", Some(3)),
                bone("thigh.R", Some(1)),
                bone("shin.R", Some(5)),
                bone("foot.R", Some(6)),
                bone("spine", Some(1)),
            ],
        };
        ModelAnimation { skeleton, skins: Vec::new(), clips }
    }

    fn moves() -> ModelAnimation {
        walker(vec![
            clip("Stand", Vec3::ZERO),
            clip("Walk", Vec3::Z * 1.4),
            clip("Jog", Vec3::Z * 3.5),
            clip("WalkBack", Vec3::NEG_Z * 1.2),
            clip("Step", Vec3::X * 1.2),
        ])
    }

    /// Moves the Obj at `velocity` for `seconds`, wanting it, and returns
    /// the clip playing most.
    fn drive(
        state: &mut MotionMatcherState,
        matcher: &MotionMatcher,
        play: &Arc<StatePlay>,
        seconds: f32,
        velocity: Vec3,
    ) -> (String, f32) {
        let mut asked = matcher.clone();
        asked.wanted_velocity = Some(velocity);
        let mut at = state.last.unwrap_or_default();
        for _ in 0..(seconds / DT).round() as usize {
            at += velocity * DT;
            step(state, &asked, play, Mat4::from_translation(at), DT);
        }
        state.playing()[0].clone()
    }

    #[test]
    fn clips_move_as_their_roots_or_planted_feet_carry_them() {
        let animation =
            walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4), in_place("Treadmill", 1.4)]);
        let database = MotionDatabase::build(&animation, &[]).unwrap();
        let [stand, walk, treadmill] = [0, 1, 2].map(|run| database.runs[run]);
        assert!(stand.stands() && walk.moves(), "{stand:?} {walk:?}");
        assert!(walk.velocity.distance(Vec3::Z * 1.4) < 0.01, "{:?}", walk.velocity);
        assert!(treadmill.moves(), "{treadmill:?}");
        assert!(
            treadmill.velocity.distance(Vec3::Z * 1.4) < 0.1,
            "forward as fast as its feet slide back: {:?}",
            treadmill.velocity
        );
    }

    #[test]
    fn a_clip_moves_and_loops_as_its_settings_say() {
        use voxear_assets::{ClipImport, ClipLoop, ClipMotion};
        let animation = walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4)]);
        let right = ClipImport { motion: ClipMotion::Set, direction: 90.0, speed: 2.0, looping: ClipLoop::Auto };
        let once = ClipImport { looping: ClipLoop::Off, ..ClipImport::default() };
        let database = MotionDatabase::build(&animation, &[right, once]).unwrap();
        let [stand, walk] = [0, 1].map(|run| database.runs[run]);
        assert!(stand.velocity.distance(Vec3::X * 2.0) < 1e-4, "set to step right: {:?}", stand.velocity);
        assert!(stand.moves(), "and so a loop it walks with");
        assert!(!walk.looping, "set not to loop");
    }

    #[test]
    fn the_speed_wanted_picks_its_clips() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let (name, weight) = drive(&mut state, &matcher, &play, 1.0, Vec3::ZERO);
        assert_eq!((name.as_str(), weight > 0.95), ("Stand", true));
        let (name, weight) = drive(&mut state, &matcher, &play, 1.0, Vec3::Z * 1.4);
        assert_eq!((name.as_str(), weight > 0.9), ("Walk", true), "{weight}");
        let (name, _) = drive(&mut state, &matcher, &play, 1.0, Vec3::Z * 3.5);
        assert_eq!(name, "Jog");
        // Stepping back while facing ahead.
        let mut aiming = matcher.clone();
        aiming.wanted_facing = Some(Vec3::Z);
        let (name, _) = drive(&mut state, &aiming, &play, 1.0, Vec3::NEG_Z * 1.2);
        assert_eq!(name, "WalkBack");
    }

    #[test]
    fn the_body_turns_to_its_way_at_its_rate() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let mut asked = matcher.clone();
        asked.wanted_velocity = Some(Vec3::X * 1.4);
        let most = matcher.turn_rate.to_radians() * DT;
        let mut before = 0.0;
        for _ in 0..90 {
            step(&mut state, &asked, &play, Mat4::IDENTITY, DT);
            let yaw = state.body_yaw().unwrap();
            assert!(wrap(yaw - before).abs() <= most + 1e-4, "{before} to {yaw}");
            before = yaw;
        }
        assert!((before - std::f32::consts::FRAC_PI_2).abs() < 1e-2, "facing the way it goes: {before}");
        assert_eq!(state.playing()[0].0, "Walk", "walking forward that way");
    }

    #[test]
    fn starting_stopping_and_changing_pace_never_pop() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let mut matcher = MotionMatcher::default();
        matcher.wanted_facing = Some(Vec3::Z);
        let mut state = MotionMatcherState::default();
        let mut at = Vec3::ZERO;
        let mut seen: Option<Vec<Vec3>> = None;
        for f in 0..360 {
            let t = f as f32 * DT;
            let velocity = match t {
                t if t < 1.0 => Vec3::ZERO,
                t if t < 2.0 => Vec3::Z * 1.4,
                t if t < 3.0 => Vec3::Z * 3.5,
                t if t < 4.0 => Vec3::ZERO,
                t if t < 5.0 => Vec3::NEG_Z * 1.2,
                _ => Vec3::ZERO,
            };
            matcher.wanted_velocity = Some(velocity);
            at += velocity * DT;
            let placed = Mat4::from_translation(at);
            let pose = step(&mut state, &matcher, &play, placed, DT);
            let bones: Vec<Vec3> = animation
                .skeleton
                .model_matrices(&pose)
                .iter()
                .map(|m| placed.transform_point3(m.w_axis.truncate()))
                .collect();
            assert!(bones[0].distance(at) < 1e-4, "the root stays on the Obj at {t:.2}s");
            if let Some(before) = &seen {
                for (bone, (now, then)) in bones.iter().zip(before).enumerate() {
                    let jump = now.distance(*then);
                    assert!(jump < 0.1, "bone {bone} jumped {jump} at {t:.2}s");
                }
            }
            seen = Some(bones);
        }
    }

    /// The yaw a bone faces in the model, after a step.
    fn yaw_of(animation: &ModelAnimation, pose: &[BonePose], bone: usize) -> f32 {
        let rotation = animation.skeleton.model_matrices(pose)[bone].to_scale_rotation_translation().1;
        facing(rotation * Vec3::Z)
    }

    #[test]
    fn going_aside_turns_the_legs_and_keeps_the_chest_ahead() {
        let animation =
            walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4), clip("WalkBack", Vec3::NEG_Z * 1.2)]);
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let mut matcher = MotionMatcher::default();
        matcher.wanted_facing = Some(Vec3::Z);
        let quarter = std::f32::consts::FRAC_PI_4;
        for (way, clip, legs) in [(quarter, "Walk", quarter), (3.0 * quarter, "WalkBack", -quarter)] {
            let mut state = MotionMatcherState::default();
            let velocity = Quat::from_rotation_y(way) * Vec3::Z * 1.3;
            let mut asked = matcher.clone();
            asked.wanted_velocity = Some(velocity);
            let mut pose = Vec::new();
            for f in 0..90 {
                pose = step(&mut state, &asked, &play, Mat4::from_translation(velocity * f as f32 * DT), DT);
            }
            let playing = state.playing()[0].0.clone();
            assert_eq!(playing, clip);
            let (hips, chest) = (yaw_of(&animation, &pose, 1), yaw_of(&animation, &pose, 8));
            assert!((hips - legs).abs() < 0.02, "the hips turned towards the way it goes: {hips} for {legs}");
            assert!(chest.abs() < 0.02, "the chest still ahead: {chest}");
        }
    }

    #[test]
    fn turning_round_never_plays_a_clip_heading_away() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let mut at = Vec3::ZERO;
        for f in 0..240 {
            let velocity = match f / 60 {
                0 => Vec3::Z,
                1 => Vec3::NEG_X,
                2 => Vec3::NEG_Z,
                _ => Vec3::X,
            } * 1.4;
            let mut asked = matcher.clone();
            asked.wanted_velocity = Some(velocity);
            at += velocity * DT;
            step(&mut state, &asked, &play, Mat4::from_translation(at), DT);
            for (name, weight) in state.playing() {
                assert!(name != "WalkBack" || weight < 0.05, "{name} at {weight} on frame {f}");
            }
        }
    }

    #[test]
    fn a_leg_bends_to_put_its_ankle_where_asked() {
        let animation = walker(Vec::new());
        let skeleton = &animation.skeleton;
        let mut pose = frame(Vec3::ZERO, [Vec3::new(0.1, -0.7, 0.1), Vec3::new(-0.1, -0.75, 0.0)]).to_vec();
        let before = skeleton.model_matrices(&pose)[4].to_scale_rotation_translation().1;
        let target = Vec3::new(0.15, 0.2, 0.25);
        reach(&mut pose, skeleton, [2, 3, 4], target);
        let after = skeleton.model_matrices(&pose);
        assert!(after[4].w_axis.truncate().distance(target) < 1e-3, "{}", after[4].w_axis);
        assert!(after[4].to_scale_rotation_translation().1.angle_between(before) < 1e-3, "the foot keeps its turn");
        let lengths = |m: &[Mat4]| (m[3].w_axis.distance(m[2].w_axis), m[4].w_axis.distance(m[3].w_axis));
        let was = lengths(&skeleton.model_matrices(&frame(Vec3::ZERO, [Vec3::new(0.1, -0.7, 0.1), Vec3::ZERO])));
        let now = lengths(&after);
        assert!((was.0 - now.0).abs() < 1e-4 && (was.1 - now.1).abs() < 1e-4, "bones keep their lengths");
    }

    #[test]
    fn a_foot_down_stays_put_though_the_obj_outpaces_the_clips() {
        let animation = walker(vec![clip("Stand", Vec3::ZERO), in_place("Treadmill", 1.4)]);
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let slide = |locking: bool| {
            let mut matcher = MotionMatcher::default();
            matcher.foot_locking = locking;
            matcher.wanted_velocity = Some(Vec3::Z * 2.2);
            let mut state = MotionMatcherState::default();
            let (mut at, mut before, mut slides) = (Vec3::ZERO, None::<[Vec3; 2]>, Vec::new());
            for f in 0..180 {
                at += Vec3::Z * 2.2 * DT;
                let placed = Mat4::from_translation(at);
                let pose = step(&mut state, &matcher, &play, placed, DT);
                let m = animation.skeleton.model_matrices(&pose);
                let ankles = [4, 7].map(|bone| placed.transform_point3(m[bone].w_axis.truncate()));
                if let (Some(then), true) = (before, f > 60) {
                    for foot in 0..2 {
                        if ankles[foot].y < 0.03 && then[foot].y < 0.03 {
                            slides.push(flat(ankles[foot] - then[foot]).length() / DT);
                        }
                    }
                }
                before = Some(ankles);
            }
            slides.sort_by(f32::total_cmp);
            slides[slides.len() / 2]
        };
        let (held, loose) = (slide(true), slide(false));
        assert!(loose > 0.2, "without holding, the feet slide: {loose}");
        assert!(held < 0.02, "held, they stay put: {held}");
    }

    #[test]
    fn clips_keep_pace_with_the_ground() {
        let animation = walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4)]);
        let database = Arc::new(MotionDatabase::build(&animation, &[]).unwrap());
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        drive(&mut state, &matcher, &play, 1.5, Vec3::Z * 1.6);
        assert!((state.rate() - 1.6 / 1.4).abs() < 0.02, "a little faster: {}", state.rate());
        let mut steady = matcher.clone();
        steady.match_speed = false;
        drive(&mut state, &steady, &play, 0.1, Vec3::Z * 1.6);
        assert_eq!(state.rate(), 1.0);
    }

    /// Drives the model at `VOXEAR_TEST_MODEL`, when set, through a walk, a
    /// run, a stop, a step back and a turn, reporting what it plays: for
    /// checking real clips by hand.
    #[test]
    fn a_model_named_by_the_environment_matches_its_motion() {
        let Ok(path) = std::env::var("VOXEAR_TEST_MODEL") else { return };
        let model = voxear_render::prepare_model(&path, &mut |file| Ok(std::fs::read(file)?)).unwrap();
        let animation = model.animation().cloned().expect("an animated model");
        let database = Arc::new(MotionDatabase::build(&animation, &[]).expect("a root"));
        let play = Arc::new(StatePlay::own(Arc::new(animation.clone()), database.clone()));
        for run in &database.runs {
            let name = &animation.clips[run.clip].name;
            eprintln!("  {name} {:.2}s {:?} loops {}", run.duration(), run.velocity, run.looping);
        }
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        for (seconds, velocity) in
            [(1.0, Vec3::ZERO), (2.0, Vec3::Z * 1.4), (2.0, Vec3::Z * 5.0), (1.0, Vec3::ZERO), (2.0, Vec3::X * 3.0)]
        {
            drive(&mut state, &matcher, &play, seconds, velocity);
            let playing: Vec<String> =
                state.playing().iter().map(|(name, w)| format!("{name} {:.0}%", w * 100.0)).collect();
            eprintln!("{velocity}: {} ×{:.2}", playing.join(", "), state.rate());
        }
        // As an aiming player would, facing ahead: forward, forward-right, right, back-right, back, left.
        let mut player = MotionMatcherState::default();
        let (mut at, mut shown) = (Vec3::ZERO, String::new());
        let mut aiming = matcher.clone();
        aiming.wanted_facing = Some(Vec3::Z);
        let mut worst = (0.0f32, 0.0f32);
        let mut before: Option<Vec<Vec3>> = None;
        for f in 0..420 {
            let t = f as f32 * DT;
            let way = [Some(0.0f32), Some(45.0), Some(90.0), Some(135.0), Some(180.0), Some(-90.0), None][f / 60];
            let velocity = way.map_or(Vec3::ZERO, |deg| Quat::from_rotation_y(deg.to_radians()) * Vec3::Z * 5.0);
            aiming.wanted_velocity = Some(velocity);
            at += velocity * DT;
            let placed = Mat4::from_translation(at);
            let pose = step(&mut player, &aiming, &play, placed, DT);
            let bones: Vec<Vec3> = animation
                .skeleton
                .model_matrices(&pose)
                .iter()
                .map(|m| placed.transform_point3(m.w_axis.truncate()))
                .collect();
            if let Some(then) = &before {
                let jump = bones.iter().zip(then).map(|(a, b)| a.distance(*b)).fold(0.0f32, f32::max);
                if jump > worst.0 {
                    worst = (jump, t);
                }
            }
            before = Some(bones);
            let playing: Vec<String> = player
                .playing()
                .iter()
                .filter(|(_, w)| *w > 0.05)
                .map(|(name, w)| format!("{name} {:.0}%", w * 100.0))
                .collect();
            let now = playing.iter().map(|p| p.split(' ').next().unwrap_or_default()).collect::<Vec<_>>().join("+");
            if now != shown {
                let heading = player.heading.unwrap_or_default().to_degrees();
                eprintln!("{t:.2}s going {heading:.0}° {}", playing.join(", "));
                shown = now;
            }
        }
        eprintln!("bones moved at most {:.3} m in a frame, at {:.2}s", worst.0, worst.1);
        // How fast a foot on the ground slides over it, at a steady walk and run.
        let toes: Vec<usize> =
            ["DEF-toe.L", "DEF-toe.R"].iter().filter_map(|name| animation.skeleton.find(name)).collect();
        for speed in [1.4, 3.0, 5.0] {
            let mut asked = matcher.clone();
            asked.wanted_velocity = Some(Vec3::Z * speed);
            let (mut at, mut before, mut slides) = (Vec3::ZERO, None::<Vec<Vec3>>, Vec::new());
            for f in 0..240 {
                at += Vec3::Z * speed * DT;
                let placed = Mat4::from_translation(at);
                let pose = step(&mut state, &asked, &play, placed, DT);
                let matrices = animation.skeleton.model_matrices(&pose);
                let feet: Vec<Vec3> =
                    toes.iter().map(|&t| placed.transform_point3(matrices[t].w_axis.truncate())).collect();
                if let (Some(then), true) = (&before, f > 60) {
                    let low = if feet[0].y < feet[1].y { 0 } else { 1 };
                    if feet[low].y < 0.05 {
                        slides.push(flat(feet[low] - then[low]).length() / DT);
                    }
                }
                before = Some(feet);
            }
            slides.sort_by(f32::total_cmp);
            let middle = slides.get(slides.len() / 2).copied().unwrap_or_default();
            eprintln!("at {speed} m/s a planted toe slides {middle:.2} m/s (middling of {})", slides.len());
        }
    }
}
