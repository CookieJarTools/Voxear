//! What a terrain's ground is covered with: its surfaces, each a material
//! laid on the ground, bottom to top.
//!
//! The first surface is the ground under the rest, wherever the others
//! leave it bare. Each one above covers those below it as far as its cover
//! says at each sample: everywhere, nowhere until painted, or by the
//! ground's height and slope there (its rules), with what was painted added
//! to that or taken from it. So rules can cover a terrain in grass, with
//! rock on its slopes and snow on its peaks, a brush can paint a path
//! through the grass, and a hill raised later is snowy without repainting.
//!
//! Each sample's share of each surface is worked out from those covers
//! ([`weigh`]), and its four largest shares kept for drawing ([`Weights`]):
//! four surfaces' numbers and their shares out of 255, in two 32-bit words
//! a sample.

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
    layers::{Area, Mask, smoothstep, stable_hash},
    noise::{self, NoiseStyle},
    raster::Rasters,
};

/// How many surfaces a terrain can have.
pub const MAX_SURFACES: usize = 32;

/// How many surfaces each sample keeps a share of.
pub const SHARES: usize = 4;

/// One surface: a material laid on the ground.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    /// The material it is drawn with, by its GUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// How many metres its pictures span before they repeat.
    #[serde(default = "four")]
    pub size: f32,
    /// How sharply it meets the surfaces beside it: blended evenly at 0,
    /// and towards 1 the higher of their height pictures standing through.
    #[serde(default = "half")]
    pub sharpness: f32,
    /// Drawn from the side as well as from above, where the ground is too
    /// steep for its pictures to lie on it unstretched.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub triplanar: bool,
    /// Where it covers the surfaces below it, before any painting.
    #[serde(default)]
    pub cover: Cover,
    /// With rules for its cover: between which heights and slopes.
    #[serde(default, skip_serializing_if = "Mask::is_none")]
    pub mask: Mask,
    /// How far the rules' edges wander, in metres for heights and degrees
    /// for slopes, so they do not run straight along the ground's lines.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub breakup: f32,
    /// How large the wanderings are, in metres.
    #[serde(default = "eight", skip_serializing_if = "is_eight")]
    pub breakup_size: f32,
    /// The raster of its painting, which adds to its cover; 0 before it is
    /// painted.
    #[serde(default, skip_serializing_if = "is_nothing")]
    pub paint: u32,
    /// How far Track Makers press into it, as a share of their depth: 1
    /// for fresh snow, a half for mud, nothing for rock.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tracks: f32,
    /// How many seconds its tracks take to fill in again, as snow falling
    /// does; 0 keeps them.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fills_in: f32,
}

fn four() -> f32 {
    4.0
}

fn half() -> f32 {
    0.5
}

fn eight() -> f32 {
    8.0
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_eight(value: &f32) -> bool {
    *value == 8.0
}

fn is_nothing(value: &u32) -> bool {
    *value == 0
}

/// Where a surface covers those below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cover {
    /// Only where it is painted.
    #[default]
    Painted,
    /// Everywhere, but where painting takes it away.
    Everywhere,
    /// Where the ground's height and slope are within its mask's ranges,
    /// and where it is painted.
    Rules,
}

impl Cover {
    pub const ALL: [Cover; 3] = [Cover::Painted, Cover::Everywhere, Cover::Rules];

    pub fn name(self) -> &'static str {
        match self {
            Cover::Painted => "Painted",
            Cover::Everywhere => "Everywhere",
            Cover::Rules => "Height and Slope",
        }
    }
}

impl Surface {
    /// A surface of `material`, covering only where it is painted.
    pub fn new(material: Option<String>) -> Self {
        Self {
            material,
            size: four(),
            sharpness: half(),
            triplanar: false,
            cover: Cover::Painted,
            mask: Mask::default(),
            breakup: 0.0,
            breakup_size: eight(),
            paint: 0,
            tracks: 0.0,
            fills_in: 0.0,
        }
    }

