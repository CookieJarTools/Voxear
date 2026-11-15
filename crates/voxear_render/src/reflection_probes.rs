//! Reflection probes: a box in the level whose surfaces reflect what can be
//! seen from its middle, as a cube map captured on the GPU, rather than the
//! sky. A room's probe shows its walls in a polished floor, and in a rough
//! floor their blurred light, where the sky would light them as if outdoors.
//!
//! Capturing traces a ray from the probe's middle through each texel of its
//! six faces against the scene the GI's rays see (by the GPU's ray queries,
//! or the compute shader where it has none), lighting what each meets as
//! the GI's rays do: the sun through a shadow ray, the lamps, glow, and the
//! GI's light. The faces are filtered down into mips, then each mip is
//! filtered with GGX for its roughness (Karis 2013, with the lower mips read
//! where a sample covers more than a texel, after Colbert and Křivánek
//! 2007), into the probe's place in one array of cubes.
//!
//! A probe is captured once, as the level opens, or live, every so often.
//! One captured once can be baked: read back and saved beside the level (a
//! `.probes` file, which the engine loads and builds ship), so a game opens
//! with it ready and the same every time. Until it is captured again, it
//! stays as it was, whatever moves in front of it.
//!
//! One probe is captured or loaded a frame, the most needed first.
//!
//! The shaders are in `shaders/reflection_probes.slang` (shading),
//! `shaders/probe_filter.slang` (the mips) and `probeCaptureMain` in
//! `shaders/ddgi.slang`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, Gpu, ImageDesc, MemoryLocation};
use voxear_scene::{Destroying, Switch, Switched, Transform, WorldTransform};

use crate::gpu::{GpuReflectionProbe, GpuReflectionProbes, Push};
use crate::graph::{FrameGraph, ImageAccess, ImageId};
use crate::passes::bind;

/// How many texels across each face of a probe's cube is.
pub const PROBE_SIZE: u32 = 128;
/// The mips a probe's cube keeps, from mirror-smooth (the face as captured)
/// to fully rough; as `REFLECTION_PROBE_MIPS` in the shaders.
pub const PROBE_MIPS: u32 = 6;
/// The most reflection probes a frame shades with.
pub const MAX_PROBES: usize = 64;
/// The format of a probe's texels: four half floats, as a baked probe is
/// given.
pub const PROBE_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const BOX_PROJECTION: u32 = 1;
/// Frames a probe waits after it appears before it is captured, so the GI's
/// light has settled and the scene's top level is built.
const SETTLE_FRAMES: u64 = crate::ddgi::WARM_UP_FRAMES as u64 + 4;

/// When a probe is captured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProbeUpdate {
    /// Once, as the level opens, or from what was baked beside it; again
    /// only when asked.
    #[default]
    Once,
    /// Again and again as the game goes, every `interval` seconds.
    Live,
}

/// A Reflection Probe: inside the Obj's unit cube, as its Transform places,
/// turns and sizes it, surfaces reflect what can be seen from its middle.
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectionProbe {
    pub update: ProbeUpdate,
    /// A live probe's seconds between captures; 0 captures every frame.
    pub interval: f32,
    /// How far outside its box, in metres, it fades out into what lies
    /// round it; 0 for a hard edge. Its box's own sides, as a room's walls
    /// and floor where the box fits the room, show it fully.
    pub blend_distance: f32,
    /// Bend each reflection to where it meets the box, so a room's probe
    /// lines up with its walls from anywhere in it. Best for a box the size
    /// of the room; off for a probe out in the open.
    pub box_projection: bool,
    /// How bright what it shows is, 1 as captured.
    pub intensity: f32,
    /// Where boxes overlap, the higher first; between equals, the smaller.
    pub importance: i32,
    /// Asks for it to be captured again: each time this changes, it is.
    /// Not saved.
    pub captures: u32,
}

impl Component for ReflectionProbe {}

