//! Light kept for each block of a world of cubes: two lights a block, each
//! from 0 to [`MOST`].
//!
//! **Sky light** comes straight down from open sky at its full 15 through
//! blocks that let light through whole (air, water, glass), and spreads out
//! from there a step less each block: sideways, round corners and in under
//! overhangs. Leaves and blocks of parts take it in and pass it on a step
//! less, as air does, but never let it straight down, so the ground under a
//! crown is in its shade. Ground stops it.
//!
//! **Block light** floods out from blocks that give light (a torch, lava, a
//! lamp; `BlockFaces::light`) the same way, a step less each block.
//!
//! Light comes from the blocks alone, so it is never saved, and saves and
//! what goes over the network stay as small as the blocks' changes. A box's
//! light is worked out exactly from the blocks within [`MOST`] of it and
//! what lies over it ([`solve`]): a world's columns are lit as their faces
//! are made, a world without end's from the eight columns round each (see
//! `stream`), and a change floods its light again round where it was made
//! ([`LightMap::relight`]), its chunks drawn again where their light
//! changed.
//!
//! The mesher lights each corner of a face by the blocks before it that
//! touch that corner (see `mesh`), and the renderer lights the face by
//! that: the sky's light by its sky light, a warm light by its block light.

use std::{collections::HashMap, sync::Arc};

use super::{
    BlockFaces, BlockMap, PART_DRAWN,
    chunk::{AIR, BlockId, CHUNK, CHUNK_LEN},
};

/// A block's light: its sky light in the high four bits and its block
/// light in the low four.
pub type Light = u8;

/// The most light a block has of either, and how many blocks light reaches
/// at most from where it is that bright.
pub const MOST: u8 = 15;

/// The light of a block under open sky, near no block giving light: what a
/// chunk whose light is not kept has.
pub const OPEN_SKY: Light = MOST << 4;

/// The sky light of `light`.
pub const fn sky_of(light: Light) -> u8 {
    light >> 4
}

/// The block light of `light`.
pub const fn glow_of(light: Light) -> u8 {
    light & 15
}

/// A block's light of `sky` light and `glow` block light.
pub fn light_of(sky: u8, glow: u8) -> Light {
    (sky.min(MOST) << 4) | glow.min(MOST)
}

/// How a block lets light through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passes {
    /// Ground: no light goes into it or through it.
    Stops,
    /// Air, water, glass, a vine: light passes a step less each block, and
    /// the sky's straight down at its full.
    Clear,
    /// Leaves and blocks of parts: light passes a step less each block,
    /// but the sky's not straight down.
    Shades,
}

/// How block `block` lets light through, by `faces`: ground that hides
/// what is behind it stops it; leaves and blocks of parts shade; anything
/// else lets it through.
pub fn passes(faces: &[BlockFaces], block: BlockId) -> Passes {
    if block == AIR {
        return Passes::Clear;
    }
    let Some(kind) = faces.get(usize::from(block)) else { return Passes::Clear };
    if kind.water || kind.shape.is_vine() {
        return Passes::Clear;
    }
    if kind.part & PART_DRAWN != 0 {
        return Passes::Shades;
    }
    if kind.see_through {
        let leaves = kind.sways || kind.shape == super::block::BlockShape::Bushy;
        return if leaves { Passes::Shades } else { Passes::Clear };
    }
    Passes::Stops
}

/// One chunk's light, each block's in the order a chunk keeps its blocks
/// (along x, then z, then up), or one light for all of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkLight {
    All(Light),
    Each(Box<[Light]>),
}

impl ChunkLight {
    /// The light of block `(x, y, z)` of the chunk, each from 0 to `CHUNK - 1`.
    pub fn get(&self, x: u32, y: u32, z: u32) -> Light {
        match self {
            Self::All(light) => *light,
            Self::Each(each) => each[((y * CHUNK + z) * CHUNK + x) as usize],
        }
    }

    /// The light of every block `each` gives, kept as one where they are
    /// all the same.
    fn of(each: Vec<Light>) -> Self {
        match each.iter().all(|&light| light == each[0]) {
            true => Self::All(each[0]),
            false => Self::Each(each.into_boxed_slice()),
        }
    }

