//! Digging into a terrain: trenches, craters with overhanging lips, dugouts
//! and tunnels, which heights alone cannot make, as ground at one place
//! has one height.
//!
//! Where a terrain is dug, the ground is voxels instead: chunks
//! [`CELLS`] voxels a side, two voxels to each of the heights' samples, so
//! a quarter of a metre on ground of half-metre samples. A chunk starts as
//! the heights it replaces, each corner as far into the ground as the
//! ground there is above it (`height − y`, more than nothing in the
//! ground), and digging makes corners air where a shape takes the ground
//! away ([`Excavation::carve`]). Every chunk in a column of them is kept
//! once one is dug, and the heights' samples inside the column are holes,
//! so the heights draw and collide round it and the voxels in it.
//!
//! The voxels' surface is made of triangles by marching tetrahedra
//! ([`mesh`]): each voxel split into six tetrahedra along its diagonal, the
//! same way in every voxel, so neighbours share their faces' triangles and
//! the surface has no gaps. On a column's outside faces the ground's
//! density is linear both ways, so every point the surface crosses them at
//! lies on the heights' own edges there, and the voxels meet the heights
//! with no crack.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use voxear_core::math::Vec3;

use crate::{
    grass::{GrassMap, GrassTile},
    heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_QUADS, TILE_SAMPLES},
    layers::Area,
    ray::GroundHit,
};

/// How many voxels a chunk has a side.
pub const CELLS: usize = 32;
/// How many corners a chunk has a side: its voxels' and one more.
pub const CORNERS: usize = CELLS + 1;
/// How many of the heights' samples a chunk spans a side: two voxels a
/// sample.
pub const CHUNK_SAMPLES: u32 = (CELLS / 2) as u32;

/// A chunk's place in a terrain, in chunks from its corner along x and z
/// and from its height of nothing up y. In order by x, then z, then y, so a
/// column's chunks come together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkKey {
    pub x: i32,
    pub z: i32,
    pub y: i32,
}

/// One chunk of dug ground: for each of its corners, how far into the
/// ground it is, in metres (less than nothing in the air), and for one
/// digging opened, the surface it laid on what it bared: 0 for the
/// terrain's own, or a surface's number and one. Corners run along x, then
/// z, then y.
#[derive(Clone, Debug, PartialEq)]
pub struct DugChunk {
    pub density: Box<[f32]>,
    pub dug: Box<[u8]>,
}

fn corner(x: usize, y: usize, z: usize) -> usize {
    (y * CORNERS + z) * CORNERS + x
}

/// A terrain's dug ground: every chunk of every dug column, in order, and a
/// hash of the layers that dug it (see [`redig`]), which is no part of the
/// ground.
#[derive(Clone, Debug, Default)]
pub struct Excavation {
    chunks: BTreeMap<ChunkKey, Arc<DugChunk>>,
    pub dug_by: u64,
}

impl PartialEq for Excavation {
    fn eq(&self, other: &Self) -> bool {
        self.chunks.len() == other.chunks.len()
            && self.chunks.iter().zip(&other.chunks).all(|((ka, a), (kb, b))| ka == kb && (Arc::ptr_eq(a, b) || a == b))
    }
}

/// A shape that digs or fills: a ball, a capsule from one point to
/// another, or a length of trench from one point on the ground to another,
/// in metres from the terrain's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Ball { at: [f32; 3], radius: f32 },
    Capsule { from: [f32; 3], to: [f32; 3], radius: f32 },
    Trench { from: [f32; 3], to: [f32; 3], floor: f32, profile: Trench },
}

/// A trench's shape across it, below the line it is dug along: its floor
/// `depth` metres down, its walls leaning back `batter` metres for each
/// metre up, a fire step along its front wall `step_height` metres above
/// the floor and `step_width` metres deep (none at 0), and a parapet of the
/// earth dug out heaped along its front, `parapet` metres high (none at 0).
/// Its front is on the left of the line going from its first point, or the
/// right; the surface `surface` (its number) is laid on what it bares.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Trench {
    pub depth: f32,
    pub batter: f32,
    pub step_height: f32,
    pub step_width: f32,
    pub parapet: f32,
    pub front_left: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<u8>,
}

impl Default for Trench {
    fn default() -> Self {
        Self {
            depth: 2.0,
            batter: 0.1,
            step_height: 0.6,
            step_width: 0.5,
            parapet: 0.5,
            front_left: true,
            surface: None,
        }
    }
}

/// One stroke of the Dig, Fill or Smooth brush: a ball `[x, y, z]` in
/// metres from the terrain's corner, `radius` round, filling, smoothing
/// `smooth` (0 to 1) of the way, or digging, and for a dig the surface laid
/// on what it bares (its number, or none).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DigStroke {
    pub at: [f32; 3],
    pub radius: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill: bool,
    #[serde(default, skip_serializing_if = "is_nothing")]
    pub smooth: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<u8>,
}

fn is_nothing(value: &f32) -> bool {
    *value == 0.0
}

impl DigStroke {
    /// How it carves the ground.
    pub fn carve(&self) -> Carve {
        match (self.smooth > 0.0, self.fill) {
            (true, _) => Carve::Smooth { amount: self.smooth },
            (false, true) => Carve::Fill,
            (false, false) => Carve::Dig,
        }
    }
}

/// How far `p` is outside a convex polygon `corners`, wound
/// counter-clockwise; less than nothing inside.
fn polygon_distance(p: [f32; 2], corners: &[[f32; 2]]) -> f32 {
    let mut nearest = f32::MAX;
    let mut inside = true;
    for (i, a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % corners.len()];
        let (edge, to) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
        let length2 = edge[0] * edge[0] + edge[1] * edge[1];
        let t = if length2 > 0.0 { ((to[0] * edge[0] + to[1] * edge[1]) / length2).clamp(0.0, 1.0) } else { 0.0 };
        let (dx, dy) = (to[0] - edge[0] * t, to[1] - edge[1] * t);
        nearest = nearest.min((dx * dx + dy * dy).sqrt());
        // Counter-clockwise: inside is to the left of every edge.
        inside &= edge[0] * to[1] - edge[1] * to[0] >= 0.0;
    }
    if inside { -nearest } else { nearest }
}

impl Trench {
    /// How far `(across, up)` is outside the ground it digs away, across
    /// from the line towards the front and up from it.
    fn distance(&self, across: f32, up: f32, floor: f32) -> f32 {
        // Its cut, open well above the line: full width above the fire step, narrowed on the front below it.
        let (half, top) = (floor * 0.5, 6.0);
        let lean = |v: f32| self.batter * (v + self.depth);
        let step = -self.depth + self.step_height.min(self.depth);
        let back = |v: f32| -(half + lean(v));
        let upper = [[back(step), step], [half + lean(step), step], [half + lean(top), top], [back(top), top]];
        let narrow = (half - self.step_width).max(-half * 0.5);
        let floor_at = -self.depth;
        let lower = [
            [back(floor_at), floor_at],
            [narrow + lean(floor_at), floor_at],
            [narrow + lean(step), step],
            [back(step), step],
        ];
        let cut = polygon_distance([across, up], &upper);
        if self.step_width > 0.0 && self.step_height > 0.0 {
            cut.min(polygon_distance([across, up], &lower))
        } else {
            let whole = [
                [back(floor_at), floor_at],
                [half + lean(floor_at), floor_at],
                [half + lean(top), top],
                [back(top), top],
            ];
            polygon_distance([across, up], &whole)
        }
    }
}

