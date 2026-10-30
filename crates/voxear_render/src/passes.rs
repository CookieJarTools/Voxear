//! The passes of one frame, in order:
//!
//! 1. **Reset** draw counters and stats, then build **Light clusters**.
//! 2. **Shadow cull** and **Shadows**: casters per sun cascade into the atlas.
//! 3. **Cull (early)** and **Opaque (early)**: what was visible last frame.
//! 4. **Depth pyramid** from that depth.
//! 5. **Cull (late)** and **Opaque (late)**: whatever the pyramid does not
//!    hide that early missed, which also records visibility for next frame.
//! 6. **Sky** behind everything, then **Tonemap** into the output.

use voxear_rhi::{StorageImageId, TextureId, vk};

use crate::{
    gpu::{CLUSTER_COUNT, DRAW_COMMAND_SIZE, LIST_COUNT, LIST_EARLY, LIST_LATE, LIST_SHADOW, Push, stat},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    pipelines::Pipelines,
    shadows::{CASCADE_RESOLUTION, CASCADES},
};

pub struct Targets {
    pub hdr: ImageId,
    pub depth: ImageId,
    pub depth_texture: TextureId,
    pub pyramid: ImageId,
    pub pyramid_texture: TextureId,
    pub pyramid_mips: Vec<StorageImageId>,
    pub pyramid_size: [u32; 2],
    pub shadows: Option<ImageId>,
    pub output: ImageId,
}

pub struct Draws {
    pub draws: BufferId,
    pub count: BufferId,
    pub instances: BufferId,
    pub visibility: BufferId,
    pub lod_state: BufferId,
    pub stats: BufferId,
    pub clusters: BufferId,
    pub draws_handle: vk::Buffer,
    pub count_handle: vk::Buffer,
    pub stats_handle: vk::Buffer,
    pub index_buffer: vk::Buffer,
}

pub struct Frame {
    pub pipelines: Pipelines,
    pub push: Push,
    pub instance_count: u32,
    pub extent: vk::Extent2D,
    pub hdr_texture: TextureId,
}

