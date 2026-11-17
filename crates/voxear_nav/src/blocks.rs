//! Navigation in a world of blocks: places made from the blocks chunk by
//! chunk ([`places`]), the steps between them worked out as they are
//! walked ([`step`]), regions of places and the coarse graph between them
//! ([`regions`]), ways found over them ([`search`]) and pulled tight
//! ([`tight`]), and a world's places kept up as its blocks change
//! ([`build`]).
//!
//! Places are counted in blocks from the world's corner, as its blocks
//! are: a world placed elsewhere, or a block more than a metre a side,
//! changes nothing here.

pub mod build;
pub mod moving;
pub mod places;
pub mod regions;
pub mod search;
pub mod tight;
pub mod walking;

use std::sync::Arc;

pub use build::BlockNavBuilder;
pub use places::{NavChunk, Place, PlaceKind};
use regions::{Edges, Regions};
use voxear_terrain::{blocks::chunk::CHUNK, coord_hash::CoordMap};
use walking::Walker;

/// No chunk.
pub const NONE: u32 = u32::MAX;

/// Where in the 3 × 3 × 3 chunks round one (itself in the middle) the
/// chunk `d` from it is.
#[inline]
pub fn around_index(d: [i32; 3]) -> usize {
    ((d[0] + 1) + 3 * (d[1] + 1) + 9 * (d[2] + 1)) as usize
}

/// The chunk at around index `index`, from the one in the middle.
pub fn around_step(index: usize) -> [i32; 3] {
    let index = index as i32;
    [index % 3 - 1, index / 3 % 3 - 1, index / 9 - 1]
}

/// A place of a world: the chunk it is in (by its number in the world's
/// [`BlockNav`]), which of the chunk's places it is, and its column there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaceRef {
    pub slot: u32,
    pub index: u16,
    pub x: u8,
    pub z: u8,
}

/// One chunk's navigation: its places, the chunks round it, and its
/// regions for each walker the world keeps them for.
#[derive(Clone, Debug)]
pub struct Slot {
    pub key: [i32; 3],
    pub chunk: Arc<NavChunk>,
    /// The chunks round it, by [`around_index`], as numbers of the world's;
    /// [`NONE`] where there is none.
    pub around: [u32; 27],
    /// Its regions, a set for each walker of [`BlockNav::walkers`].
    pub regions: Arc<[Regions]>,
    /// Its regions' edges to those of the chunks round it, a set for each
    /// walker.
    pub outer: Arc<[Edges]>,
    /// The version of the world its places last changed in.
    pub changed: u64,
}

/// A world of blocks' navigation, whole, as made from its blocks at one
/// moment: what searches read. It is never changed once made; a change to
/// the blocks makes another, sharing what did not change.
#[derive(Clone, Debug, Default)]
pub struct BlockNav {
    slots: Vec<Option<Slot>>,
    keys: CoordMap<[i32; 3], u32>,
    free: Vec<u32>,
    walkers: Arc<[Walker]>,
    version: u64,
}

impl BlockNav {
    /// Navigation with no places yet, keeping regions for `walkers`.
    pub fn new(walkers: &[Walker]) -> Self {
        Self { walkers: walkers.into(), ..Self::default() }
    }

    /// The walkers it keeps regions for.
    pub fn walkers(&self) -> &[Walker] {
        &self.walkers
    }

    /// Which of [`walkers`](Self::walkers) `walker` is, if it is one.
    pub fn class_of(&self, walker: Walker) -> Option<usize> {
        self.walkers.iter().position(|kept| *kept == walker)
    }

    /// How many times it has been made again since it began.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Chunk number `slot`.
    #[inline]
    pub fn slot(&self, slot: u32) -> Option<&Slot> {
        self.slots.get(slot as usize).and_then(Option::as_ref)
    }

    /// The number of chunk `key`, if it has navigation.
    pub fn slot_of(&self, key: [i32; 3]) -> Option<u32> {
        self.keys.get(&key).copied()
    }

    /// How many numbers chunks have been given, some perhaps let go: what
    /// a table by chunk number needs room for.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Every chunk with navigation.
    pub fn slots(&self) -> impl Iterator<Item = (u32, &Slot)> {
        self.slots.iter().enumerate().filter_map(|(at, slot)| slot.as_ref().map(|slot| (at as u32, slot)))
    }

    /// How many chunks have places, and how many places there are.
    pub fn counts(&self) -> (usize, usize) {
        self.slots().fold((0, 0), |(chunks, places), (_, slot)| {
            (chunks + usize::from(!slot.chunk.is_empty()), places + slot.chunk.len())
        })
    }

    /// How many bytes its places keep.
    pub fn bytes(&self) -> usize {
        self.slots().map(|(_, slot)| slot.chunk.bytes()).sum()
    }

    /// Place `at`, and the height of its chunk.
    #[inline]
    pub fn place(&self, at: PlaceRef) -> Option<(Place, i32)> {
        let slot = self.slot(at.slot)?;
        Some((slot.chunk.place(at.index), slot.key[1]))
    }

