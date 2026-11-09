//! The files a terrain is kept in.
//!
//! A `.terrain` file is text, what a terrain is: how large and how fine.
//! Its heights are kept beside it, in a `.heights` file of the same name
//! (`Terrains/Hills.terrain` and `Terrains/Hills.heights`), as binary: the
//! samples are many, and a picture of them or numbers in text would be
//! larger and slower to read.
//!
//! A `.heights` file is `VXHEIGHT`, its version, the number of tiles each
//! way and the samples' spacing, then each tile, row after row along z:
//! its length, then its samples compressed. Before compressing, a tile's
//! 32-bit samples are split into four planes of bytes, all their lowest
//! bytes together, then the next, and so on: ground that changes slowly
//! repeats the same high bytes, and the compressor finds them together.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::heights::{HeightTile, Heightmap, TILE_LEN, TILE_QUADS};

const MAGIC: &[u8; 8] = b"VXHEIGHT";
const VERSION: u32 = 1;

/// The sample counts a terrain can be across: a power of two and one.
pub const RESOLUTIONS: [u32; 6] = [257, 513, 1025, 2049, 4097, 8193];

/// What a `.terrain` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerrainFile {
    /// How wide and long the terrain is, in metres.
    pub size: f32,
    /// How many samples it is across each way, one of `RESOLUTIONS`.
    pub resolution: u32,
}

impl Default for TerrainFile {
    fn default() -> Self {
        Self { size: 512.0, resolution: 1025 }
    }
}

impl TerrainFile {
    pub fn parse(text: &str) -> Result<Self> {
        let file: Self = toml::from_str(text)?;
        ensure!(file.size > 0.0 && file.size.is_finite(), "a terrain's size is more than nothing");
        ensure!(RESOLUTIONS.contains(&file.resolution), "a terrain's resolution is one of {RESOLUTIONS:?}");
        Ok(file)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a terrain file writes as TOML")
    }

    /// How many tiles it is across.
    pub fn tiles(&self) -> u32 {
        (self.resolution - 1) / TILE_QUADS
    }

    /// How many metres apart its samples are.
    pub fn spacing(&self) -> f32 {
        self.size / (self.resolution - 1) as f32
    }

    /// Flat ground as large and as fine as this says.
    pub fn flat(&self) -> Heightmap {
        Heightmap::flat(self.tiles(), self.tiles(), self.spacing(), 0.0)
    }

    /// `heights` made as large and as fine as this says, if they are not.
    pub fn fit(&self, heights: Heightmap) -> Heightmap {
        let (tiles, spacing) = (self.tiles(), self.spacing());
        let fits = heights.tiles_x() == tiles && heights.tiles_z() == tiles && heights.spacing() == spacing;
        if fits { heights } else { heights.resampled(tiles, tiles, spacing) }
    }
}

/// Where a terrain's heights are kept: beside it, named as it is.
pub fn heights_file(terrain: &str) -> String {
    let stem = terrain.strip_suffix(".terrain").unwrap_or(terrain);
    format!("{stem}.heights")
}

/// `heights` as a `.heights` file.
pub fn write_heights(heights: &Heightmap) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&heights.tiles_x().to_le_bytes());
    out.extend_from_slice(&heights.tiles_z().to_le_bytes());
    out.extend_from_slice(&heights.spacing().to_le_bytes());
    let mut planes = vec![0u8; TILE_LEN * 4];
    for tile in heights.tiles() {
        for (i, sample) in tile.samples().iter().enumerate() {
            for (plane, byte) in sample.to_le_bytes().into_iter().enumerate() {
                planes[plane * TILE_LEN + i] = byte;
            }
        }
        let packed = lz4_flex::compress(&planes);
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
    }
    out
}

/// The heights in a `.heights` file.
pub fn read_heights(bytes: &[u8]) -> Result<Heightmap> {
    let mut reader = Reader { bytes, at: 0 };
    ensure!(reader.take(8)? == MAGIC, "not a heights file");
    let version = reader.u32()?;
    if version != VERSION {
        bail!("heights file version {version}; this engine reads {VERSION}");
    }
    let (tiles_x, tiles_z) = (reader.u32()?, reader.u32()?);
    let spacing = f32::from_le_bytes(reader.take(4)?.try_into().expect("four bytes"));
    ensure!(tiles_x > 0 && tiles_z > 0 && tiles_x * tiles_z <= 64 * 64, "{tiles_x} by {tiles_z} tiles");
    ensure!(spacing > 0.0 && spacing.is_finite(), "samples {spacing} m apart");
    let mut tiles = Vec::with_capacity((tiles_x * tiles_z) as usize);
    for index in 0..tiles_x * tiles_z {
        let length = reader.u32()? as usize;
        let planes = lz4_flex::decompress(reader.take(length)?, TILE_LEN * 4)
            .with_context(|| format!("tile {index} of the heights"))?;
        ensure!(planes.len() == TILE_LEN * 4, "tile {index} is {} bytes", planes.len());
        let samples =
            (0..TILE_LEN).map(|i| f32::from_le_bytes([0, 1, 2, 3].map(|plane| planes[plane * TILE_LEN + i]))).collect();
        tiles.push(Arc::new(HeightTile::from_samples(samples)));
    }
    Ok(Heightmap::from_tiles(tiles_x, tiles_z, spacing, tiles))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len()).context("the file ends early")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_written_read_back_exactly_and_smaller_than_they_were() {
        let heights = Heightmap::from_fn(2, 1, 0.5, |x, z| (x * 0.03).sin() * 10.0 + (z * 0.01).cos() * 3.0);
        let bytes = write_heights(&heights);
        let read = read_heights(&bytes).expect("heights");
        assert_eq!((read.tiles_x(), read.tiles_z(), read.spacing()), (2, 1, 0.5));
        for (a, b) in heights.tiles().iter().zip(read.tiles()) {
            assert_eq!(a.samples(), b.samples());
        }
        assert!(bytes.len() < 2 * TILE_LEN * 4, "{} bytes", bytes.len());
        // Flat ground is next to nothing.
        assert!(write_heights(&Heightmap::flat(4, 4, 1.0, 2.0)).len() < 16 * 4096);
    }

    #[test]
    fn a_broken_heights_file_says_what_is_wrong() {
        let heights = write_heights(&Heightmap::flat(1, 1, 1.0, 0.0));
        assert!(read_heights(&heights[..heights.len() - 10]).is_err());
        assert!(read_heights(b"VXHEIGHT").is_err());
        assert!(read_heights(b"something else entirely").is_err());
    }

    #[test]
    fn a_terrain_file_is_its_size_and_resolution() {
        let file = TerrainFile::parse("size = 1000.0\nresolution = 2049\n").expect("a terrain");
        assert_eq!((file.tiles(), file.spacing()), (8, 1000.0 / 2048.0));
        assert_eq!(TerrainFile::parse(&file.to_text()).expect("again"), file);
        assert!(TerrainFile::parse("resolution = 1000").is_err());
        assert_eq!(heights_file("Terrains/Hills.terrain"), "Terrains/Hills.heights");
        let flat = file.flat();
        assert_eq!(flat.size(), [1000.0, 1000.0]);
        let fitted = TerrainFile::default().fit(flat);
        assert_eq!(fitted.size(), [512.0, 512.0]);
    }
}