    /// How many bytes it keeps.
    pub fn bytes(&self) -> usize {
        match self {
            Self::All(_) => 1,
            Self::Each(each) => each.len(),
        }
    }
}

/// Where the mesher finds the light of the blocks round a chunk: a world's
/// [`LightMap`], or a [`LightBox`] worked out for the chunk.
pub trait LightAt: Sync {
    /// The light of each block of chunk `key` and of those just past it, a
    /// block each way: `PADDED` blocks a side, along x, then z, then up.
    fn padded(&self, key: [i32; 3]) -> Vec<Light>;
}

/// How many blocks a side a chunk's blocks and those just past it are.
pub const PADDED: usize = CHUNK as usize + 2;

/// Every chunk's light in a world, by chunk, where it is not all open sky.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightMap {
    chunks: HashMap<[i32; 3], Arc<ChunkLight>>,
}

impl LightMap {
    /// The light of block `at`, counted in blocks from the world's corner.
    pub fn get(&self, at: [i32; 3]) -> Light {
        let (key, [x, y, z]) = BlockMap::chunk_of(at);
        self.chunks.get(&key).map_or(OPEN_SKY, |light| light.get(x, y, z))
    }

    /// Chunk `key`'s light, if it is not all open sky.
    pub fn chunk(&self, key: [i32; 3]) -> Option<&Arc<ChunkLight>> {
        self.chunks.get(&key)
    }

    /// Puts chunk `key`'s light, or forgets it where it is all open sky.
    pub fn put(&mut self, key: [i32; 3], light: ChunkLight) {
        match light {
            ChunkLight::All(OPEN_SKY) => drop(self.chunks.remove(&key)),
            light => drop(self.chunks.insert(key, Arc::new(light))),
        }
    }

    /// Forgets the light of every chunk of chunk columns `columns`, by place
    /// along x and z, as when they are let go.
    pub fn forget_columns(&mut self, columns: &[[i32; 2]]) {
        let columns: std::collections::HashSet<[i32; 2]> = columns.iter().copied().collect();
        self.chunks.retain(|key, _| !columns.contains(&[key[0], key[2]]));
    }

    /// Every chunk's light it keeps, by where it is.
    pub fn chunks(&self) -> impl Iterator<Item = (&[i32; 3], &Arc<ChunkLight>)> {
        self.chunks.iter()
    }

    /// How many chunks' light it keeps, and how many bytes they take.
    pub fn size(&self) -> (usize, usize) {
        (self.chunks.len(), self.chunks.values().map(|light| light.bytes()).sum())
    }

    /// The light of every chunk of `map`, each column worked out apart on
    /// every core, from the lowest chunk of the map to its highest.
    pub fn of(map: &BlockMap, faces: &[BlockFaces]) -> Self {
        voxear_core::profile_scope!("Block Light");
        let mut columns: Vec<[i32; 2]> = map.chunks().map(|(key, _)| [key[0], key[2]]).collect();
        columns.sort_unstable();
        columns.dedup();
        let Some(heights) = heights(map) else { return Self::default() };
        let lit = crate::workers::each(&columns, |&column| light_column(map, faces, column, heights).0);
        let mut light = Self::default();
        for (key, chunk) in lit.into_iter().flatten() {
            light.put(key, chunk);
        }
        light
    }

