//! What the sculpting brushes paint: for each Sculpt layer, how far its
//! brushes raised or lowered the ground, sample by sample, only in the tiles
//! they touched. Tiles are shared (`Arc`), so a brush stroke makes new tiles
//! where it went and keeps the rest, and undoing it puts the old ones back.
//!
//! They are kept in a `.layers` file beside the terrain: `VXLAYERS`, its
//! version, how many tiles, then each tile's raster, place and samples,
//! compressed as a `.heights` file's are. Builds leave it out: a game reads
//! the ground as it was made, in the `.heights` file.

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, ensure};

use crate::heights::{TILE_LEN, TILE_QUADS, TILE_SAMPLES};

const MAGIC: &[u8; 8] = b"VXLAYERS";
const VERSION: u32 = 1;

/// One tile of a raster: a change in height for each sample, in metres.
pub type RasterTile = Arc<[f32]>;

/// Every Sculpt layer's tiles, by raster and tile.
#[derive(Clone, Debug, Default)]
pub struct Rasters {
    tiles: BTreeMap<(u32, u32, u32), RasterTile>,
}

impl PartialEq for Rasters {
    fn eq(&self, other: &Self) -> bool {
        self.tiles.len() == other.tiles.len()
            && self.tiles.iter().zip(&other.tiles).all(|((a, x), (b, y))| a == b && (Arc::ptr_eq(x, y) || x == y))
    }
}

impl Rasters {
    /// Raster `raster`'s tile `(x, z)`, if a brush has touched it.
    pub fn tile(&self, raster: u32, x: u32, z: u32) -> Option<&RasterTile> {
        self.tiles.get(&(raster, x, z))
    }

    /// The change raster `raster` makes at sample `(x, z)` of the whole
    /// terrain: none where no brush went.
    pub fn sample(&self, raster: u32, x: i64, z: i64) -> f32 {
        if x < 0 || z < 0 {
            return 0.0;
        }
        let (tx, tz) = ((x / i64::from(TILE_QUADS)) as u32, (z / i64::from(TILE_QUADS)) as u32);
        let (sx, sz) = (x as u32 - tx * TILE_QUADS, z as u32 - tz * TILE_QUADS);
        self.tile(raster, tx, tz).map_or(0.0, |tile| tile[(sz * TILE_SAMPLES + sx) as usize])
    }

    /// Raster `raster`'s tile `(x, z)` to change, made as a copy, or empty,
    /// so the one held before is kept as it was.
    pub fn tile_mut(&mut self, raster: u32, x: u32, z: u32) -> &mut [f32] {
        let tile = self.tiles.entry((raster, x, z)).or_insert_with(|| vec![0.0; TILE_LEN].into());
        if Arc::get_mut(tile).is_none() {
            *tile = tile.iter().copied().collect();
        }
        Arc::get_mut(tile).expect("a tile of its own")
    }

    /// Puts a tile in, or takes it out with `None`.
    pub fn set_tile(&mut self, raster: u32, x: u32, z: u32, tile: Option<RasterTile>) {
        match tile {
            Some(tile) => self.tiles.insert((raster, x, z), tile),
            None => self.tiles.remove(&(raster, x, z)),
        };
    }

    /// Every tile, by raster and place.
    pub fn tiles(&self) -> impl Iterator<Item = ((u32, u32, u32), &RasterTile)> {
        self.tiles.iter().map(|(key, tile)| (*key, tile))
    }

    /// Takes away raster `raster`'s tiles, as when its layer is removed.
    pub fn remove(&mut self, raster: u32) {
        self.tiles.retain(|(r, ..), _| *r != raster);
    }

    /// A raster number no layer has yet.
    pub fn next_raster(&self, taken: impl IntoIterator<Item = u32>) -> u32 {
        let used = taken.into_iter().chain(self.tiles.keys().map(|(r, ..)| *r));
        used.max().map_or(1, |most| most + 1)
    }

    /// The same changes on a terrain of `tiles` tiles `spacing` apart, from
    /// one of `old_tiles` tiles `old_spacing` apart, each sample's change read
    /// where it falls, stretched as the ground is.
    pub fn resampled(&self, old: ([u32; 2], f32), new: ([u32; 2], f32)) -> Self {
        let ((old_tiles, old_spacing), (new_tiles, new_spacing)) = (old, new);
        let old_size = [
            old_tiles[0] as f32 * TILE_QUADS as f32 * old_spacing,
            old_tiles[1] as f32 * TILE_QUADS as f32 * old_spacing,
        ];
        let new_size = [
            new_tiles[0] as f32 * TILE_QUADS as f32 * new_spacing,
            new_tiles[1] as f32 * TILE_QUADS as f32 * new_spacing,
        ];
        let rasters: std::collections::BTreeSet<u32> = self.tiles.keys().map(|(r, ..)| *r).collect();
        let mut made = Self::default();
        for raster in rasters {
            for tz in 0..new_tiles[1] {
                for tx in 0..new_tiles[0] {
                    let mut samples = vec![0.0f32; TILE_LEN];
                    let mut any = false;
                    for z in 0..TILE_SAMPLES {
                        for x in 0..TILE_SAMPLES {
                            let along = [
                                (tx * TILE_QUADS + x) as f32 * new_spacing,
                                (tz * TILE_QUADS + z) as f32 * new_spacing,
                            ];
                            let at = [
                                along[0] / new_size[0] * old_size[0] / old_spacing,
                                along[1] / new_size[1] * old_size[1] / old_spacing,
                            ];
                            let value = self.bilinear(raster, at);
                            any |= value != 0.0;
                            samples[(z * TILE_SAMPLES + x) as usize] = value;
                        }
                    }
                    if any {
                        made.tiles.insert((raster, tx, tz), samples.into());
                    }
                }
            }
        }
        made
    }

