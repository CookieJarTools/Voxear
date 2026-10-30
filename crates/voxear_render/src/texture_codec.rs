//! Texture compression for the content build: mip chains built on the CPU
//! (compressed images cannot have mips blitted on the GPU) and encoded as
//! BC7 for colour and BC5 for normal maps, a quarter of RGBA8's memory.
//!
//! BC7 uses mode 6 only: one colour line per 4×4 block with RGBA endpoints
//! and 16 steps. It is not the sharpest BC7 encoder, but it is simple,
//! fast, and its quality is good for most textures; the endpoints are
//! fitted along each block's principal axis, then refined by least squares
//! once. BC5 stores two channels (a normal map's X and Y) as two BC4
//! blocks; shaders rebuild Z.

/// BC7's 4-bit interpolation weights, out of 64.
const BC7_WEIGHTS: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// A mip chain: level 0 is the image itself, and each level halves, down to
/// 1×1. Colour (`srgb`) is averaged in linear space, so mips do not darken.
pub(crate) fn mip_chain(rgba: &[u8], width: u32, height: u32, srgb: bool) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(width, height, rgba.to_vec())];
    let to_linear: Vec<f32> =
        (0..256).map(|v| if srgb { srgb_to_linear(v as f32 / 255.0) } else { v as f32 / 255.0 }).collect();
    while let Some((w, h, pixels)) = levels.last().filter(|(w, h, _)| *w > 1 || *h > 1) {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                        let value = pixels[((sy * w + sx) * 4 + c) as usize];
                        sum += if c == 3 { value as f32 / 255.0 } else { to_linear[value as usize] };
                    }
                    let average = sum / 4.0;
                    let encoded = if c == 3 || !srgb { average } else { linear_to_srgb(average) };
                    next[((y * nw + x) * 4 + c) as usize] = (encoded * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        levels.push((nw, nh, next));
    }
    levels
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// The 16 pixels of the block at (bx, by), repeating edge pixels for blocks
/// that run past the image.
fn block(rgba: &[u8], width: u32, height: u32, bx: u32, by: u32) -> [[u8; 4]; 16] {
    let mut pixels = [[0u8; 4]; 16];
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let x = (bx * 4 + i as u32 % 4).min(width - 1);
        let y = (by * 4 + i as u32 / 4).min(height - 1);
        let at = ((y * width + x) * 4) as usize;
        pixel.copy_from_slice(&rgba[at..at + 4]);
    }
    pixels
}

fn for_each_block(width: u32, height: u32, mut f: impl FnMut(u32, u32)) {
    for by in 0..height.div_ceil(4) {
        for bx in 0..width.div_ceil(4) {
            f(bx, by);
        }
    }
}

pub(crate) fn encode_bc7(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width.div_ceil(4) * height.div_ceil(4) * 16) as usize);
    for_each_block(width, height, |bx, by| out.extend_from_slice(&bc7_block(&block(rgba, width, height, bx, by))));
    out
}

pub(crate) fn encode_bc5(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width.div_ceil(4) * height.div_ceil(4) * 16) as usize);
    for_each_block(width, height, |bx, by| {
        let pixels = block(rgba, width, height, bx, by);
        out.extend_from_slice(&bc4_block(pixels.map(|p| p[0])));
        out.extend_from_slice(&bc4_block(pixels.map(|p| p[1])));
    });
    out
}

/// A colour line through the block: its mean and main direction.
fn principal_axis(pixels: &[[u8; 4]; 16]) -> ([f32; 4], [f32; 4]) {
    let mut mean = [0.0f32; 4];
    for p in pixels {
        for c in 0..4 {
            mean[c] += p[c] as f32 / 16.0;
        }
    }
    let mut covariance = [[0.0f32; 4]; 4];
    for p in pixels {
        let d: [f32; 4] = std::array::from_fn(|c| p[c] as f32 - mean[c]);
        for i in 0..4 {
            for j in 0..4 {
                covariance[i][j] += d[i] * d[j];
            }
        }
    }
    // Power iteration converges on the largest eigenvector in a few steps.
    let mut axis = [1.0f32, 1.0, 1.0, 1.0];
    for _ in 0..8 {
        let next: [f32; 4] = std::array::from_fn(|i| (0..4).map(|j| covariance[i][j] * axis[j]).sum());
        let length = next.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 1e-6 {
            return (mean, [0.0; 4]);
        }
        axis = next.map(|v| v / length);
    }
    (mean, axis)
}

