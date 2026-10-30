//! Post-processing, after the sky. Each pass runs only when its effect is on:
//!
//! 1. **Fog** blends over the HDR image.
//! 2. **TAA** resolves the jittered frame against last frame's result.
//! 3. **Depth of field**, then **Motion blur**.
//! 4. **Bloom down** halves the image six times. **Exposure** meters one of
//!    those levels, then **Bloom up** adds each level onto the next larger.
//! 5. **Composite**: exposure, bloom, white balance, grading, tonemapping,
//!    vignette, chromatic aberration and grain.
//! 6. **Finish**: FXAA or sharpening, and dithering, into the output.
//!
//! The shaders are in `shaders/post.slang` and `shaders/exposure.slang`.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    passes::{begin_rendering, bind, fullscreen},
    pipelines::{BLOOM_FORMAT, HDR_FORMAT, LDR_FORMAT, Pipelines},
    post::{AntiAliasingMode, PostProcess},
};

/// Matches `BLOOM_LEVELS` in post.slang, which normalises the sum.
const BLOOM_LEVELS: usize = 6;
const BLOOM_NAMES: [&str; BLOOM_LEVELS] = ["bloom 0", "bloom 1", "bloom 2", "bloom 3", "bloom 4", "bloom 5"];
/// The bloom level exposure meters: an eighth of the screen across, and
/// already filtered, so single bright pixels do not swing the exposure.
const METERED_LEVEL: usize = 2;
const HISTORY_NAMES: [&str; 2] = ["taa history 0", "taa history 1"];

pub struct PostInput {
    pub pipelines: Pipelines,
    pub push: Push,
    pub extent: vk::Extent2D,
    pub settings: PostProcess,
    pub hdr: ImageId,
    pub velocity: ImageId,
    pub depth: ImageId,
    pub output: ImageId,
    /// Auto exposure's adapted EV, which persists between frames.
    pub exposure: BufferId,
    /// Which of the two TAA history images this frame writes; the other
    /// holds last frame's result.
    pub history: usize,
}

