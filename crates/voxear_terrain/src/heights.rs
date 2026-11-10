//! A terrain's ground: heights on a square grid of samples, kept in tiles.
//!
//! Each tile is `TILE_QUADS` quads across, so `TILE_SAMPLES` samples each
//! way, and shares its edge samples with the tiles beside it. A tile is
//! whole on its own: drawing, collision and editing each take a tile at a
//! time, and the tiles of a terrain are shared (`Arc`), so a change to one
//! leaves the rest as they were, and what keeps a copy of a terrain on the
//! GPU or in physics sees which tiles changed by their pointers.
//!
//! Between samples the ground is two triangles a quad, split along the
//! diagonal from a quad's lowest corner to its highest (from `(x, z)` to
//! `(x + 1, z + 1)`), as physics splits its height fields, so what stands on
//! the ground, what is drawn and what a ray hits agree.
//!
//! A sample can be a hole's: every triangle with a corner on it is cut away,
//! drawn as nothing, and nothing stands on it or stops a ray there, as
//! physics cuts its height fields.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use voxear_core::math::Vec3;

/// How many quads a tile is across.
pub const TILE_QUADS: u32 = 256;

/// How many samples a tile is across: one more than its quads, the last
/// shared with the next tile.
pub const TILE_SAMPLES: u32 = TILE_QUADS + 1;

/// How many samples a tile holds.
pub const TILE_LEN: usize = (TILE_SAMPLES * TILE_SAMPLES) as usize;

/// How many quads the smallest block of a tile's lowest and highest
/// heights is across: the levels of detail's smallest node.
pub const LEAF_QUADS: u32 = 64;

/// How many such blocks a tile has each way.
pub const LEAVES: u32 = TILE_QUADS / LEAF_QUADS;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// How many 32-bit words a tile's holes are kept in, a bit a sample.
pub const HOLE_WORDS: usize = TILE_LEN.div_ceil(32);

/// One tile's heights, in metres, row after row along z, each row along x;
/// the lowest and highest height in each of its blocks; and which of its
/// samples are holes', if any are.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightTile {
    samples: Box<[f32]>,
    leaves: [[f32; 2]; (LEAVES * LEAVES) as usize],
    holes: Option<Box<[u32]>>,
}

impl HeightTile {
    /// A tile at one height.
    pub fn flat(height: f32) -> Self {
        Self::from_samples(vec![height; TILE_LEN])
    }

    /// A tile from its `TILE_LEN` samples.
    pub fn from_samples(samples: Vec<f32>) -> Self {
        assert_eq!(samples.len(), TILE_LEN, "a tile has {TILE_LEN} samples");
        let mut tile =
            Self { samples: samples.into_boxed_slice(), leaves: [[0.0; 2]; (LEAVES * LEAVES) as usize], holes: None };
        tile.measure();
        tile
    }

    /// The same tile with holes at the samples whose bits are set, `HOLE_WORDS`
    /// words of them, or none.
    pub fn with_holes(mut self, holes: Option<Box<[u32]>>) -> Self {
        if let Some(bits) = &holes {
            assert_eq!(bits.len(), HOLE_WORDS, "a tile's holes are {HOLE_WORDS} words");
        }
        self.holes = holes.filter(|bits| bits.iter().any(|&word| word != 0));
        self
    }

    /// Its holes, a bit a sample, row after row; `None` when it has none.
    pub fn holes(&self) -> Option<&[u32]> {
        self.holes.as_deref()
    }

    /// Whether sample `(x, z)` of the tile is a hole's.
    pub fn is_hole(&self, x: u32, z: u32) -> bool {
        self.holes.as_ref().is_some_and(|bits| {
            let index = (z * TILE_SAMPLES + x) as usize;
            bits[index / 32] >> (index % 32) & 1 != 0
        })
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The height at sample `(x, z)` of the tile, each from 0 to `TILE_QUADS`.
    pub fn get(&self, x: u32, z: u32) -> f32 {
        self.samples[(z * TILE_SAMPLES + x) as usize]
    }

    /// The lowest and highest heights of block `(x, z)`, each from 0 to
    /// `LEAVES - 1`, its edge samples included.
    pub fn leaf_range(&self, x: u32, z: u32) -> [f32; 2] {
        self.leaves[(z * LEAVES + x) as usize]
    }

    /// The lowest and highest heights in the tile.
    pub fn range(&self) -> [f32; 2] {
        self.leaves.iter().fold([f32::MAX, f32::MIN], |[low, high], [a, b]| [low.min(*a), high.max(*b)])
    }

    fn measure(&mut self) {
        for lz in 0..LEAVES {
            for lx in 0..LEAVES {
                let mut range = [f32::MAX, f32::MIN];
                for z in lz * LEAF_QUADS..=(lz + 1) * LEAF_QUADS {
                    let row = &self.samples[(z * TILE_SAMPLES + lx * LEAF_QUADS) as usize..][..=LEAF_QUADS as usize];
                    for &h in row {
                        range = [range[0].min(h), range[1].max(h)];
                    }
                }
                self.leaves[(lz * LEAVES + lx) as usize] = range;
            }
        }
    }
}

/// A terrain's heights: `tiles_x` by `tiles_z` tiles, `spacing` metres
/// between samples, from its corner at `(0, 0)` along +x and +z.
///
/// Its `id` is kept by every copy made from it with changes, so what keeps
/// the heights elsewhere can tell a new terrain from the same one changed.
#[derive(Clone, Debug)]
pub struct Heightmap {
    id: u64,
    spacing: f32,
    tiles_x: u32,
    tiles_z: u32,
    tiles: Vec<Arc<HeightTile>>,
}

impl PartialEq for Heightmap {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.spacing == other.spacing
            && (self.tiles_x, self.tiles_z) == (other.tiles_x, other.tiles_z)
            && self.tiles.iter().zip(&other.tiles).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
    }
}

