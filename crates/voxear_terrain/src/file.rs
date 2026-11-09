//! The files a terrain is kept in.
//!
//! A `.terrain` file is text, what a terrain is: how large and how fine,
//! and its layers, bottom to top (see `layers`). Its heights, as its layers
//! made them, are kept beside it in a `.heights` file of the same name
//! (`Terrains/Hills.terrain` and `Terrains/Hills.heights`), as binary: the
//! samples are many, and a picture of them or numbers in text would be
//! larger and slower to read. A game reads only those; the editor makes them
//! again when the layers change.
//!
//! A `.heights` file is `VXHEIGHT`, its version, the number of tiles each
//! way, the samples' spacing, a hash of the layers it was made from and one
//! of the surfaces its shares were worked out from (0 for none kept), then
//! each tile, row after row along z: its samples, its holes and, with the
//! surfaces' hash, its shares (see `surfaces`), each as a length and the
//! bytes compressed, a length of 0 for no holes. Before compressing, a
//! tile's 32-bit values are split into planes of bytes, all their lowest
//! bytes together, then the next, and so on: ground that changes slowly
//! repeats the same high bytes, and the compressor finds them together.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_LEN, TILE_QUADS},
    layers::{Layer, LayerKind},
    surfaces::{Surface, WeightTile, Weights},
};

const MAGIC: &[u8; 8] = b"VXHEIGHT";
const VERSION: u32 = 3;

/// The sample counts a terrain can be across: a power of two and one.
pub const RESOLUTIONS: [u32; 6] = [257, 513, 1025, 2049, 4097, 8193];

/// What a `.terrain` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "Written")]
pub struct TerrainFile {
    /// How wide and long the terrain is, in metres.
    pub size: f32,
    /// How many samples it is across each way, one of `RESOLUTIONS`.
    pub resolution: u32,
    /// How high the ground is before any layer, in metres.
    #[serde(skip_serializing_if = "is_zero")]
    pub base: f32,
    /// What the ground is made of, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// What the ground is covered with, bottom to top (see `surfaces`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<Surface>,
    /// The raster of its holes, a sample a hole's where it is 1; 0 for none.
    #[serde(skip_serializing_if = "is_none")]
    pub holes: u32,
}

fn is_none(raster: &u32) -> bool {
    *raster == 0
}

/// A `.terrain` file as written, files from before layers too, whose
/// heightmap picture is read as an Import layer.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Written {
    size: f32,
    resolution: u32,
    base: f32,
    layers: Vec<Layer>,
    surfaces: Vec<Surface>,
    holes: u32,
    heightmap: Option<String>,
    height: f32,
}

impl Default for Written {
    fn default() -> Self {
        let file = TerrainFile::default();
        Self {
            size: file.size,
            resolution: file.resolution,
            base: 0.0,
            layers: Vec::new(),
            surfaces: Vec::new(),
            holes: 0,
            heightmap: None,
            height: 100.0,
        }
    }
}

impl From<Written> for TerrainFile {
    fn from(written: Written) -> Self {
        let mut layers = written.layers;
        if let Some(picture) = written.heightmap {
            let mut layer = Layer::new(LayerKind::Import { picture: Some(picture), height: written.height });
            layer.name = "Heightmap".into();
            layers.insert(0, layer);
        }
        Self {
            size: written.size,
            resolution: written.resolution,
            base: written.base,
            layers,
            surfaces: written.surfaces,
            holes: written.holes,
        }
    }
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl Default for TerrainFile {
    fn default() -> Self {
        Self { size: 512.0, resolution: 1025, base: 0.0, layers: Vec::new(), surfaces: Vec::new(), holes: 0 }
    }
}

impl TerrainFile {
    pub fn parse(text: &str) -> Result<Self> {
        let file: Self = toml::from_str(text)?;
        ensure!(file.size > 0.0 && file.size.is_finite(), "a terrain's size is more than nothing");
        ensure!(RESOLUTIONS.contains(&file.resolution), "a terrain's resolution is one of {RESOLUTIONS:?}");
        ensure!(file.base.is_finite(), "a terrain's base is a number");
        Ok(file)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a terrain file writes as TOML")
    }

