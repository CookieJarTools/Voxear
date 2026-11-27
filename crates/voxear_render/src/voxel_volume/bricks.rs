//! A Voxel Volume's voxels as the GPU walks them (`shaders/voxels.slang`):
//! three levels, as the small-voxels research found best
//! (`docs/research/small-voxels.md`).
//!
//! - A **sector** is 8×8×8 bricks, 64 voxels a side. Its 512 bits say which
//!   bricks it has, and its bricks are kept one after another in the order
//!   of its bits, so a brick's place is the sector's first plus the bits
//!   before its own. Sixteen-bit counts of the bits before each pair of
//!   words keep that quick.
//! - A **brick** is 8×8×8 voxels: 512 bits for which are solid, 64 for which
//!   2×2×2 groups have any, where its materials start, and its average
//!   colour, which it is drawn in from far off.
//! - A **material** is a byte a solid voxel, in the order of its brick's
//!   bits: a place in the volume's palette.
//!
//! Voxels are set into a [`Builder`] brick by brick, then packed once.

use std::collections::HashMap;

/// How many voxels a brick and a sector are a side.
pub const BRICK: i32 = 8;
pub const SECTOR: i32 = 64;

/// Words a sector and a brick take, as the shader reads them.
pub const SECTOR_WORDS: usize = 25;
pub const BRICK_WORDS: usize = 20;

/// A sector with no bricks keeps this where its first brick would be.
pub const EMPTY: u32 = u32::MAX;

const BRICK_CELLS: usize = (BRICK * BRICK * BRICK) as usize;

/// Voxels gathered brick by brick before they are packed: for each brick
/// with anything in it, a byte a voxel, 0 for none or a palette place and
/// one.
#[derive(Default)]
pub struct Builder {
    bricks: HashMap<[i32; 3], Box<[u8; BRICK_CELLS]>>,
}

impl Builder {
    /// Sets the voxel at `cell` to palette place `material - 1`, or clears
    /// it with 0.
    pub fn set(&mut self, cell: [i32; 3], material: u8) {
        let brick = cell.map(|a| a.div_euclid(BRICK));
        let local = cell.map(|a| a.rem_euclid(BRICK));
        let index = (local[0] + BRICK * (local[1] + BRICK * local[2])) as usize;
        if material == 0 {
            if let Some(cells) = self.bricks.get_mut(&brick) {
                cells[index] = 0;
            }
            return;
        }
        self.bricks.entry(brick).or_insert_with(|| Box::new([0; BRICK_CELLS]))[index] = material;
    }

    /// The voxel at `cell`: 0 for none.
    pub fn get(&self, cell: [i32; 3]) -> u8 {
        let brick = cell.map(|a| a.div_euclid(BRICK));
        let local = cell.map(|a| a.rem_euclid(BRICK));
        self.bricks.get(&brick).map_or(0, |cells| cells[(local[0] + BRICK * (local[1] + BRICK * local[2])) as usize])
    }

    /// Each voxel set, and its palette place and one.
    pub fn each(&self, mut f: impl FnMut([i32; 3], u8)) {
        for (brick, cells) in &self.bricks {
            for (index, &cell) in cells.iter().enumerate().filter(|(_, cell)| **cell != 0) {
                let index = index as i32;
                let local = [index % BRICK, (index / BRICK) % BRICK, index / (BRICK * BRICK)];
                f([0, 1, 2].map(|a| brick[a] * BRICK + local[a]), cell);
            }
        }
    }

    /// Packs the voxels within `size` (in voxels, from the corner at 0) for
    /// the GPU, with `palette`'s colours averaged into each brick's.
    pub fn pack(&self, size: [u32; 3], palette: &[[u8; 4]]) -> Packed {
        self.pack_with_grass(size, palette, None)
    }

