//! The sea on the GPU (see `ocean` for its waves and `shaders/water.slang`):
//! each frame its four cascades of waves are moved on and summed by an
//! inverse FFT, rows then columns, into each point's movement, folding and
//! slopes, with mips; then, after the opaque scene and the sky and before
//! glass, its surface is drawn over a copy of the picture so far and of its
//! depth, which it is seen through, writing its own depth so fog and depth
//! of field see it.
//!
//! Each view keeps the spectrum and the FFT's rows in buffers of its own,
//! and its waves in two sets of images it takes turns to write, so last
//! frame's are there for motion vectors.

use anyhow::Result;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_rhi::{Bindless, Buffer, Gpu, ImageDesc, MemoryLocation, StorageImageId, TextureId, vk};

use crate::{
    assets::Assets,
    gpu::{GpuInstance, GpuOcean, GpuStreamPoint, GpuWaterBody, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    ocean::{self, CASCADES, CASCADE_SIZES, Complex, GPU_RESOLUTION as N, Ocean, SeaState},
    water_bodies::{MAX_STREAM_POINTS, MAX_WATER_BODIES},
    passes::{attachment, begin_rendering, bind, set_viewport},
    pipelines::{HDR_FORMAT, Pipelines},
};

/// The waves' images: each point's movement and folding, and its slopes and
/// their squares.
const WAVE_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// Mips down to one texel, for the waves seen far off.
const WAVE_MIPS: u32 = 9;
/// How many quads across a ring of the sea's grid, and how many rings (as
/// `WATER_QUADS` and `WATER_RINGS` in `shaders/water.slang`).
const QUADS: u32 = 64;
const RINGS: u32 = 12;
/// A Pool's grid, as `POOL_QUADS` in `shaders/water.slang`.
const POOL_QUADS: u32 = 64;
/// How many quads across a Stream, as `STREAM_ACROSS` in `shaders/water.slang`.
const STREAM_ACROSS: u32 = 8;
/// The sea floor round the camera, seen from straight above: this many
/// slices, each this many texels a side, the first this many metres across
/// and each twice the last, its heights kept within this many metres of
/// the sea's level; as `FLOOR_*` in `shaders/water.slang`. The slices share
/// one depth image, two a side.
const FLOOR_SLICES: usize = 4;
const FLOOR_TEXELS: u32 = 256;
const FLOOR_FIRST: f32 = 64.0;
/// How many frames each slice goes before it is drawn again even if it has
/// not moved, so ground moved in the editor shows up.
const FLOOR_REFRESH: u32 = 120;
/// The foam round the camera: this many texels a side, this many metres
/// each, as `FOAM_TEXELS` and `FOAM_TEXEL` in `shaders/water.slang`.
const FOAM_TEXELS: u32 = 512;
const FOAM_TEXEL: f32 = 0.5;
/// The tiling pattern foam is made of, this many texels a side, as
/// `FOAM_PATTERN` in `shaders/water.slang`; made once, with mips.
const FOAM_PATTERN: u32 = 512;

/// A view's sea: its spectrum, written when the sea changes, and the FFT's
/// rows.
pub(crate) struct OceanBuffers {
    pub spectrum: Buffer,
    pub fft: Buffer,
    /// Each frame in flight's Pools, Streams and Dry Areas, and the points
    /// along its Streams.
    pub bodies: Vec<Buffer>,
    pub stream_points: Vec<Buffer>,
    /// The sea the spectrum is of.
    sea: Option<SeaState>,
    /// Its waves' significant height, in metres.
    wave_height: f32,
    /// Which set of wave images this frame writes.
    parity: bool,
    /// Where last frame's foam's first texel was, if there was any.
    foam_origin: Option<[f32; 2]>,
    /// Whether the lens's wetness has a frame before this one.
    lens_kept: bool,
    /// Where each slice of the sea floor was last drawn round, the sea's
    /// level then, and frames since the first.
    floor_origins: [Option<[f32; 2]>; FLOOR_SLICES],
    floor_level: Option<f32>,
    floor_frames: u32,
}

impl OceanBuffers {
    pub fn new(gpu: &Gpu, named: &dyn Fn(&str) -> String, slots: usize) -> Result<Self> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let waves = (CASCADES * N * N) as u64;
        let spectrum = gpu.create_buffer(&named("ocean spectrum"), waves * 16, usage, MemoryLocation::CpuToGpu)?;
        let fft = gpu.create_buffer(&named("ocean fft"), waves * 4 * 8, usage, MemoryLocation::GpuOnly)?;
        let size = (MAX_WATER_BODIES * size_of::<GpuWaterBody>()) as u64;
        let bodies = (0..slots)
            .map(|slot| gpu.create_buffer(&named(&format!("water bodies {slot}")), size, usage, MemoryLocation::CpuToGpu))
            .collect::<Result<_>>()?;
        let size = (MAX_STREAM_POINTS * size_of::<GpuStreamPoint>()) as u64;
        let stream_points = (0..slots)
            .map(|slot| gpu.create_buffer(&named(&format!("stream points {slot}")), size, usage, MemoryLocation::CpuToGpu))
            .collect::<Result<_>>()?;
        Ok(Self { spectrum, fft, bodies, stream_points, sea: None, wave_height: 0.0, parity: false, foam_origin: None, lens_kept: false, floor_origins: [None; FLOOR_SLICES], floor_level: None, floor_frames: 0 })
    }

    pub fn into_buffers(self) -> Vec<Buffer> {
        let mut buffers = vec![self.spectrum, self.fft];
        buffers.extend(self.bodies);
        buffers.extend(self.stream_points);
        buffers
    }

    /// Where each slice of the sea floor lies round `camera`, as the shaders
    /// read them (corner x and z, size, and 1), and which to draw this
    /// frame with what stands in them: those that moved with the camera, or
    /// whose turn it is to be drawn again, or all of them when the sea's
    /// level changed.
    fn floor_slices(&mut self, camera: Vec3, level: f32, ground: &[Ground]) -> ([[f32; 4]; FLOOR_SLICES], Vec<(u32, Vec<Ground>)>) {
        let moved_level = self.floor_level != Some(level);
        self.floor_level = Some(level);
        self.floor_frames = self.floor_frames.wrapping_add(1);
        let mut slices = [[0.0; 4]; FLOOR_SLICES];
        let mut draws = Vec::new();
        for (slice, kept) in self.floor_origins.iter_mut().enumerate() {
            let size = FLOOR_FIRST * (1 << slice) as f32;
            let texel = size / FLOOR_TEXELS as f32;
            let origin = [(camera.x / texel).floor() * texel - size * 0.5, (camera.z / texel).floor() * texel - size * 0.5];
            let due = self.floor_frames % FLOOR_REFRESH == slice as u32 * FLOOR_REFRESH / FLOOR_SLICES as u32;
            if *kept != Some(origin) || moved_level || due {
                *kept = Some(origin);
                // Only what reaches down into the water: a box hanging over the sea is no ground under it.
                let inside = |g: &&Ground| {
                    g.low.y < level
                        && g.high.x >= origin[0]
                        && g.low.x <= origin[0] + size
                        && g.high.z >= origin[1]
                        && g.low.z <= origin[1] + size
                };
                draws.push((slice as u32, ground.iter().filter(inside).copied().collect()));
            }
            slices[slice] = [origin[0], origin[1], size, 1.0];
        }
        (slices, draws)
    }

    /// Writes the spectrum of `sea`, when it is not the one written already.
    fn prepare(&mut self, gpu: &Gpu, sea: &SeaState) {
        if self.sea.as_ref() == Some(sea) {
            return;
        }
        let mut waves = Vec::with_capacity(CASCADES * N * N);
        let mut energy = 0.0;
        for cascade in 0..CASCADES {
            let spectrum = ocean::initial_spectrum(sea, cascade, N);
            for at in 0..N * N {
                let (n, m) = (at % N, at / N);
                let h = spectrum[at];
                let opposite = spectrum[(N - n) % N + (N - m) % N * N];
                energy += h.re * h.re + h.im * h.im;
                let partner = Complex::new(opposite.re, -opposite.im);
                waves.push([h.re as f32, h.im as f32, partner.re as f32, partner.im as f32]);
            }
        }
        // Frames in flight read it.
        gpu.wait_idle();
        self.spectrum.write(0, &waves);
        // Each wave and its mirror, h0(k) and h0*(-k), add up to twice the energy of the h0s alone.
        self.wave_height = 4.0 * (2.0 * energy).sqrt() as f32;
        self.sea = Some(*sea);
    }
}