    /// A hash of what makes its ground: its size, resolution, base,
    /// layers and holes.
    pub fn shape_hash(&self) -> u64 {
        let layers = crate::layers::hash_layers(self.base, &self.layers);
        crate::layers::stable_hash(format!("{} {} {layers} {}", self.size, self.resolution, self.holes).as_bytes())
    }

    /// The rasters its sculpt layers, surfaces and holes keep, for giving a
    /// new one a number of its own.
    pub fn rasters_in_use(&self) -> Vec<u32> {
        let sculpted = self.layers.iter().filter_map(|layer| match layer.kind {
            LayerKind::Sculpt { raster } => Some(raster),
            _ => None,
        });
        let painted = self.surfaces.iter().map(|surface| surface.paint);
        sculpted.chain(painted).chain([self.holes]).filter(|&raster| raster != 0).collect()
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
        Heightmap::flat(self.tiles(), self.tiles(), self.spacing(), self.base)
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

/// `heights` as a `.heights` file, made from layers whose
/// [`TerrainFile::shape_hash`] is `made_from`, with no shares kept.
pub fn write_heights(heights: &Heightmap, made_from: u64) -> Vec<u8> {
    write_ground(heights, made_from, None)
}

/// The heights in a `.heights` file, and the hash of the layers they were
/// made from (0 for a file that does not say).
pub fn read_heights(bytes: &[u8]) -> Result<(Heightmap, u64)> {
    read_ground(bytes).map(|stored| (stored.heights, stored.made_from))
}

/// What a `.heights` file keeps: the ground's heights and holes, the hash of
/// the layers they were made from, and the surfaces' shares with the hash
/// of the surfaces they were worked out from, if it keeps them.
pub struct Stored {
    pub heights: Heightmap,
    pub made_from: u64,
    pub weights: Option<(Weights, u64)>,
}

/// `heights` as a `.heights` file, made from layers whose
/// [`TerrainFile::shape_hash`] is `made_from`, with the shares `weights`
/// worked out from surfaces whose `surfaces::hash_surfaces` is the number
/// beside them.
pub fn write_ground(heights: &Heightmap, made_from: u64, weights: Option<(&Weights, u64)>) -> Vec<u8> {
    let weights = weights.filter(|(weights, _)| weights.fits(heights));
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&heights.tiles_x().to_le_bytes());
    out.extend_from_slice(&heights.tiles_z().to_le_bytes());
    out.extend_from_slice(&heights.spacing().to_le_bytes());
    out.extend_from_slice(&made_from.to_le_bytes());
    out.extend_from_slice(&weights.map_or(0, |(_, hash)| hash.max(1)).to_le_bytes());
    let mut planes = vec![0u8; TILE_LEN * 4];
    let put = |out: &mut Vec<u8>, bytes: &[u8]| {
        let packed = lz4_flex::compress(bytes);
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
    };
    for (index, tile) in heights.tiles().iter().enumerate() {
        split_planes(tile.samples(), &mut planes);
        put(&mut out, &planes);
        match tile.holes() {
            Some(bits) => put(&mut out, bytemuck::cast_slice(bits)),
            None => out.extend_from_slice(&0u32.to_le_bytes()),
        }
        if let Some((weights, _)) = weights {
            let samples = weights.tiles()[index].samples();
            let mut words = vec![0u8; TILE_LEN * 8];
            // The surfaces' numbers and the shares as two runs, each in planes of bytes.
            let (surfaces, shares): (Vec<u32>, Vec<u32>) = samples.iter().map(|[a, b]| (*a, *b)).unzip();
            split_words(&surfaces, &mut words[..TILE_LEN * 4]);
            split_words(&shares, &mut words[TILE_LEN * 4..]);
            put(&mut out, &words);
        }
    }
    out
}

/// What a `.heights` file keeps (see [`Stored`]).
pub fn read_ground(bytes: &[u8]) -> Result<Stored> {
    let mut reader = Reader::new(bytes);
    ensure!(reader.take(8)? == MAGIC, "not a heights file");
    let version = reader.u32()?;
    if !(1..=VERSION).contains(&version) {
        bail!("heights file version {version}; this engine reads up to {VERSION}");
    }
    let (tiles_x, tiles_z) = (reader.u32()?, reader.u32()?);
    let spacing = f32::from_le_bytes(reader.take(4)?.try_into().expect("four bytes"));
    let mut u64_after = |since: u32| -> Result<u64> {
        if version < since {
            return Ok(0);
        }
        Ok(u64::from_le_bytes(reader.take(8)?.try_into().expect("eight bytes")))
    };
    let made_from = u64_after(2)?;
    let weighed_from = u64_after(3)?;
    ensure!(tiles_x > 0 && tiles_z > 0 && tiles_x * tiles_z <= 64 * 64, "{tiles_x} by {tiles_z} tiles");
    ensure!(spacing > 0.0 && spacing.is_finite(), "samples {spacing} m apart");
    let mut tiles = Vec::with_capacity((tiles_x * tiles_z) as usize);
    let mut shares = Vec::new();
    for index in 0..tiles_x * tiles_z {
        let length = reader.u32()? as usize;
        let planes = lz4_flex::decompress(reader.take(length)?, TILE_LEN * 4)
            .with_context(|| format!("tile {index} of the heights"))?;
        let mut tile = HeightTile::from_samples(join_planes(&planes)?);
        if version >= 3 {
            let length = reader.u32()? as usize;
            if length > 0 {
                let bytes = lz4_flex::decompress(reader.take(length)?, HOLE_WORDS * 4)
                    .with_context(|| format!("tile {index}'s holes"))?;
                ensure!(bytes.len() == HOLE_WORDS * 4, "tile {index}'s holes are {} bytes", bytes.len());
                let bits: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|word| u32::from_le_bytes(*word)).collect();
                tile = tile.with_holes(Some(bits.into()));
            }
        }
        tiles.push(Arc::new(tile));
        if weighed_from != 0 {
            let length = reader.u32()? as usize;
            let words = lz4_flex::decompress(reader.take(length)?, TILE_LEN * 8)
                .with_context(|| format!("tile {index}'s surfaces"))?;
            ensure!(words.len() == TILE_LEN * 8, "tile {index}'s surfaces are {} bytes", words.len());
            let (surfaces, weights) = (join_words(&words[..TILE_LEN * 4]), join_words(&words[TILE_LEN * 4..]));
            let samples = surfaces.into_iter().zip(weights).map(|(a, b)| [a, b]).collect();
            shares.push(Arc::new(WeightTile::from_samples(samples)));
        }
    }
    let heights = Heightmap::from_tiles(tiles_x, tiles_z, spacing, tiles);
    let weights = (weighed_from != 0).then(|| (Weights::from_tiles(tiles_x, tiles_z, shares), weighed_from));
    Ok(Stored { heights, made_from, weights })
}

/// A tile's samples as four planes of bytes, lowest bytes first.
pub(crate) fn split_planes(samples: &[f32], planes: &mut [u8]) {
    for (i, sample) in samples.iter().enumerate() {
        for (plane, byte) in sample.to_le_bytes().into_iter().enumerate() {
            planes[plane * TILE_LEN + i] = byte;
        }
    }
}

/// A tile's 32-bit words as four planes of bytes, lowest bytes first.
fn split_words(words: &[u32], planes: &mut [u8]) {
    for (i, word) in words.iter().enumerate() {
        for (plane, byte) in word.to_le_bytes().into_iter().enumerate() {
            planes[plane * TILE_LEN + i] = byte;
        }
    }
}

/// A tile's 32-bit words from their four planes of bytes.
fn join_words(planes: &[u8]) -> Vec<u32> {
    (0..TILE_LEN).map(|i| u32::from_le_bytes([0, 1, 2, 3].map(|plane| planes[plane * TILE_LEN + i]))).collect()
}

/// A tile's samples from its four planes of bytes.
pub(crate) fn join_planes(planes: &[u8]) -> Result<Vec<f32>> {
    ensure!(planes.len() == TILE_LEN * 4, "a tile is {} bytes", planes.len());
    Ok((0..TILE_LEN).map(|i| f32::from_le_bytes([0, 1, 2, 3].map(|plane| planes[plane * TILE_LEN + i]))).collect())
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(crate) fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len()).context("the file ends early")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_written_read_back_exactly_and_smaller_than_they_were() {
        let heights = Heightmap::from_fn(2, 1, 0.5, |x, z| (x * 0.03).sin() * 10.0 + (z * 0.01).cos() * 3.0);
        let bytes = write_heights(&heights, 42);
        let (read, made_from) = read_heights(&bytes).expect("heights");
        assert_eq!(made_from, 42);
        assert_eq!((read.tiles_x(), read.tiles_z(), read.spacing()), (2, 1, 0.5));
        for (a, b) in heights.tiles().iter().zip(read.tiles()) {
            assert_eq!(a.samples(), b.samples());
        }
        assert!(bytes.len() < 2 * TILE_LEN * 4, "{} bytes", bytes.len());
        // Flat ground is next to nothing.
        assert!(write_heights(&Heightmap::flat(4, 4, 1.0, 2.0), 0).len() < 16 * 4096);
    }