    /// How much it covers the surfaces below it, by its cover alone, on
    /// ground `height` metres high rising `rise` metres a metre, at `(x, z)`
    /// metres from the corner; `index` is its place in the list, which
    /// seeds its wandering.
    pub fn rule(&self, index: usize, height: f32, rise: f32, x: f32, z: f32) -> f32 {
        match self.cover {
            Cover::Painted => 0.0,
            Cover::Everywhere => 1.0,
            Cover::Rules => {
                let wander = if self.breakup > 0.0 {
                    let size = self.breakup_size.max(0.1);
                    let seed = 0x5EED_0000 ^ index as u32;
                    (noise::fractal(NoiseStyle::Fractal, x / size, z / size, 3, seed) * 2.0 - 1.0) * self.breakup
                } else {
                    0.0
                };
                self.mask.weight_by(height + wander, || rise.atan().to_degrees() + wander)
            }
        }
    }
}

/// A hash of everything about these surfaces that decides where each
/// covers, for knowing when the shares kept were worked out from others.
/// What they look like (material, size, sharpness) is left out.
pub fn hash_surfaces(surfaces: &[Surface]) -> u64 {
    let mut text = format!("{}", surfaces.len());
    for surface in surfaces {
        let toml = toml::to_string(&surface.mask).unwrap_or_default();
        text.push_str(&format!(
            "\n{:?} {} {} {} {toml}",
            surface.cover, surface.breakup, surface.breakup_size, surface.paint
        ));
    }
    // Never 0, which a file writes for none kept.
    stable_hash(text.as_bytes()).max(1)
}

/// One tile's shares: for each sample, row after row, the numbers of its
/// four surfaces with the largest shares, a byte each, then their shares,
/// out of 255, a byte each.
#[derive(Clone, Debug, PartialEq)]
pub struct WeightTile {
    samples: Box<[[u32; 2]]>,
}

impl WeightTile {
    /// A tile all of surface `surface`.
    pub fn all(surface: u8) -> Self {
        Self::from_samples(vec![[u32::from(surface), 255]; TILE_LEN])
    }

    pub fn from_samples(samples: Vec<[u32; 2]>) -> Self {
        assert_eq!(samples.len(), TILE_LEN, "a tile has {TILE_LEN} samples");
        Self { samples: samples.into_boxed_slice() }
    }

    pub fn samples(&self) -> &[[u32; 2]] {
        &self.samples
    }

    /// Sample `(x, z)`'s surfaces and their shares out of 255, largest
    /// first.
    pub fn shares(&self, x: u32, z: u32) -> [(u8, u8); SHARES] {
        let [surfaces, shares] = self.samples[(z * TILE_SAMPLES + x) as usize];
        std::array::from_fn(|i| ((surfaces >> (8 * i)) as u8, (shares >> (8 * i)) as u8))
    }
}

/// Every tile's shares, laid out as the terrain's heights are.
#[derive(Clone, Debug)]
pub struct Weights {
    tiles_x: u32,
    tiles_z: u32,
    tiles: Vec<Arc<WeightTile>>,
}

impl PartialEq for Weights {
    fn eq(&self, other: &Self) -> bool {
        (self.tiles_x, self.tiles_z) == (other.tiles_x, other.tiles_z)
            && self.tiles.iter().zip(&other.tiles).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
    }
}