    /// As [`Builder::pack`], marking a brick whose voxels are all of the
    /// palette places `grass` (a ground's blades and flowers) as grass, as
    /// the GPU marks one: it leans in the wind and takes light through it.
    pub fn pack_with_grass(
        &self,
        size: [u32; 3],
        palette: &[[u8; 4]],
        grass: Option<std::ops::RangeInclusive<u8>>,
    ) -> Packed {
        let sector_count = size.map(|a| (a.div_ceil(SECTOR as u32) as i32).max(1));
        let count = (sector_count[0] * sector_count[1] * sector_count[2]) as usize;
        let mut sectors = vec![0u32; count * SECTOR_WORDS];
        for sector in 0..count {
            sectors[sector * SECTOR_WORDS + 24] = EMPTY;
        }
        // The bricks of each sector in the order of their bits.
        let mut by_sector: Vec<Vec<(usize, &[i32; 3], &[u8; BRICK_CELLS])>> = vec![Vec::new(); count];
        let inside = |b: &[i32; 3]| (0..3).all(|a| b[a] >= 0 && b[a] * BRICK < size[a] as i32);
        for (brick, cells) in self.bricks.iter().filter(|(brick, _)| inside(brick)) {
            if cells.iter().all(|&c| c == 0) {
                continue;
            }
            let s = brick.map(|a| a / BRICK);
            let sector = ((s[1] * sector_count[2] + s[2]) * sector_count[0] + s[0]) as usize;
            let l = brick.map(|a| a % BRICK);
            by_sector[sector].push(((l[0] + BRICK * (l[1] + BRICK * l[2])) as usize, brick, cells));
        }
        let mut bricks = Vec::new();
        let mut materials: Vec<u8> = Vec::new();
        let mut solid = 0u64;
        for (sector, list) in by_sector.iter_mut().enumerate() {
            if list.is_empty() {
                continue;
            }
            list.sort_by_key(|(index, _, _)| *index);
            let at = sector * SECTOR_WORDS;
            for (index, _, _) in list.iter() {
                sectors[at + (index >> 5)] |= 1 << (index & 31);
            }
            let mut before = 0u32;
            for pair in 0..8 {
                let low = before;
                before += sectors[at + pair * 2].count_ones();
                let high = before;
                before += sectors[at + pair * 2 + 1].count_ones();
                sectors[at + 16 + pair] = low | (high << 16);
            }
            sectors[at + 24] = (bricks.len() / BRICK_WORDS) as u32;
            for (_, _, cells) in list.iter() {
                let mut words = [0u32; BRICK_WORDS];
                let mut sum = [0u32; 3];
                let mut counted = 0u32;
                let mut kinds = 0u32;
                let mut all_grass = grass.is_some();
                words[18] = materials.len() as u32;
                for (index, &cell) in cells.iter().enumerate() {
                    if cell == 0 {
                        continue;
                    }
                    words[index >> 5] |= 1 << (index & 31);
                    let (x, y, z) = (index % 8, (index / 8) % 8, index / 64);
                    let group = (x >> 1) + 4 * ((y >> 1) + 4 * (z >> 1));
                    words[16 + (group >> 5)] |= 1 << (group & 31);
                    materials.push(cell - 1);
                    all_grass &= grass.as_ref().is_some_and(|places| places.contains(&(cell - 1)));
                    let colour = palette.get(usize::from(cell - 1)).copied().unwrap_or([200, 200, 200, 255]);
                    for a in 0..3 {
                        sum[a] += u32::from(colour[a]);
                    }
                    kinds |= u32::from(255 - colour[3]);
                    counted += 1;
                }
                solid += u64::from(counted);
                let average = sum.map(|a| a / counted.max(1));
                words[19] = average[0] | (average[1] << 8) | (average[2] << 16);
                words[19] &= !(GLOWS | SEE_THROUGH);
                words[19] |= if kinds & 0xF0 != 0 { GLOWS } else { 0 };
                words[19] |= if kinds & 0x0F != 0 { SEE_THROUGH } else { 0 };
                words[19] |= if all_grass && counted > 0 { SWAY_GRASS } else { 0 };
                bricks.extend_from_slice(&words);
            }
        }
        while materials.len() % 4 != 0 {
            materials.push(0);
        }
        Packed { sector_count, sectors, bricks, materials, solid }
    }
}

