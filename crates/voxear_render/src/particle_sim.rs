//! Simulated particles: the Particle Systems whose particles need more than
//! their age to say where they are (see `particles`), kept on the GPU from
//! frame to frame and stepped there once a frame, for every view
//! (`shaders/particle_sim.slang`). The main view records the step among its
//! own passes, once its depth is drawn, which is what colliding particles
//! meet; a frame that does not draw the main view steps nothing.
//!
//! All of them share one pool. A system has a run of it to itself, as many
//! particles as it can have out at once, and fills it in a ring: each new
//! particle takes the place of its oldest, which has died by then. So the
//! GPU never counts or sorts, nothing comes back to the CPU for drawing, and
//! a system's particles are drawn in the order they were sent out, the same
//! every frame. Here, each frame, the CPU only says how many each system
//! sends out and how old they already are, which is how a system first seen
//! with its stream already running starts full rather than empty.
//!
//! The pool is handed out in chunks of [`CHUNK`] particles, and the pass
//! steps only the chunks of systems that have particles out: one whose last
//! particle has died costs nothing until it plays again. The pool starts
//! small and doubles as systems need room, up to the budget
//! ([`ParticleBudget`]); a system there is no room for is left out,
//! and counted ([`ParticleCounts::left_out`]).

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_rhi::{Buffer, Frames, Gpu, MemoryLocation, Retired, vk};
use voxear_scene::{Destroying, Switch, Switched, Transform, WorldOrigin, WorldTransform};

use crate::{
    effects::ParticleEffects,
    gpu::{GpuParticle, GpuParticleSim, GpuParticleSystem, GpuWind},
    graph::{BufferId, FrameGraph},
    particles::{self, ParticlePlayback, ParticleSpace, ParticleSystem, Placed, PlacedSystems, Step},
};

/// How many particles a chunk of the pool holds (`PARTICLE_CHUNK` in the
/// shaders): a system's room is whole chunks.
pub const CHUNK: u32 = 256;
/// The most simulated systems stepped in a frame.
pub const MAX_SIMULATED_SYSTEMS: usize = 1024;
/// The most particles the pool holds unless a project says otherwise.
pub const DEFAULT_PARTICLE_BUDGET: u32 = 1_000_000;
/// The most a project can ask for: what one pass can step.
pub const MAX_PARTICLE_BUDGET: u32 = 4_000_000;
/// The most simulated particles a level's Particle Systems may have room
/// for between them, as its project says (a resource of the world); without
/// one, [`DEFAULT_PARTICLE_BUDGET`]. Never more than [`MAX_PARTICLE_BUDGET`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBudget(pub u32);

/// How many chunks the pool starts with.
const FIRST_CHUNKS: u32 = 64;
/// How many bytes the pass's counters take.
pub(crate) const COUNTERS: u64 = 16;
/// How long after a system's last particle should have died it is still
/// stepped, in seconds, so the last are seen out.
const SEEN_OUT: f32 = 0.5;

/// What the simulation holds and did, for the stats and the debugger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticleCounts {
    /// Particles alive, as the GPU counted them a few frames ago.
    pub alive: u32,
    /// Simulated systems with particles out, and with room in the pool.
    pub awake: u32,
    pub systems: u32,
    /// Particles the systems have room for between them, and the most the
    /// project lets them have.
    pub reserved: u32,
    pub budget: u32,
    /// Systems there was no room for.
    pub left_out: u32,
}

/// Runs of chunks handed out and given back.
#[derive(Debug, Default)]
struct Spans {
    /// What has been given back, by where it starts, no two touching.
    free: Vec<(u32, u32)>,
    /// How many chunks have ever been reached into.
    end: u32,
}

impl Spans {
    /// A run of `chunks`, the first that fits, or more of the pool up to
    /// `limit` chunks; `None` when there is no room.
    fn take(&mut self, chunks: u32, limit: u32) -> Option<u32> {
        if let Some(at) = self.free.iter().position(|&(_, length)| length >= chunks) {
            let (start, length) = self.free[at];
            match length == chunks {
                true => drop(self.free.remove(at)),
                false => self.free[at] = (start + chunks, length - chunks),
            }
            return Some(start);
        }
        let start = self.end;
        (start.checked_add(chunks)? <= limit).then(|| {
            self.end += chunks;
            start
        })
    }