    /// The block a place's feet are in, counted from the world's corner.
    pub fn cell(&self, at: PlaceRef) -> Option<[i32; 3]> {
        let slot = self.slot(at.slot)?;
        let place = slot.chunk.place(at.index);
        let side = CHUNK as i32;
        Some([
            slot.key[0] * side + i32::from(at.x),
            slot.key[1] * side + i32::from(place.y),
            slot.key[2] * side + i32::from(at.z),
        ])
    }

    /// Where a place's feet stand: the middle of its block, on its floor,
    /// in blocks from the world's corner.
    pub fn point(&self, at: PlaceRef) -> Option<[f32; 3]> {
        let slot = self.slot(at.slot)?;
        let place = slot.chunk.place(at.index);
        let side = CHUNK as f32;
        Some([
            slot.key[0] as f32 * side + f32::from(at.x) + 0.5,
            place.feet(slot.key[1]) as f32 / 16.0,
            slot.key[2] as f32 * side + f32::from(at.z) + 0.5,
        ])
    }

    /// The region `at` is in for walker `class`.
    #[inline]
    pub fn region_of(&self, at: PlaceRef, class: usize) -> u16 {
        self.slot(at.slot)
            .and_then(|slot| slot.regions.get(class))
            .map_or(regions::NO_REGION, |regions| regions.of(at.index))
    }

    /// The places of column `(x, z)` (each 0 to 31) of the chunk `d` from
    /// chunk `slot`, with that chunk's number and height.
    #[inline]
    fn column_from(&self, slot: u32, d: [i32; 3], x: u32, z: u32) -> Option<Column<'_>> {
        let near = self.slot(slot)?.around[around_index(d)];
        let slot = self.slot(near)?;
        Some(Column { places: slot.chunk.column(x, z), first: slot.chunk.first_of(x, z), slot: near, cy: slot.key[1] })
    }

    /// The place to stand in the block column of `at` (in blocks from the
    /// world's corner) whose feet are nearest its height, no more than
    /// `reach` blocks up or down, where `walker` fits.
    pub fn standing_at(&self, at: [f32; 3], walker: Walker, reach: f32) -> Option<PlaceRef> {
        let side = CHUNK as i32;
        let cell = at.map(|v| v.floor() as i32);
        let (cx, cz) = (cell[0].div_euclid(side), cell[2].div_euclid(side));
        let (x, z) = (cell[0].rem_euclid(side) as u32, cell[2].rem_euclid(side) as u32);
        let feet = (at[1] * 16.0).round() as i32;
        let reach = (reach * 16.0).round() as i32;
        let (low, high) = ((feet - reach - 32).div_euclid(16 * side), (feet + reach + 32).div_euclid(16 * side));
        let mut best: Option<(i32, PlaceRef)> = None;
        for cy in low..=high {
            let Some(slot_at) = self.slot_of([cx, cy, cz]) else { continue };
            let chunk = &self.slot(slot_at).expect("numbered").chunk;
            let first = chunk.first_of(x, z);
            for (n, place) in chunk.column(x, z).iter().enumerate() {
                let off = (place.feet(cy) - feet).abs();
                let fits = place.top(cy) - place.feet(cy) >= i32::from(walker.height);
                if off <= reach && fits && best.is_none_or(|(kept, _)| off < kept) {
                    let index = first + n as u16;
                    best = Some((off, PlaceRef { slot: slot_at, index, x: x as u8, z: z as u8 }));
                }
            }
        }
        best.map(|(_, place)| place)
    }

    /// The place nearest `at` (in blocks from the world's corner) where
    /// `walker` fits: in its own column, or the nearest column round it,
    /// out to `across` blocks, no more than `reach` blocks up or down.
    pub fn nearest(&self, at: [f32; 3], walker: Walker, across: f32, reach: f32) -> Option<PlaceRef> {
        if let Some(place) = self.standing_at(at, walker, reach) {
            return Some(place);
        }
        let most = across.ceil().max(0.0) as i32;
        for ring in 1..=most {
            let mut best: Option<(f32, PlaceRef)> = None;
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let point = [at[0] + dx as f32, at[1], at[2] + dz as f32];
                    let Some(place) = self.standing_at(point, walker, reach) else { continue };
                    let far = (dx * dx + dz * dz) as f32;
                    if best.is_none_or(|(kept, _)| far < kept) {
                        best = Some((far, place));
                    }
                }
            }
            if let Some((_, place)) = best {
                return Some(place);
            }
        }
        None
    }

    /// Where a step from place `from` along `(dx, dz)` (each -1, 0 or 1)
    /// lands, by `walker`'s rules (see [`step`]).
    #[inline]
    pub fn step(&self, from: PlaceRef, dx: i32, dz: i32, walker: Walker) -> Option<PlaceRef> {
        step(self, from, dx, dz, walker).map(|(to, _)| to)
    }
}

/// A column's places, as a step looks at them.
pub(crate) struct Column<'a> {
    places: &'a [Place],
    first: u16,
    slot: u32,
    cy: i32,
}