/// Marks a packed brick as leaves (`VOXEL_LEAFY` in `shaders/voxels.slang`),
/// over its average colour, `seed` telling it from the others: its groups
/// are loosely filled as its voxels are, about three in five of those with
/// any leaves and all those mostly full, by hash. Far off, where the walk
/// stops at groups, leaves stay loose cubes, twice the size, each group in
/// the colour of a voxel of its own, and not a solid skin.
pub fn mark_leafy(brick: &mut [u32], seed: u32) {
    let mut counts = [0u8; 64];
    for index in 0..BRICK_CELLS {
        if brick[index >> 5] & (1 << (index & 31)) != 0 {
            let (x, y, z) = (index % 8, (index / 8) % 8, index / 64);
            counts[(x >> 1) + 4 * ((y >> 1) + 4 * (z >> 1))] += 1;
        }
    }
    brick[16] = 0;
    brick[17] = 0;
    for (group, &count) in counts.iter().enumerate() {
        if count >= 5 || (count > 0 && loosely_kept(seed, group as u32)) {
            brick[16 + (group >> 5)] |= 1 << (group & 31);
        }
    }
    brick[19] |= LEAFY;
}

/// Whether a group of leaves with only a few of its voxels is kept: three in
/// five, by hash (`looselyKept` in `shaders/voxel_world.slang`).
pub fn loosely_kept(seed: u32, group: u32) -> bool {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ group.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    (h >> 8) % 5 < 3
}

/// The mark of a brick of leaves, over its average colour.
pub const LEAFY: u32 = 1 << 24;
/// The marks of a brick with a voxel that glows, and with one that is
/// see-through (`VOXEL_GLOWS` and `VOXEL_SEE_THROUGH` in
/// `shaders/voxels.slang`): the lowest bits of its average red and green.
pub const GLOWS: u32 = 1;
pub const SEE_THROUGH: u32 = 1 << 8;
/// The mark, over its average colour and the mark of leaves, of a brick of
/// leaves that sways whole in the wind (`VOXEL_SWAY_SHIFT` in
/// `shaders/voxels.slang`). A brick with ground or wood in it stays still;
/// one of grass and flowers leans from its foot, as the GPU marks it.
pub const SWAY_LEAVES: u32 = 2 << 25;
/// The mark of a brick of grass and flowers alone, which leans from its foot.
pub const SWAY_GRASS: u32 = 1 << 25;

/// A volume's voxels packed for the GPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Packed {
    pub sector_count: [i32; 3],
    pub sectors: Vec<u32>,
    pub bricks: Vec<u32>,
    pub materials: Vec<u8>,
    pub solid: u64,
}

impl Packed {
    /// How many bricks it has.
    pub fn brick_count(&self) -> usize {
        self.bricks.len() / BRICK_WORDS
    }

    /// Its size on the GPU, in bytes.
    pub fn bytes(&self) -> usize {
        self.sectors.len() * 4 + self.bricks.len() * 4 + self.materials.len()
    }

