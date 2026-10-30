//! A cooked model's GPU data, as it ships: vertices, indices and texture
//! mips in one block, compressed with GDeflate in 64 KiB tiles. Each tile
//! decompresses on its own, so the GPU unpacks them all at once
//! (`VK_NV_memory_decompression`); without that, the CPU unpacks them on the
//! loading thread instead. Data that does not compress ships as it is.

use std::borrow::Cow;

use anyhow::{Result, ensure};
use gdeflate::sys;

/// Bytes per tile, the most one GPU decompression region may produce.
pub const TILE: usize = 64 * 1024;
/// Compressing has to save at least this share to be worth a decompression.
const WORTHWHILE: f64 = 0.97;

#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// The bytes as the GPU uses them.
    Raw(Vec<u8>),
    /// GDeflate tiles. Tile `i` unpacks to bytes `i * TILE` onwards, up to
    /// `size`; `tiles` holds where each starts in `data` and its length.
    /// Every tile starts 4-byte aligned, as the GPU requires.
    Tiled { size: u64, tiles: Vec<(u32, u32)>, data: Vec<u8> },
}

impl Payload {
    /// Compresses `raw` into tiles when that makes it smaller, at the
    /// strongest level: cooking happens once, and decompression costs the
    /// same at any level.
    pub fn compress(raw: Vec<u8>) -> Result<Self> {
        if raw.is_empty() {
            return Ok(Self::Raw(raw));
        }
        let mut compressor = gdeflate::Compressor::new(gdeflate::CompressionLevel::Level12)?;
        let compressed = compressor.compress(&raw, TILE)?;
        let mut data = Vec::with_capacity(compressed.bytes.len() + compressed.tiles.len() * 3);
        let mut tiles = Vec::with_capacity(compressed.tiles.len());
        let mut at = 0;
        for tile in &compressed.tiles {
            let len = tile.compressed_size as usize;
            data.resize(data.len().next_multiple_of(4), 0);
            tiles.push((data.len() as u32, len as u32));
            data.extend_from_slice(&compressed.bytes[at..at + len]);
            at += len;
        }
        if data.len() as f64 > raw.len() as f64 * WORTHWHILE {
            return Ok(Self::Raw(raw));
        }
        Ok(Self::Tiled { size: raw.len() as u64, tiles, data })
    }

    /// Bytes once decompressed.
    pub fn size(&self) -> u64 {
        match self {
            Self::Raw(bytes) => bytes.len() as u64,
            Self::Tiled { size, .. } => *size,
        }
    }

    /// The bytes as the GPU uses them, decompressed on this thread if need be.
    pub fn unpack(&self) -> Result<Cow<'_, [u8]>> {
        let Self::Tiled { size, tiles, data } = self else {
            let Self::Raw(bytes) = self else { unreachable!() };
            return Ok(Cow::Borrowed(bytes));
        };
        let mut out = vec![0u8; *size as usize];
        let decompressor = unsafe { sys::libdeflate_alloc_gdeflate_decompressor() };
        ensure!(!decompressor.is_null(), "could not create a GDeflate decompressor");
        let result = unpack_tiles(decompressor, tiles, data, &mut out);
        unsafe { sys::libdeflate_free_gdeflate_decompressor(decompressor) };
        result?;
        Ok(Cow::Owned(out))
    }

    /// Decompresses in place, for GPUs that cannot.
    pub fn make_raw(&mut self) -> Result<()> {
        if let Cow::Owned(bytes) = self.unpack()? {
            *self = Self::Raw(bytes);
        }
        Ok(())
    }
}

/// What tile `index` of a payload of `size` bytes unpacks to.
pub fn tile_len(size: u64, index: usize) -> u64 {
    (size - (index * TILE) as u64).min(TILE as u64)
}

fn unpack_tiles(
    decompressor: *mut sys::libdeflate_gdeflate_decompressor,
    tiles: &[(u32, u32)],
    data: &[u8],
    out: &mut [u8],
) -> Result<()> {
    let size = out.len() as u64;
    ensure!(tiles.len() == (out.len()).div_ceil(TILE), "{} tiles cannot make {size} bytes", tiles.len());
    for (index, (chunk, &(offset, len))) in out.chunks_mut(TILE).zip(tiles).enumerate() {
        let source = data.get(offset as usize..(offset + len) as usize);
        let Some(source) = source else { anyhow::bail!("tile {index} lies outside the payload") };
        let mut page = sys::libdeflate_gdeflate_in_page { data: source.as_ptr().cast(), nbytes: source.len() };
        let mut written = 0;
        // The library's own wrapper ignores this result, so damaged data would
        // pass unnoticed through it.
        let result = unsafe {
            sys::libdeflate_gdeflate_decompress(
                decompressor,
                &mut page,
                1,
                chunk.as_mut_ptr().cast(),
                chunk.len(),
                &mut written,
            )
        };
        ensure!(result == sys::libdeflate_result_LIBDEFLATE_SUCCESS, "tile {index} is damaged ({result})");
        ensure!(written as u64 == tile_len(size, index), "tile {index} unpacked to {written} bytes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Something like vertex data: smooth, so it compresses.
    fn smooth(len: usize) -> Vec<u8> {
        (0..len).map(|i| ((i / 7) % 200) as u8).collect()
    }

    #[test]
    fn compressible_data_tiles_and_unpacks_exactly() {
        let raw = smooth(3 * TILE + 1234);
        let payload = Payload::compress(raw.clone()).unwrap();
        let Payload::Tiled { size, tiles, data } = &payload else { panic!("smooth data compresses") };
        assert_eq!((*size, tiles.len()), (raw.len() as u64, 4));
        assert!(tiles.iter().all(|(offset, _)| offset % 4 == 0), "tiles start aligned for the GPU");
        assert!(data.len() < raw.len() / 4);
        assert_eq!(payload.unpack().unwrap(), raw.as_slice());
        assert_eq!(tile_len(*size, 3), 1234);
    }

    #[test]
    fn noise_ships_raw_and_damage_is_caught() {
        let mut state = 0x2545_f491_u32;
        let noise: Vec<u8> = (0..TILE)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        assert!(matches!(Payload::compress(noise).unwrap(), Payload::Raw(_)));

        let Payload::Tiled { size, tiles, mut data } = Payload::compress(smooth(TILE)).unwrap() else { panic!() };
        let middle = data.len() / 2;
        data[middle] ^= 0xff;
        data[middle + 1] ^= 0x55;
        assert!(Payload::Tiled { size, tiles, data }.unpack().is_err());
    }
}
