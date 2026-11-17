//! What a window shows while the renderer gets ready, before its first
//! frame: the engine's name, what it is doing, and how far it has got,
//! drawn on the CPU and copied into the window. Making the pipelines for the
//! first time on a GPU, or for a new version, takes a while, and a window
//! that shows nothing for that long looks broken.

use std::time::{Duration, Instant};

use anyhow::Result;
use voxear_rhi::{Buffer, Encoding, Frames, Gpu, MemoryLocation, Retired, Swapchain, vk};
use voxear_ui::{TextCoverage, TextRaster};

/// The default theme's ground, text and accent, and the bar's track.
const GROUND: [u8; 3] = [0x0c, 0x0e, 0x10];
const TEXT: [u8; 3] = [0xe8, 0xea, 0xee];
const MUTED: [u8; 3] = [0x8b, 0x91, 0x9a];
const TRACK: [u8; 3] = [0x1f, 0x22, 0x28];
const ACCENT: [u8; 3] = [0x79, 0x88, 0xff];
/// How often it is drawn again while nothing else changes but the count.
const EVERY: Duration = Duration::from_millis(100);
/// Paper white in HDR, in nits.
const PAPER_WHITE: f32 = 203.0;

pub(crate) struct Splash {
    text: Option<TextRaster>,
    title: Option<TextCoverage>,
    /// What it showed last, and when.
    shown: Option<(String, Option<(usize, usize)>, Instant)>,
    /// A buffer the picture goes up through, for each frame in flight.
    staging: Vec<Option<Buffer>>,
    /// The window went out of date, as when it was resized: the frame
    /// after the splash makes it again, and the splash shows no more.
    stopped: bool,
}

impl Splash {
    pub fn new() -> Self {
        Self { text: None, title: None, shown: None, staging: Vec::new(), stopped: false }
    }