    /// Gives back a run, joining it to those beside it.
    fn give(&mut self, start: u32, chunks: u32) {
        let at = self.free.partition_point(|&(other, _)| other < start);
        self.free.insert(at, (start, chunks));
        if at + 1 < self.free.len() && self.free[at].0 + self.free[at].1 == self.free[at + 1].0 {
            self.free[at].1 += self.free[at + 1].1;
            self.free.remove(at + 1);
        }
        if at > 0 && self.free[at - 1].0 + self.free[at - 1].1 == self.free[at].0 {
            self.free[at - 1].1 += self.free[at].1;
            self.free.remove(at);
        }
        if let Some(&(last, length)) = self.free.last()
            && last + length == self.end
        {
            self.end = last;
            self.free.pop();
        }
    }
}

/// What is kept of a simulated system between frames.
#[derive(Debug)]
struct Kept {
    /// Its room: its first chunk and how many, and how many particles.
    first: u32,
    chunks: u32,
    capacity: u32,
    /// Where in its room the next particle goes.
    head: u32,
    /// How many it has sent out, which numbers them.
    emitted: u32,
    /// When it started, as its playback says, and how far in it was when it
    /// was last stepped; `None` before its first step.
    started: f32,
    last: Option<f32>,
    /// What of its stream is owed, less than one particle.
    owed: f32,
    /// How far in it was when it last sent one out.
    last_out: f32,
    /// Where its Obj was last frame.
    origin: Vec3,
    /// How far the world's origin has moved since it was stepped.
    shift: Vec3,
    /// Its room holds none of its own particles yet.
    fresh: bool,
    /// The tick its particles' tails last kept a point at.
    trail_tick: Option<u32>,
    /// The last frame it was seen.
    seen: u64,
}

/// What a system sends out in a step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sent {
    /// How many from bursts, and how old they are, in seconds.
    burst: u32,
    burst_age: f32,
    /// How many of its stream, and how old the first and the last are.
    stream: u32,
    stream_ages: [f32; 2],
}

/// What `system` sends out between `last` seconds in, or since as long
/// before `now` as its longest lived lives when it has never been stepped,
/// and `now`, having `stopped` then if ever; `owed` carries what of its
/// stream is left over, less than one particle, to the next step.
fn sent(system: &ParticleSystem, last: Option<f32>, now: f32, stopped: f32, owed: &mut f32) -> Sent {
    let mut sent = Sent::default();
    let duration = system.duration.max(0.01);
    let longest = system.lifetime[0].max(system.lifetime[1]).max(0.01);
    let until = now.min(stopped).min(if system.looping { f32::INFINITY } else { duration });
    let from = last.unwrap_or((now - longest).max(0.0)).max(0.0);
    if until < from || now < 0.0 {
        return sent;
    }
    // The rounds that started in that time: each one's burst, as old as the latest.
    let rounds = if system.looping { (until / duration).floor() } else { 0.0 };
    let first = if last.is_some() { (from / duration).floor() + 1.0 } else { (from / duration).ceil() };
    if rounds >= first && system.burst > 0 {
        sent.burst = system.burst.saturating_mul((rounds - first + 1.0) as u32);
        sent.burst_age = now - rounds * duration;
    }
    *owed += system.rate.max(0.0) * (until - from);
    sent.stream = owed.floor() as u32;
    *owed -= sent.stream as f32;
    sent.stream_ages = [now - from, now - until];
    sent
}

/// One frame in flight's buffers: the systems, the chunks to step, what the
/// pass reads of the frame, and what its counters came to, copied out for
/// the CPU once it is done.
struct SimBuffers {
    systems: Buffer,
    chunks: Buffer,
    frame: Buffer,
    counted: Buffer,
    /// How many chunks `chunks` has room for.
    room: u32,
}

