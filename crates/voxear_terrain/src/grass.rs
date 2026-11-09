//! Grass and small details on a terrain: blades, tufts, flowers and pebbles,
//! millions of them, too many to keep a list of. Each kind keeps how thickly
//! it grows at each sample, a byte from nothing to as thick as its density
//! says, worked out from where it grows (everywhere, only where painted, or
//! on some of the terrain's surfaces, as much as they cover) and what was
//! painted added to that or taken from it, and kept off ground too steep.
//! The renderer scatters the blades round the camera on the GPU from those
//! bytes, the same blades in the same places every frame.
//!
//! Four kinds' bytes fit in a 32-bit word, so a terrain with up to four
//! kinds keeps one word a sample, and up to eight, two.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    brush::{Brush, add},
    composite::Dirty,
    file::TerrainFile,
    heights::{Heightmap, TILE_LEN, TILE_QUADS, TILE_SAMPLES},
    layers::{Area, smoothstep, stable_hash},
    raster::Rasters,
    surfaces::{Weights, slope_at},
};

/// How many kinds of grass a terrain can have.
pub const MAX_GRASS: usize = 8;

/// One kind of grass or detail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grass {
    /// The mesh each blade or tuft is, as a Mesh Renderer names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
    /// The material it is drawn with, by its GUID; none for a plain one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// How many to a square metre where it grows fully.
    #[serde(default = "four")]
    pub density: f32,
    /// The smallest and largest size.
    #[serde(default = "sizes")]
    pub scale: [f32; 2],
    /// How far from the camera it is drawn, in metres; it thins out over
    /// the last fifth.
    #[serde(default = "forty")]
    pub draw_distance: f32,
    /// Where it grows, before any painting.
    #[serde(default)]
    pub cover: GrassCover,
    /// With `GrassCover::Surfaces`: the surfaces it grows on, by their place
    /// in the terrain's list, as thickly as they cover.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<u32>,
    /// The steepest ground it grows on, in degrees, thinning over the last
    /// five.
    #[serde(default = "steep")]
    pub steepest: f32,
    /// The raster of its painting, which adds to where it grows; 0 before
    /// it is painted.
    #[serde(default, skip_serializing_if = "is_nothing")]
    pub paint: u32,
}

fn four() -> f32 {
    4.0
}

fn sizes() -> [f32; 2] {
    [0.8, 1.2]
}

fn forty() -> f32 {
    40.0
}

fn steep() -> f32 {
    40.0
}

fn is_nothing(value: &u32) -> bool {
    *value == 0
}

/// Where a kind of grass grows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrassCover {
    /// Everywhere, but where painting takes it away.
    #[default]
    Everywhere,
    /// Only where it is painted.
    Painted,
    /// On the surfaces it names, as thickly as they cover, and where it is
    /// painted.
    Surfaces,
}

impl GrassCover {
    pub const ALL: [GrassCover; 3] = [GrassCover::Everywhere, GrassCover::Painted, GrassCover::Surfaces];

    pub fn name(self) -> &'static str {
        match self {
            GrassCover::Everywhere => "Everywhere",
            GrassCover::Painted => "Painted",
            GrassCover::Surfaces => "On Surfaces",
        }
    }
}

impl Grass {
    /// A kind of grass of `mesh`, growing everywhere.
    pub fn new(mesh: Option<String>) -> Self {
        Self {
            mesh,
            material: None,
            density: four(),
            scale: sizes(),
            draw_distance: forty(),
            cover: GrassCover::Everywhere,
            surfaces: Vec::new(),
            steepest: steep(),
            paint: 0,
        }
    }
}

/// How many 32-bit words a sample of `kinds` kinds takes.
pub fn words(kinds: usize) -> usize {
    kinds.min(MAX_GRASS).div_ceil(4).max(1)
}

/// A hash of everything about these kinds that decides where each grows,
/// for knowing when the densities kept were worked out from others. What
/// they look like (mesh, material, size) is left out; the surfaces' own
/// hash goes in, as the kinds grow on them.
pub fn hash_grass(grass: &[Grass], surfaces: u64) -> u64 {
    let mut text = format!("{} {surfaces}", grass.len());
    for kind in grass {
        text.push_str(&format!("\n{:?} {:?} {} {}", kind.cover, kind.surfaces, kind.steepest, kind.paint));
    }
    stable_hash(text.as_bytes()).max(1)
}

/// One tile's densities: each sample's `words` words, row after row, a
/// byte a kind.
#[derive(Clone, Debug, PartialEq)]
pub struct GrassTile {
    samples: Box<[u32]>,
}