impl Default for ReflectionProbe {
    fn default() -> Self {
        Self {
            update: ProbeUpdate::Once,
            interval: 0.0,
            blend_distance: 1.0,
            box_projection: true,
            intensity: 1.0,
            importance: 0,
            captures: 0,
        }
    }
}

impl ReflectionProbe {
    /// Captures it again, next frame or soon after.
    pub fn capture(&mut self) {
        self.captures = self.captures.wrapping_add(1);
    }
}

/// A probe's six faces as captured, `size` texels square each, as four half
/// floats a texel (red, green, blue, unused), face after face in the order
/// +x, -x, +y, -y, +z, -z, rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbePixels {
    pub size: u32,
    pub texels: Vec<[u16; 4]>,
}

/// What a baked probe was captured as, which the engine puts on its Obj from
/// the level's `.probes` file: shown in place of capturing it.
#[derive(Clone, Debug)]
pub struct ReflectionProbeBake(pub Arc<ProbePixels>);

impl Component for ReflectionProbeBake {}

/// What the renderer knows of one probe.
struct Known {
    slot: u32,
    /// Its cube holds what it shows.
    filled: bool,
    /// The frame it appeared in.
    appeared: u64,
    /// When it was last captured, in seconds.
    captured_at: f32,
    /// Its `captures` when it was last captured.
    captures: u32,
    /// The bake it had when it was last filled, by address: a bake it has
    /// since been given is loaded in place of what its cube holds.
    bake: Option<usize>,
}

/// This frame's capture or load, the view that traces the scene records it.
#[derive(Clone, Copy, Debug)]
pub struct Job {
    pub entity: Entity,
    pub slot: u32,
    pub kind: JobKind,
}

#[derive(Clone, Copy, Debug)]
pub enum JobKind {
    /// Trace the scene from this point.
    Capture { centre: [f32; 3] },
    /// Copy in this frame's staging buffer's baked faces.
    Load { buffer: vk::Buffer },
}

/// This frame's probes as a view takes them.
#[derive(Clone, Copy, Debug)]
pub struct ProbeFrame {
    /// The array of cubes, and the cube a capture or load is made in first.
    pub cubes: ImageId,
    pub source: ImageId,
    /// What shading reads: how many probes, and the cubes' texture.
    pub gpu: GpuReflectionProbes,
    /// The probes, in the order shading takes them.
    pub list: u64,
    pub job: Option<Job>,
}

/// Every probe the renderer has a cube for, and the buffers that list them.
#[derive(Default)]
pub struct ReflectionProbes {
    known: HashMap<Entity, Known>,
    /// How many cubes the array holds.
    capacity: u32,
    /// Each frame in flight's list of probes, and staging buffer for a load.
    lists: Vec<Buffer>,
    staging: Vec<Option<Buffer>>,
    /// The job given out this frame, until a view records it.
    pending: Option<Job>,
}