pub fn record(fg: &mut FrameGraph<'_>, frame: &Frame, targets: &Targets, draws: &Draws) {
    let culling = [
        (draws.count, BufferAccess::ComputeWrite),
        (draws.draws, BufferAccess::ComputeWrite),
        (draws.instances, BufferAccess::ComputeWrite),
        (draws.visibility, BufferAccess::ComputeWrite),
        (draws.lod_state, BufferAccess::ComputeWrite),
        (draws.stats, BufferAccess::ComputeWrite),
    ];
    let drawing = [
        (draws.draws, BufferAccess::IndirectRead),
        (draws.count, BufferAccess::IndirectRead),
        (draws.instances, BufferAccess::ShaderRead),
    ];
    let shading_buffers = [
        (draws.draws, BufferAccess::IndirectRead),
        (draws.count, BufferAccess::IndirectRead),
        (draws.instances, BufferAccess::ShaderRead),
        (draws.clusters, BufferAccess::ShaderRead),
        (draws.lod_state, BufferAccess::ShaderRead),
    ];

    let (count_handle, stats_handle) = (draws.count_handle, draws.stats_handle);
    fg.add_pass(
        "Reset",
        &[],
        &[(draws.count, BufferAccess::TransferWrite), (draws.stats, BufferAccess::TransferWrite)],
        move |ctx| unsafe {
            ctx.gpu.device.cmd_fill_buffer(ctx.cmd, count_handle, 0, u64::from(LIST_COUNT) * 4, 0);
            ctx.gpu.device.cmd_fill_buffer(ctx.cmd, stats_handle, 0, stat::COUNT as u64 * 4, 0);
        },
    );

    let push = frame.push;
    let instances = frame.instance_count;
    let pipelines = frame.pipelines;

    fg.add_pass(
        "Light clusters",
        &[],
        &[(draws.clusters, BufferAccess::ComputeWrite), (draws.stats, BufferAccess::ComputeWrite)],
        move |ctx| {
            bind(ctx, pipelines.clusters, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, CLUSTER_COUNT.div_ceil(64), 1, 1) };
        },
    );
    let indirect = Indirect { draws: draws.draws_handle, count: draws.count_handle, index_buffer: draws.index_buffer };

    if let Some(atlas) = targets.shadows {
        fg.add_pass("Shadow cull", &[], &culling, move |ctx| {
            cull(ctx, pipelines.cull, &push, LIST_SHADOW, instances, CASCADES as u32);
        });
        fg.add_pass("Shadows", &[(atlas, ImageAccess::DepthAttachment)], &drawing, move |ctx| {
            let size = CASCADE_RESOLUTION * 2;
            let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, Some(clear_depth(1.0)));
            begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
            bind(ctx, pipelines.shadow, vk::PipelineBindPoint::GRAPHICS, &push);
            for cascade in 0..CASCADES as u32 {
                let offset = vk::Offset2D {
                    x: (cascade % 2 * CASCADE_RESOLUTION) as i32,
                    y: (cascade / 2 * CASCADE_RESOLUTION) as i32,
                };
                set_viewport(ctx, offset, vk::Extent2D { width: CASCADE_RESOLUTION, height: CASCADE_RESOLUTION });
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: LIST_SHADOW + cascade, ..push });
                indirect.draw(ctx, LIST_SHADOW + cascade);
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }

    let shading = |color: ImageAccess| {
        let mut images = vec![(targets.hdr, color), (targets.depth, ImageAccess::DepthAttachment)];
        images.extend(targets.shadows.map(|atlas| (atlas, ImageAccess::Sampled)));
        images
    };
    let shading_push = Push { size: [CASCADE_RESOLUTION; 2], ..push };
    let (hdr, depth) = (targets.hdr, targets.depth);

    fg.add_pass("Cull (early)", &[], &culling, move |ctx| {
        cull(ctx, pipelines.cull, &push, LIST_EARLY, instances, 1);
    });
    fg.add_pass("Opaque (early)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
        opaque(ctx, pipelines.forward, &Push { list: LIST_EARLY, ..shading_push }, indirect, hdr, depth, true);
    });

    let pyramid = targets.pyramid;
    let pyramid_mips = targets.pyramid_mips.clone();
    let (depth_texture, pyramid_texture, pyramid_size) =
        (targets.depth_texture, targets.pyramid_texture, targets.pyramid_size);
    fg.add_pass(
        "Depth pyramid",
        &[(depth, ImageAccess::Sampled), (pyramid, ImageAccess::StorageWrite)],
        &[],
        move |ctx| {
            build_pyramid(ctx, pipelines.pyramid, &push, depth_texture, pyramid_texture, &pyramid_mips, pyramid_size)
        },
    );

    fg.add_pass("Cull (late)", &[(pyramid, ImageAccess::Sampled)], &culling, move |ctx| {
        cull(ctx, pipelines.cull, &push, LIST_LATE, instances, 1);
    });
    fg.add_pass("Opaque (late)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
        opaque(ctx, pipelines.forward, &Push { list: LIST_LATE, ..shading_push }, indirect, hdr, depth, false);
    });
    fg.sync_buffer(draws.stats, BufferAccess::HostRead);

    let extent = frame.extent;
    fg.add_pass("Sky", &[(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthRead)], &[], move |ctx| {
        let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL, None);
        begin_rendering(ctx, extent, &color, Some(&depth));
        fullscreen(ctx, pipelines.sky, &push);
    });

    let output = targets.output;
    let tonemap_push = Push { texture: frame.hdr_texture.0, ..push };
    fg.add_pass("Tonemap", &[(hdr, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)], &[], move |ctx| {
        let color = [attachment(ctx.view(output), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, extent, &color, None);
        fullscreen(ctx, pipelines.tonemap, &tonemap_push);
    });
}

/// The indirect draw buffers; each draw list occupies its own range.
#[derive(Clone, Copy)]
struct Indirect {
    draws: vk::Buffer,
    count: vk::Buffer,
    index_buffer: vk::Buffer,
}

