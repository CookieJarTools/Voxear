//! A dungeon built into one chunk column, or an area of them: its spaces
//! and ways carved, built stone laid into them, its entrance, then worn
//! and lit. Each column builds only its own blocks, from the layout kept
//! for the site, and every step reads only the blocks round each one, so
//! a column built alone builds exactly what it builds among its neighbours.

use super::plan::{DungeonLayout, LightKind, WayKind};
use super::{Dungeon, DungeonBlocks, carve, entrance, noise, rooms};
use crate::blocks::chunk::{AIR, BlockId};
use crate::blocks::structure::Ground;

/// A block not set: the ground's own is left there.
pub(crate) const LEAVE: u32 = u32::MAX;

/// What a block set in a [`Grid`] is, for the passes that wear it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Tag {
    /// Left as the ground has it.
    None = 0,
    /// Carved out.
    Air,
    /// Built stone, which may be broken and worn.
    Built,
    /// What is never broken: floors on the way, stair treads, lights.
    Kept,
}

/// The blocks a dungeon sets over some columns and a block round them,
/// from its lowest to its highest.
pub(crate) struct Grid {
    pub(crate) low: [i32; 3],
    pub(crate) size: [i32; 3],
    cells: Vec<u32>,
    tags: Vec<u8>,
}

impl Grid {
    fn new(low: [i32; 3], high: [i32; 3]) -> Self {
        let size = [high[0] - low[0] + 1, high[1] - low[1] + 1, high[2] - low[2] + 1];
        let count = (size[0] * size[1] * size[2]).max(0) as usize;
        Self { low, size, cells: vec![LEAVE; count], tags: vec![0; count] }
    }

    fn index(&self, [x, y, z]: [i32; 3]) -> Option<usize> {
        let (dx, dy, dz) = (x - self.low[0], y - self.low[1], z - self.low[2]);
        (dx >= 0 && dy >= 0 && dz >= 0 && dx < self.size[0] && dy < self.size[1] && dz < self.size[2])
            .then(|| ((dy * self.size[2] + dz) * self.size[0] + dx) as usize)
    }

    /// The columns it holds, from and to along x and z.
    pub(crate) fn columns(&self) -> ([i32; 2], [i32; 2]) {
        ([self.low[0], self.low[2]], [self.low[0] + self.size[0] - 1, self.low[2] + self.size[2] - 1])
    }

    pub(crate) fn top(&self) -> i32 {
        self.low[1] + self.size[1] - 1
    }

    pub(crate) fn set(&mut self, at: [i32; 3], block: BlockId, tag: Tag) {
        if let Some(i) = self.index(at) {
            self.cells[i] = u32::from(block);
            self.tags[i] = tag as u8;
        }
    }

    /// Sets `block` where nothing is kept.
    pub(crate) fn build(&mut self, at: [i32; 3], block: BlockId) {
        if block == AIR {
            return;
        }
        if let Some(i) = self.index(at)
            && self.tags[i] != Tag::Kept as u8
        {
            self.cells[i] = u32::from(block);
            self.tags[i] = Tag::Built as u8;
        }
    }

    /// Sets `block` as kept: never broken or worn.
    pub(crate) fn keep(&mut self, at: [i32; 3], block: BlockId) {
        if block == AIR {
            return;
        }
        self.set(at, block, Tag::Kept);
    }

    pub(crate) fn get(&self, at: [i32; 3]) -> u32 {
        self.index(at).map_or(LEAVE, |i| self.cells[i])
    }

    pub(crate) fn tag(&self, at: [i32; 3]) -> Tag {
        match self.index(at).map_or(0, |i| self.tags[i]) {
            1 => Tag::Air,
            2 => Tag::Built,
            3 => Tag::Kept,
            _ => Tag::None,
        }
    }

    /// Whether block `at` is carved open: air the dungeon made.
    pub(crate) fn open(&self, at: [i32; 3]) -> bool {
        self.get(at) == u32::from(AIR)
    }

    /// Whether block `at` is solid, as far as the grid knows: built, or the
    /// ground's own.
    pub(crate) fn solid(&self, at: [i32; 3]) -> bool {
        let block = self.get(at);
        block != u32::from(AIR)
    }
}