impl Heightmap {
    /// Heights all at `height`.
    pub fn flat(tiles_x: u32, tiles_z: u32, spacing: f32, height: f32) -> Self {
        let tile = Arc::new(HeightTile::flat(height));
        let tiles = vec![tile; (tiles_x * tiles_z) as usize];
        Self::from_tiles(tiles_x, tiles_z, spacing, tiles)
    }

    /// Heights from `height(x, z)`, given each sample's place from the
    /// corner, in metres.
    pub fn from_fn(tiles_x: u32, tiles_z: u32, spacing: f32, height: impl Fn(f32, f32) -> f32) -> Self {
        let mut tiles = Vec::with_capacity((tiles_x * tiles_z) as usize);
        for tz in 0..tiles_z {
            for tx in 0..tiles_x {
                let mut samples = Vec::with_capacity(TILE_LEN);
                for z in 0..TILE_SAMPLES {
                    for x in 0..TILE_SAMPLES {
                        let at = |tile: u32, sample: u32| (tile * TILE_QUADS + sample) as f32 * spacing;
                        samples.push(height(at(tx, x), at(tz, z)));
                    }
                }
                tiles.push(Arc::new(HeightTile::from_samples(samples)));
            }
        }
        Self::from_tiles(tiles_x, tiles_z, spacing, tiles)
    }