impl Shape {
    /// The pieces a trench along `points` (in metres from the terrain's
    /// corner) is dug as: a length of trench every two metres or less along
    /// it, each from the ground to the ground, so it follows the ground
    /// between its points; and first, where it has one, its parapet, heaped
    /// along its front as a filled capsule.
    pub fn trench(heights: &Heightmap, points: &[[f32; 3]], floor: f32, profile: Trench) -> Vec<(Shape, Carve)> {
        let mut along: Vec<[f32; 3]> = Vec::new();
        for pair in points.windows(2) {
            let (a, b) = (Vec3::from(pair[0]), Vec3::from(pair[1]));
            let pieces = ((b - a).length() / 2.0).ceil().max(1.0) as usize;
            for i in 0..=pieces {
                if i == 0 && !along.is_empty() {
                    continue;
                }
                let p = a + (b - a) * (i as f32 / pieces as f32);
                along.push([p.x, heights.height_at(p.x, p.z), p.z]);
            }
        }
        let mut shapes = Vec::new();
        if profile.parapet > 0.0 {
            for pair in along.windows(2) {
                let (a, b) = (Vec3::from(pair[0]), Vec3::from(pair[1]));
                let across = across_of(a, b) * if profile.front_left { 1.0 } else { -1.0 };
                let out = floor * 0.5 + profile.batter * profile.depth + profile.parapet * 1.2;
                let radius = profile.parapet * 1.4;
                let lift = Vec3::new(0.0, profile.parapet - radius, 0.0);
                let (from, to) = (a + across * out + lift, b + across * out + lift);
                shapes.push((Shape::Capsule { from: from.to_array(), to: to.to_array(), radius }, Carve::Fill));
            }
        }
        for pair in along.windows(2) {
            shapes.push((Shape::Trench { from: pair[0], to: pair[1], floor, profile }, Carve::Dig));
        }
        shapes
    }

    /// How far `p` is outside it, in metres; less than nothing inside.
    pub fn distance(&self, p: Vec3) -> f32 {
        match *self {
            Shape::Ball { at, radius } => p.distance(Vec3::from(at)) - radius,
            Shape::Capsule { from, to, radius } => {
                let (a, b) = (Vec3::from(from), Vec3::from(to));
                let along = b - a;
                let t = if along.length_squared() > 1e-12 {
                    ((p - a).dot(along) / along.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                p.distance(a + along * t) - radius
            }
            Shape::Trench { from, to, floor, profile } => {
                // Across and up from the line where `p` is beside it; past its ends, rounded off.
                let (a, b) = (Vec3::from(from), Vec3::from(to));
                let flat = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
                let length = flat.length();
                let (direction, t) = match length > 1e-6 {
                    true => (flat / length, (p - a).dot(flat / length) / length),
                    false => (Vec3::X, 0.0),
                };
                let base = a + (b - a) * t.clamp(0.0, 1.0);
                let across = across_of(a, a + direction) * if profile.front_left { 1.0 } else { -1.0 };
                let section = profile.distance((p - base).dot(across), p.y - base.y, floor);
                let past = (t.max(1.0) - 1.0 - t.min(0.0)) * length;
                if past <= 0.0 {
                    section
                } else if section > 0.0 {
                    (section * section + past * past).sqrt()
                } else {
                    past.max(section)
                }
            }
        }
    }

    /// Its lowest and highest corners.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        match *self {
            Shape::Ball { at, radius } => (Vec3::from(at) - radius, Vec3::from(at) + radius),
            Shape::Capsule { from, to, radius } => {
                let (a, b) = (Vec3::from(from), Vec3::from(to));
                (a.min(b) - radius, a.max(b) + radius)
            }
            Shape::Trench { from, to, floor, profile } => {
                let (a, b) = (Vec3::from(from), Vec3::from(to));
                let wide = floor * 0.5 + profile.batter * (profile.depth + 6.0);
                (a.min(b) - Vec3::new(wide, profile.depth, wide), a.max(b) + Vec3::new(wide, 6.0, wide))
            }
        }
    }

    /// Whether every number is one and it has some size, as one from
    /// another machine may not.
    pub fn is_sound(&self) -> bool {
        let numbers: Vec<f32> = match *self {
            Shape::Ball { at, radius } => vec![at[0], at[1], at[2], radius],
            Shape::Capsule { from, to, radius } => vec![from[0], from[1], from[2], to[0], to[1], to[2], radius],
            Shape::Trench { from, to, floor, profile } => {
                vec![
                    from[0],
                    from[1],
                    from[2],
                    to[0],
                    to[1],
                    to[2],
                    profile.batter,
                    profile.step_height,
                    floor.min(profile.depth),
                ]
            }
        };
        numbers.iter().all(|v| v.is_finite()) && numbers.last().is_some_and(|r| *r > 0.0)
    }
}

/// Whether a shape takes the ground away, puts it back, or evens out the
/// dug ground inside it, `amount` (0 to 1) of the way to the ground round
/// each point at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Carve {
    Dig,
    Fill,
    Smooth { amount: f32 },
}

/// How many times a smoothing evens the ground out, each over the last.
const SMOOTH_PASSES: usize = 3;

/// How a terrain's voxels lie: how wide each is, and how tall a chunk.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub voxel: f32,
}

impl Grid {
    pub fn of(heights: &Heightmap) -> Self {
        Self { voxel: heights.spacing() / 2.0 }
    }

    /// How wide a chunk is, every way, in metres.
    pub fn chunk(&self) -> f32 {
        self.voxel * CELLS as f32
    }

    /// Where corner `(x, y, z)` of chunk `key` is.
    pub fn corner_at(&self, key: ChunkKey, x: usize, y: usize, z: usize) -> Vec3 {
        let cells = CELLS as i64;
        let at = |chunk: i32, i: usize| (i64::from(chunk) * cells + i as i64) as f32 * self.voxel;
        Vec3::new(at(key.x, x), at(key.y, y), at(key.z, z))
    }

    /// The chunk holding the voxel at `p`, each way.
    fn chunk_of(&self, v: f32) -> i32 {
        (v / self.chunk()).floor() as i32
    }
}

/// The way across a line from `a` to `b`, level, to its left going along.
fn across_of(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(-(b.z - a.z), 0.0, b.x - a.x).try_normalize().unwrap_or(Vec3::Z)
}

/// The ground the layers `layers` dig into `heights`: each Dig layer's
/// strokes and each Spline dug as a trench, in the layers' order, over the
/// ground every layer makes. The heights with holes for the dug columns,
/// and the dug ground, none where nothing is dug.
pub fn dig_layers(layers: &[crate::layers::Layer], heights: &Heightmap) -> (Option<Heightmap>, Option<Excavation>) {
    use crate::layers::LayerKind;
    let mut dug = Excavation::default();
    let mut holed: Option<Heightmap> = None;
    for layer in layers.iter().filter(|layer| layer.enabled) {
        let carves: Vec<(Shape, Carve, Option<u8>)> = match &layer.kind {
            LayerKind::Dig { strokes } => strokes
                .iter()
                .map(|stroke| (Shape::Ball { at: stroke.at, radius: stroke.radius }, stroke.carve(), stroke.surface))
                .collect(),
            LayerKind::Spline { points, width, dig: Some(trench), .. } => {
                let shapes = Shape::trench(holed.as_ref().unwrap_or(heights), points, *width, *trench);
                shapes.into_iter().map(|(shape, carve)| (shape, carve, trench.surface)).collect()
            }
            _ => continue,
        };
        for (shape, carve, surface) in carves {
            let now = holed.as_ref().unwrap_or(heights);
            if let Some((Some(cut), _)) = dug.carve(now, &shape, carve, surface) {
                holed = Some(cut);
            }
        }
    }
    (holed, (!dug.is_empty()).then_some(dug))
}

/// A hash of the layers that dig: what [`dig_layers`] digs, in order.
pub fn digging_hash(layers: &[crate::layers::Layer]) -> u64 {
    let mut text = String::new();
    for layer in layers.iter().filter(|layer| layer.enabled && layer.kind.digs()) {
        text.push_str(&toml::to_string(layer).unwrap_or_default());
        text.push('\n');
    }
    crate::layers::stable_hash(text.as_bytes())
}