/// A dungeon's blocks by what they are for, and what it is building them
/// into.
pub(crate) struct Kit<'a> {
    pub(crate) blocks: DungeonBlocks,
    pub(crate) dungeon: &'a Dungeon,
    pub(crate) layout: &'a DungeonLayout,
    pub(crate) ground: &'a Ground<'a>,
    pub(crate) turn: &'a dyn Fn(BlockId, u8) -> BlockId,
}

/// Builds `layout` of `dungeon` into the columns from `first` to `last`
/// along x and z, block by block into `put`: carved, built, worn and lit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    dungeon: &Dungeon,
    layout: &DungeonLayout,
    ground: &Ground,
    under: BlockId,
    (first, last): ([i32; 2], [i32; 2]),
    turn: &dyn Fn(BlockId, u8) -> BlockId,
    put: &mut dyn FnMut([i32; 3], BlockId),
) {
    if last[0] < layout.low[0] || first[0] > layout.high[0] || last[1] < layout.low[2] || first[1] > layout.high[2] {
        return;
    }
    // A block round the columns, for what reads its neighbours.
    let low = [first[0].max(layout.low[0]) - 1, layout.low[1], first[1].max(layout.low[2]) - 1];
    let high = [last[0].min(layout.high[0]) + 1, layout.high[1], last[1].min(layout.high[2]) + 1];
    let mut grid = Grid::new(low, high);
    let kit = Kit { blocks: dungeon.blocks.filled(under), dungeon, layout, ground, turn };
    carve::carve(layout, &mut grid);
    rooms::build_spaces(&kit, &mut grid);
    build_ways(&kit, &mut grid);
    entrance::build(&kit, &mut grid);
    wear(&kit, &mut grid);
    light(&kit, &mut grid);
    let (lo, hi) = grid.columns();
    for y in grid.low[1]..=grid.top() {
        for z in lo[1] + 1..hi[1] {
            for x in lo[0] + 1..hi[0] {
                if x < first[0] || x > last[0] || z < first[1] || z > last[1] {
                    continue;
                }
                let block = grid.get([x, y, z]);
                if block != LEAVE {
                    put([x, y, z], block as BlockId);
                }
            }
        }
    }
}

/// The floor under each column of each way that is carved open over it,
/// slabs on a stair's half steps; a border of trim at its edges; and
/// across it every ten blocks a rib of trim round its walls and roof. A
/// grand stair has a kerb at each side, and pillars and pointed arches
/// over it.
fn build_ways(kit: &Kit, grid: &mut Grid) {
    let b = &kit.blocks;
    let (lo, hi) = grid.columns();
    for way in &kit.layout.ways {
        if way.high[0] + 3 < lo[0] || way.low[0] - 3 > hi[0] || way.high[1] + 3 < lo[1] || way.low[1] - 3 > hi[1] {
            continue;
        }
        let r = way.width as f32 * 0.5;
        let grand = way.kind == WayKind::Grand;
        for cell in &way.cells {
            let [x, z] = cell.at;
            if x < lo[0] || x > hi[0] || z < lo[1] || z > hi[1] {
                continue;
            }
            let s = cell.surface;
            let from = s.div_euclid(2);
            if !grid.open([x, from, z]) && !grid.open([x, from + 1, z]) {
                continue;
            }
            // Never into another space's or way's floor that is there already, lower than this one.
            let edge = cell.off > r - 1.0;
            let floor_block = if edge && cell.off <= r { b.trim } else { b.floor };
            if s % 2 == 0 {
                grid.keep([x, from - 1, z], floor_block);
            } else {
                grid.keep([x, from - 1, z], floor_block);
                let half = if b.slab != AIR { b.slab } else { floor_block };
                grid.keep([x, from, z], half);
            }
            let top = (from..=from + way.height + 3)
                .take_while(|&y| grid.open([x, y, z]) || (y == from && s % 2 == 1))
                .last()
                .unwrap_or(from);
            // The kerbs of a grand stair.
            if grand && cell.off > r - 1.0 && cell.off <= r + 0.5 {
                let y = if s % 2 == 0 { from } else { from + 1 };
                grid.build([x, y, z], b.stone);
            }
            // Ribs across it every ten blocks.
            let spacing = if grand { 10.0 } else { 12.0 };
            let rib = (cell.along + 2.0).rem_euclid(spacing) < 1.0 && cell.along > 3.0 && cell.along < way.length - 3.0;
            if rib && way.kind != WayKind::Shortcut {
                let wall = cell.off > r - 0.6;
                for y in from + 1..=top {
                    let roof = y == top;
                    if wall || roof {
                        grid.build([x, y, z], b.trim);
                    }
                }
                // A grand stair's pillars rise from its kerbs into pointed arches.
                if grand && wall {
                    grid.build([x, from, z], b.trim);
                }
            }
        }
    }
}