    /// Heights from their tiles, row after row along z. A new terrain, with
    /// an id of its own.
    pub fn from_tiles(tiles_x: u32, tiles_z: u32, spacing: f32, tiles: Vec<Arc<HeightTile>>) -> Self {
        assert!(tiles_x > 0 && tiles_z > 0, "a terrain has a tile at least");
        assert_eq!(tiles.len(), (tiles_x * tiles_z) as usize, "a terrain has every tile");
        assert!(spacing > 0.0, "samples are apart");
        Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), spacing, tiles_x, tiles_z, tiles }
    }

    /// The same terrain with some of its tiles changed, `(x, z, tile)` each,
    /// keeping its id.
    pub fn with_tiles(&self, changed: impl IntoIterator<Item = (u32, u32, HeightTile)>) -> Self {
        let mut tiles = self.tiles.clone();
        for (x, z, tile) in changed {
            tiles[(z * self.tiles_x + x) as usize] = Arc::new(tile);
        }
        Self { tiles, ..self.clone() }
    }

    /// The same terrain with every tile in place of its own, as many,
    /// keeping its id: as a terrain without end's window moves.
    pub fn with_shared_tiles(&self, tiles: Vec<Arc<HeightTile>>) -> Self {
        assert_eq!(tiles.len(), self.tiles.len(), "a terrain has every tile");
        Self { tiles, ..self.clone() }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// How many metres apart samples are.
    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }

    pub fn tiles_z(&self) -> u32 {
        self.tiles_z
    }

    /// Every tile, row after row along z.
    pub fn tiles(&self) -> &[Arc<HeightTile>] {
        &self.tiles
    }

    pub fn tile(&self, x: u32, z: u32) -> &Arc<HeightTile> {
        &self.tiles[(z * self.tiles_x + x) as usize]
    }

    /// How many quads it is across each way.
    pub fn quads(&self) -> [u32; 2] {
        [self.tiles_x * TILE_QUADS, self.tiles_z * TILE_QUADS]
    }

    /// How many samples it is across each way.
    pub fn resolution(&self) -> [u32; 2] {
        let [x, z] = self.quads();
        [x + 1, z + 1]
    }

    /// How large it is, in metres along x and z.
    pub fn size(&self) -> [f32; 2] {
        let [x, z] = self.quads();
        [x as f32 * self.spacing, z as f32 * self.spacing]
    }

    /// The lowest and highest heights anywhere.
    pub fn range(&self) -> [f32; 2] {
        self.tiles
            .iter()
            .map(|tile| tile.range())
            .fold([f32::MAX, f32::MIN], |[low, high], [a, b]| [low.min(a), high.max(b)])
    }

    /// The height at sample `(x, z)` of the whole terrain, each clamped to
    /// its edges.
    pub fn sample(&self, x: i64, z: i64) -> f32 {
        let [qx, qz] = self.quads();
        let x = x.clamp(0, i64::from(qx)) as u32;
        let z = z.clamp(0, i64::from(qz)) as u32;
        let tx = (x / TILE_QUADS).min(self.tiles_x - 1);
        let tz = (z / TILE_QUADS).min(self.tiles_z - 1);
        self.tile(tx, tz).get(x - tx * TILE_QUADS, z - tz * TILE_QUADS)
    }

    /// Whether sample `(x, z)` of the whole terrain is a hole's, each clamped
    /// to its edges.
    pub fn is_hole(&self, x: i64, z: i64) -> bool {
        let [qx, qz] = self.quads();
        let x = x.clamp(0, i64::from(qx)) as u32;
        let z = z.clamp(0, i64::from(qz)) as u32;
        let tx = (x / TILE_QUADS).min(self.tiles_x - 1);
        let tz = (z / TILE_QUADS).min(self.tiles_z - 1);
        self.tile(tx, tz).is_hole(x - tx * TILE_QUADS, z - tz * TILE_QUADS)
    }

    /// Whether any tile has holes.
    pub fn has_holes(&self) -> bool {
        self.tiles.iter().any(|tile| tile.holes.is_some())
    }

    /// Whether the ground under `(x, z)`, in metres from the corner, is cut
    /// away: the triangle there has a corner on a hole's sample.
    pub fn cut_at(&self, x: f32, z: f32) -> bool {
        let ([ix, iz], [tx, tz]) = self.quad_at(x, z);
        self.is_hole(ix, iz)
            || self.is_hole(ix + 1, iz + 1)
            || if tz > tx { self.is_hole(ix, iz + 1) } else { self.is_hole(ix + 1, iz) }
    }

    /// The quad under `(x, z)`, in metres from the corner, and how far
    /// across it, 0 to 1 each way: clamped to the terrain's edges.
    fn quad_at(&self, x: f32, z: f32) -> ([i64; 2], [f32; 2]) {
        let [qx, qz] = self.quads();
        let fx = (x / self.spacing).clamp(0.0, qx as f32);
        let fz = (z / self.spacing).clamp(0.0, qz as f32);
        let ix = (fx.floor() as i64).min(i64::from(qx) - 1);
        let iz = (fz.floor() as i64).min(i64::from(qz) - 1);
        ([ix, iz], [fx - ix as f32, fz - iz as f32])
    }

    /// The ground's height at `(x, z)`, in metres from the corner, on the
    /// triangles between samples; at the nearest edge outside it.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let ([ix, iz], [tx, tz]) = self.quad_at(x, z);
        let h00 = self.sample(ix, iz);
        let h10 = self.sample(ix + 1, iz);
        let h01 = self.sample(ix, iz + 1);
        let h11 = self.sample(ix + 1, iz + 1);
        if tz > tx { h00 + tz * (h01 - h00) + tx * (h11 - h01) } else { h00 + tx * (h10 - h00) + tz * (h11 - h10) }
    }

    /// Which way the triangle under `(x, z)` faces.
    pub fn face_normal_at(&self, x: f32, z: f32) -> Vec3 {
        let ([ix, iz], [tx, tz]) = self.quad_at(x, z);
        let h00 = self.sample(ix, iz);
        let h10 = self.sample(ix + 1, iz);
        let h01 = self.sample(ix, iz + 1);
        let h11 = self.sample(ix + 1, iz + 1);
        let (dx, dz) = if tz > tx { (h11 - h01, h01 - h00) } else { (h10 - h00, h11 - h10) };
        Vec3::new(-dx, self.spacing, -dz).normalize()
    }

    /// Which way the ground faces at `(x, z)`, smoothed over the samples
    /// round it, as it is drawn: for standing things on it.
    pub fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        let step = self.spacing;
        let dx = self.height_at(x + step, z) - self.height_at(x - step, z);
        let dz = self.height_at(x, z + step) - self.height_at(x, z - step);
        Vec3::new(-dx, 2.0 * step, -dz).normalize()
    }

    /// Whether `(x, z)`, in metres from the corner, is over the terrain.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        let [w, l] = self.size();
        (0.0..=w).contains(&x) && (0.0..=l).contains(&z)
    }

    /// The same ground, `tiles_x` by `tiles_z` tiles `spacing` apart, its
    /// heights read from this one where they fall: for a terrain made
    /// larger, smaller, finer or coarser. A new terrain, with an id of its
    /// own.
    pub fn resampled(&self, tiles_x: u32, tiles_z: u32, spacing: f32) -> Self {
        let [w, l] = self.size();
        let [nw, nl] = [(tiles_x * TILE_QUADS) as f32 * spacing, (tiles_z * TILE_QUADS) as f32 * spacing];
        // Stretched to fit, so the same hills cover the same share of it.
        Self::from_fn(tiles_x, tiles_z, spacing, |x, z| self.bilinear_at(x / nw * w, z / nl * l))
    }

    /// The height at `(x, z)` blended from the four samples round it.
    pub fn bilinear_at(&self, x: f32, z: f32) -> f32 {
        let ([ix, iz], [tx, tz]) = self.quad_at(x, z);
        let top = self.sample(ix, iz) * (1.0 - tx) + self.sample(ix + 1, iz) * tx;
        let bottom = self.sample(ix, iz + 1) * (1.0 - tx) + self.sample(ix + 1, iz + 1) * tx;
        top * (1.0 - tz) + bottom * tz
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slope() -> Heightmap {
        Heightmap::from_fn(2, 2, 0.5, |x, z| x * 0.1 + z * 0.05)
    }

    #[test]
    fn a_tile_knows_its_lowest_and_highest_heights_block_by_block() {
        let map = Heightmap::from_fn(1, 1, 1.0, |x, z| x + z * 1000.0);
        let tile = map.tile(0, 0);
        assert_eq!(tile.leaf_range(0, 0), [0.0, 64.0 + 64_000.0]);
        assert_eq!(tile.leaf_range(3, 3), [192.0 + 192_000.0, 256.0 + 256_000.0]);
        assert_eq!(tile.range(), [0.0, 256.0 + 256_000.0]);
    }

    #[test]
    fn tiles_share_their_edges_so_a_sample_reads_the_same_from_either() {
        let map = slope();
        let edge = map.tile(0, 0).get(TILE_QUADS, 10);
        assert_eq!(edge, map.tile(1, 0).get(0, 10));
        assert_eq!(map.sample(256, 10), edge);
    }

    #[test]
    fn a_plane_reads_back_exactly_between_samples_and_its_normal_is_its_slope() {
        let map = slope();
        let at = map.height_at(100.3, 77.7);
        assert!((at - (100.3 * 0.1 + 77.7 * 0.05)).abs() < 1e-4, "{at}");
        let normal = map.face_normal_at(100.3, 77.7);
        let expected = Vec3::new(-0.1, 1.0, -0.05).normalize();
        assert!(normal.distance(expected) < 1e-4, "{normal}");
        assert!(map.normal_at(100.3, 77.7).distance(expected) < 1e-4);
    }

    #[test]
    fn the_ground_between_samples_is_two_triangles_split_from_the_low_corner_to_the_high() {
        // One raised far corner: on the split, halfway along, the ground is half as high.
        let map = Heightmap::from_fn(1, 1, 1.0, |x, z| if x == 1.0 && z == 1.0 { 1.0 } else { 0.0 });
        assert!((map.height_at(0.5, 0.5) - 0.5).abs() < 1e-6);
        // Off the split, the triangle it is in rises only towards that corner.
        assert!((map.height_at(0.75, 0.25) - 0.25).abs() < 1e-6);
        assert!((map.height_at(0.25, 0.75) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn outside_the_terrain_the_nearest_edge_holds() {
        let map = slope();
        let [w, _] = map.size();
        assert_eq!(map.height_at(-10.0, 0.0), map.height_at(0.0, 0.0));
        assert!((map.height_at(w + 50.0, 0.0) - map.height_at(w, 0.0)).abs() < 1e-6);
        assert!(!map.contains(-0.1, 1.0) && map.contains(w, 1.0));
    }

    #[test]
    fn a_change_to_a_tile_keeps_the_terrain_and_the_other_tiles() {
        let map = slope();
        let changed = map.with_tiles([(1, 0, HeightTile::flat(3.0))]);
        assert_eq!(changed.id(), map.id());
        assert!(Arc::ptr_eq(changed.tile(0, 0), map.tile(0, 0)));
        assert!(!Arc::ptr_eq(changed.tile(1, 0), map.tile(1, 0)));
        assert_ne!(changed, map);
        assert_eq!(changed.height_at(200.0, 10.0), 3.0);
    }

    #[test]
    fn resampling_keeps_the_ground_where_it_was() {
        let map = slope();
        let finer = map.resampled(4, 4, 0.25);
        assert_ne!(finer.id(), map.id());
        assert_eq!(finer.size(), map.size());
        assert!((finer.height_at(100.3, 77.7) - map.height_at(100.3, 77.7)).abs() < 1e-3);
    }
}
