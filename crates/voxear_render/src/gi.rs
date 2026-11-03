//! Real-time voxel global illumination: light bouncing off nearby surfaces,
//! and the sky's light hidden where things block it, for everything the
//! opaque passes shade. The shaders are `shaders/gi.slang`, which shading
//! traces cones through, and `shaders/gi_inject.slang`, which fills the
//! voxels.
//!
//! The voxels are a clipmap round the camera: three nested grids of 64³,
//! each with voxels twice the last's size, so near things are fine and far
//! ones coarse. Each frame:
//!
//! 1. **GI clear** empties the albedo, normal and emissive voxels.
//! 2. **GI voxelize** draws the meshes in each grid into them, three times
//!    each, flat along each axis (`shaders/gi_voxelize.slang`), so walls
//!    behind the camera and round corners are there as much as those in
//!    view.
//! 3. **GI light** lights every filled voxel as its surface is lit: by the
//!    sun where the shadow map says it reaches, by its own glow, and by
//!    what the radiance cache found round it last frame, so light bounces
//!    on again.
//! 4. **GI view** brightens the voxels of every other pixel the camera
//!    sees to that pixel's lit colour, which brings in the lamps.
//! 5. **GI downsample** makes each coarser grid, where the finer one covers
//!    it, the average of the finer voxels, so thin floors and walls fill
//!    only part of a coarse voxel.
//!
//! 6. **GI probes** updates the radiance cache: a grid of probes round the
//!    camera, each an ambient cube holding the light falling on a surface
//!    facing each of six ways, as 32 cones through the voxels find it. A
//!    sixteenth of the probes are traced again each frame, and any the grid has
//!    just moved over.
//!
//! The coarser grids keep how much they block along each axis as well as
//! their light, so a floor blocks a cone going down through its coarse
//! voxel but little of one passing level above it.
//!
//! Shading reads the eight probes round each pixel, rather than tracing
//! cones for each pixel, which costs several times less, skipping probes a
//! wall stands in front of. Reflections trace one cone of their own.
//!
//! The lit voxels and the probes are each two images used in turn, as TAA's
//! history is: shading reads last frame's, finished, while this frame's are
//! filled. Light that bounces once lights the next frame's voxels, so it
//! bounces again, losing some to each surface's colour on the way.

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{GI_LEVELS, GI_PROBES, GI_SIZE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind},
    pipelines::Pipelines,
};

pub const VOXEL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

const HISTORY_NAMES: [&str; 2] = ["gi voxels 0", "gi voxels 1"];
const PROBE_NAMES: [&str; 2] = ["gi probes 0", "gi probes 1"];
const OPACITY_NAMES: [&str; 2] = ["gi opacity 0", "gi opacity 1"];

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

/// One mesh to draw into the voxels: its instance, and its most detailed
/// LOD's first index and index count.
#[derive(Clone, Copy, Debug)]
pub struct VoxelDraw {
    pub instance: u32,
    pub first_index: u32,
    pub index_count: u32,
}

/// The meshes each grid draws into the voxels, those whose bounds reach
/// into it, from instances' world-space spheres (centre and radius).
pub fn voxel_draws(
    spheres: impl Iterator<Item = (VoxelDraw, Vec3, f32)>,
    origins: &[[i32; 4]; GI_LEVELS],
    voxel_size: f32,
) -> [Vec<VoxelDraw>; GI_LEVELS] {
    let mut lists: [Vec<VoxelDraw>; GI_LEVELS] = Default::default();
    for (draw, centre, radius) in spheres {
        for (level, list) in lists.iter_mut().enumerate() {
            let size = voxel_size * (1 << level) as f32;
            let low = Vec3::new(origins[level][0] as f32, origins[level][1] as f32, origins[level][2] as f32) * size;
            let high = low + Vec3::splat(GI_SIZE as f32 * size);
            let nearest = centre.clamp(low, high);
            if nearest.distance(centre) <= radius {
                list.push(draw);
            }
        }
    }
    lists
}

/// One frame's voxels: last frame's lit voxels, which shading reads, and
/// this frame's, which the passes fill, with the meshes' albedo, normals
/// and glow drawn into voxels this frame to light them from.
#[derive(Clone, Copy)]
pub struct Voxels {
    pub previous: ImageId,
    pub current: ImageId,
    /// The radiance cache's probes, last frame's and this frame's.
    pub previous_probes: ImageId,
    pub probes: ImageId,
    /// How much the voxels block along X, Y and Z, last frame's and this
    /// frame's.
    pub previous_opacity: ImageId,
    pub opacity: ImageId,
    pub albedo: ImageId,
    pub normals: ImageId,
    pub emissive: ImageId,
}

