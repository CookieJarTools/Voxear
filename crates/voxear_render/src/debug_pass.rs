//! Debug draw over the finished frame: the lines `voxear_core::debug_draw`
//! holds, from a per-frame buffer, each as wide as it asks or a pixel and a
//! half, and dimmed where the scene is in front of it unless drawn on top,
//! so it stays readable without hiding what is in front. Drawn after post-processing, so their colours
//! are exactly as asked, and before the UI. The shader is
//! `shaders/debug.slang`.

use voxear_core::debug_draw;
use voxear_rhi::{Buffer, vk};

use crate::{
    gpu::{DEBUG_ON_TOP, GpuDebugLine, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering_in, bind},
};

/// Lines the per-frame buffer holds, as many as debug draw keeps.
pub const MAX_DEBUG_LINES: u64 = debug_draw::MAX_LINES as u64;
/// How wide lines are, in pixels at a scale of one.
const WIDTH: f32 = 1.5;

/// Writes the lines debug draw holds now, and adds the pass that draws them
/// onto `view` of `output`, reading the scene's depth.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    pipeline: vk::Pipeline,
    push: Push,
    buffer: &Buffer,
    (output, view): (ImageId, vk::Rect2D),
    depth: ImageId,
) {
    let lines = debug_draw::lines();
    if lines.is_empty() {
        return;
    }
    let gpu: Vec<GpuDebugLine> = lines
        .iter()
        .take(MAX_DEBUG_LINES as usize)
        .map(|line| GpuDebugLine {
            from: line.from.to_array(),
            flags: if line.on_top { DEBUG_ON_TOP } else { 0 },
            to: line.to.to_array(),
            width: line.width,
            color: line.color,
        })
        .collect();
    buffer.write(0, &gpu);
    let depth_texture = fg.texture(depth).0;
    let offset = [view.offset.x as u32, view.offset.y as u32];
    let push = Push { textures: [depth_texture, offset[0], offset[1], WIDTH.to_bits()], ..push };
    let vertices = gpu.len() as u32 * 6;
    let images = [(output, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    fg.add_pass("Debug draw", &images, &[], move |ctx| {
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(ctx.view(output))
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        begin_rendering_in(ctx, view, &color);
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe {
            ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0);
            ctx.gpu.device.cmd_end_rendering(ctx.cmd);
        }
    });
}
