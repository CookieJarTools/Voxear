//! Motion matching: rather than clips chosen by hand, a Motion Matcher
//! plays whichever frame of its model's clips best carries on from the pose
//! it shows into the path the character is about to take, searching again
//! every tenth of a second and settling into each new frame without a pop.
//!
//! The game moves the Obj, by a Character Controller or otherwise; the
//! matcher follows. The body it shows moves as its clips carry it, pulled
//! towards the Obj and never further from it than `max_drift`, nor turned
//! from it more than `max_turn`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, CLIP_RATE, ModelAnimation, ModelRenderer, Pose};
use voxear_scene::Destroying;

use super::{
    motion_database::{AHEAD, FEATURES, Ground, MotionDatabase, MotionWeights, database_for, facing, wrap},
    settling::Settling,
};

/// How quickly the Obj's seen velocity follows its movement, in seconds to
/// halve the gap: enough to smooth a frame's jitter.
const SEEN_HALFLIFE: f32 = 0.05;
/// How quickly the body is pulled to the Obj, in seconds to halve the gap.
const PULL_HALFLIFE: f32 = 0.1;
const TURN_HALFLIFE: f32 = 0.2;
/// How near, in frames, a found frame may be to the one playing and still
/// count as it.
const NEAR: usize = 3;

/// Animates its Obj's model by motion matching over the model's clips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionMatcher {
    /// The velocity the game wants, in metres a second; none expects the
    /// Obj to carry on as it moves.
    #[serde(skip)]
    pub wanted_velocity: Option<Vec3>,
    /// The way the game wants the character to face; none, the Obj's
    /// forward.
    #[serde(skip)]
    pub wanted_facing: Option<Vec3>,
    /// How quickly the character is expected to reach what is wanted, in
    /// seconds to halve the gap.
    pub response: f32,
    /// Seconds between searches.
    pub search_every: f32,
    /// How long, in seconds, a jump to a new frame takes to settle.
    pub blend: f32,
    /// How far, in metres, the body may stray from the Obj.
    pub max_drift: f32,
    /// How far, in degrees, the body may turn from the Obj's facing.
    pub max_turn: f32,
    /// How much each part of a frame counts in a search: where the feet
    /// are, how they move, how the hips move, and the path ahead.
    pub foot_position: f32,
    pub foot_velocity: f32,
    pub hip_velocity: f32,
    pub path_position: f32,
    pub path_facing: f32,
    /// Drawing the path wanted, the path of the frame playing, and the
    /// body's place.
    pub debug: bool,
}

impl Component for MotionMatcher {}

impl MotionMatcher {
    pub fn weights(&self) -> MotionWeights {
        MotionWeights {
            foot_position: self.foot_position,
            foot_velocity: self.foot_velocity,
            hip_velocity: self.hip_velocity,
            path_position: self.path_position,
            path_facing: self.path_facing,
        }
    }
}

impl Default for MotionMatcher {
    fn default() -> Self {
        let weights = MotionWeights::default();
        Self {
            wanted_velocity: None,
            wanted_facing: None,
            response: 0.25,
            search_every: 0.1,
            blend: 0.2,
            max_drift: 0.3,
            max_turn: 45.0,
            foot_position: weights.foot_position,
            foot_velocity: weights.foot_velocity,
            hip_velocity: weights.hip_velocity,
            path_position: weights.path_position,
            path_facing: weights.path_facing,
            debug: false,
        }
    }
}

/// What a Motion Matcher is doing, kept by the engine.
#[derive(Clone, Default)]
pub struct MotionMatcherState {
    database: Option<Arc<MotionDatabase>>,
    /// The run of the database playing, and how far in, in seconds.
    playing: Option<(usize, f32)>,
    since_search: f32,
    shown: Vec<BonePose>,
    before: Vec<BonePose>,
    settling: Option<Settling>,
    /// Where the body stands in the world.
    body: Option<Ground>,
    /// The Obj's place last frame, and its velocity as seen.
    last: Option<Vec3>,
    velocity: Vec3,
}

impl Component for MotionMatcherState {}