impl GrassTile {
    pub fn from_samples(samples: Vec<u32>) -> Self {
        assert_eq!(samples.len() % TILE_LEN, 0, "whole samples");
        Self { samples: samples.into_boxed_slice() }
    }

    pub fn samples(&self) -> &[u32] {
        &self.samples
    }

    /// How thickly kind `kind` grows at sample `(x, z)`, 0 to 255.
    pub fn density(&self, kind: usize, x: u32, z: u32) -> u8 {
        let words = self.samples.len() / TILE_LEN;
        let index = (z * TILE_SAMPLES + x) as usize * words + kind / 4;
        self.samples.get(index).map_or(0, |word| (word >> (8 * (kind % 4))) as u8)
    }
}

/// Every tile's densities, laid out as the terrain's heights are.
#[derive(Clone, Debug)]
pub struct GrassMap {
    tiles_x: u32,
    tiles_z: u32,
    words: usize,
    tiles: Vec<Arc<GrassTile>>,
}

impl PartialEq for GrassMap {
    fn eq(&self, other: &Self) -> bool {
        (self.tiles_x, self.tiles_z, self.words) == (other.tiles_x, other.tiles_z, other.words)
            && self.tiles.iter().zip(&other.tiles).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
    }
}

impl GrassMap {
    pub fn from_tiles(tiles_x: u32, tiles_z: u32, words: usize, tiles: Vec<Arc<GrassTile>>) -> Self {
        assert_eq!(tiles.len(), (tiles_x * tiles_z) as usize, "densities for every tile");
        Self { tiles_x, tiles_z, words, tiles }
    }

    /// How many words a sample takes: 1 for up to four kinds, 2 for eight.
    pub fn words(&self) -> usize {
        self.words
    }

    pub fn tiles(&self) -> &[Arc<GrassTile>] {
        &self.tiles
    }

    pub fn tile(&self, x: u32, z: u32) -> &Arc<GrassTile> {
        &self.tiles[(z * self.tiles_x + x) as usize]
    }

    pub fn fits(&self, heights: &Heightmap) -> bool {
        (self.tiles_x, self.tiles_z) == (heights.tiles_x(), heights.tiles_z())
    }

    /// How thickly kind `kind` grows at sample `(x, z)` of the whole
    /// terrain, each clamped to its edges, 0 to 255.
    pub fn density_at(&self, kind: usize, x: i64, z: i64) -> u8 {
        let (qx, qz) = (i64::from(self.tiles_x * TILE_QUADS), i64::from(self.tiles_z * TILE_QUADS));
        let (x, z) = (x.clamp(0, qx) as u32, z.clamp(0, qz) as u32);
        let tx = (x / TILE_QUADS).min(self.tiles_x - 1);
        let tz = (z / TILE_QUADS).min(self.tiles_z - 1);
        self.tile(tx, tz).density(kind, x - tx * TILE_QUADS, z - tz * TILE_QUADS)
    }
}

/// How thickly kind `index` grows at sample `(x, z)` by its cover alone, 0
/// to 1: everywhere, nowhere, or as much as its surfaces cover there.
fn cover_of(kind: &Grass, weights: Option<&Weights>, x: i64, z: i64) -> f32 {
    match kind.cover {
        GrassCover::Everywhere => 1.0,
        GrassCover::Painted => 0.0,
        GrassCover::Surfaces => {
            let Some(weights) = weights else { return 0.0 };
            let shares = weights.shares_at(x, z);
            let on = shares.iter().filter(|(surface, _)| kind.surfaces.contains(&u32::from(*surface)));
            on.map(|(_, share)| f32::from(*share) / 255.0).sum::<f32>().min(1.0)
        }
    }
}

/// How thickly kind `index` grows at sample `(x, z)`, its painting with it,
/// before the slope takes any away.
fn painted_cover(
    file: &TerrainFile,
    weights: Option<&Weights>,
    rasters: &Rasters,
    index: usize,
    x: i64,
    z: i64,
) -> f32 {
    let kind = &file.grass[index];
    let painted = if kind.paint == 0 { 0.0 } else { rasters.sample(kind.paint, x, z) };
    (cover_of(kind, weights, x, z) + painted).clamp(0.0, 1.0)
}

/// Sample `(x, z)`'s densities, packed.
fn densities_of(
    file: &TerrainFile,
    (heights, weights): (&Heightmap, Option<&Weights>),
    rasters: &Rasters,
    (x, z): (i64, i64),
    out: &mut [u32],
) {
    out.fill(0);
    let (_, rise) = slope_at(heights, x, z);
    let degrees = rise.atan().to_degrees();
    for (index, kind) in file.grass.iter().enumerate().take(MAX_GRASS) {
        let steep = 1.0 - smoothstep(kind.steepest - 5.0, kind.steepest, degrees);
        let density = painted_cover(file, weights, rasters, index, x, z) * steep;
        out[index / 4] |= ((density * 255.0).round() as u32).min(255) << (8 * (index % 4));
    }
}