impl Weights {
    pub fn from_tiles(tiles_x: u32, tiles_z: u32, tiles: Vec<Arc<WeightTile>>) -> Self {
        assert_eq!(tiles.len(), (tiles_x * tiles_z) as usize, "shares for every tile");
        Self { tiles_x, tiles_z, tiles }
    }

    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }

    pub fn tiles_z(&self) -> u32 {
        self.tiles_z
    }

    /// Every tile, row after row along z.
    pub fn tiles(&self) -> &[Arc<WeightTile>] {
        &self.tiles
    }

    pub fn tile(&self, x: u32, z: u32) -> &Arc<WeightTile> {
        &self.tiles[(z * self.tiles_x + x) as usize]
    }

    /// Whether they are laid out as `heights` are.
    pub fn fits(&self, heights: &Heightmap) -> bool {
        (self.tiles_x, self.tiles_z) == (heights.tiles_x(), heights.tiles_z())
    }

    /// The shares at sample `(x, z)` of the whole terrain, each clamped to
    /// its edges, largest first.
    pub fn shares_at(&self, x: i64, z: i64) -> [(u8, u8); SHARES] {
        let (qx, qz) = (i64::from(self.tiles_x * TILE_QUADS), i64::from(self.tiles_z * TILE_QUADS));
        let (x, z) = (x.clamp(0, qx) as u32, z.clamp(0, qz) as u32);
        let tx = (x / TILE_QUADS).min(self.tiles_x - 1);
        let tz = (z / TILE_QUADS).min(self.tiles_z - 1);
        self.tile(tx, tz).shares(x - tx * TILE_QUADS, z - tz * TILE_QUADS)
    }

    /// The surface with the largest share at the sample nearest `(x, z)`,
    /// in metres from the corner of ground `spacing` metres between samples:
    /// for which sound a footstep makes.
    pub fn surface_at(&self, spacing: f32, x: f32, z: f32) -> usize {
        let at = |v: f32| (v / spacing).round() as i64;
        usize::from(self.shares_at(at(x), at(z))[0].0)
    }
}

/// The ground's height there and how steeply it rises, in metres a metre.
pub(crate) fn slope_at(heights: &Heightmap, x: i64, z: i64) -> (f32, f32) {
    let dx = heights.sample(x + 1, z) - heights.sample(x - 1, z);
    let dz = heights.sample(x, z + 1) - heights.sample(x, z - 1);
    (heights.sample(x, z), (dx * dx + dz * dz).sqrt() / (2.0 * heights.spacing()))
}

/// How much surface `index` covers those below it at sample `(x, z)`: its
/// rule's cover with its painting added.
fn cover_at(file: &TerrainFile, rasters: &Rasters, index: usize, x: i64, z: i64, ground: (f32, f32)) -> f32 {
    let surface = &file.surfaces[index];
    let spacing = file.spacing();
    let rule = surface.rule(index, ground.0, ground.1, x as f32 * spacing, z as f32 * spacing);
    let painted = if surface.paint == 0 { 0.0 } else { rasters.sample(surface.paint, x, z) };
    (rule + painted).clamp(0.0, 1.0)
}

