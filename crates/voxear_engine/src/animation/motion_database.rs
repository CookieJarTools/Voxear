//! Motion databases: every frame of a model's clips described by a few
//! numbers, for motion matching to search. Each frame says where the feet
//! are and how they move, how the hips move, and where the character will
//! be, and which way it will face, a third of a second, two thirds and a
//! second on: all as seen from where it stands at that frame, the place on
//! the ground its root bone marks.

use std::sync::{Arc, Weak};

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{CLIP_RATE, Clip, ModelAnimation, Skeleton};

/// How many numbers describe a frame.
pub const FEATURES: usize = 27;
/// How far ahead, in frames, the path is described.
pub const AHEAD: [usize; 3] = [20, 40, 60];
/// Frames a box of the search's holds.
const BOX: usize = 16;
/// The last frames of a clip that does not loop a search never lands on:
/// there would be nothing left to play.
const ENDING: usize = 20;

/// How much each part of a frame's description counts in a search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionWeights {
    pub foot_position: f32,
    pub foot_velocity: f32,
    pub hip_velocity: f32,
    pub path_position: f32,
    pub path_facing: f32,
}

impl Default for MotionWeights {
    fn default() -> Self {
        Self { foot_position: 0.75, foot_velocity: 1.0, hip_velocity: 1.0, path_position: 1.0, path_facing: 1.5 }
    }
}

impl MotionWeights {
    /// Each run of the features, and its weight.
    fn groups(&self) -> [(std::ops::Range<usize>, f32); 5] {
        [
            (0..6, self.foot_position),
            (6..12, self.foot_velocity),
            (12..15, self.hip_velocity),
            (15..21, self.path_position),
            (21..27, self.path_facing),
        ]
    }

    fn key(&self) -> [u32; 5] {
        [self.foot_position, self.foot_velocity, self.hip_velocity, self.path_position, self.path_facing]
            .map(f32::to_bits)
    }
}

/// A place on the ground and which way it faces, as a turn about Y from +Z.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ground {
    pub position: Vec3,
    pub yaw: f32,
}

impl Ground {
    pub fn new(position: Vec3, yaw: f32) -> Self {
        Self { position: Vec3::new(position.x, 0.0, position.z), yaw }
    }

    /// Where a root bone's matrix stands and faces, its forward being
    /// +Z turned by `rest`, the root's turn at rest.
    pub fn of(matrix: Mat4, rest: Quat) -> Self {
        let (_, rotation, position) = matrix.to_scale_rotation_translation();
        Self::new(position, facing(rotation * rest.inverse() * Vec3::Z))
    }

    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation(), self.position)
    }

    /// `point` as seen from here.
    pub fn local(&self, point: Vec3) -> Vec3 {
        self.rotation().inverse() * (point - self.position)
    }

    /// `other` as seen from here.
    pub fn to(&self, other: Ground) -> Ground {
        Ground { position: self.local(other.position), yaw: wrap(other.yaw - self.yaw) }
    }

    /// `step`, seen from here, as a place in the world.
    pub fn then(&self, step: Ground) -> Ground {
        Ground { position: self.position + self.rotation() * step.position, yaw: wrap(self.yaw + step.yaw) }
    }

    /// `t` of the way to `other`, turning the shorter way.
    pub fn lerp(&self, other: Ground, t: f32) -> Ground {
        Ground { position: self.position.lerp(other.position, t), yaw: wrap(self.yaw + wrap(other.yaw - self.yaw) * t) }
    }
}

/// The yaw a direction faces, from +Z towards +X.
pub fn facing(direction: Vec3) -> f32 {
    direction.x.atan2(direction.z)
}

/// An angle brought into -π..π.
pub fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The bones motion matching follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionBones {
    pub root: usize,
    pub hips: usize,
    pub left_foot: usize,
    pub right_foot: usize,
}