    /// Floods the light again round blocks `changed` of `map`, as `map` has
    /// them now: every block whose light they may change is worked out
    /// again from the blocks round it. The chunks to draw again for it, by
    /// where they are: those holding a block whose light changed, and those
    /// it lies just past.
    pub fn relight(&mut self, map: &BlockMap, faces: &[BlockFaces], changed: &[[i32; 3]]) -> Vec<[i32; 3]> {
        voxear_core::profile_scope!("Block Light");
        let Some(first) = changed.first() else { return Vec::new() };
        // Light reaches as far as MOST from each, and from the bottom of the sky's way down past it: down as far as
        // the blocks under it let the sky straight through, now or before.
        let reach = i32::from(MOST);
        let floor = heights(map).map_or(0, |(low, _)| low * CHUNK as i32);
        let (mut low, mut high) = (*first, *first);
        for &at in changed {
            let mut bottom = at[1];
            while bottom > floor && passes(faces, map.get([at[0], bottom - 1, at[2]])) == Passes::Clear {
                bottom -= 1;
            }
            for axis in 0..3 {
                low[axis] = low[axis].min(at[axis]);
                high[axis] = high[axis].max(at[axis]);
            }
            low[1] = low[1].min(bottom);
        }
        let (low, high) = (low.map(|v| v - reach), high.map(|v| v + reach + 1));
        let solved = solve(map, faces, low, high);
        let mut drawn: Vec<[i32; 3]> = Vec::new();
        let side = CHUNK as i32;
        let (from, to) = (low.map(|v| v.div_euclid(side)), high.map(|v| (v - 1).div_euclid(side)));
        for cy in from[1]..=to[1] {
            for cz in from[2]..=to[2] {
                for cx in from[0]..=to[0] {
                    let key = [cx, cy, cz];
                    let base = key.map(|v| v * side);
                    let old = self.chunks.get(&key).cloned();
                    let mut each: Vec<Light> = (0..CHUNK_LEN)
                        .map(|i| {
                            let (x, rest) = (i as u32 % CHUNK, i as u32 / CHUNK);
                            old.as_deref().map_or(OPEN_SKY, |light| light.get(x, rest / CHUNK, rest % CHUNK))
                        })
                        .collect();
                    // Which way each block whose light changed lies on its chunk's sides, for the chunks beside it.
                    let mut sides = [[false; 2]; 3];
                    let mut any = false;
                    for y in base[1].max(low[1])..(base[1] + side).min(high[1]) {
                        for z in base[2].max(low[2])..(base[2] + side).min(high[2]) {
                            for x in base[0].max(low[0])..(base[0] + side).min(high[0]) {
                                let now = solved.get([x, y, z]).unwrap_or(OPEN_SKY);
                                let local = [x - base[0], y - base[1], z - base[2]];
                                let index = ((local[1] * side + local[2]) * side + local[0]) as usize;
                                if each[index] == now {
                                    continue;
                                }
                                each[index] = now;
                                any = true;
                                for axis in 0..3 {
                                    sides[axis][0] |= local[axis] == 0;
                                    sides[axis][1] |= local[axis] == side - 1;
                                }
                            }
                        }
                    }
                    if !any {
                        continue;
                    }
                    self.put(key, ChunkLight::of(each));
                    let near = |axis: usize| {
                        let mut near = vec![key[axis]];
                        near.extend(sides[axis][0].then_some(key[axis] - 1));
                        near.extend(sides[axis][1].then_some(key[axis] + 1));
                        near
                    };
                    for x in near(0) {
                        for y in near(1) {
                            for z in near(2) {
                                drawn.push([x, y, z]);
                            }
                        }
                    }
                }
            }
        }
        drawn.sort_unstable();
        drawn.dedup();
        drawn
    }
}

impl LightAt for LightMap {
    fn padded(&self, key: [i32; 3]) -> Vec<Light> {
        let mut padded = vec![OPEN_SKY; PADDED * PADDED * PADDED];
        let side = CHUNK as i32;
        let base = key.map(|v| v * side - 1);
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let near = [key[0] + dx, key[1] + dy, key[2] + dz];
                    let Some(light) = self.chunks.get(&near) else { continue };
                    let corner = near.map(|v| v * side);
                    // The padded blocks within this chunk, in padded places.
                    let span = |axis: usize| {
                        let from = (corner[axis] - base[axis]).max(0);
                        let to = (corner[axis] + side - base[axis]).min(PADDED as i32);
                        from..to
                    };
                    for y in span(1) {
                        for z in span(2) {
                            for x in span(0) {
                                let local = [x + base[0] - corner[0], y + base[1] - corner[1], z + base[2] - corner[2]];
                                let at = ((y as usize * PADDED) + z as usize) * PADDED + x as usize;
                                padded[at] = light.get(local[0] as u32, local[1] as u32, local[2] as u32);
                            }
                        }
                    }
                }
            }
        }
        padded
    }
}

