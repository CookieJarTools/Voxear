//! The renderer. Today it owns the swapchain and draws the bootstrap pass;
//! the render graph, GPU-driven geometry and lighting build on this.

mod triangle;

use anyhow::Result;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use voxear_rhi::{Frames, Gpu, GpuInfo, Swapchain, transition_image, vk};

use crate::triangle::TrianglePass;

/// `#111113` in linear space: the editor's background, close to Linear's.
const CLEAR: [f32; 4] = [0.0056, 0.0056, 0.0065, 1.0];

pub struct Renderer {
    gpu: Gpu,
    swapchain: Swapchain,
    frames: Frames,
    triangle: TrianglePass,
    pending_size: Option<(u32, u32)>,
}

impl Renderer {
    /// # Safety
    /// The window must outlive the renderer.
    pub unsafe fn new(display: RawDisplayHandle, window: RawWindowHandle, width: u32, height: u32) -> Result<Self> {
        let (gpu, surface) = unsafe { Gpu::new(display, window)? };
        let vsync = std::env::var("VOXEAR_VSYNC").map(|v| v != "0").unwrap_or(true);
        let swapchain = Swapchain::new(&gpu, surface, width, height, vsync)?;
        let frames = Frames::new(&gpu)?;
        let triangle = TrianglePass::new(&gpu, swapchain.format.format)?;
        Ok(Self { gpu, swapchain, frames, triangle, pending_size: None })
    }

    pub fn gpu(&self) -> &GpuInfo {
        &self.gpu.info
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.pending_size = Some((width, height));
    }

    pub fn render(&mut self, time: f32) -> Result<()> {
        if let Some((width, height)) = self.pending_size.take() {
            self.swapchain.recreate(&self.gpu, width, height)?;
        }
        if !self.swapchain.is_renderable() {
            return Ok(());
        }
        let Some(frame) = self.frames.begin(&self.gpu, &self.swapchain)? else {
            self.pending_size = Some((self.swapchain.extent.width, self.swapchain.extent.height));
            return Ok(());
        };

        let cmd = frame.cmd;
        transition_image(
            &self.gpu,
            cmd,
            frame.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            (vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, vk::AccessFlags2::NONE),
            (vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE),
        );

        let attachment = [vk::RenderingAttachmentInfo::default()
            .image_view(frame.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: CLEAR } })];
        let area = vk::Rect2D { offset: vk::Offset2D::default(), extent: frame.extent };
        let rendering = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(&attachment);
        unsafe { self.gpu.device.cmd_begin_rendering(cmd, &rendering) };
        self.triangle.record(&self.gpu, cmd, frame.extent, time);
        unsafe { self.gpu.device.cmd_end_rendering(cmd) };

        transition_image(
            &self.gpu,
            cmd,
            frame.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            (vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE),
            (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
        );

        if self.frames.end(&self.gpu, &self.swapchain, frame)? {
            self.pending_size = Some((self.swapchain.extent.width, self.swapchain.extent.height));
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.gpu.wait_idle();
        self.triangle.destroy(&self.gpu);
        self.frames.destroy(&self.gpu);
        self.swapchain.destroy(&self.gpu);
    }
}
