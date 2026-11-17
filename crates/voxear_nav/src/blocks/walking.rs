//! What each kind of block is to something walking, and the rules a walker
//! steps by.
//!
//! A world's [`WalkTable`] says of each of its blocks, by number, whether
//! it is ground (nothing walks through it), whether it can be stood on (a
//! fence or a wall cannot), whether it is water, and how high its top is,
//! so places are made from a chunk's blocks without asking of each what
//! shape it is.

use voxear_terrain::blocks::{
    BlockFaces, PART_SOLID, PART_TALL,
    block::BlockShape,
    chunk::{AIR, BlockId},
    is_ground, is_part, is_water, part_of,
    shape::{Around, BlockState, part_boxes},
};

use crate::{WALKABLE, WATER};

/// Nothing walks through it.
pub const GROUND: u8 = 1;
/// It can be stood on: ground, but neither a fence nor a wall.
pub const STANDS: u8 = 2;
/// It is water.
pub const WET: u8 = 4;
/// It can be climbed: a vine or a rope.
pub const CLIMB: u8 = 8;
/// It is a shut door, which something that opens doors walks through.
pub const DOOR: u8 = 16;

/// What a kind of block is to something walking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walking {
    /// [`GROUND`], [`STANDS`], [`WET`], [`CLIMB`] and [`DOOR`], a bit each.
    pub bits: u8,
    /// How high its top is over its bottom, in sixteenths of a block, to
    /// stand on: 16 for a whole block, 8 for a slab lying low.
    pub floor: u8,
    /// The area of a place standing on it.
    pub area: u8,
}

impl Walking {
    /// A block nobody knows: ground, as the world's own rules take it.
    const UNKNOWN: Self = Self { bits: GROUND | STANDS, floor: 16, area: WALKABLE };

    pub fn is_ground(self) -> bool {
        self.bits & GROUND != 0
    }

    pub fn stands(self) -> bool {
        self.bits & STANDS != 0
    }

    pub fn is_wet(self) -> bool {
        self.bits & WET != 0
    }

    pub fn is_door(self) -> bool {
        self.bits & DOOR != 0
    }
}

/// What each of a world's blocks is to something walking, by its number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalkTable(Box<[Walking]>);

impl WalkTable {
    /// The table for a world whose blocks are drawn as `faces` say.
    pub fn of(faces: &[BlockFaces]) -> Self {
        let count = faces.len().max(1);
        Self((0..count).map(|id| walking(faces, id as BlockId)).collect())
    }

    /// What block `id` is.
    #[inline]
    pub fn get(&self, id: BlockId) -> Walking {
        self.0.get(usize::from(id)).copied().unwrap_or(Walking::UNKNOWN)
    }

    /// Whether `newer`, a table made since, says the same of every block
    /// this one knows, so only chunks holding the blocks it adds can have
    /// changed: as when a world numbers a block for the first time.
    pub fn grown_into(&self, newer: &WalkTable) -> bool {
        newer.0.len() >= self.0.len() && newer.0[..self.0.len()] == self.0[..]
    }
}

fn walking(faces: &[BlockFaces], id: BlockId) -> Walking {
    if id == AIR {
        return Walking::default();
    }
    let Some(kind) = faces.get(usize::from(id)) else { return Walking::UNKNOWN };
    if is_water(faces, id) {
        return Walking { bits: WET, floor: 0, area: WATER };
    }
    let climbs = matches!(kind.shape, BlockShape::Vine | BlockShape::Rope);
    if !is_ground(faces, id) {
        return Walking { bits: u8::from(climbs) * CLIMB, floor: 0, area: WALKABLE };
    }
    let part = part_of(faces, id);
    let door = u8::from(kind.shape == BlockShape::Door) * DOOR;
    // A fence or a wall is too high to stand on, and a door is no floor.
    let stands = u8::from(part & PART_TALL == 0 && door == 0) * STANDS;
    // The top of a block of parts is the highest of its boxes as it rests.
    let floor = match is_part(faces, id) && part & PART_SOLID != 0 {
        true => part_boxes(kind.shape, BlockState::from_byte(kind.state), Around::default(), kind.custom.as_ref())
            .iter()
            .map(|part| part.to[1].min(16))
            .max()
            .unwrap_or(16),
        false => 16,
    };
    Walking { bits: GROUND | stands | door, floor, area: WALKABLE }
}

/// How something walks a world of blocks, in sixteenths of a block: how
/// tall it is, how high it steps up, and how far it drops; and what more it
/// does: whether it swims across deep water, opens shut doors, and jumps
/// gaps (as many blocks across as `jump`, none for 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Walker {
    pub height: u16,
    pub step: u16,
    pub drop: u16,
    pub swims: bool,
    pub doors: bool,
    pub jump: u8,
}

impl Walker {
    /// A walker `tall` blocks high that steps up a block and drops three,
    /// as mobs walk worlds of blocks, and does nothing more.
    pub const fn blocks(tall: u16) -> Self {
        Self { height: tall * 16, step: 16, drop: 48, swims: false, doors: false, jump: 0 }
    }

    /// A walker `height` metres tall in a world of blocks `size` metres a
    /// side, stepping up `step` metres (a block at least) and dropping
    /// `drop`.
    pub fn of(height: f32, step: f32, drop: f32, size: f32) -> Self {
        let sixteenths = |metres: f32| (metres / size.max(1e-3) * 16.0).ceil().clamp(1.0, 4096.0) as u16;
        Self { height: sixteenths(height), step: sixteenths(step).max(16), drop: sixteenths(drop), ..Self::blocks(1) }
    }

    /// The same walker swimming, opening doors and jumping as `swims`,
    /// `doors` and `jump` say.
    pub const fn with(self, swims: bool, doors: bool, jump: u8) -> Self {
        Self { swims, doors, jump, ..self }
    }

    /// The walker whose regions it shares: as tall, stepping and dropping
    /// as far, doing everything any walker does (see [`WALKERS`]), so a way
    /// over regions never misses one it could take.
    pub const fn class(self) -> Self {
        self.with(true, true, MOST_JUMP)
    }

    /// How many whole blocks its drop and its step reach, at most: how far
    /// up or down a step from a place can land.
    pub fn reach(self) -> (i32, i32) {
        (i32::from(self.drop.div_ceil(16)) + 1, i32::from(self.step.div_ceil(16)) + 1)
    }
}

/// The widest gap a walker jumps, in blocks.
pub const MOST_JUMP: u8 = 2;

/// The walkers places are gathered into regions for, by default: two, one
/// and three blocks tall, each doing everything a walker does (see
/// [`Walker::class`]).
pub const WALKERS: [Walker; 3] = [Walker::blocks(2).class(), Walker::blocks(1).class(), Walker::blocks(3).class()];
