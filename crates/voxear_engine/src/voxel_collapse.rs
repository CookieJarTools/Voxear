//! What was built in endless Voxel Volumes coming loose and giving way
//! while the game runs.
//!
//! A blast ([`blast`]) digs out what was built in a ball at once, dust
//! flying where it was. Then, on a thread of its own, the volume works out
//! what that left cut off from what holds (`voxear_render::voxel_settle`)
//! and what gives way under its own weight after: what was cut off comes
//! loose as soon as that is known, thrown away from the blast; what gives
//! way after comes loose a round at a time, a moment apart, so a house
//! with its corner blown out sags and falls in. Digging and carving from
//! scripts settles the volume the same way ([`settle_after`]).
//!
//! Each piece is a body of its own as a felled tree is
//! (`voxel_felling`), its voxels a Voxel Volume turned with it. Pieces that
//! hit the volume hard dig out what they hit, which settles in turn, and
//! pieces that hit each other chip; pieces too small to be worth a body,
//! and the smallest past the volume's Most Pieces falling at once, fall to
//! dust. At rest a piece is stamped back into the volume where it lies.

use std::{collections::VecDeque, sync::Arc, thread::JoinHandle};

use voxear_core::math::{IVec3, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{VoxelBlast, VoxelPiece, VoxelRound, VoxelSettled, VoxelStrainMemory, VoxelVolume};
use voxear_scene::Transform;

use crate::{
    physics::{Acoustic, PhysicsEvent, PhysicsScene},
    voxel_felling::{CutPieces, FelledPiece, PieceFall, PieceMade, PiecePush, PieceRest, VoxelPieceBody},
};

/// How long apart the rounds of what gives way come loose, in seconds.
const ROUND_GAP: f32 = 0.22;
/// Pieces with fewer voxels than this fall to dust: chips not worth a body.
pub const DEBRIS: usize = 320;
/// How much slower a piece must suddenly go, in metres a second, and how
/// much it must lose of its moving, in joules, for it to have hit
/// something hard enough to break it; and how many joules dig out a cubic
/// metre of what it hits.
const IMPACT_SPEED: f32 = 3.5;
const IMPACT_ENERGY: f32 = 1500.0;
const CRUSH: f32 = 60_000.0;
/// How soon after one hit a piece's next can dig, in seconds.
const IMPACT_GAP: f32 = 0.25;
/// The most voxels a dig or carve is looked over for what it dug out.
const MOST_LOOKED_OVER: f32 = 8.0e6;
/// The most dust clouds one change raises.
const MOST_DUST: usize = 5;

/// How a blast is made: whether what comes loose falls or is taken away,
/// how fast what it cuts off is thrown away from it at once, in metres a
/// second, what becomes of the pieces at rest (none for stamped back), and
/// whether to settle at once, not on a thread of its own: what came loose
/// at once is then in what it did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlastOptions {
    pub pieces: CutPieces,
    pub throw: f32,
    pub rest: Option<PieceRest>,
    pub wait: bool,
}

impl Default for BlastOptions {
    fn default() -> Self {
        Self { pieces: CutPieces::Fall, throw: 3.0, rest: None, wait: false }
    }
}

/// What a blast did: how many volumes it reached and how many built voxels
/// it dug out; and, where it waited, the pieces that came loose at once
/// and how many joins gave way after.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlastDone {
    pub reached: usize,
    pub dug: usize,
    pub pieces: Vec<FelledPiece>,
    pub broken: usize,
    pub rounds: usize,
}

/// A change to settle: the volume before and after it, the voxels it dug
/// out, where in the world what it cut off is thrown from and how fast,
/// and what becomes of the pieces.
#[derive(Clone)]
struct Job {
    before: VoxelVolume,
    dug: Vec<IVec3>,
    from: Vec3,
    throw: f32,
    pieces: CutPieces,
    rest: Option<PieceRest>,
}

