//! Real-time voxel global illumination: light bouncing off nearby surfaces,
//! and the sky's light hidden where things block it, for everything the
//! opaque passes shade. The shaders are `shaders/gi.slang`, which shading
//! traces cones through, and `shaders/gi_inject.slang`, which fills the
//! voxels.
//!
//! The voxels are a clipmap round the camera: three nested grids of 64³,
//! each with voxels twice the last's size, so near things are fine and far
//! ones coarse. They are filled from what the frame has already, rather
//! than by drawing the scene again:
//!
//! 1. **GI shift** moves last frame's voxels to this frame's origins, as
//!    the camera moved, and fades them a little, so what has moved away or
//!    gone fades out over a second or so.
//! 2. **GI shadows** marks a voxel for every few texels of the sun's shadow
//!    cascades, where the sun sees a surface, so walls out of view still
//!    block light and bounce some sunlight.
//! 3. **GI view** writes the lit colour of every other pixel the camera
//!    sees into its voxel: the light that surface sends on.
//!
//! 4. **GI probes** updates the radiance cache: a grid of probes round the
//!    camera, each holding the light reaching it from every direction, as
//!    twelve cones through the voxels find it. An eighth of the probes are
//!    traced again each frame, and any the grid has just moved over.
//!
//! Shading reads the eight probes round each pixel, rather than tracing
//! cones for each pixel, which costs several times less.
//!
//! The voxels and the probes are each two images used in turn, as TAA's
//! history is: shading reads last frame's, finished, while this frame's are
//! filled. Light that bounces once lights the next frame's voxels, so it
//! bounces again, and the fade keeps that from running away.

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{GI_LEVELS, GI_PROBES, GI_SIZE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::bind,
    pipelines::Pipelines,
};

pub const VOXEL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

const HISTORY_NAMES: [&str; 2] = ["gi voxels 0", "gi voxels 1"];
const PROBE_NAMES: [&str; 2] = ["gi probes 0", "gi probes 1"];

/// How far apart the probes are, in the finest voxels.
pub const PROBE_SPACING: f32 = 2.0;

/// The probe grid's lowest probe, in probes from the world's origin, so
/// that the camera is in its middle.
pub fn probe_origin(camera: Vec3, spacing: f32) -> [i32; 4] {
    let middle = (camera / spacing).floor();
    let half = (GI_PROBES / 2) as f32;
    [(middle.x - half) as i32, (middle.y - half) as i32, (middle.z - half) as i32, 0]
}

/// Each grid's lowest voxel, in its own voxels from the world's origin, so
/// that the camera is in its middle. Grids move a voxel at a time.
pub fn origins(camera: Vec3, voxel_size: f32) -> [[i32; 4]; GI_LEVELS] {
    std::array::from_fn(|level| {
        let size = voxel_size * (1 << level) as f32;
        let middle = (camera / size).floor();
        let half = (GI_SIZE / 2) as f32;
        [(middle.x - half) as i32, (middle.y - half) as i32, (middle.z - half) as i32, 0]
    })
}

/// One frame's voxels: last frame's, which shading reads, and this frame's,
/// which the passes fill.
#[derive(Clone, Copy)]
pub struct Voxels {
    pub previous: ImageId,
    pub current: ImageId,
    /// The radiance cache's probes, last frame's and this frame's.
    pub previous_probes: ImageId,
    pub probes: ImageId,
}

impl Voxels {
    /// `history` says which of the two images this frame fills.
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, history: usize) -> Result<Self> {
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let desc = ImageDesc::new(GI_SIZE * GI_SIZE, GI_SIZE * GI_LEVELS as u32, VOXEL_FORMAT, usage);
        // Two texels a probe, one above the other.
        let probe_desc = ImageDesc::new(GI_PROBES * GI_PROBES, GI_PROBES * 2, VOXEL_FORMAT, usage);
        Ok(Self {
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], desc)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], desc)?,
            previous_probes: fg.history_image(gpu, bindless, PROBE_NAMES[1 - history], probe_desc)?,
            probes: fg.history_image(gpu, bindless, PROBE_NAMES[history], probe_desc)?,
        })
    }
}

/// Records the three passes, once the opaque passes have drawn `scene` and
/// `depth`, at `size`, and the shadows `shadows`.
pub fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    voxels: Voxels,
    (scene, depth, size): (ImageId, ImageId, [u32; 2]),
    shadows: Option<ImageId>,
) {
    let Voxels { previous, current, previous_probes, probes } = voxels;
    let storage = fg.storage(current, 0).0;
    let dispatch = |width: u32, height: u32| [width.div_ceil(8), height.div_ceil(8)];

    let shift = Push { textures: [fg.texture(previous).0, 0, 0, 0], output: storage, ..push };
    let images = [(previous, ImageAccess::Sampled), (current, ImageAccess::StorageWrite)];
    let groups = dispatch(GI_SIZE * GI_SIZE, GI_SIZE * GI_LEVELS as u32);
    fg.add_pass("GI shift", &images, &[], move |ctx| compute(ctx, pipelines.gi_shift, &shift, groups));

    if let Some(atlas) = shadows {
        let marks = Push { output: storage, ..push };
        let images = [(atlas, ImageAccess::Sampled), (current, ImageAccess::StorageWrite)];
        // Every fourth texel each way of the whole atlas.
        let quarter = crate::shadows::ATLAS_RESOLUTION / 4;
        let groups = dispatch(quarter, quarter);
        fg.add_pass("GI shadows", &images, &[], move |ctx| compute(ctx, pipelines.gi_shadows, &marks, groups));
    }

    let view = Push { textures: [fg.texture(scene).0, fg.texture(depth).0, 0, 0], output: storage, ..push };
    let images = [(scene, ImageAccess::Sampled), (depth, ImageAccess::Sampled), (current, ImageAccess::StorageWrite)];
    let groups = dispatch(size[0].div_ceil(2), size[1].div_ceil(2));
    fg.add_pass("GI view", &images, &[], move |ctx| compute(ctx, pipelines.gi_view, &view, groups));

    // The probes trace cones through last frame's finished voxels, which shading also read.
    let cache = Push { textures: [fg.texture(previous_probes).0, 0, 0, 0], output: fg.storage(probes, 0).0, ..push };
    let images = [
        (previous, ImageAccess::Sampled),
        (previous_probes, ImageAccess::Sampled),
        (probes, ImageAccess::StorageWrite),
    ];
    let groups = [(GI_PROBES * GI_PROBES * GI_PROBES).div_ceil(64), 1];
    fg.add_pass("GI probes", &images, &[], move |ctx| compute(ctx, pipelines.gi_probes, &cache, groups));
}

fn compute(ctx: &crate::graph::PassContext, pipeline: vk::Pipeline, push: &Push, [x, y]: [u32; 2]) {
    bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, push);
    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, x, y, 1) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_centre_on_the_camera_and_move_a_voxel_at_a_time() {
        let at = origins(Vec3::new(10.2, 1.7, -3.1), 0.5);
        assert_eq!(at[0], [20 - 32, 3 - 32, -7 - 32, 0]);
        assert_eq!(at[1], [10 - 32, 1 - 32, -4 - 32, 0], "the next grid's voxels are twice the size");
        let nudged = origins(Vec3::new(10.3, 1.7, -3.1), 0.5);
        assert_eq!(nudged, at, "within a voxel, nothing moves");
    }
}