/// The ground `layers` dig into `heights`, as [`dig_layers`], but keeping
/// `previous`'s dug ground (and the holes for it, already in `heights`)
/// where the same layers dug it into the same heights: none of the tiles
/// its dug columns are in made again since (`heights` holding the same
/// tiles as `previous_heights` there). The heights, and the dug ground.
pub fn redig(
    layers: &[crate::layers::Layer],
    heights: Heightmap,
    previous: Option<(&Heightmap, &Arc<Excavation>)>,
) -> (Heightmap, Option<Arc<Excavation>>) {
    let hash = digging_hash(layers);
    if !layers.iter().any(|layer| layer.enabled && layer.kind.digs()) {
        return (heights, None);
    }
    let quads = TILE_QUADS as i32 / CHUNK_SAMPLES as i32;
    if let Some((was, dug)) = previous
        && dug.dug_by == hash
        && dug.columns().iter().all(|&(x, z)| {
            let (tx, tz) = ((x / quads) as u32, (z / quads) as u32);
            Arc::ptr_eq(heights.tile(tx, tz), was.tile(tx, tz))
        })
    {
        return (heights, Some(dug.clone()));
    }
    let (holed, dug) = dig_layers(layers, &heights);
    let dug = dug.map(|mut dug| {
        dug.dug_by = hash;
        Arc::new(dug)
    });
    (holed.unwrap_or(heights), dug)
}

/// `heights` without the holes digging cut, as a terrain's `.heights` file
/// keeps them: each tile with dug columns in it holed only where its
/// terrain's hole raster says (`painted`, a tile's bits, or none).
pub fn undug(heights: &Heightmap, dug: &Excavation, painted: impl Fn(u32, u32) -> Option<Box<[u32]>>) -> Heightmap {
    let quads = TILE_QUADS as i32 / CHUNK_SAMPLES as i32;
    let mut tiles: Vec<(u32, u32)> =
        dug.columns().iter().map(|&(x, z)| ((x / quads) as u32, (z / quads) as u32)).collect();
    tiles.sort_unstable();
    tiles.dedup();
    let changed = tiles.into_iter().map(|(tx, tz)| {
        let tile = heights.tile(tx, tz);
        (tx, tz, HeightTile::from_samples(tile.samples().to_vec()).with_holes(painted(tx, tz)))
    });
    heights.with_tiles(changed.collect::<Vec<_>>())
}

/// `grass` with none growing where digging opened the ground under it:
/// where the ground just under the heights' surface is air now. Over `area`
/// (metres from the terrain's corner, `[x0, z0, x1, z1]`), or every dug
/// column with none.
pub fn clear_grass(grass: &GrassMap, heights: &Heightmap, dug: &Excavation, area: Option<Area>) -> GrassMap {
    let column = Grid::of(heights).chunk();
    let area = area.unwrap_or_else(|| {
        let columns = dug.columns();
        let (mut x0, mut z0, mut x1, mut z1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, z) in columns {
            (x0, z0) = (x0.min(x as f32 * column), z0.min(z as f32 * column));
            (x1, z1) = (x1.max((x + 1) as f32 * column), z1.max((z + 1) as f32 * column));
        }
        [x0, z0, x1, z1]
    });
    if area[0] > area[2] {
        return grass.clone();
    }
    let spacing = heights.spacing();
    let words = grass.words();
    let [qx, qz] = heights.quads().map(i64::from);
    let first = |v: f32, most: i64| ((v / spacing).floor() as i64).clamp(0, most);
    let last = |v: f32, most: i64| ((v / spacing).ceil() as i64).clamp(0, most);
    let (x0, z0, x1, z1) = (first(area[0], qx), first(area[1], qz), last(area[2], qx), last(area[3], qz));
    let mut tiles = grass.tiles().to_vec();
    let (tiles_x, tiles_z) = (i64::from(heights.tiles_x()), i64::from(heights.tiles_z()));
    let quads = i64::from(TILE_QUADS);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let (px, pz) = (x as f32 * spacing, z as f32 * spacing);
            let under = Vec3::new(px, heights.sample(x, z) - spacing * 0.25, pz);
            if dug.density_at(heights, under) > 0.0 {
                continue;
            }
            // In every tile holding the sample: those either side of a tile's edge share it.
            for tz in [z / quads - 1, z / quads].into_iter().filter(|t| (0..tiles_z).contains(t)) {
                for tx in [x / quads - 1, x / quads].into_iter().filter(|t| (0..tiles_x).contains(t)) {
                    let (ix, iz) = (x - tx * quads, z - tz * quads);
                    if !(0..=quads).contains(&ix) || !(0..=quads).contains(&iz) {
                        continue;
                    }
                    let index = (tz * tiles_x + tx) as usize;
                    let mut samples = tiles[index].samples().to_vec();
                    let at = (iz as u32 * TILE_SAMPLES + ix as u32) as usize * words;
                    samples[at..at + words].iter_mut().for_each(|word| *word = 0);
                    tiles[index] = Arc::new(GrassTile::from_samples(samples));
                }
            }
        }
    }
    GrassMap::from_tiles(heights.tiles_x(), heights.tiles_z(), words, tiles)
}

/// The ground's density at `p` as the heights have it: how far below the
/// ground's height there `p` is.
fn natural(heights: &Heightmap, p: Vec3) -> f32 {
    heights.height_at(p.x, p.z) - p.y
}

impl Excavation {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Every dug chunk, in order.
    pub fn chunks(&self) -> &BTreeMap<ChunkKey, Arc<DugChunk>> {
        &self.chunks
    }

    pub fn chunk(&self, key: ChunkKey) -> Option<&Arc<DugChunk>> {
        self.chunks.get(&key)
    }

    /// The same, with chunk `key` as `chunk`.
    pub fn with_chunk(&self, key: ChunkKey, chunk: DugChunk) -> Self {
        let mut chunks = self.chunks.clone();
        chunks.insert(key, Arc::new(chunk));
        Self { chunks, dug_by: self.dug_by }
    }

    /// Whether the column of chunks at `(x, z)` is dug.
    pub fn is_dug(&self, x: i32, z: i32) -> bool {
        self.chunks.range(ChunkKey { x, y: i32::MIN, z }..=ChunkKey { x, y: i32::MAX, z }).next().is_some()
    }

    /// The dug columns, `(x, z)` each, in order.
    pub fn columns(&self) -> Vec<(i32, i32)> {
        let mut columns: Vec<(i32, i32)> = self.chunks.keys().map(|key| (key.x, key.z)).collect();
        columns.dedup();
        columns
    }

    /// The ground's density at corner `(x, y, z)` of the whole terrain's
    /// voxels: its dug chunk's, or the heights'.
    pub fn density(&self, heights: &Heightmap, grid: Grid, x: i64, y: i64, z: i64) -> f32 {
        let cells = CELLS as i64;
        let key =
            ChunkKey { x: x.div_euclid(cells) as i32, y: y.div_euclid(cells) as i32, z: z.div_euclid(cells) as i32 };
        let p = Vec3::new(x as f32, y as f32, z as f32) * grid.voxel;
        match self.chunks.get(&key) {
            Some(chunk) => {
                let (i, j, k) =
                    (x.rem_euclid(cells) as usize, y.rem_euclid(cells) as usize, z.rem_euclid(cells) as usize);
                chunk.density[corner(i, j, k)]
            }
            None => natural(heights, p),
        }
    }

