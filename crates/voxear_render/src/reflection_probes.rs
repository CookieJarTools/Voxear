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
//! Each probe has a resolution of its own, 64 to 512 texels a face; probes of
//! one resolution share an array of cubes, and shading reads each probe from
//! its own. A probe takes its picture from its box's middle, or from a point
//! moved off it, as from a room's middle where the box reaches past a wall.
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

/// How many texels across each face of a probe's cube is, unless it is given
/// another resolution.
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

/// How many texels across each face of a probe's cube is: sharper
/// reflections for more memory and a dearer capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ProbeResolution {
    Size64,
    #[default]
    Size128,
    Size256,
    Size512,
}

impl ProbeResolution {
    pub const ALL: [ProbeResolution; 4] = [Self::Size64, Self::Size128, Self::Size256, Self::Size512];

    /// Texels across a face.
    pub fn texels(self) -> u32 {
        match self {
            Self::Size64 => 64,
            Self::Size128 => 128,
            Self::Size256 => 256,
            Self::Size512 => 512,
        }
    }

    /// The resolution `texels` across, if it is one.
    pub fn from_texels(texels: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|resolution| resolution.texels() == texels)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A Reflection Probe: inside the Obj's unit cube, as its Transform places,
/// turns and sizes it, surfaces reflect what can be seen from its middle, or
/// from its capture point.
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
    /// Texels across each face of its cube.
    pub resolution: ProbeResolution,
    /// Where it takes its picture from, in metres from its box's middle
    /// along the box's own axes: zero for the middle. Box projection still
    /// bends reflections to the box, from this point.
    pub capture_offset: Vec3,
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
            resolution: ProbeResolution::Size128,
            capture_offset: Vec3::ZERO,
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
    /// Its resolution, and its cube in that resolution's array.
    resolution: ProbeResolution,
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
    pub resolution: ProbeResolution,
    pub slot: u32,
    pub kind: JobKind,
}

#[derive(Clone, Copy, Debug)]
pub enum JobKind {
    /// Trace the scene from this point.
    Capture { point: [f32; 3] },
    /// Copy in this frame's staging buffer's baked faces.
    Load { buffer: vk::Buffer },
}

/// This frame's probes as a view takes them.
#[derive(Clone, Copy, Debug)]
pub struct ProbeFrame {
    /// Each resolution's array of cubes, where any probe has it, and the
    /// cube this frame's capture or load is made in first.
    pub cubes: [Option<ImageId>; 4],
    pub source: Option<ImageId>,
    /// What shading reads: how many probes.
    pub gpu: GpuReflectionProbes,
    /// The probes, in the order shading takes them.
    pub list: u64,
    pub job: Option<Job>,
}