/// The sea in a frame: its settings, and its images.
#[derive(Clone)]
pub struct Water {
    displacement: [ImageId; CASCADES],
    slopes: [ImageId; CASCADES],
    previous: [ImageId; CASCADES],
    fft: BufferId,
    copy: ImageId,
    copy_texture: TextureId,
    copy_mips: Vec<StorageImageId>,
    depth_copy: ImageId,
    /// A copy of the picture and depth again after the sea, for the Pools
    /// and Streams drawn over it, when there are any.
    second: Option<(ImageId, TextureId, Vec<StorageImageId>, ImageId)>,
    size: [u32; 2],
    pub frame: GpuOcean,
    /// The foam round the camera: this frame's, last frame's, and where last
    /// frame's first texel was (NaN for none).
    foam: (ImageId, ImageId, [f32; 2]),
    /// How wet the camera's lens is: this frame's, last frame's, and
    /// whether there was a last frame's.
    lens: (ImageId, ImageId, bool),
    /// The sea floor, and the slices drawn into it this frame, with what
    /// stands in each.
    floor: (ImageId, Vec<(u32, Vec<Ground>)>),
    /// The pattern foam is made of, and whether it is to be made this frame.
    pattern: (ImageId, bool),
    /// Whether the sea is drawn, how many Pools, how many points each
    /// Stream has, and how many Dry Areas.
    sea: bool,
    pools: u32,
    streams: Vec<u32>,
    dries: u32,
}

