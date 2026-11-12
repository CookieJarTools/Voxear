//! Heat for C#: `Heat` in Voxear.Core asks how hot it is at a point and
//! what makes it so, and sets off heat for a while; `Clock` reads and sets
//! the level's time of day. The engine answers, as only it knows the
//! level's climate, blocks and sources (see `voxear_engine::heat`).
//!
//! Heat Bodies' changes, and characters coming to feel otherwise, reach
//! scripts as [`HeatEventData`], after the frame.

use std::sync::OnceLock;

use voxear_ecs::World;

use crate::interop::with_world;

/// A Heat Body's change, or a character's feeling, as scripts are told of
/// it: the Obj, what it became (0 for nothing, or for a feeling), which
/// (0 a change, 1 a feeling) and its number (the change's place in the
/// list, or the feeling's). Mirrors HeatEventData in Voxear.Core.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeatEventData {
    pub obj: u64,
    pub became: u64,
    pub kind: i32,
    pub value: i32,
}

const _: () = assert!(size_of::<HeatEventData>() == 24, "HeatEventData in Voxear.Core is 24 bytes");

/// What answers heat for scripts: a place's temperature and what makes it
/// so, eight numbers; heat set off at a point, so hot and so wide, for so
/// many seconds; and the clock's value `what`, set first when given.
#[derive(Clone, Copy)]
pub struct HeatHooks {
    pub sample: fn(&mut World, [f32; 3]) -> [f32; 8],
    pub add: fn(&mut World, [f32; 3], f32, f32, f32),
    pub clock: fn(&mut World, i32, Option<f32>) -> f32,
}

static HOOKS: OnceLock<HeatHooks> = OnceLock::new();

/// Sets what answers `Heat` and `Clock` in scripts; the first set is kept.
pub fn set_heat_hooks(hooks: HeatHooks) {
    let _ = HOOKS.set(hooks);
}

/// `Heat.Sample`: the temperature at a point, and the seven numbers after it
/// into `out` when given; NaN while scripts are not running.
pub(crate) unsafe extern "C" fn heat_sample(at: *const [f32; 3], out: *mut [f32; 8]) -> f32 {
    let (Some(hooks), false) = (HOOKS.get(), at.is_null()) else { return f32::NAN };
    let at = unsafe { at.read_unaligned() };
    let Some(sample) = with_world(|world| Some((hooks.sample)(world, at))) else { return f32::NAN };
    if !out.is_null() {
        unsafe { out.write_unaligned(sample) };
    }
    sample[0]
}

/// `Heat.AddSource`: heat at a point, so hot and so wide, for so many
/// seconds; 1 when it was set off.
pub(crate) unsafe extern "C" fn heat_add(at: *const [f32; 3], celsius: f32, radius: f32, seconds: f32) -> i32 {
    let (Some(hooks), false) = (HOOKS.get(), at.is_null()) else { return 0 };
    if !(celsius.is_finite() && radius.is_finite() && seconds.is_finite()) {
        return 0;
    }
    let at = unsafe { at.read_unaligned() };
    i32::from(with_world(|world| Some((hooks.add)(world, at, celsius, radius, seconds))).is_some())
}

/// `Clock`: value `what` of the level's clock, set first when `set` is not
/// 0; NaN while scripts are not running.
pub(crate) unsafe extern "C" fn clock_value(what: i32, set: i32, value: f32) -> f32 {
    let Some(hooks) = HOOKS.get() else { return f32::NAN };
    let written = (set != 0 && value.is_finite()).then_some(value);
    with_world(|world| Some((hooks.clock)(world, what, written))).unwrap_or(f32::NAN)
}
