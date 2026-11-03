//! The frame debugger: the passes of the frame just drawn, in order, each
//! with the images it used and its GPU time, and one of those images shown
//! as it stood right after its pass. Unity's Frame Debugger steps through
//! draws; this steps through the render graph's passes, which is how a
//! Voxear frame is described.
//!
//! Looking at an image costs one copy and one small pass a frame: the image
//! is copied after its pass, one mip of it, and drawn into a view image with
//! the channels and exposure asked for, which the UI shows as an image. The
//! view is a frame behind when the pass comes after the UI. With nothing
//! picked, it only notes the passes, and switched off it costs nothing.

use anyhow::Result;
use voxear_rhi::{Bindless, Gpu, ImageDesc, TextureId, vk};

use crate::{
    gpu::Push,
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, fullscreen},
    pipelines::HDR_FORMAT,
};

/// The view is at most this many pixels across, however large the image.
const MAX_VIEW: u32 = 2048;

/// A pass of the frame, as the frame debugger lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePass {
    pub name: String,
    /// Its GPU time in the last frame timed, in milliseconds.
    pub ms: Option<f32>,
    pub images: Vec<FrameImage>,
}

/// An image a pass used, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameImage {
    pub name: String,
    /// How the pass used it, in words: "Drawn into", "Sampled" and so on.
    pub used: &'static str,
    /// Whether the pass changed it.
    pub writes: bool,
    pub size: [u32; 2],
    pub mips: u32,
    /// Its format, in words, such as "RGBA16 float".
    pub format: String,
    /// How many channels it has, so a one-channel image, such as depth,
    /// shows as grey.
    pub channels: u32,
    pub depth: bool,
    /// Whether it can be shown: everything the graph owns can, and the
    /// window's image where the system allows copying it.
    pub viewable: bool,
}

/// Which channels a view shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channels {
    #[default]
    Rgb,
    Red,
    Green,
    Blue,
    Alpha,
}

/// What the frame debugger looks at: an image of a pass, by their places in
/// [`Renderer::frame_passes`](crate::Renderer::frame_passes), one of its
/// mips, and how to show it. `exposure` is in stops, so 1 is twice as bright.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameLook {
    pub pass: usize,
    pub image: usize,
    pub mip: u32,
    pub channels: Channels,
    pub exposure: f32,
}

/// The frame debugger's state in the renderer.
#[derive(Default)]
pub(crate) struct FrameDebugger {
    pub on: bool,
    pub look: Option<FrameLook>,
    pub passes: Vec<FramePass>,
    /// The view image's texture and size, once one was drawn.
    pub view: Option<(TextureId, [u32; 2])>,
}

impl FrameDebugger {
    /// The view image for this frame, made before the UI is recorded so the
    /// UI can show it, sized from what was looked at last frame.
    pub fn view_image(
        &mut self,
        fg: &mut FrameGraph<'_>,
        gpu: &Gpu,
        bindless: &mut Bindless,
    ) -> Result<Option<ImageId>> {
        let Some(look) = self.look.filter(|_| self.on) else {
            self.view = None;
            return Ok(None);
        };
        let Some(image) = self.passes.get(look.pass).and_then(|p| p.images.get(look.image)) else { return Ok(None) };
        let [width, height] = view_size(image.size, look.mip);
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let view =
            fg.history_image(gpu, bindless, "frame debugger view", ImageDesc::new(width, height, HDR_FORMAT, usage))?;
        self.view = Some((fg.texture(view), [width, height]));
        Ok(Some(view))
    }

