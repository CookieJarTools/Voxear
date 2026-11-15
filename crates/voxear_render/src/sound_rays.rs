//! Rays traced for sound, against the scene the ray-traced GI traces (its
//! instances' triangles, the chunks of worlds of blocks and the terrain's
//! tiles), by the GPU's ray queries where it has them and in a compute
//! shader where it does not. The game asks with [`SoundRays`] before a
//! frame; the first camera's view records them in a pass of its own,
//! "Sound rays", and what they found comes back a few frames later, once
//! the GPU has finished that frame, as [`SoundHeard`].
//!
//! The shaders are in `shaders/sound_rays.slang`:
//!
//! - **occlusion**: rays from the listener to points across each source's
//!   sphere. How many meet no wall, and of those that do, how much of each
//!   band of pitch the walls' acoustic materials let through.
//! - **reverb**: rays from a spot, the listener's or a reverb probe's
//!   being baked, bounced round the scene, each surface sending back to
//!   the spot what it hears of the sound, into the spot's energy field
//!   (see `voxear_audio::sound_field`).
//!
//! What a surface is made of, for sound, is its material's acoustic preset,
//! or a chunk of blocks' own, or a guess from how it looks (see
//! [`MaterialDesc::acoustic`](crate::MaterialDesc::acoustic)). The presets'
//! numbers come with each request, so the renderer holds none of its own.

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Gpu, MemoryLocation, vk};

use crate::{gpu::Push, graph::FrameGraph, passes::bind};

/// The most spots a frame fills an energy field for: the listener and as
/// many probes again of a bake.
pub const MAX_SOUND_SPOTS: usize = 9;
/// The most sources a frame traces occlusion for.
pub const MAX_SOUND_SOURCES: usize = 64;
/// The most rays across one source's sphere.
pub const MAX_SOURCE_RAYS: u32 = 16;
/// The acoustic presets a request may give.
pub const SOUND_MATERIALS: usize = 16;
/// The numbers in one energy field: four channels of three bands of 200
/// bins of 10 ms.
pub const SOUND_FIELD: usize = 4 * 3 * 200;
/// The whole numbers the GPU adds energy up in, a unit.
pub const SOUND_FIXED: f32 = 16_777_216.0;
/// The most rays a spot traces a frame.
pub const MAX_SOUND_RAYS: u32 = 16_384;

const JOB_SIZE: usize = 16 + 16 * MAX_SOUND_SPOTS + 32 * MAX_SOUND_SOURCES + 32 * SOUND_MATERIALS + 32;
const HEARD_SIZE: usize = 4 * SOUND_FIELD * MAX_SOUND_SPOTS + 16 * MAX_SOUND_SOURCES;

/// One surface's acoustics as the rays hear it, in three bands of pitch:
/// how much it soaks up, how much it scatters rather than reflecting as a
/// mirror does, and how much passes through it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundMaterial {
    pub absorption: [f32; 3],
    pub scattering: f32,
    pub transmission: [f32; 3],
}

/// A source to trace occlusion for: where it is, the radius of the sphere
/// it is heard from, and how many rays cross it (1 for its middle alone).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundSource {
    pub at: Vec3,
    pub radius: f32,
    pub rays: u32,
}

/// What to trace for sound in the next frame. `id` comes back with what is
/// found, to tell which request it answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundRays {
    pub id: u64,
    pub listener: Vec3,
    pub sources: Vec<SoundSource>,
    /// The spots to fill an energy field for, each with `rays` rays
    /// bounced `bounces` times, followed for `seconds`.
    pub spots: Vec<Vec3>,
    pub rays: u32,
    pub bounces: u32,
    pub seconds: f32,
    /// The acoustic presets, by number.
    pub materials: Vec<SoundMaterial>,
}

/// What a frame's sound rays found, for the request `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundHeard {
    pub id: u64,
    /// For each source asked of, in order: how many of its rays met no
    /// wall, and of those that did, what they brought through in each band,
    /// added up, each 1 for all of it.
    pub sources: Vec<(u32, [f32; 3])>,
    /// For each spot asked of, in order, its energy field in the GPU's
    /// whole numbers, [`SOUND_FIXED`] a unit.
    pub fields: Vec<Vec<i32>>,
    /// How many rays each spot traced.
    pub rays: u32,
}

/// A frame slot's buffers and the request it carries.
struct Slot {
    job: Buffer,
    heard: Buffer,
    carrying: Option<SoundRays>,
}