impl MotionBones {
    /// Found by their names as most rigs give them: "root" (or the first
    /// bone), the hips or pelvis, and each foot or ankle.
    pub fn find(skeleton: &Skeleton) -> Option<Self> {
        let names: Vec<String> = skeleton.bones.iter().map(|bone| bone.name.to_lowercase()).collect();
        let named = |part: &str, side: Option<bool>| {
            names.iter().position(|name| name.contains(part) && side.is_none_or(|left| side_of(name) == Some(left)))
        };
        let root = names.iter().position(|name| name == "root").or((!names.is_empty()).then_some(0))?;
        let hips = named("hips", None).or_else(|| named("pelvis", None))?;
        let foot = |left| named("foot", Some(left)).or_else(|| named("ankle", Some(left)));
        Some(Self { root, hips, left_foot: foot(true)?, right_foot: foot(false)? })
    }
}

/// Which side a bone's lowercase name puts it on: true for left.
fn side_of(name: &str) -> Option<bool> {
    let marked = |side: &str, letter: char| {
        name.contains(side)
            || [".", "_", " ", "-"].iter().any(|mark| name.ends_with(&format!("{mark}{letter}")))
            || [".", "_", " ", "-"].iter().any(|mark| name.starts_with(&format!("{letter}{mark}")))
    };
    match (marked("left", 'l'), marked("right", 'r')) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// One clip's frames among the database's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRun {
    /// The model's clip.
    pub clip: usize,
    pub start: usize,
    pub frames: usize,
    /// Its last frame is its first again, so it plays round.
    pub looping: bool,
}

impl ClipRun {
    pub fn duration(&self) -> f32 {
        (self.frames - 1) as f32 / CLIP_RATE
    }
}

/// Every frame of a model's clips, described for searching.
pub struct MotionDatabase {
    pub bones: MotionBones,
    /// The root's turn at rest, in the model's space.
    pub rest_root: Quat,
    pub runs: Vec<ClipRun>,
    /// Each frame's place on the ground, in its clip's model space.
    grounds: Vec<Ground>,
    /// Each frame's features, `FEATURES` each, made comparable and weighed.
    features: Vec<f32>,
    /// What turns a raw feature into a weighed one: less `offset`, times `scale`.
    offset: [f32; FEATURES],
    scale: [f32; FEATURES],
    /// Whether a search may land on each frame.
    landable: Vec<bool>,
    /// The least and most of each feature over each `BOX` frames.
    boxes: Vec<([f32; FEATURES], [f32; FEATURES])>,
}

impl MotionDatabase {
    /// Describes every clip of `animation` of two frames or more; `None`
    /// when its skeleton has no hips or feet to follow.
    pub fn build(animation: &ModelAnimation, weights: &MotionWeights) -> Option<Self> {
        let bones = MotionBones::find(&animation.skeleton)?;
        let rest = animation.skeleton.rest();
        let rest_root = animation.skeleton.model_matrices(&rest)[bones.root].to_scale_rotation_translation().1;
        let mut database = Self {
            bones,
            rest_root,
            runs: Vec::new(),
            grounds: Vec::new(),
            features: Vec::new(),
            offset: [0.0; FEATURES],
            scale: [1.0; FEATURES],
            landable: Vec::new(),
            boxes: Vec::new(),
        };
        let mut raw = Vec::new();
        let mut places = Vec::new();
        for (index, clip) in animation.clips.iter().enumerate().filter(|(_, clip)| clip.frames >= 2) {
            let mut run = ClipRun { clip: index, start: database.grounds.len(), frames: clip.frames, looping: false };
            let mut feet = Vec::with_capacity(clip.frames);
            for frame in 0..clip.frames {
                let matrices = animation.skeleton.model_matrices(clip.frame(frame));
                let at = |bone: usize| matrices[bone].w_axis.truncate();
                database.grounds.push(Ground::of(matrices[bones.root], rest_root));
                feet.push([at(bones.left_foot), at(bones.right_foot), at(bones.hips)]);
            }
            run.looping = loops(clip, bones.root) && database.steps_alike(&run);
            places.push(feet);
            database.runs.push(run);
        }
        for (run, feet) in database.runs.clone().iter().zip(&places) {
            for frame in 0..run.frames {
                raw.push(database.describe(run, feet, frame));
                database.landable.push(run.looping || frame + ENDING < run.frames);
            }
        }
        database.weigh(&raw, weights);
        Some(database)
    }