impl ReflectionProbes {
    /// Looks over the world's probes, gives each a cube, picks this frame's
    /// capture or load, and lists those that show anything for shading.
    /// Without any probes, there is nothing for the views to do.
    pub fn prepare(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut voxear_rhi::Bindless),
        world: &World,
        (time, frame_index, slot): (f32, u64, usize),
    ) -> Result<Option<ProbeFrame>> {
        self.pending = None;
        let switched = Switched::of(world);
        let query = world
            .query_ref::<(Entity, &ReflectionProbe, &Transform, Option<&WorldTransform>, Option<&ReflectionProbeBake>)>(
            )
            .without::<Destroying>();
        let mut probes: Vec<(Entity, ReflectionProbe, Mat4, Option<Arc<ProbePixels>>)> = query
            .into_iter()
            .filter(|(entity, ..)| switched.is_on(world, *entity, Switch::ReflectionProbe))
            .map(|(entity, probe, local, placed, bake)| {
                let matrix = placed.map_or_else(|| local.matrix(), |placed| placed.0);
                (entity, probe.clone(), matrix, bake.map(|bake| bake.0.clone()))
            })
            .collect();
        probes.truncate(MAX_PROBES);
        self.known.retain(|entity, _| probes.iter().any(|(e, ..)| e == entity));
        if probes.is_empty() {
            return Ok(None);
        }

        // Room for every probe, in an array that doubles as it fills; made again, every cube is filled again.
        let wanted = (probes.len() as u32).next_power_of_two().max(4);
        if wanted > self.capacity {
            self.capacity = wanted;
            self.known.values_mut().for_each(|known| (known.filled, known.bake) = (false, None));
        }
        let mut used: Vec<bool> = vec![false; self.capacity as usize];
        self.known.values().for_each(|known| used[known.slot as usize] = true);
        for (entity, probe, ..) in &probes {
            if !self.known.contains_key(entity) {
                let free = used.iter().position(|u| !u).expect("room for every probe") as u32;
                used[free as usize] = true;
                let known = Known {
                    slot: free,
                    filled: false,
                    appeared: frame_index,
                    captured_at: time,
                    captures: probe.captures,
                    bake: None,
                };
                self.known.insert(*entity, known);
            }
        }

        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
        let cubes_desc = ImageDesc {
            mip_levels: PROBE_MIPS,
            ..ImageDesc::cube_array(PROBE_SIZE, self.capacity, PROBE_FORMAT, usage)
        };
        let cubes = fg.history_image(gpu, bindless, "reflection probes", cubes_desc)?;
        if fg.is_new(cubes) {
            self.known.values_mut().for_each(|known| (known.filled, known.bake) = (false, None));
        }
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
        let source = fg.create_image(
            gpu,
            bindless,
            "reflection probe capture",
            ImageDesc::cube_array(PROBE_SIZE, 1, PROBE_FORMAT, usage),
        )?;

        let job = self.pick_job(&probes, (time, frame_index));
        let job = match job {
            Some((entity, cube, JobChoice::Load(pixels))) => {
                let buffer = self.stage(gpu, slot, &pixels)?;
                Some(Job { entity, slot: cube, kind: JobKind::Load { buffer } })
            }
            Some((entity, cube, JobChoice::Capture(centre))) => {
                Some(Job { entity, slot: cube, kind: JobKind::Capture { centre: centre.into() } })
            }
            None => None,
        };
        self.pending = job;

        // The probes that show anything, the most important first, and between equals the smallest, which is the
        // more particular.
        let mut shown: Vec<(i32, f32, GpuReflectionProbe)> = probes
            .iter()
            .filter_map(|(entity, probe, matrix, _)| {
                let known = &self.known[entity];
                known.filled.then(|| (probe.importance, volume(matrix), gpu_probe(probe, matrix, known.slot)))
            })
            .collect();
        shown.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)));
        let list = self.list(gpu, slot)?;
        let rows: Vec<GpuReflectionProbe> = shown.iter().map(|(.., probe)| *probe).collect();
        list.write(0, &rows);
        let texture = fg.texture(cubes).0;
        let gpu_probes = GpuReflectionProbes { count: rows.len() as u32, cubes: texture, pad: [0; 2] };
        Ok(Some(ProbeFrame { cubes, source, gpu: gpu_probes, list: list.address, job }))
    }

    /// The probe most in need, and what to do for it: first a probe that
    /// shows nothing yet, loading its bake or, once the scene has settled,
    /// capturing it; then one asked to capture again; then the live probe
    /// longest overdue.
    fn pick_job(
        &self,
        probes: &[(Entity, ReflectionProbe, Mat4, Option<Arc<ProbePixels>>)],
        (time, frame_index): (f32, u64),
    ) -> Option<(Entity, u32, JobChoice)> {
        let centre = |matrix: &Mat4| matrix.transform_point3(Vec3::ZERO);
        // A bake never shown, or one that changed, as when baked again.
        for (entity, probe, _, bake) in probes {
            let known = &self.known[entity];
            if let Some(pixels) = bake_of(bake).filter(|_| probe.update == ProbeUpdate::Once) {
                if !known.filled || known.bake != Some(Arc::as_ptr(pixels) as usize) {
                    return Some((*entity, known.slot, JobChoice::Load(pixels.clone())));
                }
            }
        }
        let settled = |known: &Known| frame_index >= known.appeared + SETTLE_FRAMES;
        for (entity, _, matrix, _) in probes {
            let known = &self.known[entity];
            if !known.filled && settled(known) {
                return Some((*entity, known.slot, JobChoice::Capture(centre(matrix))));
            }
        }
        for (entity, probe, matrix, _) in probes {
            let known = &self.known[entity];
            if known.captures != probe.captures {
                return Some((*entity, known.slot, JobChoice::Capture(centre(matrix))));
            }
        }
        let overdue = probes
            .iter()
            .filter(|(_, probe, ..)| probe.update == ProbeUpdate::Live)
            .map(|(entity, probe, matrix, _)| {
                let known = &self.known[entity];
                (entity, known, time - known.captured_at - probe.interval.max(0.0), matrix)
            })
            .filter(|(_, known, late, _)| *late >= 0.0 && known.filled)
            .max_by(|a, b| a.2.total_cmp(&b.2));
        overdue.map(|(entity, known, _, matrix)| (*entity, known.slot, JobChoice::Capture(centre(matrix))))
    }

    /// The job this frame's first view is to record, once: after that,
    /// none, so no other view records it again.
    pub fn take_job(&mut self, world: &World, time: f32) -> Option<Job> {
        let job = self.pending.take()?;
        if let Some(known) = self.known.get_mut(&job.entity) {
            known.filled = true;
            known.captured_at = time;
            if let Some(probe) = world.get::<ReflectionProbe>(job.entity) {
                known.captures = probe.captures;
            }
            known.bake = world.get::<ReflectionProbeBake>(job.entity).map(|bake| Arc::as_ptr(&bake.0) as usize);
        }
        Some(job)
    }

    /// The `captures` a probe had when its cube was last filled, if it has
    /// been.
    pub fn captured(&self, entity: Entity) -> Option<u32> {
        self.known.get(&entity).filter(|known| known.filled).map(|known| known.captures)
    }

    /// The cube a probe's capture is in, if it has one yet.
    pub fn slot_of(&self, entity: Entity) -> Option<u32> {
        self.known.get(&entity).filter(|known| known.filled).map(|known| known.slot)
    }

    fn list(&mut self, gpu: &Gpu, slot: usize) -> Result<&Buffer> {
        while self.lists.len() <= slot {
            let size = (MAX_PROBES * size_of::<GpuReflectionProbe>()) as u64;
            let name = format!("reflection probes {}", self.lists.len());
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
            self.lists.push(gpu.create_buffer(&name, size, usage, MemoryLocation::CpuToGpu)?);
        }
        Ok(&self.lists[slot])
    }

    fn stage(&mut self, gpu: &Gpu, slot: usize, pixels: &ProbePixels) -> Result<vk::Buffer> {
        while self.staging.len() <= slot {
            self.staging.push(None);
        }
        if self.staging[slot].is_none() {
            let size = (texel_count() * 8) as u64;
            let usage = vk::BufferUsageFlags::TRANSFER_SRC;
            let buffer = gpu.create_buffer("reflection probe load", size, usage, MemoryLocation::CpuToGpu)?;
            self.staging[slot] = Some(buffer);
        }
        let buffer = self.staging[slot].as_ref().expect("made above");
        buffer.write(0, &pixels.texels);
        Ok(buffer.handle)
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        for buffer in self.lists.drain(..).chain(self.staging.drain(..).flatten()) {
            gpu.destroy_buffer(buffer);
        }
    }
}