    fn bilinear(&self, raster: u32, [x, z]: [f32; 2]) -> f32 {
        let (ix, iz) = (x.floor() as i64, z.floor() as i64);
        let (tx, tz) = (x - ix as f32, z - iz as f32);
        let top = self.sample(raster, ix, iz) * (1.0 - tx) + self.sample(raster, ix + 1, iz) * tx;
        let bottom = self.sample(raster, ix, iz + 1) * (1.0 - tx) + self.sample(raster, ix + 1, iz + 1) * tx;
        top * (1.0 - tz) + bottom * tz
    }
}

/// Where a terrain's rasters are kept: beside it, named as it is.
pub fn layers_file(terrain: &str) -> String {
    let stem = terrain.strip_suffix(".terrain").unwrap_or(terrain);
    format!("{stem}.layers")
}

/// The rasters as a `.layers` file.
pub fn write_rasters(rasters: &Rasters) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(rasters.tiles.len() as u32).to_le_bytes());
    let mut planes = vec![0u8; TILE_LEN * 4];
    for ((raster, x, z), tile) in &rasters.tiles {
        for value in [*raster, *x, *z] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        crate::file::split_planes(tile, &mut planes);
        let packed = lz4_flex::compress(&planes);
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
    }
    out
}

/// The rasters in a `.layers` file.
pub fn read_rasters(bytes: &[u8]) -> Result<Rasters> {
    let mut reader = crate::file::Reader::new(bytes);
    ensure!(reader.take(8)? == MAGIC, "not a layers file");
    let version = reader.u32()?;
    ensure!(version == VERSION, "layers file version {version}; this engine reads {VERSION}");
    let count = reader.u32()?;
    let mut rasters = Rasters::default();
    for index in 0..count {
        let (raster, x, z) = (reader.u32()?, reader.u32()?, reader.u32()?);
        ensure!(x < 64 && z < 64, "tile {x}, {z} of raster {raster}");
        let length = reader.u32()? as usize;
        let planes = lz4_flex::decompress(reader.take(length)?, TILE_LEN * 4)
            .with_context(|| format!("tile {index} of the layers"))?;
        let samples: Vec<f32> = crate::file::join_planes(&planes)?;
        rasters.tiles.insert((raster, x, z), samples.into());
    }
    Ok(rasters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_painted_tile_is_a_copy_and_the_one_before_stays_as_it_was() {
        let mut rasters = Rasters::default();
        rasters.tile_mut(1, 0, 0)[5] = 2.0;
        let before = rasters.clone();
        rasters.tile_mut(1, 0, 0)[5] = 3.0;
        assert_eq!(before.sample(1, 5, 0), 2.0);
        assert_eq!(rasters.sample(1, 5, 0), 3.0);
        assert_ne!(before, rasters);
        assert_eq!(rasters.sample(2, 5, 0), 0.0, "another raster is untouched");
        assert_eq!(rasters.next_raster([4]), 5);
    }

    #[test]
    fn rasters_written_read_back_exactly() {
        let mut rasters = Rasters::default();
        rasters.tile_mut(1, 1, 0)[300] = -1.5;
        rasters.tile_mut(3, 0, 2)[0] = 0.25;
        let read = read_rasters(&write_rasters(&rasters)).expect("rasters");
        assert_eq!(read, rasters);
        assert!(read_rasters(b"VXLAYERS").is_err());
        assert_eq!(layers_file("Terrains/Hills.terrain"), "Terrains/Hills.layers");
    }

    #[test]
    fn a_raster_stretched_to_a_finer_terrain_keeps_its_changes_where_they_were() {
        let mut rasters = Rasters::default();
        for x in 100..=120 {
            rasters.tile_mut(1, 0, 0)[(50 * TILE_SAMPLES + x) as usize] = 4.0;
        }
        // From 1 tile 1 m apart to 2 tiles 0.5 m apart: the same 256 m.
        let finer = rasters.resampled(([1, 1], 1.0), ([2, 2], 0.5));
        assert_eq!(finer.sample(1, 220, 100), 4.0, "110 m along, 50 m in");
        assert_eq!(finer.sample(1, 100, 100), 0.0);
    }
}
