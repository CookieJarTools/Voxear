//! Mob Motion: a model animated from how its Obj moves, with no clips. A
//! model whose bones say what part of a body each moves (see
//! `voxear_render::BoneRole`, as a box model's do) walks when its Obj
//! moves, its legs stepping by how far it has gone so its feet keep pace
//! with the ground: two legs turn about, four step as diagonal pairs, six
//! and more as two sets. Lower legs bend as they come through, arms swing
//! against the legs, the body bobs and leans into its speed, the head turns
//! to what it is told to look at, no further than a neck goes and not all
//! at once, a tail and wings trail behind what the body does, and standing
//! still it breathes. An attack and a flinch play over all of that.
//!
//! [`MobPoser`] is the motion itself, from a skeleton at rest, its roles
//! and a [`MotionInput`] to every bone's pose, with nothing of the world in
//! it, so an editor can show it on a model that stands nowhere.
//! [`MobMotion`] is the component: its settings, what a game tells it
//! (where to look, to attack, that it was hurt), and [`animate_mobs`] works
//! out each one's input from how its Obj moved since the last frame.

use std::{f32::consts::PI, sync::Arc};

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{BonePose, BoneRole, ModelRenderer, Pose, Skeleton};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{Content, scene_file::ModelSource};

/// How long an attack and a flinch last, in seconds.
const ATTACK_SECONDS: f32 = 0.4;
const HURT_SECONDS: f32 = 0.35;

/// How a mob's model moves: how much of each motion, each 1 as made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MobMotion {
    /// How far its legs swing as it walks, in degrees.
    pub leg_swing: f32,
    /// How far its arms swing against its legs, in degrees.
    pub arm_swing: f32,
    /// How many metres one step of each leg covers, or 0 for what its legs'
    /// length gives: longer strides step slower at a speed.
    pub stride: f32,
    /// How much its body bobs and leans as it walks.
    pub bounce: f32,
    /// How much it breathes and shifts standing still.
    pub idle: f32,
    /// How fast its head turns to what it looks at: higher is quicker.
    pub look_speed: f32,
    /// How far its head turns from straight ahead, in degrees.
    pub look_limit: f32,
    /// How much its tail and wings trail behind what its body does.
    pub lag: f32,
    /// The point in the world its head turns to, for a game to set; none
    /// looks ahead.
    #[serde(skip)]
    pub look_at: Option<Vec3>,
    /// How it is to be taken to move, in metres a second in the world, for
    /// a game that knows better than where its Obj went; none goes by the
    /// Obj.
    #[serde(skip)]
    pub velocity: Option<Vec3>,
    /// An attack and a flinch asked for and not yet begun.
    #[serde(skip)]
    pub attacks: u32,
    #[serde(skip)]
    pub hurts: u32,
}

impl Component for MobMotion {}

impl Default for MobMotion {
    fn default() -> Self {
        Self {
            leg_swing: 38.0,
            arm_swing: 30.0,
            stride: 0.0,
            bounce: 1.0,
            idle: 1.0,
            look_speed: 9.0,
            look_limit: 70.0,
            lag: 1.0,
            look_at: None,
            velocity: None,
            attacks: 0,
            hurts: 0,
        }
    }
}

impl MobMotion {
    /// Swings at what is before it: its arms where it has them, or its
    /// whole body.
    pub fn attack(&mut self) {
        self.attacks += 1;
    }

    /// Flinches, as when it is hit.
    pub fn hurt(&mut self) {
        self.hurts += 1;
    }
}

/// What part of a body each bone of a Mob Motion's model moves, given by
/// the game from the model's file (see [`give_mob_bones`]).
#[derive(Clone, Debug, PartialEq)]
pub struct MobBones {
    pub roles: Arc<[BoneRole]>,
    /// The model the roles are of, as the Obj's Model Renderer names it.
    model: String,
}

impl Component for MobBones {}

/// What moves a model for one frame, in the model's own space: looking
/// along +Z, a metre a metre whatever its Obj's size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionInput {
    /// How it moves along the ground, in metres a second.
    pub velocity: Vec3,
    /// How fast it turns about its up, in radians a second.
    pub turn: f32,
    /// Which way its head is to look, or none for ahead.
    pub look: Option<Vec3>,
    pub delta: f32,
}

/// A leg: its bone, the bone of its lower half where it has one, how long
/// it is from hip to ground, and where in the walk it steps, as a share of
/// a whole turn.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Leg {
    bone: usize,
    lower: Option<usize>,
    length: f32,
    offset: f32,
}

