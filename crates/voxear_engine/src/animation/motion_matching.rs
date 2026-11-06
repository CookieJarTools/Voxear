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
//! slower so their feet keep pace with the ground the Obj covers.
//!
//! The game moves the Obj, by a Character Controller or otherwise, and the
//! model stands on it. The body turns, at its turn rate, the way it is
//! wanted to face, or the way it goes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, ModelAnimation, ModelRenderer, Pose, Skeleton};
use voxear_scene::Destroying;

use super::motion_database::{ClipRun, Ground, MotionDatabase, database_for, facing, wrap};

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
    /// The clip it stands with, by its name in the model; none, the first
    /// loop that stands still.
    pub idle: String,
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
    /// Playing clips a little faster or slower so their feet keep pace with
    /// the ground.
    pub match_speed: bool,
    /// Drawing the velocity wanted, the clips' velocity and the body's
    /// facing, and which clips play.
    pub debug: bool,
}

impl Component for MotionMatcher {}

impl Default for MotionMatcher {
    fn default() -> Self {
        Self {
            wanted_velocity: None,
            wanted_facing: None,
            idle: String::new(),
            idle_speed: 0.15,
            search_every: 0.1,
            blend: 0.15,
            turn_rate: 540.0,
            turn_to_movement: true,
            match_speed: true,
            debug: false,
        }
    }
}

/// What a Motion Matcher is doing, kept by the engine.
#[derive(Clone, Default)]
pub struct MotionMatcherState {
    database: Option<Arc<MotionDatabase>>,
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
    /// The Obj's place last frame, and its velocity as seen.
    last: Option<Vec3>,
    velocity: Vec3,
}

impl Component for MotionMatcherState {}

