//! Post-processing, after the sky. Each pass runs only when its effect is on:
//!
//! 1. **Fog** blends over the HDR image.
//! 2. **TAA** resolves the jittered frame against last frame's result.
//! 3. **Depth of field**, then **Motion blur**.
//! 4. **Bloom down** halves the image six times. **Exposure** meters one of
//!    those levels, then **Bloom up** adds each level onto the next larger.
//! 5. **Composite**: exposure, bloom, white balance, grading, tonemapping,
//!    vignette and chromatic aberration.
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
    shader_packs::PackPoint,
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
    /// The view under the water, drawn after the fog, so the water's own fog
    /// covers it.
    pub under_water: Option<crate::water::UnderWater>,
    /// The particles, drawn after the fog, each fogged as far off as it is,
    /// and before the view under the water.
    pub particles: Option<crate::particles::Draw>,
    /// The smokes, laid over the scene after the fog and before the
    /// particles.
    pub smoke: Option<crate::smoke::Draw>,
    /// Each pixel's normal and roughness, when the depth passes wrote them,
    /// which TAA reads to tell mirrors by (`post.surface_texture`).
    pub surface: Option<ImageId>,
    /// AMD FSR 3.1, to take TAA's place where the settings choose it and the
    /// GPU can run it.
    pub fsr3: Option<Fsr3Input>,
    /// A worn shader pack's passes after the fog, after TAA and over the
    /// finished picture.
    pub pack: Option<crate::shader_packs::PackRecorder>,
}