pub(crate) struct ParticleSim {
    budget: u32,
    /// The pool, and how many chunks it holds; none until a system needs it.
    pool: Option<Buffer>,
    pool_chunks: u32,
    /// What the pass counts as it steps, in the GPU's own memory, where its
    /// adding up is fast: how many particles are alive.
    counters: Option<Buffer>,
    spans: Spans,
    kept: HashMap<(Entity, u32), Kept>,
    buffers: Vec<Option<SimBuffers>>,
    /// This frame's systems with particles out, for each view to draw.
    pub placed: Arc<PlacedSystems>,
    /// How many chunks this frame steps.
    stepped: u32,
    counts: ParticleCounts,
    warned: bool,
    /// The particles' time at the last step.
    last_time: Option<f32>,
}

impl ParticleSim {
    pub fn new(slots: usize) -> Self {
        Self {
            budget: DEFAULT_PARTICLE_BUDGET,
            pool: None,
            pool_chunks: 0,
            counters: None,
            spans: Spans::default(),
            kept: HashMap::new(),
            buffers: (0..slots).map(|_| None).collect(),
            placed: Arc::default(),
            stepped: 0,
            counts: ParticleCounts { budget: DEFAULT_PARTICLE_BUDGET, ..ParticleCounts::default() },
            warned: false,
            last_time: None,
        }
    }

    pub fn counts(&self) -> ParticleCounts {
        self.counts
    }

    /// The pool's address, for a view's shaders to draw from; 0 with none.
    pub fn pool_address(&self) -> u64 {
        self.pool.as_ref().map_or(0, |pool| pool.address)
    }

    /// Moves every simulated particle by `by`, as the world's origin moves
    /// everything: those in the world, at their system's next step.
    pub fn shift_origin(&mut self, by: Vec3) {
        for kept in self.kept.values_mut() {
            kept.shift += by;
            kept.origin += by;
        }
    }