impl MotionMatcherState {
    /// Each model clip playing and its share of the pose, the clip it
    /// stands with included, most first.
    pub fn playing(&self, matcher: &MotionMatcher, animation: &ModelAnimation) -> Vec<(usize, f32)> {
        let Some(database) = &self.database else { return Vec::new() };
        let walking = 1.0 - self.idle_weight;
        let mut out: Vec<(usize, f32)> = database
            .runs
            .iter()
            .zip(&self.weights)
            .filter(|(_, w)| **w * walking > 1e-3)
            .map(|(run, w)| (run.clip, w * walking))
            .collect();
        if let Some(idle) = idle_clip(matcher, animation, database).filter(|_| self.idle_weight > 1e-3) {
            out.push((idle, self.idle_weight));
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
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
        let Some(database) = database_for(world, &animation) else { continue };
        let placed = voxear_scene::world_matrix(world, entity);
        let mut state = world.get::<MotionMatcherState>(entity).cloned().unwrap_or_default();
        let pose = step(&mut state, &matcher, &animation, &database, placed, delta);
        if matcher.debug {
            draw(&state, &matcher, &animation, &database, placed);
        }
        let bones = animation.skeleton.model_matrices(&pose).into();
        world.insert(entity, Pose { bones });
        world.insert(entity, state);
    }
}

/// Moves `state` on by `delta`, its Obj placed by `placed`: turns the body,
/// searches when it is time, fades the clips' weights and plays them on.
/// Returns the model's pose.
fn step(
    state: &mut MotionMatcherState,
    matcher: &MotionMatcher,
    animation: &ModelAnimation,
    database: &Arc<MotionDatabase>,
    placed: Mat4,
    delta: f32,
) -> Vec<BonePose> {
    let obj = Ground::new(placed.w_axis.truncate(), facing(placed.transform_vector3(Vec3::Z)));
    if !state.database.as_ref().is_some_and(|d| Arc::ptr_eq(d, database)) {
        let runs = database.runs.len();
        *state = MotionMatcherState {
            database: Some(database.clone()),
            weights: vec![0.0; runs],
            targets: vec![0.0; runs],
            idle_weight: 1.0,
            rate: 1.0,
            ..MotionMatcherState::default()
        };
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
    let local = Quat::from_rotation_y(-body) * wanted;
    let idle = idle_clip(matcher, animation, database);
    let standing = idle.is_some() && wanted.length() <= matcher.idle_speed;
    state.since_search += delta;
    let changed = state.searched.is_none_or(|searched| searched.distance(local) >= CHANGED);
    let unset = state.targets.iter().all(|&w| w <= 0.0);
    if !standing && (state.since_search >= matcher.search_every || changed || unset) {
        aim(state, database, local);
        state.since_search = 0.0;
        state.searched = Some(local);
    }
    fade(state, matcher, standing && idle.is_some(), delta);
    play_on(state, matcher, database, delta);
    let mut pose = pose(state, animation, database, idle);
    turn_root(&mut pose, &animation.skeleton, database.root, wrap(body - obj.yaw));
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
        let gap = wrap(target - *body);
        let most = matcher.turn_rate.to_radians() * delta;
        *body = match matcher.turn_rate > 0.0 {
            true => wrap(*body + gap.clamp(-most, most)),
            false => target,
        };
    }
    *body
}

/// The model clip it stands with: the one named, or the first loop that
/// stands still.
fn idle_clip(matcher: &MotionMatcher, animation: &ModelAnimation, database: &MotionDatabase) -> Option<usize> {
    if !matcher.idle.is_empty() {
        return animation.clips.iter().position(|clip| clip.name == matcher.idle);
    }
    database.runs.iter().find(|run| run.stands()).map(|run| run.clip)
}

/// Sets the weights to fade towards from the clips that best move at
/// `local`, seen from the body: the few best, weighed by how well each
/// matched, but only those heading the way wanted.
fn aim(state: &mut MotionMatcherState, database: &MotionDatabase, local: Vec3) {
    let found = database.search(&database.query(local));
    let way = local.normalize_or_zero();
    state.targets.fill(0.0);
    for &(run, cost) in found.iter().take(CANDIDATES) {
        let heading = database.runs[run].velocity.normalize_or_zero();
        if way != Vec3::ZERO && heading.dot(way) < ALIGNED {
            continue;
        }
        state.targets[run] += (-cost * SHARPNESS).exp();
    }
    let total: f32 = state.targets.iter().sum();
    match total > 1e-6 {
        true => state.targets.iter_mut().for_each(|w| *w /= total),
        // None heads the way wanted: the best of them all, rather than none.
        false => {
            if let Some(&(run, _)) = found.first() {
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
    animation: &ModelAnimation,
    database: &MotionDatabase,
    idle: Option<usize>,
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
        .map(|(run, w)| (sample(run.clip, state.phase * run.duration()), w * walking))
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

/// Draws the velocity wanted, the clips' velocity and the body's facing,
/// and names the clips playing.
fn draw(
    state: &MotionMatcherState,
    matcher: &MotionMatcher,
    animation: &ModelAnimation,
    database: &MotionDatabase,
    placed: Mat4,
) {
    use voxear_core::debug_draw::{arrow, text};
    let Some(body) = state.body_yaw else { return };
    let at = placed.w_axis.truncate() + Vec3::Y * 0.05;
    let wanted = matcher.wanted_velocity.map_or(state.velocity, flat);
    let walking = 1.0 - state.idle_weight;
    let clips: Vec3 = database.runs.iter().zip(&state.weights).map(|(run, w)| run.velocity * w * walking).sum();
    let turned = Quat::from_rotation_y(body);
    arrow(at, at + wanted * 0.5, [0.3, 0.8, 1.0, 1.0], 0.0, true);
    arrow(at, at + turned * clips * 0.5, [1.0, 0.6, 0.2, 1.0], 0.0, true);
    arrow(at, at + turned * Vec3::Z * 0.6, [1.0, 1.0, 1.0, 1.0], 0.0, true);
    let names: Vec<String> = state
        .playing(matcher, animation)
        .iter()
        .take(3)
        .map(|&(clip, w)| format!("{} {:.0}%", animation.clips[clip].name, w * 100.0))
        .collect();
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

    /// One loop a second long of a root, hips and two feet: the root moving
    /// at `velocity`, the feet swinging along it.
    fn clip(name: &str, velocity: Vec3) -> Clip {
        let rest = BonePose::default();
        let frames = 61;
        let swing = velocity.normalize_or_zero() * 0.1 * velocity.length().sqrt();
        let poses = (0..frames)
            .flat_map(|f| {
                let t = f as f32 / CLIP_RATE;
                let stride = swing * (std::f32::consts::TAU * t).sin();
                let at = |translation| BonePose { translation, ..rest };
                [
                    at(velocity * t),
                    at(Vec3::new(0.0, 0.9, 0.0)),
                    at(Vec3::new(0.1, -0.85, 0.0) + stride),
                    at(Vec3::new(-0.1, -0.85, 0.0) - stride),
                ]
            })
            .collect();
        Clip { name: name.into(), frames, poses }
    }

    /// A loop that stays in place while its planted foot slides back at
    /// `speed`: each foot down for half the loop, then lifted and swung
    /// forward.
    fn in_place(name: &str, speed: f32) -> Clip {
        let rest = BonePose::default();
        let frames = 61;
        let reach = speed * 0.25;
        let foot = |t: f32, side: f32| {
            let t = (t + if side > 0.0 { 0.0 } else { 0.5 }).rem_euclid(1.0);
            let (z, y) = match t < 0.5 {
                true => (reach - speed * t, 0.0),
                false => (-reach + speed * (t - 0.5), 0.1),
            };
            Vec3::new(0.1 * side, y - 0.9, z)
        };
        let poses = (0..frames)
            .flat_map(|f| {
                let t = f as f32 / CLIP_RATE;
                let at = |translation| BonePose { translation, ..rest };
                [at(Vec3::ZERO), at(Vec3::new(0.0, 0.9, 0.0)), at(foot(t, 1.0)), at(foot(t, -1.0))]
            })
            .collect();
        Clip { name: name.into(), frames, poses }
    }

    fn walker(clips: Vec<Clip>) -> ModelAnimation {
        let rest = BonePose::default();
        let bone = |name: &str, parent| Bone { name: name.into(), parent, rest };
        let skeleton = Skeleton {
            bones: vec![bone("root", None), bone("hips", Some(0)), bone("foot.L", Some(1)), bone("foot.R", Some(1))],
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
        animation: &ModelAnimation,
        database: &Arc<MotionDatabase>,
        seconds: f32,
        velocity: Vec3,
    ) -> (String, f32) {
        let mut asked = matcher.clone();
        asked.wanted_velocity = Some(velocity);
        let mut at = state.last.unwrap_or_default();
        for _ in 0..(seconds / DT).round() as usize {
            at += velocity * DT;
            step(state, &asked, animation, database, Mat4::from_translation(at), DT);
        }
        let (clip, weight) = state.playing(matcher, animation)[0];
        (animation.clips[clip].name.clone(), weight)
    }

    #[test]
    fn clips_move_as_their_roots_or_planted_feet_carry_them() {
        let animation =
            walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4), in_place("Treadmill", 1.4)]);
        let database = MotionDatabase::build(&animation).unwrap();
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
    fn the_speed_wanted_picks_its_clips() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation).unwrap());
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let (name, weight) = drive(&mut state, &matcher, &animation, &database, 1.0, Vec3::ZERO);
        assert_eq!((name.as_str(), weight > 0.95), ("Stand", true));
        let (name, weight) = drive(&mut state, &matcher, &animation, &database, 1.0, Vec3::Z * 1.4);
        assert_eq!((name.as_str(), weight > 0.9), ("Walk", true), "{weight}");
        let (name, _) = drive(&mut state, &matcher, &animation, &database, 1.0, Vec3::Z * 3.5);
        assert_eq!(name, "Jog");
        // Stepping back while facing ahead.
        let mut aiming = matcher.clone();
        aiming.wanted_facing = Some(Vec3::Z);
        let (name, _) = drive(&mut state, &aiming, &animation, &database, 1.0, Vec3::NEG_Z * 1.2);
        assert_eq!(name, "WalkBack");
    }

    #[test]
    fn the_body_turns_to_its_way_at_its_rate() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation).unwrap());
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        let mut asked = matcher.clone();
        asked.wanted_velocity = Some(Vec3::X * 1.4);
        let most = matcher.turn_rate.to_radians() * DT;
        let mut before = 0.0;
        for _ in 0..30 {
            step(&mut state, &asked, &animation, &database, Mat4::IDENTITY, DT);
            let yaw = state.body_yaw().unwrap();
            assert!(wrap(yaw - before).abs() <= most + 1e-4, "{before} to {yaw}");
            before = yaw;
        }
        assert!((before - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "facing the way it goes: {before}");
        let (clip, _) = state.playing(&matcher, &animation)[0];
        assert_eq!(animation.clips[clip].name, "Walk", "walking forward that way");
    }

    #[test]
    fn starting_stopping_and_changing_pace_never_pop() {
        let animation = moves();
        let database = Arc::new(MotionDatabase::build(&animation).unwrap());
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
            let pose = step(&mut state, &matcher, &animation, &database, placed, DT);
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

    #[test]
    fn clips_keep_pace_with_the_ground() {
        let animation = walker(vec![clip("Stand", Vec3::ZERO), clip("Walk", Vec3::Z * 1.4)]);
        let database = Arc::new(MotionDatabase::build(&animation).unwrap());
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        drive(&mut state, &matcher, &animation, &database, 1.5, Vec3::Z * 1.6);
        assert!((state.rate() - 1.6 / 1.4).abs() < 0.02, "a little faster: {}", state.rate());
        let mut steady = matcher.clone();
        steady.match_speed = false;
        drive(&mut state, &steady, &animation, &database, 0.1, Vec3::Z * 1.6);
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
        let database = Arc::new(MotionDatabase::build(&animation).expect("a root"));
        for run in &database.runs {
            let name = &animation.clips[run.clip].name;
            eprintln!("  {name} {:.2}s {:?} loops {}", run.duration(), run.velocity, run.looping);
        }
        let matcher = MotionMatcher::default();
        let mut state = MotionMatcherState::default();
        for (seconds, velocity) in
            [(1.0, Vec3::ZERO), (2.0, Vec3::Z * 1.4), (2.0, Vec3::Z * 5.0), (1.0, Vec3::ZERO), (2.0, Vec3::X * 3.0)]
        {
            drive(&mut state, &matcher, &animation, &database, seconds, velocity);
            let playing: Vec<String> = state
                .playing(&matcher, &animation)
                .iter()
                .map(|&(clip, w)| format!("{} {:.0}%", animation.clips[clip].name, w * 100.0))
                .collect();
            eprintln!("{velocity}: {} ×{:.2}", playing.join(", "), state.rate());
        }
        // How fast a foot on the ground slides over it, at a steady walk and run.
        let toes: Vec<usize> =
            ["DEF-toe.L", "DEF-toe.R"].iter().filter_map(|name| animation.skeleton.find(name)).collect();
        for speed in [1.4, 5.0] {
            let mut asked = matcher.clone();
            asked.wanted_velocity = Some(Vec3::Z * speed);
            let (mut at, mut before, mut slides) = (Vec3::ZERO, None::<Vec<Vec3>>, Vec::new());
            for f in 0..240 {
                at += Vec3::Z * speed * DT;
                let placed = Mat4::from_translation(at);
                let pose = step(&mut state, &asked, &animation, &database, placed, DT);
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