    /// Shows `status` and, when given, `done` of `total` as a bar and as
    /// numbers; no more often than every [`EVERY`] while only the count
    /// changes.
    pub fn show(
        &mut self,
        (gpu, frames, swapchain): (&Gpu, &mut Frames, &Swapchain),
        status: &str,
        count: Option<(usize, usize)>,
    ) -> Result<()> {
        if self.stopped || !swapchain.is_renderable() {
            return Ok(());
        }
        let same_status = self.shown.as_ref().is_some_and(|(shown, ..)| shown == status);
        let recent = self.shown.as_ref().is_some_and(|(.., at)| at.elapsed() < EVERY);
        let finished = count.is_some_and(|(done, total)| done >= total);
        if same_status && recent && !finished {
            return Ok(());
        }
        self.shown = Some((status.to_string(), count, Instant::now()));
        let vk::Extent2D { width, height } = swapchain.extent;
        let picture = self.picture([width, height], status, count);
        let Some(bytes) = encode(&picture, swapchain.format.format, swapchain.encoding) else {
            self.stopped = true;
            return Ok(());
        };
        let frame = frames.begin(gpu)?;
        let Some(acquired) = frames.acquire(gpu, swapchain)? else {
            self.stopped = true;
            return Ok(());
        };
        if self.staging.len() <= frame.slot {
            self.staging.resize_with(frame.slot + 1, || None);
        }
        let slot = &mut self.staging[frame.slot];
        if slot.as_ref().is_some_and(|buffer| buffer.size < bytes.len() as u64) {
            frames.retire(Retired::Buffer(slot.take().expect("checked above")));
        }
        if slot.is_none() {
            let usage = vk::BufferUsageFlags::TRANSFER_SRC;
            *slot = Some(gpu.create_buffer("splash", bytes.len() as u64, usage, MemoryLocation::CpuToGpu)?);
        }
        let buffer = slot.as_ref().expect("made above");
        buffer.write(0, &bytes);
        let range =
            vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1);
        // After the wait for the image, which is at the colour output stage, and before the presenting
        // semaphore, which waits for that stage too.
        let to_copy = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(acquired.image)
            .subresource_range(range);
        let to_present = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .image(acquired.image)
            .subresource_range(range);
        let layers = vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
        let region = vk::BufferImageCopy::default().image_subresource(layers).image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
        unsafe {
            let device = &gpu.device;
            device.cmd_pipeline_barrier2(frame.cmd, &vk::DependencyInfo::default().image_memory_barriers(&[to_copy]));
            let layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
            device.cmd_copy_buffer_to_image(frame.cmd, buffer.handle, acquired.image, layout, &[region]);
            device
                .cmd_pipeline_barrier2(frame.cmd, &vk::DependencyInfo::default().image_memory_barriers(&[to_present]));
        }
        self.stopped |= frames.submit(gpu, frame, Some((swapchain, acquired)))?;
        Ok(())
    }

    /// Lets go of its buffers once no frame in flight uses them.
    pub fn retire(self, frames: &mut Frames) {
        for buffer in self.staging.into_iter().flatten() {
            frames.retire(Retired::Buffer(buffer));
        }
    }

    /// The picture, sRGB, row by row: the name above the middle, and below
    /// it the bar, what is being done at its left and the count at its
    /// right.
    fn picture(&mut self, [width, height]: [u32; 2], status: &str, count: Option<(usize, usize)>) -> Vec<[u8; 3]> {
        let mut picture = vec![GROUND; (width * height) as usize];
        // As large as on a 1080 pixel high screen, larger on taller ones.
        let scale = (height as f32 / 1080.0).max(1.0);
        let text = self.text.get_or_insert_with(TextRaster::new);
        let title = self.title.get_or_insert_with(|| text.line("Voxear", 30.0 * scale, 600.0)).clone();
        let status = text.line(status, 14.0 * scale, 400.0);
        let numbers = count.map(|(done, total)| text.line(&format!("{done} / {total}"), 14.0 * scale, 400.0));
        let middle = [width as i32 / 2, height as i32 / 2];
        let bar = [(320.0 * scale) as i32, (4.0 * scale).round().max(2.0) as i32];
        let left = middle[0] - bar[0] / 2;
        let title_at = [middle[0] - title.width as i32 / 2, middle[1] - title.height as i32 - (12.0 * scale) as i32];
        blend(&mut picture, width, &title, title_at, TEXT);
        let below = middle[1] + (10.0 * scale) as i32;
        let words = below + bar[1] + (10.0 * scale) as i32;
        match (count, numbers) {
            (Some((done, total)), Some(numbers)) => {
                fill(&mut picture, width, [left, below, bar[0], bar[1]], TRACK);
                let filled = (bar[0] as f32 * (done as f32 / total.max(1) as f32).min(1.0)) as i32;
                fill(&mut picture, width, [left, below, filled, bar[1]], ACCENT);
                blend(&mut picture, width, &status, [left, words], MUTED);
                blend(&mut picture, width, &numbers, [left + bar[0] - numbers.width as i32, words], MUTED);
            }
            _ => blend(&mut picture, width, &status, [middle[0] - status.width as i32 / 2, below], MUTED),
        }
        picture
    }
}

/// Fills `[x, y, width, height]` with `color`, as far as it is in the picture.
fn fill(picture: &mut [[u8; 3]], width: u32, [x, y, w, h]: [i32; 4], color: [u8; 3]) {
    let height = picture.len() as i32 / width.max(1) as i32;
    for row in y.max(0)..(y + h).min(height) {
        for column in x.max(0)..(x + w).min(width as i32) {
            picture[(row * width as i32 + column) as usize] = color;
        }
    }
}

/// Lays `text` over the picture in `color`, its top left at `at`.
fn blend(picture: &mut [[u8; 3]], width: u32, text: &TextCoverage, at: [i32; 2], color: [u8; 3]) {
    let height = picture.len() as i32 / width.max(1) as i32;
    for row in 0..text.height as i32 {
        for column in 0..text.width as i32 {
            let (x, y) = (at[0] + column, at[1] + row);
            if x < 0 || y < 0 || x >= width as i32 || y >= height {
                continue;
            }
            let cover = u16::from(text.pixels[(row * text.width as i32 + column) as usize]);
            let under = &mut picture[(y * width as i32 + x) as usize];
            for channel in 0..3 {
                let mixed = u16::from(under[channel]) * (255 - cover) + u16::from(color[channel]) * cover;
                under[channel] = (mixed / 255) as u8;
            }
        }
    }
}