/// An arm or a wing: its bone, its lower half's, and which side it is on:
/// 1 for the right, -1 for the left.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Side {
    bone: usize,
    lower: Option<usize>,
    side: f32,
}

/// The motion: what a model's bones are, and where in its walk, its
/// breath, its look and its swings it is.
#[derive(Clone, Debug, Default)]
pub struct MobPoser {
    body: Option<usize>,
    head: Option<usize>,
    tail: Option<usize>,
    legs: Vec<Leg>,
    arms: Vec<Side>,
    wings: Vec<Side>,
    /// How many bones it was made for.
    bones: usize,
    /// How far through a walk's turn it is, in radians, and how much of a
    /// walk it is doing, from 0 standing.
    phase: f32,
    walking: f32,
    /// How long it has stood or walked, for what goes on by itself.
    time: f32,
    /// Where its head looks, round and up, in radians.
    look: [f32; 2],
    /// How far its tail trails to one side, and how fast that changes.
    trail: [f32; 2],
    /// How far through an attack and a flinch it is, in seconds.
    attack: Option<f32>,
    hurt: Option<f32>,
}

impl MobPoser {
    /// A poser for a skeleton whose bones have these roles, one each.
    pub fn new(skeleton: &Skeleton, roles: &[BoneRole]) -> Self {
        let placed = skeleton.model_matrices(&skeleton.rest());
        let at = |bone: usize| placed[bone].w_axis.truncate();
        let role = |bone: usize| roles.get(bone).copied().unwrap_or_default();
        let parent_role = |bone: usize| skeleton.bones[bone].parent.map(role);
        let first = |wanted: BoneRole| (0..skeleton.bones.len()).find(|&bone| role(bone) == wanted);
        // A limb is a bone of its role under one that is not; one under it of the same role is its lower half.
        let limbs = |wanted: BoneRole| -> Vec<(usize, Option<usize>)> {
            let tops =
                (0..skeleton.bones.len()).filter(|&bone| role(bone) == wanted && parent_role(bone) != Some(wanted));
            tops.map(|top| {
                let lower = (0..skeleton.bones.len())
                    .find(|&bone| role(bone) == wanted && skeleton.bones[bone].parent == Some(top));
                (top, lower)
            })
            .collect()
        };
        let side = |bone: usize| if at(bone).x >= 0.0 { 1.0 } else { -1.0 };
        let sided =
            |wanted| limbs(wanted).into_iter().map(|(bone, lower)| Side { bone, lower, side: side(bone) }).collect();

        // Legs step in rows from the front back: each the other way from the one beside it and the one behind it.
        let mut rows: Vec<(usize, Option<usize>)> = limbs(BoneRole::Leg);
        rows.sort_by(|a, b| at(b.0).z.total_cmp(&at(a.0).z).then(at(b.0).x.total_cmp(&at(a.0).x)));
        let mut legs: Vec<Leg> = Vec::new();
        let mut row = 0;
        for (place, &(bone, lower)) in rows.iter().enumerate() {
            if place > 0 && (at(rows[place - 1].0).z - at(bone).z).abs() > 0.02 {
                row += 1;
            }
            let right = at(bone).x >= 0.0;
            let offset = if (row % 2 == 0) == right { 0.0 } else { 0.5 };
            legs.push(Leg { bone, lower, length: at(bone).y.max(0.05), offset });
        }
        Self {
            body: first(BoneRole::Body),
            head: first(BoneRole::Head),
            tail: first(BoneRole::Tail),
            legs,
            arms: sided(BoneRole::Arm),
            wings: sided(BoneRole::Wing),
            bones: skeleton.bones.len(),
            ..Self::default()
        }
    }

    /// How many legs it walks on.
    pub fn legs(&self) -> usize {
        self.legs.len()
    }

    /// Whether anything of the model has a role for it to move.
    pub fn moves_anything(&self) -> bool {
        self.body.is_some() || self.head.is_some() || !self.legs.is_empty() || !self.arms.is_empty()
    }

    /// Begins an attack, or a flinch.
    pub fn attack(&mut self) {
        self.attack = Some(0.0);
    }

    pub fn hurt(&mut self) {
        self.hurt = Some(0.0);
    }

    /// How far through its walk's turn it is, from 0 to 1.
    pub fn walk_phase(&self) -> f32 {
        (self.phase / (2.0 * PI)).rem_euclid(1.0)
    }

