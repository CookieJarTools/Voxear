//! Glass: materials drawn after the opaque scene and the sky, pane by pane
//! from the farthest, each over a copy of what lies behind it. The shaders
//! are in `shaders/glass.slang`.
//!
//! Each frame with glass in view:
//!
//! 1. **Glass shadows** draws every pane into the glass shadow atlas, from
//!    the sun's cascades and the local lights' views that cast shadows: what
//!    each lets through of the light, multiplied together where panes stand
//!    one behind another, the nearest's depth, and their smoothness. Shading
//!    filters the light by it (`glassLight` in `shaders/shading.slang`), so
//!    green glass throws green light and frosted glass a soft patch of it.
//! 2. **Glass copy** copies the picture so far into an image with mips, each
//!    the average of the one before, which rough glass blurs through.
//! 3. **Glass** draws the panes, farthest first, their whole meshes at their
//!    finest LOD, testing depth but writing none. A pane in front of others
//!    is drawn over a copy made again after them, so glass shows through
//!    glass; past `MAX_COPIES` copies, the nearest panes share the last.
//!
//! Glass instances sit after the opaque ones in the instance list, past the
//! count culling, depth shadows and the GI's rays see, so they cast no dark
//! shadow and the GI's rays pass them.

use anyhow::Result;
use voxear_core::math::{Mat4, Vec3};
use voxear_rhi::{Bindless, Gpu, ImageDesc, StorageImageId, TextureId, vk};

use crate::{
    MaterialHandle, MeshHandle,
    assets::Assets,
    gpu::{GpuInstance, LIST_LOCAL_SHADOW, LIST_SHADOW, LOCAL_SHADOW_COLUMNS, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind, set_viewport},
    pipelines::{HDR_FORMAT, Pipelines},
    shadows::CASCADES,
};

/// How many mips the copy behind the glass keeps: enough for the roughest
/// glass to blur what lies behind it to soft shapes.
const COPY_MIPS: u32 = 7;
/// The most copies a frame makes of what lies behind its glass, each a
/// full-size copy and its mips.
const MAX_COPIES: usize = 6;

/// The glass shadow atlas: the sun's cascades in its top `SUN_SIZE` square,
/// laid out as the shadow atlas's, and under it the local lights' views,
/// `LOCAL_TILE` square, as the local shadow atlas lays them.
pub const SHADOW_ATLAS: [u32; 2] = [1024, 1792];
const SUN_SIZE: u32 = 1024;
const LOCAL_TILE: u32 = 128;
/// What the glass shadow atlas holds: the light let through, with the
/// nearest pane's depth in alpha; and the panes' smoothness. Both are read
/// and written as storage images for their mips.
const LIGHT_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const SMOOTHNESS_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
pub const SHADOW_FORMATS: [vk::Format; 2] = [LIGHT_FORMAT, SMOOTHNESS_FORMAT];
/// The glass shadow atlas's mips, each the average of the one before, which
/// the light through rough glass is read from as wide as it spreads: down to
/// 4 texels across a local light's view (`GLASS_COARSEST` in
/// `shaders/lights.slang` is the last).
const SHADOW_MIPS: u32 = 6;

/// One pane: its place in the instance list, and its mesh's finest LOD's
/// indices.
#[derive(Clone, Copy, Debug)]
pub struct Pane {
    pub instance: u32,
    pub first_index: u32,
    pub index_count: u32,
}

/// Moves the glass instances, and the entities beside them, after the rest,
/// the farthest from the camera first, and returns how many are not glass.
pub fn sort_last<T>(instances: &mut Vec<GpuInstance>, entities: &mut Vec<T>, camera: Vec3, assets: &Assets) -> usize {
    let glass = |instance: &GpuInstance| assets.is_glass(MaterialHandle(instance.material));
    if !instances.iter().any(glass) {
        return instances.len();
    }
    let distance =
        |instance: &GpuInstance| Mat4::from_cols_array_2d(&instance.model).w_axis.truncate().distance(camera);
    let mut order: Vec<usize> = (0..instances.len()).collect();
    // Opaque first, in the order they came; then glass, farthest first.
    order.sort_by(|&a, &b| {
        let (a, b) = (&instances[a], &instances[b]);
        glass(a).cmp(&glass(b)).then_with(|| match glass(a) {
            true => distance(b).total_cmp(&distance(a)),
            false => std::cmp::Ordering::Equal,
        })
    });
    let opaque = order.iter().take_while(|&&index| !glass(&instances[index])).count();
    let mut taken: Vec<Option<T>> = entities.drain(..).map(Some).collect();
    *entities = order.iter().map(|&index| taken[index].take().expect("each index once")).collect();
    *instances = order.iter().map(|&index| instances[index]).collect();
    opaque
}