/// The shares of sample `(x, z)` of the whole terrain, packed.
fn shares_of(file: &TerrainFile, heights: &Heightmap, rasters: &Rasters, x: i64, z: i64) -> [u32; 2] {
    let ground = slope_at(heights, x, z);
    let count = file.surfaces.len().min(MAX_SURFACES);
    // From the top down: each surface takes its cover of what those above left, the first all that is left.
    let mut best = [(0u32, 0.0f32); SHARES];
    let mut left = 1.0f32;
    for index in (0..count).rev() {
        let cover = if index == 0 { 1.0 } else { cover_at(file, rasters, index, x, z, ground) };
        let share = left * cover;
        left -= share;
        if share > best[SHARES - 1].1 {
            best[SHARES - 1] = (index as u32, share);
            best.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        if left <= 0.0 {
            break;
        }
    }
    pack(&best)
}

/// Four surfaces' shares, largest first, as out of 255 together.
pub(crate) fn pack(best: &[(u32, f32); SHARES]) -> [u32; 2] {
    let total: f32 = best.iter().map(|(_, share)| share).sum();
    if total <= 0.0 {
        return [best[0].0, 255];
    }
    let mut bytes = best.map(|(_, share)| (share / total * 255.0).round() as i32);
    // Rounded to make exactly 255, the largest taking up what rounding left.
    bytes[0] += 255 - bytes.iter().sum::<i32>();
    let mut surfaces = 0u32;
    let mut shares = 0u32;
    for (i, ((surface, _), byte)) in best.iter().zip(bytes).enumerate() {
        surfaces |= (surface & 0xFF) << (8 * i);
        shares |= (byte.clamp(0, 255) as u32) << (8 * i);
    }
    [surfaces, shares]
}

/// The samples of tile `(tx, tz)` (from 0 to `TILE_QUADS` each way, `[x0,
/// z0, x1, z1]`) any of `areas` reach, one more each way for the slope, or
/// `None` where none do.
pub(crate) fn samples_under(areas: &[Area], spacing: f32, tx: u32, tz: u32) -> Option<[u32; 4]> {
    let (ox, oz) = (i64::from(tx * TILE_QUADS), i64::from(tz * TILE_QUADS));
    let quads = i64::from(TILE_QUADS);
    let mut found: Option<[i64; 4]> = None;
    for &[x0, z0, x1, z1] in areas {
        let first = |v: f32, o: i64| ((v / spacing).floor() as i64 - 1 - o).max(0);
        let last = |v: f32, o: i64| ((v / spacing).ceil() as i64 + 1 - o).min(quads);
        let rect = [first(x0, ox), first(z0, oz), last(x1, ox), last(z1, oz)];
        if rect[0] > rect[2] || rect[1] > rect[3] {
            continue;
        }
        found =
            Some(found.map_or(rect, |[a, b, c, d]| [a.min(rect[0]), b.min(rect[1]), c.max(rect[2]), d.max(rect[3])]));
    }
    found.map(|rect| rect.map(|v| v as u32))
}

/// The shares of `file`'s surfaces over `heights`: the `dirty` samples
/// worked out again, the rest kept from `previous` when it is laid out as
/// the heights are. `None` for a terrain with no surfaces.
pub fn weigh(
    file: &TerrainFile,
    heights: &Heightmap,
    rasters: &Rasters,
    previous: Option<&Weights>,
    dirty: &Dirty,
) -> Option<Weights> {
    if file.surfaces.is_empty() {
        return None;
    }
    let (tiles_x, tiles_z) = (heights.tiles_x(), heights.tiles_z());
    let fits = previous.filter(|previous| previous.fits(heights));
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
                samples_under(areas, heights.spacing(), tx, tz).map(|rect| (tx, tz, rect))
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
                        None => vec![[0, 255]; TILE_LEN],
                    };
                    let (ox, oz) = (i64::from(tx * TILE_QUADS), i64::from(tz * TILE_QUADS));
                    for z in z0..=z1 {
                        for x in x0..=x1 {
                            samples[(z * TILE_SAMPLES + x) as usize] =
                                shares_of(file, heights, rasters, ox + i64::from(x), oz + i64::from(z));
                        }
                    }
                    made.lock().expect("never poisoned").push((tx, tz, WeightTile::from_samples(samples)));
                }
            });
        }
    });
    let mut tiles: Vec<Option<Arc<WeightTile>>> = match fits {
        Some(previous) => previous.tiles.iter().cloned().map(Some).collect(),
        None => vec![None; (tiles_x * tiles_z) as usize],
    };
    for (tx, tz, tile) in made.into_inner().expect("never poisoned") {
        tiles[(tz * tiles_x + tx) as usize] = Some(Arc::new(tile));
    }
    let tiles = tiles.into_iter().map(|tile| tile.expect("every tile made or kept")).collect();
    Some(Weights { tiles_x, tiles_z, tiles })
}

/// Gives surface `surface` and every surface above it a raster to paint
/// into, those that have none; true if any was given one.
pub fn ready_to_paint(file: &mut TerrainFile, rasters: &Rasters, surface: usize) -> bool {
    let mut changed = false;
    // The first surface is the ground under the rest, never painted into.
    for index in surface.max(1)..file.surfaces.len() {
        if file.surfaces[index].paint == 0 {
            file.surfaces[index].paint = rasters.next_raster(file.rasters_in_use());
            changed = true;
        }
    }
    changed
}