    /// Moves the motion on by `input`, and gives every bone's pose for it,
    /// from `skeleton`'s at rest. `settings` says how much of each motion.
    pub fn pose(&mut self, skeleton: &Skeleton, settings: &MobMotion, input: MotionInput) -> Vec<BonePose> {
        let mut pose = skeleton.rest();
        if pose.len() != self.bones {
            return pose;
        }
        let delta = input.delta.clamp(0.0, 0.1);
        self.time += delta;
        let along = Vec3::new(input.velocity.x, 0.0, input.velocity.z);
        let speed = along.length();
        // Backwards, the walk turns the other way.
        let forwards = if along.z < -0.05 && along.z.abs() > along.x.abs() { -1.0 } else { 1.0 };
        let leg_length = self.legs.iter().map(|leg| leg.length).fold(0.0, f32::max).max(0.1);
        let stride = if settings.stride > 0.0 { settings.stride } else { leg_length * 1.3 };
        // Two steps to a turn of the walk.
        self.phase = (self.phase + forwards * speed * delta / (2.0 * stride) * 2.0 * PI).rem_euclid(2.0 * PI);
        let wanted = (speed / (stride * 1.4)).clamp(0.0, 1.35);
        self.walking = approach(self.walking, wanted, 10.0, delta);
        let walking = self.walking;
        let still = (1.0 - walking).clamp(0.0, 1.0) * settings.idle;
        let turn = |bone: usize, by: Quat, pose: &mut Vec<BonePose>| pose[bone].rotation = pose[bone].rotation * by;

        // Legs: forward and back by the walk's turn, the lower half bending as the leg comes through.
        for leg in &self.legs {
            let at = self.phase + leg.offset * 2.0 * PI;
            let swing = settings.leg_swing.to_radians() * walking * at.sin();
            turn(leg.bone, Quat::from_rotation_x(-swing), &mut pose);
            if let Some(lower) = leg.lower {
                let bend = settings.leg_swing.to_radians() * 1.4 * walking * at.cos().max(0.0);
                turn(lower, Quat::from_rotation_x(bend), &mut pose);
            }
        }
        // Arms: against the leg of their side, a little bent, and at rest swaying with the breath.
        for arm in &self.arms {
            let at = self.phase + if arm.side > 0.0 { PI } else { 0.0 };
            let swing = settings.arm_swing.to_radians() * walking * at.sin();
            let sway = (self.time * 1.7).sin() * 0.03 * still;
            turn(arm.bone, Quat::from_rotation_x(-swing) * Quat::from_rotation_z(arm.side * sway), &mut pose);
            if let Some(lower) = arm.lower {
                turn(lower, Quat::from_rotation_x(-(0.25 + 0.35 * at.sin().max(0.0)) * walking), &mut pose);
            }
        }
        // The body: up and down twice a turn, leaning into its speed, rolling a little onto each step, breathing.
        if let Some(body) = self.body {
            let bob = -(0.5 - 0.5 * (2.0 * self.phase).cos()) * leg_length * 0.06 * walking * settings.bounce;
            let breath = (self.time * 1.7).sin() * leg_length * 0.012 * still;
            pose[body].translation.y += bob + breath;
            let lean = (speed * 0.035).min(0.25) * forwards * settings.bounce;
            let roll =
                self.phase.sin() * 0.035 * walking * settings.bounce * if self.legs.len() == 2 { 1.0 } else { 0.4 };
            turn(body, Quat::from_rotation_x(lean) * Quat::from_rotation_z(roll), &mut pose);
        }
        // The head: to where it is to look, no further than its neck goes, and not all at once; at rest it wanders.
        if let Some(head) = self.head {
            let limit = settings.look_limit.to_radians();
            let wander = [(self.time * 0.43).sin() * 0.12 * still, (self.time * 0.31).sin() * 0.04 * still];
            let goal = match input.look.and_then(Vec3::try_normalize) {
                Some(way) => {
                    [way.x.atan2(way.z).clamp(-limit, limit), (-way.y).asin().clamp(-limit * 0.6, limit * 0.6)]
                }
                None => wander,
            };
            for axis in 0..2 {
                self.look[axis] = approach(self.look[axis], goal[axis], settings.look_speed, delta);
            }
            // Against the body's lean, so it looks level as it runs.
            let level = self.body.map_or(0.0, |_| -(speed * 0.035).min(0.25) * forwards * settings.bounce * 0.7);
            turn(head, Quat::from_rotation_y(self.look[0]) * Quat::from_rotation_x(self.look[1] + level), &mut pose);
        }
        // The tail: swung out by a turn and springing back, wagging as it walks.
        if let Some(tail) = self.tail {
            let goal = (-input.turn * 0.12).clamp(-0.7, 0.7) * settings.lag;
            let pull = (goal - self.trail[0]) * 60.0 - self.trail[1] * 9.0;
            self.trail[1] += pull * delta;
            self.trail[0] += self.trail[1] * delta;
            let wag = (self.phase * 2.0).sin() * 0.12 * walking + (self.time * 1.3).sin() * 0.06 * still;
            turn(tail, Quat::from_rotation_y(self.trail[0] + wag * settings.lag), &mut pose);
        }
        // Wings: folded, lifting a little with each step and each breath.
        for wing in &self.wings {
            let lift =
                (0.08 * (2.0 * self.phase).sin() * walking + 0.03 * (self.time * 1.7).sin() * still) * settings.lag;
            turn(wing.bone, Quat::from_rotation_z(wing.side * lift), &mut pose);
        }
        self.swing(&mut pose, delta, leg_length);
        pose
    }

