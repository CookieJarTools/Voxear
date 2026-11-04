//! The physically based sky: a planet's atmosphere, as Hillaire models it in
//! "A Scalable and Production Ready Sky and Atmosphere Rendering Technique"
//! (2020). The shaders are `shaders/atmosphere.slang` and the table passes
//! in `shaders/sky.slang`.
//!
//! When a scene's sky is the atmosphere, four tables are drawn each frame
//! before anything is shaded, in this order, each reading the ones before:
//!
//! 1. **Transmittance**: how much light gets through the air from any height
//!    in any direction to space.
//! 2. **Multiple scattering**: the light scattered more than once, by height
//!    and the sun's angle, which keeps a low sun's sky bright and blue.
//! 3. **Sky view**: the sky as the camera sees it, by direction, which the
//!    sky pass and the sky's ambient light sample.
//! 4. **Aerial perspective**: the haze between the camera and each
//!    distance, which the opaque passes lay over what lies far off.
//!
//! The first two depend only on the atmosphere, not on the camera or the
//! sun, so they are kept from frame to frame and drawn again only when the
//! atmosphere changes. Multiple scattering is the costly one, about 0.3 ms
//! on an RTX 3070; the two drawn every frame take a few hundredths.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{GpuAtmosphere, Push},
    graph::{FrameGraph, ImageAccess, ImageId, PassContext},
    passes::{begin_rendering, fullscreen},
    pipelines::Pipelines,
};

/// The tables' format: colour, and for aerial perspective how much light the
/// air takes away, in alpha.
pub const TABLE_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

const TRANSMITTANCE: (u32, u32) = (256, 64);
const MULTI_SCATTERING: (u32, u32) = (32, 32);
const SKY_VIEW: (u32, u32) = (192, 108);
/// 32 slices of 32×32, side by side.
const AERIAL_PERSPECTIVE: (u32, u32) = (32 * 32, 32);

/// Earth's atmosphere, as Hillaire's paper gives it: air that scatters blue
/// most, aerosols that scatter every colour forward, and ozone, over a
/// planet of 6,360 km with 100 km of air. The sun's disc is half a degree
/// across.
pub fn earth() -> GpuAtmosphere {
    GpuAtmosphere {
        rayleigh_scattering: [5.802e-3, 13.558e-3, 33.1e-3],
        rayleigh_height: 8.0,
        mie_scattering: [3.996e-3; 3],
        mie_height: 1.2,
        mie_extinction: [4.40e-3; 3],
        mie_g: 0.8,
        ozone_absorption: [0.650e-3, 1.881e-3, 0.085e-3],
        bottom_radius: 6360.0,
        ground_albedo: [0.3; 3],
        top_radius: 6460.0,
        sun_cos_radius: 0.2678f32.to_radians().cos(),
        // A game's views reach a few hundred metres, where real haze barely
        // shows; counting distances as ten times as far gives it its look.
        aerial_scale: 10.0,
        _pad: [0.0; 2],
    }
}

/// Earth's air with a scene's sky's haze and sun.
pub fn of_sky(sky: &crate::Sky) -> GpuAtmosphere {
    let earth = earth();
    let haze = sky.haze.max(0.0);
    let tint = sky.haze_tint.max(voxear_core::math::Vec3::ZERO).to_array();
    // As much haze dims the light passing through it whatever its tint: what a dusty haze does not scatter, it
    // absorbs.
    let scattering = std::array::from_fn(|c| earth.mie_scattering[c] * haze * tint[c].min(1.0));
    let extinction = std::array::from_fn(|c| earth.mie_extinction[c] * haze);
    GpuAtmosphere {
        mie_scattering: scattering,
        mie_extinction: extinction,
        mie_g: sky.haze_glow.clamp(0.0, 0.95),
        sun_cos_radius: (sky.sun_size.clamp(0.05, 20.0) * 0.5).to_radians().cos(),
        aerial_scale: sky.distance_haze.max(0.0),
        ..earth
    }
}

/// One frame's tables.
#[derive(Clone, Copy)]
pub struct Tables {
    pub transmittance: ImageId,
    pub multi_scattering: ImageId,
    pub sky_view: ImageId,
    pub aerial_perspective: ImageId,
    /// Whether the kept tables are drawn this frame, rather than kept as
    /// they were.
    fresh: bool,
}

impl Tables {
    /// The tables for a frame. `fresh` draws the kept ones again, as the
    /// first frame and a changed atmosphere need.
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, fresh: bool) -> Result<Self> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let desc = |(width, height)| ImageDesc::new(width, height, TABLE_FORMAT, usage);
        Ok(Self {
            transmittance: fg.history_image(gpu, bindless, "transmittance", desc(TRANSMITTANCE))?,
            multi_scattering: fg.history_image(gpu, bindless, "multiple scattering", desc(MULTI_SCATTERING))?,
            sky_view: fg.create_image(gpu, bindless, "sky view", desc(SKY_VIEW))?,
            aerial_perspective: fg.create_image(gpu, bindless, "aerial perspective", desc(AERIAL_PERSPECTIVE))?,
            fresh,
        })
    }

    /// Every table, for the passes that sample them to say so.
    pub fn all(&self) -> [ImageId; 4] {
        [self.transmittance, self.multi_scattering, self.sky_view, self.aerial_perspective]
    }
}

/// Records the four table passes. The frame must already name the tables
/// (see `GpuLighting`), as each pass reads the ones before it by index.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), tables: Tables) {
    let Tables { transmittance, multi_scattering, sky_view, aerial_perspective, fresh } = tables;
    let sampled = |images: &[ImageId]| images.iter().map(|&image| (image, ImageAccess::Sampled)).collect::<Vec<_>>();

    if fresh {
        fg.add_pass("Transmittance", &[(transmittance, ImageAccess::ColorAttachment)], &[], move |ctx| {
            draw(ctx, pipelines.transmittance, &push, transmittance);
        });
        let mut images = sampled(&[transmittance]);
        images.push((multi_scattering, ImageAccess::ColorAttachment));
        fg.add_pass("Multiple scattering", &images, &[], move |ctx| {
            draw(ctx, pipelines.multi_scattering, &push, multi_scattering);
        });
    }
    let mut images = sampled(&[transmittance, multi_scattering]);
    images.push((sky_view, ImageAccess::ColorAttachment));
    fg.add_pass("Sky view", &images, &[], move |ctx| draw(ctx, pipelines.sky_view, &push, sky_view));
    let mut images = sampled(&[transmittance, multi_scattering]);
    images.push((aerial_perspective, ImageAccess::ColorAttachment));
    fg.add_pass("Aerial perspective", &images, &[], move |ctx| {
        draw(ctx, pipelines.aerial_perspective, &push, aerial_perspective);
    });
    // The sun's light and the sky's, once, for every pixel after to share rather than each look them up.
    fg.add_pass("Sky light", &sampled(&[transmittance, multi_scattering, sky_view]), &[], move |ctx| {
        crate::passes::bind(ctx, pipelines.sky_light, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, 1, 1, 1) };
        // Read by many passes after this, shading, the GI's rays and the fog among them: one barrier for them all.
        let barrier = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ)];
        unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &vk::DependencyInfo::default().memory_barriers(&barrier)) };
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