impl Indirect {
    fn draw(self, ctx: &PassContext, list: u32) {
        let max = crate::MAX_INSTANCES as u32;
        unsafe {
            ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            ctx.gpu.device.cmd_draw_indexed_indirect_count(
                ctx.cmd,
                self.draws,
                u64::from(list) * u64::from(max) * DRAW_COMMAND_SIZE,
                self.count,
                u64::from(list) * 4,
                max,
                DRAW_COMMAND_SIZE as u32,
            );
        }
    }
}

fn cull(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, list: u32, instances: u32, lists: u32) {
    bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &Push { list, ..*push });
    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, instances.div_ceil(64).max(1), lists, 1) };
}

fn opaque(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    indirect: Indirect,
    hdr: ImageId,
    depth: ImageId,
    clear: bool,
) {
    let extent = ctx.extent(hdr);
    let color =
        [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, clear.then(|| clear_color([0.0; 4])))];
    let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, clear.then(|| clear_depth(0.0)));
    begin_rendering(ctx, extent, &color, Some(&depth));
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    indirect.draw(ctx, push.list);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// Mip 0 reads the depth buffer; every later mip reads the one before it.
fn build_pyramid(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    depth: TextureId,
    pyramid: TextureId,
    mips: &[StorageImageId],
    size: [u32; 2],
) {
    for (mip, output) in mips.iter().enumerate() {
        let mip = mip as u32;
        let width = (size[0] >> mip).max(1);
        let height = (size[1] >> mip).max(1);
        let push = Push {
            texture: if mip == 0 { depth.0 } else { pyramid.0 },
            level: mip.saturating_sub(1),
            output: output.0,
            size: [width, height],
            ..*push
        };
        bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, width.div_ceil(8), height.div_ceil(8), 1) };
        let barrier = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)];
        let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
        unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
    }
}

fn bind(ctx: &PassContext, pipeline: vk::Pipeline, bind_point: vk::PipelineBindPoint, push: &Push) {
    unsafe { ctx.gpu.device.cmd_bind_pipeline(ctx.cmd, bind_point, pipeline) };
    ctx.bindless.bind(ctx.gpu, ctx.cmd, bind_point);
    ctx.bindless.push(ctx.gpu, ctx.cmd, push);
}

fn clear_color(color: [f32; 4]) -> vk::ClearValue {
    vk::ClearValue { color: vk::ClearColorValue { float32: color } }
}

fn clear_depth(depth: f32) -> vk::ClearValue {
    vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 } }
}

/// A rendering attachment that clears when given a value and loads otherwise.
fn attachment(
    view: vk::ImageView,
    layout: vk::ImageLayout,
    clear: Option<vk::ClearValue>,
) -> vk::RenderingAttachmentInfo<'static> {
    let info = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(layout)
        .store_op(vk::AttachmentStoreOp::STORE);
    match clear {
        Some(value) => info.load_op(vk::AttachmentLoadOp::CLEAR).clear_value(value),
        None => info.load_op(vk::AttachmentLoadOp::LOAD),
    }
}

fn begin_rendering(
    ctx: &PassContext,
    extent: vk::Extent2D,
    color: &[vk::RenderingAttachmentInfo],
    depth: Option<&vk::RenderingAttachmentInfo>,
) {
    let area = vk::Rect2D { offset: vk::Offset2D::default(), extent };
    let mut info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(color);
    if let Some(depth) = depth {
        info = info.depth_attachment(depth);
    }
    unsafe { ctx.gpu.device.cmd_begin_rendering(ctx.cmd, &info) };
    set_viewport(ctx, vk::Offset2D::default(), extent);
}

fn set_viewport(ctx: &PassContext, offset: vk::Offset2D, extent: vk::Extent2D) {
    let viewport = vk::Viewport {
        x: offset.x as f32,
        y: offset.y as f32,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    unsafe {
        ctx.gpu.device.cmd_set_viewport(ctx.cmd, 0, &[viewport]);
        ctx.gpu.device.cmd_set_scissor(ctx.cmd, 0, &[vk::Rect2D { offset, extent }]);
    }
}

/// Draws one screen-covering triangle and ends the rendering scope.
fn fullscreen(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push) {
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    unsafe {
        ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
        ctx.gpu.device.cmd_end_rendering(ctx.cmd);
    }
}