    #[test]
    fn holes_and_shares_written_read_back_exactly() {
        let mut bits = vec![0u32; HOLE_WORDS];
        bits[3] = 0b1010;
        let tile = HeightTile::from_samples(vec![1.0; TILE_LEN]).with_holes(Some(bits.clone().into()));
        let heights = Heightmap::from_tiles(2, 1, 1.0, vec![Arc::new(tile), Arc::new(HeightTile::flat(2.0))]);
        let mut samples = vec![[0u32, 255u32]; TILE_LEN];
        samples[100] = [0x0302_0100, 0x0010_20CF];
        let weights = Weights::from_tiles(2, 1, vec![Arc::new(WeightTile::from_samples(samples)), Arc::new(WeightTile::all(1))]);
        let bytes = write_ground(&heights, 7, Some((&weights, 9)));
        let stored = read_ground(&bytes).expect("ground");
        assert_eq!(stored.made_from, 7);
        assert_eq!(stored.heights.tile(0, 0).holes(), Some(&bits[..]));
        assert!(stored.heights.tile(1, 0).holes().is_none());
        assert!(stored.heights.is_hole(97, 0) && !stored.heights.is_hole(96, 0));
        let (read, hash) = stored.weights.expect("shares kept");
        assert_eq!((read, hash), (weights, 9));
        // A file of heights alone has no shares; a version 2 file reads as before.
        assert!(read_ground(&write_heights(&heights, 7)).unwrap().weights.is_none());
        assert!(bytes.len() < 3 * 4096, "{} bytes", bytes.len());
    }

    #[test]
    fn a_broken_heights_file_says_what_is_wrong() {
        let heights = write_heights(&Heightmap::flat(1, 1, 1.0, 0.0), 0);
        assert!(read_heights(&heights[..heights.len() - 10]).is_err());
        assert!(read_heights(b"VXHEIGHT").is_err());
        assert!(read_heights(b"something else entirely").is_err());
    }

    #[test]
    fn a_terrain_file_is_its_size_and_resolution() {
        let file = TerrainFile::parse("size = 1000.0\nresolution = 2049\n").expect("a terrain");
        assert!(file.layers.is_empty(), "made of no layers");
        // A file from before layers, its heightmap read as the first layer.
        let older = TerrainFile::parse("size = 100.0\nresolution = 257\nheightmap = \"abc\"\nheight = 30.0\n").unwrap();
        assert!(
            matches!(&older.layers[0].kind, LayerKind::Import { picture: Some(p), height } if p == "abc" && *height == 30.0)
        );
        assert!(!older.to_text().contains("heightmap"), "{}", older.to_text());
        assert_ne!(
            older.shape_hash(),
            TerrainFile { size: 100.0, resolution: 257, ..TerrainFile::default() }.shape_hash()
        );
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