    /// An attack and a flinch, over whatever else it does.
    fn swing(&mut self, pose: &mut [BonePose], delta: f32, leg_length: f32) {
        if let Some(time) = self.attack {
            // Up and over, quick out and slower back.
            let through = (time / ATTACK_SECONDS).clamp(0.0, 1.0);
            let reach = (through * PI).sin().powf(0.7);
            match self.arms.iter().find(|arm| arm.side > 0.0).or(self.arms.first()) {
                Some(arm) => pose[arm.bone].rotation = pose[arm.bone].rotation * Quat::from_rotation_x(-1.9 * reach),
                // With no arm, the whole of it lunges, its head down.
                None => {
                    if let Some(body) = self.body {
                        pose[body].translation.z += leg_length * 0.35 * reach;
                    }
                    if let Some(head) = self.head {
                        pose[head].rotation = pose[head].rotation * Quat::from_rotation_x(0.5 * reach);
                    }
                }
            }
            self.attack = (time + delta < ATTACK_SECONDS).then_some(time + delta);
        }
        if let Some(time) = self.hurt {
            let through = (time / HURT_SECONDS).clamp(0.0, 1.0);
            let flinch = (1.0 - through).powi(2);
            if let Some(body) = self.body {
                pose[body].rotation = pose[body].rotation * Quat::from_rotation_x(-0.28 * flinch);
            }
            if let Some(head) = self.head {
                let shake = (through * PI * 5.0).sin() * 0.22 * flinch;
                pose[head].rotation = pose[head].rotation * Quat::from_rotation_z(shake);
            }
            self.hurt = (time + delta < HURT_SECONDS).then_some(time + delta);
        }
    }
}

/// A value moved towards a goal, the nearer the goal the slower, `rate`
/// times a second.
fn approach(value: f32, goal: f32, rate: f32, delta: f32) -> f32 {
    goal + (value - goal) * (-rate.max(0.0) * delta).exp()
}

/// What a Mob Motion is doing, kept by the engine: its poser, and where
/// its Obj was and faced at the last frame, to tell how it moves.
#[derive(Clone, Debug)]
pub struct MobMotionState {
    pub poser: MobPoser,
    /// The skeleton and roles the poser was made for.
    skeleton: usize,
    roles: Arc<[BoneRole]>,
    last: Option<(Vec3, Quat)>,
}

impl Component for MobMotionState {}

/// Gives each Obj with a Mob Motion its model's bones' roles, from the
/// model's file, once, and again when it draws another model.
pub fn give_mob_bones(world: &mut World, content: &mut Content) {
    let wanting: Vec<(Entity, String)> = world
        .query_ref::<(Entity, &MobMotion, &ModelSource)>()
        .into_iter()
        .map(|(entity, _, source)| (entity, source.0.model.clone()))
        .filter(|(entity, model)| world.get::<MobBones>(*entity).is_none_or(|bones| bones.model != *model))
        .collect();
    for (entity, model) in wanting {
        let guid = content.parse(&model).ok().and_then(|parsed| parsed.guid());
        let roles = guid.map(|guid| content.bone_roles(guid)).unwrap_or_default();
        world.insert(entity, MobBones { roles, model });
    }
}