/// A bake the size probes are now, which can be shown.
fn bake_of(bake: &Option<Arc<ProbePixels>>) -> Option<&Arc<ProbePixels>> {
    bake.as_ref().filter(|pixels| pixels.size == PROBE_SIZE && pixels.texels.len() == texel_count())
}

enum JobChoice {
    Load(Arc<ProbePixels>),
    Capture(Vec3),
}

/// How many texels a probe's six faces have.
pub fn texel_count() -> usize {
    (PROBE_SIZE * PROBE_SIZE * 6) as usize
}

fn volume(matrix: &Mat4) -> f32 {
    let (scale, ..) = matrix.to_scale_rotation_translation();
    (scale.x * scale.y * scale.z).abs()
}

/// A probe as shading reads it: its box's middle, axes and half size in
/// metres, from the Obj's unit cube.
fn gpu_probe(probe: &ReflectionProbe, matrix: &Mat4, slot: u32) -> GpuReflectionProbe {
    let axis = |column: Vec3| {
        let length = column.length();
        (column / length.max(1e-6), length * 0.5)
    };
    let (x, hx) = axis(matrix.x_axis.truncate());
    let (y, hy) = axis(matrix.y_axis.truncate());
    let (z, hz) = axis(matrix.z_axis.truncate());
    GpuReflectionProbe {
        centre: matrix.w_axis.truncate().into(),
        blend: probe.blend_distance.max(0.0),
        axis_x: x.into(),
        intensity: probe.intensity.max(0.0),
        axis_y: y.into(),
        slot,
        axis_z: z.into(),
        flags: if probe.box_projection { BOX_PROJECTION } else { 0 },
        half_size: [hx, hy, hz],
        pad: 0.0,
    }
}