    /// Notes this frame's passes, once all are recorded, and adds the copy
    /// and the view after the pass looked at.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        (pipeline, push): (vk::Pipeline, Push),
        view: Option<ImageId>,
        timings: &[(String, f32)],
    ) -> Result<()> {
        if !self.on {
            self.passes.clear();
            return Ok(());
        }
        let recorded = fg.recorded();
        // What was looked at last frame, found again by the pass's name and
        // how many before it had that name, and the image's name.
        let wanted = self.look.and_then(|look| {
            let pass = self.passes.get(look.pass)?;
            let nth = self.passes[..look.pass].iter().filter(|p| p.name == pass.name).count();
            Some((look, pass.name.clone(), nth, pass.images.get(look.image)?.name.clone()))
        });
        self.passes = recorded
            .iter()
            .enumerate()
            .map(|(at, pass)| {
                // Passes can share a name, as the bloom's do, and so can their times.
                let nth = recorded[..at].iter().filter(|p| p.name == pass.name).count();
                let ms = timings.iter().filter(|(name, _)| *name == pass.name).nth(nth).map(|&(_, ms)| ms);
                describe(fg, pass, ms)
            })
            .collect();
        let (Some(view), Some((look, name, nth, image))) = (view, wanted) else { return Ok(()) };
        let Some(pass) = recorded.iter().filter(|p| p.name == name).nth(nth) else { return Ok(()) };
        let Some(&(source, _)) = pass.images.iter().find(|&&(id, _)| fg.image_name(id) == image) else { return Ok(()) };
        let desc = fg.image_desc(source);
        if !desc.usage.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
            return Ok(());
        }
        let mip = look.mip.min(desc.mip_levels - 1);
        let size = [(desc.width >> mip).max(1), (desc.height >> mip).max(1)];
        let copy_desc = ImageDesc::new(
            size[0],
            size[1],
            desc.format,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        );
        let copy = fg.create_image(gpu, bindless, "frame debugger copy", copy_desc)?;
        let aspect = desc.aspect();
        let images = [(source, ImageAccess::TransferSrc), (copy, ImageAccess::TransferDst)];
        fg.insert_pass(pass.at + 1, "Frame debugger copy", &images, move |ctx| {
            let layers = |mip_level| {
                vk::ImageSubresourceLayers::default().aspect_mask(aspect).mip_level(mip_level).layer_count(1)
            };
            let region = vk::ImageCopy::default()
                .src_subresource(layers(mip))
                .dst_subresource(layers(0))
                .extent(vk::Extent3D { width: size[0], height: size[1], depth: 1 });
            unsafe {
                ctx.gpu.device.cmd_copy_image(
                    ctx.cmd,
                    ctx.image(source),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    ctx.image(copy),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                )
            };
        });
        let channels = match look.channels {
            Channels::Rgb => 0,
            Channels::Red => 1,
            Channels::Green => 2,
            Channels::Blue => 3,
            Channels::Alpha => 4,
        };
        let push = Push {
            texture: fg.texture(copy).0,
            size: view_size([desc.width, desc.height], mip),
            textures: [channels, look.exposure.exp2().to_bits(), 0, 0],
            ..push
        };
        let images = [(copy, ImageAccess::Sampled), (view, ImageAccess::ColorAttachment)];
        fg.insert_pass(pass.at + 2, "Frame debugger view", &images, move |ctx| {
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(ctx.view(view))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)];
            begin_rendering(ctx, ctx.extent(view), &color, None);
            fullscreen(ctx, pipeline, &push);
        });
        Ok(())
    }
}

/// The view's size for a mip of an image this size, kept under [`MAX_VIEW`].
fn view_size([width, height]: [u32; 2], mip: u32) -> [u32; 2] {
    let [width, height] = [(width >> mip).max(1), (height >> mip).max(1)];
    let scale = (MAX_VIEW as f32 / width.max(height) as f32).min(1.0);
    [((width as f32 * scale) as u32).max(1), ((height as f32 * scale) as u32).max(1)]
}

fn describe(fg: &FrameGraph<'_>, pass: &crate::graph::RecordedPass, ms: Option<f32>) -> FramePass {
    let images = pass
        .images
        .iter()
        .map(|&(id, access)| {
            let desc = fg.image_desc(id);
            let (format, channels) = format_name(desc.format);
            FrameImage {
                name: fg.image_name(id).to_string(),
                used: used(access),
                writes: matches!(
                    access,
                    ImageAccess::ColorAttachment
                        | ImageAccess::DepthAttachment
                        | ImageAccess::StorageWrite
                        | ImageAccess::TransferDst
                ),
                size: [desc.width, desc.height],
                mips: desc.mip_levels,
                format,
                channels,
                depth: desc.aspect().contains(vk::ImageAspectFlags::DEPTH),
                viewable: desc.usage.contains(vk::ImageUsageFlags::TRANSFER_SRC),
            }
        })
        .collect();
    FramePass { name: pass.name.clone(), ms, images }
}

fn used(access: ImageAccess) -> &'static str {
    match access {
        ImageAccess::Undefined => "Discarded",
        ImageAccess::ColorAttachment => "Drawn into",
        ImageAccess::DepthAttachment => "Depth drawn into",
        ImageAccess::DepthRead => "Depth tested",
        ImageAccess::Sampled | ImageAccess::SampledWithTasks => "Sampled",
        ImageAccess::StorageWrite | ImageAccess::FragmentStorageWrite => "Written",
        ImageAccess::TransferSrc => "Copied from",
        ImageAccess::TransferDst => "Copied into",
        ImageAccess::Present => "Presented",
    }
}

/// A format in words, and how many channels it has.
fn format_name(format: vk::Format) -> (String, u32) {
    use vk::Format as F;
    let (name, channels) = match format {
        F::R16G16B16A16_SFLOAT => ("RGBA16 float", 4),
        F::R16G16_SFLOAT => ("RG16 float", 2),
        F::R32_SFLOAT => ("R32 float", 1),
        F::D32_SFLOAT => ("Depth 32 float", 1),
        F::B10G11R11_UFLOAT_PACK32 => ("RGB 11-11-10 float", 3),
        F::R8G8B8A8_SRGB => ("RGBA8 sRGB", 4),
        F::B8G8R8A8_SRGB => ("BGRA8 sRGB", 4),
        F::R8G8B8A8_UNORM => ("RGBA8", 4),
        F::B8G8R8A8_UNORM => ("BGRA8", 4),
        F::R8_UNORM => ("R8", 1),
        other => return (format!("{other:?}"), 4),
    };
    (name.to_string(), channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_keep_their_shape_under_the_largest_size() {
        assert_eq!(view_size([1600, 900], 0), [1600, 900]);
        assert_eq!(view_size([1600, 900], 1), [800, 450]);
        assert_eq!(view_size([4096, 4096], 0), [2048, 2048]);
        assert_eq!(view_size([1, 1], 3), [1, 1], "a mip never shrinks to nothing");
    }
}
