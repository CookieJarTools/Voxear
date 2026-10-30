//! The UI over the finished frame: the quads `voxear_ui` lays out, from a
//! per-frame buffer, with text sampled from its glyph atlas. The shader is
//! `shaders/ui.slang`.

use anyhow::Result;
use voxear_rhi::{Bindless, Buffer, Gpu, Image, ImageDesc, TextureId, vk};
use voxear_ui::{AtlasImage, Quad, UiFrame};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind},
};

/// Quads the per-frame buffer holds; more are left out.
pub const MAX_UI_QUADS: u64 = 16 * 1024;

/// What the UI draws, kept until the next [`set`](Self::set).
#[derive(Default)]
pub(crate) struct UiGpu {
    quads: Vec<Quad>,
    atlas: Option<(Image, TextureId)>,
}

impl UiGpu {
    pub fn set(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: &UiFrame) -> Result<()> {
        if frame.quads.len() as u64 > MAX_UI_QUADS {
            tracing::warn!("the UI drew {} quads; only {MAX_UI_QUADS} fit", frame.quads.len());
        }
        self.quads.clear();
        self.quads.extend(frame.quads.iter().take(MAX_UI_QUADS as usize));
        if let Some(atlas) = &frame.atlas {
            self.upload(gpu, bindless, atlas)?;
        }
        Ok(())
    }

    /// The atlas changes only when glyphs appear for the first time, so the
    /// GPU simply finishes the frames that sample the old one first.
    fn upload(&mut self, gpu: &Gpu, bindless: &mut Bindless, atlas: &AtlasImage) -> Result<()> {
        gpu.wait_idle();
        let fits = self
            .atlas
            .as_ref()
            .is_some_and(|(image, _)| (image.desc.width, image.desc.height) == (atlas.width, atlas.height));
        if !fits {
            self.destroy(gpu, bindless);
            let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
            let image = gpu
                .create_image("glyph atlas", ImageDesc::new(atlas.width, atlas.height, vk::Format::R8_UNORM, usage))?;
            let texture = bindless.add_texture(gpu, image.view);
            self.atlas = Some((image, texture));
        }
        let (image, _) = self.atlas.as_ref().expect("created above");
        gpu.upload_mips(image, std::slice::from_ref(&atlas.pixels))
    }

    /// Writes this frame's quads and adds the pass that draws them onto `output`.
    pub fn record(
        &self,
        fg: &mut FrameGraph<'_>,
        pipeline: vk::Pipeline,
        push: Push,
        buffer: &Buffer,
        output: (ImageId, vk::Extent2D),
    ) {
        if self.quads.is_empty() {
            return;
        }
        buffer.write(0, &self.quads);
        let (output, extent) = output;
        let texture = self.atlas.as_ref().map_or(NO_TEXTURE, |(_, texture)| texture.0);
        let push = Push { texture, size: [extent.width, extent.height], ..push };
        let vertices = self.quads.len() as u32 * 6;
        fg.add_pass("UI", &[(output, ImageAccess::ColorAttachment)], &[], move |ctx| {
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(ctx.view(output))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE)];
            begin_rendering(ctx, ctx.extent(output), &color, None);
            bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe {
                ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0);
                ctx.gpu.device.cmd_end_rendering(ctx.cmd);
            }
        });
    }

    /// The caller makes sure the GPU is idle.
    pub fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        if let Some((image, texture)) = self.atlas.take() {
            bindless.remove_texture(texture);
            gpu.destroy_image(image);
        }
    }
}