/// The panes among `instances` from `first` on, each at its finest LOD.
pub fn panes(instances: &[GpuInstance], first: usize, assets: &Assets) -> Vec<Pane> {
    let numbered = instances.iter().enumerate().skip(first);
    numbered
        .filter_map(|(index, instance)| {
            let (first_index, index_count) = assets.first_lod(MeshHandle(instance.mesh))?;
            Some(Pane { instance: index as u32, first_index, index_count })
        })
        .collect()
}

/// The glass shadow atlas's two images, for a frame with glass and shadows,
/// and each mip of each to write.
#[derive(Clone, Debug)]
pub struct Shadows {
    pub light: ImageId,
    pub smoothness: ImageId,
    mips: Vec<[StorageImageId; 2]>,
}

impl Shadows {
    pub fn new(fg: &mut FrameGraph<'_>, (gpu, bindless): (&Gpu, &mut Bindless)) -> Result<Self> {
        let usage =
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE;
        let [width, height] = SHADOW_ATLAS;
        let desc = |format| {
            let mut desc = ImageDesc::new(width, height, format, usage).with_mips();
            desc.mip_levels = desc.mip_levels.min(SHADOW_MIPS);
            desc
        };
        let light = fg.create_image(gpu, bindless, "glass shadow light", desc(LIGHT_FORMAT))?;
        let smoothness = fg.create_image(gpu, bindless, "glass shadow smoothness", desc(SMOOTHNESS_FORMAT))?;
        let mips = (0..SHADOW_MIPS as usize).map(|mip| [fg.storage(light, mip), fg.storage(smoothness, mip)]).collect();
        Ok(Self { light, smoothness, mips })
    }

    /// The images, as the frame's `glass_shadow` names them: each texture's
    /// index plus one.
    pub fn frame_field(&self, fg: &FrameGraph<'_>) -> [u32; 2] {
        [fg.texture(self.light).0 + 1, fg.texture(self.smoothness).0 + 1]
    }

    pub fn images(&self) -> [ImageId; 2] {
        [self.light, self.smoothness]
    }
}

/// Records every pane into the glass shadow atlas: from each of the sun's
/// cascades when `sun`, and from each view of each local light that casts
/// shadows, given as its first view and how many it takes.
pub fn record_shadows(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    shadows: Shadows,
    panes: Vec<Pane>,
    (sun, lights): (bool, Vec<(u32, u32)>),
    index_buffer: vk::Buffer,
) {
    let images = [(shadows.light, ImageAccess::ColorAttachment), (shadows.smoothness, ImageAccess::ColorAttachment)];
    fg.add_pass("Glass shadows", &images, &[], move |ctx| {
        let [width, height] = SHADOW_ATLAS;
        let clear = |view, color: [f32; 4]| {
            vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: color } })
        };
        // Where nothing is drawn, all the light passes, from as far as can be, and nothing scatters it.
        let light = clear(ctx.first_mip(shadows.light), [1.0, 1.0, 1.0, 65504.0]);
        let color = [light, clear(ctx.first_mip(shadows.smoothness), [1.0; 4])];
        begin_rendering(ctx, vk::Extent2D { width, height }, &color, None);
        bind(ctx, pipelines.glass_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32) };
        let cascade = SUN_SIZE / 2;
        let cascades = (0..CASCADES as u32).filter(|_| sun).map(|cascade_index| {
            let corner = [cascade_index % 2 * cascade, cascade_index / 2 * cascade];
            (LIST_SHADOW + cascade_index, 0, corner, cascade)
        });
        let views = lights.iter().enumerate().flat_map(|(light, &(first, faces))| {
            (first..first + faces).map(move |view| {
                let (column, row) = (view % LOCAL_SHADOW_COLUMNS, view / LOCAL_SHADOW_COLUMNS);
                let corner = [column * LOCAL_TILE, SUN_SIZE + row * LOCAL_TILE];
                (LIST_LOCAL_SHADOW + light as u32, view, corner, LOCAL_TILE)
            })
        });
        for (list, level, [x, y], size) in cascades.chain(views) {
            set_viewport(ctx, vk::Offset2D { x: x as i32, y: y as i32 }, vk::Extent2D { width: size, height: size });
            for pane in &panes {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level, source: pane.instance, ..push });
                unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, pane.index_count, 1, pane.first_index, 0, 0) };
            }
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });

    let images = [(shadows.light, ImageAccess::StorageWrite), (shadows.smoothness, ImageAccess::StorageWrite)];
    let (light, smoothness) = (fg.texture(shadows.light), fg.texture(shadows.smoothness));
    let mips = shadows.mips;
    fg.add_pass("Glass shadow mips", &images, &[], move |ctx| {
        for (mip, [light_mip, smoothness_mip]) in mips.iter().enumerate().skip(1) {
            let mip = mip as u32;
            let size = SHADOW_ATLAS.map(|side| (side >> mip).max(1));
            let push = Push {
                texture: light.0,
                textures: [smoothness.0, smoothness_mip.0, 0, 0],
                output: light_mip.0,
                level: mip - 1,
                size,
                ..push
            };
            bind(ctx, pipelines.glass_shadow_mip, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
            // Each mip reads the one before.
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        }
    });
}