/// The light worked out for a box of a world's blocks: from its lowest
/// corner `low`, `size` blocks along x, y and z.
#[derive(Clone, Debug, PartialEq)]
pub struct LightBox {
    pub low: [i32; 3],
    pub size: [i32; 3],
    light: Vec<Light>,
}

impl LightBox {
    /// The light of block `at`, counted in blocks from the world's corner,
    /// where the box holds it.
    pub fn get(&self, at: [i32; 3]) -> Option<Light> {
        let p = [0, 1, 2].map(|i| at[i] - self.low[i]);
        if (0..3).any(|i| p[i] < 0 || p[i] >= self.size[i]) {
            return None;
        }
        Some(self.light[((p[1] * self.size[2] + p[2]) * self.size[0] + p[0]) as usize])
    }

    /// Chunk `key`'s light, where the box holds the whole chunk.
    pub fn chunk(&self, key: [i32; 3]) -> Option<ChunkLight> {
        let side = CHUNK as i32;
        let base = key.map(|v| v * side);
        if (0..3).any(|i| base[i] < self.low[i] || base[i] + side > self.low[i] + self.size[i]) {
            return None;
        }
        let mut each = Vec::with_capacity(CHUNK_LEN);
        let p = [0, 1, 2].map(|i| base[i] - self.low[i]);
        for y in p[1]..p[1] + side {
            for z in p[2]..p[2] + side {
                let start = ((y * self.size[2] + z) * self.size[0] + p[0]) as usize;
                each.extend_from_slice(&self.light[start..start + CHUNK as usize]);
            }
        }
        Some(ChunkLight::of(each))
    }
}

impl LightAt for LightBox {
    fn padded(&self, key: [i32; 3]) -> Vec<Light> {
        let base = key.map(|v| v * CHUNK as i32 - 1);
        let mut padded = Vec::with_capacity(PADDED * PADDED * PADDED);
        for y in 0..PADDED as i32 {
            for z in 0..PADDED as i32 {
                for x in 0..PADDED as i32 {
                    padded.push(self.get([base[0] + x, base[1] + y, base[2] + z]).unwrap_or(OPEN_SKY));
                }
            }
        }
        padded
    }
}

/// What a block that stops light reads as in a [`volume`]: no sky light
/// and all the block light there is, which no block that lets light in has
/// but one that gives the most light deep in a cave.
pub const VOLUME_SOLID: Light = 0x0F;

/// The light of every block of the box from `low`, `size` blocks along x, y
/// and z, a byte each along x, then z, then up, as `light` keeps it, each
/// block that stops light [`VOLUME_SOLID`]: for the renderer, which lights
/// what is not a block by the light round it.
pub fn volume(map: &BlockMap, faces: &[BlockFaces], light: &LightMap, low: [i32; 3], size: [i32; 3]) -> Vec<Light> {
    voxear_core::profile_scope!("Block Light Volume");
    let [nx, ny, nz] = size.map(|v| v.max(0) as usize);
    let mut out = vec![OPEN_SKY; nx * ny * nz];
    let side = CHUNK as i32;
    let high = [0, 1, 2].map(|i| low[i] + size[i]);
    let (first, last) = (low.map(|v| v.div_euclid(side)), high.map(|v| (v - 1).div_euclid(side)));
    let stops: Vec<bool> = (0..faces.len().max(1)).map(|id| passes(faces, id as BlockId) == Passes::Stops).collect();
    for cy in first[1]..=last[1] {
        for cz in first[2]..=last[2] {
            for cx in first[0]..=last[0] {
                let key = [cx, cy, cz];
                let (chunk, lit) = (map.chunk(key), light.chunks.get(&key));
                if chunk.is_none() && lit.is_none() {
                    continue;
                }
                let solid =
                    chunk.filter(|chunk| chunk.may_hold(|id| stops.get(usize::from(id)).copied().unwrap_or(false)));
                let base = key.map(|v| v * side);
                for y in base[1].max(low[1])..(base[1] + side).min(high[1]) {
                    for z in base[2].max(low[2])..(base[2] + side).min(high[2]) {
                        let row = ((y - low[1]) as usize * nz + (z - low[2]) as usize) * nx;
                        for x in base[0].max(low[0])..(base[0] + side).min(high[0]) {
                            let local = [x - base[0], y - base[1], z - base[2]].map(|v| v as u32);
                            let at = row + (x - low[0]) as usize;
                            let stopped = solid.is_some_and(|chunk| {
                                stops
                                    .get(usize::from(chunk.get(local[0], local[1], local[2])))
                                    .copied()
                                    .unwrap_or(false)
                            });
                            out[at] = match (stopped, lit) {
                                (true, _) => VOLUME_SOLID,
                                (false, Some(lit)) => lit.get(local[0], local[1], local[2]),
                                (false, None) => OPEN_SKY,
                            };
                        }
                    }
                }
            }
        }
    }
    out
}