/// What a step reads its places from: a whole world's navigation, or one
/// chunk alone.
pub(crate) trait Ground {
    fn column(&self, slot: u32, d: [i32; 3], x: u32, z: u32) -> Option<Column<'_>>;
    fn place(&self, at: PlaceRef) -> (Place, i32);
}

impl Ground for BlockNav {
    #[inline]
    fn column(&self, slot: u32, d: [i32; 3], x: u32, z: u32) -> Option<Column<'_>> {
        self.column_from(slot, d, x, z)
    }

    #[inline]
    fn place(&self, at: PlaceRef) -> (Place, i32) {
        BlockNav::place(self, at).unwrap_or_default()
    }
}

/// One chunk's places alone, numbered `slot`: a step out of it lands
/// nowhere.
pub(crate) struct OneChunk<'a> {
    pub chunk: &'a NavChunk,
    pub slot: u32,
    pub cy: i32,
}

impl Ground for OneChunk<'_> {
    #[inline]
    fn column(&self, slot: u32, d: [i32; 3], x: u32, z: u32) -> Option<Column<'_>> {
        (slot == self.slot && d == [0, 0, 0]).then(|| Column {
            places: self.chunk.column(x, z),
            first: self.chunk.first_of(x, z),
            slot,
            cy: self.cy,
        })
    }

    #[inline]
    fn place(&self, at: PlaceRef) -> (Place, i32) {
        (self.chunk.place(at.index), self.cy)
    }
}

/// Where a step from place `from` along `(dx, dz)` lands, by `walker`'s
/// rules, and the place there.
///
/// Along an axis: the highest place of the column beside whose feet are no
/// more than a step up and no more than a drop down, with room for the
/// walker over the higher of the two floors in both columns, so it neither
/// walks into an overhang nor drops through ground. Cornerwise: only on the
/// level, and only where both columns on the sides are on the level too,
/// so no corner of a wall or a ledge is cut.
pub(crate) fn step<G: Ground>(
    ground: &G,
    from: PlaceRef,
    dx: i32,
    dz: i32,
    walker: Walker,
) -> Option<(PlaceRef, Place)> {
    let (place, cy) = ground.place(from);
    if dx != 0 && dz != 0 {
        let feet = place.feet(cy);
        let level = |dx, dz| {
            straight_step(ground, from, place, cy, dx, dz, walker).filter(|(_, to, to_cy)| to.feet(*to_cy) == feet)
        };
        level(dx, 0)?;
        level(0, dz)?;
        return level(dx, dz).map(|(at, to, _)| (at, to));
    }
    straight_step(ground, from, place, cy, dx, dz, walker).map(|(at, to, _)| (at, to))
}

/// A step from `from`, standing on `place` in chunk height `cy`, to the
/// column `(dx, dz)` from it, by the rules for a step along an axis; for a
/// cornerwise one, only the column at the corner is looked at.
#[inline]
fn straight_step<G: Ground>(
    ground: &G,
    from: PlaceRef,
    place: Place,
    cy: i32,
    dx: i32,
    dz: i32,
    walker: Walker,
) -> Option<(PlaceRef, Place, i32)> {
    let side = CHUNK as i32;
    let (nx, nz) = (i32::from(from.x) + dx, i32::from(from.z) + dz);
    let (ox, oz) = (nx.div_euclid(side), nz.div_euclid(side));
    let (x, z) = (nx.rem_euclid(side) as u32, nz.rem_euclid(side) as u32);
    let (feet, top) = (place.feet(cy), place.top(cy));
    let (height, up, down) = (i32::from(walker.height), i32::from(walker.step), i32::from(walker.drop));
    // The chunks up and down the column beside that a landing can be in.
    let low = (feet - down).div_euclid(16).div_euclid(side) - cy;
    let high = ((feet + up).div_euclid(16) + 1).div_euclid(side) - cy;
    for oy in (low.max(-1)..=high.min(1)).rev() {
        let Some(column) = ground.column(from.slot, [ox, oy, oz], x, z) else { continue };
        for (n, to) in column.places.iter().enumerate().rev() {
            let to_feet = to.feet(column.cy);
            if to_feet > feet + up {
                continue;
            }
            if to_feet < feet - down {
                break;
            }
            let over = feet.max(to_feet) + height;
            if top >= over && to.top(column.cy) >= over {
                let at = PlaceRef { slot: column.slot, index: column.first + n as u16, x: x as u8, z: z as u8 };
                return Some((at, *to, column.cy));
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use voxear_terrain::blocks::{BlockFaces, BlockMap};

    pub const FACES: [BlockFaces; 3] = [BlockFaces::new(0, 0, 0), BlockFaces::new(0, 0, 0), BlockFaces::water(0)];
    pub const STONE: u16 = 1;
    pub const WATER: u16 = 2;

    /// A floor of stone, 16 blocks a side, its top at height 1.
    pub fn floor() -> BlockMap {
        let mut blocks = BlockMap::default();
        for x in 0..16 {
            for z in 0..16 {
                blocks.set([x, 0, z], STONE);
            }
        }
        blocks
    }
}