/// One 4-bit index per pixel for the given endpoints, and the squared error.
fn bc7_indices(pixels: &[[u8; 4]; 16], e0: [u32; 4], e1: [u32; 4]) -> ([u8; 16], u32) {
    let palette: [[u32; 4]; 16] = std::array::from_fn(|i| {
        std::array::from_fn(|c| ((64 - BC7_WEIGHTS[i]) * e0[c] + BC7_WEIGHTS[i] * e1[c] + 32) >> 6)
    });
    let mut indices = [0u8; 16];
    let mut total = 0;
    for (index, pixel) in indices.iter_mut().zip(pixels) {
        let error = |colour: &[u32; 4]| (0..4).map(|c| (colour[c] as i32 - pixel[c] as i32).pow(2) as u32).sum::<u32>();
        let (best, best_error) =
            palette.iter().enumerate().map(|(i, p)| (i, error(p))).min_by_key(|(_, e)| *e).unwrap();
        *index = best as u8;
        total += best_error;
    }
    (indices, total)
}

/// Mode 6 stores each endpoint as 7 bits per channel plus a shared low bit.
fn quantize(endpoint: [f32; 4]) -> ([u32; 4], u32) {
    let candidates = [0u32, 1].map(|p| {
        let q: [u32; 4] = endpoint.map(|v| (((v - p as f32) / 2.0).round().clamp(0.0, 127.0)) as u32);
        let error: f32 = (0..4).map(|c| (((q[c] << 1) | p) as f32 - endpoint[c]).powi(2)).sum();
        (q, p, error)
    });
    let (q, p, _) = if candidates[0].2 <= candidates[1].2 { candidates[0] } else { candidates[1] };
    (q, p)
}

fn expand(q: [u32; 4], p: u32) -> [u32; 4] {
    q.map(|v| (v << 1) | p)
}

/// The least-squares endpoints for fixed indices, which usually beat the
/// axis fit once the indices are known.
fn refit(pixels: &[[u8; 4]; 16], indices: &[u8; 16]) -> Option<([f32; 4], [f32; 4])> {
    let (mut aa, mut ab, mut bb) = (0.0f32, 0.0f32, 0.0f32);
    let (mut ax, mut bx) = ([0.0f32; 4], [0.0f32; 4]);
    for (pixel, &index) in pixels.iter().zip(indices) {
        let b = BC7_WEIGHTS[index as usize] as f32 / 64.0;
        let a = 1.0 - b;
        aa += a * a;
        ab += a * b;
        bb += b * b;
        for c in 0..4 {
            ax[c] += a * pixel[c] as f32;
            bx[c] += b * pixel[c] as f32;
        }
    }
    let determinant = aa * bb - ab * ab;
    if determinant.abs() < 1e-6 {
        return None;
    }
    let e0 = std::array::from_fn(|c| ((bb * ax[c] - ab * bx[c]) / determinant).clamp(0.0, 255.0));
    let e1 = std::array::from_fn(|c| ((aa * bx[c] - ab * ax[c]) / determinant).clamp(0.0, 255.0));
    Some((e0, e1))
}