/// The lowest and highest chunk of `map` along y, by place in chunks; none
/// for a map of no chunks.
pub fn heights(map: &BlockMap) -> Option<(i32, i32)> {
    let ys = map.chunks().map(|(key, _)| key[1]);
    ys.fold(None, |range, y| Some(range.map_or((y, y), |(low, high): (i32, i32)| (low.min(y), high.max(y)))))
}

/// The light of chunk column `column` of `map`, by place along x and z,
/// from chunk `heights.0` up to chunk `heights.1`, worked out from the
/// blocks round it, so `map` holds the columns round it too: each chunk's,
/// and the box of it and the blocks just past it, a block each way, for its
/// faces.
pub fn light_column(
    map: &BlockMap,
    faces: &[BlockFaces],
    column: [i32; 2],
    (bottom, top): (i32, i32),
) -> (Vec<([i32; 3], ChunkLight)>, LightBox) {
    voxear_core::profile_scope!("Block Light Column");
    let side = CHUNK as i32;
    let low = [column[0] * side - 1, bottom * side - 1, column[1] * side - 1];
    let high = [column[0] * side + side + 1, top * side + side + 1, column[1] * side + side + 1];
    let solved = solve(map, faces, low, high);
    let chunks = (bottom..=top)
        .map(|y| [column[0], y, column[1]])
        .filter_map(|key| Some((key, solved.chunk(key)?)))
        .filter(|(_, light)| *light != ChunkLight::All(OPEN_SKY))
        .collect();
    (chunks, solved)
}

