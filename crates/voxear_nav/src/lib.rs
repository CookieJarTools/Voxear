//! Navigation: the places something can walk to, and the ways to them.
//!
//! In a world of blocks ([`blocks`]) the places are made from the blocks
//! themselves, chunk by chunk, as they stream in and as they change: where
//! something can stand, how much room it has over its head, what it stands
//! on. A step from one place to the next is worked out as a way is looked
//! for, by the rules of what is walking ([`Walker`]: how tall it is, how high
//! it steps, how far it drops), so one set of places serves everything that
//! walks. Places are gathered into regions, a few to a chunk, and a coarse
//! search over the regions keeps a long way about as cheap as a short one.
//!
//! The work is done off the frame: places are made on threads of their own
//! ([`threads`]) and handed over whole, so a search always sees one world
//! and never half of an edit.

pub mod avoid;
pub mod blocks;
pub mod steer;
pub mod threads;

pub use blocks::{
    BlockNav, BlockNavBuilder, Place, PlaceKind, PlaceRef,
    search::{FoundPath, PathAsk, PathStatus},
    walking::Walker,
};

/// How many kinds of ground (areas) there can be: a place is one of them,
/// and what walks may cost each its own.
pub const AREAS: usize = 32;

/// The area a place is when nothing says otherwise: plain ground.
pub const WALKABLE: u8 = 0;
/// The area of ground sought out: a road, a path through a town.
pub const ROAD: u8 = 1;
/// The area of a place in water shallow enough to wade.
pub const WATER: u8 = 2;
/// The area of a place on the top of water too deep to stand in: swum.
pub const SWIM: u8 = 3;
/// The area of a place in a shut door, which is opened to go through.
pub const DOOR: u8 = 4;
/// The area of ground nothing walks on.
pub const NOT_WALKABLE: u8 = 5;
/// The first of a project's own areas, four of them.
pub const CUSTOM: u8 = 6;

/// A link a level's maker puts between two places: a ladder, a gap to jump,
/// a hole to drop down, a door that takes you elsewhere. A search may take
/// it as one step, from the place to stand nearest `from` to the one
/// nearest `to` (and back where `both_ways`), costing as far as it goes and
/// `cost` more. `kind` says how what walks it crosses it. In blocks from
/// the world's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub both_ways: bool,
    pub cost: f32,
    pub kind: u8,
}

/// A box of a world given an area, as a Nav Area component marks a road
/// through a town: a place whose feet are in it is of that area, whatever
/// it stands on. In blocks from the world's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub area: u8,
}

impl AreaBox {
    pub fn holds(&self, at: [f32; 3]) -> bool {
        (0..3).all(|axis| (self.min[axis]..=self.max[axis]).contains(&at[axis]))
    }
}

/// What each area costs to cross, a block of it at a time: 1 for plain
/// ground, more for what is avoided, less for what is sought out, as a road
/// is. An area costing nothing finite is not entered at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaCosts(pub [f32; AREAS]);

impl Default for AreaCosts {
    fn default() -> Self {
        let mut costs = [1.0; AREAS];
        costs[usize::from(WATER)] = 3.0;
        costs[usize::from(ROAD)] = 0.6;
        costs[usize::from(SWIM)] = 4.0;
        costs[usize::from(DOOR)] = 2.0;
        costs[usize::from(NOT_WALKABLE)] = f32::INFINITY;
        Self(costs)
    }
}

impl AreaCosts {
    /// The cost of a block of area `area`.
    pub fn of(&self, area: u8) -> f32 {
        self.0.get(usize::from(area)).copied().unwrap_or(1.0)
    }

    /// Whether area `area` may be entered at all.
    pub fn allows(&self, area: u8) -> bool {
        self.of(area).is_finite()
    }

    /// What a search guesses a block ahead costs: plain ground's, never
    /// more than the dearest of the ground it may cross. Ground cheaper than
    /// plain, as a road, is sought out where a search comes near it, but is
    /// not guessed to lie ahead, or every search would look at far more
    /// places for the sake of roads that are mostly not there.
    pub fn least(&self) -> f32 {
        self.of(crate::WALKABLE).clamp(0.05, 1.0)
    }
}