/// The pipelines a probe's capture and mips take.
#[derive(Clone, Copy)]
pub struct ProbePipelines {
    /// The capture, by the GPU's rays or in software.
    pub capture: vk::Pipeline,
    pub downsample: vk::Pipeline,
    pub filter: vk::Pipeline,
}

/// Records this frame's job: the probe's faces captured (or its bake copied
/// in) into `frame.source`, its mips filtered down, then each of the probe's
/// own mips filtered with GGX for its roughness into its place in the cubes.
/// The capture's rays read `sampled` as the GI's do: the lights' shadow maps,
/// and the GI's own maps.
pub fn record(
    fg: &mut FrameGraph<'_>,
    pipelines: ProbePipelines,
    push: Push,
    frame: &ProbeFrame,
    job: Job,
    sampled: &[ImageId],
) {
    let source = frame.source;
    let source_mips = fg.image_desc(source).mip_levels;
    match job.kind {
        JobKind::Capture { centre } => {
            let output = fg.storage(source, 0).0;
            let at = [centre[0].to_bits(), centre[1].to_bits(), centre[2].to_bits(), 0];
            let capture = Push { output, size: [PROBE_SIZE, PROBE_SIZE], textures: at, ..push };
            let mut images = vec![(source, ImageAccess::StorageWrite)];
            images.extend(sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
            fg.add_pass("Reflection probe capture", &images, &[], move |ctx| {
                bind(ctx, pipelines.capture, vk::PipelineBindPoint::COMPUTE, &capture);
                let groups = PROBE_SIZE.div_ceil(8);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, groups, 6) };
            });
        }
        JobKind::Load { buffer } => {
            fg.add_pass("Reflection probe load", &[(source, ImageAccess::TransferDst)], &[], move |ctx| {
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .layer_count(6);
                let region = vk::BufferImageCopy::default().image_subresource(layers).image_extent(vk::Extent3D {
                    width: PROBE_SIZE,
                    height: PROBE_SIZE,
                    depth: 1,
                });
                let layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
                unsafe {
                    ctx.gpu.device.cmd_copy_buffer_to_image(ctx.cmd, buffer, ctx.image(source), layout, &[region])
                };
            });
        }
    }
    // Each of the capture's mips the average of the one above it.
    let mips: Vec<(u32, u32)> = (1..source_mips)
        .map(|mip| (fg.storage(source, mip as usize - 1).0, fg.storage(source, mip as usize).0))
        .collect();
    fg.add_pass("Reflection probe mips", &[(source, ImageAccess::StorageWrite)], &[], move |ctx| {
        for (mip, (above, output)) in mips.iter().enumerate() {
            let size = (PROBE_SIZE >> (mip + 1)).max(1);
            let down = Push { texture: *above, output: *output, size: [size, size], ..push };
            bind(ctx, pipelines.downsample, vk::PipelineBindPoint::COMPUTE, &down);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size.div_ceil(8), size.div_ceil(8), 6) };
            compute_barrier(ctx);
        }
    });
    // Each of the probe's mips filtered for its roughness, all from the capture's.
    let texture = fg.texture(source).0;
    let outputs: Vec<u32> = (0..PROBE_MIPS).map(|mip| fg.storage(frame.cubes, mip as usize).0).collect();
    let images = [(source, ImageAccess::Sampled), (frame.cubes, ImageAccess::StorageWrite)];
    fg.add_pass("Reflection probe filter", &images, &[], move |ctx| {
        for (mip, output) in outputs.iter().enumerate() {
            let size = (PROBE_SIZE >> mip).max(1);
            let filter =
                Push { texture, output: *output, size: [size, size], level: mip as u32, list: job.slot, ..push };
            bind(ctx, pipelines.filter, vk::PipelineBindPoint::COMPUTE, &filter);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size.div_ceil(8), size.div_ceil(8), 6) };
        }
    });
    fg.transition(frame.cubes, ImageAccess::Sampled);
}