    /// The material of the voxel at `cell` and one, or 0 for none: the walk
    /// the GPU does, for tests and picking.
    pub fn get(&self, cell: [i32; 3]) -> u8 {
        let [cx, cy, cz] = self.sector_count;
        if cell.iter().any(|&a| a < 0) || cell[0] >= cx * SECTOR || cell[1] >= cy * SECTOR || cell[2] >= cz * SECTOR {
            return 0;
        }
        let s = cell.map(|a| a / SECTOR);
        let at = (((s[1] * cz + s[2]) * cx + s[0]) as usize) * SECTOR_WORDS;
        if self.sectors[at + 24] == EMPTY {
            return 0;
        }
        let b = cell.map(|a| (a / BRICK) % BRICK);
        let bi = (b[0] + BRICK * (b[1] + BRICK * b[2])) as usize;
        let word = self.sectors[at + (bi >> 5)];
        if word >> (bi & 31) & 1 == 0 {
            return 0;
        }
        let pair = self.sectors[at + 16 + (bi >> 6)];
        let mut before = if (bi >> 5) & 1 == 1 { pair >> 16 } else { pair & 0xFFFF };
        before += (word & ((1u32 << (bi & 31)) - 1)).count_ones();
        let brick = (self.sectors[at + 24] + before) as usize * BRICK_WORDS;
        let l = cell.map(|a| a % BRICK);
        let vi = (l[0] + BRICK * (l[1] + BRICK * l[2])) as usize;
        let words = &self.bricks[brick..brick + 16];
        if words[vi >> 5] >> (vi & 31) & 1 == 0 {
            return 0;
        }
        let mut k = (words[vi >> 5] & ((1u32 << (vi & 31)) - 1)).count_ones();
        for w in &words[..vi >> 5] {
            k += w.count_ones();
        }
        self.materials[(self.bricks[brick + 18] + k) as usize] + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxels_set_are_found_again_after_packing_and_nothing_else_is() {
        let mut builder = Builder::default();
        let set = [[0, 0, 0], [7, 7, 7], [8, 0, 0], [63, 1, 2], [64, 65, 66], [100, 3, 5], [5, 120, 9]];
        for (i, &cell) in set.iter().enumerate() {
            builder.set(cell, i as u8 + 1);
        }
        builder.set([300, 0, 0], 9);
        let packed = builder.pack([128, 128, 128], &[[255, 0, 0, 255]; 8]);
        for (i, &cell) in set.iter().enumerate() {
            assert_eq!(packed.get(cell), i as u8 + 1, "{cell:?}");
        }
        assert_eq!(packed.get([1, 0, 0]), 0);
        assert_eq!(packed.get([300, 0, 0]), 0, "outside the size is left out");
        assert_eq!(packed.solid, set.len() as u64);
        assert_eq!(packed.sector_count, [2, 2, 2]);
    }

    #[test]
    fn a_cleared_brick_is_left_out_and_a_brick_takes_its_voxels_average_colour() {
        let mut builder = Builder::default();
        builder.set([1, 1, 1], 1);
        builder.set([1, 1, 1], 0);
        builder.set([9, 0, 0], 1);
        builder.set([10, 0, 0], 2);
        let packed = builder.pack([16, 16, 16], &[[200, 0, 0, 255], [0, 100, 0, 255]]);
        assert_eq!(packed.brick_count(), 1);
        assert_eq!(packed.bricks[19], 100 | (50 << 8));
    }

    #[test]
    fn a_brick_of_leaves_keeps_its_full_groups_and_about_three_in_five_of_its_sparse_ones() {
        let mut brick = vec![0u32; BRICK_WORDS];
        // The lower half full, the upper half a voxel in each group.
        for index in 0..BRICK_CELLS {
            let (x, y, z) = (index % 8, (index / 8) % 8, index / 64);
            if y < 4 || (x % 2 == 0 && y % 2 == 0 && z % 2 == 0) {
                brick[index >> 5] |= 1 << (index & 31);
            }
        }
        brick[19] = 40 | (90 << 8);
        mark_leafy(&mut brick, 7);
        let kept = |group: usize| brick[16 + (group >> 5)] & (1 << (group & 31)) != 0;
        // A group's Y is the second of its places, four groups a side.
        let lower = |group: &usize| (group / 4) % 4 < 2;
        assert!((0..64).filter(lower).all(kept), "every full group");
        let sparse = (0..64).filter(|group| !lower(group) && kept(*group)).count();
        assert!((12..=26).contains(&sparse), "{sparse} of 32 sparse groups");
        assert_eq!(brick[19], 40 | (90 << 8) | LEAFY, "its average colour kept under the mark");
    }
}