/// How fast painting brings the cover to where it is going at full
/// strength: the share of the way it goes in a second.
const PAINT_PULL: f32 = 8.0;

/// One dab of paint for surface `surface`, of `brush`'s size and strength,
/// at `at` (metres from the corner), held for `amount` seconds: laying the
/// surface over the others there, or with `erase` (Shift) taking it away.
/// Surface `surface` and those above must be ready to paint
/// ([`ready_to_paint`]). Returns the area it changed.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    file: &TerrainFile,
    heights: &Heightmap,
    rasters: &mut Rasters,
    surface: usize,
    brush: &Brush,
    at: [f32; 2],
    amount: f32,
    erase: bool,
) -> Area {
    let spacing = file.spacing();
    let radius = brush.radius.max(spacing);
    let quads = i64::from(file.tiles() * TILE_QUADS);
    let first = |v: f32| (((v - radius) / spacing).floor() as i64).clamp(0, quads);
    let last = |v: f32| (((v + radius) / spacing).ceil() as i64).clamp(0, quads);
    let (x0, x1, z0, z1) = (first(at[0]), last(at[0]), first(at[1]), last(at[1]));
    let area = [x0 as f32 * spacing, z0 as f32 * spacing, x1 as f32 * spacing, z1 as f32 * spacing];
    let count = file.surfaces.len();
    if surface >= count || (erase && surface == 0) {
        return area;
    }
    let strength = brush.strength.clamp(0.0, 1.0);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let distance = ((x as f32 * spacing - at[0]).powi(2) + (z as f32 * spacing - at[1]).powi(2)).sqrt();
            let falloff = 1.0 - smoothstep(0.0, 1.0, distance / radius);
            if falloff <= 0.0 {
                continue;
            }
            let pull = (strength * falloff * amount * PAINT_PULL).min(1.0);
            let ground = slope_at(heights, x, z);
            // The surface brought towards covering all, or nothing; those above it towards nothing, so it shows.
            let towards = |rasters: &mut Rasters, index: usize, goal: f32| {
                let raster = file.surfaces[index].paint;
                if raster == 0 {
                    return;
                }
                let change = (goal - cover_at(file, rasters, index, x, z, ground)) * pull;
                if change != 0.0 {
                    add(rasters, raster, x, z, change);
                }
            };
            if erase {
                towards(rasters, surface, 0.0);
                continue;
            }
            if surface > 0 {
                towards(rasters, surface, 1.0);
            }
            for above in surface + 1..count {
                towards(rasters, above, 0.0);
            }
        }
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terrain 256 m across rising 1 m every 10 m along x, with
    /// `surfaces`.
    fn sloped(surfaces: Vec<Surface>) -> (TerrainFile, Heightmap) {
        let file = TerrainFile { size: 256.0, resolution: 257, surfaces, ..TerrainFile::default() };
        let heights = Heightmap::from_fn(1, 1, 1.0, |x, _| x * 0.1);
        (file, heights)
    }

    fn total(shares: [(u8, u8); SHARES]) -> u32 {
        shares.iter().map(|(_, share)| u32::from(*share)).sum()
    }

    #[test]
    fn the_first_surface_covers_what_the_rest_leave_and_shares_make_255() {
        let mut high = Surface::new(None);
        high.cover = Cover::Rules;
        high.mask = Mask { height: Some([10.0, 100.0]), slope: None, feather: 4.0 };
        let (file, heights) = sloped(vec![Surface::new(None), high]);
        let weights = weigh(&file, &heights, &Rasters::default(), None, &Dirty::All).expect("surfaces");
        let tile = weights.tile(0, 0);
        assert_eq!(tile.shares(20, 5)[0], (0, 255), "2 m up: the ground's surface alone");
        assert_eq!(tile.shares(200, 5)[0], (1, 255), "20 m up: the high one");
        // Eased in over the feather, from 6 m to 10 m up: halfway at 8 m.
        let between = tile.shares(80, 5);
        assert_eq!(total(between), 255);
        let high_share = between.iter().find(|(s, _)| *s == 1).map(|(_, w)| *w).unwrap();
        assert!((126..=129).contains(&high_share), "{between:?}");
        for x in (0..=256).step_by(7) {
            assert_eq!(total(tile.shares(x, 100)), 255);
        }
        assert_eq!(weights.surface_at(1.0, 200.0, 5.0), 1);
    }

    #[test]
    fn painting_lays_a_surface_over_those_above_it_and_erasing_takes_it_away() {
        let mut snow = Surface::new(None);
        snow.cover = Cover::Everywhere;
        let (mut file, heights) = sloped(vec![Surface::new(None), Surface::new(None), snow]);
        let mut rasters = Rasters::default();
        assert!(ready_to_paint(&mut file, &rasters, 1));
        assert!(file.surfaces[1].paint != 0 && file.surfaces[2].paint != file.surfaces[1].paint);
        assert_eq!(file.surfaces[0].paint, 0, "the first surface is never painted into");
        assert!(!ready_to_paint(&mut file, &rasters, 1), "ready already");
        let brush = Brush { radius: 6.0, strength: 1.0, ..Brush::default() };
        for _ in 0..10 {
            paint(&file, &heights, &mut rasters, 1, &brush, [100.0, 100.0], 0.2, false);
        }
        let weights = weigh(&file, &heights, &rasters, None, &Dirty::All).unwrap();
        assert_eq!(weights.shares_at(100, 100)[0].0, 1, "painted over the snow");
        assert_eq!(weights.shares_at(130, 100)[0], (2, 255), "the snow beyond the brush");
        // The ground's own surface painted back over both, then the snow taken away there.
        let before = weights.clone();
        ready_to_paint(&mut file, &rasters, 0);
        for _ in 0..10 {
            paint(&file, &heights, &mut rasters, 0, &brush, [100.0, 100.0], 0.2, false);
        }
        let weights = weigh(&file, &heights, &rasters, Some(&before), &Dirty::Areas(vec![[90.0, 90.0, 110.0, 110.0]]));
        let weights = weights.unwrap();
        assert_eq!(weights.shares_at(100, 100)[0].0, 0);
        assert!(!Arc::ptr_eq(weights.tile(0, 0), before.tile(0, 0)));
        for _ in 0..10 {
            paint(&file, &heights, &mut rasters, 2, &brush, [40.0, 40.0], 0.2, true);
        }
        let weights = weigh(&file, &heights, &rasters, None, &Dirty::All).unwrap();
        assert_eq!(weights.shares_at(40, 40)[0].0, 0, "snow erased to the ground below");
    }

    #[test]
    fn worked_out_again_over_an_area_matches_worked_out_whole() {
        let mut steep = Surface::new(None);
        steep.cover = Cover::Rules;
        steep.mask = Mask { height: None, slope: Some([3.0, 90.0]), feather: 1.0 };
        steep.breakup = 2.0;
        let (file, heights) = sloped(vec![Surface::new(None), steep]);
        let whole = weigh(&file, &heights, &Rasters::default(), None, &Dirty::All).unwrap();
        // Steeper ground in one place, and only there worked out again.
        let steeper =
            Heightmap::from_fn(1, 1, 1.0, |x, z| x * 0.1 + if (60.0..70.0).contains(&z) { x * 0.2 } else { 0.0 });
        let area = Dirty::Areas(vec![[0.0, 59.0, 256.0, 71.0]]);
        let part = weigh(&file, &steeper, &Rasters::default(), Some(&whole), &area).unwrap();
        let fresh = weigh(&file, &steeper, &Rasters::default(), None, &Dirty::All).unwrap();
        assert_eq!(part.tile(0, 0).samples(), fresh.tile(0, 0).samples());
        assert_ne!(part.tile(0, 0).samples(), whole.tile(0, 0).samples());
    }
}