    /// A chunk as the heights have it, nothing dug.
    pub(crate) fn fresh(heights: &Heightmap, grid: Grid, key: ChunkKey) -> DugChunk {
        let mut density = vec![0.0; CORNERS * CORNERS * CORNERS];
        for y in 0..CORNERS {
            for z in 0..CORNERS {
                for x in 0..CORNERS {
                    density[corner(x, y, z)] = natural(heights, grid.corner_at(key, x, y, z));
                }
            }
        }
        DugChunk { density: density.into_boxed_slice(), dug: vec![0; CORNERS * CORNERS * CORNERS].into_boxed_slice() }
    }

    /// The chunks of column `(x, z)` that its ground passes through, from
    /// a voxel below its lowest height to one above its highest.
    fn column_span(heights: &Heightmap, grid: Grid, x: i32, z: i32) -> (i32, i32) {
        let [qx, qz] = heights.quads().map(i64::from);
        let samples = i64::from(CHUNK_SAMPLES);
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for sz in (i64::from(z) * samples)..=((i64::from(z) + 1) * samples).min(qz) {
            for sx in (i64::from(x) * samples)..=((i64::from(x) + 1) * samples).min(qx) {
                let h = heights.sample(sx, sz);
                (low, high) = (low.min(h), high.max(h));
            }
        }
        (grid.chunk_of(low - grid.voxel), grid.chunk_of(high + grid.voxel))
    }

    /// Takes the ground away inside `shape`, or puts it back, as `carve`
    /// says, laying surface `surface` (its number) where it digs; keeps
    /// every chunk it reaches and every chunk of their columns the ground
    /// passes through, and cuts holes in `heights` inside the columns newly
    /// dug. The changed heights, and the chunks it changed, or none where
    /// the shape misses the terrain.
    pub fn carve(
        &mut self,
        heights: &Heightmap,
        shape: &Shape,
        carve: Carve,
        surface: Option<u8>,
    ) -> Option<(Option<Heightmap>, Vec<ChunkKey>)> {
        if !shape.is_sound() {
            return None;
        }
        if let Carve::Smooth { amount } = carve {
            return self.smooth(heights, shape, amount).map(|changed| (None, changed));
        }
        let grid = Grid::of(heights);
        let (low, high) = shape.bounds();
        let (low, high) = (low - grid.voxel, high + grid.voxel);
        let [qx, qz] = heights.quads();
        let columns_x = (qx / CHUNK_SAMPLES) as i32;
        let columns_z = (qz / CHUNK_SAMPLES) as i32;
        let (x0, x1) = (grid.chunk_of(low.x).max(0), grid.chunk_of(high.x).min(columns_x - 1));
        let (z0, z1) = (grid.chunk_of(low.z).max(0), grid.chunk_of(high.z).min(columns_z - 1));
        if x0 > x1 || z0 > z1 {
            return None;
        }
        let (y0, y1) = (grid.chunk_of(low.y), grid.chunk_of(high.y));
        let mut newly = Vec::new();
        let mut touched = Vec::new();
        for z in z0..=z1 {
            for x in x0..=x1 {
                if !self.is_dug(x, z) {
                    newly.push((x, z));
                }
                let (low, high) = Self::column_span(heights, grid, x, z);
                for y in low.min(y0)..=high.max(y1) {
                    let key = ChunkKey { x, y, z };
                    self.chunks.entry(key).or_insert_with(|| Arc::new(Self::fresh(heights, grid, key)));
                    if (y0..=y1).contains(&y) {
                        touched.push(key);
                    }
                }
            }
        }
        let mark = surface.map_or(0, |s| s.saturating_add(1));
        // Only corners within two voxels of the shape's surface can move the ground's surface, so only those
        // change, and none further than that either side, each to a whole number of steps of 1/1024 m: the same
        // ground, kept and sent in far fewer bytes.
        let band = grid.voxel * 2.0;
        let on_step = |v: f32| (v * 1024.0).round() / 1024.0;
        let mut changed = Vec::new();
        for key in touched {
            let chunk = Arc::make_mut(self.chunks.get_mut(&key).expect("kept above"));
            let mut any = false;
            for y in 0..CORNERS {
                for z in 0..CORNERS {
                    for x in 0..CORNERS {
                        let p = grid.corner_at(key, x, y, z);
                        let distance = shape.distance(p);
                        if distance >= band {
                            continue;
                        }
                        let at = corner(x, y, z);
                        let was = chunk.density[at];
                        let now = on_step(match carve {
                            Carve::Fill => was.max((-distance).min(band)),
                            _ => was.min(distance.max(-band)),
                        });
                        let now = match carve {
                            Carve::Fill => now.max(was),
                            _ => now.min(was),
                        };
                        if now != was {
                            chunk.density[at] = now;
                            // Ground digging opened is what it bares; ground filled again is the terrain's own.
                            if carve == Carve::Dig && was > 0.0 && now <= 0.0 {
                                chunk.dug[at] = mark;
                            } else if carve == Carve::Fill && now > 0.0 {
                                chunk.dug[at] = 0;
                            }
                            any = true;
                        }
                    }
                }
            }
            if any {
                changed.push(key);
            }
        }
        let holed = (!newly.is_empty()).then(|| cut_columns(heights, self, &newly));
        Some((holed, changed))
    }

    /// Corner `(x, y, z)` of the whole terrain's voxels as a dug chunk holds
    /// it: its density and the surface digging laid there. None where no
    /// chunk does.
    fn held(&self, x: i64, y: i64, z: i64) -> Option<(f32, u8)> {
        let cells = CELLS as i64;
        // In the chunk it starts, or on the far face of the one before it each way.
        let holders = |v: i64| {
            let (chunk, at) = (v.div_euclid(cells), v.rem_euclid(cells) as usize);
            [(chunk, at), (chunk - 1, if at == 0 { CELLS } else { usize::MAX })]
        };
        for (cx, ix) in holders(x) {
            for (cy, iy) in holders(y) {
                for (cz, iz) in holders(z) {
                    if ix > CELLS || iy > CELLS || iz > CELLS {
                        continue;
                    }
                    let key = ChunkKey { x: cx as i32, y: cy as i32, z: cz as i32 };
                    if let Some(chunk) = self.chunks.get(&key) {
                        let at = corner(ix, iy, iz);
                        return Some((chunk.density[at], chunk.dug[at]));
                    }
                }
            }
        }
        None
    }

    /// Sets corner `(x, y, z)` in every chunk holding it, noting each in
    /// `changed`.
    fn set_corner(&mut self, (x, y, z): (i64, i64, i64), density: f32, laid: u8, changed: &mut Vec<ChunkKey>) {
        let cells = CELLS as i64;
        let holders = |v: i64| {
            let (chunk, at) = (v.div_euclid(cells), v.rem_euclid(cells) as usize);
            [(chunk, at), (chunk - 1, if at == 0 { CELLS } else { usize::MAX })]
        };
        for (cx, ix) in holders(x) {
            for (cy, iy) in holders(y) {
                for (cz, iz) in holders(z) {
                    if ix > CELLS || iy > CELLS || iz > CELLS {
                        continue;
                    }
                    let key = ChunkKey { x: cx as i32, y: cy as i32, z: cz as i32 };
                    if let Some(chunk) = self.chunks.get_mut(&key) {
                        let chunk = Arc::make_mut(chunk);
                        let at = corner(ix, iy, iz);
                        (chunk.density[at], chunk.dug[at]) = (density, laid);
                        if !changed.contains(&key) {
                            changed.push(key);
                        }
                    }
                }
            }
        }
    }

    /// Whether the corners at `(x, z)` of the whole terrain's voxels are
    /// inside dug ground, every column they are a corner of dug: those on
    /// a face shared with ground not dug stay as the heights have them, so
    /// the voxels still meet the heights there.
    fn inside_dug(&self, x: i64, z: i64) -> bool {
        let cells = CELLS as i64;
        let columns = |v: i64| {
            let column = v.div_euclid(cells) as i32;
            if v.rem_euclid(cells) == 0 { [column - 1, column] } else { [column, column] }
        };
        columns(x).iter().all(|&cx| columns(z).iter().all(|&cz| self.is_dug(cx, cz)))
    }

