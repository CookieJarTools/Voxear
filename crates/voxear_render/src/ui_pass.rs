//! The UI over the finished frame: the quads `voxear_ui` lays out, from a
//! per-frame buffer, with text sampled from its glyph atlas. The shader is
//! `shaders/ui.slang`.
//!
//! There are two layers, each a Voxear UI of its own with its own atlas: the
//! game's UI, over the game's view (`set_game`), and over it the window's
//! own, the editor or a player's tools (`set`). Both share the buffer, the
//! game's quads first, and draw in one pass, each moved to where its area is
//! in the output. Each output has its two layers, and their quads follow the
//! outputs before them in the buffer.

use anyhow::Result;
use voxear_rhi::{Bindless, Buffer, Gpu, Image, ImageDesc, TextureId, vk};
use voxear_ui::{AtlasImage, Quad, UiFrame};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind},
};

/// Quads the per-frame buffer holds, both layers together; more are left out.
pub const MAX_UI_QUADS: u64 = 16 * 1024;

/// One UI's quads and glyph atlas, and where its area starts in the output.
#[derive(Default)]
struct Layer {
    quads: Vec<Quad>,
    atlas: Option<(Image, TextureId)>,
    /// The version of the atlas on the GPU.
    version: u64,
    origin: [u32; 2],
}

/// What the UI draws, kept until the next [`set`](Self::set) or
/// [`set_game`](Self::set_game).
#[derive(Default)]
pub(crate) struct UiGpu {
    game: Layer,
    window: Layer,
}

impl Layer {
    fn set(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: &UiFrame, room: usize) -> Result<()> {
        if frame.quads.len() > room {
            tracing::warn!("the UI drew {} quads; only {room} fit", frame.quads.len());
        }
        self.quads.clear();
        self.quads.extend(frame.quads.iter().take(room));
        if let Some(atlas) = frame.atlas.as_deref().filter(|a| a.version != self.version) {
            self.upload(gpu, bindless, atlas)?;
            self.version = atlas.version;
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

    fn texture(&self) -> u32 {
        self.atlas.as_ref().map_or(NO_TEXTURE, |(_, texture)| texture.0)
    }

    fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        if let Some((image, texture)) = self.atlas.take() {
            bindless.remove_texture(texture);
            gpu.destroy_image(image);
        }
    }
}

impl UiGpu {
    /// The window's own UI, over everything.
    pub fn set(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: &UiFrame) -> Result<()> {
        let room = MAX_UI_QUADS as usize - self.game.quads.len();
        self.window.set(gpu, bindless, frame, room)
    }

    /// The game's UI, laid out for its area, which starts at `origin` in the
    /// output, under the window's UI.
    pub fn set_game(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: &UiFrame, origin: [u32; 2]) -> Result<()> {
        self.game.origin = origin;
        let room = MAX_UI_QUADS as usize - self.window.quads.len();
        self.game.set(gpu, bindless, frame, room)
    }

    /// Whether a quad shows the view through it blurred (`backdrop-filter`),
    /// so the frame keeps a blurred copy of the view for it.
    pub fn wants_backdrop(&self) -> bool {
        let quads = self.game.quads.iter().chain(&self.window.quads);
        quads.into_iter().any(|q| q.kind == Quad::SHAPE && q.flags & Quad::BACKDROP != 0)
    }

    /// Writes this frame's quads into `buffer` from its `first` quad on and
    /// adds the pass that draws them onto `output`, returning how many it
    /// wrote. `sampled` are images drawn this frame that the UI shows, as
    /// the preview atlas. `backdrop` is the view blurred, with its size, and
    /// where the view is in the output, for quads that show it.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        fg: &mut FrameGraph<'_>,
        (pipeline, push): (vk::Pipeline, Push),
        buffer: &Buffer,
        first: usize,
        output: (ImageId, vk::Extent2D),
        sampled: &[ImageId],
        backdrop: Option<(ImageId, [u32; 2], vk::Rect2D)>,
    ) -> usize {
        // Every output's UI shares the buffer; what does not fit is left out.
        let room = (MAX_UI_QUADS as usize).saturating_sub(first);
        let games = self.game.quads.len().min(room);
        let windows = self.window.quads.len().min(room - games);
        if games + windows == 0 {
            return 0;
        }
        let quad = size_of::<Quad>();
        buffer.write((first * quad) as u64, &self.game.quads[..games]);
        buffer.write(((first + games) * quad) as u64, &self.window.quads[..windows]);
        let (output, extent) = output;
        let pack = |a: u32, b: u32| a.min(0xffff) | b.min(0xffff) << 16;
        let textures = match backdrop {
            Some((blurred, [w, h], view)) => [
                fg.texture(blurred).0,
                pack(view.offset.x as u32, view.offset.y as u32),
                pack(view.extent.width, view.extent.height),
                pack(w, h),
            ],
            None => [NO_TEXTURE, 0, 0, 0],
        };
        // Each layer's atlas, its area's start in `list`, which the UI shader reads as its offset,
        // and its first quad in `level`.
        let base = Push { size: [extent.width, extent.height], textures, ..push };
        let list = pack(self.game.origin[0], self.game.origin[1]);
        let game = Push { texture: self.game.texture(), list, level: first as u32, ..base };
        let window = Push { texture: self.window.texture(), list: 0, level: (first + games) as u32, ..base };
        let draws = [(game, games as u32), (window, windows as u32)];
        let mut images = vec![(output, ImageAccess::ColorAttachment)];
        images.extend(sampled.iter().map(|&image| (image, ImageAccess::Sampled)));
        images.extend(backdrop.map(|(blurred, ..)| (blurred, ImageAccess::Sampled)));
        fg.add_pass("UI", &images, &[], move |ctx| {
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(ctx.view(output))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE)];
            begin_rendering(ctx, ctx.extent(output), &color, None);
            for (push, count) in draws.into_iter().filter(|&(_, count)| count > 0) {
                bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
                unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, count * 6, 1, 0, 0) };
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
        games + windows
    }

    /// The caller makes sure the GPU is idle.
    pub fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        self.game.destroy(gpu, bindless);
        self.window.destroy(gpu, bindless);
    }
}
