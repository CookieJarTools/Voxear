//! Motion databases: every frame of a model's looping clips described by
//! how its root moves along the ground there, as seen from where it stands
//! and faces: its velocity, its direction, and where it will be a third of
//! a second, two thirds and a second on. A Motion Matcher searches them for
//! the clips that move as the character is wanted to.
//!
//! A clip that stays in place, as many walking loops are made, moves as
//! fast as its planted foot slides back, and the other way.

use std::sync::{Arc, Weak};

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{CLIP_RATE, Clip, ModelAnimation, Skeleton};

/// How many numbers describe a frame.
pub const FEATURES: usize = 10;
/// How far ahead, in frames, the path is described.
pub const AHEAD: [usize; 3] = [20, 40, 60];
/// How much more speed, and the path it makes, counts than direction: it
/// is what tells a walk from a run.
const SPEED_WEIGHT: f32 = 3.0;
/// Below this many metres a second a clip stands still.
pub const STILL: f32 = 0.1;

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

/// The root bone: the one named "root", or else the first.
fn root_bone(skeleton: &Skeleton) -> Option<usize> {
    let bones = &skeleton.bones;
    bones.iter().position(|bone| bone.name.eq_ignore_ascii_case("root")).or((!bones.is_empty()).then_some(0))
}

/// Each foot, found by its name as most rigs give it: its toes, which stay
/// down longest, or else the foot or ankle, marked left or right.
fn feet(skeleton: &Skeleton) -> Option<[usize; 2]> {
    let names: Vec<String> = skeleton.bones.iter().map(|bone| bone.name.to_lowercase()).collect();
    let foot = |left: bool| {
        let named = |part: &str| names.iter().position(|name| name.contains(part) && side_of(name) == Some(left));
        named("toe").or_else(|| named("foot")).or_else(|| named("ankle"))
    };
    Some([foot(true)?, foot(false)?])
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

/// One clip's frames among the database's, and how it moves overall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRun {
    /// The model's clip.
    pub clip: usize,
    pub start: usize,
    pub frames: usize,
    /// Its last frame is its first again, so it plays round.
    pub looping: bool,
    /// How it moves along the ground, in metres a second, as seen from
    /// where it starts.
    pub velocity: Vec3,
    /// It moves at much the same speed all the way through, as walking
    /// does, rather than lunging or stopping.
    pub steady: bool,
}

impl ClipRun {
    pub fn duration(&self) -> f32 {
        (self.frames - 1) as f32 / CLIP_RATE
    }

    /// A loop that moves steadily: one a Motion Matcher may walk with.
    pub fn moves(&self) -> bool {
        self.looping && self.steady && self.velocity.length() >= STILL
    }

    /// A loop that stands still: one it may stand with.
    pub fn stands(&self) -> bool {
        self.looping && self.velocity.length() < STILL
    }
}

/// Every frame of a model's clips, described for searching.
pub struct MotionDatabase {
    pub root: usize,
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
}