impl MotionMatcherState {
    /// The model's clip playing, and how far in, in seconds.
    pub fn playing(&self) -> Option<(usize, f32)> {
        let database = self.database.as_ref()?;
        self.playing.map(|(run, time)| (database.runs[run].clip, time))
    }

    /// How far the body stands from where the Obj is, along the ground.
    pub fn drift(&self, obj: Vec3) -> f32 {
        self.body.map_or(0.0, |body| body.position.distance(flat(obj)))
    }
}

/// Sets what answers `MotionMatcher` in scripts.
pub fn register() {
    voxear_scripting::set_motion_matcher_values(values);
}

// What `values` reads or writes, as `NativeMotionMatcher` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const WANTED_VELOCITY: i32 = 3;
const WANTED_FACING: i32 = 4;
const FOLLOW: i32 = 5;
const RESPONSE: i32 = 6;
const SEARCH_EVERY: i32 = 7;
const BLEND: i32 = 8;
const MAX_DRIFT: i32 = 9;
const MAX_TURN: i32 = 10;
const DEBUG: i32 = 11;
const DRIFT: i32 = 12;

/// Reads value `what` of `entity`'s Motion Matcher, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let vector = |v: Option<Vec3>| v.map_or([0.0; 4], |v| [v.x, v.y, v.z, 1.0]);
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<MotionMatcher>(entity))))),
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
        (DRIFT, None) => {
            world.get::<MotionMatcher>(entity)?;
            let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
            return Some(number(world.get::<MotionMatcherState>(entity).map_or(0.0, |state| state.drift(at))));
        }
        _ => {}
    }
    let matcher = world.get_mut::<MotionMatcher>(entity)?;
    match written {
        None => Some(match what {
            WANTED_VELOCITY => vector(matcher.wanted_velocity),
            WANTED_FACING => vector(matcher.wanted_facing),
            RESPONSE => number(matcher.response),
            SEARCH_EVERY => number(matcher.search_every),
            BLEND => number(matcher.blend),
            MAX_DRIFT => number(matcher.max_drift),
            MAX_TURN => number(matcher.max_turn),
            DEBUG => number(f32::from(u8::from(matcher.debug))),
            _ => return None,
        }),
        Some([x, y, z, w]) => {
            match what {
                WANTED_VELOCITY => matcher.wanted_velocity = Some(Vec3::new(x, y, z)),
                WANTED_FACING => matcher.wanted_facing = Some(Vec3::new(x, y, z)),
                FOLLOW => (matcher.wanted_velocity, matcher.wanted_facing) = (None, None),
                RESPONSE => matcher.response = x.clamp(0.01, 2.0),
                SEARCH_EVERY => matcher.search_every = x.clamp(0.02, 1.0),
                BLEND => matcher.blend = x.clamp(0.0, 2.0),
                MAX_DRIFT => matcher.max_drift = x.clamp(0.0, 2.0),
                MAX_TURN => matcher.max_turn = x.clamp(0.0, 180.0),
                DEBUG => matcher.debug = x != 0.0,
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

/// Moves every Motion Matcher on by `delta` seconds and poses its model.
pub fn match_motion(world: &mut World, delta: f32) {
    let matchers: Vec<(Entity, MotionMatcher, Arc<ModelAnimation>)> = world
        .query_ref::<(Entity, &MotionMatcher, &ModelRenderer)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, matcher, model)| Some((entity, matcher.clone(), model.animation.clone()?)))
        .collect();
    for (entity, matcher, animation) in matchers {
        let Some(database) = database_for(world, &animation, &matcher.weights()) else { continue };
        let placed = voxear_scene::world_matrix(world, entity);
        let mut state = world.get::<MotionMatcherState>(entity).cloned().unwrap_or_default();
        step(&mut state, &matcher, &animation, &database, placed, delta);
        if matcher.debug {
            draw(&state, &matcher, &database, placed);
        }
        let bones = animation.skeleton.model_matrices(&state.shown).into();
        world.insert(entity, Pose { bones });
        world.insert(entity, state);
    }
}

/// Moves `state` on by `delta`, its Obj placed by `placed`: follows the
/// Obj, searches when it is time, plays on and poses the model in
/// `state.shown`.
fn step(
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    animation: &ModelAnimation,
    database: &Arc<MotionDatabase>,
    placed: Mat4,
    delta: f32,
) {
    let obj = Ground::new(placed.w_axis.truncate(), facing(placed.transform_vector3(Vec3::Z)));
    let rest = animation.skeleton.rest();
    if state.shown.len() != rest.len() {
        state.shown = rest.clone();
        state.before = rest.clone();
    }
    if !state.database.as_ref().is_some_and(|d| Arc::ptr_eq(d, database)) {
        state.database = Some(database.clone());
        state.playing = None;
        state.settling = None;
    }
    if delta > 0.0 {
        if let Some(last) = state.last {
            let seen = (obj.position - last) / delta;
            state.velocity = state.velocity.lerp(seen, 1.0 - 0.5f32.powf(delta / SEEN_HALFLIFE));
        }
        state.last = Some(obj.position);
    }
    let body = *state.body.get_or_insert(obj);
    state.since_search += delta;
    let ended = state.playing.is_some_and(|(run, time)| {
        let run = database.runs[run];
        !run.looping && time >= run.duration()
    });
    if state.playing.is_none() || state.since_search >= matcher.search_every || ended {
        state.since_search = 0.0;
        let path = predict(state, matcher, obj);
        search(state, matcher, animation, database, body, &path, delta);
    }
    // Plays on, the body carried as the clip's root travels.
    if let Some((index, time)) = state.playing {
        let run = database.runs[index];
        let mut next = time + delta;
        if !run.looping {
            next = next.min(run.duration());
        }
        let travel = database.travel(&run, time, next);
        if run.looping && run.duration() > 0.0 {
            next = next.rem_euclid(run.duration());
        }
        state.body = Some(body.then(travel));
        state.playing = Some((index, next));
    }
    pull(state, matcher, obj, delta);
    let mut pose = rest;
    if let (Some((index, time)), Some(body)) = (state.playing, state.body) {
        let run = database.runs[index];
        animation.clips[run.clip].sample(time, run.looping, &mut pose);
        place_root(&mut pose, animation, database, body, placed);
    }
    if let Some(settling) = &mut state.settling
        && !settling.settle(delta, &mut pose)
    {
        state.settling = None;
    }
    state.before = std::mem::replace(&mut state.shown, pose);
}

/// Where the Obj will be, and face, at each of `AHEAD`: its velocity and
/// facing easing into what is wanted.
fn predict(state: &MotionMatcherState, matcher: &MotionMatcher, obj: Ground) -> [Ground; 3] {
    let now = flat(state.velocity);
    let wanted = matcher.wanted_velocity.map_or(now, flat);
    let wanted_yaw = matcher.wanted_facing.map(flat).filter(|d| d.length_squared() > 1e-6).map_or(obj.yaw, facing);
    let rate = std::f32::consts::LN_2 / matcher.response.max(1e-3);
    AHEAD.map(|ahead| {
        let t = ahead as f32 / CLIP_RATE;
        let decay = (-rate * t).exp();
        let moved = wanted * t + (now - wanted) * (1.0 - decay) / rate;
        Ground { position: obj.position + moved, yaw: wrap(wanted_yaw + wrap(obj.yaw - wanted_yaw) * decay) }
    })
}

/// Looks for the frame that best carries on from the one playing into
/// `path`, and jumps to it, settling, when it is another.
fn search(
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    animation: &ModelAnimation,
    database: &MotionDatabase,
    body: Ground,
    path: &[Ground; 3],
    delta: f32,
) {
    let mut raw = [0.0; FEATURES];
    for (k, there) in path.iter().enumerate() {
        let there = body.to(*there);
        raw[15 + k * 2] = there.position.x;
        raw[16 + k * 2] = there.position.z;
        raw[21 + k * 2] = there.yaw.sin();
        raw[22 + k * 2] = there.yaw.cos();
    }
    let mut query = database.normalise(&raw);
    // The pose is the one playing, as the database describes it.
    let current = state.playing.map(|(index, time)| {
        let run = database.runs[index];
        run.start + ((time * CLIP_RATE).round() as usize).min(run.frames - 1)
    });
    query[..15]
        .copy_from_slice(&current.map_or([0.0; 15], |frame| std::array::from_fn(|d| database.features(frame)[d])));
    let current_cost = current
        .filter(|&frame| database.landable(frame))
        .map_or(f32::MAX, |frame| distance(database.features(frame), &query));
    let (best, _) = database.search(&query, (current.unwrap_or(usize::MAX), current_cost));
    if best == usize::MAX {
        return;
    }
    let index = database.run_of(best);
    let near = current.is_some_and(|frame| database.run_of(frame) == index && frame.abs_diff(best) <= NEAR);
    if near {
        return;
    }
    let run = database.runs[index];
    let time = (best - run.start) as f32 / CLIP_RATE;
    if state.playing.is_some() && matcher.blend > 0.0 && delta > 0.0 {
        let (mut target, mut target_before) = (animation.skeleton.rest(), animation.skeleton.rest());
        animation.clips[run.clip].sample(time, run.looping, &mut target);
        animation.clips[run.clip].sample(time - delta, run.looping, &mut target_before);
        let old = [state.shown.as_slice(), &state.before];
        let mut settling = Settling::between(old, [&target, &target_before], delta, matcher.blend * 0.5);
        let root = database.bones.root;
        settling.still(root);
        state.settling = Some(settling);
    }
    state.playing = Some((index, time));
}

/// Pulls the body towards the Obj, and holds it within reach.
fn pull(state: &mut MotionMatcherState, matcher: &MotionMatcher, obj: Ground, delta: f32) {
    let Some(mut body) = state.body else { return };
    body.position = body.position.lerp(obj.position, 1.0 - 0.5f32.powf(delta / PULL_HALFLIFE));
    body.position = obj.position + (body.position - obj.position).clamp_length_max(matcher.max_drift.max(0.0));
    let limit = matcher.max_turn.max(0.0).to_radians();
    let turned = wrap(body.yaw - obj.yaw) * 0.5f32.powf(delta / TURN_HALFLIFE);
    body.yaw = wrap(obj.yaw + turned.clamp(-limit, limit));
    state.body = Some(body);
}

/// Sets the root bone so the model stands where the body is, rather than
/// where its clip's root has travelled to.
fn place_root(
    pose: &mut [BonePose],
    animation: &ModelAnimation,
    database: &MotionDatabase,
    body: Ground,
    placed: Mat4,
) {
    let root = database.bones.root;
    let matrices = animation.skeleton.model_matrices(pose);
    let ground = Ground::of(matrices[root], database.rest_root);
    let height = Mat4::from_translation(Vec3::Y * placed.w_axis.y);
    let wanted = placed.inverse() * height * body.matrix() * ground.matrix().inverse() * matrices[root];
    let parent = animation.skeleton.bones[root].parent.map_or(Mat4::IDENTITY, |parent| matrices[parent]);
    let (scale, rotation, translation) = (parent.inverse() * wanted).to_scale_rotation_translation();
    pose[root] = BonePose { translation, rotation, scale };
}

/// Draws the path wanted from the Obj, the path the frame playing takes
/// from the body, and the body's place.
fn draw(state: &MotionMatcherState, matcher: &MotionMatcher, database: &MotionDatabase, placed: Mat4) {
    use voxear_core::debug_draw::{arrow, line};
    let (Some(body), Some(current)) = (state.body, state.playing) else { return };
    let obj = Ground::new(placed.w_axis.truncate(), facing(placed.transform_vector3(Vec3::Z)));
    let lift = Vec3::Y * (placed.w_axis.y + 0.05);
    let wanted = predict(state, matcher, obj);
    let run = database.runs[current.0];
    let frame = run.start + ((current.1 * CLIP_RATE).round() as usize).min(run.frames - 1);
    let raw = database.raw(frame);
    let played: [Ground; 3] = std::array::from_fn(|k| {
        body.then(Ground {
            position: Vec3::new(raw[15 + k * 2], 0.0, raw[16 + k * 2]),
            yaw: raw[21 + k * 2].atan2(raw[22 + k * 2]),
        })
    });
    for (path, start, color) in [(wanted, obj, [0.3, 0.8, 1.0, 1.0]), (played, body, [1.0, 0.6, 0.2, 1.0])] {
        let mut from = start.position;
        for there in path {
            line(from + lift, there.position + lift, color, 0.0, true);
            arrow(there.position + lift, there.position + lift + there.rotation() * Vec3::Z * 0.3, color, 0.0, true);
            from = there.position;
        }
    }
    line(obj.position + lift, body.position + lift, [1.0, 1.0, 1.0, 1.0], 0.0, true);
}

/// The squared distance between a frame's features and a query.
fn distance(a: &[f32], b: &[f32; FEATURES]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::Quat;
    use voxear_render::{Bone, Clip, Skeleton};

    const DT: f32 = 1.0 / 60.0;

    /// A root, hips and two feet, and four clips four seconds long:
    /// standing, walking forward and stepping sideways, each from a
    /// standing start, and walking round to the right.
    fn walker() -> ModelAnimation {
        let rest = BonePose::default();
        let bone = |name: &str, parent| Bone { name: name.into(), parent, rest };
        let skeleton = Skeleton {
            bones: vec![bone("root", None), bone("hips", Some(0)), bone("foot.L", Some(1)), bone("foot.R", Some(1))],
        };
        let clip = |name: &str, path: &dyn Fn(f32) -> Ground, swing: &dyn Fn(f32) -> Vec3| {
            let frames = 241;
            let poses = (0..frames)
                .flat_map(|f| {
                    let t = f as f32 / CLIP_RATE;
                    let ground = path(t);
                    let stride = swing(t) * (std::f32::consts::TAU * t).sin();
                    let at = |translation| BonePose { translation, ..rest };
                    [
                        BonePose { translation: ground.position, rotation: Quat::from_rotation_y(ground.yaw), ..rest },
                        at(Vec3::new(0.0, 0.9, 0.0)),
                        at(Vec3::new(0.1, -0.85, 0.0) + stride),
                        at(Vec3::new(-0.1, -0.85, 0.0) - stride),
                    ]
                })
                .collect();
            Clip { name: name.into(), frames, poses }
        };
        let turning = std::f32::consts::FRAC_PI_2;
        // Half a second to reach full speed and stride.
        let off = |t: f32| if t < 0.5 { t * t } else { t - 0.25 };
        let striding = |t: f32| (t / 0.5).min(1.0);
        let clips = vec![
            clip("Stand", &|_| Ground::default(), &|_| Vec3::ZERO),
            clip("Walk", &|t| Ground::new(Vec3::new(0.0, 0.0, 1.4 * off(t)), 0.0), &|t| Vec3::Z * 0.25 * striding(t)),
            clip("Strafe", &|t| Ground::new(Vec3::new(1.2 * off(t), 0.0, 0.0), 0.0), &|t| Vec3::X * 0.2 * striding(t)),
            clip(
                "Turn",
                &|t| {
                    let yaw = turning * t;
                    Ground::new(Vec3::new((1.0 - yaw.cos()) / turning, 0.0, yaw.sin() / turning), yaw)
                },
                &|_| Vec3::Z * 0.2,
            ),
        ];
        ModelAnimation { skeleton, skins: Vec::new(), clips }
    }

    fn placed(ground: Ground) -> Mat4 {
        ground.matrix()
    }

    /// Plays `seconds` with the Obj moved by `motion`, from time 0, and
    /// returns the clip playing at the end.
    fn drive(
        state: &mut MotionMatcherState,
        matcher: &MotionMatcher,
        animation: &ModelAnimation,
        database: &Arc<MotionDatabase>,
        seconds: f32,
        motion: &dyn Fn(f32) -> Ground,
    ) -> String {
        let frames = (seconds / DT).round() as usize;
        for f in 0..frames {
            step(state, matcher, animation, database, placed(motion(f as f32 * DT)), DT);
        }
        let (clip, _) = state.playing().expect("a clip playing");
        animation.clips[clip].name.clone()
    }

    #[test]
    fn a_frame_finds_itself() {
        let animation = walker();
        let database = MotionDatabase::build(&animation, &MotionWeights::default()).expect("bones found");
        assert_eq!(database.runs.len(), 4);
        let frame = database.runs[3].start + 50;
        let query: [f32; FEATURES] = std::array::from_fn(|d| database.features(frame)[d]);
        let (found, cost) = database.search(&query, (usize::MAX, f32::MAX));
        assert!(cost < 1e-6, "found at no cost: {cost}");
        assert_eq!(database.run_of(found), 3, "in the turning clip");
    }

    #[test]
    fn the_movement_wanted_picks_its_clip() {
        let animation = walker();
        let database = Arc::new(MotionDatabase::build(&animation, &MotionWeights::default()).unwrap());
        let matcher = MotionMatcher::default();
        let still = |_: f32| Ground::default();
        let mut state = MotionMatcherState::default();
        assert_eq!(drive(&mut state, &matcher, &animation, &database, 1.0, &still), "Stand");
        let mut state = MotionMatcherState::default();
        let forward = |t: f32| Ground::new(Vec3::new(0.0, 0.0, 1.4 * t), 0.0);
        assert_eq!(drive(&mut state, &matcher, &animation, &database, 1.0, &forward), "Walk");
        let mut state = MotionMatcherState::default();
        let sideways = |t: f32| Ground::new(Vec3::new(1.2 * t, 0.0, 0.0), 0.0);
        assert_eq!(drive(&mut state, &matcher, &animation, &database, 1.0, &sideways), "Strafe");
        // Turning right, and wanting to face further round.
        let turning = std::f32::consts::FRAC_PI_2;
        let round = |t: f32| {
            let yaw = turning * t;
            Ground::new(Vec3::new((1.0 - yaw.cos()) / turning, 0.0, yaw.sin() / turning), yaw)
        };
        let mut state = MotionMatcherState::default();
        let mut turner = matcher.clone();
        for f in 0..90 {
            let at = round(f as f32 * DT);
            turner.wanted_facing = Some(Quat::from_rotation_y(at.yaw + turning) * Vec3::Z);
            step(&mut state, &turner, &animation, &database, placed(at), DT);
        }
        let (clip, _) = state.playing().unwrap();
        assert_eq!(animation.clips[clip].name, "Turn");
    }

    #[test]
    fn the_body_keeps_near_the_obj_and_never_pops() {
        let animation = walker();
        let database = Arc::new(MotionDatabase::build(&animation, &MotionWeights::default()).unwrap());
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        // Stand, walk off at once, stop dead, then step sideways.
        let mut obj = Ground::default();
        let mut seen: Option<Vec<Vec3>> = None;
        for f in 0..300 {
            let t = f as f32 * DT;
            let velocity = match t {
                t if t < 1.0 => Vec3::ZERO,
                t if t < 3.0 => Vec3::Z * 1.4,
                t if t < 4.0 => Vec3::ZERO,
                _ => Vec3::X * 1.2,
            };
            obj.position += velocity * DT;
            step(&mut state, &matcher, &animation, &database, placed(obj), DT);
            let drift = state.drift(obj.position);
            assert!(drift <= matcher.max_drift + 1e-4, "within reach at {t:.2}s: {drift}");
            let bones: Vec<Vec3> = animation
                .skeleton
                .model_matrices(&state.shown)
                .iter()
                .map(|m| placed(obj).transform_point3(m.w_axis.truncate()))
                .collect();
            if let Some(before) = &seen {
                for (bone, (now, then)) in bones.iter().zip(before).enumerate() {
                    let jump = now.distance(*then);
                    assert!(jump < 0.08, "bone {bone} jumped {jump} at {t:.2}s");
                }
            }
            seen = Some(bones);
        }
    }

    #[test]
    fn the_model_stands_where_the_body_is() {
        let animation = walker();
        let database = Arc::new(MotionDatabase::build(&animation, &MotionWeights::default()).unwrap());
        let mut state = MotionMatcherState::default();
        let matcher = MotionMatcher::default();
        let at = Ground::new(Vec3::new(5.0, 0.0, -3.0), 1.0);
        for _ in 0..30 {
            step(&mut state, &matcher, &animation, &database, placed(at), DT);
        }
        let root = animation.skeleton.model_matrices(&state.shown)[0].w_axis.truncate();
        assert!(root.length() < 0.01, "the root over the Obj, though its clip's root is elsewhere: {root}");
    }

    /// Drives the model at `VOXEAR_TEST_MODEL`, when set, through a walk, a
    /// stop, steps sideways and back, and a turn about, reporting what it
    /// plays: for checking real clips by hand.
    #[test]
    fn a_model_named_by_the_environment_matches_its_motion() {
        let Ok(path) = std::env::var("VOXEAR_TEST_MODEL") else { return };
        let model = voxear_render::prepare_model(&path, &mut |file| Ok(std::fs::read(file)?)).unwrap();
        let animation = model.animation().cloned().expect("an animated model");
        let started = std::time::Instant::now();
        let database = Arc::new(MotionDatabase::build(&animation, &MotionWeights::default()).expect("bones found"));
        eprintln!("{} frames described in {:?}", database.frames(), started.elapsed());
        for run in &database.runs {
            eprintln!(
                "  {} {} frames{}",
                animation.clips[run.clip].name,
                run.frames,
                if run.looping { " looping" } else { "" }
            );
        }
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let (mut obj, mut velocity) = (Ground::default(), Vec3::ZERO);
        let (mut playing, mut worst_drift, mut worst_jump) = (String::new(), 0.0f32, (0.0f32, 0.0f32, 0usize));
        let mut seen: Option<Vec<Vec3>> = None;
        let started = std::time::Instant::now();
        let frames = 900;
        for f in 0..frames {
            let t = f as f32 * DT;
            let (wanted, turn) = match t {
                t if t < 1.5 => (Vec3::ZERO, 0.0),
                t if t < 4.5 => (Vec3::Z * 1.4, 0.0),
                t if t < 6.0 => (Vec3::ZERO, 0.0),
                t if t < 8.0 => (Vec3::X * 1.0, 0.0),
                t if t < 10.0 => (Vec3::NEG_Z * 1.0, 0.0),
                t if t < 13.0 => (Vec3::Z * 1.4, std::f32::consts::PI / 3.0),
                _ => (Vec3::ZERO, 0.0),
            };
            // As a game's controller would: easing into the speed wanted.
            velocity = velocity.lerp(obj.rotation() * wanted, 1.0 - 0.5f32.powf(DT / 0.2));
            obj.position += velocity * DT;
            obj.yaw = wrap(obj.yaw + turn * DT);
            let mut asked = matcher.clone();
            asked.wanted_velocity = Some(obj.rotation() * wanted);
            step(&mut state, &asked, &animation, &database, placed(obj), DT);
            let name = state.playing().map(|(clip, time)| format!("{} {:.2}s", animation.clips[clip].name, time));
            let clip = name.clone().unwrap_or_default();
            let clip = clip.split(' ').next().unwrap_or_default().to_string();
            if clip != playing {
                eprintln!("{t:5.2}s {}", name.unwrap_or_default());
                playing = clip;
            }
            worst_drift = worst_drift.max(state.drift(obj.position));
            let bones: Vec<Vec3> = animation
                .skeleton
                .model_matrices(&state.shown)
                .iter()
                .map(|m| placed(obj).transform_point3(m.w_axis.truncate()))
                .collect();
            if let Some(before) = &seen {
                for (bone, (now, then)) in bones.iter().zip(before).enumerate() {
                    let jump = now.distance(*then);
                    if jump > worst_jump.0 {
                        worst_jump = (jump, t, bone);
                    }
                }
            }
            seen = Some(bones);
        }
        let bone = &animation.skeleton.bones[worst_jump.2].name;
        eprintln!(
            "{:?} a frame; drift at most {worst_drift:.3} m; {bone} moved at most {:.3} m in a frame, at {:.2}s",
            started.elapsed() / frames,
            worst_jump.0,
            worst_jump.1
        );
        assert!(worst_drift <= matcher.max_drift + 1e-3);
    }
}
