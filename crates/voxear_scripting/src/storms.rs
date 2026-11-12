//! Storms for C#: `Storms`, `Storm` and `Sea` in Voxear.Core start storms
//! and whirlpools, change them, ask what the weather is at a point and hear
//! of storms coming and going. The engine answers, as only it has the
//! level's storms (see `voxear_engine::storms`).

use std::sync::OnceLock;

use voxear_ecs::World;

use crate::interop::with_world;

/// How many numbers a call is given and may write.
pub const STORM_NUMBERS: usize = 16;

/// What answers storms for scripts: what `what` asks, of the storm,
/// whirlpool or setting numbered `id`, with the numbers given and written.
pub type StormCall = fn(&mut World, i32, u32, &mut [f32; STORM_NUMBERS]) -> i32;

static CALL: OnceLock<StormCall> = OnceLock::new();

/// Sets what answers `Storms` and `Sea` in scripts; the first set is kept.
pub fn set_storm_call(call: StormCall) {
    let _ = CALL.set(call);
}

/// `Storms`, `Storm` and `Sea`: what `what` asks of `id`, with `values`
/// given and written; -1 while scripts are not running.
pub(crate) unsafe extern "C" fn storm_call(what: i32, id: u32, values: *mut [f32; STORM_NUMBERS]) -> i32 {
    let Some(call) = CALL.get() else { return -1 };
    let mut numbers = if values.is_null() { [0.0; STORM_NUMBERS] } else { unsafe { values.read_unaligned() } };
    let Some(answer) = with_world(|world| Some(call(world, what, id, &mut numbers))) else { return -1 };
    if !values.is_null() {
        unsafe { values.write_unaligned(numbers) };
    }
    answer
}

/// What keeps and puts back the storms for scripts: with no bytes, the
/// storms as bytes; with bytes, those storms put back, or none where they
/// are no storms' bytes.
pub type StormState = fn(&mut World, Option<&[u8]>) -> Option<Vec<u8>>;

static STATE: OnceLock<StormState> = OnceLock::new();

/// Sets what answers `Storms.CaptureState` and `Storms.ApplyState` in
/// scripts; the first set is kept.
pub fn set_storm_state(state: StormState) {
    let _ = STATE.set(state);
}

/// `Storms.CaptureState` (`what` 0): the storms as bytes into `out` when
/// they fit, their length. `Storms.ApplyState` (`what` 1): the storms in
/// `data` put back, 0, or -1 where they are no storms' bytes. -1 while
/// scripts are not running.
pub(crate) unsafe extern "C" fn storm_state(what: i32, data: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32 {
    let Some(state) = STATE.get() else { return -1 };
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
