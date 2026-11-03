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
    passes::{begin_rendering, begin_rendering_in, bind, fullscreen},
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
    /// The view's size, which post-processing after TAA works at.
    pub extent: vk::Extent2D,
    /// The size the scene was drawn at: the view's, or less when TAA
    /// upscales it.
    pub render_extent: vk::Extent2D,
    pub settings: PostProcess,
    pub hdr: ImageId,
    pub velocity: ImageId,
    pub depth: ImageId,
    pub output: ImageId,
    /// Where in the output the view goes, when not all of it. Anything
    /// outside it is cleared.
    pub output_rect: Option<vk::Rect2D>,
    /// What lies outside `output_rect` is cleared to, linear.
    pub clear: [f32; 4],
    /// Whether to clear it: for the first view drawn, not those after it,
    /// which would clear the views before them.
    pub clear_outside: bool,
    /// Auto exposure's adapted EV, which persists between frames.
    pub exposure: BufferId,
    /// Which of the two TAA history images this frame writes; the other
    /// holds last frame's result.
    pub history: usize,
    /// Keep the finished view as an image of its own, for the UI's backdrop
    /// blur to read, as FXAA and sharpening need it anyway.
    pub keep_ldr: bool,
    /// The volumetric fog's integrated volume, laid over the scene in place
    /// of the plain height fog when there is one.
    pub fog_volume: Option<ImageId>,
}

/// Steps the backdrop's blur halves the view's size by.
const BACKDROP_STEPS: usize = 3;
const BACKDROP_NAMES: [&str; BACKDROP_STEPS] = ["backdrop 0", "backdrop 1", "backdrop 2"];