/// What a volume is settling: the change being worked out on its thread,
/// those waiting their turn, the rounds of what gives way still to come
/// loose and when, and what the volume's joins bore at rest.
#[derive(Default)]
pub struct VoxelCollapse {
    memory: Option<VoxelStrainMemory>,
    running: Option<(Job, JoinHandle<(VoxelSettled, VoxelStrainMemory)>)>,
    waiting: Vec<Job>,
    rounds: VecDeque<(f32, VoxelRound, Job)>,
    clock: f32,
}

impl Component for VoxelCollapse {}

impl VoxelCollapse {
    /// Whether it has nothing still to come loose.
    pub fn settled(&self) -> bool {
        self.running.is_none() && self.waiting.is_empty() && self.rounds.is_empty()
    }
}

/// How fast a built piece last went, and when it last hit hard.
#[derive(Clone, Copy, Debug, Default)]
struct PieceSpeed {
    speed: Vec3,
    since: f32,
}

impl Component for PieceSpeed {}

/// Each endless volume (all, or `only`) and its corner in the world.
fn volumes(world: &World, only: Option<Entity>) -> Vec<(Entity, Vec3)> {
    world
        .query_ref::<(Entity, &VoxelVolume, &Transform, Option<&voxear_scene::WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .map(|(entity, _, local, placed)| (entity, placed.map_or(local.position, |placed| placed.0.w_axis.truncate())))
        .collect()
}

/// Digs out what was built in a ball of `radius` metres round `at` in the
/// world, in every endless volume it reaches or `only`'s, dust flying; and
/// settles what is left, as `options` say. Kept with the volume and saved
/// with its other changes.
pub fn blast(world: &mut World, at: Vec3, radius: f32, options: BlastOptions, only: Option<Entity>) -> BlastDone {
    let mut done = BlastDone::default();
    for (entity, corner) in volumes(world, only) {
        let Some(volume) = world.get::<VoxelVolume>(entity).cloned() else { continue };
        let local = VoxelBlast { at: at - corner, radius };
        // What it digs out, at once; what that leaves loose is worked out after.
        let (changes, dug, dust) = voxear_render::voxel_blast_dig(&volume, local);
        if dug.is_empty() {
            continue;
        }
        done.reached += 1;
        done.dug += dug.len();
        let changes = Arc::new(changes);
        if let Some(kept) = world.get_mut::<VoxelVolume>(entity) {
            kept.changes = changes.clone();
        }
        crate::voxel_sync::hint_change(world, entity, &volume, &changes, crate::voxel_sync::HintWhat::Blast(local));
        raise_dust(world, dust.iter().map(|p| corner + *p));
        let job =
            Job { before: volume, dug, from: at, throw: options.throw, pieces: options.pieces, rest: options.rest };
        if options.wait {
            let (pieces, broken, rounds) = settle_now(world, entity, job);
            done.pieces.extend(pieces);
            done.broken += broken;
            done.rounds += rounds;
        } else {
            queue(world, entity, job);
        }
    }
    done
}

/// Settles `entity`'s volume after its voxels in the box from `low` to
/// `high` (in the world) were dug or carved out, it having been `before`:
/// what that cut off comes loose, and what gives way after, falling as
/// `pieces` says.
pub fn settle_after(world: &mut World, entity: Entity, before: VoxelVolume, low: Vec3, high: Vec3, pieces: CutPieces) {
    let Some((_, corner)) = volumes(world, Some(entity)).pop() else { return };
    let Some(after) = world.get::<VoxelVolume>(entity) else { return };
    if after.anchor == voxear_render::VoxelAnchor::Everything {
        return;
    }
    let voxel = 1.0 / after.voxels_per_metre.clamp(1, voxear_render::MOST_VOXELS_A_METRE) as f32;
    let (least, most) =
        (((low - corner) / voxel).floor().as_ivec3() - 1, ((high - corner) / voxel).floor().as_ivec3() + 1);
    // Too large a change to look over voxel by voxel, as an area laid over a field: nothing is let loose.
    let span = (most - least + 1).max(IVec3::ZERO).as_vec3();
    if span.x * span.y * span.z > MOST_LOOKED_OVER {
        return;
    }
    let dug = voxear_render::voxel_dug_between(&before, after, least, most);
    if dug.is_empty() {
        return;
    }
    let from = (low + high) * 0.5;
    queue(world, entity, Job { before, dug, from, throw: 0.0, pieces, rest: None });
}

/// Starts working out `job` on a thread of its own, or keeps it for when
/// the one being worked out is done.
fn queue(world: &mut World, entity: Entity, job: Job) {
    if world.get::<VoxelCollapse>(entity).is_none() {
        world.insert(entity, VoxelCollapse::default());
    }
    let Some(after) = world.get::<VoxelVolume>(entity).cloned() else { return };
    let Some(collapse) = world.get_mut::<VoxelCollapse>(entity) else { return };
    if collapse.running.is_some() {
        collapse.waiting.push(job);
        return;
    }
    let mut memory = collapse.memory.take().unwrap_or_default();
    let (before, dug) = (job.before.clone(), job.dug.clone());
    let handle = std::thread::Builder::new()
        .name("Voxel settling".into())
        .spawn(move || {
            let (_, settled) = voxear_render::voxel_settle(&before, &after, &dug, &mut memory);
            (settled, memory)
        })
        .expect("a thread to settle on");
    collapse.running = Some((job, handle));
}

/// Settles `entity`'s volume after `job` here and now, every round of what
/// gives way coming loose at once: the pieces, how many joins broke and in
/// how many rounds.
fn settle_now(world: &mut World, entity: Entity, job: Job) -> (Vec<FelledPiece>, usize, usize) {
    let Some(after) = world.get::<VoxelVolume>(entity).cloned() else { return (Vec::new(), 0, 0) };
    if world.get::<VoxelCollapse>(entity).is_none() {
        world.insert(entity, VoxelCollapse::default());
    }
    let mut memory = world.get_mut::<VoxelCollapse>(entity).and_then(|c| c.memory.take()).unwrap_or_default();
    let (_, settled) = voxear_render::voxel_settle(&job.before, &after, &job.dug, &mut memory);
    if let Some(collapse) = world.get_mut::<VoxelCollapse>(entity) {
        collapse.memory = Some(memory);
    }
    let mut pieces = loosen(
        world,
        entity,
        &settled.loosened.dig,
        settled.loosened.pieces.clone(),
        &settled.loosened.crumbs,
        &job,
        true,
    );
    for round in &settled.strain.loose {
        pieces.extend(loosen(world, entity, &round.dig, round.pieces.clone(), &round.crumbs, &job, false));
    }
    (pieces, settled.strain.broken, settled.strain.rounds)
}

/// Digs `dig` out of `entity`'s volume and lets `pieces` fall from it,
/// thrown from the job's blast where `thrown`, dust where `crumbs` were.
fn loosen(
    world: &mut World,
    entity: Entity,
    dig: &[IVec3],
    pieces: Vec<VoxelPiece>,
    crumbs: &[Vec3],
    job: &Job,
    thrown: bool,
) -> Vec<FelledPiece> {
    let Some((_, corner)) = volumes(world, Some(entity)).pop() else { return Vec::new() };
    if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
        if dig.is_empty() && pieces.is_empty() {
            return Vec::new();
        }
        let changes = Arc::make_mut(&mut volume.changes);
        for c in dig {
            changes.set(*c, None);
        }
    }
    raise_dust(world, crumbs.iter().map(|p| corner + *p));
    let mut made = Vec::new();
    for piece in pieces {
        let place = corner + piece.low.as_vec3() * piece.voxel + piece.middle();
        let gone = job.pieces == CutPieces::Gone;
        if gone || piece.voxels.len() < DEBRIS {
            if !gone {
                raise_dust(world, std::iter::once(place));
            }
            made.push(felled(None, place, &piece));
            continue;
        }
        // What a blast cut off is thrown from it; what gave way after sags into the gap, tipping over the edge of
        // its foot toward it.
        let toward = ((job.from - place) * Vec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
        let push = match (thrown, job.throw > 0.0) {
            (true, true) => PiecePush::Away { from: job.from, speed: job.throw },
            (false, _) if toward != Vec3::ZERO => PiecePush::Tip,
            _ => PiecePush::Drop,
        };
        let pivot = (
            corner + piece.low.as_vec3() * piece.voxel + crate::voxel_felling::stump_edge(&piece, toward),
            Vec3::Y.cross(toward),
        );
        let rest = Some(job.rest.unwrap_or(PieceRest::Stamp));
        let body = PieceMade { push, rest, built: true, mass: None };
        let obj = crate::voxel_felling::spawn_body(
            world,
            entity,
            corner,
            piece.clone(),
            body,
            "Fallen piece",
            Acoustic::Brick,
        );
        if let Some(body) = world.get_mut::<VoxelPieceBody>(obj) {
            (body.toward, body.pivot) = (toward, pivot);
        }
        world.insert(obj, PieceSpeed::default());
        made.push(felled(Some(obj), place, &piece));
    }
    made
}

fn felled(obj: Option<Entity>, place: Vec3, piece: &VoxelPiece) -> FelledPiece {
    let size = Vec3::from(piece.size.map(|a| a as f32)) * piece.voxel;
    FelledPiece { obj, place, size, logs: piece.voxels.len(), leaves: 0, resting: false }
}

/// Dust where what was built crumbled, as many clouds as [`MOST_DUST`] at
/// most, spread among the places.
fn raise_dust(world: &mut World, places: impl Iterator<Item = Vec3>) {
    let places: Vec<Vec3> = places.collect();
    if places.is_empty() {
        return;
    }
    let every = places.len().div_ceil(MOST_DUST);
    for at in places.into_iter().step_by(every.max(1)) {
        let effect = voxear_render::EffectRef::Builtin(voxear_render::Starter::Dust);
        let cloud = crate::particles::play(world, effect, (at, Quat::IDENTITY), None, None);
        world.insert(cloud, Dusting(DUST_FOR));
    }
}

/// How long a cloud of dust is sent out for, in seconds: a puff, its dust
/// then settling.
const DUST_FOR: f32 = 0.8;

/// A cloud of dust still being sent out: for how many seconds more.
struct Dusting(f32);

impl Component for Dusting {}

/// Stops the clouds of dust whose time is up sending out more.
fn settle_dust(world: &mut World, dt: f32) {
    let clouds: Vec<(Entity, f32)> =
        world.query_ref::<(Entity, &Dusting)>().into_iter().map(|(e, left)| (e, left.0 - dt)).collect();
    if clouds.is_empty() {
        return;
    }
    let now = crate::particles::now(world) as f32;
    for (cloud, left) in clouds {
        if left > 0.0 {
            if let Some(kept) = world.get_mut::<Dusting>(cloud) {
                kept.0 = left;
            }
            continue;
        }
        world.remove::<Dusting>(cloud);
        if let Some(playback) = world.get_mut::<voxear_render::ParticlePlayback>(cloud) {
            playback.stopped = Some(now);
        }
    }
}

/// Moves what is settling on, after each physics step: what a volume's
/// thread has worked out comes loose, the rounds of what gives way after
/// come loose in their turn, built pieces that hit hard dig out or chip
/// what they hit, and past a volume's Most Pieces the smallest falling
/// fall to dust.
pub fn step(world: &mut World, dt: f32, events: &[PhysicsEvent]) {
    let settling: Vec<Entity> = world.query_ref::<(Entity, &VoxelCollapse)>().into_iter().map(|(e, _)| e).collect();
    for entity in settling {
        let Some(collapse) = world.get_mut::<VoxelCollapse>(entity) else { continue };
        collapse.clock += dt;
        let clock = collapse.clock;
        // Worked out: what was cut off comes loose now, what gives way after in its rounds.
        let finished = collapse.running.as_ref().is_some_and(|(_, handle)| handle.is_finished());
        if finished && let Some((job, handle)) = collapse.running.take() {
            if let Ok((settled, memory)) = handle.join() {
                collapse.memory = Some(memory);
                for (k, round) in settled.strain.loose.into_iter().enumerate() {
                    collapse.rounds.push_back((clock + ROUND_GAP * (k + 1) as f32, round, job.clone()));
                }
                let loose = settled.loosened;
                loosen(world, entity, &loose.dig, loose.pieces, &loose.crumbs, &job, true);
            }
            let next = world
                .get_mut::<VoxelCollapse>(entity)
                .and_then(|c| (!c.waiting.is_empty()).then(|| c.waiting.remove(0)));
            if let Some(next) = next {
                queue(world, entity, next);
            }
        }
        let due: Vec<(VoxelRound, Job)> = {
            let Some(collapse) = world.get_mut::<VoxelCollapse>(entity) else { continue };
            let mut due = Vec::new();
            while collapse.rounds.front().is_some_and(|(at, ..)| *at <= clock) {
                if let Some((_, round, job)) = collapse.rounds.pop_front() {
                    due.push((round, job));
                }
            }
            due
        };
        for (round, job) in due {
            loosen(world, entity, &round.dig, round.pieces, &round.crumbs, &job, false);
        }
    }
    hits(world, dt, events);
    keep_to_most(world);
    settle_dust(world, dt);
}

/// Built pieces that slowed hard dig out what they hit in their volume;
/// built pieces that hit each other chip where they touch.
fn hits(world: &mut World, dt: f32, events: &[PhysicsEvent]) {
    let Some(physics) = world.remove_resource::<PhysicsScene>() else { return };
    let falling: Vec<(Entity, VoxelPieceBody, Vec3)> = world
        .query_ref::<(Entity, &VoxelPieceBody, &Transform)>()
        .into_iter()
        .filter(|(_, body, _)| body.built && body.fall == PieceFall::Falling)
        .map(|(e, body, t)| (e, body.clone(), t.position))
        .collect();
    let mut blasts: Vec<(Entity, Vec3, f32)> = Vec::new();
    let mut speeds: Vec<(Entity, Vec3)> = Vec::new();
    for (entity, body, place) in &falling {
        let Some(id) = physics.body(*entity) else { continue };
        let (now, _) = physics.world().velocity(id);
        let Some(was) = world.get::<PieceSpeed>(*entity).copied() else { continue };
        speeds.push((*entity, now));
        let slowed = was.speed.length() - now.length();
        let lost = 0.5 * body.piece.mass * (was.speed.length_squared() - now.length_squared()).max(0.0);
        if slowed < IMPACT_SPEED || lost < IMPACT_ENERGY || was.since < IMPACT_GAP {
            continue;
        }
        // Where it hit: the volume along the way it went.
        let way = was.speed.normalize_or(Vec3::NEG_Y);
        let reach = body.reach + 0.6;
        if let Some((hit_volume, hit)) = crate::voxel_volumes::raycast(world, *place, way, reach, Some(body.volume)) {
            let radius = (lost / CRUSH).cbrt().clamp(0.15, 0.6);
            blasts.push((hit_volume, hit.point + way * radius * 0.3, radius));
        }
        if let Some(kept) = world.get_mut::<PieceSpeed>(*entity) {
            kept.since = 0.0;
        }
    }
    // Pieces that met: each chipped where they touch, as hard as they met.
    let mut chips: Vec<(Entity, Vec3, f32)> = Vec::new();
    for event in events.iter().filter(|e| e.began && !e.trigger) {
        let pair = [event.a, event.b];
        let both = pair.map(|e| falling.iter().find(|(f, ..)| *f == e));
        let [Some(a), Some(b)] = both else { continue };
        let speed = |e: Entity| world.get::<PieceSpeed>(e).map_or(Vec3::ZERO, |s| s.speed);
        let met = (speed(a.0) - speed(b.0)).length();
        if met < IMPACT_SPEED * 1.5 {
            continue;
        }
        let mass = a.1.piece.mass.min(b.1.piece.mass);
        let radius = (0.25 * mass * met * met / CRUSH).cbrt().clamp(0.1, 0.4);
        chips.push((a.0, event.point, radius));
        chips.push((b.0, event.point, radius));
    }
    world.insert_resource(physics);
    for (entity, speed) in speeds {
        if let Some(kept) = world.get_mut::<PieceSpeed>(entity) {
            (kept.speed, kept.since) = (speed, kept.since + dt);
        }
    }
    for (volume, at, radius) in blasts {
        let options = BlastOptions { throw: 0.0, ..BlastOptions::default() };
        blast(world, at, radius, options, Some(volume));
    }
    for (entity, at, radius) in chips {
        chip(world, entity, at, radius);
    }
}

/// Chips built piece `entity` where it was hit, at `at` in the world: its
/// voxels within `radius` metres fall to dust. Gives how many.
pub fn chip(world: &mut World, entity: Entity, at: Vec3, radius: f32) -> usize {
    let Some(body) = world.get::<VoxelPieceBody>(entity).cloned() else { return 0 };
    let Some((position, rotation)) = world.get::<Transform>(entity).map(|t| (t.position, t.rotation)) else { return 0 };
    let piece = &body.piece;
    // Into the piece's box, in its voxels.
    let local = (rotation.inverse() * (at - position) + body.middle) / piece.voxel;
    let r = radius / piece.voxel;
    let kept: Vec<([u16; 3], u8)> = piece
        .voxels
        .iter()
        .copied()
        .filter(|(c, _)| (Vec3::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) + 0.5).distance(local) > r)
        .collect();
    let taken = piece.voxels.len() - kept.len();
    if taken == 0 {
        return 0;
    }
    raise_dust(world, std::iter::once(at));
    if kept.len() < DEBRIS {
        crate::voxel_felling::remove_piece(world, entity);
        return taken;
    }
    let left = kept.len() as f32 / piece.voxels.len() as f32;
    let chipped = VoxelPiece { voxels: kept, mass: piece.mass * left, ..(**piece).clone() };
    let roughness = world.get::<VoxelVolume>(body.look).map_or(0.8, |look| look.roughness);
    world.insert(body.look, crate::voxel_felling::piece_volume(&chipped, roughness));
    if let Some(kept) = world.get_mut::<VoxelPieceBody>(entity) {
        kept.piece = Arc::new(chipped);
    }
    taken
}

