//! Ray-traced global illumination: probes round the camera that trace rays
//! against the scene's triangles, as RTXGI and DDGI do, by ray tracing
//! where the GPU has it and in a compute shader where it does not. The
//! shaders are `shaders/ddgi.slang`, which traces and updates the probes,
//! and `shaders/ddgi_sample.slang`, which shading reads them with.
//!
//! Each probe keeps, in small octahedral maps, the light reaching it from
//! every direction and how far its rays went before they hit something.
//! Shading takes light from the probes round a point only as far as their
//! rays say they can see it, so a probe on the far side of a wall gives
//! none, and probes whose rays find mostly the backs of surfaces are inside
//! something and are set aside. Light does not leak through walls.
//!
//! Each frame:
//!
//! 1. **Scene BVH** builds the frame's top level from the instances drawn.
//! 2. **DDGI rays** traces 256 rays from each probe due: a quarter of the
//!    fine probes and a sixteenth of the coarse, scattered, every probe the
//!    grid has just moved over, and all of them while the GI warms up.
//! 3. **DDGI update** blends those rays into the due probes' maps.
//!
//! The probes stay in their slots as the camera moves: a world probe's slot
//! is its coordinates modulo the grid's size, so nothing is copied.

use anyhow::Result;
use voxear_core::math::{Quat, Vec3};
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{DDGI_CASCADE_SCALE, DDGI_CASCADES, DDGI_PROBES, DDGI_RAYS, DDGI_TEXELS, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::bind,
    pipelines::Pipelines,
};

const FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const TILES_ACROSS: u32 = 128;

/// How many frames every probe traces when the GI starts, or starts again.
pub const WARM_UP_FRAMES: u32 = 8;

/// The probes in one cascade.
const fn probe_count() -> u32 {
    DDGI_PROBES[0] * DDGI_PROBES[1] * DDGI_PROBES[2]
}

/// Each cascade's lowest probe, in its own probes from the world's origin,
/// so that the camera is in its middle; `spacing` is the finest cascade's.
pub fn origins(camera: Vec3, spacing: f32) -> [[i32; 4]; DDGI_CASCADES] {
    std::array::from_fn(|cascade| {
        let size = spacing * DDGI_CASCADE_SCALE.powi(cascade as i32);
        let middle = (camera / size).floor();
        let half = |axis: usize| (DDGI_PROBES[axis] / 2) as f32;
        [(middle.x - half(0)) as i32, (middle.y - half(1)) as i32, (middle.z - half(2)) as i32, 0]
    })
}

/// A rotation for this frame's rays, the rows of its matrix, so probes
/// sample other directions each frame.
pub fn rotation(frame: u64) -> [[f32; 4]; 3] {
    // A Weyl sequence of angles keeps successive turns far apart.
    let t = frame as f32;
    let axis =
        Vec3::new((t * 0.7548777).fract() * 2.0 - 1.0, (t * 0.5698403).fract() * 2.0 - 1.0, 0.5).normalize_or(Vec3::Y);
    let turn = Quat::from_axis_angle(axis, (t * 0.6180340).fract() * std::f32::consts::TAU);
    let m = voxear_core::math::Mat3::from_quat(turn);
    let row = |r: usize| [m.x_axis[r], m.y_axis[r], m.z_axis[r], 0.0];
    [row(0), row(1), row(2)]
}

/// The probes' images, kept from frame to frame.
#[derive(Clone, Copy)]
pub struct Probes {
    /// The rays traced this frame, a texel each: the light found and how
    /// far, negative for the back of a surface.
    pub rays: ImageId,
    pub irradiance: ImageId,
    pub depth: ImageId,
    /// Each probe's state: w 1 while it lights the scene, 0 inside a wall.
    pub states: ImageId,
    /// Its rays are traced in a compute shader through the scene's hierarchy,
    /// not by ray tracing.
    pub software: bool,
}

impl Probes {
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, software: bool) -> Result<Self> {
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let tile_rows = probe_count() / TILES_ACROSS * DDGI_CASCADES as u32;
        let maps = ImageDesc::new(TILES_ACROSS * DDGI_TEXELS, tile_rows * DDGI_TEXELS, FORMAT, usage);
        let rays = ImageDesc::new(16 * DDGI_RAYS, probe_count() / 16 * DDGI_CASCADES as u32, FORMAT, usage);
        Ok(Self {
            rays: fg.history_image(gpu, bindless, "ddgi rays", rays)?,
            irradiance: fg.history_image(gpu, bindless, "ddgi irradiance", maps)?,
            depth: fg.history_image(gpu, bindless, "ddgi depth", maps)?,
            states: fg.history_image(
                gpu,
                bindless,
                "ddgi states",
                ImageDesc::new(TILES_ACROSS, tile_rows, FORMAT, usage),
            )?,
            software,
        })
    }
}

/// Records the rays and update passes, once the scene's top level is built.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), probes: Probes) {
    let traced = if probes.software { Some(pipelines.ddgi_software) } else { pipelines.ddgi };
    let Some([rays_pipeline, update_pipeline]) = traced else {
        return;
    };
    let Probes { rays, irradiance, depth, states, .. } = probes;
    let groups = probe_count() * DDGI_CASCADES as u32;

    let trace = Push { output: fg.storage(rays, 0).0, ..push };
    let images = [
        (rays, ImageAccess::StorageWrite),
        (irradiance, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (states, ImageAccess::Sampled),
    ];
    fg.add_pass("DDGI rays", &images, &[], move |ctx| {
        bind(ctx, rays_pipeline, vk::PipelineBindPoint::COMPUTE, &trace);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });

    let storage = |image| fg.storage(image, 0).0;
    let update = Push { textures: [storage(rays), storage(irradiance), storage(depth), storage(states)], ..push };
    let images = [
        (rays, ImageAccess::StorageWrite),
        (irradiance, ImageAccess::StorageWrite),
        (depth, ImageAccess::StorageWrite),
        (states, ImageAccess::StorageWrite),
    ];
    fg.add_pass("DDGI update", &images, &[], move |ctx| {
        bind(ctx, update_pipeline, vk::PipelineBindPoint::COMPUTE, &update);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grids_centre_on_the_camera_and_the_coarse_one_is_four_times_as_wide() {
        let at = origins(Vec3::new(10.2, 1.7, -3.1), 1.0);
        assert_eq!(at[0], [10 - 16, 1 - 8, -4 - 16, 0]);
        assert_eq!(at[1], [2 - 16, -8, -1 - 16, 0]);
    }

    #[test]
    fn each_frame_turns_the_rays_a_different_way() {
        assert_ne!(rotation(1), rotation(2));
        let rows = rotation(7);
        let length = |r: [f32; 4]| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        assert!(rows.iter().all(|&r| (length(r) - 1.0).abs() < 1e-4), "a rotation keeps lengths");
    }
}