/// The densities of `file`'s grass over `heights`, with the surfaces'
/// `weights` they grow on: the `dirty` samples worked out again, the rest
/// kept from `previous` when it is laid out as the heights are and has as
/// many words. `None` for a terrain with no grass.
pub fn grow(
    file: &TerrainFile,
    (heights, weights): (&Heightmap, Option<&Weights>),
    rasters: &Rasters,
    previous: Option<&GrassMap>,
    dirty: &Dirty,
) -> Option<GrassMap> {
    if file.grass.is_empty() {
        return None;
    }
    let words = words(file.grass.len());
    let (tiles_x, tiles_z) = (heights.tiles_x(), heights.tiles_z());
    let fits = previous.filter(|previous| previous.fits(heights) && previous.words == words);
    let whole = [0, 0, TILE_QUADS, TILE_QUADS];
    let every = || (0..tiles_x * tiles_z).map(|i| (i % tiles_x, i / tiles_x, whole)).collect();
    let wanted: Vec<(u32, u32, [u32; 4])> = match (dirty, fits) {
        (Dirty::All, _) | (_, None) => every(),
        (Dirty::Tiles(list), Some(_)) => {
            list.iter().filter(|&&(x, z)| x < tiles_x && z < tiles_z).map(|&(x, z)| (x, z, whole)).collect()
        }
        (Dirty::Areas(areas), Some(_)) => (0..tiles_x * tiles_z)
            .filter_map(|i| {
                let (tx, tz) = (i % tiles_x, i / tiles_x);
                crate::surfaces::samples_under(areas, heights.spacing(), tx, tz).map(|rect| (tx, tz, rect))
            })
            .collect(),
    };
    let made = Mutex::new(Vec::with_capacity(wanted.len()));
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(wanted.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(tx, tz, [x0, z0, x1, z1])) = wanted.get(at) else { break };
                    let mut samples = match fits {
                        Some(previous) => previous.tile(tx, tz).samples().to_vec(),
                        None => vec![0; TILE_LEN * words],
                    };
                    let (ox, oz) = (i64::from(tx * TILE_QUADS), i64::from(tz * TILE_QUADS));
                    for z in z0..=z1 {
                        for x in x0..=x1 {
                            let index = (z * TILE_SAMPLES + x) as usize * words;
                            let sample = (ox + i64::from(x), oz + i64::from(z));
                            densities_of(file, (heights, weights), rasters, sample, &mut samples[index..index + words]);
                        }
                    }
                    made.lock().expect("never poisoned").push((tx, tz, GrassTile::from_samples(samples)));
                }
            });
        }
    });
    let mut tiles: Vec<Option<Arc<GrassTile>>> = match fits {
        Some(previous) => previous.tiles.iter().cloned().map(Some).collect(),
        None => vec![None; (tiles_x * tiles_z) as usize],
    };
    for (tx, tz, tile) in made.into_inner().expect("never poisoned") {
        tiles[(tz * tiles_x + tx) as usize] = Some(Arc::new(tile));
    }
    let tiles = tiles.into_iter().map(|tile| tile.expect("every tile made or kept")).collect();
    Some(GrassMap { tiles_x, tiles_z, words, tiles })
}

/// Gives kind `kind` a raster to paint into, if it has none; true if it was
/// given one.
pub fn ready_to_paint(file: &mut TerrainFile, rasters: &Rasters, kind: usize) -> bool {
    match file.grass.get(kind) {
        Some(grass) if grass.paint == 0 => {
            file.grass[kind].paint = rasters.next_raster(file.rasters_in_use());
            true
        }
        _ => false,
    }
}

/// How fast painting brings the density to where it is going at full
/// strength: the share of the way it goes in a second.
const PAINT_PULL: f32 = 8.0;

