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
//! to the first one's face instead, lit a face at a time. With any softness
//! the cubes are mist: each takes out only some of the light, less at a
//! cloud's edges, and the walk goes on through them.
//!
//! 3. **Cloud panorama** marches them the same way all round the camera, into
//!    a small picture across once round the horizon and from straight up to
//!    straight down, blended with last frame's: what reflections, reflection
//!    probes' captures and screen-space reflections' misses take of the sky
//!    show the clouds from it.
//!
//! The sky pass lays the Clouds picture over the sky it draws, sun, moon and
//! stars behind it; shading lays it over what is drawn as well, as much of it
//! as lies in front (`cloudsBefore` in `shaders/clouds.slang`): a far range
//! beyond the clouds' layer, a peak that rises into them, and the ground round
//! a camera inside them or above them are seen through them.

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
const PANORAMA_NAMES: [&str; 2] = ["cloud panorama 0", "cloud panorama 1"];

/// The clouds' panorama, texels across and down: about 1.4 degrees a texel.
pub const PANORAMA: [u32; 2] = [256, 128];

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
        softness: clouds.softness.clamp(0.0, 1.0),
        view: NO_TEXTURE,
        shadow_map: NO_TEXTURE,
        noise: NO_TEXTURE,
        panorama: NO_TEXTURE,
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
    /// The clouds all round, last frame's and this frame's, for the camera's
    /// own view; a view through a Portal reflects the clouds without them.
    pub panorama: Option<[ImageId; 2]>,
    /// The only part of the picture marched, where only part of it shows,
    /// as through a Portal's opening: its lowest and highest x and y, from
    /// -1 to 1.
    pub within: Option<[f32; 4]>,
}

impl CloudImages {
    /// The pictures for a frame drawn `extent` across, `shrink` times
    /// smaller each way; `history` says which of the two history images
    /// this frame writes, as TAA's does.
    pub fn new(
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        (extent, shrink): (vk::Extent2D, u32),
        history: usize,
    ) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let half = ImageDesc::new(
            extent.width.div_ceil(shrink).max(1),
            extent.height.div_ceil(shrink).max(1),
            TABLE_FORMAT,
            usage,
        );
        let shadows = ImageDesc::new(SHADOW_SIDE, SHADOW_SIDE, TABLE_FORMAT, usage);
        let panorama = ImageDesc::new(PANORAMA[0], PANORAMA[1], TABLE_FORMAT, usage);
        Ok(Self {
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], half)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], half)?,
            shadows: fg.create_image(gpu, bindless, "cloud shadows", shadows)?,
            panorama: Some([
                fg.history_image(gpu, bindless, PANORAMA_NAMES[1 - history], panorama)?,
                fg.history_image(gpu, bindless, PANORAMA_NAMES[history], panorama)?,
            ]),
            within: None,
        })
    }

    /// The pictures later passes sample.
    pub fn sampled(&self) -> Vec<ImageId> {
        let mut sampled = vec![self.current, self.shadows];
        sampled.extend(self.panorama.map(|[_, current]| current));
        sampled
    }
}

/// Where the shadow map's middle is for a camera at `camera`, along x and
/// z, over clouds drifted `drift` metres: on a texel's corner of a grid
/// that drifts with the clouds. Each texel then keeps its own place in the
/// clouds, so the map does not swim as the camera moves, and the shadows
/// glide over the ground with the clouds; on a grid fixed to the ground
/// they would cross it a texel at a time, in jumps tens of metres long.
pub fn shadow_middle(camera: [f32; 2], drift: [f32; 2]) -> [f32; 2] {
    let texel = SHADOW_REACH * 2.0 / SHADOW_SIDE as f32;
    [0, 1].map(|axis| ((camera[axis] - drift[axis]) / texel).round() * texel + drift[axis])
}