/// The renderer's side: the request for the next frame, each frame slot's
/// buffers, and what came back last.
#[derive(Default)]
pub(crate) struct SoundRayWork {
    asked: Option<SoundRays>,
    slots: Vec<Option<Slot>>,
    heard: Option<SoundHeard>,
}

/// What the first view records: where its request and results are, and
/// how much to dispatch.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SoundPass {
    job: u64,
    heard: (vk::Buffer, u64),
    sources: u32,
    spots: u32,
    rays: u32,
}

/// The pipelines a view traces sound with: by ray queries, or in software.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SoundPipelines {
    pub occlusion: vk::Pipeline,
    pub reverb: vk::Pipeline,
}

impl SoundRayWork {
    pub fn ask(&mut self, rays: Option<SoundRays>) {
        self.asked = rays;
    }

    pub fn take_heard(&mut self) -> Option<SoundHeard> {
        self.heard.take()
    }

    /// Reads what slot `slot` carried, now its frame has finished.
    pub fn finish(&mut self, slot: usize) {
        let Some(Some(held)) = self.slots.get_mut(slot) else { return };
        let Some(asked) = held.carrying.take() else { return };
        let bytes = held.heard.read();
        let numbers: &[i32] = bytemuck::cast_slice(&bytes[..HEARD_SIZE]);
        let (fields, sources) = numbers.split_at(SOUND_FIELD * MAX_SOUND_SPOTS);
        let sources = sources
            .chunks_exact(4)
            .take(asked.sources.len())
            .map(|s| (s[0].max(0) as u32, [s[1], s[2], s[3]].map(|n| n as f32 / SOUND_FIXED)))
            .collect();
        let fields = fields.chunks_exact(SOUND_FIELD).take(asked.spots.len()).map(<[i32]>::to_vec).collect();
        self.heard = Some(SoundHeard { id: asked.id, sources, fields, rays: asked.rays });
    }

    /// Writes the request into slot `slot`'s buffers, for the first view to
    /// record; none without one. It is carried only once recorded.
    pub fn prepare(&mut self, gpu: &Gpu, slot: usize) -> Result<Option<SoundPass>> {
        let Some(asked) = &self.asked else { return Ok(None) };
        if self.slots.is_empty() {
            self.slots = (0..FRAMES_IN_FLIGHT).map(|_| None).collect();
        }
        if self.slots[slot].is_none() {
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let job =
                gpu.create_buffer(&format!("sound rays {slot}"), JOB_SIZE as u64, storage, MemoryLocation::CpuToGpu)?;
            let usage = storage | vk::BufferUsageFlags::TRANSFER_DST;
            let heard =
                gpu.create_buffer(&format!("sound heard {slot}"), HEARD_SIZE as u64, usage, MemoryLocation::GpuToCpu)?;
            self.slots[slot] = Some(Slot { job, heard, carrying: None });
        }
        let held = self.slots[slot].as_ref().expect("made above");
        let (bytes, pass) = job_bytes(asked);
        held.job.write(0, &bytes);
        Ok(Some(SoundPass { job: held.job.address, heard: (held.heard.handle, held.heard.address), ..pass }))
    }

    /// The request was recorded into slot `slot`'s frame.
    pub fn recorded(&mut self, slot: usize) {
        if let (Some(asked), Some(Some(held))) = (self.asked.take(), self.slots.get_mut(slot)) {
            held.carrying = Some(asked);
        }
    }

    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        self.slots.drain(..).flatten().flat_map(|slot| [slot.job, slot.heard]).collect()
    }
}