    /// A frame's raw features: feet, their speed, the hips' speed, and the
    /// path ahead, as seen from where the frame stands.
    fn describe(&self, run: &ClipRun, places: &[[Vec3; 3]], frame: usize) -> [f32; FEATURES] {
        let ground = self.grounds[run.start + frame];
        let (before, after) = (frame.saturating_sub(1), (frame + 1).min(run.frames - 1));
        let speed = |part: usize| {
            let moved = places[after][part] - places[before][part];
            ground.rotation().inverse() * moved * (CLIP_RATE / (after - before) as f32)
        };
        let mut out = [0.0; FEATURES];
        let parts = [ground.local(places[frame][0]), ground.local(places[frame][1]), speed(0), speed(1), speed(2)];
        for (k, part) in parts.iter().enumerate() {
            out[k * 3..k * 3 + 3].copy_from_slice(&part.to_array());
        }
        for (k, &ahead) in AHEAD.iter().enumerate() {
            let there = ground.to(self.ground_at(run, (frame + ahead) as f32));
            out[15 + k * 2] = there.position.x;
            out[16 + k * 2] = there.position.z;
            out[21 + k * 2] = there.yaw.sin();
            out[22 + k * 2] = there.yaw.cos();
        }
        out
    }

    /// Makes each run of features comparable, by its spread over every
    /// frame, and weighs it.
    fn weigh(&mut self, raw: &[[f32; FEATURES]], weights: &MotionWeights) {
        let count = raw.len().max(1) as f32;
        for d in 0..FEATURES {
            self.offset[d] = raw.iter().map(|f| f[d]).sum::<f32>() / count;
        }
        let spread: Vec<f32> = (0..FEATURES)
            .map(|d| (raw.iter().map(|f| (f[d] - self.offset[d]).powi(2)).sum::<f32>() / count).sqrt())
            .collect();
        for (range, weight) in weights.groups() {
            let group = (spread[range.clone()].iter().sum::<f32>() / range.len() as f32).max(1e-4);
            for d in range {
                self.scale[d] = weight / group;
            }
        }
        self.features = raw.iter().flat_map(|f| self.normalise(f)).collect();
        self.boxes = self
            .features
            .chunks(FEATURES * BOX)
            .map(|chunk| {
                let (mut low, mut high) = ([f32::MAX; FEATURES], [f32::MIN; FEATURES]);
                for frame in chunk.chunks(FEATURES) {
                    for d in 0..FEATURES {
                        low[d] = low[d].min(frame[d]);
                        high[d] = high[d].max(frame[d]);
                    }
                }
                (low, high)
            })
            .collect();
    }

    /// Raw features made comparable and weighed, as the database's are.
    pub fn normalise(&self, raw: &[f32; FEATURES]) -> [f32; FEATURES] {
        std::array::from_fn(|d| (raw[d] - self.offset[d]) * self.scale[d])
    }

    /// A frame's raw features again.
    pub fn raw(&self, frame: usize) -> [f32; FEATURES] {
        let f = self.features(frame);
        std::array::from_fn(|d| f[d] / self.scale[d] + self.offset[d])
    }

    pub fn features(&self, frame: usize) -> &[f32] {
        &self.features[frame * FEATURES..(frame + 1) * FEATURES]
    }

    pub fn frames(&self) -> usize {
        self.grounds.len()
    }

    /// The run a frame is in.
    pub fn run_of(&self, frame: usize) -> usize {
        self.runs.partition_point(|run| run.start + run.frames <= frame)
    }

    pub fn landable(&self, frame: usize) -> bool {
        self.landable.get(frame).copied().unwrap_or(false)
    }

    /// The frame nearest `query`, and how far it is, or `best` if none is
    /// nearer than it.
    pub fn search(&self, query: &[f32; FEATURES], mut best: (usize, f32)) -> (usize, f32) {
        for (b, (low, high)) in self.boxes.iter().enumerate() {
            let mut bound = 0.0;
            for d in 0..FEATURES {
                let outside = (low[d] - query[d]).max(query[d] - high[d]).max(0.0);
                bound += outside * outside;
            }
            if bound >= best.1 {
                continue;
            }
            for frame in b * BOX..((b + 1) * BOX).min(self.frames()) {
                if !self.landable[frame] {
                    continue;
                }
                let cost = distance(self.features(frame), query, best.1);
                if cost < best.1 {
                    best = (frame, cost);
                }
            }
        }
        best
    }