    /// Evens out the dug ground inside `shape`, each corner near its surface
    /// `amount` of the way to the average of the six round it, fully more
    /// than a metre inside the shape and less towards its edge, a few times
    /// over. Only dug ground changes, and none of it on a face it shares
    /// with ground not dug. The chunks changed, none where the shape misses
    /// the terrain.
    fn smooth(&mut self, heights: &Heightmap, shape: &Shape, amount: f32) -> Option<Vec<ChunkKey>> {
        if !amount.is_finite() || amount <= 0.0 {
            return None;
        }
        let amount = amount.min(1.0);
        let grid = Grid::of(heights);
        let band = grid.voxel * 2.0;
        let on_step = |v: f32| (v * 1024.0).round() / 1024.0;
        let (low, high) = shape.bounds();
        let first = |v: f32| (v / grid.voxel).floor() as i64;
        let last = |v: f32| (v / grid.voxel).ceil() as i64;
        let [qx, qz] = heights.quads().map(|q| i64::from(q) * 2);
        let (x0, x1) = (first(low.x).max(0), last(high.x).min(qx));
        let (z0, z1) = (first(low.z).max(0), last(high.z).min(qz));
        let (y0, y1) = (first(low.y), last(high.y));
        if x0 > x1 || z0 > z1 {
            return None;
        }
        let value = |dug: &Excavation, x: i64, y: i64, z: i64| {
            dug.held(x, y, z).map_or_else(|| dug.density(heights, grid, x, y, z), |(density, _)| density)
        };
        const AROUND: [(i64, i64, i64); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
        let mut changed = Vec::new();
        for _ in 0..SMOOTH_PASSES {
            // Each pass reads the ground as the last left it, then changes it.
            let mut moves = Vec::new();
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if !self.inside_dug(x, z) {
                        continue;
                    }
                    for y in y0..=y1 {
                        let Some((was, laid)) = self.held(x, y, z) else { continue };
                        if was.abs() >= band {
                            continue;
                        }
                        let p = Vec3::new(x as f32, y as f32, z as f32) * grid.voxel;
                        let weight = amount * (-shape.distance(p) / (band * 2.0)).clamp(0.0, 1.0);
                        if weight <= 0.0 {
                            continue;
                        }
                        let round: f32 =
                            AROUND.iter().map(|&(dx, dy, dz)| value(self, x + dx, y + dy, z + dz)).sum::<f32>() / 6.0;
                        let now = on_step((was + (round - was) * weight).clamp(-band, band));
                        if now == was {
                            continue;
                        }
                        // Opened, it bares what digging laid beside it.
                        let laid = match was > 0.0 && now <= 0.0 {
                            true => AROUND
                                .iter()
                                .filter_map(|&(dx, dy, dz)| self.held(x + dx, y + dy, z + dz).map(|(_, laid)| laid))
                                .max()
                                .unwrap_or(laid),
                            false => laid,
                        };
                        moves.push(((x, y, z), now, laid));
                    }
                }
            }
            if moves.is_empty() {
                break;
            }
            for (at, density, laid) in moves {
                self.set_corner(at, density, laid, &mut changed);
            }
        }
        changed.sort_unstable();
        Some(changed)
    }
}

impl Excavation {
    /// The ground's density at `p`, in metres from the terrain's corner,
    /// blended between the corners round it.
    pub fn density_at(&self, heights: &Heightmap, p: Vec3) -> f32 {
        let grid = Grid::of(heights);
        let g = p / grid.voxel;
        let low = g.floor();
        let t = g - low;
        let (x, y, z) = (low.x as i64, low.y as i64, low.z as i64);
        let d = |dx: i64, dy: i64, dz: i64| self.density(heights, grid, x + dx, y + dy, z + dz);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let along_x = |dy: i64, dz: i64| lerp(d(0, dy, dz), d(1, dy, dz), t.x);
        let along_z = |dy: i64| lerp(along_x(dy, 0), along_x(dy, 1), t.z);
        lerp(along_z(0), along_z(1), t.y)
    }

    /// Which way the ground faces at `p`: down its density's slope.
    fn normal_at(&self, heights: &Heightmap, p: Vec3) -> Vec3 {
        let e = Grid::of(heights).voxel * 0.5;
        let d = |offset: Vec3| self.density_at(heights, p + offset);
        let slope = Vec3::new(
            d(Vec3::X * e) - d(-Vec3::X * e),
            d(Vec3::Y * e) - d(-Vec3::Y * e),
            d(Vec3::Z * e) - d(-Vec3::Z * e),
        );
        (-slope).try_normalize().unwrap_or(Vec3::Y)
    }

    /// The first place a ray from `origin` along `direction`, in the
    /// terrain's space, goes from air into dug ground within
    /// `max_distance`: stepped through the dug chunks it crosses a half
    /// voxel at a time, then found between the last two steps.
    pub fn raycast(&self, heights: &Heightmap, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<GroundHit> {
        let direction = direction.try_normalize()?;
        let grid = Grid::of(heights);
        let size = grid.chunk();
        // Where the ray is inside any dug chunk.
        let (mut first, mut last) = (f32::MAX, f32::MIN);
        for key in self.chunks.keys() {
            let low = Vec3::new(key.x as f32, key.y as f32, key.z as f32) * size;
            let high = low + size;
            let (mut near, mut far) = (0.0f32, max_distance);
            for axis in 0..3 {
                if direction[axis].abs() < 1e-9 {
                    if origin[axis] < low[axis] || origin[axis] > high[axis] {
                        (near, far) = (1.0, 0.0);
                    }
                    continue;
                }
                let (a, b) =
                    ((low[axis] - origin[axis]) / direction[axis], (high[axis] - origin[axis]) / direction[axis]);
                near = near.max(a.min(b));
                far = far.min(a.max(b));
            }
            if near <= far {
                (first, last) = (first.min(near), last.max(far));
            }
        }
        if first > last {
            return None;
        }
        let step = grid.voxel * 0.5;
        let mut t = first;
        let mut before = self.density_at(heights, origin + direction * t);
        while t < last {
            let next = (t + step).min(last);
            let now = self.density_at(heights, origin + direction * next);
            if before <= 0.0 && now > 0.0 {
                let met = t + (next - t) * (before / (before - now));
                let point = origin + direction * met;
                return Some(GroundHit { point, distance: met, normal: self.normal_at(heights, point) });
            }
            (t, before) = (next, now);
        }
        None
    }

    /// The top of the dug ground over `(x, z)`, in the terrain's space,
    /// where its column is dug: how high, and which way it faces there.
    pub fn surface_at(&self, heights: &Heightmap, x: f32, z: f32) -> Option<(f32, Vec3)> {
        let grid = Grid::of(heights);
        let (cx, cz) = (grid.chunk_of(x), grid.chunk_of(z));
        let mut column =
            self.chunks.range(ChunkKey { x: cx, z: cz, y: i32::MIN }..=ChunkKey { x: cx, z: cz, y: i32::MAX });
        let top = column.next_back()?.0.y;
        let top = (top + 1) as f32 * grid.chunk();
        let hit = self.raycast(heights, Vec3::new(x, top + 1.0, z), Vec3::NEG_Y, f32::MAX)?;
        Some((hit.point.y, hit.normal))
    }
}

/// `heights` with holes where the voxels of `dug` columns stand: every
/// sample whose columns, all those it is the corner or edge of, are dug,
/// for those of `newly` dug. A sample between a dug column and one not dug
/// is kept, where the voxels meet the heights.
pub fn cut_columns(heights: &Heightmap, dug: &Excavation, newly: &[(i32, i32)]) -> Heightmap {
    let mut tiles: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    let samples = i64::from(CHUNK_SAMPLES);
    let [qx, qz] = heights.quads().map(i64::from);
    let quads = i64::from(TILE_QUADS);
    // Every column a sample is in: one inside a column, two on an edge, four at a corner.
    let all_dug = |x: i64, z: i64| {
        let span = |v: i64| {
            let column = v.div_euclid(samples);
            if v.rem_euclid(samples) == 0 { vec![column - 1, column] } else { vec![column] }
        };
        let (xs, zs) = (span(x), span(z));
        xs.iter().all(|&cx| zs.iter().all(|&cz| dug.is_dug(cx as i32, cz as i32)))
    };
    for &(cx, cz) in newly {
        let (ox, oz) = (i64::from(cx) * samples, i64::from(cz) * samples);
        for z in oz..=(oz + samples).min(qz) {
            for x in ox..=(ox + samples).min(qx) {
                if !all_dug(x, z) {
                    continue;
                }
                // In every tile that holds the sample: those either side of a tile's edge share it.
                let holders = |v: i64, most: i64| {
                    let tile = (v / quads).min(most);
                    if v % quads == 0 && v > 0 {
                        vec![tile - 1, tile].into_iter().filter(|t| *t <= most).collect::<Vec<_>>()
                    } else {
                        vec![tile]
                    }
                };
                for tz in holders(z, qz / quads - 1) {
                    for tx in holders(x, qx / quads - 1) {
                        let (tx, tz) = (tx as u32, tz as u32);
                        let bits = tiles.entry((tx, tz)).or_insert_with(|| {
                            heights.tile(tx, tz).holes().map_or_else(|| vec![0; HOLE_WORDS], <[u32]>::to_vec)
                        });
                        let (ix, iz) = (x - i64::from(tx) * quads, z - i64::from(tz) * quads);
                        let index = (iz as u32 * TILE_SAMPLES + ix as u32) as usize;
                        bits[index / 32] |= 1 << (index % 32);
                    }
                }
            }
        }
    }
    let changed = tiles.into_iter().map(|((tx, tz), bits)| {
        let tile = heights.tile(tx, tz);
        (tx, tz, HeightTile::from_samples(tile.samples().to_vec()).with_holes(Some(bits.into_boxed_slice())))
    });
    heights.with_tiles(changed.collect::<Vec<_>>())
}

/// A chunk's surface as triangles: each point in metres from the terrain's
/// corner, the way the ground faces there, the surface digging laid there
/// or beside it (its number and one; 0 for the terrain's own), and whether
/// it lies on it, to blend from; three points a triangle, wound
/// counter-clockwise seen from the air.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub surfaces: Vec<u8>,
    pub laid: Vec<bool>,
    pub indices: Vec<u32>,
}

