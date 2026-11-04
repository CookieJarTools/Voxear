//! Glass: materials drawn after the opaque scene and the sky, pane by pane
//! from the farthest, each over a copy of what lies behind it. The shaders
//! are in `shaders/glass.slang`.
//!
//! Each frame with glass in view:
//!
//! 1. **Glass copy** copies the picture so far into an image with mips, each
//!    the average of the one before, which rough glass blurs through.
//! 2. **Glass** draws each pane, farthest first, its whole mesh at its finest
//!    LOD, testing depth but writing none.
//!
//! Glass instances sit after the opaque ones in the instance list, past the
//! count culling, shadows and the GI's rays see, so they cast no shadows and
//! light passes them. Panes read only the copy, made before any is drawn, so
//! glass behind glass does not show through it.

use anyhow::Result;
use voxear_core::math::{Mat4, Vec3};
use voxear_rhi::{Bindless, Gpu, ImageDesc, StorageImageId, TextureId, vk};

use crate::{
    MaterialHandle, MeshHandle,
    assets::Assets,
    gpu::{GpuInstance, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind},
    pipelines::{HDR_FORMAT, Pipelines},
};

/// How many mips the copy behind the glass keeps: enough for the roughest
/// glass to blur what lies behind it to soft shapes.
const COPY_MIPS: u32 = 7;

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
    pub index_buffer: vk::Buffer,
}

/// Records the copy and the panes over it.
pub fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), glass: Glass, targets: GlassTargets) {
    let Glass { copy, copy_texture, copy_mips, size, panes } = glass;
    let GlassTargets { hdr, depth, sampled, clusters, index_buffer } = targets;
    let hdr_texture = fg.texture(hdr);
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

    let mut images =
        vec![(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthRead), (copy, ImageAccess::Sampled)];
    images.extend(sampled.into_iter().map(|image| (image, ImageAccess::Sampled)));
    let extent = vk::Extent2D { width: size[0], height: size[1] };
    let push = Push { texture: copy_texture.0, ..push };
    fg.add_pass("Glass", &images, &[(clusters, BufferAccess::ShaderRead)], move |ctx| {
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
        for pane in &panes {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: pane.instance, ..push });
            unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, pane.index_count, 1, pane.first_index, 0, 0) };
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}