    /// Works out this frame's step for frame slot `slot`, at `time` by the
    /// particles' time: which systems are simulated, room for the new
    /// ones, what each sends out, and which chunks of the pool are stepped.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        frames: &mut Frames,
        world: &World,
        time: f32,
        (frame, slot): (u64, usize),
    ) -> Result<()> {
        // How long since the last step, by the particles' own time: none while it stands still, or goes back.
        let delta_time = self.last_time.map_or(0.0, |last| (time - last).clamp(0.0, 0.25));
        self.last_time = Some(time);
        // This slot's last frame has finished, so its count is final.
        if let Some(buffers) = &self.buffers[slot] {
            self.counts.alive = bytemuck::cast_slice::<u8, u32>(&buffers.counted.read()[..4])[0];
        }
        // Systems that already have room keep it; no more is handed out past the budget.
        let budget = world.resource::<ParticleBudget>().map_or(DEFAULT_PARTICLE_BUDGET, |budget| budget.0);
        if std::mem::replace(&mut self.budget, budget.min(MAX_PARTICLE_BUDGET)) != self.budget {
            self.warned = false;
        }
        let limit = self.budget / CHUNK;
        let mut systems: Vec<GpuParticleSystem> = Vec::new();
        let mut chunks: Vec<[u32; 2]> = Vec::new();
        let mut placed = PlacedSystems::new();
        let mut left_out = 0;
        let query = world
            .query_ref::<(Entity, &ParticleSystem, &Transform, Option<&WorldTransform>, Option<&ParticlePlayback>)>();
        let effects = world.resource::<ParticleEffects>();
        for (entity, own, transform, at, playback) in query.without::<Destroying>() {
            let model = at.map_or_else(|| transform.matrix(), |w| w.0);
            let origin = model.w_axis.truncate();
            let whole = playback.copied().unwrap_or(ParticlePlayback { started: 0.0, stopped: None });
            // Its effect's emitters, each a system of its own, or itself.
            for (emitter, system) in (0u32..).zip(crate::effects::emitters(effects, own)) {
                let key = (entity, emitter);
                let capacity = system.capacity();
                if !system.is_simulated() || capacity == 0 {
                    continue;
                }
                // Each emitter starts its Delay after the rest, and sends out particles of its own.
                let started = whole.started + system.delay.max(0.0);
                let playback = ParticlePlayback { started, stopped: whole.stopped.map(|stopped| stopped.max(started)) };
                // Room for it, made again when it needs more or less.
                let needed = system.room().div_ceil(CHUNK);
                if self.kept.get(&key).is_some_and(|kept| kept.capacity != capacity || kept.chunks != needed) {
                    let old = self.kept.remove(&key).expect("checked above");
                    self.spans.give(old.first, old.chunks);
                }
                if !self.kept.contains_key(&key) {
                    let Some(first) = self.spans.take(needed, limit).filter(|_| systems.len() < MAX_SIMULATED_SYSTEMS)
                    else {
                        left_out += 1;
                        continue;
                    };
                    let kept = Kept {
                        first,
                        chunks: needed,
                        capacity,
                        head: 0,
                        emitted: 0,
                        started: playback.started,
                        last: None,
                        owed: 0.0,
                        last_out: f32::NEG_INFINITY,
                        origin,
                        shift: Vec3::ZERO,
                        fresh: true,
                        trail_tick: None,
                        seen: frame,
                    };
                    self.kept.insert(key, kept);
                }
                let kept = self.kept.get_mut(&key).expect("made above");
                kept.seen = frame;
                if kept.started != playback.started {
                    // Played again: over from the start, its old particles gone.
                    (kept.started, kept.last, kept.owed, kept.fresh) = (playback.started, None, 0.0, true);
                    (kept.head, kept.last_out) = (0, f32::NEG_INFINITY);
                }
                let now = time - playback.started;
                if kept.last.is_some_and(|last| now < last) {
                    // Time has gone back, as when a level starts over: it starts over with it.
                    (kept.last, kept.owed, kept.fresh, kept.head, kept.last_out) =
                        (None, 0.0, true, 0, f32::NEG_INFINITY);
                }
                let stopped = playback.stopped.map_or(f32::INFINITY, |stopped| stopped - playback.started);
                let sent = sent(system, kept.last, now, stopped, &mut kept.owed);
                kept.last = Some(now);
                let burst = sent.burst.min(capacity);
                let count = burst + sent.stream.min(capacity - burst);
                if count > 0 {
                    kept.last_out = now;
                }
                let longest = system.lifetime[0].max(system.lifetime[1]).max(0.01);
                // Nothing out and nothing to clear: not stepped, not drawn.
                if !kept.fresh && now - kept.last_out > longest + SEEN_OUT {
                    (kept.origin, kept.shift) = (origin, Vec3::ZERO);
                    continue;
                }
                let fresh = if std::mem::take(&mut kept.fresh) { particles::FRESH } else { 0 };
                let stepped = particles::gpu(system, model, time, Some(whole));
                // Each tick gone by since the last step, a point for its particles' tails to keep.
                let ticks = kept.trail_tick.replace(stepped.trail_tick).map_or(0, |last| stepped.trail_tick.saturating_sub(last));
                let index = systems.len() as u32;
                systems.push(GpuParticleSystem {
                    flags: stepped.flags | fresh,
                    pool_first: kept.first * CHUNK,
                    capacity,
                    head: kept.head,
                    emit_count: count,
                    emit_burst: burst,
                    burst_age: sent.burst_age,
                    emitted: kept.emitted,
                    seed: particles::emitter_seed(stepped.seed, emitter),
                    stream_ages: sent.stream_ages,
                    moved: (origin - kept.origin).to_array(),
                    shift: if system.space == ParticleSpace::World { kept.shift.to_array() } else { [0.0; 3] },
                    trail_new: if delta_time > 0.0 { ticks.min(particles::TRAIL_POINTS) } else { 0 },
                    ..stepped
                });
                // Only its particles are stepped: their tails' points, kept after them, are written along with them.
                chunks.extend((0..capacity.div_ceil(CHUNK)).map(|chunk| [index, chunk * CHUNK]));
                kept.head = (kept.head + count) % capacity;
                kept.emitted = kept.emitted.wrapping_add(count);
                (kept.origin, kept.shift) = (origin, Vec3::ZERO);
                placed.insert(key, Placed { first: kept.first * CHUNK, capacity, oldest: kept.head });
            }
        }
        // Systems that have gone give their room back.
        let gone: Vec<(Entity, u32)> =
            self.kept.iter().filter(|(_, kept)| kept.seen != frame).map(|(&e, _)| e).collect();
        for entity in gone {
            let old = self.kept.remove(&entity).expect("listed above");
            self.spans.give(old.first, old.chunks);
        }
        if left_out > 0 && !std::mem::replace(&mut self.warned, true) {
            tracing::warn!(
                "{left_out} Particle Systems have no room in the particle budget of {} and are not drawn; raise it in \
                 Project Settings, or lower their Rate or Lifetime",
                self.budget
            );
        }
        self.counts = ParticleCounts {
            alive: self.counts.alive,
            awake: systems.len() as u32,
            systems: self.kept.len() as u32,
            reserved: self.kept.values().map(|kept| kept.chunks * CHUNK).sum(),
            budget: self.budget,
            left_out,
        };
        self.placed = Arc::new(placed);
        self.stepped = chunks.len() as u32;
        if systems.is_empty() {
            return Ok(());
        }
        self.fit_pool(gpu, frames)?;
        let buffers = self.fit_buffers(gpu, frames, slot, chunks.len() as u32)?;
        buffers.systems.write(0, &systems);
        buffers.chunks.write(0, &chunks);
        let mut header = GpuParticleSim {
            time,
            delta_time,
            chunks: chunks.len() as u32,
            wind: wind(world),
            origin: crate::gpu::frame_origin(WorldOrigin::of(world).0),
            ..GpuParticleSim::default()
        };
        let forces = crate::forces::gather(world, time);
        header.force_count = forces.len() as u32;
        header.forces[..forces.len()].copy_from_slice(&forces);
        buffers.frame.write(0, &[header]);
        Ok(())
    }

    /// Makes the pool hold every chunk handed out, doubling it and copying
    /// what it held.
    fn fit_pool(&mut self, gpu: &Gpu, frames: &mut Frames) -> Result<()> {
        if self.counters.is_none() {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::TRANSFER_DST;
            self.counters = Some(gpu.create_buffer("particle counters", COUNTERS, usage, MemoryLocation::GpuOnly)?);
        }
        if self.spans.end <= self.pool_chunks {
            return Ok(());
        }
        let chunks =
            self.spans.end.next_power_of_two().max(FIRST_CHUNKS).min((self.budget / CHUNK).max(self.spans.end));
        let size = u64::from(chunks) * u64::from(CHUNK) * size_of::<GpuParticle>() as u64;
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::TRANSFER_DST;
        let pool = gpu.create_buffer("particles", size, usage, MemoryLocation::GpuOnly)?;
        if let Some(old) = self.pool.take() {
            // After every frame already sent, which stepped the old pool, and before this one, which steps the new.
            gpu.immediate(|cmd| unsafe {
                let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: old.size.min(size) };
                gpu.device.cmd_copy_buffer(cmd, old.handle, pool.handle, &[region]);
            })?;
            frames.retire(Retired::Buffer(old));
        }
        (self.pool, self.pool_chunks) = (Some(pool), chunks);
        Ok(())
    }

    /// Frame slot `slot`'s buffers, with room for `chunks` chunks.
    fn fit_buffers(&mut self, gpu: &Gpu, frames: &mut Frames, slot: usize, chunks: u32) -> Result<&SimBuffers> {
        if self.buffers[slot].as_ref().is_some_and(|buffers| buffers.room < chunks) {
            let old = self.buffers[slot].take().expect("checked above");
            for buffer in [old.systems, old.chunks, old.frame, old.counted] {
                frames.retire(Retired::Buffer(buffer));
            }
        }
        if self.buffers[slot].is_none() {
            let room = chunks.next_power_of_two().max(FIRST_CHUNKS);
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let host = MemoryLocation::CpuToGpu;
            let named = |name: &str| format!("particle {name} {slot}");
            let systems = (MAX_SIMULATED_SYSTEMS * size_of::<GpuParticleSystem>()) as u64;
            self.buffers[slot] = Some(SimBuffers {
                systems: gpu.create_buffer(&named("simulation systems"), systems, storage, host)?,
                chunks: gpu.create_buffer(&named("chunks"), u64::from(room) * 8, storage, host)?,
                frame: gpu.create_buffer(
                    &named("simulation frame"),
                    size_of::<GpuParticleSim>() as u64,
                    storage,
                    host,
                )?,
                // Host visible, so the count shows in the stats once the frame is done.
                counted: gpu.create_buffer(
                    &named("counted"),
                    COUNTERS,
                    storage | vk::BufferUsageFlags::TRANSFER_DST,
                    MemoryLocation::GpuToCpu,
                )?,
                room,
            });
        }
        Ok(self.buffers[slot].as_ref().expect("made above"))
    }

    /// The pool as this frame's graph knows it, for the pass that steps it
    /// and the passes that draw from it; none with no pool yet.
    pub fn import(&self, fg: &mut FrameGraph<'_>) -> Option<BufferId> {
        self.pool.as_ref().map(|pool| fg.import_buffer(pool))
    }

    /// This frame's step for the main view to record among its passes, once
    /// its depth is drawn; none with nothing to step.
    pub fn step(&self, fg: &mut FrameGraph<'_>, slot: usize) -> Option<Step> {
        let (Some(buffers), Some(counters), true) = (&self.buffers[slot], &self.counters, self.stepped > 0) else {
            return None;
        };
        Some(Step {
            groups: self.stepped * (CHUNK / 64),
            counters: counters.handle,
            counted: buffers.counted.handle,
            counted_id: fg.import_buffer(&buffers.counted),
            systems: buffers.systems.address,
            chunks: buffers.chunks.address,
            frame: buffers.frame.address,
            counters_address: counters.address,
        })
    }

    /// Frees the pool and every frame's buffers.
    pub fn destroy(&mut self, gpu: &Gpu) {
        for buffers in self.buffers.iter_mut().filter_map(Option::take) {
            for buffer in [buffers.systems, buffers.chunks, buffers.frame, buffers.counted] {
                gpu.destroy_buffer(buffer);
            }
        }
        if let Some(counters) = self.counters.take() {
            gpu.destroy_buffer(counters);
        }
        if let Some(pool) = self.pool.take() {
            gpu.destroy_buffer(pool);
        }
    }
}