/// Poses every Obj with a Mob Motion `delta` seconds on, from how it moved
/// since the last frame.
pub fn animate_mobs(world: &mut World, delta: f32) {
    let mobs: Vec<(Entity, MobMotion, Arc<voxear_render::ModelAnimation>, Arc<[BoneRole]>)> = world
        .query_ref::<(Entity, &MobMotion, &ModelRenderer, &MobBones)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, motion, model, bones)| {
            Some((entity, motion.clone(), model.animation.clone()?, bones.roles.clone()))
        })
        .collect();
    for (entity, mut motion, animation, roles) in mobs {
        let skeleton = &animation.skeleton;
        let made_for = Arc::as_ptr(&animation) as usize;
        let mut state = world
            .get::<MobMotionState>(entity)
            .filter(|state| state.skeleton == made_for && Arc::ptr_eq(&state.roles, &roles))
            .cloned()
            .unwrap_or_else(|| MobMotionState {
                poser: MobPoser::new(skeleton, &roles),
                skeleton: made_for,
                roles: roles.clone(),
                last: None,
            });
        if !state.poser.moves_anything() {
            continue;
        }
        // Where it is and faces in the world, and how large, to tell its own speed from the world's.
        let placed = world.get::<WorldTransform>(entity).map(|placed| placed.0).or_else(|| {
            world
                .get::<Transform>(entity)
                .map(|t| Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.position))
        });
        let (scale, facing, at) = placed.unwrap_or(Mat4::IDENTITY).to_scale_rotation_translation();
        let size = scale.y.abs().max(1e-4);
        let (was, faced) = state.last.unwrap_or((at, facing));
        let moved = match (motion.velocity, delta > 1e-5) {
            (Some(velocity), _) => velocity,
            (None, true) => (at - was) / delta,
            (None, false) => Vec3::ZERO,
        };
        let to_model = facing.inverse();
        let turned = (facing * faced.inverse()).to_scaled_axis().dot(facing * Vec3::Y);
        let input = MotionInput {
            velocity: to_model * moved / size,
            turn: if delta > 1e-5 { turned / delta } else { 0.0 },
            look: motion.look_at.map(|point| {
                let head = state.poser.head.map_or(Vec3::Y * size, |head| {
                    placed
                        .unwrap_or(Mat4::IDENTITY)
                        .transform_point3(skeleton.model_matrices(&skeleton.rest())[head].w_axis.truncate())
                });
                to_model * (point - head)
            }),
            delta,
        };
        state.last = Some((at, facing));
        (0..std::mem::take(&mut motion.attacks)).for_each(|_| state.poser.attack());
        (0..std::mem::take(&mut motion.hurts)).for_each(|_| state.poser.hurt());
        let pose = state.poser.pose(skeleton, &motion, input);
        let bones = skeleton.model_matrices(&pose).into();
        world.insert(entity, Pose { bones });
        world.insert(entity, state);
        // What was asked of it is taken.
        if let Some(kept) = world.get_mut::<MobMotion>(entity) {
            (kept.attacks, kept.hurts) = (0, 0);
        }
    }
}

/// What a script asks of a Mob Motion, by number, as `NativeMobMotion` in
/// Voxear.Core has them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const LOOK_AT: i32 = 3;
const ATTACK: i32 = 4;
const HURT: i32 = 5;
const VELOCITY: i32 = 6;
const LEG_SWING: i32 = 7;
const ARM_SWING: i32 = 8;
const STRIDE: i32 = 9;
const BOUNCE: i32 = 10;
const IDLE: i32 = 11;
const LOOK_SPEED: i32 = 12;
const LOOK_LIMIT: i32 = 13;
const LAG: i32 = 14;

/// Sets what answers `MobMotion` in scripts.
pub fn register() {
    voxear_scripting::set_mob_motion_values(values);
}