/// Records the two passes. `tables` are the atmosphere's, where the sky is
/// the atmosphere, which light the clouds and haze them.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), clouds: CloudImages, tables: &[ImageId]) {
    let CloudImages { previous, current, shadows, panorama, within } = clouds;
    // Drawn only in part, each picture is cleared first when it is made: what lies outside that part is read as
    // last frame's where the sky has turned into it, and a new image holds whatever its memory held, NaN among it,
    // which the blend would keep from frame to frame.
    if within.is_some() {
        let new: Vec<ImageId> = [previous, current].into_iter().filter(|&image| fg.is_new(image)).collect();
        for image in new {
            fg.add_pass("Clouds cleared", &[(image, ImageAccess::ColorAttachment)], &[], move |ctx| {
                let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] } };
                let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
                let color = [vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.view(image))
                    .image_layout(layout)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(clear)];
                begin_rendering(ctx, ctx.extent(image), &color, None);
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            });
        }
    }
    fg.add_pass("Cloud shadows", &[(shadows, ImageAccess::ColorAttachment)], &[], move |ctx| {
        draw(ctx, pipelines.cloud_shadows, &push, shadows);
    });
    let march = Push { textures: [fg.texture(previous).0, 0, 0, 0], ..push };
    let mut images = vec![(previous, ImageAccess::Sampled), (current, ImageAccess::ColorAttachment)];
    images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
    fg.add_pass("Clouds", &images, &[], move |ctx| match within {
        None => draw(ctx, pipelines.clouds, &march, current),
        Some(rect) => draw_within(ctx, pipelines.clouds, &march, current, rect),
    });
    if let Some([before, now]) = panorama {
        let march = Push { textures: [fg.texture(before).0, 0, 0, 0], ..push };
        let mut images = vec![(before, ImageAccess::Sampled), (now, ImageAccess::ColorAttachment)];
        images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
        fg.add_pass("Cloud panorama", &images, &[], move |ctx| draw(ctx, pipelines.cloud_panorama, &march, now));
    }
}

/// A pass that writes the texels of `target` within `rect` of it (lowest
/// and highest x and y, from -1 to 1) and a margin round it, for what moves
/// into view next frame to have a history; the rest keep what they held.
fn draw_within(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId, [x0, y0, x1, y1]: [f32; 4]) {
    let extent = ctx.extent(target);
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(ctx.view(target))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE)];
    begin_rendering(ctx, extent, &color, None);
    let (wide, high) = (extent.width as f32, extent.height as f32);
    let margin = 0.08;
    let pixel = |v: f32, size: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * size) as i32;
    let (left, top) = (pixel(x0 - margin, wide), pixel(y0 - margin, high));
    let (right, bottom) = (pixel(x1 + margin, wide) + 1, pixel(y1 + margin, high) + 1);
    let scissor = vk::Rect2D {
        offset: vk::Offset2D { x: left, y: top },
        extent: vk::Extent2D {
            width: (right - left).clamp(1, extent.width as i32 - left) as u32,
            height: (bottom - top).clamp(1, extent.height as i32 - top) as u32,
        },
    };
    unsafe { ctx.gpu.device.cmd_set_scissor(ctx.cmd, 0, &[scissor]) };
    fullscreen(ctx, pipeline, push);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shadow_maps_grid_drifts_with_the_clouds_and_stays_round_the_camera() {
        let texel = SHADOW_REACH * 2.0 / SHADOW_SIDE as f32;
        let camera = [100.0, -40.0];
        // Clouds drifted part of a texel carry the map as far: each texel keeps its place in them.
        let (still, drifted) = (shadow_middle(camera, [0.0, 0.0]), shadow_middle(camera, [7.5, -3.25]));
        assert!((drifted[0] - still[0] - 7.5).abs() < 1e-3 && (drifted[1] - still[1] + 3.25).abs() < 1e-3);
        // However far they drift, the map's middle is within half a texel of the camera, and on the clouds' grid.
        for step in 0..400 {
            let drift = [step as f32 * 3.7, step as f32 * -1.9];
            let middle = shadow_middle(camera, drift);
            for axis in 0..2 {
                assert!((middle[axis] - camera[axis]).abs() <= texel * 0.5 + 0.01, "{middle:?} after {drift:?}");
                let cells = (middle[axis] - drift[axis]) / texel;
                assert!((cells - cells.round()).abs() < 1e-2, "{middle:?} after {drift:?}");
            }
        }
    }
}