/// One dab of grass kind `kind`, of `brush`'s size and strength, at `at`
/// (metres from the corner), held for `amount` seconds: growing it there,
/// or with `erase` (Shift) digging it up. The kind must be ready to paint
/// ([`ready_to_paint`]). Returns the area it changed.
pub fn paint(
    file: &TerrainFile,
    weights: Option<&Weights>,
    rasters: &mut Rasters,
    kind: usize,
    (brush, at, amount): (&Brush, [f32; 2], f32),
    erase: bool,
) -> Area {
    let spacing = file.spacing();
    let radius = brush.radius.max(spacing);
    let quads = i64::from(file.tiles() * TILE_QUADS);
    let first = |v: f32| (((v - radius) / spacing).floor() as i64).clamp(0, quads);
    let last = |v: f32| (((v + radius) / spacing).ceil() as i64).clamp(0, quads);
    let (x0, x1, z0, z1) = (first(at[0]), last(at[0]), first(at[1]), last(at[1]));
    let area = [x0 as f32 * spacing, z0 as f32 * spacing, x1 as f32 * spacing, z1 as f32 * spacing];
    let Some(raster) = file.grass.get(kind).map(|grass| grass.paint).filter(|&raster| raster != 0) else {
        return area;
    };
    let goal = if erase { 0.0 } else { 1.0 };
    let strength = brush.strength.clamp(0.0, 1.0);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let distance = ((x as f32 * spacing - at[0]).powi(2) + (z as f32 * spacing - at[1]).powi(2)).sqrt();
            let falloff = 1.0 - smoothstep(0.0, 1.0, distance / radius);
            if falloff <= 0.0 {
                continue;
            }
            let pull = (strength * falloff * amount * PAINT_PULL).min(1.0);
            let change = (goal - painted_cover(file, weights, rasters, kind, x, z)) * pull;
            if change != 0.0 {
                add(rasters, raster, x, z, change);
            }
        }
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::{Cover, Surface, weigh};

    fn field(grass: Vec<Grass>, surfaces: Vec<Surface>) -> (TerrainFile, Heightmap) {
        let file = TerrainFile { size: 256.0, resolution: 257, grass, surfaces, ..TerrainFile::default() };
        // Level for 100 m, then a wall rising 1 m a metre.
        (file, Heightmap::from_fn(1, 1, 1.0, |x, _| (x - 100.0).max(0.0)))
    }

    #[test]
    fn grass_grows_everywhere_but_on_steep_ground_and_on_its_surfaces_as_thickly_as_they_cover() {
        let mut mud = Surface::new(None);
        mud.cover = Cover::Rules;
        mud.mask.height = Some([-10.0, 0.5]);
        let mut on_mud = Grass::new(None);
        on_mud.cover = GrassCover::Surfaces;
        on_mud.surfaces = vec![1];
        let mut painted = Grass::new(None);
        painted.cover = GrassCover::Painted;
        let (file, heights) = field(vec![Grass::new(None), on_mud, painted], vec![Surface::new(None), mud]);
        let rasters = Rasters::default();
        let weights = weigh(&file, &heights, &rasters, None, &Dirty::All);
        let map = grow(&file, (&heights, weights.as_ref()), &rasters, None, &Dirty::All).expect("grass");
        assert_eq!(map.words(), 1, "three kinds in one word");
        assert_eq!(map.density_at(0, 50, 50), 255, "everywhere on level ground");
        assert_eq!(map.density_at(0, 200, 50), 0, "none on a 45° wall");
        assert_eq!(map.density_at(1, 50, 50), 255, "on the mud, which covers the level");
        assert_eq!(map.density_at(1, 200, 50), 0);
        assert_eq!(map.density_at(2, 50, 50), 0, "painted only, and not painted");
    }

    #[test]
    fn painting_grows_grass_and_erasing_digs_it_up_where_the_brush_went() {
        let mut painted = Grass::new(None);
        painted.cover = GrassCover::Painted;
        let (mut file, heights) = field(vec![Grass::new(None), painted], Vec::new());
        let mut rasters = Rasters::default();
        assert!(ready_to_paint(&mut file, &rasters, 1) && !ready_to_paint(&mut file, &rasters, 1));
        let brush = Brush { radius: 6.0, strength: 1.0, ..Brush::default() };
        for _ in 0..10 {
            paint(&file, None, &mut rasters, 1, (&brush, [40.0, 40.0], 0.2), false);
        }
        assert!(ready_to_paint(&mut file, &rasters, 0));
        for _ in 0..10 {
            paint(&file, None, &mut rasters, 0, (&brush, [60.0, 40.0], 0.2), true);
        }
        let map = grow(&file, (&heights, None), &rasters, None, &Dirty::All).unwrap();
        assert!(map.density_at(1, 40, 40) > 250 && map.density_at(1, 90, 40) == 0, "grown where painted");
        assert!(map.density_at(0, 60, 40) < 5 && map.density_at(0, 90, 40) == 255, "dug up where erased");
        // Worked out again over an area, the same as worked out whole.
        let before = grow(&file, (&heights, None), &Rasters::default(), None, &Dirty::All).unwrap();
        let again =
            grow(&file, (&heights, None), &rasters, Some(&before), &Dirty::Areas(vec![[30.0, 30.0, 70.0, 50.0]]));
        assert_eq!(again.unwrap().tile(0, 0).samples(), map.tile(0, 0).samples());
    }
}
