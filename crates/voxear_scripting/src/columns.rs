//! Chunk columns for C#: `Blocks.ColumnMade` and `Blocks.ColumnLetGo` in
//! Voxear.Core hear each column of a world of blocks without end made or
//! let go, as [`ColumnEventData`] after the frame, and `Blocks.IsMade` asks
//! whether the ground round a place is made. The engine answers, as only it
//! streams the worlds (see `voxear_engine::ground_streaming`).

use std::sync::OnceLock;

use voxear_ecs::World;

use crate::interop::with_world;

/// A column made or let go, as scripts are told of it: the world of blocks'
/// Obj, the column's place in it, 32 blocks a side, 1 made or 0 let go, how
/// wide it is in metres, and its low corner along x and z in this world.
/// Mirrors ColumnEventData in Voxear.Core.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColumnEventData {
    pub world: u64,
    pub column: [i32; 2],
    pub made: i32,
    pub size: f32,
    pub corner: [f32; 2],
}

const _: () = assert!(size_of::<ColumnEventData>() == 32, "ColumnEventData in Voxear.Core is 32 bytes");

/// What answers `Blocks.IsMade`: whether every world of blocks without end in
/// the dimension asked of has made its blocks within a radius of a place.
pub type GroundMade = fn(&World, [f32; 3], f32) -> bool;

static MADE: OnceLock<GroundMade> = OnceLock::new();

/// Sets what answers `Blocks.IsMade`; the first set is kept.
pub fn set_ground_made(made: GroundMade) {
    let _ = MADE.set(made);
}

/// `Blocks.IsMade`: 1 where the ground within `radius` metres of `at` is made,
/// or no world is being made there; 0 where it is not; -1 while scripts are
/// not running.
pub(crate) unsafe extern "C" fn ground_made(at: *const [f32; 3], radius: f32) -> i32 {
    let Some(made) = MADE.get() else { return -1 };
    if at.is_null() {
        return -1;
    }
    let at = unsafe { *at };
    with_world(|world| Some(i32::from(made(world, at, radius)))).unwrap_or(-1)
}
