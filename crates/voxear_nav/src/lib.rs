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
/// The area of a place in water shallow enough to wade.
pub const WATER: u8 = 2;

/// What each area costs to cross, a block of it at a time: 1 for plain
/// ground, more for what is avoided, less for what is sought out, as a road
/// is. An area costing nothing finite is not entered at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaCosts(pub [f32; AREAS]);

impl Default for AreaCosts {
    fn default() -> Self {
        let mut costs = [1.0; AREAS];
        costs[usize::from(WATER)] = 3.0;
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

    /// The least any area that may be entered costs: how cheaply a block
    /// can be crossed at best, so a search never guesses a way dearer than
    /// it can be.
    pub fn least(&self) -> f32 {
        self.0.iter().copied().filter(|cost| cost.is_finite()).fold(f32::INFINITY, f32::min).clamp(0.05, 1.0)
    }
}