/// Wears the built stone: broken away at its exposed edges, by chance, with
/// rubble under where it fell; cracked where most exposed; mossy and hung
/// with vines near the way in, thinning with depth.
fn wear(kit: &Kit, grid: &mut Grid) {
    let passes = kit.dungeon.passes;
    let b = &kit.blocks;
    let seed = kit.layout.entrance.at[0] as u32 ^ (kit.layout.entrance.at[2] as u32).rotate_left(16);
    let surface = kit.layout.entrance.at[1];
    let (lo, hi) = grid.columns();
    let around = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
    // Read before anything changes, so each block's wear is that of the blocks round it as built.
    let mut changes: Vec<([i32; 3], BlockId, Tag)> = Vec::new();
    for y in grid.low[1]..=grid.top() {
        for z in lo[1] + 1..hi[1] {
            for x in lo[0] + 1..hi[0] {
                let at = [x, y, z];
                let tag = grid.tag(at);
                if tag == Tag::Air {
                    // Vines hang from the roof near the way in.
                    let depth = (surface - y).max(0) as f32;
                    let near = (1.0 - depth / 24.0).max(0.0);
                    if b.vine != AIR
                        && grid.solid([x, y + 1, z])
                        && noise::unit(x, y, z, seed ^ 0x7111) < passes.overgrowth * near * near * 0.2
                    {
                        let length = 1 + (noise::hash(x, y, z, seed) % 5) as i32;
                        for down in 0..length {
                            if !grid.open([x, y - down, z]) || !grid.open([x, y - down - 1, z]) {
                                break;
                            }
                            changes.push(([x, y - down, z], b.vine, Tag::Kept));
                        }
                    }
                    continue;
                }
                if tag != Tag::Built {
                    continue;
                }
                let block = grid.get(at) as BlockId;
                if block != b.stone && block != b.trim {
                    continue;
                }
                let open = around.iter().filter(|d| grid.open([x + d[0], y + d[1], z + d[2]])).count();
                let roll = noise::unit(x, y, z, seed);
                if open >= 2 && roll < passes.ruin * 0.14 {
                    changes.push((at, AIR, Tag::Air));
                    // Rubble on the floor under it, in its own column.
                    let mut down = y - 1;
                    while down > grid.low[1] && grid.open([x, down, z]) {
                        down -= 1;
                    }
                    if down < y - 1 {
                        changes.push(([x, down + 1, z], b.rubble, Tag::Built));
                    }
                    continue;
                }
                let depth = (surface - y).max(0) as f32;
                let near = (1.0 - depth / 40.0).max(0.15);
                if open >= 1 && roll > 1.0 - passes.wear * near * 0.5 {
                    changes.push((at, b.mossy, Tag::Built));
                } else if open >= 2 && roll > 1.0 - passes.wear * 0.45 {
                    changes.push((at, b.cracked, Tag::Built));
                }
            }
        }
    }
    for (at, block, tag) in changes {
        grid.set(at, block, tag);
    }
}

/// Puts each light that stands in the columns: a brazier's fire on an iron
/// stand, a cluster of crystal.
fn light(kit: &Kit, grid: &mut Grid) {
    let b = &kit.blocks;
    for light in &kit.layout.lights {
        let [x, y, z] = light.at;
        if !grid.open([x, y, z]) {
            continue;
        }
        match light.kind {
            LightKind::Brazier => {
                let stand = if b.metal != AIR { b.metal } else { b.trim };
                grid.keep([x, y, z], stand);
                if b.light != AIR {
                    grid.keep([x, y + 1, z], b.light);
                }
            }
            LightKind::Crystal => {
                let crystal = if b.cold_light != AIR { b.cold_light } else { continue };
                grid.keep([x, y, z], crystal);
                if noise::hash(x, y, z, 3).is_multiple_of(2) && grid.open([x, y + 1, z]) {
                    grid.keep([x, y + 1, z], crystal);
                }
            }
            LightKind::Glow => {}
        }
    }
}