impl MotionDatabase {
    /// Describes every clip of `animation` of two frames or more; `None`
    /// for a model with no bones.
    pub fn build(animation: &ModelAnimation) -> Option<Self> {
        let skeleton = &animation.skeleton;
        let root = root_bone(skeleton)?;
        let rest_root = skeleton.model_matrices(&skeleton.rest())[root].to_scale_rotation_translation().1;
        let feet = feet(skeleton);
        let mut database = Self {
            root,
            rest_root,
            runs: Vec::new(),
            grounds: Vec::new(),
            features: Vec::new(),
            offset: [0.0; FEATURES],
            scale: [1.0; FEATURES],
        };
        for (index, clip) in animation.clips.iter().enumerate().filter(|(_, clip)| clip.frames >= 2) {
            let start = database.grounds.len();
            let mut planted = Vec::with_capacity(clip.frames);
            for frame in 0..clip.frames {
                let matrices = skeleton.model_matrices(clip.frame(frame));
                let ground = Ground::of(matrices[root], rest_root);
                database.grounds.push(ground);
                planted.push(feet.map(|[l, r]| [l, r].map(|foot| ground.local(matrices[foot].w_axis.truncate()))));
            }
            let mut run =
                ClipRun { clip: index, start, frames: clip.frames, looping: false, velocity: Vec3::ZERO, steady: true };
            let travelled = database.grounds[start].to(database.grounds[start + clip.frames - 1]).position;
            run.velocity = travelled / run.duration().max(1e-3);
            // A clip made to stay in place moves as its planted foot slides back.
            if run.velocity.length() < STILL
                && let Some(slide) = slide(&planted)
            {
                run.velocity = -slide;
                let first = database.grounds[start];
                for frame in 0..clip.frames {
                    let carried = Ground { position: run.velocity * (frame as f32 / CLIP_RATE), yaw: 0.0 };
                    database.grounds[start + frame] = first.then(carried);
                }
            }
            run.looping = loops(clip, root) && database.steps_alike(&run);
            run.steady = database.steady(&run);
            database.runs.push(run);
        }
        let raw: Vec<[f32; FEATURES]> = database
            .runs
            .iter()
            .flat_map(|run| (0..run.frames).map(move |frame| (run, frame)))
            .map(|(run, frame)| database.describe(run, frame))
            .collect();
        database.weigh(&raw);
        Some(database)
    }

    /// A frame's raw features: its velocity, its direction and the path
    /// ahead, as seen from where it stands.
    fn describe(&self, run: &ClipRun, frame: usize) -> [f32; FEATURES] {
        let ground = self.grounds[run.start + frame];
        let soon = ground.to(self.ground_at(run, (frame + 6) as f32)).position * (CLIP_RATE / 6.0);
        let path = AHEAD.map(|ahead| ground.to(self.ground_at(run, (frame + ahead) as f32)).position);
        describe(soon, path)
    }

    /// Makes the features comparable, by their spread over the frames that
    /// move, each direction's run alike, and weighs speed above direction.
    fn weigh(&mut self, raw: &[[f32; FEATURES]]) {
        let moving: Vec<&[f32; FEATURES]> = self
            .runs
            .iter()
            .filter(|run| run.moves())
            .flat_map(|run| raw[run.start..run.start + run.frames].iter())
            .collect();
        let count = moving.len().max(1) as f32;
        for d in 0..FEATURES {
            self.offset[d] = moving.iter().map(|f| f[d]).sum::<f32>() / count;
        }
        let spread: Vec<f32> = (0..FEATURES)
            .map(|d| (moving.iter().map(|f| (f[d] - self.offset[d]).powi(2)).sum::<f32>() / count).sqrt())
            .collect();
        let groups =
            [(0..2, SPEED_WEIGHT), (2..4, 1.0), (4..6, SPEED_WEIGHT), (6..8, SPEED_WEIGHT), (8..10, SPEED_WEIGHT)];
        for (range, weight) in groups {
            let group = spread[range.clone()].iter().copied().fold(0.0f32, f32::max);
            for d in range {
                self.scale[d] = if group > 1e-6 { weight / group } else { weight };
            }
        }
        self.features = raw.iter().flat_map(|f| self.normalise(f)).collect();
    }

    /// Raw features made comparable and weighed, as the database's are.
    pub fn normalise(&self, raw: &[f32; FEATURES]) -> [f32; FEATURES] {
        std::array::from_fn(|d| (raw[d] - self.offset[d]) * self.scale[d])
    }

    pub fn features(&self, frame: usize) -> &[f32] {
        &self.features[frame * FEATURES..(frame + 1) * FEATURES]
    }

    /// The features of moving at `velocity`, as seen from the body, and on
    /// at it.
    pub fn query(&self, velocity: Vec3) -> [f32; FEATURES] {
        let path = AHEAD.map(|ahead| velocity * (ahead as f32 / CLIP_RATE));
        self.normalise(&describe(velocity, path))
    }

