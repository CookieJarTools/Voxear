//! Crowd records for C#: `CrowdRecords` in Voxear.Core keeps the level's
//! characters kept as records in a save of the scene and puts them back.
//! The engine answers, as only it has the records (see
//! `voxear_engine::crowd_records`).

use std::sync::OnceLock;

use voxear_ecs::World;

use crate::interop::with_world;

/// What keeps and puts back the level's crowd records for scripts, as
/// [`StormState`] does storms (see `voxear_engine::crowd_records`).
pub type CrowdRecordsState = fn(&mut World, Option<&[u8]>) -> Option<Vec<u8>>;

static RECORDS: OnceLock<CrowdRecordsState> = OnceLock::new();

/// Sets what answers `CrowdRecords.Capture` and `CrowdRecords.Apply` in
/// scripts; the first set is kept.
pub fn set_crowd_records(state: CrowdRecordsState) {
    let _ = RECORDS.set(state);
}

/// `CrowdRecords.Capture` (`what` 0): the records as bytes into `out` when
/// they fit, their length. `CrowdRecords.Apply` (`what` 1): the records in
/// `data` put back, 0, or -1 where they are no records' bytes. -1 while
/// scripts are not running.
pub(crate) unsafe extern "C" fn crowd_records(
    what: i32,
    data: *const u8,
    len: i32,
    out: *mut u8,
    capacity: i32,
) -> i32 {
    let Some(state) = RECORDS.get() else { return -1 };
    if what == 1 {
        if data.is_null() || len < 0 {
            return -1;
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) };
        return with_world(|world| state(world, Some(bytes))).map_or(-1, |_| 0);
    }
    let Some(bytes) = with_world(|world| state(world, None)) else { return -1 };
    if !out.is_null() && capacity >= bytes.len() as i32 {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    }
    bytes.len() as i32
}

/// A figure a script asks to be drawn far off, as `FarFigure` in
/// Voxear.Core lays it out: its prefab's engine id (-1 for none), its own
/// number, where it is in this world, which way it faces about up, and how
/// fast it walks, 0 standing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FarFigureData {
    pub prefab: i32,
    pub id: u32,
    pub position: [f32; 3],
    pub yaw: f32,
    pub speed: f32,
}

const _: () = assert!(size_of::<FarFigureData>() == 28, "FarFigureData in Voxear.Core is 28 bytes");

/// What puts the figures scripts set in place of those set before (see
/// `voxear_engine::crowd_records`).
pub type FarFiguresSet = fn(&mut World, &[FarFigureData]);

static FIGURES: OnceLock<FarFiguresSet> = OnceLock::new();

/// Sets what answers `FarFigures.Set`; the first set is kept.
pub fn set_far_figures(set: FarFiguresSet) {
    let _ = FIGURES.set(set);
}

/// `FarFigures.Set`: the figures in `data` in place of those set before, none
/// for `count` 0. 0, or -1 while scripts are not running.
pub(crate) unsafe extern "C" fn far_figures(data: *const FarFigureData, count: i32) -> i32 {
    let Some(set) = FIGURES.get() else { return -1 };
    let figures = match data.is_null() || count <= 0 {
        true => &[][..],
        false => unsafe { std::slice::from_raw_parts(data, count as usize) },
    };
    with_world(|world| {
        set(world, figures);
        Some(())
    })
    .map_or(-1, |_| 0)
}