impl Water {
    /// The first Ocean's sea, at `level`, its wind along `rotation`, if
    /// there is one, else a sea drawn nowhere whose small waves the Pools
    /// take; its waves at `time`: its spectrum written if the sea changed,
    /// this frame's Pools and Dry Areas written into `slot`'s buffer, and
    /// this frame's images.
    pub(crate) fn new(
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        (buffers, slot): (&mut OceanBuffers, usize),
        (found, bodies): (Option<(Ocean, f32, Quat)>, &WaterBodies),
        (extent, time, camera): (vk::Extent2D, f32, voxear_core::math::Vec3),
        ground: &[Ground],
    ) -> Result<Self> {
        let (ocean, level, rotation) = found.unwrap_or((Ocean::default(), 0.0, Quat::IDENTITY));
        let ocean = &ocean;
        let sea = ocean.sea_state(rotation);
        buffers.bodies[slot].write(0, &bodies.bodies);
        buffers.stream_points[slot].write(0, &bodies.points);
        buffers.prepare(gpu, &sea);
        buffers.parity = !buffers.parity;
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let mut desc = ImageDesc::new(N as u32, N as u32, WAVE_FORMAT, usage).with_mips();
        desc.mip_levels = WAVE_MIPS;
        let foam_desc = ImageDesc::new(FOAM_TEXELS, FOAM_TEXELS, vk::Format::R32_SFLOAT, usage);
        let (foam_a, foam_b) = (fg.history_image(gpu, bindless, "ocean foam a", foam_desc)?, fg.history_image(gpu, bindless, "ocean foam b", foam_desc)?);
        let (foam_now, foam_then) = if buffers.parity { (foam_a, foam_b) } else { (foam_b, foam_a) };
        let lens_desc = ImageDesc::new(1, 1, vk::Format::R32_SFLOAT, usage);
        let (lens_a, lens_b) = (fg.history_image(gpu, bindless, "lens wetness a", lens_desc)?, fg.history_image(gpu, bindless, "lens wetness b", lens_desc)?);
        let (lens_now, lens_then) = if buffers.parity { (lens_a, lens_b) } else { (lens_b, lens_a) };
        let lens_kept = std::mem::replace(&mut buffers.lens_kept, true);
        let floor_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let floor_desc = ImageDesc::new(FLOOR_TEXELS * 2, FLOOR_TEXELS * 2, crate::pipelines::DEPTH_FORMAT, floor_usage);
        let floor_image = fg.history_image(gpu, bindless, "sea floor", floor_desc)?;
        let (floor, floor_draws) = buffers.floor_slices(camera, level, ground);
        let pattern_desc = ImageDesc::new(FOAM_PATTERN, FOAM_PATTERN, WAVE_FORMAT, usage).with_mips();
        let pattern = fg.history_image(gpu, bindless, "foam pattern", pattern_desc)?;
        let pattern_new = fg.is_new(pattern);
        let mut wave = |name: &str, cascade: usize, set: bool| {
            let set = if set { "a" } else { "b" };
            fg.history_image(gpu, bindless, &format!("ocean {name} {cascade} {set}"), desc)
        };
        let (now, then) = (buffers.parity, !buffers.parity);
        // Round the camera, on whole texels, so the foam does not swim as it follows.
        let snap = |v: f32| (v / FOAM_TEXEL).floor() * FOAM_TEXEL - FOAM_TEXELS as f32 * FOAM_TEXEL * 0.5;
        let foam_origin = [snap(camera.x), snap(camera.z)];
        let previous_origin = buffers.foam_origin.replace(foam_origin).unwrap_or([f32::NAN; 2]);
        let (mut displacement, mut previous, mut slopes) = (Vec::new(), Vec::new(), Vec::new());
        for cascade in 0..CASCADES {
            displacement.push(wave("displacement", cascade, now)?);
            previous.push(wave("displacement", cascade, then)?);
            slopes.push(wave("slopes", cascade, now)?);
        }
        let cascades = |images: Vec<ImageId>| -> [ImageId; CASCADES] { images.try_into().expect("one a cascade") };
        let (displacement, previous, slopes) = (cascades(displacement), cascades(previous), cascades(slopes));
        let mut copy_desc = ImageDesc::new(extent.width, extent.height, HDR_FORMAT, usage).with_mips();
        copy_desc.mip_levels = copy_desc.mip_levels.min(7);
        let copy = fg.create_image(gpu, bindless, "water copy", copy_desc)?;
        let depth_desc = ImageDesc::new(extent.width, extent.height, vk::Format::R32_SFLOAT, usage);
        let depth_copy = fg.create_image(gpu, bindless, "water depth copy", depth_desc)?;
        let second = match bodies.pools > 0 || !bodies.streams.is_empty() {
            true => {
                let copy = fg.create_image(gpu, bindless, "pool copy", copy_desc)?;
                let depth = fg.create_image(gpu, bindless, "pool depth copy", depth_desc)?;
                let mips = (0..copy_desc.mip_levels as usize).map(|mip| fg.storage(copy, mip)).collect();
                Some((copy, fg.texture(copy), mips, depth))
            }
            false => None,
        };

        let linear = |srgb: [f32; 3]| srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        let absorb = linear(ocean.clarity).map(|t| -t.max(1e-3).ln() * ocean.murk.max(0.0));
        let texture = |id: ImageId| fg.texture(id).0;
        let frame = GpuOcean {
            sizes: CASCADE_SIZES,
            displacement: displacement.map(texture),
            slopes: slopes.map(texture),
            previous: previous.map(texture),
            deep: linear(ocean.deep),
            level,
            scatter: linear(ocean.scatter),
            foam: (ocean.foam * ocean.weather.foam()).max(0.0),
            absorb,
            choppiness: sea.choppiness,
            wave_height: buffers.wave_height,
            enabled: u32::from(found.is_some()),
            time: f64::from(time).rem_euclid(ocean::REPEAT_SECONDS) as f32,
            pools: bodies.pools,
            dries: bodies.dries(),
            foam_texture: fg.texture(foam_now).0,
            foam_origin,
            streams: bodies.streams.len() as u32,
            floor_texture: fg.texture(floor_image).0,
            foam_pattern: fg.texture(pattern).0,
            _pad: 0,
            floor,
        };
        Ok(Self {
            displacement,
            slopes,
            previous,
            fft: fg.import_buffer(&buffers.fft),
            copy,
            copy_texture: fg.texture(copy),
            copy_mips: (0..copy_desc.mip_levels as usize).map(|mip| fg.storage(copy, mip)).collect(),
            depth_copy,
            second,
            foam: (foam_now, foam_then, previous_origin),
            lens: (lens_now, lens_then, lens_kept),
            floor: (floor_image, floor_draws),
            pattern: (pattern, pattern_new),
            size: [extent.width, extent.height],
            frame,
            sea: found.is_some(),
            pools: bodies.pools,
            streams: bodies.streams.clone(),
            dries: bodies.dries(),
        })
    }
}

