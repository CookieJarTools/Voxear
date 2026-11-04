//! Ambient occlusion: ground-truth ambient occlusion (GTAO, Jimenez et al.
//! 2016), worked out from depth before the opaque passes shade, so they can
//! take it out of the sky's ambient light alone and leave the sun and lamps
//! as they were.
//!
//! The renderer is forward, so the frame's depth has to exist before
//! anything is shaded. With ambient occlusion on, the two-phase culling's
//! opaque passes draw depth alone (**Depth (early)** and **Depth (late)**),
//! then these passes run, then **Opaque (early)** and **Opaque (late)** draw
//! the same lists again over that depth, shading each pixel once:
//!
//! 1. **GTAO** searches each pixel's horizons, with normals rebuilt from
//!    depth.
//! 2. **AO denoise** blurs it across the surface, not across edges.
//! 3. **AO temporal** blends it with last frame's result, reprojected by the
//!    camera's motion, into one of two history images. The opaque passes read
//!    that one.
//!
//! Unless the settings ask for full resolution, the first two passes run at
//! half resolution: each of their pixels stands for one pixel of its 2x2 at
//! full resolution, a different one each frame, so four frames cover them
//! all. The temporal pass, always at full resolution, takes each pixel's
//! occlusion from the nearest of those that lie on its surface. That is
//! about a quarter of the cost, and the history hides the rest.
//!
//! The shaders are in `shaders/ambient_occlusion.slang`, and what the opaque
//! passes do with it in `shaders/occlusion.slang`.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::Push,
    graph::{FrameGraph, ImageAccess, ImageId, PassContext},
    passes::{begin_rendering, fullscreen},
    pipelines::{AO_FORMAT, AO_HISTORY_FORMAT, Pipelines},
};

const HISTORY_NAMES: [&str; 2] = ["ao history 0", "ao history 1"];

/// The images one frame's ambient occlusion passes use.
#[derive(Clone, Copy)]
pub struct Targets {
    raw: ImageId,
    denoised: ImageId,
    previous: ImageId,
    /// What the opaque passes read: this frame's result.
    pub current: ImageId,
    /// How many times the first two passes' images are halved: 0 or 1.
    level: u32,
}

impl Targets {
    /// The images for a view of `extent`; `history` says which of the two
    /// history images this frame writes, as TAA's does.
    pub fn new(
        fg: &mut FrameGraph<'_>,
        gpu: &Gpu,
        bindless: &mut Bindless,
        extent: vk::Extent2D,
        history: usize,
        full_resolution: bool,
    ) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let level = u32::from(!full_resolution);
        let halved = |size: u32| size.div_ceil(1 << level);
        let search = ImageDesc::new(halved(extent.width), halved(extent.height), AO_FORMAT, usage);
        let full = ImageDesc::new(extent.width, extent.height, AO_HISTORY_FORMAT, usage);
        Ok(Self {
            raw: fg.create_image(gpu, bindless, "ao", search)?,
            denoised: fg.create_image(gpu, bindless, "ao denoised", search)?,
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], full)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], full)?,
            level,
        })
    }
}

/// Records the three passes, reading `depth` once the depth passes drew it.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), depth: ImageId, ao: Targets) {
    let depth_texture = fg.texture(depth).0;
    let Targets { raw, denoised, previous, current, level } = ao;
    let push = Push { level, ..push };

    let gtao = Push { textures: [depth_texture, 0, 0, 0], ..push };
    let images = [(depth, ImageAccess::Sampled), (raw, ImageAccess::ColorAttachment)];
    fg.add_pass("GTAO", &images, &[], move |ctx| draw(ctx, pipelines.gtao, &gtao, raw));

    let denoise = Push { textures: [fg.texture(raw).0, depth_texture, 0, 0], ..push };
    let images = [(raw, ImageAccess::Sampled), (depth, ImageAccess::Sampled), (denoised, ImageAccess::ColorAttachment)];
    fg.add_pass("AO denoise", &images, &[], move |ctx| draw(ctx, pipelines.ao_denoise, &denoise, denoised));

    let temporal = Push { textures: [fg.texture(denoised).0, fg.texture(previous).0, depth_texture, 0], ..push };
    let images = [
        (denoised, ImageAccess::Sampled),
        (previous, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (current, ImageAccess::ColorAttachment),
    ];
    fg.add_pass("AO temporal", &images, &[], move |ctx| draw(ctx, pipelines.ao_temporal, &temporal, current));
}

/// Records the contact shadows pass into `target`, a full-resolution image
/// of `AO_FORMAT`, reading `depth` once the depth passes drew it. Its shader
/// lives beside GTAO's, as both march through the depth buffer.
pub fn record_contact_shadows(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), depth: ImageId, target: ImageId) {
    let push = Push { textures: [fg.texture(depth).0, 0, 0, 0], ..push };
    let images = [(depth, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
    fg.add_pass("Contact shadows", &images, &[], move |ctx| draw(ctx, pipelines.contact_shadows, &push, target));
}

/// Records screen-space reflections into `target`, an image of
/// `HDR_FORMAT` at half the view's size, reading `depth` once the depth
/// passes drew it and `history`, last frame's finished picture. Its shader
/// lives beside GTAO's, as both march through the depth buffer, and like
/// GTAO's half-resolution passes each of its pixels stands for a different
/// pixel of its 2×2 each frame.
pub fn record_reflections(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    (depth, roughness): (ImageId, ImageId),
    history: ImageId,
    target: ImageId,
) {
    let textures = [fg.texture(depth).0, fg.texture(history).0, fg.texture(roughness).0, 0];
    let push = Push { textures, level: 1, ..push };
    let images = [
        (depth, ImageAccess::Sampled),
        (history, ImageAccess::Sampled),
        (roughness, ImageAccess::Sampled),
        (target, ImageAccess::ColorAttachment),
    ];
    fg.add_pass("Reflections", &images, &[], move |ctx| draw(ctx, pipelines.reflections, &push, target));
}

/// A fullscreen pass that writes every pixel of `target`.
fn draw(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId) {
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering(ctx, ctx.extent(target), &color, None);
    fullscreen(ctx, pipeline, push);
}
