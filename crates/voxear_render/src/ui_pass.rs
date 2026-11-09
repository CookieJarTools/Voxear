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

/// Quads the per-frame buffer holds, every layer and UI panel together; more
/// are left out.
pub const MAX_UI_QUADS: u64 = 32 * 1024;
/// The shaded backgrounds' values the per-frame buffer holds, every layer
/// and UI panel together, four numbers each.
pub const MAX_UI_VALUES: u64 = 64 * 1024;

/// One UI's quads, glyph atlas and distance fields, and where its area
/// starts in the output.
#[derive(Default)]
pub(crate) struct Layer {
    pub(crate) quads: Vec<Quad>,
    /// What its shaded backgrounds read.
    pub(crate) values: Vec<[f32; 4]>,
    atlas: Option<(Image, TextureId)>,
    /// The version of the atlas on the GPU.
    pub(crate) version: u64,
    /// Its glyphs as distance fields, with mips, and the version on the GPU.
    fields: Option<(Image, TextureId)>,
    pub(crate) fields_version: u64,
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
    pub(crate) fn set(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: &UiFrame, room: usize) -> Result<()> {
        if frame.quads.len() > room {
            tracing::warn!("the UI drew {} quads; only {room} fit", frame.quads.len());
        }
        self.quads.clear();
        self.quads.extend(frame.quads.iter().take(room));
        self.values.clear();
        self.values.extend(frame.values.iter().take(MAX_UI_VALUES as usize / 2));
        if let Some(atlas) = frame.atlas.as_deref().filter(|a| a.version != self.version) {
            upload(gpu, bindless, &mut self.atlas, ("glyph atlas", atlas), vec![atlas.pixels.clone()])?;
            self.version = atlas.version;
        }
        if let Some(fields) = frame.fields.as_deref().filter(|f| f.version != self.fields_version) {
            upload(gpu, bindless, &mut self.fields, ("glyph distance fields", fields), mips(fields))?;
            self.fields_version = fields.version;
        }
        Ok(())
    }

    pub(crate) fn texture(&self) -> u32 {
        self.atlas.as_ref().map_or(NO_TEXTURE, |(_, texture)| texture.0)
    }

    /// Its distance fields' texture, which `ui.slang` reads from
    /// `push.source`.
    pub(crate) fn fields_texture(&self) -> u32 {
        self.fields.as_ref().map_or(NO_TEXTURE, |(_, texture)| texture.0)
    }

    pub(crate) fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        for (image, texture) in [self.atlas.take(), self.fields.take()].into_iter().flatten() {
            bindless.remove_texture(texture);
            gpu.destroy_image(image);
        }
    }
}

/// Puts `levels` of an atlas into `slot`'s image, made anew when its size
/// or mips differ. An atlas changes only when glyphs appear for the first
/// time, so the GPU simply finishes the frames that sample the old one
/// first.
fn upload(
    gpu: &Gpu,
    bindless: &mut Bindless,
    slot: &mut Option<(Image, TextureId)>,
    (name, atlas): (&str, &AtlasImage),
    levels: Vec<Vec<u8>>,
) -> Result<()> {
    gpu.wait_idle();
    let fits = slot.as_ref().is_some_and(|(image, _)| {
        (image.desc.width, image.desc.height, image.desc.mip_levels) == (atlas.width, atlas.height, levels.len() as u32)
    });
    if !fits {
        if let Some((image, texture)) = slot.take() {
            bindless.remove_texture(texture);
            gpu.destroy_image(image);
        }
        let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
        let desc = ImageDesc { mip_levels: levels.len() as u32, ..ImageDesc::new(atlas.width, atlas.height, vk::Format::R8_UNORM, usage) };
        let image = gpu.create_image(name, desc)?;
        let texture = bindless.add_texture(gpu, image.view);
        *slot = Some((image, texture));
    }
    let (image, _) = slot.as_ref().expect("made above");
    gpu.upload_mips(image, &levels)
}

/// An atlas and its mips down to one texel, each the average of the four
/// under it: distances averaged, so text drawn far smaller than its fields
/// fades rather than sparkles.
fn mips(atlas: &AtlasImage) -> Vec<Vec<u8>> {
    let mut levels = vec![atlas.pixels.clone()];
    let (mut width, mut height) = (atlas.width as usize, atlas.height as usize);
    while width > 1 || height > 1 {
        let (half_w, half_h) = ((width / 2).max(1), (height / 2).max(1));
        let above = levels.last().expect("the first level");
        let texel = |x: usize, y: usize| u32::from(above[y.min(height - 1) * width + x.min(width - 1)]);
        let level = (0..half_w * half_h)
            .map(|at| {
                let (x, y) = (at % half_w * 2, at / half_w * 2);
                ((texel(x, y) + texel(x + 1, y) + texel(x, y + 1) + texel(x + 1, y + 1) + 2) / 4) as u8
            })
            .collect();
        levels.push(level);
        (width, height) = (half_w, half_h);
    }
    levels
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

    /// Writes this frame's quads into `buffer` from its `first` quad on, and
    /// their shaded backgrounds' values into `value_buffer` from `first_value`
    /// on, and adds the pass that draws them onto `output`, returning how
    /// many quads and values it wrote. `sampled` are images drawn this frame that the UI shows, as
    /// the preview atlas. `backdrop` is the view blurred, with its size, and
    /// where the view is in the output, for quads that show it.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        fg: &mut FrameGraph<'_>,
        (pipeline, push): (vk::Pipeline, Push),
        (buffer, value_buffer): (&Buffer, &Buffer),
        (first, first_value): (usize, usize),
        output: (ImageId, vk::Extent2D),
        sampled: &[ImageId],
        backdrop: Option<(ImageId, [u32; 2], vk::Rect2D)>,
    ) -> (usize, usize) {
        // Every output's UI shares the buffers; what does not fit is left out.
        let room = (MAX_UI_QUADS as usize).saturating_sub(first);
        let games = self.game.quads.len().min(room);
        let windows = self.window.quads.len().min(room - games);
        if games + windows == 0 {
            return (0, 0);
        }
        let quad = size_of::<Quad>();
        buffer.write((first * quad) as u64, &self.game.quads[..games]);
        buffer.write(((first + games) * quad) as u64, &self.window.quads[..windows]);
        let value_room = (MAX_UI_VALUES as usize).saturating_sub(first_value);
        let game_values = self.game.values.len().min(value_room);
        let window_values = self.window.values.len().min(value_room - game_values);
        let value = size_of::<[f32; 4]>();
        value_buffer.write((first_value * value) as u64, &self.game.values[..game_values]);
        value_buffer.write(((first_value + game_values) * value) as u64, &self.window.values[..window_values]);
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
        // Each layer's atlas, its area's start in `list`, which the UI shader reads as its offset, its
        // first quad in `level`, and where its shaded backgrounds' values start in `max_draws`.
        let base = Push { size: [extent.width, extent.height], textures, ..push };
        let list = pack(self.game.origin[0], self.game.origin[1]);
        // Each layer's distance fields in `source`.
        let game = Push {
            texture: self.game.texture(),
            source: self.game.fields_texture(),
            list,
            level: first as u32,
            max_draws: first_value as u32,
            ..base
        };
        let window = Push {
            texture: self.window.texture(),
            source: self.window.fields_texture(),
            list: 0,
            level: (first + games) as u32,
            max_draws: (first_value + game_values) as u32,
            ..base
        };
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
        (games + windows, game_values + window_values)
    }

    /// The caller makes sure the GPU is idle.
    pub fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        self.game.destroy(gpu, bindless);
        self.window.destroy(gpu, bindless);
    }
}