/// The light of every block of the box from `low` up to `high` of `map`,
/// worked out from the blocks within [`MOST`] of it and what is over them:
/// exact wherever `map` holds the blocks round the box, as no light
/// reaches further.
pub fn solve(map: &BlockMap, faces: &[BlockFaces], low: [i32; 3], high: [i32; 3]) -> LightBox {
    let reach = i32::from(MOST);
    let (from, to) = (low.map(|v| v - reach), high.map(|v| v + reach));
    let n = [0, 1, 2].map(|i| (to[i] - from[i]).max(0) as usize);
    let (nx, ny, nz) = (n[0], n[1], n[2]);
    let count = nx * ny * nz;
    let index = |x: usize, y: usize, z: usize| (y * nz + z) * nx + x;
    // How each block lets light through and the light it gives, by its number.
    let pass_of: Vec<Passes> = (0..faces.len().max(1)).map(|id| passes(faces, id as BlockId)).collect();
    let glow_of: Vec<u8> =
        (0..faces.len().max(1)).map(|id| faces.get(id).map_or(0, |kind| kind.light.min(MOST))).collect();
    let glows = glow_of.iter().any(|&glow| glow > 0);
    let mut pass = vec![Passes::Clear; count];
    let mut glow = vec![0u8; if glows { count } else { 0 }];
    let side = CHUNK as i32;
    let (first, last) = (from.map(|v| v.div_euclid(side)), to.map(|v| (v - 1).div_euclid(side)));
    let pass_at = |id: BlockId| pass_of.get(usize::from(id)).copied().unwrap_or(Passes::Clear);
    for cy in first[1]..=last[1] {
        for cz in first[2]..=last[2] {
            for cx in first[0]..=last[0] {
                let Some(chunk) = map.chunk([cx, cy, cz]) else { continue };
                if chunk.is_air() {
                    continue;
                }
                let base = [cx * side, cy * side, cz * side];
                let span = |axis: usize| base[axis].max(from[axis])..(base[axis] + side).min(to[axis]);
                let gives = glows && chunk.may_hold(|id| glow_of.get(usize::from(id)).is_some_and(|&glow| glow > 0));
                for y in span(1) {
                    for z in span(2) {
                        for x in span(0) {
                            let id = chunk.get((x - base[0]) as u32, (y - base[1]) as u32, (z - base[2]) as u32);
                            let at = index((x - from[0]) as usize, (y - from[1]) as usize, (z - from[2]) as usize);
                            pass[at] = pass_at(id);
                            if gives {
                                glow[at] = glow_of.get(usize::from(id)).copied().unwrap_or(0);
                            }
                        }
                    }
                }
            }
        }
    }
    // Which columns of the box are open to the sky over it: nothing over it but blocks that let the sky through.
    let mut open = vec![true; nx * nz];
    if let Some((_, top)) = heights(map).filter(|&(_, top)| (top + 1) * side > to[1]) {
        for cz in first[2]..=last[2] {
            for cx in first[0]..=last[0] {
                for cy in (to[1].div_euclid(side)..=top).rev() {
                    let Some(chunk) = map.chunk([cx, cy, cz]) else { continue };
                    if chunk.is_air() {
                        continue;
                    }
                    let base = [cx * side, cy * side, cz * side];
                    for z in base[2].max(from[2])..(base[2] + side).min(to[2]) {
                        for x in base[0].max(from[0])..(base[0] + side).min(to[0]) {
                            let column = (z - from[2]) as usize * nx + (x - from[0]) as usize;
                            if !open[column] {
                                continue;
                            }
                            let lowest = base[1].max(to[1]);
                            let shut = (lowest..base[1] + side).rev().any(|y| {
                                let id = chunk.get((x - base[0]) as u32, (y - base[1]) as u32, (z - base[2]) as u32);
                                pass_at(id) != Passes::Clear
                            });
                            open[column] &= !shut;
                        }
                    }
                }
            }
        }
    }
    // The sky straight down each open column, as far as the first block that does not let it through whole.
    let mut sky = vec![0u8; count];
    let mut floor = vec![ny; nx * nz];
    for z in 0..nz {
        for x in 0..nx {
            if !open[z * nx + x] {
                continue;
            }
            for y in (0..ny).rev() {
                let at = index(x, y, z);
                if pass[at] != Passes::Clear {
                    break;
                }
                sky[at] = MOST;
                floor[z * nx + x] = y;
            }
        }
    }
    // It spreads from where it meets blocks it does not reach straight: under its lowest, and beside it down to
    // where the sky reaches as low beside it.
    let mut queues: [Vec<u32>; MOST as usize + 1] = Default::default();
    for z in 0..nz {
        for x in 0..nx {
            let lowest = floor[z * nx + x];
            if lowest >= ny {
                continue;
            }
            let beside = [(x > 0).then(|| floor[z * nx + x - 1]), (x + 1 < nx).then(|| floor[z * nx + x + 1])];
            let beside = beside
                .into_iter()
                .chain([(z > 0).then(|| floor[(z - 1) * nx + x]), (z + 1 < nz).then(|| floor[(z + 1) * nx + x])]);
            let reached = beside.flatten().max().unwrap_or(lowest).clamp(lowest + 1, ny);
            for y in lowest..reached {
                queues[MOST as usize].push(index(x, y, z) as u32);
            }
        }
    }
    flood(&mut sky, &pass, n, &mut queues);
    // Block light floods out from each block that gives it.
    let mut lamp = vec![0u8; count];
    if glows {
        for (at, &light) in glow.iter().enumerate().filter(|(_, light)| **light > 0) {
            lamp[at] = light;
            queues[usize::from(light)].push(at as u32);
        }
        flood(&mut lamp, &pass, n, &mut queues);
    }
    // The box's own blocks.
    let size = [0, 1, 2].map(|i| (high[i] - low[i]).max(0));
    let mut light = Vec::with_capacity((size[0] * size[1] * size[2]) as usize);
    let at = |v: i32, axis: usize| (v - from[axis]) as usize;
    for y in low[1]..high[1] {
        for z in low[2]..high[2] {
            let row = index(at(low[0], 0), at(y, 1), at(z, 2));
            for i in 0..size[0] as usize {
                light.push(light_of(sky[row + i], lamp[row + i]));
            }
        }
    }
    LightBox { low, size, light }
}

