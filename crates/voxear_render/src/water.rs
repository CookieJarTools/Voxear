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
use voxear_core::math::Quat;
use voxear_rhi::{Bindless, Buffer, Gpu, ImageDesc, MemoryLocation, StorageImageId, TextureId, vk};

use crate::{
    gpu::{GpuOcean, GpuWaterBody, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    ocean::{self, CASCADES, CASCADE_SIZES, Complex, GPU_RESOLUTION as N, Ocean, SeaState},
    water_bodies::MAX_WATER_BODIES,
    passes::{attachment, begin_rendering, bind},
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

/// A view's sea: its spectrum, written when the sea changes, and the FFT's
/// rows.
pub(crate) struct OceanBuffers {
    pub spectrum: Buffer,
    pub fft: Buffer,
    /// Each frame in flight's Pools and Dry Areas.
    pub bodies: Vec<Buffer>,
    /// The sea the spectrum is of.
    sea: Option<SeaState>,
    /// Its waves' significant height, in metres.
    wave_height: f32,
    /// Which set of wave images this frame writes.
    parity: bool,
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
        Ok(Self { spectrum, fft, bodies, sea: None, wave_height: 0.0, parity: false })
    }

    pub fn into_buffers(self) -> Vec<Buffer> {
        let mut buffers = vec![self.spectrum, self.fft];
        buffers.extend(self.bodies);
        buffers
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
    size: [u32; 2],
    pub frame: GpuOcean,
    /// Whether the sea is drawn, and how many Pools.
    sea: bool,
    pools: u32,
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
        (extent, time): (vk::Extent2D, f32),
    ) -> Result<Self> {
        let (ocean, level, rotation) = found.unwrap_or((Ocean::default(), 0.0, Quat::IDENTITY));
        let ocean = &ocean;
        let sea = ocean.sea_state(rotation);
        buffers.bodies[slot].write(0, &bodies.bodies);
        buffers.prepare(gpu, &sea);
        buffers.parity = !buffers.parity;
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let mut desc = ImageDesc::new(N as u32, N as u32, WAVE_FORMAT, usage).with_mips();
        desc.mip_levels = WAVE_MIPS;
        let mut wave = |name: &str, cascade: usize, set: bool| {
            let set = if set { "a" } else { "b" };
            fg.history_image(gpu, bindless, &format!("ocean {name} {cascade} {set}"), desc)
        };
        let (now, then) = (buffers.parity, !buffers.parity);
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
            dries: bodies.bodies.len() as u32 - bodies.pools,
            _pad: [0; 3],
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
            size: [extent.width, extent.height],
            frame,
            sea: found.is_some(),
            pools: bodies.pools,
        })
    }
}

/// This frame's Pools and then Dry Areas, the nearest the camera first, as
/// the shaders read them.
#[derive(Clone, Debug, Default)]
pub struct WaterBodies {
    pub bodies: Vec<GpuWaterBody>,
    pub pools: u32,
}

/// What the sea's pass draws into and reads besides its own images.
pub struct WaterTargets {
    pub hdr: ImageId,
    pub velocity: ImageId,
    pub depth: ImageId,
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
    let Water { displacement, slopes, previous, fft, copy, copy_texture, copy_mips, depth_copy, size, sea, pools, .. } = water;
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

    let WaterTargets { hdr, velocity, depth, sampled } = targets;
    crate::glass::record_copy(fg, (pipelines, push), (hdr, copy, copy_texture), &copy_mips, size);
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
    images.extend(displacement.iter().chain(&slopes).chain(&previous).map(|&image| (image, ImageAccess::Sampled)));
    images.extend(sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
    let extent = vk::Extent2D { width: size[0], height: size[1] };
    let push = Push { texture: copy_texture.0, textures: [0, depth_copy_texture.0, 0, 0], ..push };
    fg.add_pass("Water", &images, &[], move |ctx| {
        let color = [
            attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
            attachment(ctx.view(velocity), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
        ];
        let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, extent, &color, Some(&depth));
        if sea {
            bind(ctx, pipelines.water, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, QUADS * QUADS * 6, RINGS, 0, 0) };
        }
        if pools > 0 {
            bind(ctx, pipelines.water_pool, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, POOL_QUADS * POOL_QUADS * 6, pools, 0, 0) };
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}
