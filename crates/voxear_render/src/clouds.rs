//! Clouds in the sky: a layer of them between two heights, shaped from
//! tiling noise, lit by the sun and the sky, and shading what lies under
//! them. The settings are `Sky::clouds`; the shaders are
//! `shaders/clouds.slang`, with the passes' own in `shaders/sky.slang`.
//!
//! Each frame with any cloud cover, two passes run before anything is
//! shaded:
//!
//! 1. **Cloud shadows** draws, looking down, how much of the sun's light the
//!    clouds let through over the ground round the camera. Everything the
//!    sun lights reads it where it reads the sun's shadow, so cloud shadows
//!    drift over ground, plants, water and the fog, whose shafts then fall
//!    between the clouds.
//! 2. **Clouds** marches the layer along each pixel's ray at half the
//!    picture's width and height, from a start jittered each frame, and
//!    blends the result with last frame's where the sky was then. It is one
//!    of two history images, as the fog's volume is.
//!
//! Clouds of the **Blocks** kind are the same clouds built of cubes: every
//! place in a cube is cloud or not as the cube's middle is, so the shadows
//! are square-edged too, and the Clouds pass walks the cubes along each ray
//! to the first one's face instead, lit a face at a time.
//!
//! The sky pass lays that picture over the sky it draws, sun, moon and stars
//! behind it. Clouds are only ever behind what is drawn: a peak higher than
//! their undersides is not hidden in them.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    atmosphere::TABLE_FORMAT,
    gpu::{GpuClouds, NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId, PassContext},
    passes::{begin_rendering, fullscreen},
    pipelines::Pipelines,
    scene::{CloudKind, Clouds},
};

/// The shadow map: how many texels a side, and how far it reaches from the
/// camera each way, in metres.
const SHADOW_SIDE: u32 = 256;
pub const SHADOW_REACH: f32 = 6000.0;

const HISTORY_NAMES: [&str; 2] = ["clouds 0", "clouds 1"];

/// A sky's clouds as the shaders read them, before a frame gives them their
/// pictures and how far they have drifted.
pub fn gpu(clouds: &Clouds) -> GpuClouds {
    GpuClouds {
        cover: clouds.cover.clamp(0.0, 1.0),
        base: clouds.height.max(0.0),
        thickness: clouds.thickness.max(10.0),
        size: clouds.size.max(10.0),
        density: clouds.density.max(0.0),
        shadows: clouds.shadows.clamp(0.0, 1.0),
        speed: clouds.drift,
        kind: match clouds.kind {
            CloudKind::Soft => 0,
            CloudKind::Blocks => 1,
        },
        block: clouds.block.clamp(1.0, 10_000.0),
        view: NO_TEXTURE,
        shadow_map: NO_TEXTURE,
        noise: NO_TEXTURE,
        ..GpuClouds::default()
    }
}

/// One frame's cloud pictures.
#[derive(Clone, Copy)]
pub struct CloudImages {
    previous: ImageId,
    /// What the sky pass lays over the sky.
    pub current: ImageId,
    /// The sun let through, over the ground round the camera.
    pub shadows: ImageId,
}

impl CloudImages {
    /// The pictures for a frame drawn `extent` across; `history` says which
    /// of the two history images this frame writes, as TAA's does.
    pub fn new(
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        extent: vk::Extent2D,
        history: usize,
    ) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let half =
            ImageDesc::new(extent.width.div_ceil(2).max(1), extent.height.div_ceil(2).max(1), TABLE_FORMAT, usage);
        let shadows = ImageDesc::new(SHADOW_SIDE, SHADOW_SIDE, TABLE_FORMAT, usage);
        Ok(Self {
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], half)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], half)?,
            shadows: fg.create_image(gpu, bindless, "cloud shadows", shadows)?,
        })
    }

    /// The pictures later passes sample.
    pub fn sampled(&self) -> [ImageId; 2] {
        [self.current, self.shadows]
    }
}

/// Where the shadow map's middle is for a camera at `camera`, along x and
/// z: on a texel's corner, so the map does not swim as the camera moves.
pub fn shadow_middle(camera: [f32; 2]) -> [f32; 2] {
    let texel = SHADOW_REACH * 2.0 / SHADOW_SIDE as f32;
    camera.map(|v| (v / texel).round() * texel)
}

/// Records the two passes. `tables` are the atmosphere's, where the sky is
/// the atmosphere, which light the clouds and haze them.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), clouds: CloudImages, tables: &[ImageId]) {
    let CloudImages { previous, current, shadows } = clouds;
    fg.add_pass("Cloud shadows", &[(shadows, ImageAccess::ColorAttachment)], &[], move |ctx| {
        draw(ctx, pipelines.cloud_shadows, &push, shadows);
    });
    let march = Push { textures: [fg.texture(previous).0, 0, 0, 0], ..push };
    let mut images = vec![(previous, ImageAccess::Sampled), (current, ImageAccess::ColorAttachment)];
    images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
    fg.add_pass("Clouds", &images, &[], move |ctx| draw(ctx, pipelines.clouds, &march, current));
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