/// How many mips the foam pattern has, down to one texel.
fn pattern_desc_mips() -> usize {
    (32 - FOAM_PATTERN.leading_zeros()) as usize
}

/// What the sea floor is drawn from: an opaque instance standing still, where
/// its first LOD's indices are, and its box in the world.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    instance: u32,
    first_index: u32,
    index_count: u32,
    low: Vec3,
    high: Vec3,
}

/// The instances that are ground under the water: all of `instances` but
/// what moved since last frame, as a boat or a floating crate does.
pub fn ground(instances: &[GpuInstance], assets: &Assets) -> Vec<Ground> {
    instances
        .iter()
        .enumerate()
        .filter(|(_, instance)| instance.model == instance.prev_model)
        .filter_map(|(index, instance)| {
            let mesh = crate::MeshHandle(instance.mesh);
            let (first_index, index_count) = assets.first_lod(mesh)?;
            let (min, max) = assets.mesh_bounds(mesh)?;
            let model = Mat4::from_cols_array_2d(&instance.model);
            let corners = (0..8).map(|i| {
                let pick = |bit: usize, a: f32, b: f32| if i & bit == 0 { a } else { b };
                model.transform_point3(Vec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z)))
            });
            let (low, high) = corners.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), c| (lo.min(c), hi.max(c)));
            Some(Ground { instance: index as u32, first_index, index_count, low, high })
        })
        .collect()
}