/// The six tetrahedra each voxel is split into, by its corners (bit 0 x,
/// bit 1 y, bit 2 z): along its diagonal from corner 0 to corner 7, one for
/// each order of the three ways, so every face is split along its own
/// diagonal from its lowest corner, as its neighbour's is.
const TETRAHEDRA: [[usize; 4]; 6] =
    [[0, 1, 3, 7], [0, 1, 5, 7], [0, 2, 3, 7], [0, 2, 6, 7], [0, 4, 5, 7], [0, 4, 6, 7]];

/// Chunk `key`'s surface, from its densities (and its neighbours', for the
/// way the ground faces at its edges); none for a chunk not dug.
pub fn mesh(excavation: &Excavation, heights: &Heightmap, key: ChunkKey) -> ChunkMesh {
    let mut out = ChunkMesh::default();
    let Some(chunk) = excavation.chunk(key) else { return out };
    let grid = Grid::of(heights);
    let cells = CELLS as i64;
    let origin = [i64::from(key.x) * cells, i64::from(key.y) * cells, i64::from(key.z) * cells];
    let at = |x: usize, y: usize, z: usize| chunk.density[corner(x, y, z)];
    // The way the ground faces at a corner of the whole terrain's voxels: down its density's slope, as it grows
    // into the ground.
    let density = |x: i64, y: i64, z: i64| -> f32 {
        let (i, j, k) = (x - origin[0], y - origin[1], z - origin[2]);
        if (0..=cells).contains(&i) && (0..=cells).contains(&j) && (0..=cells).contains(&k) {
            at(i as usize, j as usize, k as usize)
        } else {
            excavation.density(heights, grid, x, y, z)
        }
    };
    let slope = |x: i64, y: i64, z: i64| -> Vec3 {
        Vec3::new(
            density(x + 1, y, z) - density(x - 1, y, z),
            density(x, y + 1, z) - density(x, y - 1, z),
            density(x, y, z + 1) - density(x, y, z - 1),
        )
    };
    let mut made: HashMap<(u64, u64), u32> = HashMap::new();
    let id = |x: i64, y: i64, z: i64| -> u64 {
        // Every corner of the terrain its own number, the same in every chunk.
        let pack = |v: i64| (v + (1 << 20)) as u64 & 0x1F_FFFF;
        pack(x) | pack(y) << 21 | pack(z) << 42
    };
    for y in 0..CELLS {
        for z in 0..CELLS {
            for x in 0..CELLS {
                let corners: [(usize, usize, usize); 8] =
                    std::array::from_fn(|c| (x + (c & 1), y + ((c >> 1) & 1), z + ((c >> 2) & 1)));
                let values = corners.map(|(cx, cy, cz)| at(cx, cy, cz));
                let solid = values.iter().filter(|v| **v > 0.0).count();
                if solid == 0 || solid == 8 {
                    continue;
                }
                // The surface digging laid anywhere in the voxel, which its triangles blend to.
                let laid_here = corners.iter().map(|&(cx, cy, cz)| chunk.dug[corner(cx, cy, cz)]).max().unwrap_or(0);
                for tet in TETRAHEDRA {
                    let inside: Vec<usize> = tet.iter().copied().filter(|&c| values[c] > 0.0).collect();
                    let outside: Vec<usize> = tet.iter().copied().filter(|&c| values[c] <= 0.0).collect();
                    if inside.is_empty() || outside.is_empty() {
                        continue;
                    }
                    // The point where the surface crosses an edge, made once for every chunk that shares it.
                    let mut point = |a: usize, b: usize| -> u32 {
                        let global = |c: usize| {
                            let (cx, cy, cz) = corners[c];
                            (origin[0] + cx as i64, origin[1] + cy as i64, origin[2] + cz as i64)
                        };
                        let (ga, gb) = (global(a), global(b));
                        let (ia, ib) = (id(ga.0, ga.1, ga.2), id(gb.0, gb.1, gb.2));
                        // Always from the lower-numbered end, so both chunks work it out alike.
                        let (lo, hi, glo, ghi, clo, chi) =
                            if ia < ib { (ia, ib, ga, gb, a, b) } else { (ib, ia, gb, ga, b, a) };
                        if let Some(&found) = made.get(&(lo, hi)) {
                            return found;
                        }
                        let (dlo, dhi) = (values[clo], values[chi]);
                        let t = dlo / (dlo - dhi);
                        let place = |g: (i64, i64, i64)| Vec3::new(g.0 as f32, g.1 as f32, g.2 as f32) * grid.voxel;
                        let p = place(glo) + (place(ghi) - place(glo)) * t;
                        let n = -(slope(glo.0, glo.1, glo.2) * (1.0 - t) + slope(ghi.0, ghi.1, ghi.2) * t);
                        let n = n.try_normalize().unwrap_or(Vec3::Y);
                        // Bared by digging where its air end was opened; beside that, blending to it.
                        let air = if dlo > 0.0 { chi } else { clo };
                        let (ax, ay, az) = corners[air];
                        let opened = chunk.dug[corner(ax, ay, az)];
                        out.positions.push(p.to_array());
                        out.normals.push(n.to_array());
                        out.surfaces.push(if opened != 0 { opened } else { laid_here });
                        out.laid.push(opened != 0);
                        let index = out.positions.len() as u32 - 1;
                        made.insert((lo, hi), index);
                        index
                    };
                    let triangles: Vec<[u32; 3]> = match (inside.as_slice(), outside.as_slice()) {
                        (&[a], &[b, c, d]) | (&[b, c, d], &[a]) => vec![[point(a, b), point(a, c), point(a, d)]],
                        (&[a, b], &[c, d]) => {
                            let quad = [point(a, c), point(a, d), point(b, d), point(b, c)];
                            vec![[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]]
                        }
                        _ => Vec::new(),
                    };
                    // Each wound to face the air: away from the solid corners.
                    let place = |c: usize| {
                        let (cx, cy, cz) = corners[c];
                        Vec3::new(cx as f32, cy as f32, cz as f32)
                    };
                    let solid_middle = inside.iter().map(|&c| place(c)).sum::<Vec3>() / inside.len() as f32;
                    let air_middle = outside.iter().map(|&c| place(c)).sum::<Vec3>() / outside.len() as f32;
                    let outward = air_middle - solid_middle;
                    for [i0, i1, i2] in triangles {
                        if i0 == i1 || i1 == i2 || i0 == i2 {
                            continue;
                        }
                        let p = |i: u32| Vec3::from(out.positions[i as usize]);
                        let facing = (p(i1) - p(i0)).cross(p(i2) - p(i0));
                        if facing.dot(outward) >= 0.0 {
                            out.indices.extend([i0, i1, i2]);
                        } else {
                            out.indices.extend([i0, i2, i1]);
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> Heightmap {
        Heightmap::from_fn(1, 1, 0.5, |x, z| 2.0 + (x * 0.2).sin() * 0.5 + z * 0.03)
    }

    #[test]
    fn a_ball_dug_into_the_ground_keeps_its_columns_and_holes_their_insides() {
        let heights = field();
        let mut dug = Excavation::default();
        let ground = heights.height_at(20.0, 20.0);
        let ball = Shape::Ball { at: [20.0, ground, 20.0], radius: 1.5 };
        let (holed, changed) = dug.carve(&heights, &ball, Carve::Dig, Some(2)).expect("on the terrain");
        let holed = holed.expect("a newly dug column");
        assert!(!changed.is_empty());
        // 20 m is in the third column of 8 m: samples 32 to 48, those inside holes.
        assert!(dug.is_dug(2, 2) && !dug.is_dug(0, 0));
        assert!(holed.is_hole(40, 40) && holed.is_hole(33, 47));
        assert!(!holed.is_hole(32, 40) && !holed.is_hole(48, 40), "its edges kept");
        // Dug on the edge of four columns, the samples between them are holes too, but not their outer edges.
        let mut more = Excavation::default();
        let (holed, _) =
            more.carve(&heights, &Shape::Ball { at: [16.0, ground, 16.0], radius: 1.0 }, Carve::Dig, None).unwrap();
        let holed = holed.unwrap();
        assert!(holed.is_hole(32, 32) && holed.is_hole(32, 40) && holed.is_hole(40, 32), "between dug columns");
        assert!(!holed.is_hole(16, 32) && !holed.is_hole(32, 48), "where they meet ground not dug");
        // Air in the ball, ground beside it, as the heights were where it did not reach.
        let grid = Grid::of(&heights);
        let corner = |p: f32| (p / grid.voxel).round() as i64;
        let at =
            |dug: &Excavation, x: f32, y: f32, z: f32| dug.density(&heights, grid, corner(x), corner(y), corner(z));
        assert!(at(&dug, 20.0, ground, 20.0) < -0.4, "air in the ball");
        let beside = heights.height_at(17.0, 17.0) - (ground - 1.0);
        assert!((at(&dug, 17.0, ground - 1.0, 17.0) - beside).abs() < 0.3);
        // Filled again, the ground is back.
        dug.carve(&heights, &ball, Carve::Fill, None);
        assert!(at(&dug, 20.0, ground - 1.0, 20.0) > 0.0);
    }

    #[test]
    fn a_dug_columns_surface_meets_the_heights_on_its_edges_and_has_no_gaps() {
        let heights = field();
        let mut dug = Excavation::default();
        let ground = heights.height_at(20.0, 20.0);
        dug.carve(&heights, &Shape::Ball { at: [20.0, ground, 20.0], radius: 2.0 }, Carve::Dig, Some(1));
        let mut edges: HashMap<([u32; 3], [u32; 3]), i32> = HashMap::new();
        let mut on_edge = 0;
        for &key in dug.chunks().keys() {
            let mesh = mesh(&dug, &heights, key);
            for [a, b, c] in mesh.indices.chunks(3).map(|t| [t[0], t[1], t[2]]) {
                for (from, to) in [(a, b), (b, c), (c, a)] {
                    let (p, q) = (mesh.positions[from as usize], mesh.positions[to as usize]);
                    let (p, q) = (p.map(f32::to_bits), q.map(f32::to_bits));
                    // Each edge once each way across chunks: an edge on its own is a gap or the edge of the column.
                    *edges.entry((p, q)).or_default() += 1;
                    *edges.entry((q, p)).or_default() -= 1;
                }
            }
            // Every point on the column's outside faces lies on the heights.
            for p in &mesh.positions {
                let on_x = (p[0] / 0.5 - 32.0).abs() < 1e-4 || (p[0] / 0.5 - 48.0).abs() < 1e-4;
                let on_z = (p[2] / 0.5 - 32.0).abs() < 1e-4 || (p[2] / 0.5 - 48.0).abs() < 1e-4;
                if on_x || on_z {
                    on_edge += 1;
                    let h = heights.height_at(p[0], p[2]);
                    assert!((p[1] - h).abs() < 1e-4, "{p:?} off the heights' {h}");
                }
            }
        }
        assert!(on_edge > 40, "the column meets the heights all round: {on_edge} points");
        // Inside the column every edge is shared, wound one way and the other; the open ones lie on its edges.
        for ((p, _), count) in edges.iter().filter(|(_, count)| **count != 0) {
            let p = p.map(f32::from_bits);
            let edge = [p[0], p[2]].iter().any(|v| ((v / 0.5) - 32.0).abs() < 1e-4 || ((v / 0.5) - 48.0).abs() < 1e-4);
            assert!(edge, "an open edge at {p:?} ({count}) inside the column");
        }
    }

    #[test]
    fn rays_and_the_ground_under_a_point_find_the_floor_of_a_pit() {
        let heights = field();
        let mut dug = Excavation::default();
        let ground = heights.height_at(20.0, 20.0);
        dug.carve(&heights, &Shape::Ball { at: [20.0, ground, 20.0], radius: 2.0 }, Carve::Dig, None);
        // Straight down into it: its floor two metres under the ground.
        let (floor, up) = dug.surface_at(&heights, 20.0, 20.0).expect("a dug column");
        assert!((floor - (ground - 2.0)).abs() < 0.1, "{floor} against {}", ground - 2.0);
        assert!(up.y > 0.9);
        // A ray across the pit's mouth meets its far wall, facing back.
        let hit = dug.raycast(&heights, Vec3::new(17.0, ground - 1.0, 20.0), Vec3::X, 10.0).expect("its wall");
        assert!(hit.point.x > 21.0 && hit.point.x < 22.0 && hit.normal.x < -0.5, "{hit:?}");
        // Off the dug columns, nothing.
        assert!(dug.surface_at(&heights, 2.0, 2.0).is_none());
    }

    #[test]
    fn a_trench_along_a_spline_has_its_floor_fire_step_and_parapet() {
        use crate::layers::{Layer, LayerKind};
        let heights = Heightmap::flat(1, 1, 0.5, 0.0);
        // A trench along z, its front on the left going along, which is -x: floor 1 m wide, 2 m down.
        let spline = LayerKind::Spline {
            points: vec![[64.0, 0.0, 40.0], [64.0, 0.0, 90.0]],
            width: 1.0,
            falloff: 1.0,
            dig: Some(Trench::default()),
        };
        let (holed, dug) = dig_layers(&[Layer::new(spline)], &heights);
        let (holed, dug) = (holed.expect("cut"), dug.expect("dug"));
        assert!(holed.is_hole(128, 130));
        let floor = |x: f32| dug.surface_at(&heights, x, 64.0).map(|(h, _)| h);
        let at = |x: f32| floor(x).unwrap_or(f32::NAN);
        assert!((at(64.3) + 2.0).abs() < 0.15, "its floor 2 m down at the back: {}", at(64.3));
        assert!((at(63.75) + 1.4).abs() < 0.15, "the fire step 0.6 m up at the front: {}", at(63.75));
        // Heaped 1.3 m out in front of the line, beyond the cut's lip at 63.3 m.
        let parapet = (0..20).map(|i| at(63.2 - i as f32 * 0.1)).fold(f32::MIN, f32::max);
        assert!((0.3..0.8).contains(&parapet), "the parapet heaped up in front: {parapet}");
        assert!(at(66.5).abs() < 0.05, "and the ground behind as it was: {}", at(66.5));
    }

    #[test]
    fn a_dig_layers_strokes_read_back_and_dig_as_they_were_made() {
        use crate::layers::{Layer, LayerKind};
        let heights = field();
        let ground = heights.height_at(30.0, 30.0);
        let strokes = vec![
            DigStroke { at: [30.0, ground, 30.0], radius: 1.5, fill: false, smooth: 0.0, surface: Some(1) },
            DigStroke { at: [31.0, ground - 0.5, 30.0], radius: 0.8, fill: true, smooth: 0.0, surface: None },
            DigStroke { at: [30.5, ground, 30.0], radius: 1.5, fill: false, smooth: 0.7, surface: None },
        ];
        let strokes_kept = strokes.clone();
        let layer = Layer::new(LayerKind::Dig { strokes: strokes.clone() });
        let text = toml::to_string(&layer).unwrap();
        assert_eq!(toml::from_str::<Layer>(&text).unwrap(), layer);
        let (_, by_layer) = dig_layers(&[layer], &heights);
        let mut by_hand = Excavation::default();
        for stroke in strokes {
            let ball = Shape::Ball { at: stroke.at, radius: stroke.radius };
            by_hand.carve(&heights, &ball, stroke.carve(), stroke.surface);
        }
        assert_eq!(by_layer, Some(by_hand));
        // Kept in a file without the holes it cut, which digging cuts again as it loads.
        let (holed, Some(dug)) = dig_layers(&[Layer::new(LayerKind::Dig { strokes: vec![strokes_kept[0]] })], &heights)
        else {
            panic!("dug")
        };
        let holed = holed.expect("cut");
        assert!(holed.has_holes());
        assert!(!undug(&holed, &dug, |_, _| None).has_holes(), "none left");
        // Digging lays no heights, so the heights' hash ignores it.
        let plain = crate::layers::hash_layers(0.0, &[]);
        let dug = Layer::new(LayerKind::Dig { strokes: Vec::new() });
        assert_eq!(crate::layers::hash_layers(0.0, &[dug]), plain);
    }

    #[test]
    fn smoothing_rounds_a_trenchs_lip_and_leaves_the_ground_not_dug_as_it_was() {
        let heights = Heightmap::flat(1, 1, 0.5, 0.1);
        let mut dug = Excavation::default();
        // Straight walls 2 m deep from x 61.5 to 63.5 m, all in the column from 56 to 64 m: the one beyond not dug.
        let profile = Trench {
            batter: 0.0,
            step_height: 0.0,
            step_width: 0.0,
            parapet: 0.0,
            surface: Some(3),
            ..Trench::default()
        };
        let trench = Shape::Trench { from: [62.5, 0.1, 50.0], to: [62.5, 0.1, 70.0], floor: 2.0, profile };
        dug.carve(&heights, &trench, Carve::Dig, profile.surface);
        assert!(dug.is_dug(7, 7) && !dug.is_dug(8, 7));
        let top = |dug: &Excavation, x: f32| dug.surface_at(&heights, x, 60.0).map_or(f32::NAN, |(h, _)| h);
        assert!((top(&dug, 63.75) - 0.1).abs() < 1e-3, "a sharp lip: {}", top(&dug, 63.75));
        // The corners on the face it shares with the column not dug, at 64 m.
        let face = |dug: &Excavation| -> Vec<f32> {
            let corners = (-12..4).flat_map(|y| (200..280).map(move |z| (y, z)));
            corners.map(|(y, z)| dug.held(256, y, z).expect("held by the dug column").0).collect()
        };
        let (face_before, before) = (face(&dug), dug.clone());
        let ball = Shape::Ball { at: [63.5, 0.1, 60.0], radius: 2.0 };
        for _ in 0..4 {
            let (holed, changed) = dug.carve(&heights, &ball, Carve::Smooth { amount: 1.0 }, None).expect("reached");
            assert!(holed.is_none() && !changed.is_empty(), "smoothing digs no more columns");
        }
        assert!(top(&dug, 63.75) < 0.07, "the lip rounded off: {}", top(&dug, 63.75));
        assert!((top(&dug, 62.5) + 1.9).abs() < 0.05, "the floor beyond its reach as it was: {}", top(&dug, 62.5));
        assert_eq!(face(&dug), face_before, "the face shared with ground not dug untouched");
        assert!(!dug.is_dug(8, 7));
        // Smoothing nothing dug changes nothing.
        let mut plain = before.clone();
        let away = Shape::Ball { at: [20.0, 0.1, 20.0], radius: 2.0 };
        assert_eq!(plain.carve(&heights, &away, Carve::Smooth { amount: 1.0 }, None).map(|(_, c)| c), Some(Vec::new()));
        assert_eq!(plain, before);
        // Still whole: every edge inside the dug column shared, as a dig leaves it.
        let mut edges: HashMap<([u32; 3], [u32; 3]), i32> = HashMap::new();
        for &key in dug.chunks().keys() {
            let mesh = mesh(&dug, &heights, key);
            for [a, b, c] in mesh.indices.chunks(3).map(|t| [t[0], t[1], t[2]]) {
                for (from, to) in [(a, b), (b, c), (c, a)] {
                    let (p, q) = (mesh.positions[from as usize], mesh.positions[to as usize]);
                    let (p, q) = (p.map(f32::to_bits), q.map(f32::to_bits));
                    *edges.entry((p, q)).or_default() += 1;
                    *edges.entry((q, p)).or_default() -= 1;
                }
            }
        }
        for ((p, _), count) in edges.iter().filter(|(_, count)| **count != 0) {
            let p = p.map(f32::from_bits);
            let edge = [p[0], p[2]].iter().any(|v| (v / 8.0 - (v / 8.0).round()).abs() < 1e-4);
            assert!(edge, "an open edge at {p:?} ({count}) inside the dug columns");
        }
    }

    #[test]
    fn digging_is_the_same_every_time() {
        let heights = field();
        let dig = |shape: Shape| {
            let mut dug = Excavation::default();
            dug.carve(&heights, &shape, Carve::Dig, None);
            dug
        };
        let tunnel = Shape::Capsule { from: [10.0, 1.0, 10.0], to: [30.0, 1.5, 12.0], radius: 1.0 };
        assert_eq!(dig(tunnel), dig(tunnel));
        assert!(dig(Shape::Ball { at: [-50.0, 0.0, 10.0], radius: 2.0 }).is_empty(), "off the terrain");
        assert!(!Shape::Ball { at: [f32::NAN, 0.0, 0.0], radius: 1.0 }.is_sound());
    }
}