/// The wind over the whole level, as particles are carried by it: the first
/// Global Wind Zone that is on, as plants lean by.
fn wind(world: &World) -> GpuWind {
    let switched = Switched::of(world);
    let zones = world.query_ref::<(Entity, &crate::WindZone, &Transform)>().without::<Destroying>();
    let global = |(entity, zone, _): &(Entity, &crate::WindZone, &Transform)| {
        zone.shape == crate::WindShape::Global && switched.is_on(world, *entity, Switch::WindZone)
    };
    zones.into_iter().find(global).map(|(_, zone, transform)| zone.gpu(transform.rotation)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_is_handed_out_first_fit_and_joined_when_given_back() {
        let mut spans = Spans::default();
        assert_eq!((spans.take(4, 16), spans.take(2, 16), spans.take(8, 16)), (Some(0), Some(4), Some(6)));
        assert_eq!(spans.take(4, 16), None, "no more than the limit");
        spans.give(0, 4);
        spans.give(4, 2);
        assert_eq!(spans.free, [(0, 6)], "two runs side by side are one");
        assert_eq!(spans.take(5, 16), Some(0));
        assert_eq!(spans.take(1, 16), Some(5));
        spans.give(6, 8);
        assert_eq!((spans.end, spans.free.len()), (6, 0), "the last run given back is the pool's again");
        assert_eq!(spans.take(10, 16), Some(6));
    }

    #[test]
    fn a_stream_is_sent_out_evenly_and_what_is_left_over_is_owed() {
        let system = ParticleSystem { rate: 10.0, lifetime: [2.0, 2.0], ..ParticleSystem::default() };
        let mut owed = 0.0;
        // A quarter of a second in, having been stepped at 0.1: one and a half particles' worth.
        let first = sent(&system, Some(0.1), 0.25, f32::INFINITY, &mut owed);
        assert_eq!((first.stream, first.burst), (1, 0));
        assert!((owed - 0.5).abs() < 1e-4 && (first.stream_ages[0] - 0.15).abs() < 1e-5 && first.stream_ages[1] == 0.0);
        let next = sent(&system, Some(0.25), 0.3, f32::INFINITY, &mut owed);
        assert_eq!(next.stream, 1, "the half owed and half of this step's");
    }

    #[test]
    fn a_system_first_seen_running_starts_with_all_it_would_have_out() {
        let system =
            ParticleSystem { rate: 10.0, lifetime: [1.0, 2.0], burst: 5, duration: 4.0, ..ParticleSystem::default() };
        let mut owed = 0.0;
        // Ten seconds in, never stepped: the two seconds its longest lived could have come from.
        let first = sent(&system, None, 10.0, f32::INFINITY, &mut owed);
        assert_eq!((first.stream, first.stream_ages), (20, [2.0, 0.0]));
        // Its third round started at eight seconds, within that time: its burst, two seconds old.
        assert_eq!((first.burst, first.burst_age), (5, 2.0));
        // Just started: its first round's burst, and no stream yet.
        let mut owed = 0.0;
        let start = sent(&system, None, 0.0, f32::INFINITY, &mut owed);
        assert_eq!((start.burst, start.burst_age, start.stream), (5, 0.0, 0));
    }

    #[test]
    fn bursts_come_as_each_round_starts_and_nothing_after_it_stops() {
        let system = ParticleSystem { rate: 0.0, burst: 8, duration: 1.0, ..ParticleSystem::default() };
        let mut owed = 0.0;
        assert_eq!(sent(&system, Some(0.5), 0.9, f32::INFINITY, &mut owed).burst, 0);
        let round = sent(&system, Some(0.9), 1.1, f32::INFINITY, &mut owed);
        assert_eq!(round.burst, 8);
        assert!((round.burst_age - 0.1).abs() < 1e-5);
        assert_eq!(sent(&system, Some(1.1), 2.5, 1.5, &mut owed).burst, 0, "stopped before its next round");
        let once = ParticleSystem { looping: false, rate: 10.0, duration: 1.0, ..ParticleSystem::default() };
        let mut owed = 0.0;
        let last = sent(&once, Some(0.75), 1.5, f32::INFINITY, &mut owed);
        assert_eq!(last.stream, 2, "only up to the end of its one round");
        assert_eq!(sent(&once, Some(1.5), 2.0, f32::INFINITY, &mut owed).stream, 0);
    }

    #[test]
    fn a_simulated_system_has_room_for_its_stream_and_every_burst_still_out() {
        let steady = ParticleSystem { rate: 100.0, lifetime: [1.0, 3.0], wind: 1.0, ..ParticleSystem::default() };
        assert_eq!(steady.capacity(), 300);
        let bursts = ParticleSystem { rate: 0.0, burst: 50, duration: 1.0, lifetime: [2.5, 2.5], ..steady };
        assert_eq!(bursts.capacity(), 150, "three rounds' bursts are out at once");
        assert_eq!(ParticleSystem { looping: false, ..bursts }.capacity(), 50);
        assert!(steady.is_simulated() && !ParticleSystem::default().is_simulated());
        assert_eq!(ParticleSystem { turbulence: 1.0, ..steady }.simulated_for(), ["Wind", "Turbulence"]);
        // A tail's points take five particles' room after each particle; a ribbon's nothing more.
        let tailed = ParticleSystem { trails: particles::ParticleTrails::Particles, ..steady };
        assert_eq!((steady.room(), tailed.room()), (300, 300 * (1 + particles::TRAIL_ROOM)));
        assert_eq!(ParticleSystem { trails: particles::ParticleTrails::Ribbon, ..steady }.room(), 300);
        assert_eq!(particles::TRAIL_POINTS * 12, particles::TRAIL_ROOM * size_of::<GpuParticle>() as u32, "three points to a particle's room");
    }
}
