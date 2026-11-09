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

use crate::heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_QUADS, TILE_SAMPLES};

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

/// A terrain's dug ground: every chunk of every dug column, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Excavation {
    chunks: BTreeMap<ChunkKey, Arc<DugChunk>>,
}

/// A shape that digs or fills: a ball, or a capsule from one point to
/// another, in metres from the terrain's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Ball { at: [f32; 3], radius: f32 },
    Capsule { from: [f32; 3], to: [f32; 3], radius: f32 },
}

impl Shape {
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
        }
    }

    /// Whether every number is one and it has some size, as one from
    /// another machine may not.
    pub fn is_sound(&self) -> bool {
        let numbers: Vec<f32> = match *self {
            Shape::Ball { at, radius } => vec![at[0], at[1], at[2], radius],
            Shape::Capsule { from, to, radius } => vec![from[0], from[1], from[2], to[0], to[1], to[2], radius],
        };
        numbers.iter().all(|v| v.is_finite()) && numbers.last().is_some_and(|r| *r > 0.0)
    }
}

/// Whether a shape takes the ground away or puts it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carve {
    Dig,
    Fill,
}

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
        Self { chunks }
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
                            Carve::Dig => was.min(distance.max(-band)),
                            Carve::Fill => was.max((-distance).min(band)),
                        });
                        let now = match carve {
                            Carve::Dig => now.min(was),
                            Carve::Fill => now.max(was),
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