    /// The best frame of each clip that moves, and how far it is from
    /// `query`, nearest first.
    pub fn search(&self, query: &[f32; FEATURES]) -> Vec<(usize, f32)> {
        let mut best: Vec<(usize, f32)> = self
            .runs
            .iter()
            .enumerate()
            .filter(|(_, run)| run.moves())
            .map(|(index, run)| {
                let cost = (run.start..run.start + run.frames)
                    .map(|frame| distance(self.features(frame), query))
                    .fold(f32::MAX, f32::min);
                (index, cost)
            })
            .collect();
        best.sort_by(|a, b| a.1.total_cmp(&b.1));
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

    /// Whether `run`'s root never slows below half its speed overall, a
    /// tenth of a second at a time.
    fn steady(&self, run: &ClipRun) -> bool {
        let speed = run.velocity.length();
        (0..run.frames.saturating_sub(6)).step_by(6).all(|frame| {
            let at = |f: usize| self.grounds[run.start + f];
            at(frame).to(at(frame + 6)).position.length() * (CLIP_RATE / 6.0) >= speed * 0.5
        })
    }

    /// Whether `run`'s root moves the same over its first frame as over its
    /// last, as a loop's does.
    fn steps_alike(&self, run: &ClipRun) -> bool {
        let at = |f: usize| self.grounds[run.start + f];
        let (first, last) = (at(0).to(at(1)), at(run.frames - 2).to(at(run.frames - 1)));
        first.position.distance(last.position) < 0.005 && (first.yaw - last.yaw).abs() < 0.005
    }
}

/// Raw features from a velocity and the path ahead.
fn describe(velocity: Vec3, path: [Vec3; 3]) -> [f32; FEATURES] {
    let direction = velocity.normalize_or_zero();
    let mut out = [velocity.x, velocity.z, direction.x, direction.z, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    for (k, step) in path.iter().enumerate() {
        out[4 + k * 2] = step.x;
        out[5 + k * 2] = step.z;
    }
    out
}

/// How fast, and which way, the planted foot slides along the ground, in
/// metres a second, seen from the root: none for feet that stay put, or
/// never come down to the ground, as a swimmer's.
fn slide(planted: &[Option<[Vec3; 2]>]) -> Option<Vec3> {
    let feet: Vec<[Vec3; 2]> = planted.iter().copied().collect::<Option<_>>()?;
    let low = feet.iter().map(|[l, r]| l.y.min(r.y)).fold(f32::MAX, f32::min);
    if low > 0.2 {
        return None;
    }
    let down = |f: &[Vec3; 2]| usize::from(f[0].y >= f[1].y);
    let mut slides: Vec<Vec3> = feet
        .windows(2)
        .filter_map(|pair| {
            let foot = down(&pair[0]);
            let held = pair[0][foot].y < low + 0.02 && down(&pair[1]) == foot;
            held.then(|| (pair[1][foot] - pair[0][foot]) * CLIP_RATE)
        })
        .collect();
    if slides.len() < 3 {
        return None;
    }
    let middle = slides.len() / 2;
    let mut median = |pick: fn(&Vec3) -> f32| {
        slides.sort_by(|a, b| pick(a).total_cmp(&pick(b)));
        pick(&slides[middle])
    };
    let slid = Vec3::new(median(|v| v.x), 0.0, median(|v| v.z));
    (slid.length() >= STILL).then_some(slid)
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

/// The squared distance between two frames' features.
fn distance(a: &[f32], b: &[f32; FEATURES]) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
}

/// The databases built, each shared by every Obj whose model is the same;
/// none for a model with nothing to follow.
#[derive(Default)]
struct Databases {
    built: Vec<(Weak<ModelAnimation>, Option<Arc<MotionDatabase>>)>,
}

/// The database for a model's clips, built the first time it is asked for.
pub fn database_for(world: &mut World, animation: &Arc<ModelAnimation>) -> Option<Arc<MotionDatabase>> {
    if world.resource::<Databases>().is_none() {
        world.insert_resource(Databases::default());
    }
    let databases = world.resource_mut::<Databases>()?;
    databases.built.retain(|(model, _)| model.strong_count() > 0);
    let found = databases.built.iter().find(|(model, _)| Weak::ptr_eq(model, &Arc::downgrade(animation)));
    if let Some((_, database)) = found {
        return database.clone();
    }
    let database = MotionDatabase::build(animation).map(Arc::new);
    databases.built.push((Arc::downgrade(animation), database.clone()));
    database
}