/// Every probe the renderer has a cube for, and the buffers that list them.
#[derive(Default)]
pub struct ReflectionProbes {
    known: HashMap<Entity, Known>,
    /// How many cubes each resolution's array holds.
    capacity: [u32; 4],
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
        // A probe whose resolution changed is given a cube in its new one's array, and captured again.
        self.known.retain(|entity, known| {
            probes.iter().any(|(e, probe, ..)| e == entity && probe.resolution == known.resolution)
        });
        if probes.is_empty() {
            return Ok(None);
        }

        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
        let mut cubes = [None; 4];
        for resolution in ProbeResolution::ALL {
            let count = probes.iter().filter(|(_, probe, ..)| probe.resolution == resolution).count() as u32;
            if count == 0 {
                continue;
            }
            let index = resolution.index();
            // Room for every probe, in an array that doubles as it fills; made again, every cube is filled again.
            let wanted = count.next_power_of_two().max(4);
            let refill = |known: &mut HashMap<Entity, Known>| {
                known.values_mut().filter(|k| k.resolution == resolution).for_each(|k| (k.filled, k.bake) = (false, None));
            };
            if wanted > self.capacity[index] {
                self.capacity[index] = wanted;
                refill(&mut self.known);
            }
            let mut used: Vec<bool> = vec![false; self.capacity[index] as usize];
            self.known.values().filter(|k| k.resolution == resolution).for_each(|k| used[k.slot as usize] = true);
            for (entity, probe, ..) in probes.iter().filter(|(_, probe, ..)| probe.resolution == resolution) {
                if !self.known.contains_key(entity) {
                    let free = used.iter().position(|u| !u).expect("room for every probe") as u32;
                    used[free as usize] = true;
                    let known = Known {
                        resolution,
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
            let desc = ImageDesc {
                mip_levels: PROBE_MIPS,
                ..ImageDesc::cube_array(resolution.texels(), self.capacity[index], PROBE_FORMAT, usage)
            };
            let image = fg.history_image(gpu, bindless, &array_name(resolution), desc)?;
            if fg.is_new(image) {
                refill(&mut self.known);
            }
            cubes[index] = Some(image);
        }

        let job = self.pick_job(&probes, (time, frame_index));
        let job = match job {
            Some((entity, resolution, cube, JobChoice::Load(pixels))) => {
                let buffer = self.stage(gpu, slot, &pixels)?;
                Some(Job { entity, resolution, slot: cube, kind: JobKind::Load { buffer } })
            }
            Some((entity, resolution, cube, JobChoice::Capture(point))) => {
                Some(Job { entity, resolution, slot: cube, kind: JobKind::Capture { point: point.into() } })
            }
            None => None,
        };
        self.pending = job;
        let source = match job {
            Some(job) => {
                let usage =
                    vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
                let desc = ImageDesc::cube_array(job.resolution.texels(), 1, PROBE_FORMAT, usage);
                Some(fg.create_image(gpu, bindless, "reflection probe capture", desc)?)
            }
            None => None,
        };

        // The probes that show anything, the most important first, and between equals the smallest, which is the
        // more particular.
        let mut shown: Vec<(i32, f32, GpuReflectionProbe)> = probes
            .iter()
            .filter_map(|(entity, probe, matrix, _)| {
                let known = &self.known[entity];
                let array = cubes[known.resolution.index()].map(|image| fg.texture(image).0)?;
                known.filled.then(|| (probe.importance, volume(matrix), gpu_probe(probe, matrix, (array, known.slot))))
            })
            .collect();
        shown.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)));
        let list = self.list(gpu, slot)?;
        let rows: Vec<GpuReflectionProbe> = shown.iter().map(|(.., probe)| *probe).collect();
        list.write(0, &rows);
        let gpu_probes = GpuReflectionProbes { count: rows.len() as u32, pad: [0; 3] };
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
    ) -> Option<(Entity, ProbeResolution, u32, JobChoice)> {
        let point = |probe: &ReflectionProbe, matrix: &Mat4| capture_point(probe, matrix);
        // A bake never shown, or one that changed, as when baked again.
        for (entity, probe, _, bake) in probes {
            let known = &self.known[entity];
            if let Some(pixels) = bake_of(bake, probe.resolution).filter(|_| probe.update == ProbeUpdate::Once) {
                if !known.filled || known.bake != Some(Arc::as_ptr(pixels) as usize) {
                    return Some((*entity, known.resolution, known.slot, JobChoice::Load(pixels.clone())));
                }
            }
        }
        let settled = |known: &Known| frame_index >= known.appeared + SETTLE_FRAMES;
        for (entity, probe, matrix, _) in probes {
            let known = &self.known[entity];
            if !known.filled && settled(known) {
                return Some((*entity, known.resolution, known.slot, JobChoice::Capture(point(probe, matrix))));
            }
        }
        for (entity, probe, matrix, _) in probes {
            let known = &self.known[entity];
            if known.captures != probe.captures {
                return Some((*entity, known.resolution, known.slot, JobChoice::Capture(point(probe, matrix))));
            }
        }
        let overdue = probes
            .iter()
            .filter(|(_, probe, ..)| probe.update == ProbeUpdate::Live)
            .map(|(entity, probe, matrix, _)| {
                let known = &self.known[entity];
                (entity, known, time - known.captured_at - probe.interval.max(0.0), (probe, matrix))
            })
            .filter(|(_, known, late, _)| *late >= 0.0 && known.filled)
            .max_by(|a, b| a.2.total_cmp(&b.2));
        overdue.map(|(entity, known, _, (probe, matrix))| {
            (*entity, known.resolution, known.slot, JobChoice::Capture(point(probe, matrix)))
        })
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

    /// The resolution and cube a probe's capture is in, if it has one yet.
    pub fn slot_of(&self, entity: Entity) -> Option<(ProbeResolution, u32)> {
        self.known.get(&entity).filter(|known| known.filled).map(|known| (known.resolution, known.slot))
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
        let size = (pixels.texels.len() * 8) as u64;
        if self.staging[slot].as_ref().is_some_and(|buffer| buffer.size < size) {
            gpu.destroy_buffer(self.staging[slot].take().expect("checked above"));
        }
        if self.staging[slot].is_none() {
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

/// A bake the size its probe is now, which can be shown.
fn bake_of(bake: &Option<Arc<ProbePixels>>, resolution: ProbeResolution) -> Option<&Arc<ProbePixels>> {
    let size = resolution.texels();
    bake.as_ref().filter(|pixels| pixels.size == size && pixels.texels.len() == texel_count(size))
}

/// How far from where a probe takes its picture a plant is traced, in metres:
/// past this, plants are small in its picture and many.
pub const PLANT_REACH: f32 = 64.0;
/// The most plants a capture traces.
pub const MOST_PLANTS: usize = 8192;

/// The places in the frame's instances of the plants among `plants` within
/// `PLANT_REACH` of `point`, the nearest first where there are more than a
/// capture traces; `first` is the first plant's place.
pub fn plants_near(plants: &[crate::gpu::GpuInstance], point: Vec3, first: u32) -> Vec<u32> {
    let mut near: Vec<(f32, u32)> = plants
        .iter()
        .enumerate()
        .filter(|(_, plant)| plant.flags & crate::gpu::INSTANCE_IMPOSTOR == 0)
        .map(|(i, plant)| (Vec3::from_slice(&plant.model[3][..3]).distance_squared(point), first + i as u32))
        .filter(|(far, _)| *far < PLANT_REACH * PLANT_REACH)
        .collect();
    if near.len() > MOST_PLANTS {
        near.select_nth_unstable_by(MOST_PLANTS, |a, b| a.0.total_cmp(&b.0));
        near.truncate(MOST_PLANTS);
    }
    near.into_iter().map(|(_, id)| id).collect()
}

/// The name of a resolution's array of cubes, kept from frame to frame.
pub(crate) fn array_name(resolution: ProbeResolution) -> String {
    format!("reflection probes {}", resolution.texels())
}

/// Where a probe takes its picture from, in the world.
fn capture_point(probe: &ReflectionProbe, matrix: &Mat4) -> Vec3 {
    let (x, y, z) = (matrix.x_axis.truncate(), matrix.y_axis.truncate(), matrix.z_axis.truncate());
    let along = |axis: Vec3| axis / axis.length().max(1e-6);
    let offset = probe.capture_offset;
    matrix.w_axis.truncate() + along(x) * offset.x + along(y) * offset.y + along(z) * offset.z
}

enum JobChoice {
    Load(Arc<ProbePixels>),
    Capture(Vec3),
}

/// How many texels a probe's six faces have, `size` across each.
pub fn texel_count(size: u32) -> usize {
    (size * size * 6) as usize
}

fn volume(matrix: &Mat4) -> f32 {
    let (scale, ..) = matrix.to_scale_rotation_translation();
    (scale.x * scale.y * scale.z).abs()
}

/// A probe as shading reads it: its box's middle, axes and half size in
/// metres, from the Obj's unit cube, its capture point, and its cube: the
/// texture of its resolution's array, and its place in it.
fn gpu_probe(probe: &ReflectionProbe, matrix: &Mat4, (cubes, slot): (u32, u32)) -> GpuReflectionProbe {
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
        cubes,
        capture: capture_point(probe, matrix).into(),
        pad: 0,
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
    let (Some(source), Some(cubes)) = (frame.source, frame.cubes[job.resolution.index()]) else { return };
    let size = job.resolution.texels();
    let source_mips = fg.image_desc(source).mip_levels;
    match job.kind {
        JobKind::Capture { point } => {
            let output = fg.storage(source, 0).0;
            let at = [point[0].to_bits(), point[1].to_bits(), point[2].to_bits(), 0];
            let capture = Push { output, size: [size, size], textures: at, ..push };
            let mut images = vec![(source, ImageAccess::StorageWrite)];
            images.extend(sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
            fg.add_pass("Reflection probe capture", &images, &[], move |ctx| {
                bind(ctx, pipelines.capture, vk::PipelineBindPoint::COMPUTE, &capture);
                let groups = size.div_ceil(8);
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
                    width: size,
                    height: size,
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
            let size = (size >> (mip + 1)).max(1);
            let down = Push { texture: *above, output: *output, size: [size, size], ..push };
            bind(ctx, pipelines.downsample, vk::PipelineBindPoint::COMPUTE, &down);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size.div_ceil(8), size.div_ceil(8), 6) };
            compute_barrier(ctx);
        }
    });
    // Each of the probe's mips filtered for its roughness, all from the capture's.
    let texture = fg.texture(source).0;
    let outputs: Vec<u32> = (0..PROBE_MIPS).map(|mip| fg.storage(cubes, mip as usize).0).collect();
    let images = [(source, ImageAccess::Sampled), (cubes, ImageAccess::StorageWrite)];
    fg.add_pass("Reflection probe filter", &images, &[], move |ctx| {
        for (mip, output) in outputs.iter().enumerate() {
            let size = (size >> mip).max(1);
            let filter =
                Push { texture, output: *output, size: [size, size], level: mip as u32, list: job.slot, ..push };
            bind(ctx, pipelines.filter, vk::PipelineBindPoint::COMPUTE, &filter);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size.div_ceil(8), size.div_ceil(8), 6) };
        }
    });
    fg.transition(cubes, ImageAccess::Sampled);
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
    let size = (texel_count(desc.width) * 8) as u64;
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
    Ok(ProbePixels { size: desc.width, texels: texels.context("reading a reflection probe back")? })
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    #[test]
    fn a_probes_box_is_its_objs_unit_cube_turned_and_sized() {
        let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let matrix = Mat4::from_scale_rotation_translation(Vec3::new(8.0, 3.0, 4.0), turn, Vec3::new(1.0, 2.0, 3.0));
        let probe = ReflectionProbe { capture_offset: Vec3::new(1.0, 0.0, 0.0), ..ReflectionProbe::default() };
        let probe = gpu_probe(&probe, &matrix, (9, 5));
        assert_eq!(probe.centre, [1.0, 2.0, 3.0]);
        let half = Vec3::from(probe.half_size);
        assert!(half.distance(Vec3::new(4.0, 1.5, 2.0)) < 1e-5, "{half}");
        let x = Vec3::from(probe.axis_x);
        assert!(x.distance(Vec3::new(0.0, 0.0, -1.0)) < 1e-5, "{x}");
        assert_eq!((probe.cubes, probe.slot, probe.flags), (9, 5, BOX_PROJECTION));
        // Its capture point a metre along its own x, which is turned to the world's -z.
        let capture = Vec3::from(probe.capture);
        assert!(capture.distance(Vec3::new(1.0, 2.0, 2.0)) < 1e-5, "{capture}");
    }
}