fn bc7_block(pixels: &[[u8; 4]; 16]) -> [u8; 16] {
    let (mean, axis) = principal_axis(pixels);
    let projections = pixels.map(|p| (0..4).map(|c| (p[c] as f32 - mean[c]) * axis[c]).sum::<f32>());
    let low = projections.iter().copied().fold(f32::MAX, f32::min);
    let high = projections.iter().copied().fold(f32::MIN, f32::max);
    let along = |t: f32| std::array::from_fn(|c| (mean[c] + axis[c] * t).clamp(0.0, 255.0));

    let mut best = encode_mode6(pixels, along(low), along(high));
    if let Some((e0, e1)) = refit(pixels, &best.2) {
        let refined = encode_mode6(pixels, e0, e1);
        if refined.3 < best.3 {
            best = refined;
        }
    }
    pack_mode6(best)
}

type Mode6 = (([u32; 4], u32), ([u32; 4], u32), [u8; 16], u32);

fn encode_mode6(pixels: &[[u8; 4]; 16], e0: [f32; 4], e1: [f32; 4]) -> Mode6 {
    let (q0, q1) = (quantize(e0), quantize(e1));
    let (indices, error) = bc7_indices(pixels, expand(q0.0, q0.1), expand(q1.0, q1.1));
    (q0, q1, indices, error)
}

/// Writes mode 6's bits: the mode, the endpoints channel by channel, the two
/// low bits, then the indices. Pixel 0's index loses its top bit, so the
/// endpoints swap when it would need it.
fn pack_mode6((mut q0, mut q1, mut indices, _): Mode6) -> [u8; 16] {
    if indices[0] >= 8 {
        std::mem::swap(&mut q0, &mut q1);
        indices = indices.map(|i| 15 - i);
    }
    let mut bits = BitWriter::default();
    bits.put(1 << 6, 7);
    for c in 0..4 {
        bits.put(q0.0[c], 7);
        bits.put(q1.0[c], 7);
    }
    bits.put(q0.1, 1);
    bits.put(q1.1, 1);
    for (i, &index) in indices.iter().enumerate() {
        bits.put(u32::from(index), if i == 0 { 3 } else { 4 });
    }
    bits.0.to_le_bytes()
}