/// Reads, or with `written` writes, value `what` of an Obj's Mob Motion.
/// A point and a velocity are their three numbers and a fourth of 1, or
/// all 0 for none.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<MobMotion>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<MobMotion>(entity) {
                world.insert(entity, MobMotion::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<MobMotionState>(entity);
            world.remove::<MobBones>(entity);
            world.remove::<Pose>(entity);
            return world.remove::<MobMotion>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    let motion = world.get_mut::<MobMotion>(entity)?;
    let point = |v: Option<Vec3>| v.map_or([0.0; 4], |v| [v.x, v.y, v.z, 1.0]);
    let given = |[x, y, z, has]: [f32; 4]| (has != 0.0).then_some(Vec3::new(x, y, z));
    let amount = match what {
        LEG_SWING => &mut motion.leg_swing,
        ARM_SWING => &mut motion.arm_swing,
        STRIDE => &mut motion.stride,
        BOUNCE => &mut motion.bounce,
        IDLE => &mut motion.idle,
        LOOK_SPEED => &mut motion.look_speed,
        LOOK_LIMIT => &mut motion.look_limit,
        LAG => &mut motion.lag,
        LOOK_AT => {
            if let Some(value) = written {
                motion.look_at = given(value);
            }
            return Some(point(motion.look_at));
        }
        VELOCITY => {
            if let Some(value) = written {
                motion.velocity = given(value);
            }
            return Some(point(motion.velocity));
        }
        ATTACK => {
            return written.map(|_| {
                number(f32::from(u8::from({
                    motion.attack();
                    true
                })))
            });
        }
        HURT => {
            return written.map(|_| {
                number(f32::from(u8::from({
                    motion.hurt();
                    true
                })))
            });
        }
        _ => return None,
    };
    if let Some([value, ..]) = written {
        *amount = value.max(0.0);
    }
    Some(number(*amount))
}

#[cfg(test)]
mod tests {
    use voxear_render::Bone;

    use super::*;

    fn bone(name: &str, parent: Option<usize>, at: Vec3) -> Bone {
        Bone { name: name.into(), parent, rest: BonePose { translation: at, ..BonePose::default() } }
    }

    /// A body on two legs with knees, two arms, a head and a tail.
    fn figure() -> (Skeleton, Vec<BoneRole>) {
        use BoneRole::{Arm, Body, Head, Leg, Tail};
        let bones = vec![
            bone("Body", None, Vec3::new(0.0, 0.75, 0.0)),
            bone("Head", Some(0), Vec3::new(0.0, 0.75, 0.0)),
            bone("RightArm", Some(0), Vec3::new(0.375, 0.6, 0.0)),
            bone("LeftArm", Some(0), Vec3::new(-0.375, 0.6, 0.0)),
            bone("RightLeg", Some(0), Vec3::new(0.125, 0.0, 0.0)),
            bone("RightLegLower", Some(4), Vec3::new(0.0, -0.375, 0.0)),
            bone("LeftLeg", Some(0), Vec3::new(-0.125, 0.0, 0.0)),
            bone("Tail", Some(0), Vec3::new(0.0, 0.1, -0.125)),
        ];
        (Skeleton { bones }, vec![Body, Head, Arm, Arm, Leg, Leg, Leg, Tail])
    }

    /// A body on four legs.
    fn beast() -> (Skeleton, Vec<BoneRole>) {
        use BoneRole::{Body, Leg};
        let bones = vec![
            bone("Body", None, Vec3::new(0.0, 0.5, 0.0)),
            bone("FrontRightLeg", Some(0), Vec3::new(0.2, 0.0, 0.4)),
            bone("FrontLeftLeg", Some(0), Vec3::new(-0.2, 0.0, 0.4)),
            bone("BackRightLeg", Some(0), Vec3::new(0.2, 0.0, -0.4)),
            bone("BackLeftLeg", Some(0), Vec3::new(-0.2, 0.0, -0.4)),
        ];
        (Skeleton { bones }, vec![Body, Leg, Leg, Leg, Leg])
    }

    /// How far a bone is turned from rest about X, in radians: forward for a leg is less than none.
    fn pitch(skeleton: &Skeleton, pose: &[BonePose], bone: usize) -> f32 {
        (skeleton.bones[bone].rest.rotation.inverse() * pose[bone].rotation).to_scaled_axis().x
    }

    fn walk(poser: &mut MobPoser, skeleton: &Skeleton, speed: f32, seconds: f32) -> Vec<BonePose> {
        let settings = MobMotion::default();
        let input = MotionInput { velocity: Vec3::Z * speed, delta: 1.0 / 60.0, ..MotionInput::default() };
        let mut pose = skeleton.rest();
        for _ in 0..(seconds * 60.0) as u32 {
            pose = poser.pose(skeleton, &settings, input);
        }
        pose
    }