fn compute_barrier(ctx: &crate::graph::PassContext) {
    let barrier = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ)];
    let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
    unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
}

/// Reads a probe's captured faces back from the GPU, as a bake saves them.
/// Waits for the GPU.
pub fn read_back(gpu: &Gpu, (image, desc): (vk::Image, ImageDesc), slot: u32) -> Result<ProbePixels> {
    gpu.wait_idle();
    let size = (texel_count() * 8) as u64;
    let buffer =
        gpu.create_buffer("reflection probe read", size, vk::BufferUsageFlags::TRANSFER_DST, MemoryLocation::GpuToCpu)?;
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(slot * 6)
        .layer_count(6);
    let region = vk::BufferImageCopy::default().image_subresource(layers).image_extent(vk::Extent3D {
        width: desc.width,
        height: desc.height,
        depth: 1,
    });
    let copied = gpu.immediate(|cmd| unsafe {
        let barrier = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)];
        gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&barrier));
        gpu.device.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::GENERAL, buffer.handle, &[region]);
    });
    let texels = copied.map(|()| bytemuck::cast_slice::<u8, [u16; 4]>(&buffer.read()[..size as usize]).to_vec());
    gpu.destroy_buffer(buffer);
    Ok(ProbePixels { size: PROBE_SIZE, texels: texels.context("reading a reflection probe back")? })
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    #[test]
    fn a_probes_box_is_its_objs_unit_cube_turned_and_sized() {
        let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let matrix = Mat4::from_scale_rotation_translation(Vec3::new(8.0, 3.0, 4.0), turn, Vec3::new(1.0, 2.0, 3.0));
        let probe = gpu_probe(&ReflectionProbe::default(), &matrix, 5);
        assert_eq!(probe.centre, [1.0, 2.0, 3.0]);
        assert_eq!(probe.half_size, [4.0, 1.5, 2.0]);
        let x = Vec3::from(probe.axis_x);
        assert!(x.distance(Vec3::new(0.0, 0.0, -1.0)) < 1e-5, "{x}");
        assert_eq!((probe.slot, probe.flags), (5, BOX_PROJECTION));
    }
}