pub fn record(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, input: &PostInput) -> Result<()> {
    let settings = &input.settings;
    let (pipelines, push, extent) = (input.pipelines, input.push, input.extent);
    let (hdr, velocity, depth) = (input.hdr, input.velocity, input.depth);
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let full = |format| ImageDesc::new(extent.width, extent.height, format, usage);
    let depth_texture = fg.texture(depth).0;
    let mut scene = hdr;

    if settings.fog.enabled && settings.fog.density > 0.0 {
        let push = Push { textures: [depth_texture, 0, 0, 0], ..push };
        let images = [(depth, ImageAccess::Sampled), (hdr, ImageAccess::ColorAttachment)];
        fg.add_pass("Fog", &images, &[], move |ctx| draw(ctx, pipelines.fog, &push, hdr, true));
    }

    if settings.anti_aliasing.mode == AntiAliasingMode::Taa {
        let current = fg.history_image(gpu, bindless, HISTORY_NAMES[input.history], full(HDR_FORMAT))?;
        let previous = fg.history_image(gpu, bindless, HISTORY_NAMES[1 - input.history], full(HDR_FORMAT))?;
        let textures = [fg.texture(hdr).0, fg.texture(previous).0, fg.texture(velocity).0, depth_texture];
        let push = Push { textures, ..push };
        let images = [
            (hdr, ImageAccess::Sampled),
            (previous, ImageAccess::Sampled),
            (velocity, ImageAccess::Sampled),
            (depth, ImageAccess::Sampled),
            (current, ImageAccess::ColorAttachment),
        ];
        fg.add_pass("TAA", &images, &[], move |ctx| draw(ctx, pipelines.taa, &push, current, false));
        scene = current;
    }

    if settings.depth_of_field.enabled {
        let target = fg.create_image(gpu, bindless, "depth of field", full(HDR_FORMAT))?;
        let push = Push { textures: [fg.texture(scene).0, depth_texture, 0, 0], ..push };
        let images =
            [(scene, ImageAccess::Sampled), (depth, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
        fg.add_pass("Depth of field", &images, &[], move |ctx| draw(ctx, pipelines.dof, &push, target, false));
        scene = target;
    }

    if settings.motion_blur.enabled && settings.motion_blur.intensity > 0.0 {
        let target = fg.create_image(gpu, bindless, "motion blur", full(HDR_FORMAT))?;
        let push = Push { textures: [fg.texture(scene).0, fg.texture(velocity).0, 0, 0], ..push };
        let images =
            [(scene, ImageAccess::Sampled), (velocity, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
        fg.add_pass("Motion blur", &images, &[], move |ctx| draw(ctx, pipelines.motion_blur, &push, target, false));
        scene = target;
    }

    let bloom = settings.bloom.enabled && settings.bloom.intensity > 0.0;
    let metering = settings.exposure.auto;
    let mut levels = Vec::with_capacity(BLOOM_LEVELS);
    if bloom || metering {
        let (mut source, mut size) = (scene, [extent.width, extent.height]);
        for (level, name) in BLOOM_NAMES.iter().enumerate() {
            let half = size.map(|v| (v / 2).max(1));
            let target = fg.create_image(gpu, bindless, name, ImageDesc::new(half[0], half[1], BLOOM_FORMAT, usage))?;
            let push = Push { textures: [fg.texture(source).0, 0, 0, 0], size, level: level as u32, ..push };
            let images = [(source, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
            fg.add_pass("Bloom down", &images, &[], move |ctx| draw(ctx, pipelines.bloom_down, &push, target, false));
            levels.push((target, half));
            (source, size) = (target, half);
        }
    }

    if metering {
        let metered = levels[METERED_LEVEL].0;
        let push = Push { textures: [fg.texture(metered).0, 0, 0, 0], ..push };
        let buffers = [(input.exposure, BufferAccess::ComputeWrite)];
        fg.add_pass("Exposure", &[(metered, ImageAccess::Sampled)], &buffers, move |ctx| {
            bind(ctx, pipelines.exposure, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, 1, 1, 1) };
        });
        fg.sync_buffer(input.exposure, BufferAccess::HostRead);
    }

    if bloom {
        for level in (0..BLOOM_LEVELS - 1).rev() {
            let ((source, size), (target, _)) = (levels[level + 1], levels[level]);
            let push = Push { textures: [fg.texture(source).0, 0, 0, 0], size, ..push };
            let images = [(source, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
            fg.add_pass("Bloom up", &images, &[], move |ctx| draw(ctx, pipelines.bloom_up, &push, target, true));
        }
    }

    // Sharpening and FXAA need the finished image, so they get a pass of their own.
    let finish = settings.anti_aliasing.mode == AntiAliasingMode::Fxaa || settings.anti_aliasing.sharpness > 0.0;
    let composited = if finish { fg.create_image(gpu, bindless, "ldr", full(LDR_FORMAT))? } else { input.output };
    let bloom_texture = if bloom { fg.texture(levels[0].0).0 } else { NO_TEXTURE };
    let push = Push { textures: [fg.texture(scene).0, bloom_texture, 0, 0], output: u32::from(!finish), ..push };
    let mut images = vec![(scene, ImageAccess::Sampled), (composited, ImageAccess::ColorAttachment)];
    if bloom {
        images.push((levels[0].0, ImageAccess::Sampled));
    }
    let buffers = [(input.exposure, BufferAccess::ShaderRead)];
    let composite = if finish { pipelines.composite_ldr } else { pipelines.composite };
    fg.add_pass("Composite", &images, &buffers, move |ctx| draw(ctx, composite, &push, composited, false));

    if finish {
        let output = input.output;
        let push = Push { textures: [fg.texture(composited).0, 0, 0, 0], ..push };
        let images = [(composited, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)];
        fg.add_pass("Finish", &images, &[], move |ctx| draw(ctx, pipelines.finish, &push, output, false));
    }
    Ok(())
}

/// A fullscreen pass into `target`. Passes that blend keep its contents; the
/// rest overwrite every pixel, so nothing needs loading.
fn draw(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId, blend: bool) {
    let load = if blend { vk::AttachmentLoadOp::LOAD } else { vk::AttachmentLoadOp::DONT_CARE };
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load)
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering(ctx, ctx.extent(target), &color, None);
    fullscreen(ctx, pipeline, push);
}
