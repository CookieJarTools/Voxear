//! A chunk's places: where in it something can stand, kept column by
//! column, bottom up.
//!
//! A place is a block whose feet are in it: the block under it can be
//! stood on, nothing of ground is in it, and the block over it is not
//! water (a walker wades water a block deep and no deeper). It keeps how
//! many blocks over it are clear, its own first, so whether something fits
//! is a sum, whatever its height; and how high its floor is, so a slab
//! lying low is stood on half a block down.
//!
//! A chunk's places are made from its own blocks, the top of the chunk
//! under it (the floors of its lowest places) and the chunk over it (the
//! room over its highest).

use voxear_terrain::blocks::{
    BlockMap,
    chunk::{CHUNK, CHUNK_LEN, Chunk},
};

use super::walking::{WalkTable, Walking};

/// The most blocks of room a place keeps count of.
pub const ROOM_MOST: u8 = 31;

/// What a place is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaceKind {
    /// Standing on ground in the air.
    #[default]
    Stand = 0,
    /// Standing in water a block deep.
    Wade = 1,
}

/// A place to stand, in its chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    /// Its height in its chunk, 0 to 31: the block its feet are in.
    pub y: u8,
    /// How many blocks are clear over its floor, its own first, up to
    /// [`ROOM_MOST`].
    pub room: u8,
    /// The top of the block it stands on, in sixteenths of a block over
    /// that block's bottom: 16 for a whole one.
    pub floor: u8,
    /// Its area (see [`crate::AreaCosts`]).
    pub area: u8,
    pub kind: PlaceKind,
}

impl Place {
    /// How high its feet are, in sixteenths of a block from the world's
    /// corner, for a place of the chunk `cy` up.
    #[inline]
    pub fn feet(self, cy: i32) -> i32 {
        (cy * CHUNK as i32 + i32::from(self.y) - 1) * 16 + i32::from(self.floor)
    }

    /// How high the clear room over it ends, in sixteenths of a block.
    #[inline]
    pub fn top(self, cy: i32) -> i32 {
        (cy * CHUNK as i32 + i32::from(self.y) + i32::from(self.room)) * 16
    }
}

/// A chunk's places, column by column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavChunk {
    /// Where each column's places begin, the columns along x then z, and
    /// one more for where the last ends; empty for a chunk with none.
    starts: Box<[u16]>,
    places: Box<[Place]>,
}

/// How many columns a chunk has.
const COLUMNS: usize = (CHUNK * CHUNK) as usize;

impl NavChunk {
    /// Column `(x, z)`'s places, lowest first.
    #[inline]
    pub fn column(&self, x: u32, z: u32) -> &[Place] {
        match self.starts.is_empty() {
            true => &[],
            false => {
                let at = (z * CHUNK + x) as usize;
                &self.places[usize::from(self.starts[at])..usize::from(self.starts[at + 1])]
            }
        }
    }

    /// Where column `(x, z)`'s first place is among them all.
    #[inline]
    pub fn first_of(&self, x: u32, z: u32) -> u16 {
        self.starts.get((z * CHUNK + x) as usize).copied().unwrap_or(0)
    }

    /// Place `index` of them all.
    #[inline]
    pub fn place(&self, index: u16) -> Place {
        self.places[usize::from(index)]
    }

    /// Every place, column by column.
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// The column of place `index`, as `(x, z)`.
    pub fn column_of(&self, index: u16) -> (u32, u32) {
        let column = self.starts.partition_point(|&start| start <= index) - 1;
        (column as u32 % CHUNK, column as u32 / CHUNK)
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// How many bytes it keeps.
    pub fn bytes(&self) -> usize {
        self.starts.len() * 2 + self.places.len() * std::mem::size_of::<Place>()
    }
}

/// A chunk's blocks as what they are to something walking.
enum Cells {
    /// Every block the same.
    Same(Walking),
    /// Each block, along x, then z, then up.
    Each(Box<[Walking]>),
}

impl Cells {
    fn of(chunk: Option<&Chunk>, table: &WalkTable) -> Self {
        let Some(chunk) = chunk else { return Self::Same(Walking::default()) };
        let (palette, bits, words) = chunk.parts();
        let kinds: Vec<Walking> = palette.iter().map(|&id| table.get(id)).collect();
        if bits == 0 || kinds.iter().all(|kind| *kind == kinds[0]) {
            return Self::Same(kinds[0]);
        }
        let per_word = (64 / bits) as usize;
        let mask = (1u64 << bits) - 1;
        let mut cells = Vec::with_capacity(CHUNK_LEN);
        'words: for &word in words {
            for at in 0..per_word {
                if cells.len() == CHUNK_LEN {
                    break 'words;
                }
                let entry = ((word >> (at as u32 * bits)) & mask) as usize;
                cells.push(kinds.get(entry).copied().unwrap_or_default());
            }
        }
        cells.resize(CHUNK_LEN, Walking::default());
        Self::Each(cells.into_boxed_slice())
    }

    #[inline]
    fn at(&self, x: u32, y: u32, z: u32) -> Walking {
        match self {
            Self::Same(kind) => *kind,
            Self::Each(cells) => cells[((y * CHUNK + z) * CHUNK + x) as usize],
        }
    }

    /// Whether no block of it can be stood in: it is all ground.
    fn all_ground(&self) -> bool {
        matches!(self, Self::Same(kind) if kind.is_ground())
    }
}