/// What FSR 3.1 needs of a view beyond the post-processing's own inputs.
pub struct Fsr3Input {
    pub pipelines: crate::fsr3::Fsr3Pipelines,
    /// The view's FSR images, made or remade here at the sizes it is drawn
    /// and shown at, and those it no longer needs, for the renderer to free.
    pub state: std::sync::Arc<std::sync::Mutex<Option<crate::fsr3::Fsr3State>>>,
    pub retired: std::sync::Arc<std::sync::Mutex<Vec<crate::fsr3::Fsr3State>>>,
    pub jitter: [f32; 2],
    pub fov_y: f32,
    pub near: f32,
    pub delta_time: f32,
    pub reset: bool,
    pub slot: usize,
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
    if let Some(pack) = &input.pack {
        pack.record(fg, PackPoint::AfterFog, hdr, (None, None));
    }
    // FSR's reactive mask is made from the scene before and after what is drawn over it.
    let fsr_chosen = input.fsr3.is_some()
        && settings.anti_aliasing.mode == AntiAliasingMode::Taa
        && settings.anti_aliasing.upscaler == crate::post::Upscaler::Fsr;
    let drawn_over = input.smoke.is_some() || input.particles.is_some() || input.under_water.is_some();
    let opaque = match fsr_chosen && drawn_over {
        true => {
            let copy = fg.create_image(gpu, bindless, "FSR opaque only", drawn(HDR_FORMAT))?;
            let push = Push { textures: [fg.texture(hdr).0, 0, 0, 0], ..push };
            let images = [(hdr, ImageAccess::Sampled), (copy, ImageAccess::ColorAttachment)];
            fg.add_pass("FSR opaque copy", &images, &[], move |ctx| draw(ctx, pipelines.fsr_copy, &push, copy, false));
            Some(copy)
        }
        false => None,
    };
    if let Some(smoke) = input.smoke.clone() {
        let target = (smoke, hdr, render_extent, input.history);
        crate::smoke::record(fg, (gpu, bindless), (pipelines, push), target, input.fog_volume)?;
    }
    if let Some(particles) = input.particles.clone() {
        crate::particles::record(fg, (gpu, bindless), (pipelines, push), particles, input.fog_volume)?;
    }
    if let Some(under) = input.under_water.clone() {
        crate::water::record_under(fg, (pipelines, push), under);
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

    let fsr3 = input.fsr3.as_ref().filter(|_| {
        settings.anti_aliasing.mode == AntiAliasingMode::Taa && settings.anti_aliasing.upscaler == crate::post::Upscaler::Fsr
    });
    if let Some(fsr3) = fsr3 {
        scene = record_fsr3(fg, gpu, bindless, input, fsr3, (scene, velocity, depth), opaque)?;
    } else if settings.anti_aliasing.mode == AntiAliasingMode::Taa {
        let current = taa_history(fg, gpu, bindless, extent, input.history)?;
        let previous = taa_history(fg, gpu, bindless, extent, 1 - input.history)?;
        let textures = [fg.texture(scene).0, fg.texture(previous).0, fg.texture(velocity).0, depth_texture];
        let push = Push { textures, ..push };
        let mut images = vec![
            (scene, ImageAccess::Sampled),
            (previous, ImageAccess::Sampled),
            (velocity, ImageAccess::Sampled),
            (depth, ImageAccess::Sampled),
            (current, ImageAccess::ColorAttachment),
        ];
        images.extend(input.surface.map(|surface| (surface, ImageAccess::Sampled)));
        // Upscaling, each output pixel is built from the drawn pixels round it, by where their jittered samples fell.
        let taa = if upscaling { pipelines.taa_upscale } else { pipelines.taa };
        let name = if upscaling { "TAA upscale" } else { "TAA" };
        fg.add_pass(name, &images, &[], move |ctx| draw(ctx, taa, &push, current, false));
        scene = current;
    }

    if let Some(pack) = &input.pack {
        scene = pack.record(fg, PackPoint::AfterTaa, scene, (None, None));
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

    // Sharpening and FXAA need the finished image, so they get a pass of their own, as a shader pack's passes over
    // the finished picture do.
    let pack_final = input.pack.as_ref().filter(|pack| pack.runs_at(PackPoint::Final));
    // A pack's pass that writes the colour at the tonemap takes the composite's place.
    let pack_tonemap = input.pack.as_ref().filter(|pack| pack.runs_at(PackPoint::Tonemap));
    let tonemapped = pack_tonemap.is_some_and(|pack| pack.writes_color_at(PackPoint::Tonemap));
    let sharpened = settings.anti_aliasing.mode == AntiAliasingMode::Fxaa || settings.anti_aliasing.sharpness > 0.0;
    let finish = sharpened || input.keep_ldr || pack_final.is_some() || tonemapped;
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
    let glow = bloom.then(|| levels[0].0);
    if let Some(pack) = pack_tonemap {
        pack.record(fg, PackPoint::Tonemap, scene, (glow, Some(composited)));
    }
    if !tonemapped {
        fg.add_pass("Composite", &images, &buffers, move |ctx| {
            draw_into(ctx, composite, &push, composited, composite_rect)
        });
    }

    let finished = match pack_final {
        Some(pack) => pack.record(fg, PackPoint::Final, composited, (glow, None)),
        None => composited,
    };
    if finish {
        let output = input.output;
        // Over a pack's picture with neither FXAA nor sharpening, only dithered.
        let level = u32::from((pack_final.is_some() || tonemapped) && !sharpened);
        let push = Push { textures: [fg.texture(finished).0, 0, 0, 0], size: origin(rect), level, ..push };
        let images = [(finished, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)];
        fg.add_pass("Finish", &images, &[], move |ctx| draw_into(ctx, pipelines.finish, &push, output, rect));
    }
    Ok(finish.then_some(finished))
}

/// The picture drawn in pixels, `small`, enlarged into `output`, or into
/// part of it with what lies outside cleared to a colour: each drawn pixel
/// a block of `size` screen pixels, the view's first falling at `start` in
/// the small picture's pixels (see `pixels::PixelGrid::start`).
pub fn enlarge(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    (small, output): (ImageId, ImageId),
    (rect, clear_outside): (Option<vk::Rect2D>, Option<[f32; 4]>),
    size: u32,
    start: [f32; 2],
) {
    if let (Some(_), Some(color)) = (rect, clear_outside) {
        fg.add_pass("Clear output", &[(output, ImageAccess::ColorAttachment)], &[], move |ctx| clear(ctx, output, color));
    }
    let origin = rect.map_or([0, 0], |r| [r.offset.x as u32, r.offset.y as u32]);
    let textures = [fg.texture(small).0, (size as f32).to_bits(), start[0].to_bits(), start[1].to_bits()];
    let push = Push { textures, size: origin, ..push };
    let images = [(small, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)];
    fg.add_pass("Enlarge", &images, &[], move |ctx| draw_into(ctx, pipelines.enlarge, &push, output, rect));
}

/// AMD FSR 3.1 in TAA's place: the frame's exposure as FSR reads it, then
/// FSR's passes, from the scene as drawn to the output's size; and the result
/// copied into TAA's history, which reflections read next frame. Returns the
/// upscaled picture.
fn record_fsr3(
    fg: &mut FrameGraph<'_>,
    gpu: &Gpu,
    bindless: &mut Bindless,
    input: &PostInput,
    fsr3: &Fsr3Input,
    (scene, velocity, depth): (ImageId, ImageId, ImageId),
    opaque: Option<ImageId>,
) -> Result<ImageId> {
    let (render, upscale) = (input.render_extent, input.extent);
    let (render, upscale) = ([render.width, render.height], [upscale.width, upscale.height]);
    {
        let mut state = fsr3.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.as_ref().is_some_and(|state| state.fits(render, upscale)) {
            if let Some(old) = state.take() {
                fsr3.retired.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(old);
            }
            *state = Some(crate::fsr3::Fsr3State::new(gpu, render, upscale)?);
        }
    }
    let pipelines = input.pipelines;
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let exposure_desc = ImageDesc::new(1, 1, crate::pipelines::FSR_EXPOSURE_FORMAT, usage);
    let exposure = fg.create_image(gpu, bindless, "FSR exposure", exposure_desc)?;
    let push = input.push;
    let buffers = [(input.exposure, BufferAccess::ShaderRead)];
    fg.add_pass("FSR exposure", &[(exposure, ImageAccess::ColorAttachment)], &buffers, move |ctx| {
        draw(ctx, pipelines.fsr_exposure, &push, exposure, false)
    });
    let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
    let output = fg.create_image(gpu, bindless, "FSR output", ImageDesc::new(upscale[0], upscale[1], HDR_FORMAT, usage))?;
    let mut images = vec![
        (scene, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (velocity, ImageAccess::Sampled),
        (exposure, ImageAccess::Sampled),
        (output, ImageAccess::StorageWrite),
    ];
    images.extend(opaque.map(|opaque| (opaque, ImageAccess::Sampled)));
    let state = fsr3.state.clone();
    let (fsr, jitter, fov_y, near, delta_time, reset, slot) =
        (fsr3.pipelines, fsr3.jitter, fsr3.fov_y, fsr3.near, fsr3.delta_time, fsr3.reset, fsr3.slot);
    fg.add_pass("FSR 3", &images, &[], move |ctx| {
        let frame = crate::fsr3::Fsr3Frame {
            color: ctx.view(scene),
            opaque: opaque.map(|opaque| ctx.view(opaque)),
            depth: ctx.view(depth),
            motion: ctx.view(velocity),
            exposure: ctx.view(exposure),
            output: ctx.view(output),
            render,
            upscale,
            jitter,
            fov_y,
            near,
            delta_time,
            reset,
            slot,
        };
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(state) = state.as_mut() {
            state.record(ctx.gpu, ctx.cmd, &fsr, &frame);
        }
    });
    // Reflections read last frame's finished picture from TAA's history.
    let current = taa_history(fg, gpu, bindless, input.extent, input.history)?;
    let copy = [(output, ImageAccess::TransferSrc), (current, ImageAccess::TransferDst)];
    fg.add_pass("FSR into history", &copy, &[], move |ctx| {
        let layers = vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
        let extent = vk::Extent3D { width: upscale[0], height: upscale[1], depth: 1 };
        let region = vk::ImageCopy::default().src_subresource(layers).dst_subresource(layers).extent(extent);
        unsafe {
            ctx.gpu.device.cmd_copy_image(
                ctx.cmd,
                ctx.image(output),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ctx.image(current),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            )
        };
    });
    Ok(output)
}

/// One of TAA's two history images for a view of `extent`: the one a
/// frame writes, or the other, last frame's finished picture, which
/// screen-space reflections read before post-processing has run.
pub fn taa_history(
    fg: &mut FrameGraph<'_>,
    gpu: &Gpu,
    bindless: &mut Bindless,
    extent: vk::Extent2D,
    which: usize,
) -> Result<ImageId> {
    // Copied into from FSR 3's output, where FSR takes TAA's place.
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
    fg.history_image(gpu, bindless, HISTORY_NAMES[which], ImageDesc::new(extent.width, extent.height, HDR_FORMAT, usage))
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
pub(crate) fn draw(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId, blend: bool) {
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