/// Past each volume's Most Pieces falling at once, the smallest fall to
/// dust.
fn keep_to_most(world: &mut World) {
    let mut live: Vec<(Entity, Entity, usize, Vec3)> = world
        .query_ref::<(Entity, &VoxelPieceBody, &Transform)>()
        .into_iter()
        .filter(|(_, body, _)| body.built && body.fall != PieceFall::Resting)
        .map(|(e, body, t)| (e, body.volume, body.piece.voxels.len(), t.position))
        .collect();
    if live.is_empty() {
        return;
    }
    live.sort_by_key(|(_, volume, voxels, _)| (*volume, std::cmp::Reverse(*voxels)));
    let mut at = 0;
    while at < live.len() {
        let volume = live[at].1;
        let most = world.get::<VoxelVolume>(volume).map_or(voxear_render::MOST_PIECES, |v| v.most_pieces) as usize;
        let end = live[at..].iter().position(|l| l.1 != volume).map_or(live.len(), |n| at + n);
        for (entity, _, _, place) in live[(at + most).min(end)..end].to_vec() {
            raise_dust(world, std::iter::once(place));
            crate::voxel_felling::remove_piece(world, entity);
        }
        at = end;
    }
}

/// Whether every endless volume has settled: nothing being worked out and
/// no round still to come loose.
pub fn all_settled(world: &World) -> bool {
    world.query_ref::<&VoxelCollapse>().into_iter().all(VoxelCollapse::settled)
}