    #[test]
    fn legs_and_their_lower_halves_arms_and_the_rest_are_found_by_their_roles() {
        let (skeleton, roles) = figure();
        let poser = MobPoser::new(&skeleton, &roles);
        assert_eq!((poser.body, poser.head, poser.tail), (Some(0), Some(1), Some(7)));
        assert_eq!(poser.legs(), 2, "a lower leg is no leg of its own");
        let right = poser.legs.iter().find(|leg| leg.bone == 4).unwrap();
        assert_eq!((right.lower, right.length), (Some(5), 0.75));
        let left = poser.legs.iter().find(|leg| leg.bone == 6).unwrap();
        assert_ne!(right.offset, left.offset, "one steps as the other swings back");
        assert_eq!(poser.arms.len(), 2);
        assert!(!MobPoser::new(&skeleton, &[]).moves_anything(), "a model that says nothing of its bones is left be");
    }

    #[test]
    fn two_legs_step_in_turn_and_the_arms_swing_against_them() {
        let (skeleton, roles) = figure();
        let mut poser = MobPoser::new(&skeleton, &roles);
        // Walking, until the right leg is as far forward as it gets.
        let settings = MobMotion::default();
        let input = MotionInput { velocity: Vec3::Z * 1.5, delta: 1.0 / 120.0, ..MotionInput::default() };
        let mut furthest: (f32, Vec<BonePose>) = (0.0, skeleton.rest());
        for _ in 0..240 {
            let pose = poser.pose(&skeleton, &settings, input);
            if pitch(&skeleton, &pose, 4) < furthest.0 {
                furthest = (pitch(&skeleton, &pose, 4), pose);
            }
        }
        let (forward, pose) = furthest;
        assert!(forward < -0.4, "the right leg swings well forward: {forward}");
        assert!(pitch(&skeleton, &pose, 6) > 0.3, "as the left is back");
        assert!(pitch(&skeleton, &pose, 2) > 0.2, "the right arm back with it");
        assert!(pitch(&skeleton, &pose, 3) < -0.2, "and the left arm forward");
        assert!(
            pose[0].translation.y <= skeleton.bones[0].rest.translation.y + 1e-4,
            "the body never bobs above standing"
        );
    }

    #[test]
    fn a_step_covers_the_same_ground_at_any_speed_and_a_lower_leg_bends_only_back() {
        let (skeleton, roles) = figure();
        // A stride is 1.3 of a 0.75 m leg: a whole turn of the walk is two of them.
        let turn_metres = 2.0 * 1.3 * 0.75;
        for speed in [0.8, 2.4] {
            let mut poser = MobPoser::new(&skeleton, &roles);
            walk(&mut poser, &skeleton, speed, turn_metres * 0.5 / speed);
            assert!((poser.walk_phase() - 0.5).abs() < 0.02, "half a turn at {speed} m/s: {}", poser.walk_phase());
        }
        let mut poser = MobPoser::new(&skeleton, &roles);
        let settings = MobMotion::default();
        let input = MotionInput { velocity: Vec3::Z * 1.5, delta: 1.0 / 60.0, ..MotionInput::default() };
        let mut most = 0.0f32;
        for _ in 0..180 {
            let pose = poser.pose(&skeleton, &settings, input);
            assert!(pitch(&skeleton, &pose, 5) >= -1e-4, "a knee does not bend forward");
            most = most.max(pitch(&skeleton, &pose, 5));
        }
        assert!(most > 0.3, "and does bend: {most}");
    }

    #[test]
    fn four_legs_step_as_diagonal_pairs() {
        let (skeleton, roles) = beast();
        let mut poser = MobPoser::new(&skeleton, &roles);
        assert_eq!(poser.legs(), 4);
        let pose = walk(&mut poser, &skeleton, 1.2, 0.37);
        let swing = |bone: usize| pitch(&skeleton, &pose, bone);
        assert!((swing(1) - swing(4)).abs() < 1e-4, "front right with back left");
        assert!((swing(2) - swing(3)).abs() < 1e-4, "front left with back right");
        assert!((swing(1) + swing(2)).abs() < 1e-4 && swing(1).abs() > 0.05, "one pair against the other");
    }