/// The copy glass is drawn over, and its panes.
#[derive(Clone)]
pub struct Glass {
    copy: ImageId,
    copy_texture: TextureId,
    copy_mips: Vec<StorageImageId>,
    size: [u32; 2],
    panes: Vec<Pane>,
}

impl Glass {
    pub fn new(
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        extent: vk::Extent2D,
        panes: Vec<Pane>,
    ) -> Result<Self> {
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let mut desc = ImageDesc::new(extent.width, extent.height, HDR_FORMAT, usage).with_mips();
        desc.mip_levels = desc.mip_levels.min(COPY_MIPS);
        let copy = fg.create_image(gpu, bindless, "glass copy", desc)?;
        Ok(Self {
            copy,
            copy_texture: fg.texture(copy),
            copy_mips: (0..desc.mip_levels as usize).map(|mip| fg.storage(copy, mip)).collect(),
            size: [extent.width, extent.height],
            panes,
        })
    }
}

/// What the glass pass draws into and reads, beside the copy.
pub struct GlassTargets {
    pub hdr: ImageId,
    pub depth: ImageId,
    /// What shading reads besides: the shadow atlas, the GI's probes and the
    /// atmosphere's tables.
    pub sampled: Vec<ImageId>,
    pub clusters: BufferId,
    /// The pushes through foliage, which the panes' vertices read too.
    pub pushes: BufferId,
    pub index_buffer: vk::Buffer,
}

/// Records the panes, farthest first, in up to `MAX_COPIES` layers, each
/// over a copy of the picture made after the layers behind it were drawn.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), glass: Glass, targets: GlassTargets) {
    let Glass { copy, copy_texture, copy_mips, size, panes } = glass;
    let GlassTargets { hdr, depth, sampled, clusters, pushes, index_buffer } = targets;
    let layer = panes.len().div_ceil(MAX_COPIES).max(1);
    // The farthest panes share the first copy, so the nearest, which most often stand in front of others, each
    // have one of their own.
    let first = panes.len() - (panes.len() - 1) / layer * layer;
    let layers = std::iter::once(&panes[..first]).chain(panes[first..].chunks(layer));
    for drawn in layers {
        record_copy(fg, (pipelines, push), (hdr, copy, copy_texture), &copy_mips, size);
        let mut images =
            vec![(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthRead), (copy, ImageAccess::Sampled)];
        images.extend(sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
        let extent = vk::Extent2D { width: size[0], height: size[1] };
        let push = Push { texture: copy_texture.0, ..push };
        let buffers = [(clusters, BufferAccess::ShaderRead), (pushes, BufferAccess::ShaderRead)];
        let drawn = drawn.to_vec();
        fg.add_pass("Glass", &images, &buffers, move |ctx| {
            let load = |view, layout| {
                vk::RenderingAttachmentInfo::default()
                    .image_view(view)
                    .image_layout(layout)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE)
            };
            let color = [load(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let depth = load(ctx.view(depth), vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL);
            begin_rendering(ctx, extent, &color, Some(&depth));
            bind(ctx, pipelines.glass, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32) };
            for pane in &drawn {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: pane.instance, ..push });
                unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, pane.index_count, 1, pane.first_index, 0, 0) };
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
}

/// Records a copy of the picture so far, and its mips, for glass to be
/// drawn over.
fn record_copy(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    (hdr, copy, copy_texture): (ImageId, ImageId, TextureId),
    copy_mips: &[StorageImageId],
    size: [u32; 2],
) {
    let hdr_texture = fg.texture(hdr);
    let copy_mips = copy_mips.to_vec();
    let images = [(hdr, ImageAccess::Sampled), (copy, ImageAccess::StorageWrite)];
    fg.add_pass("Glass copy", &images, &[], move |ctx| {
        for (mip, output) in copy_mips.iter().enumerate() {
            let mip = mip as u32;
            let (width, height) = ((size[0] >> mip).max(1), (size[1] >> mip).max(1));
            let push = Push {
                texture: if mip == 0 { hdr_texture.0 } else { copy_texture.0 },
                level: mip.saturating_sub(1),
                output: output.0,
                size: [width, height],
                ..push
            };
            bind(ctx, pipelines.glass_copy, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, width.div_ceil(8), height.div_ceil(8), 1) };
            // Each mip reads the one before.
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        }
    });
}