/// Records the post passes; returns the finished view as an image of its own
/// when it is kept (see [`PostInput::keep_ldr`]).
pub fn record(
    fg: &mut FrameGraph<'_>,
    gpu: &Gpu,
    bindless: &mut Bindless,
    input: &PostInput,
) -> Result<Option<ImageId>> {
    let settings = &input.settings;
    let (pipelines, push, extent) = (input.pipelines, input.push, input.extent);
    let (hdr, velocity, depth) = (input.hdr, input.velocity, input.depth);
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let full = |format| ImageDesc::new(extent.width, extent.height, format, usage);
    let render_extent = input.render_extent;
    let drawn = |format| ImageDesc::new(render_extent.width, render_extent.height, format, usage);
    let upscaling = render_extent != extent;
    let depth_texture = fg.texture(depth).0;
    let mut scene = hdr;

    if let Some(volume) = input.fog_volume {
        let push = Push { textures: [depth_texture, fg.texture(volume).0, 0, 0], ..push };
        let images = [(depth, ImageAccess::Sampled), (volume, ImageAccess::Sampled), (hdr, ImageAccess::ColorAttachment)];
        fg.add_pass("Volumetric fog", &images, &[], move |ctx| draw(ctx, pipelines.volumetric_fog, &push, hdr, true));
    } else if settings.fog.enabled && settings.fog.density > 0.0 {
        let push = Push { textures: [depth_texture, 0, 0, 0], ..push };
        let images = [(depth, ImageAccess::Sampled), (hdr, ImageAccess::ColorAttachment)];
        fg.add_pass("Fog", &images, &[], move |ctx| draw(ctx, pipelines.fog, &push, hdr, true));
    }

    // Depth of field and motion blur work on the scene as drawn, before TAA, as Unreal orders them, so
    // when TAA upscales they work on fewer pixels and TAA smooths their noise too.
    if settings.depth_of_field.enabled {
        let target = fg.create_image(gpu, bindless, "depth of field", drawn(HDR_FORMAT))?;
        let push = Push { textures: [fg.texture(scene).0, depth_texture, 0, 0], ..push };
        let images =
            [(scene, ImageAccess::Sampled), (depth, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
        fg.add_pass("Depth of field", &images, &[], move |ctx| draw(ctx, pipelines.dof, &push, target, false));
        scene = target;
    }

    if settings.motion_blur.enabled && settings.motion_blur.intensity > 0.0 {
        let target = fg.create_image(gpu, bindless, "motion blur", drawn(HDR_FORMAT))?;
        let push = Push { textures: [fg.texture(scene).0, fg.texture(velocity).0, 0, 0], ..push };
        let images =
            [(scene, ImageAccess::Sampled), (velocity, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
        fg.add_pass("Motion blur", &images, &[], move |ctx| draw(ctx, pipelines.motion_blur, &push, target, false));
        scene = target;
    }

    if settings.anti_aliasing.mode == AntiAliasingMode::Taa {
        let current = fg.history_image(gpu, bindless, HISTORY_NAMES[input.history], full(HDR_FORMAT))?;
        let previous = fg.history_image(gpu, bindless, HISTORY_NAMES[1 - input.history], full(HDR_FORMAT))?;
        let textures = [fg.texture(scene).0, fg.texture(previous).0, fg.texture(velocity).0, depth_texture];
        let push = Push { textures, ..push };
        let images = [
            (scene, ImageAccess::Sampled),
            (previous, ImageAccess::Sampled),
            (velocity, ImageAccess::Sampled),
            (depth, ImageAccess::Sampled),
            (current, ImageAccess::ColorAttachment),
        ];
        // Upscaling, each output pixel is built from the drawn pixels round it, by where their jittered samples fell.
        let taa = if upscaling { pipelines.taa_upscale } else { pipelines.taa };
        let name = if upscaling { "TAA upscale" } else { "TAA" };
        fg.add_pass(name, &images, &[], move |ctx| draw(ctx, taa, &push, current, false));
        scene = current;
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
    let finish = settings.anti_aliasing.mode == AntiAliasingMode::Fxaa
        || settings.anti_aliasing.sharpness > 0.0
        || input.keep_ldr;
    let composited = if finish { fg.create_image(gpu, bindless, "ldr", full(LDR_FORMAT))? } else { input.output };
    let bloom_texture = if bloom { fg.texture(levels[0].0).0 } else { NO_TEXTURE };
    let push = Push { textures: [fg.texture(scene).0, bloom_texture, 0, 0], output: u32::from(!finish), ..push };
    let mut images = vec![(scene, ImageAccess::Sampled), (composited, ImageAccess::ColorAttachment)];
    if bloom {
        images.push((levels[0].0, ImageAccess::Sampled));
    }
    let buffers = [(input.exposure, BufferAccess::ShaderRead)];
    let composite = if finish { pipelines.composite_ldr } else { pipelines.composite };
    let rect = input.output_rect;
    if rect.is_some() && input.clear_outside {
        let (output, color) = (input.output, input.clear);
        fg.add_pass("Clear output", &[(output, ImageAccess::ColorAttachment)], &[], move |ctx| {
            clear(ctx, output, color)
        });
    }
    let composite_rect = if composited == input.output { rect } else { None };
    let origin = |rect: Option<vk::Rect2D>| rect.map_or([0, 0], |r| [r.offset.x as u32, r.offset.y as u32]);
    let push = Push { size: origin(composite_rect), ..push };
    fg.add_pass("Composite", &images, &buffers, move |ctx| {
        draw_into(ctx, composite, &push, composited, composite_rect)
    });

    if finish {
        let output = input.output;
        let push = Push { textures: [fg.texture(composited).0, 0, 0, 0], size: origin(rect), ..push };
        let images = [(composited, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)];
        fg.add_pass("Finish", &images, &[], move |ctx| draw_into(ctx, pipelines.finish, &push, output, rect));
    }
    Ok(finish.then_some(composited))
}

/// The finished view blurred for UI backdrops (`backdrop-filter`): halved a
/// few times with the same 13-tap filter bloom goes down with, which blurs
/// as it shrinks, so the smallest is soft enough for frosted glass. Returns
/// it with its size.
pub fn blur_for_backdrop(
    fg: &mut FrameGraph<'_>,
    gpu: &Gpu,
    bindless: &mut Bindless,
    (pipelines, push): (Pipelines, Push),
    view: ImageId,
    extent: vk::Extent2D,
) -> Result<(ImageId, [u32; 2])> {
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let (mut source, mut size) = (view, [extent.width, extent.height]);
    for name in BACKDROP_NAMES {
        let half = size.map(|v| (v / 2).max(1));
        let target = fg.create_image(gpu, bindless, name, ImageDesc::new(half[0], half[1], BLOOM_FORMAT, usage))?;
        // Level 1, so the filter does not damp bright pixels as bloom's first step does.
        let push = Push { textures: [fg.texture(source).0, 0, 0, 0], size, level: 1, ..push };
        let images = [(source, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
        fg.add_pass("Backdrop blur", &images, &[], move |ctx| draw(ctx, pipelines.bloom_down, &push, target, false));
        (source, size) = (target, half);
    }
    Ok((source, size))
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

/// A fullscreen pass into part of `target`, or all of it.
fn draw_into(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId, rect: Option<vk::Rect2D>) {
    let Some(rect) = rect else { return draw(ctx, pipeline, push, target, false) };
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering_in(ctx, rect, &color);
    fullscreen(ctx, pipeline, push);
}

/// Clears all of `target` to `color`, before a view drawn into part of it.
pub(crate) fn clear(ctx: &PassContext, target: ImageId, color: [f32; 4]) {
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: color } })
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering(ctx, ctx.extent(target), &color, None);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}