    #[test]
    fn standing_still_it_breathes_and_its_legs_are_at_rest() {
        let (skeleton, roles) = figure();
        let mut poser = MobPoser::new(&skeleton, &roles);
        let mut heights = Vec::new();
        for _ in 0..240 {
            let pose = poser.pose(
                &skeleton,
                &MobMotion::default(),
                MotionInput { delta: 1.0 / 60.0, ..MotionInput::default() },
            );
            assert!(pitch(&skeleton, &pose, 4).abs() < 1e-4);
            heights.push(pose[0].translation.y);
        }
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(low, high), y| (low.min(*y), high.max(*y)));
        assert!(high - low > 0.005 && high - low < 0.05, "a little up and down: {low} to {high}");
        // With no idle, nothing moves at all.
        let mut poser = MobPoser::new(&skeleton, &roles);
        let still = MobMotion { idle: 0.0, ..MobMotion::default() };
        let pose = poser.pose(&skeleton, &still, MotionInput { delta: 0.5, ..MotionInput::default() });
        assert_eq!(pose, skeleton.rest());
    }

    #[test]
    fn the_head_turns_to_what_it_looks_at_by_degrees_and_no_further_than_its_limit() {
        let (skeleton, roles) = figure();
        let mut poser = MobPoser::new(&skeleton, &roles);
        let settings = MobMotion { idle: 0.0, ..MobMotion::default() };
        let yaw = |pose: &[BonePose]| pose[1].rotation.to_scaled_axis().y;
        // To its right and a little ahead: +X is its right, and turning to it is turning about Y the positive way.
        let aside = MotionInput { look: Some(Vec3::new(1.0, 0.0, 1.0)), delta: 1.0 / 60.0, ..MotionInput::default() };
        let first = poser.pose(&skeleton, &settings, aside);
        assert!(yaw(&first) > 0.0 && yaw(&first) < 0.3, "not all at once: {}", yaw(&first));
        let mut pose = first;
        (0..120).for_each(|_| pose = poser.pose(&skeleton, &settings, aside));
        assert!((yaw(&pose) - PI / 4.0).abs() < 0.02, "an eighth of a turn: {}", yaw(&pose));
        // Behind it: as far as its neck goes.
        let behind = MotionInput { look: Some(Vec3::new(0.3, 0.0, -1.0)), ..aside };
        (0..180).for_each(|_| pose = poser.pose(&skeleton, &settings, behind));
        assert!((yaw(&pose) - 70f32.to_radians()).abs() < 0.02, "{}", yaw(&pose));
        // Told nothing, it looks ahead again.
        (0..180).for_each(|_| pose = poser.pose(&skeleton, &settings, MotionInput { look: None, ..aside }));
        assert!(yaw(&pose).abs() < 0.01);
    }

    #[test]
    fn a_tail_trails_a_turn_and_an_attack_and_a_flinch_come_and_go() {
        let (skeleton, roles) = figure();
        let mut poser = MobPoser::new(&skeleton, &roles);
        let settings = MobMotion { idle: 0.0, ..MobMotion::default() };
        let frame = MotionInput { delta: 1.0 / 60.0, ..MotionInput::default() };
        let mut pose = skeleton.rest();
        (0..30).for_each(|_| pose = poser.pose(&skeleton, &settings, MotionInput { turn: 3.0, ..frame }));
        let trailed = pose[7].rotation.to_scaled_axis().y;
        assert!(trailed < -0.1, "turning one way, the tail swings the other: {trailed}");
        (0..240).for_each(|_| pose = poser.pose(&skeleton, &settings, frame));
        assert!(pose[7].rotation.to_scaled_axis().y.abs() < 0.01, "and settles");

        poser.attack();
        let mut reach = 0.0f32;
        for _ in 0..12 {
            pose = poser.pose(&skeleton, &settings, frame);
            reach = reach.min(pitch(&skeleton, &pose, 2));
        }
        assert!(reach < -1.2, "the right arm swings up and over: {reach}");
        (0..40).for_each(|_| pose = poser.pose(&skeleton, &settings, frame));
        assert!(pitch(&skeleton, &pose, 2).abs() < 1e-3, "and is back");

        poser.hurt();
        pose = poser.pose(&skeleton, &settings, frame);
        assert!(pitch(&skeleton, &pose, 0) < -0.2, "knocked back");
        (0..40).for_each(|_| pose = poser.pose(&skeleton, &settings, frame));
        assert_eq!((pose[0], pose[1]), (skeleton.bones[0].rest, skeleton.bones[1].rest), "and standing again");

        // A beast with no arms lunges with its whole body.
        let (skeleton, roles) = beast();
        let mut poser = MobPoser::new(&skeleton, &roles);
        poser.attack();
        (0..10).for_each(|_| pose = poser.pose(&skeleton, &settings, frame));
        assert!(pose[0].translation.z > 0.05);
    }
}