/// The picture as the window's format holds it; `None` for a format the
/// splash does not write.
fn encode(picture: &[[u8; 3]], format: vk::Format, encoding: Encoding) -> Option<Vec<u8>> {
    use vk::Format as F;
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    Some(match format {
        F::B8G8R8A8_SRGB | F::B8G8R8A8_UNORM => picture.iter().flat_map(|&[r, g, b]| [b, g, r, 255]).collect(),
        F::R8G8B8A8_SRGB | F::R8G8B8A8_UNORM => picture.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        F::A2B10G10R10_UNORM_PACK32 => {
            let pack = |[r, g, b]: [f32; 3]| {
                let ten = |v: f32| (v.clamp(0.0, 1.0) * 1023.0).round() as u32;
                (ten(r) | ten(g) << 10 | ten(b) << 20 | 3 << 30).to_le_bytes()
            };
            match encoding {
                Encoding::Hdr10 => picture
                    .iter()
                    .flat_map(|&pixel| {
                        let [r, g, b] = pixel.map(linear);
                        // BT.709 to BT.2020, then through the PQ curve.
                        let wide = [
                            0.6274 * r + 0.3293 * g + 0.0433 * b,
                            0.0691 * r + 0.9195 * g + 0.0114 * b,
                            0.0164 * r + 0.0880 * g + 0.8956 * b,
                        ];
                        pack(wide.map(|v| pq(v * PAPER_WHITE)))
                    })
                    .collect(),
                _ => picture.iter().flat_map(|pixel| pack(pixel.map(|c| f32::from(c) / 255.0))).collect(),
            }
        }
        F::R16G16B16A16_SFLOAT => {
            // scRGB: linear, 1 being 80 nits.
            let half = |v: f32| half_float(v).to_le_bytes();
            picture
                .iter()
                .flat_map(|&pixel| {
                    let [r, g, b] = pixel.map(|c| linear(c) * PAPER_WHITE / 80.0);
                    [half(r), half(g), half(b), half(1.0)].concat()
                })
                .collect()
        }
        _ => return None,
    })
}

/// Nits through the PQ curve (ST 2084), 1 being 10,000 nits.
fn pq(nits: f32) -> f32 {
    let (m1, m2, c1, c2, c3) = (0.159_301_76, 78.843_75, 0.835_937_5, 18.851_563, 18.687_5);
    let power = (nits / 10_000.0).max(0.0).powf(m1);
    ((c1 + c2 * power) / (1.0 + c3 * power)).powf(m2)
}

/// A float as a half float's bits, for the small positive values the
/// splash writes; past a half float's range, infinity.
fn half_float(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    match exponent {
        _ if value == 0.0 => sign,
        ..=0 => sign,
        31.. => sign | 0x7c00,
        _ => sign | (exponent as u16) << 10 | ((bits >> 13) & 0x3ff) as u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_and_pq_land_where_they_should() {
        assert_eq!(half_float(1.0), 0x3c00);
        assert_eq!(half_float(2.0), 0x4000);
        assert_eq!(half_float(0.5), 0x3800);
        assert_eq!(half_float(0.0), 0);
        assert!((pq(10_000.0) - 1.0).abs() < 1e-4, "10,000 nits is the top");
        assert!((pq(100.0) - 0.508).abs() < 0.002, "100 nits is about half way: {}", pq(100.0));
    }

    #[test]
    fn eight_bit_windows_get_the_picture_in_their_order() {
        let picture = [[1, 2, 3]];
        let bgra = encode(&picture, vk::Format::B8G8R8A8_SRGB, Encoding::Sdr).unwrap();
        let rgba = encode(&picture, vk::Format::R8G8B8A8_UNORM, Encoding::Sdr).unwrap();
        assert_eq!((bgra, rgba), (vec![3, 2, 1, 255], vec![1, 2, 3, 255]));
        assert!(encode(&picture, vk::Format::R32_SFLOAT, Encoding::Sdr).is_none());
    }
}