/// The request as the shader reads it, a `SoundJob`, and what to dispatch.
fn job_bytes(asked: &SoundRays) -> (Vec<u8>, SoundPass) {
    let mut bytes = Vec::with_capacity(JOB_SIZE);
    let mut floats = |values: &[f32]| bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
    let point = |at: Vec3| [at.x, at.y, at.z, 0.0];
    floats(&point(asked.listener));
    let spots = asked.spots.len().min(MAX_SOUND_SPOTS);
    for index in 0..MAX_SOUND_SPOTS {
        floats(&asked.spots.get(index).map_or([0.0; 4], |&at| point(at)));
    }
    let sources = asked.sources.len().min(MAX_SOUND_SOURCES);
    for index in 0..MAX_SOUND_SOURCES {
        let source = asked.sources.get(index);
        let sphere = source.map_or([0.0; 4], |s| [s.at.x, s.at.y, s.at.z, s.radius.max(0.0)]);
        floats(&sphere);
        let rays = source.map_or(0, |s| s.rays.clamp(1, MAX_SOURCE_RAYS));
        floats(&[f32::from_bits(rays), 0.0, 0.0, 0.0]);
    }
    for index in 0..SOUND_MATERIALS {
        let material = asked.materials.get(index).copied().unwrap_or_default();
        let [a, b, c] = material.absorption.map(|v| v.clamp(0.0, 1.0));
        floats(&[a, b, c, material.scattering.clamp(0.0, 1.0)]);
        let [a, b, c] = material.transmission.map(|v| v.clamp(0.0, 1.0));
        floats(&[a, b, c, 0.0]);
    }
    let rays = if spots == 0 { 0 } else { asked.rays.clamp(1, MAX_SOUND_RAYS) };
    let seed = asked.id as u32;
    let words = [spots as u32, sources as u32, rays, asked.bounces.clamp(1, 32), seed];
    floats(&words.map(f32::from_bits));
    floats(&[asked.seconds.clamp(0.1, 2.0), 0.0, 0.0]);
    debug_assert_eq!(bytes.len(), JOB_SIZE);
    let pass = SoundPass { job: 0, heard: (vk::Buffer::null(), 0), sources: sources as u32, spots: spots as u32, rays };
    (bytes, pass)
}

/// Records the frame's sound rays: their results cleared, then the
/// occlusion rays and the reverb rays, read on the CPU once the frame's
/// fence is passed.
pub(crate) fn record(fg: &mut FrameGraph<'_>, pipelines: SoundPipelines, push: Push, pass: SoundPass) {
    let split = |address: u64| [address as u32, (address >> 32) as u32];
    let [job_low, job_high] = split(pass.job);
    let [heard_low, heard_high] = split(pass.heard.1);
    let push = Push { textures: [job_low, job_high, heard_low, heard_high], ..push };
    fg.add_pass("Sound rays", &[], &[], move |ctx| {
        let device = &ctx.gpu.device;
        let barrier = |src_stage, src_access, dst_stage, dst_access| {
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(src_stage)
                .src_access_mask(src_access)
                .dst_stage_mask(dst_stage)
                .dst_access_mask(dst_access)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        };
        unsafe { device.cmd_fill_buffer(ctx.cmd, pass.heard.0, 0, vk::WHOLE_SIZE, 0) };
        barrier(
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
        );
        if pass.sources > 0 {
            bind(ctx, pipelines.occlusion, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { device.cmd_dispatch(ctx.cmd, 1, pass.sources, 1) };
        }
        if pass.spots > 0 && pass.rays > 0 {
            bind(ctx, pipelines.reverb, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { device.cmd_dispatch(ctx.cmd, pass.rays.div_ceil(64), pass.spots, 1) };
        }
        barrier(
            vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::SHADER_STORAGE_WRITE,
            vk::PipelineStageFlags2::HOST,
            vk::AccessFlags2::HOST_READ,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_laid_out_as_the_shader_reads_it() {
        let asked = SoundRays {
            id: 7,
            listener: Vec3::new(1.0, 2.0, 3.0),
            sources: vec![SoundSource { at: Vec3::X, radius: 0.5, rays: 40 }],
            spots: vec![Vec3::ZERO, Vec3::Y],
            rays: 1000,
            bounces: 8,
            seconds: 2.0,
            materials: vec![SoundMaterial { absorption: [0.1, 0.2, 0.3], scattering: 0.05, transmission: [0.5; 3] }],
        };
        let (bytes, pass) = job_bytes(&asked);
        assert_eq!(bytes.len(), JOB_SIZE);
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let float = |at: usize| f32::from_bits(word(at));
        assert_eq!((float(0), float(8)), (1.0, 3.0), "the listener first");
        let sources = 16 + 16 * MAX_SOUND_SPOTS;
        assert_eq!((float(sources), float(sources + 12), word(sources + 16)), (1.0, 0.5, MAX_SOURCE_RAYS));
        let counts = sources + 32 * MAX_SOUND_SOURCES + 32 * SOUND_MATERIALS;
        assert_eq!([word(counts), word(counts + 4), word(counts + 8), word(counts + 12)], [2, 1, 1000, 8]);
        assert_eq!((pass.spots, pass.sources, pass.rays), (2, 1, 1000));
    }
}