/// This frame's Pools, Streams and then Dry Areas, the nearest the camera
/// first, as the shaders read them, and the points along its Streams: how
/// many each has, in order.
#[derive(Clone, Debug, Default)]
pub struct WaterBodies {
    pub bodies: Vec<GpuWaterBody>,
    pub pools: u32,
    pub streams: Vec<u32>,
    pub points: Vec<GpuStreamPoint>,
}

impl WaterBodies {
    pub fn dries(&self) -> u32 {
        self.bodies.len() as u32 - self.pools - self.streams.len() as u32
    }
}

/// What the sea's pass draws into and reads besides its own images.
pub struct WaterTargets {
    pub hdr: ImageId,
    pub velocity: ImageId,
    pub depth: ImageId,
    /// Every mesh's indices, for drawing the sea floor.
    pub index_buffer: vk::Buffer,
    /// What shading reads: the shadow atlas, the GI's probes and the
    /// atmosphere's tables.
    pub sampled: Vec<ImageId>,
}

fn compute_barrier(ctx: &crate::graph::PassContext) {
    let barrier = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_SAMPLED_READ)];
    let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
    unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
}

/// Records the sea's waves (the FFT, then the mips), then its surface over
/// a copy of the picture and the depth so far.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), water: Water, targets: WaterTargets) {
    let Water { displacement, slopes, previous, fft, copy, copy_texture, copy_mips, depth_copy, second, foam, lens, floor, pattern, size, sea, pools, streams, dries, .. } = water;
    let (floor_image, floor_draws) = floor;
    // The pattern foam is made of, once, with its mips.
    let (pattern, pattern_new) = pattern;
    if pattern_new {
        let mips: Vec<StorageImageId> = (0..pattern_desc_mips()).map(|mip| fg.storage(pattern, mip)).collect();
        let texture = fg.texture(pattern);
        fg.add_pass("Foam pattern", &[(pattern, ImageAccess::StorageWrite)], &[], move |ctx| {
            let push = Push { output: mips[0].0, ..push };
            bind(ctx, pipelines.foam_pattern, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, FOAM_PATTERN / 8, FOAM_PATTERN / 8, 1) };
            for mip in 1..mips.len() as u32 {
                compute_barrier(ctx);
                let side = (FOAM_PATTERN >> mip).max(1);
                let push = Push { texture: texture.0, level: mip - 1, output: mips[mip as usize].0, size: [side, side], ..push };
                bind(ctx, pipelines.ocean_mip, vk::PipelineBindPoint::COMPUTE, &push);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, side.div_ceil(8), side.div_ceil(8), 1) };
            }
        });
    }
    let WaterTargets { hdr, velocity, depth, index_buffer, sampled } = targets;
    // The ground under the water, from straight above, into the slices that moved or are due.
    if !floor_draws.is_empty() {
        fg.add_pass("Sea floor", &[(floor_image, ImageAccess::DepthAttachment)], &[], move |ctx| {
            let depth = attachment(ctx.view(floor_image), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
            let side = vk::Extent2D { width: FLOOR_TEXELS * 2, height: FLOOR_TEXELS * 2 };
            begin_rendering(ctx, side, &[], Some(&depth));
            bind(ctx, pipelines.sea_floor, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32) };
            for (slice, items) in &floor_draws {
                let offset = vk::Offset2D { x: (slice % 2 * FLOOR_TEXELS) as i32, y: (slice / 2 * FLOOR_TEXELS) as i32 };
                let extent = vk::Extent2D { width: FLOOR_TEXELS, height: FLOOR_TEXELS };
                set_viewport(ctx, offset, extent);
                // Nothing there until something is drawn: far below the sea.
                let clear = vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::DEPTH)
                    .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } });
                let rect = vk::ClearRect { rect: vk::Rect2D { offset, extent }, base_array_layer: 0, layer_count: 1 };
                unsafe { ctx.gpu.device.cmd_clear_attachments(ctx.cmd, &[clear], &[rect]) };
                for g in items {
                    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: g.instance, level: *slice, ..push });
                    unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, g.index_count, 1, g.first_index, 0, 0) };
                }
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    let storage = |fg: &mut FrameGraph<'_>, images: &[ImageId; CASCADES]| -> Vec<Vec<StorageImageId>> {
        images.iter().map(|&image| (0..WAVE_MIPS as usize).map(|mip| fg.storage(image, mip)).collect()).collect()
    };
    let (moved_mips, slope_mips) = (storage(fg, &displacement), storage(fg, &slopes));
    let textures: Vec<(TextureId, TextureId)> =
        displacement.iter().zip(&slopes).map(|(&d, &s)| (fg.texture(d), fg.texture(s))).collect();
    let images: Vec<(ImageId, ImageAccess)> =
        displacement.iter().chain(&slopes).map(|&image| (image, ImageAccess::StorageWrite)).collect();
    fg.add_pass("Ocean waves", &images, &[(fft, BufferAccess::ComputeWrite)], move |ctx| {
        // Rows, then columns into the images' first mips.
        for cascade in 0..CASCADES as u32 {
            bind(ctx, pipelines.ocean_rows, vk::PipelineBindPoint::COMPUTE, &Push { level: cascade, ..push });
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, N as u32, 1, 1) };
        }
        compute_barrier(ctx);
        for cascade in 0..CASCADES {
            let push = Push {
                level: cascade as u32,
                output: moved_mips[cascade][0].0,
                textures: [slope_mips[cascade][0].0, 0, 0, 0],
                ..push
            };
            bind(ctx, pipelines.ocean_columns, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, N as u32, 1, 1) };
        }
        // Each mip from the one before, for every image at once.
        for mip in 1..WAVE_MIPS {
            compute_barrier(ctx);
            let side = (N as u32 >> mip).max(1);
            for cascade in 0..CASCADES {
                for (texture, mips) in [(textures[cascade].0, &moved_mips[cascade]), (textures[cascade].1, &slope_mips[cascade])] {
                    let push = Push {
                        texture: texture.0,
                        level: mip - 1,
                        output: mips[mip as usize].0,
                        size: [side, side],
                        ..push
                    };
                    bind(ctx, pipelines.ocean_mip, vk::PipelineBindPoint::COMPUTE, &push);
                    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, side.div_ceil(8), side.div_ceil(8), 1) };
                }
            }
        }
    });

    // The foam round the camera: last frame's carried with the water, and what breaks now.
    let (foam_now, foam_then, previous_origin) = foam;
    let foam_storage = fg.storage(foam_now, 0);
    let foam_then_texture = fg.texture(foam_then);
    let mut foam_images =
        vec![(foam_now, ImageAccess::StorageWrite), (foam_then, ImageAccess::Sampled), (floor_image, ImageAccess::Sampled)];
    foam_images.extend(displacement.iter().map(|&image| (image, ImageAccess::Sampled)));
    fg.add_pass("Ocean foam", &foam_images, &[], move |ctx| {
        let [x, z] = previous_origin.map(f32::to_bits);
        let push = Push { output: foam_storage.0, textures: [foam_then_texture.0, x, z, 0], ..push };
        bind(ctx, pipelines.ocean_foam, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, FOAM_TEXELS / 8, FOAM_TEXELS / 8, 1) };
    });

    let mut waves: Vec<ImageId> = displacement.iter().chain(&slopes).chain(&previous).copied().collect();
    waves.extend([foam_now, floor_image, pattern]);
    let layer = Layer { hdr, velocity, depth, sampled, waves, size };
    // The sea and the walls round its Dry Areas, over a copy of the picture so far.
    let first_dry = pools + streams.len() as u32;
    let draws = Draws { sea, pools: 0, streams: Vec::new(), walls: (first_dry, dries) };
    record_layer(fg, (pipelines, push), &layer, (copy, copy_texture, &copy_mips, depth_copy), draws);
    // The Pools, their walls and the Streams over a copy made after the sea, so the sea shows through them.
    if let Some((copy, copy_texture, copy_mips, depth_copy)) = second {
        let streams = streams.iter().enumerate().map(|(i, &points)| (pools + i as u32, points)).collect();
        let draws = Draws { sea: false, pools, streams, walls: (0, pools) };
        record_layer(fg, (pipelines, push), &layer, (copy, copy_texture, &copy_mips, depth_copy), draws);
    }
    // How wet the lens is: soaked under water, drying once out.
    let (lens_now, lens_then, lens_kept) = lens;
    let lens_storage = fg.storage(lens_now, 0);
    let (lens_now_texture, lens_then_texture) = (fg.texture(lens_now), fg.texture(lens_then));
    let mut lens_images = vec![(lens_now, ImageAccess::StorageWrite), (lens_then, ImageAccess::Sampled)];
    lens_images.extend(layer.waves.iter().map(|&image| (image, ImageAccess::Sampled)));
    fg.add_pass("Lens wetness", &lens_images, &[], move |ctx| {
        let push = Push { output: lens_storage.0, textures: [lens_then_texture.0, u32::from(!lens_kept), 0, 0], ..push };
        bind(ctx, pipelines.lens_wetness, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, 1, 1, 1) };
    });
    // Under water, across the waterline and through drops on the lens: over a copy of the picture with the water
    // in it, to bend and blur.
    crate::glass::record_copy(fg, (pipelines, push), (hdr, copy, copy_texture), &copy_mips, size);
    let mut images = vec![
        (hdr, ImageAccess::ColorAttachment),
        (copy, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (lens_now, ImageAccess::Sampled),
    ];
    images.extend(layer.waves.iter().map(|&image| (image, ImageAccess::Sampled)));
    images.extend(layer.sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
    let depth_texture = fg.texture(depth);
    let push = Push { texture: copy_texture.0, textures: [depth_texture.0, lens_now_texture.0, 0, 0], ..push };
    let extent = vk::Extent2D { width: size[0], height: size[1] };
    fg.add_pass("Under water", &images, &[], move |ctx| {
        let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, extent, &color, None);
        bind(ctx, pipelines.underwater, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0) };
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// What a layer of water draws into and reads.
struct Layer {
    hdr: ImageId,
    velocity: ImageId,
    depth: ImageId,
    sampled: Vec<ImageId>,
    waves: Vec<ImageId>,
    size: [u32; 2],
}

/// What a layer of water draws: the sea, how many Pools, each Stream's body
/// and how many points it has, and the walls of the bodies from the first
/// given, how many.
struct Draws {
    sea: bool,
    pools: u32,
    streams: Vec<(u32, u32)>,
    walls: (u32, u32),
}

/// Copies the picture and its depth, then draws a layer of water over them.
fn record_layer(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    layer: &Layer,
    (copy, copy_texture, copy_mips, depth_copy): (ImageId, TextureId, &[StorageImageId], ImageId),
    draws: Draws,
) {
    let Layer { hdr, velocity, depth, size, .. } = *layer;
    crate::glass::record_copy(fg, (pipelines, push), (hdr, copy, copy_texture), copy_mips, size);
    let depth_storage = fg.storage(depth_copy, 0);
    let depth_texture = fg.texture(depth);
    let depth_copy_texture = fg.texture(depth_copy);
    let copy_images = [(depth, ImageAccess::Sampled), (depth_copy, ImageAccess::StorageWrite)];
    fg.add_pass("Water depth copy", &copy_images, &[], move |ctx| {
        let push = Push { texture: depth_texture.0, output: depth_storage.0, size, ..push };
        bind(ctx, pipelines.water_depth_copy, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
    });

    let mut images = vec![
        (hdr, ImageAccess::ColorAttachment),
        (velocity, ImageAccess::ColorAttachment),
        (depth, ImageAccess::DepthAttachment),
        (copy, ImageAccess::Sampled),
        (depth_copy, ImageAccess::Sampled),
    ];
    images.extend(layer.waves.iter().map(|&image| (image, ImageAccess::Sampled)));
    images.extend(layer.sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
    let extent = vk::Extent2D { width: size[0], height: size[1] };
    let push = Push { texture: copy_texture.0, textures: [0, depth_copy_texture.0, 0, 0], ..push };
    fg.add_pass("Water", &images, &[], move |ctx| {
        let color = [
            attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
            attachment(ctx.view(velocity), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
        ];
        let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, extent, &color, Some(&depth));
        if draws.sea {
            bind(ctx, pipelines.water, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, QUADS * QUADS * 6, RINGS, 0, 0) };
        }
        if draws.pools > 0 {
            bind(ctx, pipelines.water_pool, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, POOL_QUADS * POOL_QUADS * 6, draws.pools, 0, 0) };
        }
        // Each Stream a strip along its points.
        for &(body, points) in draws.streams.iter().filter(|(_, points)| *points > 1) {
            bind(ctx, pipelines.water_stream, vk::PipelineBindPoint::GRAPHICS, &Push { list: body, level: points, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, (points - 1) * STREAM_ACROSS * 6, 1, 0, 0) };
        }
        // Walls of water: six faces of each body's box.
        let (first, count) = draws.walls;
        if count > 0 {
            bind(ctx, pipelines.water_wall, vk::PipelineBindPoint::GRAPHICS, &Push { list: first, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 36, count, 0, 0) };
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}