/// Floods `light` out from the blocks in `queues`, each queued by its light
/// and as bright as that, through the blocks `pass` lets light into, a step
/// less each block: brightest first, so each block is reached first by its
/// brightest way.
fn flood(light: &mut [u8], pass: &[Passes], [nx, ny, nz]: [usize; 3], queues: &mut [Vec<u32>; MOST as usize + 1]) {
    for level in (2..=MOST as usize).rev() {
        let queue = std::mem::take(&mut queues[level]);
        let next = (level - 1) as u8;
        for &at in &queue {
            let at = at as usize;
            if usize::from(light[at]) != level {
                continue;
            }
            let (x, rest) = (at % nx, at / nx);
            let (z, y) = (rest % nz, rest / nz);
            let mut reach = |to: usize| {
                if pass[to] != Passes::Stops && light[to] < next {
                    light[to] = next;
                    queues[usize::from(next)].push(to as u32);
                }
            };
            if x > 0 {
                reach(at - 1);
            }
            if x + 1 < nx {
                reach(at + 1);
            }
            if z > 0 {
                reach(at - nx);
            }
            if z + 1 < nz {
                reach(at + nx);
            }
            if y > 0 {
                reach(at - nx * nz);
            }
            if y + 1 < ny {
                reach(at + nx * nz);
            }
        }
        // Kept for the next box, its room and all.
        queues[level] = queue;
        queues[level].clear();
    }
    queues.iter_mut().for_each(Vec::clear);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::BlockFaces;

    const STONE: BlockId = 1;
    const LEAVES: BlockId = 2;
    const TORCH: BlockId = 3;
    const GLASS: BlockId = 4;

    fn faces() -> Vec<BlockFaces> {
        let stone = BlockFaces::new(0, 0, 0);
        let leaves = BlockFaces { see_through: true, sways: true, ..BlockFaces::new(1, 1, 1) };
        let torch = BlockFaces { light: 14, ..BlockFaces::new(2, 2, 2) };
        let glass = BlockFaces { see_through: true, ..BlockFaces::new(3, 3, 3) };
        vec![BlockFaces::default(), stone, leaves, torch, glass]
    }

    /// A floor of stone 4 blocks thick under open sky, 64 blocks a side, from y 0.
    fn floor() -> BlockMap {
        let mut map = BlockMap::default();
        for x in -32..32 {
            for z in -32..32 {
                for y in 0..4 {
                    map.set([x, y, z], STONE);
                }
            }
        }
        map
    }

    #[test]
    fn open_sky_is_full_down_to_the_ground_and_dark_in_it() {
        let map = floor();
        let light = LightMap::of(&map, &faces());
        assert_eq!(light.get([0, 4, 0]), OPEN_SKY, "on the floor");
        assert_eq!(light.get([5, 40, 5]), OPEN_SKY, "high over it");
        assert_eq!(sky_of(light.get([0, 2, 0])), 0, "in the stone");
    }

    #[test]
    fn under_a_roof_the_sky_falls_a_step_a_block_in_from_its_edge() {
        let mut map = floor();
        // A roof 3 blocks over the floor from x 0 to 20, the whole way along z.
        for x in 0..=20 {
            for z in -32..32 {
                map.set([x, 7, z], STONE);
            }
        }
        let light = LightMap::of(&map, &faces());
        let sky_at = |x: i32| sky_of(light.get([x, 4, 0]));
        assert_eq!(sky_at(-1), 15, "in the open");
        assert_eq!((sky_at(0), sky_at(1), sky_at(5)), (14, 13, 9), "a step less each block in");
        assert_eq!(sky_at(10), 4, "deepest in the middle, the sky reaching from both edges");
        assert_eq!(sky_at(10), sky_of(light.get([10, 5, 0])), "as much higher under the roof");
    }

    #[test]
    fn a_sealed_cave_is_dark_and_a_torch_lights_it_a_step_less_each_block() {
        let mut map = BlockMap::default();
        for x in -20..20 {
            for y in -20..20 {
                for z in -20..20 {
                    map.set([x, y, z], STONE);
                }
            }
        }
        for x in -6..6 {
            for y in -3..3 {
                for z in -6..6 {
                    map.set([x, y, z], AIR);
                }
            }
        }
        let light = LightMap::of(&map, &faces());
        assert_eq!(light.get([0, 0, 0]), light_of(0, 0), "no light in it");
        map.set([0, -3, 0], TORCH);
        let mut lit = light.clone();
        let drawn = lit.relight(&map, &faces(), &[[0, -3, 0]]);
        assert_eq!(glow_of(lit.get([0, -3, 0])), 14, "the torch itself");
        assert_eq!(glow_of(lit.get([0, -2, 0])), 13, "beside it");
        assert_eq!(glow_of(lit.get([3, -2, 2])), 14 - 1 - 3 - 2, "round a few blocks off");
        assert_eq!(sky_of(lit.get([0, 0, 0])), 0, "no sky in it still");
        assert!(
            drawn.contains(&[0, -1, 0]) && drawn.contains(&[-1, -1, -1]),
            "its chunk and those it is beside: {drawn:?}"
        );
        // Worked out again whole, it is the same as flooded again round the torch.
        assert_eq!(LightMap::of(&map, &faces()), lit);
        // And dark again once it is taken out.
        map.set([0, -3, 0], STONE);
        lit.relight(&map, &faces(), &[[0, -3, 0]]);
        assert_eq!(lit, light);
    }

    #[test]
    fn leaves_shade_the_ground_under_them_and_glass_does_not() {
        let mut map = floor();
        for x in -3..=3 {
            for z in -3..=3 {
                map.set([x, 9, z], LEAVES);
                map.set([x + 12, 9, z], GLASS);
            }
        }
        let light = LightMap::of(&map, &faces());
        assert_eq!(sky_of(light.get([0, 10, 0])), 15, "over the crown");
        assert_eq!(sky_of(light.get([0, 9, 0])), 14, "in it, a step less than over it");
        assert!(sky_of(light.get([0, 4, 0])) < 15, "the ground under it is in its shade");
        assert!(sky_of(light.get([0, 4, 0])) >= 11, "but lit from round it");
        assert_eq!(sky_of(light.get([12, 4, 0])), 15, "glass lets the sky straight through");
    }

    #[test]
    fn a_hole_dug_into_a_cave_lets_the_sky_down_into_it_and_filled_again_shuts_it_out() {
        let mut map = floor();
        for x in -32..32 {
            for z in -32..32 {
                for y in -20..0 {
                    map.set([x, y, z], STONE);
                }
            }
        }
        for x in -4..4 {
            for z in -4..4 {
                for y in -16..-10 {
                    map.set([x, y, z], AIR);
                }
            }
        }
        let mut light = LightMap::of(&map, &faces());
        let before = light.clone();
        assert_eq!(sky_of(light.get([0, -16, 0])), 0);
        let shaft: Vec<[i32; 3]> = (-10..4).map(|y| [0, y, 0]).collect();
        for &at in &shaft {
            map.set(at, AIR);
        }
        light.relight(&map, &faces(), &shaft);
        assert_eq!(sky_of(light.get([0, -16, 0])), 15, "straight down the shaft");
        assert_eq!(sky_of(light.get([3, -16, 0])), 12, "and out across the cave's floor");
        assert_eq!(light, LightMap::of(&map, &faces()), "as worked out whole");
        map.set([0, 3, 0], STONE);
        light.relight(&map, &faces(), &[[0, 3, 0]]);
        assert_eq!(light, LightMap::of(&map, &faces()), "covered, as worked out whole");
        for &at in &shaft {
            map.set(at, STONE);
        }
        light.relight(&map, &faces(), &shaft);
        assert_eq!(light, before, "shut again");
    }
}
