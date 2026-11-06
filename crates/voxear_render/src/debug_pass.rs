//! Debug draw over the finished frame: the lines and solid shapes
//! `voxear_core::debug_draw` holds, from per-frame buffers, the lines each as wide as it asks or a pixel and a
//! half, and dimmed where the scene is in front of it unless drawn on top,
//! so it stays readable without hiding what is in front. Drawn after post-processing, so their colours
//! are exactly as asked, and before the UI. The shader is
//! `shaders/debug.slang`.

use voxear_core::debug_draw;
use voxear_rhi::{Buffer, vk};

use crate::{
    gpu::{DEBUG_IN_METRES, DEBUG_ON_TOP, GpuDebugLine, GpuDebugTriangle, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering_in, bind},
};

/// Lines and triangles the per-frame buffers hold, as many as debug draw
/// keeps.
pub const MAX_DEBUG_LINES: u64 = debug_draw::MAX_LINES as u64;
pub const MAX_DEBUG_TRIANGLES: u64 = debug_draw::MAX_TRIANGLES as u64;
/// How wide lines are, in pixels at a scale of one.
const WIDTH: f32 = 1.5;

/// Writes the lines and solid shapes debug draw holds now, those of
/// behaviours' gizmos only when `gizmos`, and adds the pass that draws them
/// onto `view` of `output`, reading the scene's depth: the solids first, so
/// lines along their edges show over them.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (lines_pipeline, solids_pipeline, push): (vk::Pipeline, vk::Pipeline, Push),
    (line_buffer, triangle_buffer): (&Buffer, &Buffer),
    (output, view): (ImageId, vk::Rect2D),
    depth: ImageId,
    gizmos: bool,
) {
    let mut lines = debug_draw::lines();
    let mut triangles = debug_draw::triangles();
    if !gizmos {
        lines.retain(|line| !line.gizmo);
        triangles.retain(|triangle| !triangle.gizmo);
    }
    if lines.is_empty() && triangles.is_empty() {
        return;
    }
    let on_top = |on: bool| if on { DEBUG_ON_TOP } else { 0 };
    let lines: Vec<GpuDebugLine> = lines
        .iter()
        .take(MAX_DEBUG_LINES as usize)
        .map(|line| GpuDebugLine {
            from: line.from.to_array(),
            flags: on_top(line.on_top) | if line.in_metres { DEBUG_IN_METRES } else { 0 },
            to: line.to.to_array(),
            width: line.width,
            color: line.color,
        })
        .collect();
    let triangles: Vec<GpuDebugTriangle> = triangles
        .iter()
        .take(MAX_DEBUG_TRIANGLES as usize)
        .map(|triangle| {
            let [a, b, c] = triangle.corners.map(|corner| corner.to_array());
            GpuDebugTriangle { a, flags: on_top(triangle.on_top), b, c, color: triangle.color, ..Default::default() }
        })
        .collect();
    if !lines.is_empty() {
        line_buffer.write(0, &lines);
    }
    if !triangles.is_empty() {
        triangle_buffer.write(0, &triangles);
    }
    let depth_texture = fg.texture(depth).0;
    let offset = [view.offset.x as u32, view.offset.y as u32];
    let push = Push { textures: [depth_texture, offset[0], offset[1], WIDTH.to_bits()], ..push };
    let (line_vertices, triangle_vertices) = (lines.len() as u32 * 6, triangles.len() as u32 * 3);
    let images = [(output, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    fg.add_pass("Debug draw", &images, &[], move |ctx| {
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(ctx.view(output))
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        begin_rendering_in(ctx, view, &color);
        for (pipeline, vertices) in [(solids_pipeline, triangle_vertices), (lines_pipeline, line_vertices)] {
            if vertices > 0 {
                bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
                unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0) };
            }
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}