/// Makes places for chunks one over another, sharing the blocks read for
/// one with the next.
pub struct ColumnBuilder<'a> {
    blocks: &'a BlockMap,
    table: &'a WalkTable,
    read: Vec<([i32; 3], Cells)>,
}

impl<'a> ColumnBuilder<'a> {
    pub fn new(blocks: &'a BlockMap, table: &'a WalkTable) -> Self {
        Self { blocks, table, read: Vec::new() }
    }

    /// The blocks of chunk `key`, read once.
    fn cells(&mut self, key: [i32; 3]) -> usize {
        if let Some(at) = self.read.iter().position(|(kept, _)| *kept == key) {
            return at;
        }
        let cells = Cells::of(self.blocks.chunk(key).map(|chunk| &**chunk), self.table);
        self.read.push((key, cells));
        self.read.len() - 1
    }

    /// The places of chunk `key`.
    pub fn build(&mut self, key: [i32; 3]) -> NavChunk {
        let [cx, cy, cz] = key;
        // Only the chunks either side of this one are kept read: those of the next one up are the same.
        self.read.retain(|([x, y, z], _)| [*x, *z] == [cx, cz] && (cy - 1..=cy + 1).contains(y));
        let own = self.cells(key);
        if self.read[own].1.all_ground() {
            return NavChunk::default();
        }
        let under = self.cells([cx, cy - 1, cz]);
        let over = self.cells([cx, cy + 1, cz]);
        let own = self.cells(key);
        let (under, own, over) = (&self.read[under].1, &self.read[own].1, &self.read[over].1);
        let side = CHUNK as usize;
        let mut starts = vec![0u16; COLUMNS + 1];
        let mut places = Vec::new();
        // A column's blocks: the top of the chunk under, its own, the chunk over.
        let mut column = [Walking::default(); 2 * CHUNK as usize + 1];
        let mut clear = [0u8; 2 * CHUNK as usize + 2];
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                column[0] = under.at(x, CHUNK - 1, z);
                for y in 0..CHUNK {
                    column[1 + y as usize] = own.at(x, y, z);
                    column[1 + side + y as usize] = over.at(x, y, z);
                }
                // How many blocks are clear from each up, counted down from the top.
                clear[column.len()] = 0;
                for at in (0..column.len()).rev() {
                    clear[at] = if column[at].is_ground() { 0 } else { clear[at + 1].saturating_add(1) };
                }
                for y in 0..side {
                    let (floor, feet, head) = (column[y], column[y + 1], column[y + 2]);
                    if !floor.stands() || feet.is_ground() || head.is_wet() {
                        continue;
                    }
                    let wades = feet.is_wet();
                    places.push(Place {
                        y: y as u8,
                        room: clear[y + 1].min(ROOM_MOST),
                        floor: floor.floor,
                        area: if wades { feet.area } else { floor.area },
                        kind: if wades { PlaceKind::Wade } else { PlaceKind::Stand },
                    });
                }
                starts[(z * CHUNK + x) as usize + 1] = places.len() as u16;
            }
        }
        if places.is_empty() {
            return NavChunk::default();
        }
        NavChunk { starts: starts.into_boxed_slice(), places: places.into_boxed_slice() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::tests::{FACES, STONE, WATER, floor};

    #[test]
    fn a_floor_has_a_place_over_each_block_with_the_room_over_it() {
        let mut blocks = floor();
        blocks.set([3, 4, 3], STONE);
        let table = WalkTable::of(&FACES);
        let chunk = ColumnBuilder::new(&blocks, &table).build([0, 0, 0]);
        let open = chunk.column(5, 5);
        assert_eq!(open.len(), 1);
        assert_eq!((open[0].y, open[0].room, open[0].floor), (1, ROOM_MOST, 16));
        assert_eq!(chunk.column(3, 3)[0].room, 3, "a block three over the floor");
        assert_eq!(chunk.column(3, 3)[1].y, 5, "and a place on top of it");
        assert!(chunk.column(20, 20).is_empty(), "no floor there");
        assert_eq!(open[0].feet(0), 16, "its feet on the floor's top");
        assert!(chunk.bytes() < 8 * 1024, "{} bytes", chunk.bytes());
    }

    #[test]
    fn water_a_block_deep_is_waded_and_deeper_water_is_no_place() {
        let mut blocks = floor();
        blocks.set([4, 1, 4], WATER);
        blocks.set([6, 0, 6], WATER);
        blocks.set([6, 1, 6], WATER);
        let table = WalkTable::of(&FACES);
        let chunk = ColumnBuilder::new(&blocks, &table).build([0, 0, 0]);
        assert_eq!(chunk.column(4, 4)[0].kind, PlaceKind::Wade);
        assert_eq!(chunk.column(4, 4)[0].area, crate::WATER);
        assert!(chunk.column(6, 6).is_empty(), "two blocks deep, stood in nowhere");
    }

    #[test]
    fn places_at_the_bottom_of_a_chunk_stand_on_the_chunk_under_it_and_count_room_into_the_one_over() {
        let mut blocks = BlockMap::default();
        for x in 0..4 {
            blocks.set([x, 31, 0], STONE);
        }
        blocks.set([1, 36, 0], STONE);
        let table = WalkTable::of(&FACES);
        let mut builder = ColumnBuilder::new(&blocks, &table);
        let over = builder.build([0, 1, 0]);
        assert_eq!(over.column(0, 0)[0].y, 0, "on the chunk under's top");
        assert_eq!(over.column(1, 0)[0].room, 4);
        assert!(builder.build([0, 0, 0]).column(0, 0).is_empty(), "its own top is ground");
    }
}