    /// Where `run`'s root stands `frame` frames in, between frames, going
    /// on round a looping clip past its end, held at the end otherwise.
    pub fn ground_at(&self, run: &ClipRun, frame: f32) -> Ground {
        let whole = frame.max(0.0).floor();
        let (a, b) = (self.ground_whole(run, whole as usize), self.ground_whole(run, whole as usize + 1));
        a.lerp(b, frame.max(0.0) - whole)
    }

    fn ground_whole(&self, run: &ClipRun, frame: usize) -> Ground {
        let last = run.frames - 1;
        let at = |f: usize| self.grounds[run.start + f];
        if frame <= last || !run.looping || last == 0 {
            return at(frame.min(last));
        }
        // Round the loop: each lap carries on from where the last ended.
        let lap = at(0).to(at(last));
        let mut place = at(0);
        for _ in 0..frame / last {
            place = place.then(lap);
        }
        place.then(at(0).to(at(frame % last)))
    }

    /// How `run`'s root moves from `from` to `to` seconds in, as seen from
    /// where it stood at `from`.
    pub fn travel(&self, run: &ClipRun, from: f32, to: f32) -> Ground {
        self.ground_at(run, from * CLIP_RATE).to(self.ground_at(run, to * CLIP_RATE))
    }
}

impl MotionDatabase {
    /// Whether `run`'s root moves the same over its first frame as over its
    /// last, as a loop's does.
    fn steps_alike(&self, run: &ClipRun) -> bool {
        let at = |f: usize| self.grounds[run.start + f];
        let (first, last) = (at(0).to(at(1)), at(run.frames - 2).to(at(run.frames - 1)));
        first.position.distance(last.position) < 0.005 && (first.yaw - last.yaw).abs() < 0.005
    }
}

/// Whether a clip ends as it starts, every bone but its root within a few
/// degrees and centimetres.
fn loops(clip: &Clip, root: usize) -> bool {
    let (first, last) = (clip.frame(0), clip.frame(clip.frames - 1));
    first
        .iter()
        .zip(last)
        .enumerate()
        .filter(|(bone, _)| *bone != root)
        .all(|(_, (a, b))| a.rotation.angle_between(b.rotation) < 0.05 && a.translation.distance(b.translation) < 0.02)
}

/// The squared distance between two frames' features, given up once past
/// `limit`.
fn distance(a: &[f32], b: &[f32; FEATURES], limit: f32) -> f32 {
    let mut sum = 0.0;
    for d in 0..FEATURES {
        let gap = a[d] - b[d];
        sum += gap * gap;
        if sum >= limit {
            return sum;
        }
    }
    sum
}

/// The databases built, each shared by every Obj whose model and weights
/// are the same; none for a model with nothing to follow.
#[derive(Default)]
struct Databases {
    built: Vec<(Weak<ModelAnimation>, [u32; 5], Option<Arc<MotionDatabase>>)>,
}

/// The database for a model's clips and weights, built the first time it is
/// asked for.
pub fn database_for(
    world: &mut World,
    animation: &Arc<ModelAnimation>,
    weights: &MotionWeights,
) -> Option<Arc<MotionDatabase>> {
    if world.resource::<Databases>().is_none() {
        world.insert_resource(Databases::default());
    }
    let databases = world.resource_mut::<Databases>()?;
    databases.built.retain(|(model, _, _)| model.strong_count() > 0);
    let key = weights.key();
    let found =
        databases.built.iter().find(|(model, k, _)| *k == key && Weak::ptr_eq(model, &Arc::downgrade(animation)));
    if let Some((_, _, database)) = found {
        return database.clone();
    }
    let database = MotionDatabase::build(animation, weights).map(Arc::new);
    databases.built.push((Arc::downgrade(animation), key, database.clone()));
    database
}