impl Voxels {
    /// `history` says which of the two images this frame fills.
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, history: usize) -> Result<Self> {
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let desc = ImageDesc::new(GI_SIZE * GI_SIZE, GI_SIZE * GI_LEVELS as u32, VOXEL_FORMAT, usage);
        // Six texels a probe, its ambient cube's faces, one above another.
        let probe_desc = ImageDesc::new(GI_PROBES * GI_PROBES, GI_PROBES * 6, VOXEL_FORMAT, usage);
        Ok(Self {
            previous: fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], desc)?,
            current: fg.history_image(gpu, bindless, HISTORY_NAMES[history], desc)?,
            previous_probes: fg.history_image(gpu, bindless, PROBE_NAMES[1 - history], probe_desc)?,
            probes: fg.history_image(gpu, bindless, PROBE_NAMES[history], probe_desc)?,
            previous_opacity: fg.history_image(gpu, bindless, OPACITY_NAMES[1 - history], desc)?,
            opacity: fg.history_image(gpu, bindless, OPACITY_NAMES[history], desc)?,
            albedo: fg.create_image(gpu, bindless, "gi albedo", desc)?,
            normals: fg.create_image(gpu, bindless, "gi normals", desc)?,
            emissive: fg.create_image(gpu, bindless, "gi emissive", desc)?,
        })
    }
}

/// The views each grid is drawn from when voxelizing: as many pixels
/// across as this many to a voxel, so small things still land in one.
const VOXELIZE_PIXELS: u32 = 4;

/// Records the passes, once the opaque passes have drawn `scene` and
/// `depth`, at `size`, with the shadows `shadows` and the meshes `draws`
/// draws into each grid from the shared index buffer `indices`.
pub fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    voxels: Voxels,
    (scene, depth, size): (ImageId, ImageId, [u32; 2]),
    shadows: Option<ImageId>,
    (draws, indices): ([Vec<VoxelDraw>; GI_LEVELS], vk::Buffer),
) {
    let Voxels { previous, current, previous_probes, probes, previous_opacity, opacity, albedo, normals, emissive } =
        voxels;
    let storage = fg.storage(current, 0).0;
    let opacity_storage = fg.storage(opacity, 0).0;
    let geometry = [fg.storage(albedo, 0).0, fg.storage(normals, 0).0, fg.storage(emissive, 0).0, 0];
    let dispatch = |width: u32, height: u32| [width.div_ceil(8), height.div_ceil(8)];
    let every_voxel = dispatch(GI_SIZE * GI_SIZE, GI_SIZE * GI_LEVELS as u32);

    let clear = Push { textures: geometry, ..push };
    let written = |access| [(albedo, access), (normals, access), (emissive, access)];
    fg.add_pass("GI clear", &written(ImageAccess::StorageWrite), &[], move |ctx| {
        compute(ctx, pipelines.gi_clear, &clear, every_voxel)
    });

    let voxelize = Push { textures: geometry, ..push };
    fg.add_pass("GI voxelize", &written(ImageAccess::FragmentStorageWrite), &[], move |ctx| {
        let side = GI_SIZE * VOXELIZE_PIXELS;
        begin_rendering(ctx, vk::Extent2D { width: side, height: side }, &[], None);
        bind(ctx, pipelines.gi_voxelize, vk::PipelineBindPoint::GRAPHICS, &voxelize);
        unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, indices, 0, vk::IndexType::UINT32) };
        for (level, list) in draws.iter().enumerate() {
            for axis in 0..3 {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { level: level as u32, list: axis, ..voxelize });
                for draw in list {
                    let VoxelDraw { instance, first_index, index_count } = *draw;
                    unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, index_count, 1, first_index, 0, instance) };
                }
            }
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });

    // Shadows are sampled as shading samples them, by cascade texels.
    let light = Push {
        textures: [geometry[0], geometry[1], geometry[2], opacity_storage],
        output: storage,
        size: [crate::shadows::CASCADE_RESOLUTION; 2],
        ..push
    };
    let mut images = vec![
        (albedo, ImageAccess::StorageWrite),
        (normals, ImageAccess::StorageWrite),
        (emissive, ImageAccess::StorageWrite),
        (previous, ImageAccess::Sampled),
        (previous_opacity, ImageAccess::Sampled),
        (previous_probes, ImageAccess::Sampled),
        (current, ImageAccess::StorageWrite),
        (opacity, ImageAccess::StorageWrite),
    ];
    images.extend(shadows.map(|atlas| (atlas, ImageAccess::Sampled)));
    fg.add_pass("GI light", &images, &[], move |ctx| compute(ctx, pipelines.gi_light, &light, every_voxel));

    let view =
        Push { textures: [fg.texture(scene).0, fg.texture(depth).0, opacity_storage, 0], output: storage, ..push };
    let images = [
        (scene, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (current, ImageAccess::StorageWrite),
        (opacity, ImageAccess::StorageWrite),
    ];
    let groups = dispatch(size[0].div_ceil(2), size[1].div_ceil(2));
    fg.add_pass("GI view", &images, &[], move |ctx| compute(ctx, pipelines.gi_view, &view, groups));

    for level in 1..GI_LEVELS as u32 {
        let down = Push { output: storage, level, textures: [opacity_storage, 0, 0, 0], ..push };
        let groups = dispatch(GI_SIZE * GI_SIZE, GI_SIZE);
        let images = [(current, ImageAccess::StorageWrite), (opacity, ImageAccess::StorageWrite)];
        fg.add_pass("GI downsample", &images, &[], move |ctx| {
            compute(ctx, pipelines.gi_downsample, &down, groups)
        });
    }

    // The probes trace cones through last frame's finished voxels, which shading also read.
    let cache = Push { textures: [fg.texture(previous_probes).0, 0, 0, 0], output: fg.storage(probes, 0).0, ..push };
    let images = [
        (previous, ImageAccess::Sampled),
        (previous_opacity, ImageAccess::Sampled),
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