/// One BC4 block: two endpoints and eight steps between them.
fn bc4_block(values: [u8; 16]) -> [u8; 8] {
    let (high, low) = (*values.iter().max().unwrap(), *values.iter().min().unwrap());
    let mut out = [0u8; 8];
    out[0] = high;
    out[1] = low;
    if high == low {
        return out;
    }
    let palette: [u32; 8] = std::array::from_fn(|i| match i {
        0 => u32::from(high),
        1 => u32::from(low),
        _ => ((8 - i as u32) * u32::from(high) + (i as u32 - 1) * u32::from(low)) / 7,
    });
    let mut bits = 0u64;
    for (i, &value) in values.iter().enumerate() {
        let index = (0..8).min_by_key(|&k| (palette[k] as i32 - value as i32).unsigned_abs()).unwrap();
        bits |= (index as u64) << (3 * i);
    }
    out[2..].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

#[derive(Default)]
struct BitWriter(u128, u32);

impl BitWriter {
    fn put(&mut self, value: u32, count: u32) {
        self.0 |= u128::from(value) << self.1;
        self.1 += count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes one mode 6 block, the inverse of `pack_mode6`.
    fn decode_bc7(block: &[u8]) -> [[u8; 4]; 16] {
        let bits = u128::from_le_bytes(block.try_into().unwrap());
        assert_eq!(bits & 0x7f, 1 << 6, "mode 6");
        let mut at = 7;
        let mut take = |count: u32| {
            let value = ((bits >> at) & ((1u128 << count) - 1)) as u32;
            at += count;
            value
        };
        let mut q = [[0u32; 4]; 2];
        let [first, second] = &mut q;
        for (a, b) in first.iter_mut().zip(second.iter_mut()) {
            *a = take(7);
            *b = take(7);
        }
        let (p0, p1) = (take(1), take(1));
        let (e0, e1) = (expand(q[0], p0), expand(q[1], p1));
        std::array::from_fn(|i| {
            let index = take(if i == 0 { 3 } else { 4 }) as usize;
            std::array::from_fn(|c| (((64 - BC7_WEIGHTS[index]) * e0[c] + BC7_WEIGHTS[index] * e1[c] + 32) >> 6) as u8)
        })
    }

    fn decode_bc4(block: &[u8]) -> [u8; 16] {
        let (high, low) = (u32::from(block[0]), u32::from(block[1]));
        let mut bytes = [0u8; 8];
        bytes[..6].copy_from_slice(&block[2..8]);
        let bits = u64::from_le_bytes(bytes);
        std::array::from_fn(|i| {
            let index = ((bits >> (3 * i)) & 7) as u32;
            (match index {
                0 => high,
                1 => low,
                k if high > low => ((8 - k) * high + (k - 1) * low) / 7,
                _ => low,
            }) as u8
        })
    }

    /// Peak signal-to-noise ratio in decibels; above 40 is hard to tell apart.
    fn psnr(a: &[[u8; 4]; 16], b: &[[u8; 4]; 16]) -> f64 {
        let error: f64 =
            a.iter().zip(b).flat_map(|(x, y)| (0..4).map(move |c| (x[c] as f64 - y[c] as f64).powi(2))).sum();
        let mse = error / 64.0;
        if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
    }

    #[test]
    fn bc7_blocks_decode_close_to_their_source() {
        let flat = [[200u8, 100, 50, 255]; 16];
        let decoded = decode_bc7(&bc7_block(&flat));
        assert!(decoded.iter().all(|p| p.iter().zip(&flat[0]).all(|(a, b)| a.abs_diff(*b) <= 1)), "{decoded:?}");

        let gradient: [[u8; 4]; 16] = std::array::from_fn(|i| {
            let t = (i * 17) as u8;
            [t, 255 - t, t / 2, 255]
        });
        assert!(psnr(&gradient, &decode_bc7(&bc7_block(&gradient))) > 40.0);

        // On noise one colour line cannot do well, but it must never do worse than filling the block with
        // its average colour; this also checks that pixel 0 never needs the index bit mode 6 drops.
        let mut seed = 7u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as u8
        };
        for _ in 0..50 {
            let pixels: [[u8; 4]; 16] = std::array::from_fn(|_| [noise(), noise(), noise(), 255]);
            let mean: [u8; 4] = std::array::from_fn(|c| (pixels.iter().map(|p| p[c] as u32).sum::<u32>() / 16) as u8);
            assert!(psnr(&pixels, &decode_bc7(&bc7_block(&pixels))) >= psnr(&pixels, &[mean; 16]) - 0.1);
        }
    }

    #[test]
    fn bc5_keeps_two_channels() {
        let values: [u8; 16] = std::array::from_fn(|i| (i * 16) as u8);
        let decoded = decode_bc4(&bc4_block(values));
        // Eight steps across 0 to 240 are 34 apart, so no value is off by more than half that.
        assert!(values.iter().zip(&decoded).all(|(a, b)| a.abs_diff(*b) <= 17), "{decoded:?}");
        assert_eq!(decode_bc4(&bc4_block([77; 16])), [77; 16]);
        assert_eq!(encode_bc5(&[0; 4 * 4 * 4], 4, 4).len(), 16, "two BC4 blocks per 4x4");
    }

    #[test]
    fn mips_halve_down_to_one_pixel_and_keep_brightness() {
        let checker: Vec<u8> =
            (0..16).flat_map(|i| if (i + i / 4) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
        let mips = mip_chain(&checker, 4, 4, true);
        let sizes: Vec<(u32, u32)> = mips.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(sizes, [(4, 4), (2, 2), (1, 1)]);
        // Half white, half black averages to linear 0.5, which is sRGB 188, not 128.
        assert_eq!(mips[2].2[0], 188);
        assert_eq!(mip_chain(&checker, 4, 4, false)[2].2[0], 128);
    }
}
