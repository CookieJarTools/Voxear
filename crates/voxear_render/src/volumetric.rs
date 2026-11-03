//! Volumetric fog: the height fog near the camera as a volume of froxels,
//! lit by the sun through its shadows, by the clustered lights and by the
//! sky, so light shafts show and lamps glow in the mist. The shaders are
//! `shaders/volumetric.slang` and `shaders/volume.slang`.
//!
//! After the opaque passes, two passes fill the volume:
//!
//! 1. **Fog volume** works out each froxel's density and the light it
//!    scatters towards the camera, at a depth jittered each frame, and
//!    blends it with last frame's volume where that froxel was. The volume
//!    is one of two history images, as TAA's is.
//! 2. **Fog integration** adds it up from the camera outwards.
//!
//! The post-processing fog pass then lays it over the scene in place of the
//! plain height fog, which carries on beyond the volume's reach.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::Push,
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    passes::{begin_rendering, fullscreen},
    pipelines::Pipelines,
    shadows::CASCADE_RESOLUTION,
};

/// Colour scattered and, before integration, how thick the fog is, in
/// alpha; after it, the share of light it lets through.
pub const VOLUME_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// 128×72 froxels, 64 slices side by side, as `shaders/volume.slang` lays
/// them out.
const SIZE: (u32, u32) = (128 * 64, 72);

const HISTORY_NAMES: [&str; 2] = ["fog volume 0", "fog volume 1"];

/// One frame's volumes.
#[derive(Clone, Copy)]
pub struct Volume {
    previous: ImageId,
    current: ImageId,
    /// What the fog pass reads.
    pub integrated: ImageId,
}

impl Volume {
    /// The volumes for a frame; `history` says which of the two history
    /// images this frame writes, as TAA's does.
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, history: usize) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let desc = ImageDesc::new(SIZE.0, SIZE.1, VOLUME_FORMAT, usage);
        Ok(Self {
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], desc)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], desc)?,
            integrated: fg.create_image(gpu, bindless, "fog integrated", desc)?,
        })
    }
}

/// Records the two passes, reading the sun's shadows from `shadows` and the
/// lights from `lights`.
pub fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    volume: Volume,
    shadows: Option<ImageId>,
    lights: &[BufferId],
) {
    let Volume { previous, current, integrated } = volume;
    // The sun's shadows are read as the opaque passes read them, with their atlas's tile size.
    let scatter = Push { textures: [fg.texture(previous).0, 0, 0, 0], size: [CASCADE_RESOLUTION; 2], ..push };
    let mut images = vec![(previous, ImageAccess::Sampled), (current, ImageAccess::ColorAttachment)];
    images.extend(shadows.map(|atlas| (atlas, ImageAccess::Sampled)));
    let buffers: Vec<_> = lights.iter().map(|&buffer| (buffer, BufferAccess::ShaderRead)).collect();
    fg.add_pass("Fog volume", &images, &buffers, move |ctx| draw(ctx, pipelines.volume_scatter, &scatter, current));

    let integrate = Push { textures: [fg.texture(current).0, 0, 0, 0], ..push };
    let images = [(current, ImageAccess::Sampled), (integrated, ImageAccess::ColorAttachment)];
    fg.add_pass("Fog integration", &images, &[], move |ctx| {
        draw(ctx, pipelines.volume_integrate, &integrate, integrated);
    });
}

/// A fullscreen pass that writes every texel of `target`.
fn draw(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId) {
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering(ctx, ctx.extent(target), &color, None);
    fullscreen(ctx, pipeline, push);
}
