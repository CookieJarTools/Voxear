//! Pictures of what the renderer draws, read back to the CPU: an area of the
//! output, as the view is before the UI draws over it or as the window shows
//! it after. VoxDocs puts the Scene view into its pages this way, and a run
//! of them, one asked for each frame, into a GIF.
//!
//! A capture asked for is copied from the next frame that reaches its
//! moment, into a buffer the CPU reads once that frame's work is done, when
//! its frame slot comes round again: a frame or two later. The output is
//! sRGB, so the bytes are as people see them; a window's blue-first pixels
//! are put in red-first order, and alpha is opaque.

use std::collections::VecDeque;

use anyhow::Result;
use voxear_rhi::{Buffer, Gpu, ImageDesc, MemoryLocation, vk};

use crate::graph::{BufferAccess, FrameGraph, ImageAccess, ImageId};

/// What to capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capture {
    /// The rectangle of the output to keep, left, top, width and height in
    /// pixels, such as a viewport's; the whole output when `None`.
    pub area: Option<[u32; 4]>,
    /// The view as it is before the UI draws over it, rather than the
    /// window as it shows.
    pub before_ui: bool,
}

/// A captured picture: RGBA8, sRGB, the top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A copy on its way.
struct Read {
    slot: usize,
    buffer: Buffer,
    width: u32,
    height: u32,
    /// The output holds blue first.
    blue_first: bool,
}

#[derive(Default)]
pub(crate) struct Captures {
    asked: VecDeque<Capture>,
    in_flight: Vec<Read>,
    ready: VecDeque<CapturedImage>,
}

impl Captures {
    pub fn ask(&mut self, capture: Capture) {
        self.asked.push_back(capture);
    }

    pub fn take(&mut self) -> Option<CapturedImage> {
        self.ready.pop_front()
    }

    /// Copies the output for the capture asked for first, when it is taken
    /// at this moment: before the UI, or after it.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        gpu: &Gpu,
        (output, desc): (ImageId, ImageDesc),
        before_ui: bool,
        slot: usize,
    ) -> Result<()> {
        let Some(&capture) = self.asked.front().filter(|c| c.before_ui == before_ui) else { return Ok(()) };
        self.asked.pop_front();
        if !desc.usage.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
            tracing::warn!("this window's image cannot be copied on this system, so it cannot be captured");
            return Ok(());
        }
        let [x, y, width, height] = capture.area.unwrap_or([0, 0, desc.width, desc.height]);
        let (x, y) = (x.min(desc.width), y.min(desc.height));
        let (width, height) = (width.min(desc.width - x), height.min(desc.height - y));
        if width == 0 || height == 0 {
            return Ok(());
        }
        let bytes = u64::from(width) * u64::from(height) * 4;
        let buffer =
            gpu.create_buffer("capture", bytes, vk::BufferUsageFlags::TRANSFER_DST, MemoryLocation::GpuToCpu)?;
        let target = fg.import_buffer(&buffer);
        let handle = buffer.handle;
        fg.add_pass(
            "Capture",
            &[(output, ImageAccess::TransferSrc)],
            &[(target, BufferAccess::TransferWrite)],
            move |ctx| {
                let layers =
                    vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
                let region = vk::BufferImageCopy::default()
                    .image_subresource(layers)
                    .image_offset(vk::Offset3D { x: x as i32, y: y as i32, z: 0 })
                    .image_extent(vk::Extent3D { width, height, depth: 1 });
                let layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
                unsafe {
                    ctx.gpu.device.cmd_copy_image_to_buffer(ctx.cmd, ctx.image(output), layout, handle, &[region])
                };
            },
        );
        fg.sync_buffer(target, BufferAccess::HostRead);
        let blue_first = matches!(desc.format, vk::Format::B8G8R8A8_SRGB | vk::Format::B8G8R8A8_UNORM);
        self.in_flight.push(Read { slot, buffer, width, height, blue_first });
        Ok(())
    }

    /// Reads the copies made when this slot last ran, whose frame the GPU
    /// has finished by now.
    pub fn finish(&mut self, gpu: &Gpu, slot: usize) {
        let (done, waiting): (Vec<Read>, Vec<Read>) = self.in_flight.drain(..).partition(|read| read.slot == slot);
        self.in_flight = waiting;
        for read in done {
            let size = (read.width * read.height * 4) as usize;
            let rgba = read.buffer.read()[..size]
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&[a, b, c, _]| if read.blue_first { [c, b, a, 255] } else { [a, b, c, 255] })
                .collect();
            self.ready.push_back(CapturedImage { width: read.width, height: read.height, rgba });
            gpu.destroy_buffer(read.buffer);
        }
    }

    /// The buffers, to let go of with the renderer.
    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        self.in_flight.drain(..).map(|read| read.buffer).collect()
    }
}
